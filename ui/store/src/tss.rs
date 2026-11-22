// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the session archive of the PC program of another instrument (`.tss`; docs/tss.md says what is known of
//! it and how, oos-plus ADR 0075 why it is read). A zip that holds a `.set` which is a zip itself, with the settings
//! as a text of commands and the reference waveforms; the record of each channel lies beside the `.set` as
//! `ch<n>.wfm`. Every waveform is a `:WFM#003` file (`parse_wfm`). Here: what an archive holds (`tss_parts`), its
//! analog waveforms as traces for the reference slots (`tss_waves`, a branch of `parse_wave`), and the archive as a
//! session of OOS (`parse_tss`): the records of the channels as the record, the references in their slots, and a
//! setup text of OOS made from the few settings that are mapped.

use super::*;
use crate::zip::{base_name, looks_zip, Zip, ZipEntry};
use std::collections::{BTreeMap, HashMap};

/// The longest settings text and the longest list of reference files that are read. A settings text seen so far
/// has up to 0.4 MiB.
const TSS_TEXT_BYTES: usize = 16 << 20;
/// The samples of all waveforms of one archive together. Two archives seen hold three records of 12.5 M points.
const TSS_SAMPLES: usize = 4 * MAX_WAVEFORM_SAMPLES;

/// The commands of a settings text by their header in capitals, each with its argument as it was written.
/// The text has one command a line, `:HEADER:PATH argument`; its first line has several, parted by `;`. A quoted
/// argument can run over lines and holds `;` and line ends as they are. A header that comes twice keeps its last
/// argument, as an instrument that runs the text would.
struct Commands(HashMap<String, String>);

impl Commands {
    fn parse(text: &str) -> Commands {
        let mut map = HashMap::new();
        let (mut quoted, mut start) = (false, 0);
        let text = text.trim_start_matches('\u{feff}');
        for (at, c) in text.char_indices().chain([(text.len(), '\n')]) {
            match c {
                '"' => quoted = !quoted,
                ';' | '\n' if !quoted => {
                    let command = text[start..at].trim();
                    start = at + 1;
                    if command.starts_with(':') {
                        let (header, argument) = command.split_once(char::is_whitespace).unwrap_or((command, ""));
                        map.insert(header.to_ascii_uppercase(), argument.trim().to_string());
                    }
                }
                _ => {}
            }
        }
        Commands(map)
    }

    /// The argument of `header` as a text: without its quotes, a doubled quote inside as one, `%20` and the like as
    /// the character they stand for (a name with a space is written so), control characters left out. None when the
    /// command is not there or its text is empty.
    fn text(&self, header: &str) -> Option<String> {
        let raw = self.0.get(header)?;
        let quoted = raw.strip_prefix('"').and_then(|r| r.strip_suffix('"'));
        let inner = quoted.map_or(raw.clone(), |r| r.replace("\"\"", "\""));
        let (bytes, mut out, mut at) = (inner.as_bytes(), Vec::new(), 0);
        let digit = |at: usize| bytes.get(at).and_then(|&d| (d as char).to_digit(16));
        while at < bytes.len() {
            let hex = digit(at + 1).zip(digit(at + 2)).map(|(high, low)| (high * 16 + low) as u8);
            match (bytes[at], hex) {
                (b'%', Some(byte)) => {
                    out.push(byte);
                    at += 3;
                }
                (byte, _) => {
                    out.push(byte);
                    at += 1;
                }
            }
        }
        let text: String = String::from_utf8_lossy(&out).chars().filter(|c| !c.is_control()).collect();
        Some(text.trim().to_string()).filter(|t| !t.is_empty())
    }

    /// The argument of the first of `headers` that is there, as a number: None when none is, or it is no finite
    /// number. A header has `{n}` where the number of the channel or the reference stands.
    fn number(&self, headers: &[&str], n: usize) -> Option<f64> {
        let raw = headers.iter().find_map(|h| self.0.get(&h.replace("{n}", &n.to_string())))?;
        raw.trim().parse::<f64>().ok().filter(|x| x.is_finite())
    }

    /// The argument of the first of `headers` that is there, as a switch: `1` or `ON`, `0` or `OFF`.
    fn on(&self, headers: &[&str], n: usize) -> Option<bool> {
        let raw = headers.iter().find_map(|h| self.0.get(&h.replace("{n}", &n.to_string())))?;
        match raw.trim().to_ascii_uppercase().as_str() {
            "1" | "ON" => Some(true),
            "0" | "OFF" => Some(false),
            _ => None,
        }
    }
}

