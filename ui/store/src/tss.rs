// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the session archive of the PC program of another instrument (`.tss`; docs/tss.md says what is known of
//! it and how, oos-plus ADR 0075 why it is read). A zip that holds a `.set` which is a zip itself, with the settings
//! as a text of commands and the reference waveforms; the record of each channel lies beside the `.set` as
//! `ch<n>.wfm`. Every waveform is a `:WFM#003` file (`parse_wfm`). Here: what an archive holds (`tss_parts`), and its
//! analog waveforms as traces for the reference slots (`tss_waves`, a branch of `parse_wave`).

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
}

impl LeftOut {
    /// "not read: 2 digital and 1 IQ waveform", or nothing when all was read.
    pub(super) fn words(&self) -> String {
        let kinds = [(self.digital, "digital"), (self.iq, "IQ"), (self.other, "other"), (self.missing, "missing")];
        let parts: Vec<String> = kinds.iter().filter(|(n, _)| *n > 0).map(|(n, what)| format!("{n} {what}")).collect();
        let Some((last, first)) = parts.split_last() else { return String::new() };
        let list = if first.is_empty() { last.clone() } else { format!("{} and {last}", first.join(", ")) };
        format!("not read: {list} waveform{}", if kinds.iter().map(|k| k.0).sum::<usize>() == 1 { "" } else { "s" })
    }
}

/// The analog waveforms of an archive. The label of a trace is the name the settings give it (`LABEL:NAME`), else
/// the name of its file without the suffix.
pub(super) struct Tss {
    /// the records of the channels, in the order of their numbers
    pub channels: Vec<WaveTrace>,
    /// the references, in the order of their numbers
    pub refs: Vec<WaveTrace>,
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
        refs.extend(wave(&inner, e, label, &mut budget, &mut left)?);
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
        channels.extend(wave(&outer, e, label, &mut budget, &mut left)?);
    }
    Ok(Tss { channels, refs, left })
}

/// The analog waveforms of a session archive for the reference slots: the records of the channels in the order of
/// the channels, then the references in the order of their numbers; and what was left out, in words.
pub(super) fn tss_waves(bytes: &[u8]) -> Result<(Vec<WaveTrace>, String), String> {
    let tss = tss_parts(bytes)?;
    let note = tss.left.words();
    let traces: Vec<WaveTrace> = tss.channels.into_iter().chain(tss.refs).collect();
    match (traces.is_empty(), note.is_empty()) {
        (false, _) => Ok((traces, note)),
        (true, true) => Err("the session holds no waveform".into()),
        (true, false) => Err(format!("the session holds no waveform that is read ({note})")),
    }
}
