// SPDX-License-Identifier: GPL-2.0-or-later
//! Stored protocol inputs share their physical time coordinates with live inputs.
//! One worker and a bounded result cache serve bands, tables, export and Search.
use super::worker::Worker;
use super::*;
use oos_dsp::decode::Packet;
use std::sync::Arc;

const BACKGROUND_POINTS: usize = 262_144;
const MAX_POINTS: usize = 32_000_000;
const MAX_PACKETS: usize = 250_000;
pub(super) const COMPUTING: &str = "Decode computing";
/// What a decoder says of a record above 1 M points that is only read as min/max pairs (Run, or a STOP that could not
/// read the record raw): the pairs carry no bit timing.
pub(super) const DEEP_PAIRS: &str = "Deep record is read compressed: stop it, or use at most 1M points";

/// "Record too long for the decoder: N samples, limit M", for the record of `n` samples. The text of a count is made
/// once and kept (an error is a `&'static str`): a handful of records is all that is ever asked.
pub(crate) fn too_long(n: usize) -> &'static str {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};
    static SEEN: OnceLock<Mutex<HashMap<usize, &'static str>>> = OnceLock::new();
    let mut seen = SEEN.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    if seen.len() > 64 {
        seen.clear();
    }
    seen.entry(n).or_insert_with(|| {
        Box::leak(format!("Record too long for the decoder: {n} samples, limit {MAX_POINTS}").into_boxed_str())
    })
}
type Packets = Result<Arc<Vec<Packet>>, &'static str>;

struct Wave {
    source: usize,
    v: Vec<f32>,
    dt: f64,
    t0: f64,
    /// the points are minimum and maximum in turn (`channel_alternates`)
    pairs: bool,
}
struct Job {
    key: String,
    model: Model,
    waves: Vec<Wave>,
    hysteresis: f32,
}
struct Finished {
    key: String,
    packets: Packets,
}
fn open_worker() -> Worker<Job, Finished> {
    Worker::spawn("reference-decode", |job: Job| Finished { key: job.key.clone(), packets: job.run() })
        .expect("start bounded reference decode worker")
}

struct Entry {
    key: String,
    packets: Packets,
    hits: Option<(BusWhat, u64, Arc<Vec<f64>>)>,
}
#[derive(Default)]
pub(super) struct Cache {
    entries: Vec<Entry>,
    worker: Option<Worker<Job, Finished>>,
    scans: usize,
}
impl Cache {
    fn put(&mut self, key: String, packets: Packets) {
        if self.entries.len() >= DECODERS * 2 {
            self.entries.remove(0);
        }
        self.entries.push(Entry { key, packets, hits: None });
    }
}

/// The samples of a decoder source: borrowed from a record, or shared with the math cache.
pub(super) enum Samples<'a> {
    Slice(&'a [f32]),
    Shared(Arc<Vec<f32>>),
}

impl std::ops::Deref for Samples<'_> {
    type Target = [f32];
    fn deref(&self) -> &[f32] {
        match self {
            Samples::Slice(v) => v,
            Samples::Shared(v) => v,
        }
    }
}

/// The required analogue roles, including the implicit Parallel data bank.
pub(super) fn sources(m: &Model) -> Result<Vec<usize>, &'static str> {
    let mut ids = vec![m.dec_src];
    if m.dec_proto == Proto::PARALLEL {
        let (base, count) = if m.dec_src >= DEC_MATH_BASE {
            (DEC_MATH_BASE, NMATH)
        } else if m.dec_src >= DEC_REF_BASE {
            (DEC_REF_BASE, NREF)
        } else {
            (0, NCH)
        };
        ids = (0..count)
            .map(|i| base + (m.dec_src - base + i) % count)
            .filter(|id| Some(*id) != m.dec_cs)
            .take((m.dec_bits.clamp(1, NCH as u32) as usize).min(NCH - usize::from(m.dec_cs.is_some())))
            .collect();
    } else {
        let roles = m.dec_proto.roles();
        if roles[1].is_some() {
            ids.push(m.dec_src2);
        }
        if roles[3].is_some() {
            ids.extend(m.dec_src4);
        }
        if m.dec_proto.diff_ok() {
            ids.extend(m.dec_diff);
        }
    }
    if m.dec_proto.roles()[2].is_some() {
        ids.extend(m.dec_cs);
    }
    let mut unique = Vec::new();
    for id in ids {
        if !decode_valid(id) {
            return Err("Invalid decoder source");
        }
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    if unique.len() > NCH {
        return Err("Too many decoder roles");
    }
    Ok(unique)
}

/// Borrowed native or candidate inputs; alignment and protocol dispatch are shared.
pub(super) struct BorrowedWave<'a> {
    pub source: usize,
    pub v: &'a [f32],
    pub dt: f64,
    pub t0: f64,
    /// the points are minimum and maximum in turn (`channel_alternates`): read by the means of neighbours
    pub pairs: bool,
}

/// The acquisition's own samples of channel `c` behind `rec`, where it keeps them (not a frame of min/max pairs).
pub(super) fn native_of(rec: &Record, c: usize) -> Option<&crate::wave::Raw> {
    rec.raw.as_ref().filter(|raw| !raw.frame.envelope && raw.frame.ch[c].len() >= 2)
}