/// What of an archive was not read, counted: for the one line that tells the user.
#[derive(Default)]
pub(super) struct LeftOut {
    /// waveform files of logic lines
    digital: usize,
    /// `ch<n>_iq.wfm`: the baseband of a spectrum view
    iq: usize,
    /// waveform files that are neither the record of a channel nor a reference the archive names
    other: usize,
    /// references the archive names without their file
    missing: usize,
    /// as a session: records of channels that are not on the time axis of the first, which a record of OOS needs
    apart: usize,
    /// as a session: records of channels and references with a number OOS has no channel or slot for
    beyond: usize,
}

impl LeftOut {
    /// "not read: 2 digital and 1 IQ waveform", or nothing when all was read.
    pub(super) fn words(&self) -> String {
        let kinds = [
            (self.digital, "digital"),
            (self.iq, "IQ"),
            (self.other, "other"),
            (self.missing, "missing"),
            (self.apart, "differently timed"),
            (self.beyond, "surplus"),
        ];
        let parts: Vec<String> = kinds.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
        let Some((last, first)) = parts.split_last() else { return String::new() };
        let list = if first.is_empty() { last.clone() } else { format!("{} and {last}", first.join(", ")) };
        format!("not read: {list} waveform{}", if kinds.iter().map(|k| k.0).sum::<usize>() == 1 { "" } else { "s" })
    }
}

/// The settings and the analog waveforms of an archive, each waveform with the number of its channel or reference
/// (from 1). The label of a trace is the name the settings give it (`LABEL:NAME`), else the name of its file without
/// the suffix.
pub(super) struct Tss {
    commands: Commands,
    /// the records of the channels, in the order of their numbers
    pub channels: Vec<(usize, WaveTrace)>,
    /// the references, in the order of their numbers
    pub refs: Vec<(usize, WaveTrace)>,
    pub left: LeftOut,
}

fn has_suffix(name: &str, suffix: &str) -> bool {
    name.to_ascii_lowercase().ends_with(suffix)
}

/// Whether a `:WFM#003` file holds a logic waveform: the 32-bit field at byte 122, the kind of its data, is 6. An
/// analog one has 2 there (docs/tss.md).
fn wfm_digital(b: &[u8]) -> bool {
    b.get(2..10) == Some(b":WFM#003") && Fields::new(b, b[0] == 0x0F).i32(122) == Ok(6)
}

/// The waveform of the entry `e`, named `label` when that is given. None for a logic waveform and for a file that
/// is no WFM file and not one other waveform either (a reference can be a file of another kind); both are counted.
/// A WFM file that cannot be read is an error: the archive is damaged. `budget` is what is left of `TSS_SAMPLES`.
fn wave(
    zip: &Zip,
    e: &ZipEntry,
    label: Option<String>,
    budget: &mut usize,
    left: &mut LeftOut,
) -> Result<Option<WaveTrace>, String> {
    let name = base_name(&e.name);
    let bytes = zip.read(e, MAX_WAVEFORM_BYTES)?;
    if wfm_digital(&bytes) {
        left.digital += 1;
        return Ok(None);
    }
    let mut trace = if looks_wfm(&bytes) {
        parse_wfm(&bytes).map_err(|e| format!("{name}: {e}"))?
    } else {
        let other = crate::import::one_wave_file(&bytes, name).ok().filter(|traces| traces.len() == 1);
        match other.and_then(|mut traces| traces.pop()) {
            Some(trace) => trace,
            None => {
                left.other += 1;
                return Ok(None);
            }
        }
    };
    let over = "the session holds more samples than the import limit of 128 million";
    *budget = budget.checked_sub(trace.v.len()).ok_or(over)?;
    trace.label = label.unwrap_or_else(|| name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string());
    Ok(Some(trace))
}

/// The number `n` of a file named `ch<n>.wfm` (true) or `ch<n>_iq.wfm` (false), whatever the case of its letters.
fn channel_file(name: &str) -> Option<(usize, bool)> {
    let name = base_name(name).to_ascii_lowercase();
    let stem = name.strip_prefix("ch")?.strip_suffix(".wfm")?;
    let (digits, record) = stem.strip_suffix("_iq").map_or((stem, true), |d| (d, false));
    Some((digits.parse().ok().filter(|n| (1..=99).contains(n) && digits.len() <= 2)?, record))
}

