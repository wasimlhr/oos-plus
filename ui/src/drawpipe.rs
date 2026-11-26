// SPDX-License-Identifier: GPL-2.0-or-later

//! Deposit pipeline (fork, docs/fork/perf-plan.md item 1), the draw path of the hardware (ADR 0059): every acquired
//! frame is used. The acquisition thread decodes only the newest frame of a batch for the UI queue (the record for measurements, layers
//! and the record path) and hands the read-out buffer itself (no copy) with the other frames to the deposit workers,
//! one per little core. The workers claim the frames a few at a time; while the FPGA's ring fills with the next batch
//! the acquisition thread claims single frames too (`help`, from `seq::acquire_batch_idle`). Each frame is
//! decoded into kept vectors and drawn with the fast path (`draw_frame`, one frame at a time; the frame loop draws
//! the same picture of the frames in its queue with `Ui::fast_draw_all`) into the claimer's own maps, with the screen mapping the UI published
//! (`View`); the trigger statistics and the cheap measurements (framestats.rs) are counted with it. Each claimer
//! draws on into its maps over its batches and, once a screen, folds them into one set in a shared mailbox, on its
//! own core (`hand`, `publish`); once per screen refresh the UI takes that set (`merge`): it swaps the maps in for
//! minimum persistence, else adds those of the channels on. It never iterates the frames, and never waits for the
//! workers.
//!
//! The picture is the one of `draw_frame` on every frame, one by one: the workers run it on the frames of a batch but
//! the newest, which takes the UI's path. A batch whose workers are still busy, frames of older settings and maps of
//! an older mapping are dropped and counted (`board::pipe::Counters`). So that the first of these stays rare, the
//! workers measure what a frame costs them and grant the acquisition thread as many frames as they draw in about one
//! picture (`grant`); it arms no more and waits while they still hold the buffers it needs (`lent`, ADR 0081). Where
//! the workers do not take a batch (no view: the fast
//! path does not apply; batches of one frame; the hardware average) the acquisition thread queues its frames for the
//! frame loop. The other sources (demo, replay, live, SIM) have no batches and no workers.
//!
//! The acquisition thread reaches the workers through what the board declares (`board::pipe`: `Workers`, `Feed`,
//! `Counters`, ADR 0074); `set_enabled` registers them there.

use crate::board::pipe::{counters, CodeLayout, CodesInto, Counters, DecodeInto, RawBatch};
use crate::board::{lock, Frame};
use crate::fastdraw::EdgeSeek;
use crate::model::*;
use crate::platform::Platform;
use crate::wave::{Phosphor, Renew};
use std::any::Any;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{channel, sync_channel, Receiver, Sender, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};

/// Trigger statistics of the fast path (`FastPath::edges`, `FastPath::off`).
pub struct Stats<'a> {
    pub edges: &'a mut [u64; 4],
    pub off: &'a mut (f64, f64),
}

/// One frame of the fast path into `ph`; false if its trigger edge was not found near the hardware trigger point.
/// Sparse frames (fewer than 2 samples per column) take the record path (`sparse::draw`, else `wave::to_record`).
pub fn draw_frame(ph: &mut Phosphor, m: &Model, f: &Frame, stats: Option<Stats>) -> bool {
    let (w, h) = (ph.w, ph.h);
    let tdiv = m.time_per_div() as f64;
    let x_per_sample = f.dt / tdiv * w as f64 / HDIV as f64;
    // min/max pairs of the live compression (compress.rs) take the dense path with a pair deposit
    let pairs = crate::compress::live(f) && x_per_sample <= 0.5;
    if !pairs && (x_per_sample > 0.5 || f.envelope || f.roll || f.scan || crate::wave::drawn_as_record(f, m)) {
        if let Some(t) = crate::sparse::draw(ph, m, f, 24) {
            return t;
        }
        let fr = std::sync::Arc::new(f.clone());
        let window = tdiv * HDIV as f64;
        let rec = crate::wave::to_record(&fr, m, 3600, window, m.delay as f64 - window / 2.0);
        for c in 0..NCH {
            if m.ch_drawn(c) && !rec.ch[c].is_empty() {
                ph.draw_ex(c, m, (m.time_per_div(), m.delay), rec.t0, rec.dt, &rec.ch[c], 24, None, rec.pairs);
            }
        }
        return rec.triggered;
    }
    let src = &f.ch[m.trig_src];
    let present: [bool; NCH] = std::array::from_fn(|c| !f.ch[c].is_empty());
    let refine = |s: EdgeSeek| {
        crate::trignear::edge_smooth_near(src, f.pre, s.level, s.hyst, s.rising, s.either, s.span, s.l, |_| true)
    };
    let deposit = |c: usize, map: &mut [u16], xm: &crate::fastdraw::Map| {
        if pairs {
            crate::compress::deposit_pairs(map, w, h, &f.ch[c], xm, 24, m.dots);
        } else if m.dots {
            crate::fastdraw::deposit_dots(map, w, h, &f.ch[c], xm, 24);
        } else {
            crate::fastdraw::deposit(map, w, h, &f.ch[c], xm, 24);
        }
    };
    dense(ph, m, f, Dense { x_per_sample, pairs, present }, stats, refine, deposit)
}

/// How `dense` lays out a frame: `x_per_sample` columns a sample, min/max `pairs` (live compression), the channels
/// `present` in the frame.
struct Dense {
    x_per_sample: f64,
    pairs: bool,
    present: [bool; NCH],
}

/// The dense fast path of `draw_frame` for the frame `f` (its samples reached only through `refine`, the trigger
/// refinement on the source channel, and `deposit`, one channel into its map): trigger statistics, mapping, channels.
fn dense(
    ph: &mut Phosphor,
    m: &Model,
    f: &Frame,
    lay: Dense,
    stats: Option<Stats>,
    refine: impl FnOnce(EdgeSeek) -> (Option<f64>, Option<oos_dsp::edges::Edge>),
    mut deposit: impl FnMut(usize, &mut [u16], &crate::fastdraw::Map),
) -> bool {
    let Dense { x_per_sample, pairs, present } = lay;
    let (w, h) = (ph.w, ph.h);
    let col = if pairs {
        crate::compress::smooth_len(x_per_sample)
    } else {
        (1.0 / x_per_sample).floor().clamp(1.0, 256.0) as usize
    };
    let t = refine_edge(m, f, col, refine);
    if let (Some(s), Some((k, d))) = (stats, t.class) {
        if k == 2 {
            *s.off = (s.off.0 + d, s.off.1.max(d));
        }
        s.edges[k] += 1;
    }
    if t.wrong {
        return false;
    }
    for c in 0..NCH {
        if m.ch[c].on && !m.ch_hidden[c] && present[c] {
            deposit(c, &mut ph.map[c], &screen_map(m, w, h, c, x_per_sample, t.ti));
        }
    }
    t.triggered
}