/// Whether the points a decoder reads of channel `c` of `rec` are minimum and maximum in turn: its own samples
/// (`native_of`) taken in Peak detect, else the points of the screen record (`Record::alternates`).
pub(super) fn channel_alternates(rec: &Record, c: usize) -> bool {
    if native_of(rec, c).is_some() {
        rec.peak
    } else {
        rec.alternates()
    }
}
pub(super) fn decode_borrowed(original: &Model, waves: &[BorrowedWave<'_>], hysteresis: f32) -> Packets {
    if waves.is_empty()
        || waves.len() > NCH
        || waves.iter().any(|w| w.v.len() < 2 || !w.dt.is_finite() || w.dt <= 0.0 || !w.t0.is_finite())
    {
        return Err("Invalid decoder record");
    }
    let start = waves.iter().map(|w| w.t0).fold(f64::NEG_INFINITY, f64::max);
    let stop = waves.iter().map(|w| w.t0 + (w.v.len() - 1) as f64 * w.dt).fold(f64::INFINITY, f64::min);
    let finest = waves.iter().min_by(|a, b| a.dt.total_cmp(&b.dt)).ok_or("Decoder source empty")?;
    let dt = finest.dt;
    let snap = |value: f64| if (value - value.round()).abs() <= 1e-7 { value.round() } else { value };
    let first = snap((start - finest.t0) / dt).ceil();
    let last = snap((stop - finest.t0) / dt).floor();
    if !first.is_finite() || !last.is_finite() || last - first < 1.0 {
        return Err("Decoder sources do not overlap");
    }
    let count = (last - first + 1.0) as usize;
    if count > MAX_POINTS {
        return Err(too_long(count));
    }
    let t0 = finest.t0 + first * dt;
    // points that are minimum and maximum in turn are read by the means of neighbours, on their own grid: aligned to
    // a finer one, neighbours are no longer minimum and maximum
    let own: Vec<std::borrow::Cow<[f32]>> =
        waves.iter().map(|w| if w.pairs { crate::trig::pair_means(w.v).into() } else { w.v.into() }).collect();
    let aligned: Vec<Option<Vec<f32>>> = waves
        .iter()
        .zip(&own)
        .map(|(w, v)| {
            if w.dt == dt && w.t0 == t0 && v.len() == count {
                return None;
            }
            Some(
                (0..count)
                    .map(|i| {
                        let index = snap((t0 + i as f64 * dt - w.t0) / w.dt);
                        if index < 0.0 || index > (v.len() - 1) as f64 {
                            return f32::NAN;
                        }
                        let k = index.floor() as usize;
                        if k + 1 == v.len() {
                            v[k]
                        } else {
                            v[k] + index.fract() as f32 * (v[k + 1] - v[k])
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    let ch: [&[f32]; NCH] = std::array::from_fn(|i| own.get(i).map_or(&[][..], |v| aligned[i].as_deref().unwrap_or(v)));
    let slot = |source| waves.iter().position(|w| w.source == source);
    let mut model = original.clone();
    model.dec_src = slot(model.dec_src).unwrap_or(0);
    model.dec_src2 = slot(model.dec_src2).unwrap_or(0);
    model.dec_cs = model.dec_cs.and_then(slot);
    model.dec_src4 = model.dec_src4.and_then(slot);
    model.dec_diff = model.dec_diff.and_then(slot);
    // Parallel's selected bank is already ordered in the four DSP slots.
    if model.dec_proto == Proto::PARALLEL {
        model.dec_src = 0;
    }
    let mut packets = crate::trig::decode_at(&model, ch, dt, model.dec_thr, hysteresis)?;
    crate::trig::share_findings(&model, original);
    if packets.len() > MAX_PACKETS {
        return Err("Decode exceeds 250000 packets; shorten the record");
    }
    for packet in &mut packets {
        packet.t0 = t0 + packet.t0 * dt;
        packet.t1 = t0 + packet.t1 * dt;
    }
    Ok(Arc::new(packets))
}
impl Job {
    fn run(&self) -> Packets {
        let waves = self
            .waves
            .iter()
            .map(|w| BorrowedWave { source: w.source, v: &w.v, dt: w.dt, t0: w.t0, pairs: w.pairs })
            .collect::<Vec<_>>();
        decode_borrowed(&self.model, &waves, self.hysteresis)
    }
}

impl Ui {
    pub(super) fn decoder_uses_refs(m: &Model) -> bool {
        !m.dec_on_la() && sources(m).is_ok_and(|ids| ids.iter().any(|&id| id >= DEC_REF_BASE))
    }
    /// Whether the live record holds the acquisition's own samples (`Record.raw`) of any channel a role of `m` reads.
    /// The screen record is peak-detect or resampled for drawing, which loses the timing of a protocol such as DCC
    /// (58 us halves); such a decoder runs on the reference path, which reads `raw` for every role (issue #225).
    pub(super) fn decoder_uses_raw(&self, m: &Model) -> bool {
        let Some(raw) = self.memory_raw() else { return false };
        !m.dec_on_la()
            && !raw.frame.envelope
            && sources(m)
                .is_ok_and(|ids| ids.iter().any(|&id| id < NCH && raw.frame.ch.get(id).is_some_and(|v| v.len() >= 2)))
    }
    fn decoder_samples(&self, id: usize) -> Result<(Samples<'_>, f64, f64, f32), &'static str> {
        self.decoder_samples_at(id, None)
    }
    fn decoder_samples_at<'a>(
        &'a self,
        id: usize,
        candidate: Option<&'a Record>,
    ) -> Result<(Samples<'a>, f64, f64, f32), &'static str> {
        if let Some(k) = decode_math(id) {
            // the result of a math channel: its own record, at its own step (a pending job is "computing")
            let c = self.m.math_ch(k);
            if matches!(c.op, MathOp::Off) {
                return Err("Math source is off");
            }
            if matches!(c.op, MathOp::Fft) {
                return Err("FFT is a frequency-domain source");
            }
            let sig = self.math_record(k)?.ok_or(COMPUTING)?;
            if sig.v.len() < 2 || !sig.dt.is_finite() || sig.dt <= 0.0 || !sig.t0.is_finite() {
                return Err("Invalid decoder record");
            }
            let scales = math_scales();
            let vpd = scales[c.scale.min(scales.len() - 1)] as f32;
            return Ok((Samples::Shared(Arc::clone(&sig.v)), sig.dt, sig.t0, vpd));
        }
        let (v, dt, t0, vpd) = match decode_wave(id).ok_or("Invalid decoder source")? {
            WaveId::Ref(i) => {
                let r = self.refs[i].as_ref().ok_or("Reference source empty")?;
                (r.v.as_slice(), r.dt, self.source_time_origin(WaveId::Ref(i), r.t0), r.vpd)
            }
            WaveId::Ch(c) => {
                let r = candidate.or(self.last[0].as_ref()).ok_or("Live decoder source has no record")?;
                // the live record's own samples: after a STOP the whole record the stop read (`memory_raw`), not the
                // view read again from it
                let own = if candidate.is_some() { native_of(r, c) } else { self.memory_raw().filter(|raw| !raw.frame.envelope && raw.frame.ch[c].len() >= 2) };
                if let Some(raw) = own {
                    if !raw.frame.dt.is_finite() || raw.frame.dt <= 0.0 || !raw.t0.is_finite() {
                        return Err("Invalid decoder record");
                    }
                    return Ok((
                        Samples::Slice(&raw.frame.ch[c]),
                        raw.frame.dt,
                        self.source_time_origin(WaveId::Ch(c), raw.t0 - raw.i0 as f64 * raw.frame.dt),
                        self.m.ch[c].volts_per_div(),
                    ));
                }
                (r.ch[c].as_slice(), r.dt, self.source_time_origin(WaveId::Ch(c), r.t0), self.m.ch[c].volts_per_div())
            }
        };
        let v = Samples::Slice(v);
        if v.len() < 2 || !dt.is_finite() || dt <= 0.0 || !t0.is_finite() {
            return Err("Invalid decoder record");
        }
        Ok((v, dt, t0, vpd))
    }
    fn decoder_key(&self, m: &Model) -> Result<String, &'static str> {
        self.decoder_key_at(m, None)
    }
    pub(super) fn decoder_key_at(&self, m: &Model, candidate: Option<&Record>) -> Result<String, &'static str> {
        let mut key = format!("{:?}|{}|", m.dec_cfg(), self.workflow_epoch);
        for id in sources(m)? {
            let (v, dt, t0, vpd) = self.decoder_samples_at(id, candidate)?;
            let revision = if id < NCH {
                (if candidate.is_some() { candidate.and_then(|rec| rec.raw.as_ref()) } else { self.memory_raw() })
                    .map_or(self.counts.wfm_total, |raw| raw.frame.seq)
            } else if self.session_is_sim && id == DEC_REF_BASE + self.m.sim_slot {
                self.replay.sim_attempt
            } else {
                0
            };
            key.push_str(&format!(
                "{id}:{:p}:{}:{}:{}:{}:{revision}|",
                v.as_ptr(),
                v.len(),
                dt.to_bits(),
                t0.to_bits(),
                vpd.to_bits()
            ));
        }
        Ok(key)
    }
    pub(super) fn reference_decode_packets(&self, m: &Model) -> Packets {
        self.reference_decode_packets_at(m, None)
    }
    /// For drawing: the packets for the record now, and while a running acquisition has outrun the decoder (a large
    /// record takes longer to decode than the next one takes to arrive) the last ones that were finished with the same
    /// settings, a record old, instead of "Decode computing" for ever. Remote answers and the event table keep the
    /// exact result (`decode_packets_of`).
    pub(super) fn decode_packets_shown(&self, m: &Model) -> Packets {
        match self.decode_packets_of(m) {
            Err(COMPUTING) if self.m.run == RunState::Run => {
                let prefix = format!("{:?}|{}|", m.dec_cfg(), self.workflow_epoch);
                let cache = self.azr.reference_decode.borrow();
                cache.entries.iter().rev().find(|e| e.key.starts_with(&prefix)).map_or(Err(COMPUTING), |e| e.packets.clone())
            }
            other => other,
        }
    }
    pub(super) fn decoder_candidate_key(&self, m: &Model, record: &Record) -> Result<String, &'static str> {
        Ok(format!("candidate:{}|{}", self.bus_candidate.epoch, self.decoder_key_at(m, Some(record))?))
    }
    pub(super) fn reference_decode_packets_at(&self, m: &Model, candidate: Option<&Record>) -> Packets {
        let key =
            if let Some(record) = candidate { self.decoder_candidate_key(m, record)? } else { self.decoder_key(m)? };
        let mut cache = self.azr.reference_decode.borrow_mut();
        let finished = cache.worker.as_mut().and_then(|worker| worker.poll().ok());
        if let Some(result) = finished {
            // The worker survives imports. A completed old source/settings job
            // must never repopulate the current workspace's accepted results.
            let current = (0..DECODERS)
                .any(|k| self.decoder_key(&self.m.dec_view(k)).ok().as_ref() == Some(&result.key))
                || self.bus_candidate.record.as_ref().is_some_and(|record| {
                    self.decoder_candidate_key(&self.m, record).ok().as_ref() == Some(&result.key)
                });
            if current {
                cache.put(result.key, result.packets);
            }
        }
        if let Some(entry) = cache.entries.iter().find(|entry| entry.key == key) {
            return entry.packets.clone();
        }
        if cache.worker.as_ref().is_some_and(Worker::busy) {
            return Err(COMPUTING);
        }
        let ids = sources(m)?;
        let left = ids
            .iter()
            .map(|&id| self.decoder_samples_at(id, candidate).map(|(_, _, t0, _)| t0))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max);
        let right = ids
            .iter()
            .map(|&id| self.decoder_samples_at(id, candidate).map(|(v, dt, t0, _)| t0 + (v.len() - 1) as f64 * dt))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .fold(f64::INFINITY, f64::min);
        let finest = ids
            .iter()
            .map(|&id| self.decoder_samples_at(id, candidate).map(|(_, dt, _, _)| dt))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .fold(f64::INFINITY, f64::min);
        if right <= left {
            return Err("Decoder sources do not overlap");
        }
        if (right - left) / finest + 1.0 > MAX_POINTS as f64 + 1e-7 {
            return Err("Decoder aligned record exceeds 32M samples");
        }
        let mut waves = Vec::new();
        for id in ids {
            let (v, dt, t0, _) = self.decoder_samples_at(id, candidate)?;
            // Only the physical intersection is copied, with one neighbour for
            // interpolation. A 500 MB CH record overlapping a short R record
            // must not create another full-size live-data allocation.
            let lo = (((left - t0) / dt).floor().max(0.0) as usize).min(v.len() - 1);
            let hi = ((((right - t0) / dt).ceil().max(0.0) as usize).saturating_add(1)).min(v.len());
            let pairs = id < NCH && candidate.or(self.last[0].as_ref()).is_some_and(|r| channel_alternates(r, id));
            waves.push(Wave { source: id, v: v[lo..hi].to_vec(), dt, t0: t0 + lo as f64 * dt, pairs });
        }
        let (_, _, _, vpd) = self.decoder_samples_at(m.dec_src, candidate)?;
        let large = waves.iter().any(|wave| wave.v.len() > BACKGROUND_POINTS);
        let job = Job { key: key.clone(), model: m.clone(), waves, hysteresis: 0.1 * vpd.max(1e-9) };
        cache.scans += 1;
        if large {
            cache.worker.get_or_insert_with(open_worker).submit(job).map_err(|_| "Decode worker unavailable")?;
            Err(COMPUTING)
        } else {
            let packets = job.run();
            cache.put(key, packets.clone());
            packets
        }
    }
    pub(super) fn seed_sim_bus_decode(&self, model: &Model, packets: Arc<Vec<Packet>>, hits: Arc<Vec<f64>>) {
        if let Ok(key) = self.decoder_key(model) {
            let mut cache = self.azr.reference_decode.borrow_mut();
            cache.put(key.clone(), Ok(packets));
            if let Some(entry) = cache.entries.iter_mut().rev().find(|entry| entry.key == key) {
                entry.hits = Some((model.trig_bus, model.trig_bus_val, hits));
            }
        }
    }
    pub(super) fn seed_accepted_decode(&self, record: &Record, packets: Arc<Vec<Packet>>) {
        if let Ok(key) = self.decoder_key_at(&self.m, Some(record)) {
            self.azr.reference_decode.borrow_mut().put(key, Ok(packets));
        }
    }
    pub(super) fn decoder_trigger_hits(&self, m: &Model) -> Option<Arc<Vec<f64>>> {
        let packets = self.reference_decode_packets(m).ok()?;
        let key = self.decoder_key(m).ok()?;
        let mut cache = self.azr.reference_decode.borrow_mut();
        let entry = cache.entries.iter_mut().find(|entry| entry.key == key)?;
        if let Some((kind, value, hits)) = &entry.hits {
            if *kind == m.trig_bus && *value == m.trig_bus_val {
                return Some(Arc::clone(hits));
            }
        }
        let mut times: Vec<_> = crate::trig::points(m, &packets);
        times.sort_by(f64::total_cmp);
        let hits = Arc::new(times);
        entry.hits = Some((m.trig_bus, m.trig_bus_val, Arc::clone(&hits)));
        Some(hits)
    }
    pub fn decode_busy(&self) -> bool {
        for k in 0..DECODERS {
            let model = self.m.dec_view(k);
            if !model.dec_on {
                continue;
            }
            if Self::decoder_uses_refs(&model)
                || self.decoder_uses_raw(&model)
                || model.deskew_ch.iter().any(|&shift| shift != 0.0)
            {
                let _ = self.reference_decode_packets(&model);
            }
        }
        self.azr.reference_decode.borrow().worker.as_ref().is_some_and(Worker::busy)
    }
    pub fn decode_events_result(&self) -> Result<String, &'static str> {
        for k in 0..DECODERS {
            if self.m.dec_is_on(k) {
                self.decode_packets_of(&self.m.dec_view(k))?;
            }
        }
        Ok(self.events_text())
    }
    /// What decoder `k` found out about its line in the last decode, as `oos_dsp::diagnosis::diagnose` says it, or
    /// the reason it could not decode (a record over the limit, a source that is off, min/max pairs): one sentence.
    /// `Err("Decode computing")` while it is still working. Empty `lines` when the decode is fine.
    pub fn decoder_diagnosis(&self, k: usize) -> Result<oos_dsp::diagnosis::Diagnosis, &'static str> {
        let m = self.m.dec_view(k);
        match self.decode_packets_of(&m) {
            Err(COMPUTING) => Err(COMPUTING),
            Err(why) => Ok(oos_dsp::diagnosis::Diagnosis { lines: vec![why.to_string()], ..Default::default() }),
            Ok(_) => Ok(crate::trig::diagnosis(&m).unwrap_or_default()),
        }
    }

    /// The first sentence of the diagnosis of a decoder that is on and decoded nothing, for the places that have one
    /// line (the band, the Events pane when it is empty).
    pub(super) fn decoder_note(&self, k: usize) -> Option<String> {
        let first = self.decoder_diagnosis(k).ok()?.lines.into_iter().next()?;
        Some(if first.to_ascii_lowercase().starts_with("nothing decoded") {
            first
        } else {
            format!("Nothing decoded: {first}")
        })
    }

    pub(super) fn decoder_sources(&self) -> Vec<usize> {
        (0..n_ch())
            .chain((0..NREF).filter(|&i| self.refs[i].is_some()).map(|i| DEC_REF_BASE + i))
            .chain(
                (0..NMATH)
                    .filter(|&k| !matches!(self.m.math_ch(k).op, MathOp::Off | MathOp::Fft))
                    .map(|k| DEC_MATH_BASE + k),
            )
            .collect()
    }
    /// The names a line's row of the Decode menu lists: the channels and the references that hold a waveform, and
    /// the line's own source when that is a reference that is empty now. A decoder left on "R1" after the reference
    /// was cleared, or after a restart, had a row of channels with none of them chosen and no word of R1.
    pub(super) fn decoder_source_names(&self, own: Option<usize>) -> Vec<String> {
        let mut sources = self.decoder_sources();
        if let Some(own) = own.filter(|s| {
            !sources.contains(s) && (matches!(decode_wave(*s), Some(WaveId::Ref(_))) || decode_math(*s).is_some())
        }) {
            sources.push(own);
            sources.sort_unstable();
        }
        sources.into_iter().map(decode_source_name).collect()
    }
    pub(super) fn decoder_vpd(&self) -> f32 {
        self.decoder_vpd_of(self.m.dec_src)
    }
    /// Volts per division of a decoder source (a channel or a reference).
    pub(super) fn decoder_vpd_of(&self, source: usize) -> f32 {
        self.decoder_samples(source).map_or(1.0, |(_, _, _, vpd)| vpd.max(1e-9))
    }
    #[cfg(test)]
    pub(super) fn decoder_worker_state_for_test(&self) -> (usize, usize) {
        let cache = self.azr.reference_decode.borrow();
        (usize::from(cache.worker.is_some()), cache.scans)
    }
    pub(super) fn decoder_source_step(&self, source: Option<usize>, optional: bool, delta: i32) -> Option<usize> {
        let mut sources: Vec<_> = self.decoder_sources().into_iter().map(Some).collect();
        if optional {
            sources.insert(0, None);
        }
        let at = sources.iter().position(|value| *value == source).unwrap_or(0);
        sources[(at as i32 + delta).rem_euclid(sources.len() as i32) as usize]
    }
    pub(crate) fn configure_decoder_source(&mut self, role: usize, source: Option<usize>) -> bool {
        if role > 4
            || ((role < 2 || (role == 2 && self.m.dec_proto == Proto::CPHY)) && source.is_none())
            || source.is_some_and(|id| !self.decoder_sources().contains(&id))
        {
            return false;
        }
        let assign = |m: &mut Model| {
            m.dec_la_in = false;
            match role {
                // roles 0 and 1 have a source: checked above
                0 => m.dec_src = source.unwrap_or(m.dec_src),
                1 => m.dec_src2 = source.unwrap_or(m.dec_src2),
                2 => m.dec_cs = source,
                3 => m.dec_src4 = source,
                _ => m.dec_diff = source,
            }
        };
        assign(&mut self.m);
        if self.m.trig_type == TrigType::Bus {
            if let Some(wave) = decode_wave(self.m.dec_src) {
                if matches!(wave, WaveId::Ref(_)) != self.record_mode() {
                    self.set_trigger_source(wave, false);
                    // a channel ends the reference session and brings back the set-up from before it: the source
                    // chosen here stays
                    assign(&mut self.m);
                } else {
                    self.m.set_trigger_wave(wave, false);
                }
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Break it catches: the decode of a channel's own samples took the points of a Peak detect record for
    /// samples (LIN at 2.5 MSa/s with 100 kpts, sync not measured, bytes wrong). A wave marked `pairs` is read by the
    /// means of neighbours on its own grid, also where another source is finer and the channel is aligned to it: the
    /// means of neighbours of the finer grid are still minimum and maximum in turn.
    #[test]
    fn points_in_turn_are_read_by_their_means_before_the_alignment() {
        use crate::trig::peak_tests::{lin_in_peak_detect, lin_right};
        let (m, peak, dt) = lin_in_peak_detect();
        let finer = vec![0.0; peak.len() * 4];
        let read = |pairs: bool, aligned: bool| {
            let mut waves = vec![BorrowedWave { source: 0, v: &peak, dt, t0: 0.0, pairs }];
            if aligned {
                waves.push(BorrowedWave { source: 1, v: &finer, dt: dt / 4.0, t0: 0.0, pairs: false });
            }
            let p = decode_borrowed(&m, &waves, 0.01).unwrap();
            let data = |x: &&oos_dsp::decode::Packet| x.kind == oos_dsp::decode::Kind::Data;
            (p.iter().filter(data).map(|x| x.value).collect::<Vec<_>>(), p.iter().all(|x| x.ok))
        };
        assert_ne!(read(false, false), lin_right(), "as samples the record made here is wrong");
        assert_eq!(read(true, false), lin_right(), "the points in turn");
        assert_eq!(read(true, true), lin_right(), "the points in turn, aligned to a finer source");
    }

    /// The live record (`decode_packets_of`) is read by the mode it was taken in (`Record::peak`), not the mode in
    /// force now, and a record of min/max pairs that is also Peak detect is averaged once.
    #[test]
    fn a_record_is_read_by_the_mode_it_was_taken_in() {
        use crate::trig::peak_tests::{lin_in_peak_detect, lin_right};
        let (m, peak, dt) = lin_in_peak_detect();
        let mut ui = crate::ui::testkit::demo_ui();
        let mut read = |pairs: bool, rec_peak: bool, now: AcqMode| {
            let ch: [Vec<f32>; NCH] = std::array::from_fn(|c| if c == 0 { peak.clone() } else { Vec::new() });
            let (t0, triggered) = (0.0, true);
            ui.last[0] =
                Some(Record { t0, dt, ch, triggered, partial: false, raw: None, la: None, pairs, peak: rec_peak });
            ui.counts.wfm_total += 1;
            let mut m = m.clone();
            m.acq = now;
            let p = ui.decode_packets_of(&m).unwrap();
            let data = |x: &&oos_dsp::decode::Packet| x.kind == oos_dsp::decode::Kind::Data;
            (
                p.iter().filter(data).map(|x| x.value).collect::<Vec<_>>(),
                p.iter().all(|x| x.ok),
                p.first().map(|x| x.t0),
            )
        };
        let right = |r: (Vec<u32>, bool, Option<f64>)| (r.0, r.1) == lin_right();
        assert!(right(read(false, true, AcqMode::Normal)), "taken in Peak detect, Normal now");
        assert!(!right(read(false, false, AcqMode::Peak)), "taken in Normal, Peak now: read as samples");
        // pairs and Peak detect: the means once, so the packets start where they start for pairs alone
        let once = read(true, false, AcqMode::Normal);
        assert!(right(once.clone()));
        assert_eq!(read(true, true, AcqMode::Peak).2, once.2, "averaged once");
    }

    /// The search of a protocol trigger in the live record (`search_events`) reads points in turn as the decoder
    /// does: a Peak detect record finds the field, read as samples it finds none.
    #[test]
    fn the_search_reads_a_peak_detect_record_like_the_decoder() {
        let (m, peak, dt) = crate::trig::peak_tests::lin_in_peak_detect();
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m = m;
        (ui.m.trig_type, ui.m.trig_src, ui.m.trig_bus, ui.m.trig_bus_val) = (TrigType::Bus, 0, BusWhat::Data, 0x30);
        let mut hits = |rec_peak: bool| {
            let ch: [Vec<f32>; NCH] = std::array::from_fn(|c| if c == 0 { peak.clone() } else { Vec::new() });
            let (t0, triggered) = (0.0, true);
            ui.last[0] = Some(Record {
                t0,
                dt,
                ch,
                triggered,
                partial: false,
                raw: None,
                la: None,
                pairs: false,
                peak: rec_peak,
            });
            ui.search_events().len()
        };
        assert_eq!(hits(false), 0, "as samples no byte is read");
        assert_eq!(hits(true), 1, "the data byte 0x30");
    }

    fn reference(v: Vec<f32>, dt: f64, t0: f64, vpd: f32) -> RefWave {
        RefWave { label: "decoder fixture".into(), vpd, ..RefWave::test(v, dt, t0) }
    }
    fn uart(byte: u8, count: usize) -> (Vec<f32>, f64) {
        let mut values = vec![3.3; count];
        let bits: Vec<bool> =
            std::iter::once(false).chain((0..8).map(|b| byte >> b & 1 != 0)).chain(std::iter::once(true)).collect();
        for (bit, high) in bits.into_iter().enumerate() {
            values[100 + bit * 20..100 + (bit + 1) * 20].fill(if high { 3.3 } else { 0.0 });
        }
        (values, 1.0 / (115_200.0 * 20.0))
    }
    fn uart_ui(count: usize) -> Ui {
        let mut ui = crate::ui::testkit::demo_ui();
        let (v, dt) = uart(0xa5, count);
        ui.refs[0] = Some(reference(v, dt, -0.005, 0.5));
        ui.m.dec_proto = Proto::UART;
        ui.m.dec_on = true;
        ui.m.dec_src = DEC_REF_BASE;
        ui.m.dec_baud = crate::decode::BAUDS.iter().position(|&v| v == 115_200.0).unwrap();
        ui.m.dec_thr = 1.65;
        ui.m.dec_bits = 8;
        ui.m.dec_parity = oos_dsp::decode::Parity::None;
        ui.m.run = RunState::Stop;
        ui.m.set_trigger_wave(WaveId::Ref(0), false);
        ui
    }
    fn wait(ui: &Ui) -> Arc<Vec<Packet>> {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            match ui.reference_decode_packets(&ui.m) {
                Ok(packets) => return packets,
                Err(COMPUTING) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("{error}"),
            }
        }
    }

    /// A math trace is a decoder source: the decode of M1 = R1 - R2 gives the packets of the differential option
    /// (R1 with R2 as its second line), and the id round-trips through a setup text.
    #[test]
    fn a_math_difference_decodes_like_the_differential_option() {
        let mut ui = uart_ui(1000);
        let (v, dt) = uart(0xa5, 1000);
        let plus: Vec<f32> = v.iter().map(|x| 1.0 + x / 2.0).collect();
        let minus: Vec<f32> = v.iter().map(|x| 1.0 - x / 2.0).collect();
        ui.refs[0] = Some(reference(plus, dt, -0.005, 0.5));
        ui.refs[1] = Some(reference(minus, dt, -0.005, 0.5));
        ui.m.dec_diff = Some(DEC_REF_BASE + 1);
        let differential = wait(&ui);
        assert!(differential.iter().any(|p| p.kind == oos_dsp::decode::Kind::Data && p.value == 0xa5));
        ui.m.dec_diff = None;
        ui.m.math = MathOp::Sub;
        ui.m.math_a = MATH_REF_BASE;
        ui.m.math_b = MATH_REF_BASE + 1;
        assert!(ui.decoder_sources().contains(&DEC_MATH_BASE), "M1 is on offer");
        assert!(ui.options(Item::DecSrc).contains(&"M1".into()), "the menu lists M1");
        ui.select_option(Item::DecSrc, "M1");
        assert_eq!(ui.m.dec_src, DEC_MATH_BASE);
        assert_eq!(decode_source_name(ui.m.dec_src), "M1");
        let over_math = wait(&ui);
        assert_eq!(over_math.as_ref(), differential.as_ref());
        let restored = crate::store::parse_setup(&crate::store::setup_text(&ui.m)).unwrap();
        assert_eq!(restored.dec_src, DEC_MATH_BASE);
        assert!(!ui.configure_decoder_source(0, Some(DEC_MATH_BASE + 1)), "M2 is off");
    }

    #[test]
    fn reference_uart_uses_original_time_and_shared_search_event_cache() {
        let mut ui = uart_ui(1000);
        assert!(ui.last[0].is_none());
        let packets = wait(&ui);
        let data = packets.iter().find(|p| p.kind == oos_dsp::decode::Kind::Data).unwrap();
        assert_eq!(data.value, 0xa5);
        assert!(data.t0 > -0.005 && data.t1 < -0.004);
        ui.trigger_type_set(TrigType::Bus);
        assert!(ui.record_mode(), "a reference Bus trigger must arm the stored source");
        ui.m.trig_bus = BusWhat::Data;
        ui.m.trig_bus_val = 0xa5;
        let hits = ui.search_events();
        assert_eq!(hits.as_ref(), &vec![data.t1]);
        ui.m.delay = 0.123;
        ui.m.zdelay = -0.7;
        assert!(Arc::ptr_eq(&hits, &ui.search_events()));
        assert!(ui.events_text().contains("D1 UART R1"));
        assert!(ui.decode_events_result().unwrap().contains("A5"));
        assert_eq!(ui.azr.reference_decode.borrow().scans, 1);
        assert_eq!(ui.replay.captures, 0);
        assert_eq!(ui.m.run, RunState::Stop);
        ui.m.trig_bus_val = 0x5a;
        assert!(ui.search_events().is_empty());
        assert_eq!(ui.azr.reference_decode.borrow().scans, 1);
    }

    #[test]
    fn loaded_sources_are_selectable_in_every_decoder_role_and_survive_setup() {
        let mut ui = uart_ui(1000);
        ui.refs[9] = Some(reference(vec![0.0; 1000], 1e-9, -1e-6, 0.2));
        for (item, role) in
            [(Item::DecSrc, 0), (Item::DecSrc2, 1), (Item::DecCs, 2), (Item::DecSrc4, 3), (Item::DecDiff, 4)]
        {
            assert!(ui.options(item).contains(&"R10".to_string()));
            assert!(ui.configure_decoder_source(role, Some(DEC_REF_BASE + 9)));
            assert_eq!(ui.value(item), "R10");
        }
        assert!(!ui.configure_decoder_source(0, Some(DEC_REF_BASE + 8)), "empty refs are not a valid selection");
        let text = crate::store::setup_text(&ui.m);
        let mut restored = crate::store::parse_setup(&text).unwrap();
        assert_eq!(
            (restored.dec_src, restored.dec_src2, restored.dec_cs, restored.dec_src4, restored.dec_diff),
            (
                DEC_REF_BASE + 9,
                DEC_REF_BASE + 9,
                Some(DEC_REF_BASE + 9),
                Some(DEC_REF_BASE + 9),
                Some(DEC_REF_BASE + 9)
            )
        );
        assert!(
            crate::store::set(&mut restored, "dec_src", "4").is_err(),
            "reserved LA IDs must not become CH indices"
        );
        ui.set_dec_proto(Proto::from_key("Jtag").unwrap());
        assert_eq!(ui.m.dec_src, DEC_REF_BASE + 9, "protocol changes must preserve stored roles");
        let mut canvas = Canvas::new(W, H);
        ui.render(&mut canvas);
    }

    #[test]
    fn four_wire_and_mixed_inputs_align_at_native_physical_times() {
        let mut ui = crate::ui::testkit::demo_ui();
        let clk = |t: f64| {
            if (0..8).any(|bit| (5.0 + 8.0 * bit as f64..9.0 + 8.0 * bit as f64).contains(&t)) {
                3.3
            } else {
                0.0
            }
        };
        let data = |t: f64| {
            let bit = ((t - 3.0).max(0.0) / 8.0) as usize;
            if bit < 8 && 0xa5u8 >> (7 - bit) & 1 != 0 {
                3.3
            } else {
                0.0
            }
        };
        ui.refs[0] = Some(reference((0..100).map(|i| data(-3.0 + i as f64)).collect(), 1e-6, -3e-6, 0.5));
        ui.refs[1] = Some(reference((0..220).map(|i| clk(-5.0 + i as f64 * 0.5)).collect(), 0.5e-6, -5e-6, 0.5));
        ui.m.dec_proto = Proto::SPI;
        ui.m.dec_src = DEC_REF_BASE;
        ui.m.dec_src2 = DEC_REF_BASE + 1;
        ui.m.dec_cs = None;
        ui.m.dec_thr = 1.65;
        ui.m.dec_bits = 8;
        let packets = wait(&ui);
        assert!(
            packets.iter().any(|packet| packet.kind == oos_dsp::decode::Kind::Data && packet.value == 0xa5),
            "{packets:?}"
        );
        assert!(packets.iter().all(|packet| packet.t0 >= -3e-6 && packet.t1 <= 96e-6));
        // Use the very same physical clock as a captured CH input instead.
        let clock = ui.refs[1].as_ref().unwrap();
        let mut rec = Record {
            ch: std::array::from_fn(|_| Vec::new()),
            dt: clock.dt,
            t0: clock.t0,
            triggered: true,
            partial: false,
            raw: None,
            la: None,
            pairs: false,
            peak: false,
        };
        rec.ch[1] = clock.v.as_ref().clone();
        rec.dt = clock.dt;
        rec.t0 = clock.t0;
        ui.last[0] = Some(rec);
        ui.m.dec_src2 = 1;
        assert_eq!(wait(&ui).as_ref(), packets.as_ref(), "mixed live/ref alignment must preserve packets");
        let full_clock = Arc::new(crate::board::Frame {
            seq: 19,
            dt: clock.dt,
            ch: [Vec::new(), clock.v.as_ref().clone(), Vec::new(), Vec::new()],
            ..crate::board::Frame::default()
        });
        let record = ui.last[0].as_mut().unwrap();
        record.ch[1] = vec![0.0; 4];
        record.dt *= 50.0;
        record.raw = Some(crate::wave::Raw {
            frame: full_clock,
            i0: 10,
            i1: 14,
            t0: clock.t0 + 10.0 * clock.dt,
            conditioned: false,
        });
        assert_eq!(
            wait(&ui).as_ref(),
            packets.as_ref(),
            "a decimated screen must not replace retained CH samples during mixed decode"
        );
        // All four JTAG roles are valid stored roles; non-overlap is explicit.
        for i in 2..4 {
            ui.refs[i] = Some(reference(vec![0.0; 100], 1e-6, 1.0, 0.5));
        }
        ui.m.dec_proto = Proto::from_key("Jtag").unwrap();
        ui.m.dec_src2 = DEC_REF_BASE + 1;
        ui.m.dec_cs = Some(DEC_REF_BASE + 2);
        ui.m.dec_src4 = Some(DEC_REF_BASE + 3);
        assert_eq!(ui.reference_decode_packets(&ui.m).unwrap_err(), "Decoder sources do not overlap");
    }

    // Issue #225 (halma1, DCC on a live track): the screen record of a live channel is peak-detect or resampled, so a
    // decoder of a live record reads the acquisition's own samples, for every role. Break it catches: the live path
    // decoding the screen record although `raw` holds the channel; a decoder whose raw channel is a second role (an SPI
    // clock) staying on the screen record; an envelope (min/max) frame or an empty raw channel taken as samples.
    #[test]
    fn a_live_decoder_reads_the_raw_samples_of_every_role() {
        let mut ui = uart_ui(4000);
        let (v, dt) = uart(0x5a, 4000);
        let frame = |c: usize, envelope: bool| {
            let mut ch: [Vec<f32>; NCH] = std::array::from_fn(|_| Vec::new());
            ch[c] = v.clone();
            Arc::new(crate::board::Frame { seq: 7, dt, ch, envelope, ..crate::board::Frame::default() })
        };
        let record = |raw: Option<Arc<crate::board::Frame>>| Record {
            // the screen: 40 points of the idle level, too coarse for a start bit
            ch: std::array::from_fn(|c| if c == 0 { vec![3.3; 40] } else { Vec::new() }),
            dt: dt * 100.0,
            t0: 0.0,
            triggered: true,
            partial: false,
            raw: raw.map(|frame| crate::wave::Raw { frame, i0: 0, i1: 40, t0: 0.0, conditioned: true }),
            la: None,
            pairs: false,
            peak: false,
        };
        ui.m.dec_src = 0;
        ui.last[0] = Some(record(None));
        assert!(!ui.decoder_uses_raw(&ui.m));
        assert!(ui.decode_packets_of(&ui.m).unwrap().iter().all(|p| p.value != 0x5a), "the screen has no byte");
        ui.last[0] = Some(record(Some(frame(0, false))));
        assert!(ui.decoder_uses_raw(&ui.m));
        let packets = ui.decode_packets_of(&ui.m).unwrap();
        assert!(packets.iter().any(|p| p.kind == oos_dsp::decode::Kind::Data && p.value == 0x5a), "{packets:?}");
        ui.last[0] = Some(record(Some(frame(0, true))));
        assert!(!ui.decoder_uses_raw(&ui.m), "min/max pairs are not samples");
        // SPI: data on CH1 without raw samples, the clock on CH2 with them
        ui.last[0] = Some(record(Some(frame(1, false))));
        ui.m.dec_proto = Proto::SPI;
        ui.m.dec_src2 = 1;
        ui.m.dec_cs = None;
        assert!(ui.decoder_uses_raw(&ui.m), "a second role with raw samples");
        ui.m.dec_src2 = 2;
        assert!(!ui.decoder_uses_raw(&ui.m), "no role has raw samples");
    }

    /// A long record of many bursts (a deep STOP record, 6 M samples, 150 bytes 40 000 samples apart) is decoded in
    /// full from the samples the acquisition kept, none thinned out; one above the decoder's limit says so with its
    /// count; a record above 1 M points that holds only min/max pairs says that, not "Nothing decoded".
    #[test]
    fn a_long_record_of_many_bursts_is_decoded_in_full_and_the_limits_are_named() {
        let mut ui = uart_ui(4000);
        let n = 6_000_000;
        let (mut v, dt) = uart(0, n);
        let (one, _) = uart(0, 400);
        let bytes: Vec<u8> = (0..150u32).map(|i| (i * 7 + 3) as u8).collect();
        for (k, &b) in bytes.iter().enumerate() {
            let (burst, _) = uart(b, 400);
            let at = 1000 + k * 40_000;
            v[at..at + one.len()].copy_from_slice(&burst);
        }
        let frame = Arc::new(crate::board::Frame {
            seq: 9,
            dt,
            ch: [v, Vec::new(), Vec::new(), Vec::new()],
            acq: (1.0 / dt, n),
            ..crate::board::Frame::default()
        });
        let record = |envelope: bool, frame: Arc<crate::board::Frame>| Record {
            ch: std::array::from_fn(|c| if c == 0 { vec![3.3; 40] } else { Vec::new() }),
            dt: dt * 1000.0,
            t0: 0.0,
            triggered: true,
            partial: false,
            raw: Some(crate::wave::Raw { frame, i0: 0, i1: n, t0: 0.0, conditioned: true }),
            la: None,
            pairs: envelope,
            peak: false,
        };
        ui.m.dec_src = 0;
        ui.last[0] = Some(record(false, frame.clone()));
        let packets = wait_live(&ui);
        let got: Vec<u8> =
            packets.iter().filter(|p| p.kind == oos_dsp::decode::Kind::Data).map(|p| p.value as u8).collect();
        assert_eq!(got, bytes, "every burst of the record, none missing");
        // only pairs of a deep record
        let pairs = Arc::new(crate::board::Frame { envelope: true, ..(*frame).clone() });
        ui.last[0] = Some(record(true, pairs));
        assert_eq!(ui.decode_packets_of(&ui.m).unwrap_err(), DEEP_PAIRS);
        // above the limit of the decoder
        let big = vec![0.0f32; MAX_POINTS + 1];
        let wave = BorrowedWave { source: 0, v: &big, dt: 1e-9, t0: 0.0, pairs: false };
        let err = decode_borrowed(&ui.m, &[wave], 0.1).unwrap_err();
        assert_eq!(err, format!("Record too long for the decoder: {} samples, limit {MAX_POINTS}", MAX_POINTS + 1));
    }

    fn wait_live(ui: &Ui) -> Arc<Vec<Packet>> {
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match ui.decode_packets_of(&ui.m) {
                Ok(packets) => return packets,
                Err(COMPUTING) => {
                    assert!(Instant::now() < deadline);
                    std::thread::sleep(Duration::from_millis(1));
                }
                Err(error) => panic!("{error}"),
            }
        }
    }

    #[test]
    fn large_decode_is_nonblocking_single_worker_and_discards_changed_source_results() {
        let mut ui = uart_ui(BACKGROUND_POINTS + 1000);
        // the call answers before the decode is done: the UI only queues it
        assert_eq!(ui.reference_decode_packets(&ui.m).unwrap_err(), COMPUTING);
        assert_eq!(ui.azr.reference_decode.borrow().scans, 1);
        let (v, dt) = uart(0x5a, BACKGROUND_POINTS + 1000);
        ui.refs[1] = Some(reference(v, dt, -0.008, 0.5));
        ui.m.dec_src = DEC_REF_BASE + 1;
        ui.workflow_epoch += 1;
        ui.replay = super::super::workflow::Replay::default();
        let packets = wait(&ui);
        assert!(packets.iter().any(|p| p.value == 0x5a));
        assert!(!packets.iter().any(|p| p.value == 0xa5));
        assert!(packets.iter().all(|p| p.t0 < -0.007));
        let cache = ui.azr.reference_decode.borrow();
        assert_eq!(cache.scans, 2);
        assert!(cache.worker.is_some());
        assert!(!cache.worker.as_ref().unwrap().busy());
        assert_eq!(cache.entries.len(), 1, "stale source result must not enter the accepted cache");
        drop(cache);
        assert!(Arc::ptr_eq(&packets, &wait(&ui)));
        assert_eq!(ui.replay.captures, 0);
    }

    #[test]
    fn all_twenty_seven_protocol_dispatchers_match_the_same_live_samples_as_refs() {
        for protocol in PROTOS.iter().copied().filter(|protocol| *protocol != Proto::CPHY) {
            let mut ui = crate::ui::testkit::demo_ui();
            ui.set_dec_proto(protocol);
            ui.m.dec_on = true;
            for channel in &mut ui.m.ch {
                channel.on = true;
            }
            let tdiv = (ui.m.bus_bit() * 20.0) as f32;
            ui.m.tscale = tscales().iter().position(|&scale| scale >= tdiv).unwrap();
            ui.m.trig_type = TrigType::Edge;
            ui.m.trig_mode = TrigMode::Auto;
            ui.step(0.05);
            let expected = ui.decode_packets().unwrap_or_else(|error| panic!("{protocol:?} live fixture: {error}"));
            let rec = ui.last[0].as_ref().unwrap();
            for c in 0..NCH {
                if !rec.ch[c].is_empty() {
                    ui.refs[c] = Some(reference(rec.ch[c].clone(), rec.dt, rec.t0, ui.m.ch[c].volts_per_div()));
                }
            }
            ui.m.dec_src += DEC_REF_BASE;
            ui.m.dec_src2 += DEC_REF_BASE;
            ui.m.dec_cs = ui.m.dec_cs.map(|c| DEC_REF_BASE + c);
            ui.m.dec_src4 = ui.m.dec_src4.map(|c| DEC_REF_BASE + c);
            ui.m.dec_diff = ui.m.dec_diff.map(|c| DEC_REF_BASE + c);
            ui.last[0] = None;
            let actual = ui.decode_packets().unwrap_or_else(|error| panic!("{protocol:?} reference fixture: {error}"));
            assert_eq!(actual, expected, "{protocol:?} native-interval packets changed after source remapping");
        }
    }

    #[test]
    fn cphy_three_reference_roles_decode_the_published_synchronized_word() {
        let mut symbols = vec![3; 12];
        symbols.extend([3, 4, 4, 4, 4, 4, 3]);
        // 7290 as it is on the wire: its least significant symbol first
        symbols.extend([0, 0, 1, 4, 2, 2, 0]);
        symbols.extend([4; 7]);
        let states = oos_dsp::cphy::states_for_symbols(0, &symbols);
        let mut ui = crate::ui::testkit::demo_ui();
        let dt = 5e-12;
        let t0 = -12e-9;
        for wire in 0..3 {
            let values = states
                .iter()
                .flat_map(|&state| std::iter::repeat_n(oos_dsp::cphy::wire_values(state)[wire], 20))
                .collect();
            ui.refs[wire] = Some(reference(values, dt, t0, 0.1));
        }
        ui.m.dec_src = DEC_REF_BASE;
        ui.set_dec_proto(Proto::CPHY);
        assert!(ui.configure_decoder_source(1, Some(DEC_REF_BASE + 1)));
        assert!(ui.configure_decoder_source(2, Some(DEC_REF_BASE + 2)));
        assert!(!ui.configure_decoder_source(2, None), "the third C-PHY wire is required");
        ui.m.dec_on = true;
        ui.m.trig_type = TrigType::Bus;
        ui.m.trig_bus = BusWhat::Data;
        ui.m.trig_bus_val = 0x7290;
        assert_eq!(ui.value(Item::DecSrc), "R1");
        assert_eq!(ui.value(Item::DecSrc2), "R2");
        assert_eq!(ui.value(Item::DecCs), "R3");
        assert_eq!(ui.item_label(Item::DecCs), "C");
        assert_eq!(ui.item_label(Item::DecThr), "Min level gap");
        let packets = wait(&ui);
        let word = packets
            .iter()
            .find(|packet| packet.kind == (oos_dsp::decode::Kind::Word { cmd: false }) && packet.value == 0x7290)
            .unwrap();
        assert!(word.ok && word.t0 >= t0 && word.t1 < 0.0);
        assert_eq!(ui.search_events().as_ref(), &vec![word.t1]);
        assert!(ui.events_text().contains("7290"));
        assert_eq!(ui.value(Item::DecCphyRate), "Auto");
        assert!(ui.decode_items().contains(&Item::DecCphyRate));
        assert!(ui.options(Item::DecCphyRate).contains(&"2.25 Gsym/s".to_string()));
        ui.select_option(Item::DecCphyRate, "2.25 Gsym/s");
        assert_eq!(ui.m.dec_cphy_rate, 2.25e9);
        assert!(
            !wait(&ui)
                .iter()
                .any(|packet| packet.kind == (oos_dsp::decode::Kind::Word { cmd: false }) && packet.value == 0x7290),
            "an intentionally slower known rate must change stability filtering, not remain an ignored UI label"
        );
        ui.select_option(Item::DecCphyRate, "10 Gsym/s");
        assert!(wait(&ui)
            .iter()
            .any(|packet| packet.kind == (oos_dsp::decode::Kind::Word { cmd: false }) && packet.value == 0x7290));
        ui.m.dec_cphy_rate = 1.234567890123e9;
        let restored = crate::store::parse_setup(&crate::store::setup_text(&ui.m)).unwrap();
        assert_eq!(restored.dec_cphy_rate, ui.m.dec_cphy_rate);
        ui.m.dec_select(1);
        assert_eq!(ui.m.dec_cphy_rate, 0.0);
        ui.m.dec_select(0);
        assert_eq!(ui.m.dec_cphy_rate, 1.234567890123e9);
        ui.select_option(Item::DecCphyRate, "Auto");
        assert_eq!(ui.m.dec_cphy_rate, 0.0, "the picker must also reset an arbitrary SCPI-provided rate");
        let mut canvas = Canvas::new(W, H);
        ui.render(&mut canvas);
    }
}