/// Opens a session archive and reads its waveforms. The archive inside (`.set`) is opened once, and nothing deeper:
/// what is expected to be the settings text and is an archive again is refused. Entries are found by their suffix
/// and not by the name of the archive (they differ), with or without a folder in front.
pub(super) fn tss_parts(bytes: &[u8]) -> Result<Tss, String> {
    let outer = Zip::open(bytes)?;
    let no_session = || "the archive holds no session of an instrument (no settings archive .set)".to_string();
    let set = outer.entries.iter().find(|e| has_suffix(&e.name, ".set")).ok_or_else(no_session)?;
    let set = outer.read(set, MAX_WAVEFORM_BYTES)?;
    if !looks_zip(&set) {
        return Err(no_session());
    }
    let inner = Zip::open(&set)?;
    let text = inner.entries.iter().find(|e| has_suffix(&e.name, ".set")).ok_or("the session holds no settings text")?;
    let text = inner.read(text, TSS_TEXT_BYTES)?;
    if looks_zip(&text) {
        return Err("the session is an archive in an archive in an archive: that deep nothing is read".into());
    }
    let commands = Commands::parse(&String::from_utf8_lossy(&text));
    let (mut left, mut budget) = (LeftOut::default(), TSS_SAMPLES);

    // the references: which file is which reference stands in a list of `key=value` lines beside the settings text
    let mut named: BTreeMap<usize, String> = BTreeMap::new();
    for list in inner.entries.iter().filter(|e| has_suffix(&e.name, ".ini")) {
        let list = inner.read(list, TSS_TEXT_BYTES)?;
        for line in String::from_utf8_lossy(&list).lines() {
            let Some((key, file)) = line.split_once('=') else { continue };
            let key = key.trim().to_ascii_lowercase();
            let number = key.strip_prefix("recallsubsystem\\refmgr\\ref").and_then(|k| k.strip_suffix("\\sourcefilename"));
            if let Some(n) = number.and_then(|n| n.parse().ok()).filter(|n| (1..=99).contains(n)) {
                named.entry(n).or_insert_with(|| base_name(file.trim().trim_matches('"')).to_ascii_lowercase());
            }
        }
    }
    let waves: Vec<&ZipEntry> = inner.entries.iter().filter(|e| has_suffix(&e.name, ".wfm")).collect();
    left.other += waves.iter().filter(|e| !named.values().any(|file| *file == base_name(&e.name).to_ascii_lowercase())).count();
    let mut refs = Vec::new();
    for (&number, file) in &named {
        let Some(e) = inner.entries.iter().find(|e| base_name(&e.name).to_ascii_lowercase() == *file) else {
            left.missing += 1;
            continue;
        };
        let label = commands.text(&format!(":REF:REF{number}:LABEL:NAME"));
        refs.extend(wave(&inner, e, label, &mut budget, &mut left)?.map(|trace| (number, trace)));
    }
    drop(inner);
    drop(set);

    // the records of the channels, beside the settings archive
    let mut files: BTreeMap<usize, &ZipEntry> = BTreeMap::new();
    for e in outer.entries.iter().filter(|e| has_suffix(&e.name, ".wfm")) {
        match channel_file(&e.name) {
            Some((n, true)) if !files.contains_key(&n) => {
                files.insert(n, e);
            }
            Some((_, false)) => left.iq += 1,
            _ => left.other += 1,
        }
    }
    let mut channels = Vec::new();
    for (&number, e) in &files {
        let label = commands.text(&format!(":CH{number}:LABEL:NAME"));
        channels.extend(wave(&outer, e, label, &mut budget, &mut left)?.map(|trace| (number, trace)));
    }
    Ok(Tss { commands, channels, refs, left })
}

/// The analog waveforms of a session archive for the reference slots: the records of the channels in the order of
/// the channels, then the references in the order of their numbers; and what was left out, in words.
pub(super) fn tss_waves(bytes: &[u8]) -> Result<(Vec<WaveTrace>, String), String> {
    let tss = tss_parts(bytes)?;
    let note = tss.left.words();
    let traces: Vec<WaveTrace> = tss.channels.into_iter().chain(tss.refs).map(|(_, trace)| trace).collect();
    if traces.is_empty() {
        return Err(no_waveform(&note));
    }
    Ok((traces, note))
}

fn no_waveform(left_out: &str) -> String {
    match left_out {
        "" => "the session holds no waveform".into(),
        _ => format!("the session holds no waveform that is read ({left_out})"),
    }
}

// --------------------------------------------------------------------------------- the archive as a session