/// The trigger of a dense frame on the fast path, as the workers and the frame loop find it (`refine_edge`).
pub struct Trigger {
    /// the trigger's sample: the refined edge, else the hardware trigger point
    pub ti: f64,
    /// for the trigger statistics (`FastPath::edges`, `FastPath::off`): 0 the edge at the hardware point, 1 an edge of
    /// the other slope, 2 one elsewhere (with its distance in samples), 3 none; None with an LA line as the source
    pub class: Option<(usize, f64)>,
    pub triggered: bool,
    /// the hardware fired on the other slope: no edge of the set direction within half a screen, but one of the other
    /// direction (in Auto: at the trigger point) — a slow, noisy signal, #581. The frame is not drawn: at its trigger point it
    /// would show the mirrored slope.
    pub wrong: bool,
}

/// The trigger of the dense frame `f`, smoothed to `col` samples. `refine` (level, hysteresis, rising, either, half a
/// screen in samples as the span, `col` as the smoothing) finds the edge on the source channel: the workers and the
/// frame loop's fast path each keep their own choice of search.
pub fn refine_edge(
    m: &Model,
    f: &Frame,
    col: usize,
    refine: impl FnOnce(EdgeSeek) -> (Option<f64>, Option<oos_dsp::edges::Edge>),
) -> Trigger {
    let tdiv = m.time_per_div() as f64;
    // an LA line as the source: the hardware trigger point stands, the analog samples cannot refine it
    let on_la = m.trig_on_la();
    // the edge of the set direction nearest the hardware trigger point, up to half a screen away, on the signal smoothed
    // to one screen column: at decimated rates the hardware point lands near the level on either edge (trigger.md 8)
    let hyst = m.trig_hyst_div() * m.ch[m.trig_src].volts_per_div();
    let (rising, either) = (m.slope != Slope::Fall, m.slope == Slope::Either);
    let half = (tdiv * HDIV as f64 / 2.0 / f.dt) as usize;
    let near = |t: f64| (t - f.pre as f64).abs() <= (col + 16) as f64;
    let (edge, class, wrong) = if on_la {
        (None, None, false)
    } else {
        let (e, any) = refine(EdgeSeek::new(crate::wave::sw_trig_level(m, f), hyst, rising, either, half, col));
        let class = match (e, any) {
            (None, _) => (3, 0.0),
            (Some(t), _) if near(t) => (0, 0.0),
            (Some(_), Some(n)) if !either && n.rising != rising => (1, 0.0),
            (Some(t), _) => (2, (t - f.pre as f64).abs()),
        };
        // Normal and Single: every frame was triggered, so with no edge of the set direction within half a screen an
        // edge of the other direction there is the one it fired on (on a slow slope the comparator fires hundreds of
        // samples from the level crossing, #580); Auto: only one at the trigger point, as a frame of the timeout has
        // no edge of its own
        let triggered_frame = m.trig_mode != TrigMode::Auto;
        let wrong = e.is_none() && !either && any.is_some_and(|n| n.rising != rising && (triggered_frame || near(n.t)));
        (e, Some(class), wrong)
    };
    // a filtered trigger path (coupling): the hardware point stands where the samples show no crossing
    let triggered = edge.is_some() || on_la || m.trig_coupling() != TrigCoup::Dc;
    Trigger { ti: edge.unwrap_or(f.pre as f64), class, triggered: triggered && !wrong, wrong }
}

/// Where channel `c` of a dense frame lands on a `w` x `h` map, `x_per_sample` columns a sample, the trigger at sample
/// `ti`.
pub fn screen_map(m: &Model, w: usize, h: usize, c: usize, x_per_sample: f64, ti: f64) -> crate::fastdraw::Map {
    let ch = &m.ch[c];
    let tdiv = m.time_per_div() as f64;
    let x0 = w as f64 / 2.0 - m.delay as f64 / tdiv * w as f64 / HDIV as f64;
    let ppd_y = h as f32 / VDIV;
    let vpd = ch.volts_per_div();
    crate::fastdraw::Map {
        ti,
        x_per_sample,
        x0,
        y_per_volt: -ppd_y / vpd,
        y0: h as f32 / 2.0 - ch.offset / vpd * ppd_y,
    }
}

/// Smallest read-out the workers draw from its words (`draw_frame_codes`): 64 kB. Below it the f32 copy stays in the
/// caches and decoding first was as fast or faster on the host (12.5 k points, 1 channel: 1.08x the time from the words).
pub const CODES_MIN_BYTES: usize = 64 << 10;

/// Most words per group the workers draw from the words: each channel's deposit reads every group, so with four
/// channels the read-out is read four times; on the host that was 0.95x the decode path, too little to rely on.
pub const CODES_MAX_STRIDE: usize = 2;

impl CodeLayout {
    /// Channel `c` of the read-out `bytes` (empty when not in it).
    pub fn channel<'a>(&self, bytes: &'a [u8], c: usize) -> crate::fastdraw::Codes<'a> {
        let words = bytes.len() / 2;
        let (word, n) = match self.word[c] {
            Some(k) if k < words => (k, (words - k).div_ceil(self.stride)),
            _ => (0, 0),
        };
        // SAFETY: u16 has no invalid bit patterns; align_to only hands out the aligned middle part
        let (pre, words, _) = unsafe { bytes.align_to::<u16>() };
        let words = (pre.is_empty() && cfg!(target_endian = "little")).then_some(words);
        crate::fastdraw::Codes { bytes, words, stride: self.stride, word, n, affine: self.affine[c], volt: self.volt }
    }
}

#[cfg(test)]
thread_local! {
    /// wide searches of the codes path (tests)
    static WIDE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

thread_local! {
    /// the trigger source's volts that the codes path decodes (the near window, or all of them for the wide search)
    static TRIG: std::cell::RefCell<Vec<f32>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `draw_frame` straight from the read-out `bytes` of a frame (`lay`, meta data in `f`, whose channel vectors stay
/// empty): the same picture and statistics without the f32 copy of every channel. The trigger source is decoded only
/// around the hardware trigger point (the near search, `trignear::edge_near_window`), all of it only when the wide
/// search is needed. None where this path does not apply (sparse, pairs, roll or scan, dots, a frame drawn as its
/// record, a coupling that needs the frame mean): then decode and call `draw_frame`.
pub fn draw_frame_codes(
    ph: &mut Phosphor,
    m: &Model,
    f: &Frame,
    bytes: &[u8],
    lay: &CodeLayout,
    stats: Option<Stats>,
) -> Option<bool> {
    let (w, h) = (ph.w, ph.h);
    let tdiv = m.time_per_div() as f64;
    let x_per_sample = f.dt / tdiv * w as f64 / HDIV as f64;
    if x_per_sample > 0.5
        || f.envelope
        || f.roll
        || f.scan
        || m.dots
        || crate::wave::drawn_as_record(f, m)
        || !matches!(m.trig_coupling(), TrigCoup::Dc | TrigCoup::HfReject)
    {
        return None;
    }
    let present: [bool; NCH] = std::array::from_fn(|c| lay.channel(bytes, c).n > 0);
    let refine = |s: EdgeSeek| {
        let src = lay.channel(bytes, m.trig_src);
        TRIG.with(|t| {
            let v = &mut *t.borrow_mut();
            let (a, b) = crate::trignear::near_window(f.pre, src.n, s.l);
            (lay.volts)(bytes, src.stride, src.word, a, b, src.affine, v);
            if let Some(r) = crate::trignear::edge_near_window(v, a, src.n, f.pre, s, |_| true) {
                return r;
            }
            #[cfg(test)]
            WIDE.with(|n| n.set(n.get() + 1));
            (lay.volts)(bytes, src.stride, src.word, 0, src.n, src.affine, v);
            crate::trignear::edge_smooth_near(v, f.pre, s.level, s.hyst, s.rising, s.either, s.span, s.l, |_| true)
        })
    };
    let deposit = |c: usize, map: &mut [u16], xm: &crate::fastdraw::Map| {
        crate::fastdraw::deposit_codes(map, w, h, &lay.channel(bytes, c), xm, 24)
    };
    Some(dense(ph, m, f, Dense { x_per_sample, pairs: false, present }, stats, refine, deposit))
}

/// The buffer while the drawers have it: back to the acquisition thread's pool when the last share is drawn.
struct Lease {
    raw: Option<Box<dyn RawBatch>>,
    home: Sender<Box<dyn RawBatch>>,
    /// buffers out (`Pipe::lent`), one less when this one goes home
    lent: Arc<AtomicUsize>,
}

impl Drop for Lease {
    fn drop(&mut self) {
        if let Some(r) = self.raw.take() {
            // the pool is gone only when the pipe is: nothing to return to
            let _ = self.home.send(r);
        }
        self.lent.fetch_sub(1, Ordering::Release);
    }
}

/// The time a grant is for (`Pipe::grant`): about one picture at 30 screens a second.
const GRANT_PERIOD_NS: u64 = 33_000_000;

/// What a frame costs a worker under the current mapping: a moving average over its batches of the time from taking
/// a batch to handing its maps on, per frame it drew (folds included), in nanoseconds; 0 while none is measured.
#[derive(Default)]
struct Cost {
    gen: u64,
    ns: u64,
}

impl Cost {
    /// A worker's batch of view `gen`: `frames` drawn in `ns`. A new mapping starts the average over.
    fn note(&mut self, gen: u64, frames: usize, ns: u64) {
        let per = ns / frames.max(1) as u64;
        self.ns = if gen == self.gen && self.ns > 0 { (3 * self.ns + per) / 4 } else { per.max(1) };
        self.gen = gen;
    }

    /// Frames `workers` draw in `GRANT_PERIOD_NS` under view `gen`: from 1 to `cap`, `cap` while nothing is measured.
    fn grant(&self, gen: u64, workers: usize, cap: u32) -> u32 {
        if gen != self.gen || self.ns == 0 {
            return cap;
        }
        (GRANT_PERIOD_NS * workers.max(1) as u64 / self.ns).clamp(1, cap.max(1) as u64) as u32
    }
}

/// A batch with the workers: its frames are claimed a few at a time by whoever is free (the workers, and the
/// acquisition thread while it waits for the next batch: `help`). Frames never claimed count as dropped.
pub(crate) struct Batch {
    /// the buffer until the last frames are claimed; then only their claimers hold it, while they draw: it goes back
    /// with the last frame drawn, not when a worker that still holds the batch (folding its maps, or with the batch
    /// in its queue) lets it go
    lease: Mutex<Option<Arc<Lease>>>,
    fb: usize,
    frames: usize,
    decode: DecodeInto,
    /// the board's view without decoding (`draw_frame_codes`), where it has one
    codes: Option<CodesInto>,
    stamp: u64,
    /// the number (`Frame::seq`) of frame 0 of the batch: the order of the Average's runs (`average::Run`)
    seq0: u64,
    /// the first frame nobody has claimed; written under `lease`
    next: AtomicUsize,
    counters: &'static Counters,
    /// batches not done yet (`Pipe::live`)
    live: Arc<AtomicUsize>,
}

impl Batch {
    /// The next `n` frames nobody has taken yet, with a share of the buffer to draw them from.
    fn claim(&self, n: usize) -> Option<(Range<usize>, Arc<Lease>)> {
        let mut lease = lock(&self.lease);
        let i = self.next.load(Ordering::Relaxed);
        if i >= self.frames {
            return None;
        }
        let end = (i + n).min(self.frames);
        self.next.store(end, Ordering::Relaxed);
        let share = if end == self.frames { lease.take() } else { lease.clone() };
        share.map(|s| (i..end, s))
    }
}

impl Drop for Batch {
    fn drop(&mut self) {
        let left = self.frames.saturating_sub(self.next.load(Ordering::Relaxed));
        Counters::add(&self.counters.dropped, left as u64);
        self.live.fetch_sub(1, Ordering::Release);
    }
}

/// Frames a worker claims at a time.
const CLAIM: usize = 4;

/// Frames a worker claims at a time in the software Average: one run each (`average::Run`, the points of every
/// channel in f64), which the frame loop applies to the average.
const AVG_CLAIM: usize = 16;

/// The screen mapping the workers draw with, and a generation that changes with anything the drawing depends on.
/// `zoom`: the zoom picture's model (its time base and delay as the main ones, `Ui::zoom_draw_model`) and size, when
/// the zoom is on: every frame is drawn into it too.
struct View {
    gen: u64,
    m: Model,
    w: usize,
    h: usize,
    zoom: Option<(Model, usize, usize)>,
}

impl View {
    /// The sizes of the main and the zoom maps drawn with this view ((1, 1) for the zoom when it is off).
    fn dims(&self) -> Dims {
        ((self.w, self.h), self.zoom.as_ref().map_or((1, 1), |z| (z.1, z.2)))
    }
}

/// The sizes of the main and the zoom maps.
type Dims = ((usize, usize), (usize, usize));

/// The fields of the model `draw_frame` depends on (a change of any other field keeps the workers' maps).
#[derive(Clone, PartialEq)]
struct Key {
    ch: [Channel; NCH],
    tscale: usize,
    delay: f32,
    trig: (usize, Option<u8>, TrigType, Slope, f32, TrigCoup, bool, bool),
    acq: AcqMode,
    /// the software Average's count: the workers fold their runs with it (`average::Run`)
    avg_count: Option<u32>,
    dots: bool,
    size: (usize, usize),
    /// the zoom picture's own key
    zoom: Option<Box<Key>>,
}

impl Key {
    fn of(m: &Model, w: usize, h: usize) -> Key {
        Key {
            ch: m.ch.clone(),
            tscale: m.tscale,
            delay: m.delay,
            trig: (
                m.trig_src,
                m.trig_line(),
                m.trig_type,
                m.slope,
                m.trig_level,
                m.trig_coup,
                m.trig_nrej,
                m.trig_on_la(),
            ),
            acq: m.acq,
            avg_count: (m.acq == AcqMode::Average).then_some(m.avg_count),
            dots: m.dots,
            size: (w, h),
            zoom: None,
        }
    }
}

/// What a worker drew and the UI has not merged yet.
struct Acc {
    gen: u64,
    ph: Phosphor,
    /// the zoom picture's maps ((1, 1) while the zoom is off), the same channels as `ph`
    phz: Phosphor,
    /// the maps that may hold counts: the channels on in the view drawn with (drawing writes no other map), all of
    /// them for maps that were the UI's picture (`Pipe::merge`); only these are cleared and added
    used: [bool; NCH],
    /// statistics of the cheap measurements over the frames (framestats.rs)
    folds: crate::framestats::Folds,
    frames: u64,
    triggered: u64,
    edges: [u64; 4],
    off: (f64, f64),
}

impl Acc {
    fn new(gen: u64, ((w, h), (zw, zh)): Dims) -> Acc {
        let (ph, phz) = (Phosphor::new(w, h), Phosphor::new(zw, zh));
        let (used, folds) = ([false; NCH], Vec::new());
        Acc { gen, ph, phz, used, folds, frames: 0, triggered: 0, edges: [0; 4], off: (0.0, 0.0) }
    }
    fn dims(&self) -> Dims {
        ((self.ph.w, self.ph.h), (self.phz.w, self.phz.h))
    }
    /// Empty again, keeping the maps' memory.
    fn reset(&mut self, gen: u64) {
        for c in 0..NCH {
            if std::mem::take(&mut self.used[c]) {
                clear_map(&mut self.ph.map[c]);
                clear_map(&mut self.phz.map[c]);
            }
        }
        self.folds.clear();
        (self.gen, self.frames, self.triggered, self.edges, self.off) = (gen, 0, 0, [0; 4], (0.0, 0.0));
    }
    /// `o` added: its maps (the ones it uses), statistics and counts.
    fn add(&mut self, o: &Acc) {
        for c in (0..NCH).filter(|&c| o.used[c]) {
            add_map(&mut self.ph.map[c], &o.ph.map[c]);
            add_map(&mut self.phz.map[c], &o.phz.map[c]);
            self.used[c] = true;
        }
        crate::framestats::add_folds(&mut self.folds, &o.folds);
        self.frames += o.frames;
        self.triggered += o.triggered;
        for (a, b) in self.edges.iter_mut().zip(o.edges) {
            *a += b;
        }
        self.off = (self.off.0 + o.off.0, self.off.1.max(o.off.1));
    }
}

/// The mailbox of the workers and the helper (the drawers): the set the UI takes next (`full`), each drawer's own set
/// between its batches (`parked`), and the maps given back to be cleared and reused. A drawer hands its set on once a
/// screen, after its first batch since a merge, folded into `full` (`publish`); after its other batches it parks the
/// set and draws on into it with the next one (`hand`). So the sets are folded a few times a screen, not once a batch
/// and drawer, and `full` is whole long before the UI takes it. Held only to move a set in or out, never while maps
/// are added: the UI never waits for the drawers.
#[derive(Default)]
struct Mail {
    full: Option<Acc>,
    /// one slot per drawer: the workers', then the helper's
    parked: Vec<Option<Acc>>,
    spare: Vec<Acc>,
    /// merges so far
    epoch: u64,
    /// the software Average's runs of frames, folded by the drawers (`average::Run`), for the UI
    avg: Vec<average::Run>,
    /// sets folded into another (`publish`)
    #[cfg(test)]
    folds: u64,
}

impl Mail {
    /// Sets kept for reuse at most: one per drawer and the UI's, more are freed.
    const SPARES: usize = 8;

    /// The mailbox locked. It is whole between two moves, so a drawer that panicked stops neither the others nor the
    /// UI. (The other states of the pipeline are locked with `board::lock`, which panics then.)
    fn lock(m: &Mutex<Mail>) -> std::sync::MutexGuard<'_, Mail> {
        m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn keep(&mut self, a: Acc) {
        if self.spare.len() < Self::SPARES {
            self.spare.push(a);
        }
    }
}

/// The acquisition thread's state as a helper (`Pipe::help`): its maps, its kept frame, and the merge it last
/// handed its maps on after.
#[derive(Default)]
pub struct Helper {
    acc: Option<Acc>,
    spare: Option<Acc>,
    f: Frame,
    seen: Option<u64>,
}

/// The view and the settings stamp the UI is at; None while the fast path does not apply.
#[derive(Default)]
struct ViewState {
    view: Option<Arc<View>>,
    stamp: u64,
    key: Option<Key>,
    gen: u64,
}

pub struct Pipe {
    view: Arc<Mutex<ViewState>>,
    counters: &'static Counters,
    workers: Vec<SyncSender<Arc<Batch>>>,
    /// the mailbox of the workers and the helper
    mail: Arc<Mutex<Mail>>,
    /// the helper's slot in it, after the workers'
    helper: usize,
    /// batches the helper may take frames of, oldest first
    open: Mutex<std::collections::VecDeque<std::sync::Weak<Batch>>>,
    /// batches fed and not done (drawn and handed on, or dropped)
    live: Arc<AtomicUsize>,
    /// read-out buffers fed and not back yet
    lent: Arc<AtomicUsize>,
    /// what a frame costs the workers (`grant`)
    cost: Arc<Mutex<Cost>>,
    home: Sender<Box<dyn RawBatch>>,
    pool: Mutex<Receiver<Box<dyn RawBatch>>>,
}

/// Merged frames: how many, how many found their trigger, the statistics of the cheap measurements over them, and
/// the pixels the merge added on the caller's thread (none when the maps were swapped in; else the blocks of the maps
/// of the channels on that hold counts, `add_map`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Merged {
    pub frames: u64,
    pub triggered: u64,
    pub folds: crate::framestats::Folds,
    pub px: u64,
}

impl Pipe {
    /// A pipe with `n` workers counting into `counters`; `little`: the workers go to the little cores.
    pub fn start(n: usize, little: bool, counters: &'static Counters) -> Pipe {
        let view: Arc<Mutex<ViewState>> = Default::default();
        // a slot for each worker and, last, the helper's
        let mail = Arc::new(Mutex::new(Mail { parked: (0..=n).map(|_| None).collect(), ..Default::default() }));
        let mut workers = Vec::new();
        let cost: Arc<Mutex<Cost>> = Default::default();
        for k in 0..n {
            // a few batches at most: their buffers (`hw::POOL`) come back with the last frame drawn, and a worker
            // behind finds nothing left to claim in the ones the others drew
            let (tx, rx) = sync_channel::<Arc<Batch>>(16);
            let (v, m, c) = (view.clone(), mail.clone(), cost.clone());
            match std::thread::Builder::new().name(format!("deposit{}", k)).spawn(move || {
                if little {
                    crate::placement::to_little_cores();
                }
                work(k, rx, &v, &m, &c)
            }) {
                Ok(_) => workers.push(tx),
                Err(e) => eprintln!("oos-ui: deposit worker: {}", e),
            }
        }
        let (home, pool) = channel();
        Pipe {
            view,
            counters,
            workers,
            mail,
            helper: n,
            open: Default::default(),
            live: Default::default(),
            lent: Default::default(),
            cost,
            home,
            pool: Mutex::new(pool),
        }
    }

    /// The UI's mapping for the frames to come (None: the fast path does not apply).
    pub fn set_view(&self, m: Option<&Model>, w: usize, h: usize) {
        self.set_view_zoom(m, w, h, None);
    }

    /// `set_view` with the zoom picture's model and size (`Ui::zoom_draw_model`) while the zoom is on.
    pub fn set_view_zoom(&self, m: Option<&Model>, w: usize, h: usize, zoom: Option<(&Model, usize, usize)>) {
        let mut v = lock(&self.view);
        let Some(m) = m else {
            (v.view, v.key) = (None, None);
            return;
        };
        let mut key = Key::of(m, w, h);
        key.zoom = zoom.map(|(z, zw, zh)| Box::new(Key::of(z, zw, zh)));
        if v.key.as_ref() != Some(&key) {
            v.gen += 1;
            let zoom = zoom.map(|(z, zw, zh)| (z.clone(), zw, zh));
            v.view = Some(Arc::new(View { gen: v.gen, m: m.clone(), w, h, zoom }));
            v.key = Some(key);
        }
        v.stamp = m.hw_stamp;
    }

    /// The workers have batches to draw (fed and not done).
    pub fn busy(&self) -> bool {
        self.live.load(Ordering::Relaxed) > 0
    }

    /// The fast path applies: the workers take frames.
    pub fn accepting(&self) -> bool {
        lock(&self.view).view.is_some()
    }

    /// Frames the workers draw in about one picture at what a frame cost them under the current mapping, from 1 to
    /// `cap`; `cap` before a batch of this mapping is measured.
    pub fn grant(&self, cap: u32) -> u32 {
        let gen = lock(&self.view).gen;
        lock(&self.cost).grant(gen, self.workers.len(), cap)
    }

    /// Read-out buffers fed and not given back yet.
    pub fn lent(&self) -> usize {
        self.lent.load(Ordering::Acquire)
    }

    /// A buffer back from the workers, to read the next batch into (None: all are still out).
    pub fn take_buffer(&self) -> Option<Box<dyn Any + Send>> {
        lock(&self.pool).try_recv().ok().map(|r| r.into_any())
    }

    /// `feed_codes` without the board's view of the read-out words (tests).
    #[cfg(test)]
    pub fn feed(&self, raw: Box<dyn RawBatch>, fb: usize, frames: usize, stamp: u64, decode: DecodeInto) {
        self.feed_codes(raw, fb, 0, frames, stamp, decode, None);
    }

    /// From the acquisition thread: frames `first..frames` of `fb` bytes in `raw` (the ones before `first` are left
    /// out: frame 0 of a batch, issue #5), recorded under settings stamp `stamp`, for the workers (and the helper) to
    /// claim, decoded with `decode` or, without per-frame measurements, drawn from their words with `codes`
    /// (`draw_frame_codes`). The buffer comes back through `take_buffer` when the batch is done. Frames nobody claims
    /// are counted as dropped. Returns the batch (tests hold it to let the helper alone draw it).
    #[cfg(test)]
    #[allow(clippy::too_many_arguments)] // the parts of one batch, each from a different place of the board loop
    pub fn feed_codes(
        &self,
        raw: Box<dyn RawBatch>,
        fb: usize,
        first: usize,
        frames: usize,
        stamp: u64,
        decode: DecodeInto,
        codes: Option<CodesInto>,
    ) -> Arc<Batch> {
        self.feed_batch(raw, (fb, first, frames, stamp, 0), decode, codes)
    }

    /// `feed_codes` with the number of frame 0 (`seq0`, `Frame::seq`): the order of the Average's runs.
    fn feed_batch(
        &self,
        raw: Box<dyn RawBatch>,
        (fb, first, frames, stamp, seq0): (usize, usize, usize, u64, u64),
        decode: DecodeInto,
        codes: Option<CodesInto>,
    ) -> Arc<Batch> {
        let frames = raw.bytes().len().checked_div(fb).map_or(0, |n| frames.min(n));
        let next = AtomicUsize::new(first.min(frames));
        self.lent.fetch_add(1, Ordering::AcqRel);
        let lease = Lease { raw: Some(raw), home: self.home.clone(), lent: self.lent.clone() };
        let b = Arc::new(Batch {
            lease: Mutex::new(Some(Arc::new(lease))),
            fb,
            frames,
            decode,
            codes,
            stamp,
            seq0,
            next,
            counters: self.counters,
            live: self.live.clone(),
        });
        self.live.fetch_add(1, Ordering::Acquire);
        {
            let mut open = lock(&self.open);
            open.retain(|w| w.strong_count() > 0);
            open.push_back(Arc::downgrade(&b));
        }
        for w in &self.workers {
            // full only if the worker is far behind: the others take its frames (all of them: dropped, counted)
            if let Err(TrySendError::Disconnected(_)) = w.try_send(b.clone()) {
                eprintln!("oos-ui: deposit worker gone");
            }
        }
        b
    }

    /// From the acquisition thread while the ring fills (`seq::acquire_batch_idle`): draws one frame of the oldest open batch into the
    /// helper's maps. False when there is none (the helper's maps then go to the UI).
    pub fn help(&self, h: &mut Helper) -> bool {
        let b = {
            let mut open = lock(&self.open);
            loop {
                match open.front().map(|w| w.upgrade()) {
                    None => break None,
                    Some(Some(b)) if b.next.load(Ordering::Relaxed) < b.frames => break Some(b),
                    Some(_) => {
                        open.pop_front();
                    }
                }
            }
        };
        // the software Average is folded by the workers in runs; frames one at a time would make a run each
        let Some(b) = b.filter(|_| !self.view_now().0.is_some_and(|v| v.m.acq == AcqMode::Average)) else {
            self.help_done(h);
            return false;
        };
        let Some((r, lease)) = b.claim(1) else { return true };
        let (v, stamp) = self.view_now();
        match v.filter(|_| b.stamp >= stamp) {
            None => Counters::add(&self.counters.dropped, r.len() as u64),
            Some(v) => {
                if h.acc.as_ref().is_some_and(|a| a.gen != v.gen) {
                    self.help_done(h);
                }
                if h.acc.is_none() {
                    h.acc = Some(take_set(&self.mail, self.helper, &mut h.spare, &v, self.counters));
                }
                let a = h.acc.as_mut().expect("helper maps");
                draw_range(&b, &lease, r, &v, a, &mut h.f, self.counters);
            }
        }
        true
    }

    /// The helper's maps to the UI: handed on or parked (`hand`), as a worker's after a batch.
    pub fn help_done(&self, h: &mut Helper) {
        if let Some(s) = hand(&self.mail, self.helper, &mut h.seen, h.acc.take(), self.counters) {
            h.spare = Some(s);
        }
    }

    /// The software Average's runs the drawers folded since the last call under the current view, oldest frames
    /// first; those of an earlier view (another time base, channel, trigger or count) are dropped and counted.
    pub fn take_average(&self) -> Vec<average::Run> {
        let gen = lock(&self.view).view.as_ref().map(|v| v.gen);
        let mut runs = std::mem::take(&mut Mail::lock(&self.mail).avg);
        let old: u64 = runs.iter().filter(|r| Some(r.gen) != gen).map(|r| r.frames as u64).sum();
        Counters::add(&self.counters.dropped, old);
        runs.retain(|r| Some(r.gen) == gen);
        runs.sort_by_key(|r| r.seq);
        runs
    }

    fn view_now(&self) -> (Option<Arc<View>>, u64) {
        let s = lock(&self.view);
        (s.view.clone(), s.stamp)
    }

    /// Adds what the workers drew under the current mapping to `ph` and the trigger statistics to `edges`, `off`.
    /// `renew`: what `ph` needs before the first frames of the screen step, done (and set to `Keep`) when anything is
    /// merged; a picture to clear (minimum persistence) is replaced by the drawn maps themselves.
    /// One set of maps a call, whatever the number of workers (they fold theirs into one, `publish`): the frame loop
    /// adds the maps of the channels on, or with `Clear` swaps them in and adds nothing (issue #186). Without a
    /// whole set it takes a parked one: the frames of a drawer that has no more batches come with a later screen.
    /// `phz`: the zoom picture while the zoom is on (the view of `set_view_zoom`), renewed as `ph`, with the zoom maps.
    pub fn merge(
        &self,
        ph: &mut Phosphor,
        phz: Option<&mut Phosphor>,
        renew: &mut Renew,
        edges: &mut [u64; 4],
        off: &mut (f64, f64),
    ) -> Merged {
        let gen = lock(&self.view).view.as_ref().map(|v| v.gen);
        let mut got = Merged::default();
        let full = {
            let mut mb = Mail::lock(&self.mail);
            mb.epoch += 1;
            match mb.full.take() {
                Some(a) => Some(a),
                None => mb.parked.iter_mut().find_map(Option::take),
            }
        };
        let Some(mut a) = full else { return got };
        // the zoom maps go where they fit: drawn with the zoom on, the same size (a set without them has (1, 1))
        let mut phz = phz.filter(|z| (a.phz.w, a.phz.h) == (z.w, z.h));
        if Some(a.gen) == gen && (a.ph.w, a.ph.h) == (ph.w, ph.h) && a.frames > 0 {
            if *renew == Renew::Clear {
                // the picture cleared and the maps added is the maps: the old picture goes back to be cleared
                std::mem::swap(&mut ph.map, &mut a.ph.map);
                if let Some(z) = phz.as_deref_mut() {
                    std::mem::swap(&mut z.map, &mut a.phz.map);
                }
                a.used = [true; NCH];
            } else {
                ph.renew(*renew);
                if let Some(z) = phz.as_deref_mut() {
                    z.renew(*renew);
                }
                for c in (0..NCH).filter(|&c| a.used[c]) {
                    got.px += add_map(&mut ph.map[c], &a.ph.map[c]);
                    if let Some(z) = phz.as_deref_mut() {
                        add_map(&mut z.map[c], &a.phz.map[c]);
                    }
                }
            }
            *renew = Renew::Keep;
            for (x, y) in edges.iter_mut().zip(a.edges) {
                *x += y;
            }
            *off = (off.0 + a.off.0, off.1.max(a.off.1));
            got.frames += a.frames;
            got.triggered += a.triggered;
            crate::framestats::add_folds(&mut got.folds, &a.folds);
        } else {
            Counters::add(&self.counters.dropped, a.frames);
        }
        // back to the drawers, which clear it
        Mail::lock(&self.mail).keep(a);
        got
    }
}

/// Empty maps for view `v`, reusing `spare` when it has the size.
fn fresh_acc(spare: Option<Acc>, v: &View) -> Acc {
    let mut a = match spare {
        Some(mut a) if a.dims() == v.dims() => {
            a.reset(v.gen);
            a
        }
        _ => Acc::new(v.gen, v.dims()),
    };
    a.used = std::array::from_fn(|c| v.m.ch[c].on);
    a.folds = crate::framestats::folds_for(&v.m);
    a
}

/// Maps for drawer `id` to draw into with view `v`: its parked set when that was drawn with `v`, else empty ones
/// (from `spare` or the mailbox's). A parked set of an older mapping is dropped (the UI would drop it).
fn take_set(mail: &Mutex<Mail>, id: usize, spare: &mut Option<Acc>, v: &View, counters: &Counters) -> Acc {
    let mut mb = Mail::lock(mail);
    match mb.parked[id].take() {
        Some(a) if a.gen == v.gen && a.dims() == v.dims() => return a,
        Some(old) => {
            Counters::add(&counters.dropped, old.frames);
            mb.keep(old);
        }
        None => {}
    }
    let s = spare.take().or_else(|| mb.spare.pop());
    drop(mb);
    fresh_acc(s, v)
}

/// Drawer `id` after a batch, with the maps it drew into (`a`; None: it drew nothing, then its parked set is meant):
/// handed on (`publish`) when a merge came since it last did (`seen`), else parked, to draw on into with the next
/// batch. Returns maps for the next set when it has some to spare.
fn hand(mail: &Mutex<Mail>, id: usize, seen: &mut Option<u64>, a: Option<Acc>, counters: &Counters) -> Option<Acc> {
    let a = {
        let mut mb = Mail::lock(mail);
        let a = match a {
            Some(a) if a.frames == 0 => return Some(a),
            Some(a) => a,
            None => match mb.parked[id].take() {
                Some(p) if *seen != Some(mb.epoch) => p,
                p => {
                    mb.parked[id] = p;
                    return None;
                }
            },
        };
        if *seen == Some(mb.epoch) {
            // the slot is empty: the drawer took its set out to draw (`take_set`)
            mb.parked[id] = Some(a);
            return None;
        }
        *seen = Some(mb.epoch);
        a
    };
    publish(mail, a, counters)
}

/// Frames `r` of batch `b` (in `lease`) decoded into `f` and drawn into `a`.
fn draw_range(b: &Batch, lease: &Lease, r: Range<usize>, v: &View, a: &mut Acc, f: &mut Frame, counters: &Counters) {
    let Some(bytes) = lease.raw.as_ref().map(|x| x.bytes()) else { return };
    for i in r {
        let chunk = &bytes[i * b.fb..(i + 1) * b.fb];
        // without per-frame measurements a large frame is drawn from its read-out words where the board has a view of them
        // (not with the zoom, which draws the decoded frame a second time)
        if a.folds.is_empty() && v.zoom.is_none() && chunk.len() >= CODES_MIN_BYTES {
            let lay = b.codes.as_ref().and_then(|c| c(chunk, f)).filter(|l| l.stride <= CODES_MAX_STRIDE);
            let st = Stats { edges: &mut a.edges, off: &mut a.off };
            if let Some(t) = lay.and_then(|l| draw_frame_codes(&mut a.ph, &v.m, f, chunk, &l, Some(st))) {
                a.triggered += t as u64;
                a.frames += 1;
                continue;
            }
        }
        (b.decode)(chunk, f);
        let st = Stats { edges: &mut a.edges, off: &mut a.off };
        a.triggered += draw_frame(&mut a.ph, &v.m, f, Some(st)) as u64;
        if let Some((zm, ..)) = &v.zoom {
            draw_frame(&mut a.phz, zm, f, None);
        }
        if crate::framestats::fold_frame(f, &v.m, &mut a.folds) {
            Counters::add(&counters.measured, 1);
        }
        a.frames += 1;
    }
}

/// Frames `r` of batch `b` (in `lease`) decoded into `f`, their screen records as `wave::hw_record` makes them folded
/// into one run (`average::Run`) for the software Average of view `v`; None without frames. Where the record's points
/// are the linear interpolation of the samples (fewer than two a point, more than half a point a sample), they are
/// folded as they are worked out, with `wave::resample`'s expressions, without the record.
fn average_range(b: &Batch, lease: &Lease, r: Range<usize>, v: &View, f: &mut Frame) -> Option<average::Run> {
    let bytes = lease.raw.as_ref().map(|x| x.bytes())?;
    let mut run = average::Run::new(b.seq0 + r.start as u64, v.m.avg_count, v.gen);
    for i in r {
        (b.decode)(&bytes[i * b.fb..(i + 1) * b.fb], f);
        average_frame(&mut run, f, &v.m);
    }
    (run.frames > 0).then_some(run)
}

/// Frame `f`'s screen record (`wave::hw_record` under model `m`) folded into `run`; `f` comes back as it was.
fn average_frame(run: &mut average::Run, f: &mut Frame, m: &Model) {
    let n = 3600;
    let window = m.time_per_div() as f64 * HDIV as f64;
    let (t_first, dt) = (m.delay as f64 - window / 2.0, window / n as f64);
    {
        let linear = !f.roll && !f.scan && !f.envelope && dt / f.dt < 2.0 && f.dt <= 2.0 * dt;
        if linear && m.acq != AcqMode::HiRes {
            let found = crate::wave::trigger_point(f, m, n, window, t_first);
            let ti = found.unwrap_or(f.pre as f64);
            let len = std::array::from_fn(|c| if f.ch[c].len() < 2 { 0 } else { n });
            crate::wave::places(t_first, dt, f.dt, n, |u| {
                let x = |c: usize, k: usize| oos_dsp::interp::linear(&f.ch[c], ti + u[k]).unwrap_or(f32::NAN);
                run.push_with(len, x, found.is_some());
            });
            return;
        }
        // the record holds the frame (its raw window): the frame's vectors come back once it is gone
        let fr = Arc::new(std::mem::take(f));
        let rec = crate::wave::to_record(&fr, m, n, window, t_first);
        run.push(&rec.ch, rec.triggered);
        drop(rec);
        *f = Arc::try_unwrap(fr).unwrap_or_default();
    }
}

/// Hands drawn maps to `mail`, folded into what the UI has not taken yet, and returns maps to draw the next ones into.
/// The set there is taken out and added to outside the lock, on the drawer's core; meanwhile the UI finds the
/// mailbox without it (those frames come with the next merge), and a set another drawer puts in is folded too. Of two
/// sets of different mappings the older one is dropped (the UI drops it anyway).
fn publish(mail: &Mutex<Mail>, mut a: Acc, counters: &Counters) -> Option<Acc> {
    // the maps of the last set folded in (returned for the next drawing), and one more kept at the next look
    let (mut free, mut extra): (Option<Acc>, Option<Acc>) = (None, None);
    loop {
        let other = {
            let mut mb = Mail::lock(mail);
            if let Some(x) = extra.take() {
                mb.keep(x);
            }
            match mb.full.take() {
                Some(o) if o.gen == a.gen && o.dims() == a.dims() => {
                    #[cfg(test)]
                    {
                        mb.folds += 1;
                    }
                    o
                }
                o => {
                    let (keep, old) = match o {
                        Some(o) if o.gen > a.gen => (o, Some(a)),
                        o => (a, o),
                    };
                    if let Some(old) = old {
                        Counters::add(&counters.dropped, old.frames);
                        mb.keep(old);
                    }
                    mb.full = Some(keep);
                    return free.or_else(|| mb.spare.pop());
                }
            }
        };
        a.add(&other);
        extra = free.replace(other);
    }
}

/// Worker `id`: claims frames of each batch until none are left, draws them into its own maps, then hands them on or
/// parks them (`hand`), and notes what its frames cost (`Cost`).
fn work(id: usize, rx: Receiver<Arc<Batch>>, view: &Mutex<ViewState>, mail: &Mutex<Mail>, cost: &Mutex<Cost>) {
    let mut f = Frame::default();
    let (mut spare, mut seen): (Option<Acc>, Option<u64>) = (None, None);
    while let Ok(b) = rx.recv() {
        let (v, stamp) = {
            let s = lock(view);
            (s.view.clone(), s.stamp)
        };
        let Some(v) = v.filter(|_| b.stamp >= stamp) else {
            // older settings or no fast path: the frames are dropped (counted when the batch goes)
            while let Some((r, _)) = b.claim(b.frames.max(1)) {
                Counters::add(&b.counters.dropped, r.len() as u64);
            }
            continue;
        };
        // the software Average: the frames folded in runs, for the UI to apply in order (no maps)
        if v.m.acq == AcqMode::Average {
            let (t, mut drawn) = (std::time::Instant::now(), 0);
            while let Some((r, lease)) = b.claim(AVG_CLAIM) {
                drawn += r.len();
                let run = average_range(&b, &lease, r, &v, &mut f);
                Mail::lock(mail).avg.extend(run);
            }
            if drawn > 0 {
                lock(cost).note(v.gen, drawn, t.elapsed().as_nanos().min(u64::MAX as u128) as u64);
            }
            continue;
        }
        // the maps only when there is something to draw: the others took every frame already
        let (mut a, t, mut drawn) = (None, std::time::Instant::now(), 0);
        while let Some((r, lease)) = b.claim(CLAIM) {
            let a = a.get_or_insert_with(|| take_set(mail, id, &mut spare, &v, b.counters));
            drawn += r.len();
            draw_range(&b, &lease, r, &v, a, &mut f, b.counters);
        }
        // handed on or parked before the batch goes: once it is gone, its frames are in the mailbox
        if let Some(s) = hand(mail, id, &mut seen, a, b.counters) {
            spare = Some(s);
        }
        if drawn > 0 {
            lock(cost).note(v.gen, drawn, t.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        }
    }
}

/// `dst` += `src`, saturating: one channel's map. A trace covers a small part of it: the blocks of `src` without
/// counts are only read, `dst` there not at all (a fold or a clear on a small core is memory-bound: the reads and
/// writes it leaves out are its time).
/// Returns the pixels of the blocks it added.
fn add_map(dst: &mut [u16], src: &[u16]) -> u64 {
    let mut px = 0;
    for (d, s) in dst.chunks_mut(BLOCK).zip(src.chunks(BLOCK)) {
        if any_count(s) {
            for (x, y) in d.iter_mut().zip(s) {
                *x = x.saturating_add(*y);
            }
            px += d.len() as u64;
        }
    }
    px
}

/// `m` cleared: only its blocks with counts are written.
fn clear_map(m: &mut [u16]) {
    for b in m.chunks_mut(BLOCK) {
        if any_count(b) {
            b.fill(0);
        }
    }
}

/// Pixels of a block of `add_map` and `clear_map`: two cache lines.
const BLOCK: usize = 64;

fn any_count(b: &[u16]) -> bool {
    b.iter().fold(0, |a, &x| a | x) != 0
}

/// Workers where the cores are all alike (the host): two.
const WORKERS: usize = 2;

/// The workers on: the source is the hardware (`Ui::step`). The other sources (demo, replay, live, SIM) put their
/// records on the screen without the queue of hardware frames, and start no workers.
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// The workers, started the first time the hardware is the source and kept afterwards.
static PIPE: OnceLock<Pipe> = OnceLock::new();

/// The workers on or off while the scope runs (`ENABLED`). Off: they stay, idle, and forget their view, so no frame
/// of the old settings is merged when they come on again.
pub fn set_enabled(on: bool) {
    crate::board::pipe::register(workers);
    let was = ENABLED.swap(on, std::sync::atomic::Ordering::Relaxed);
    if was && !on {
        if let Some(p) = PIPE.get() {
            p.set_view(None, 0, 0);
        }
    }
}

#[cfg(test)]
thread_local! {
    /// tests: `pipe` is None on this thread, whatever another test's screen switched on (`keep_out`)
    static OUT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Keeps the calling thread out of the workers (tests) while the guard lives: an acquisition loop run by a test must not
/// feed the workers of another test's screen, which counts the records they draw. The guard puts back what was before
/// when it is dropped, a panic included.
#[cfg(test)]
pub fn keep_out() -> KeepOut {
    KeepOut(OUT.with(|o| o.replace(true)))
}

/// The guard of `keep_out`: the thread's setting before it.
#[cfg(test)]
pub struct KeepOut(bool);

#[cfg(test)]
impl Drop for KeepOut {
    fn drop(&mut self) {
        OUT.with(|o| o.set(self.0));
    }
}

/// The pipe while the source is the hardware (started on first use), else None.
pub fn pipe() -> Option<&'static Pipe> {
    #[cfg(test)]
    if OUT.with(|o| o.get()) {
        return None;
    }
    if !ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        return None;
    }
    Some(PIPE.get_or_init(|| Pipe::start(crate::placement::little_count().unwrap_or(WORKERS), true, counters())))
}

/// `pipe` as the acquisition thread sees it (`board::pipe::workers`).
fn workers() -> Option<&'static dyn crate::board::pipe::Workers> {
    pipe().map(|p| p as &dyn crate::board::pipe::Workers)
}

/// The workers for the acquisition thread (`board::pipe`): the methods above, the thread's helper state made on its
/// first use.
impl crate::board::pipe::Workers for Pipe {
    fn accepting(&self) -> bool {
        Pipe::accepting(self)
    }
    fn grant(&self, cap: u32) -> u32 {
        Pipe::grant(self, cap)
    }
    fn lent(&self) -> usize {
        Pipe::lent(self)
    }
    fn take_buffer(&self) -> Option<Box<dyn Any + Send>> {
        Pipe::take_buffer(self)
    }
    fn feed(&self, b: crate::board::pipe::Feed) {
        self.feed_batch(b.raw, (b.fb, b.first, b.frames, b.stamp, b.seq0), b.decode, b.codes);
    }
    fn help(&self, h: &mut crate::board::pipe::Helper) -> bool {
        own_helper(h).is_some_and(|h| Pipe::help(self, h))
    }
    fn help_done(&self, h: &mut crate::board::pipe::Helper) {
        if let Some(h) = own_helper(h) {
            Pipe::help_done(self, h);
        }
    }
}

/// The helper state in the acquisition thread's keeping, made the first time.
fn own_helper(h: &mut crate::board::pipe::Helper) -> Option<&mut Helper> {
    h.0.get_or_insert_with(|| Box::new(Helper::default())).downcast_mut::<Helper>()
}

/// The pipe with `n` workers for the tests of the frame loop in batch mode (the first call decides).
#[cfg(test)]
pub fn start_for_tests(n: usize) -> &'static Pipe {
    PIPE.get_or_init(|| Pipe::start(n, false, counters()))
}

/// The status line's word for the workers (`pipeline ...` of `:SYSTem:STATe?`): `Batch` while they draw the
/// hardware's batches, `Off` with another source.
pub fn status() -> &'static str {
    if ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        "Batch"
    } else {
        "Off"
    }
}

pub mod average;

#[cfg(test)]
mod tests;