// The settings that are mapped (docs/tss.md), by their headers: the first that is there counts, and `{n}` is the
// number of the channel or the reference.
const CH_SHOWN: &[&str] = &[":DISPLAY:GLOBAL:CH{n}:STATE", ":SELECT:CH{n}"];
const CH_SCALE: &[&str] = &[":CH{n}:SCALE", ":DISPLAY:WAVEVIEW1:CH{n}:VERTICAL:SCALE"];
const CH_POSITION: &[&str] = &[":CH{n}:POSITION", ":DISPLAY:WAVEVIEW1:CH{n}:VERTICAL:POSITION"];
const CH_OFFSET: &[&str] = &[":CH{n}:OFFSET"];
const REF_SHOWN: &[&str] = &[":DISPLAY:GLOBAL:REF{n}:STATE"];
const REF_SCALE: &[&str] = &[":DISPLAY:WAVEVIEW1:REF:REF{n}:VERTICAL:SCALE"];
const REF_POSITION: &[&str] = &[":DISPLAY:WAVEVIEW1:REF:REF{n}:VERTICAL:POSITION"];

/// Volts a division and the volts added before display, for a waveform. From the settings when they give the scale:
/// there a sample `v` stands `(v - offset) / scale + position` divisions above the middle, on OOS
/// `(v + added) / scale`. Without a scale from the samples: they take at most six of the eight divisions, around a
/// level that is a whole number of divisions, as a waveform loaded into a slot is fitted.
fn vertical(scale: Option<f64>, position: Option<f64>, offset: Option<f64>, v: &[f32]) -> (f64, f64) {
    if let Some(scale) = scale.filter(|s| *s > 0.0) {
        return (scale, position.unwrap_or(0.0) * scale - offset.unwrap_or(0.0));
    }
    let ends = (f32::INFINITY, f32::NEG_INFINITY);
    let (lo, hi) = v.iter().filter(|x| x.is_finite()).fold(ends, |(lo, hi), &x| (lo.min(x), hi.max(x)));
    if !(lo <= hi) {
        return (1.0, 0.0);
    }
    let want = ((hi - lo) as f64 / 6.0).max(1e-6);
    let step = VSCALES.iter().map(|&s| s as f64).find(|&s| s >= want * 0.999).unwrap_or(want);
    (step, -step * ((hi + lo) as f64 / 2.0 / step).round())
}

/// `value` as a step of `ladder` and a factor on it between `low` and `high` (1 on the step itself): the last step
/// at or under the value for a factor from 1 up (volts a division), the first at or over it for one up to 1 (time).
fn step_and_fine(ladder: &[f32], value: f64, low: f32, high: f32) -> (usize, f32) {
    let step = if low < 1.0 {
        ladder.iter().position(|&s| s as f64 >= value * 0.999).unwrap_or(ladder.len() - 1)
    } else {
        ladder.iter().rposition(|&s| s as f64 <= value * 1.001).unwrap_or(0)
    };
    let fine = (value / ladder[step] as f64) as f32;
    (step, if (fine - 1.0).abs() < 1e-3 { 1.0 } else { fine.clamp(low, high) })
}

/// The window of the screen the settings give: the time a division and the time in its middle, the trigger at 0.
/// Ten divisions of `:HORIZONTAL:SCALE` with the trigger at `:HORIZONTAL:POSITION` percent of them; with the delay
/// on (`:HORIZONTAL:DELAY:MODE`, assumed: no session seen has it on) the middle is `:HORIZONTAL:DELAY:TIME` after
/// the trigger.
fn window(c: &Commands) -> Option<(f64, f64)> {
    let per_div = c.number(&[":HORIZONTAL:SCALE"], 0).filter(|s| *s > 0.0)?;
    let middle = match c.on(&[":HORIZONTAL:DELAY:MODE"], 0) {
        Some(true) => c.number(&[":HORIZONTAL:DELAY:TIME"], 0)?,
        _ => (50.0 - c.number(&[":HORIZONTAL:POSITION"], 0)?) / 100.0 * HDIV as f64 * per_div,
    };
    Some((per_div, middle))
}

/// Sets the time base for `n` samples from `t0` on: `window` when it lies in them, else all of them on the screen.
fn time_base(m: &mut Model, window: Option<(f64, f64)>, t0: f64, dt: f64, n: usize) {
    let span = n as f64 * dt;
    let inside = |&(per_div, middle): &(f64, f64)| {
        let half = per_div * HDIV as f64 / 2.0;
        middle - half >= t0 - 0.02 * half && middle + half <= t0 + span + 0.02 * half
    };
    let (per_div, middle) = window.filter(inside).unwrap_or((span / HDIV as f64, t0 + span / 2.0));
    (m.tscale, m.tfine) = step_and_fine(&tscales(), per_div, 0.1, 1.0);
    m.delay = middle as f32;
}

/// What a load as a session says it took and left, in front of the waveforms it did not read.
const TSS_MAPPED: &str = "waveforms, scales and time base, without its trigger, buses, measurements and maths";

/// A session archive as a session of OOS (`Session`, what `parse_session` gives of OOS's own file), so that the
/// load of a session applies it down its one path. The records of the channels 1 to 4 become the record, at their
/// full length: those on the time axis of the first (same start, step and length), the others are left out and
/// counted. The references go into the slots of their numbers. The setup is the default one with what is mapped
/// (docs/tss.md): which channels are shown, their volts a division, offset and name, the time base, and the look of
/// each reference; a value the settings do not give is taken from the samples (`vertical`, `time_base`). A session
/// without a record fills the screen with its first reference (`offline_ref`).
pub fn parse_tss<R: Samples>(bytes: &[u8], slots: usize) -> Result<Session<R>, String> {
    if bytes.len() > MAX_WAVEFORM_BYTES {
        return Err("the session exceeds the 128 MiB import limit".into());
    }
    let Tss { commands, channels, refs, mut left } = tss_parts(bytes)?;
    let mut m = Model::default();
    m.ch.iter_mut().for_each(|ch| ch.on = false);
    let mut record: Option<(f64, f64, usize, [Vec<f32>; NCH])> = None;
    for (n, trace) in channels {
        let axis = (trace.t0, trace.dt, trace.v.len());
        if n > NCH {
            left.beyond += 1;
            continue;
        }
        if record.as_ref().is_some_and(|(t0, dt, len, _)| axis != (*t0, *dt, *len)) {
            left.apart += 1;
            continue;
        }
        let (scale, position) = (commands.number(CH_SCALE, n), commands.number(CH_POSITION, n));
        let (vdiv, added) = vertical(scale, position, commands.number(CH_OFFSET, n), &trace.v);
        let ch = &mut m.ch[n - 1];
        ch.on = commands.on(CH_SHOWN, n) != Some(false);
        (ch.vscale, ch.vfine) = step_and_fine(&VSCALES, vdiv, 1.0, 2.5);
        (ch.offset, ch.probe) = (added as f32, 1.0);
        // (the name of the trace is that of its file when the settings give none: no name of a channel)
        let name = commands.text(&format!(":CH{n}:LABEL:NAME")).unwrap_or_default();
        m.ch_label[n - 1] = name.chars().take(10).collect();
        record.get_or_insert_with(|| (axis.0, axis.1, axis.2, Default::default())).3[n - 1] = trace.v;
    }
    let mut looks: Vec<Option<RefLook>> = vec![None; slots];
    let mut held = Vec::new();
    for (n, trace) in refs {
        if n > slots {
            left.beyond += 1;
            continue;
        }
        let (vdiv, added) = vertical(commands.number(REF_SCALE, n), commands.number(REF_POSITION, n), None, &trace.v);
        let (on, vdiv, offset) = (Some(commands.on(REF_SHOWN, n) != Some(false)), Some(vdiv as f32), Some(added as f32));
        looks[n - 1] = Some(RefLook { on, vdiv, offset, color: None, label: Some(trace.label) });
        held.push(SessionRef { slot: n - 1, src: 0, t0: trace.t0, dt: trace.dt, v: trace.v });
    }
    let words = left.words();
    // the time base: of the record; without one of the first reference, which then fills the screen
    let offline_ref = match (&record, held.first()) {
        (Some((t0, dt, n, _)), _) => {
            time_base(&mut m, window(&commands), *t0, *dt, *n);
            None
        }
        (None, Some(first)) => {
            time_base(&mut m, None, first.t0, first.dt, first.v.len());
            Some(first.slot)
        }
        (None, None) => return Err(no_waveform(&words)),
    };
    // the knobs and the trigger on a channel that is shown, when there is one
    if let Some(first) = (0..NCH).find(|&c| m.ch[c].on) {
        (m.active, m.trig_src) = (first, first);
    }
    Ok(Session {
        setup: setup_text(&m) + &ref_setup_text(&looks),
        record: record.map(|(t0, dt, _, ch)| R::from_file(t0, dt, ch)),
        refs: held,
        offline_ref,
        note: if words.is_empty() { TSS_MAPPED.into() } else { format!("{TSS_MAPPED}; {words}") },
    })
}
