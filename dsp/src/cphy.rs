// SPDX-License-Identifier: GPL-2.0-or-later
//! MIPI C-PHY six-wirestate, single-trio electrical symbols and 16b/7s words, with the low-power states around them.
//!
//! Mapper: MIPI DevCon 2016, "Implementing MIPI C-PHY", p12. Symbol/state
//! rules and the independent 0224100 -> 7290 example: MIPI "Introduction from
//! Basic Theory to Practical Implementation", pp14-16. This is not base-five
//! decoding, eighteen-wirestate mode or lane merging. The packet in the words of a
//! burst is read as far as a capture shows it (`packets`).
//!
//! No level is a fixed voltage. Seen in a capture of a real lane (1.25 GS/s, 25 samples a symbol): the low-power
//! levels 1.16 V apart and both away from 0 V, the three high-speed levels at a quarter, a half and three quarters
//! of that swing (the specification has them near the low level: not seen here, assumed), the three lines 20 mV
//! apart at the same level, and noise of 40 mV peak to peak. So the levels are taken from the record (`levels`).
//! Seen in the same capture: the first symbol of a word on the wire is its least significant (the checksums of 64
//! packet headers and 43 payloads fit this way only), and the sync word stands a second time inside a burst,
//! between the two copies of a packet header.
use crate::decode::{Kind, Packet};

const SYNC: [u8; 7] = [3, 4, 4, 4, 4, 4, 3];
const LIMIT: usize = 250_000;
const LP: [&str; 8] = ["!LP000", "!LP001", "!LP010", "!LP011", "!LP100", "!LP101", "!LP110", "!LP111"];
/// The share of the spread of a high-speed state each of its two gaps has at least. It is half when the middle
/// level is in the middle. Halfway from one state to the next, when all three lines move, the levels are evenly
/// apart too and look like a state, with gaps of a quarter of the spread: that is none.
const EVEN: f32 = 0.35;

const fn regions() -> [(u8, u32); 28] {
    let mut out = [(0, 0); 28];
    let mut i = 0;
    while i < 7 { out[i + 1] = (1 << i, 0x4000 + i as u32 * 0x1000); i += 1; }
    let mut index = 0;
    let mut lo = 0;
    while lo < 5 {
        let mut hi = lo + 1;
        while hi < 7 {
            out[index + 8] = ((1 << lo) | (1 << hi), 0xb000 + index as u32 * 0x400);
            index += 1; hi += 1;
        }
        lo += 1;
    }
    out
}
const REGIONS: [(u8, u32); 28] = regions();

/// The word of seven symbols, the most significant first: the order the published example is written in (assumed;
/// on the wire it is the other way round, see [`decode`]).
pub fn word(symbols: &[u8; 7]) -> Option<u16> {
    if symbols.iter().any(|&s| s > 4) { return None; }
    let flip = symbols.iter().enumerate().fold(0u8, |mask, (i, &s)| mask | if s == 4 { 1 << (6 - i) } else { 0 });
    let base = REGIONS.iter().find(|r| r.0 == flip)?.1;
    let payload = symbols.iter().filter(|&&s| s != 4).fold(0u32, |v, &s| (v << 2) | s as u32);
    u16::try_from(base + payload).ok()
}

/// The seven symbols of a word, the most significant first ([`word`]); a transmitter sends the last one first.
pub fn symbols_for_word(value: u16) -> [u8; 7] {
    let &(flip, base) = REGIONS.iter().rev().find(|r| r.1 <= value as u32).unwrap();
    let mut payload = value as u32 - base;
    let mut symbols = [0; 7];
    for i in (0..7).rev() {
        if flip & (1 << (6 - i)) != 0 { symbols[i] = 4; }
        else { symbols[i] = (payload & 3) as u8; payload >>= 2; }
    }
    symbols
}

/// State IDs: +x,+y,+z,-x,-y,-z. Only used to build deterministic test signals.
pub fn wire_values(state: u8) -> [f32; 3] {
    match state { 0 => [0.4, 0.0, 0.2], 1 => [0.2, 0.4, 0.0], 2 => [0.0, 0.2, 0.4],
        3 => [0.0, 0.4, 0.2], 4 => [0.2, 0.0, 0.4], _ => [0.4, 0.2, 0.0] }
}

pub fn states_for_symbols(initial: u8, symbols: &[u8]) -> Vec<u8> {
    let mut state = initial % 6;
    let mut states = vec![state];
    for &s in symbols {
        let axis = state % 3;
        let sign = state / 3;
        state = if s == 4 { axis + (1 - sign) * 3 }
            else { (axis + if s & 2 != 0 { 1 } else { 2 }) % 3 + (sign ^ (s & 1)) * 3 };
        states.push(state);
    }
    states
}

fn symbol(previous: u8, next: u8) -> Option<u8> {
    if previous == next { return None; }
    if previous % 3 == next % 3 { return Some(4); }
    let rotation = u8::from((next % 3 + 3 - previous % 3) % 3 == 1);
    Some(rotation * 2 + u8::from(previous / 3 != next / 3))
}

/// What a record says about its levels.
#[derive(Clone, Copy)]
struct Levels {
    /// the smallest gap between two neighbouring levels of a high-speed state; infinite: the record has none
    gap: f32,
    /// the largest spread from the lowest to the highest level of a high-speed state
    spread: f32,
    /// a line under `low` is at the low level of the low-power states, a line over `high` at their high level
    low: f32,
    high: f32,
}

/// The smaller of the two gaps between neighbouring levels of the three lines, and their spread. Not a number
/// when a sample is none.
fn gaps(a: f32, b: f32, c: f32) -> (f32, f32) {
    let (lo, hi) = (a.min(b).min(c), a.max(b).max(c));
    let mid = a + b + c - lo - hi;
    ((mid - lo).min(hi - mid), hi - lo)
}

/// The levels of a record. `None`: its lines do not move by four times `floor`, the smallest gap between two levels
/// that is taken for more than noise.
///
/// A high-speed state has the three lines at three levels evenly apart, a low-power state has two lines at least at
/// the same level: that tells them apart whatever the voltages are. The spread of the high-speed states is the one
/// most samples with evenly spaced levels have, each weighted with its square: noise between lines at one level
/// gives many such samples of little spread, a single burst few of a large one. A sample is then a high-speed state
/// when its gaps fit that spread, which the noise of a low-power state does not, nor two lines of a low-power
/// transition that happen to be evenly apart (their spread is nearer that of the low-power levels).
///
/// The low-power levels are the lowest and the highest of the record, and a line counts as at one of them within a
/// quarter of the swing. Where a record has no low-power state the middle level of its high-speed states is at
/// neither, so none is found. Lines on their way from one wire state to the next can all be within that quarter
/// for a sample or two: a low-power state is one that lasts ([`decode`]).
fn levels(a: &[f32], b: &[f32], c: &[f32], floor: f32) -> Option<Levels> {
    let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
    for &v in a.iter().chain(b).chain(c) {
        if v.is_finite() { lo = lo.min(v); hi = hi.max(v); }
    }
    let swing = hi - lo;
    if !(swing >= 4.0 * floor) || !swing.is_finite() { return None; }
    // per bin of spread: samples, the sum of their spread, the sum of its square; an empty bin at either end
    const BINS: usize = 64;
    let mut bins = [[0f64; 3]; BINS + 2];
    for i in 0..a.len() {
        let (gap, spread) = gaps(a[i], b[i], c[i]);
        if gap >= floor && gap >= EVEN * spread && spread <= swing {
            let bin = &mut bins[1 + ((spread / swing * BINS as f32) as usize).min(BINS - 1)];
            bin[0] += 1.0; bin[1] += spread as f64; bin[2] += spread as f64 * spread as f64;
        }
    }
    // a level with noise falls into two bins: the three bins with the most weight
    let window = |k: usize, field: usize| bins[k - 1][field] + bins[k][field] + bins[k + 1][field];
    let best = (1..=BINS).fold(1, |best, k| if window(k, 2) > window(best, 2) { k } else { best });
    let spread = if window(best, 0) > 0.0 { (window(best, 1) / window(best, 0)) as f32 } else { 0.0 };
    let gap = if spread > 0.0 { floor.max(EVEN * spread) } else { f32::INFINITY };
    Some(Levels { gap, spread: 1.5 * spread, low: lo + swing / 4.0, high: hi - swing / 4.0 })
}

/// 0..5: a high-speed wire state. 8..15: a low-power state, 8 + the lines A, B, C as bits. 255: neither (a line on
/// its way, or no sample).
fn state(a: f32, b: f32, c: f32, l: &Levels) -> u8 {
    if !a.is_finite() || !b.is_finite() || !c.is_finite() { return 255; }
    let (gap, spread) = gaps(a, b, c);
    if gap >= l.gap && spread <= l.spread {
        return match ((a > b) as u8) * 4 + ((b > c) as u8) * 2 + (c > a) as u8 {
            4 => 0, 2 => 1, 1 => 2, 3 => 3, 5 => 4, 6 => 5, _ => 255,
        };
    }
    let mut bits = 0;
    for v in [a, b, c] {
        if v > l.high { bits = bits * 2 + 1; } else if v < l.low { bits *= 2; } else { return 255; }
    }
    8 + bits
}

fn packet(kind: Kind, value: u32, start: usize, end: usize, ok: bool) -> Packet {
    Packet { kind, value, t0: start as f64, t1: end as f64, value2: None, ok }
}
#[derive(Clone, Copy)]
struct Run { state: u8, start: usize, end: usize }

/// The record as runs of one state. `None`: more than a million of them.
fn runs(a: &[f32], b: &[f32], c: &[f32], l: &Levels) -> Option<Vec<Run>> {
    let mut runs: Vec<Run> = Vec::new();
    for i in 0..a.len() {
        let s = state(a[i], b[i], c[i], l);
        if let Some(last) = runs.last_mut().filter(|last| last.state == s) { last.end = i + 1; }
        else {
            if runs.len() >= 1_000_000 { return None; }
            runs.push(Run { state: s, start: i, end: i + 1 });
        }
    }
    Some(runs)
}

/// The symbol time in samples as the record has it: every symbol changes the wire state, so it is the usual distance
/// from where one high-speed state begins to where the next does, the next being no further away than the state is
/// long. `None`: fewer than three states in a row.
fn symbol_time(runs: &[Run]) -> Option<f64> {
    let mut steps: Vec<usize> = Vec::new();
    let mut last: Option<Run> = None;
    for &run in runs.iter().filter(|run| run.state < 6) {
        if let Some(l) = last.filter(|l| run.start - l.end <= l.end - l.start) { steps.push(run.start - l.start); }
        last = Some(run);
    }
    if steps.len() < 2 { return None; }
    let middle = steps.len() / 2;
    Some(*steps.select_nth_unstable(middle).1 as f64)
}

/// What is known of the burst the decoder is in.
#[derive(Default)]
struct Burst {
    /// the last high-speed state that held
    previous: Option<Run>,
    /// the state the burst began with
    head: Option<Run>,
    /// the last seven symbols: symbol, start, end, and how many symbols 3 stood right before it
    recent: Vec<(u8, usize, usize, usize)>,
    /// the symbols of the word in work: symbol, start, end
    group: Vec<(u8, usize, usize)>,
    threes: usize,
    /// a sync word said where the words begin
    aligned: bool,
    /// the last packet is the post, and symbols 4 make it longer
    posted: bool,
    /// sync words so far
    syncs: u32,
}

/// Returns packet times as native sample indices, like the other DSP decoders.
///
/// `rate` is the symbol rate where it is known, else the symbol time is taken from the record. `min_gap` is the
/// smallest gap between two levels that is more than noise. The packets:
/// - `Field("!LPabc")`: a low-power state, the lines A, B, C at the high (1) or low (0) level, for as long as a
///   symbol of the record at least (two samples where it has no burst);
/// - `Field("PRE")`: the preamble before a sync word, at least seven symbols 3; the value is their number;
/// - `Field("SYNC")`: the sync word 3444443; the value counts them within the burst from 0, so that 0 is the start
///   of the burst and the others stand between the two copies of a packet header;
/// - `Word`: seven symbols after the sync word as 16 bits, the first symbol the least significant;
///   `Field("!CODE")` where seven symbols are no word;
/// - `Stop`: the post, seven symbols 4 and all that follow them;
/// - `Field("SYM")`: a symbol outside a word. Unaligned mid-burst captures expose electrical symbols rather than
///   guessed words.
///
/// `Field("!SOURCE")` alone: the lines do not move, or show no state. `Field("!SAMPLE")` alone: fewer than 2.5
/// samples a symbol.
pub fn decode(a: &[f32], b: &[f32], c: &[f32], dt: f64, rate: Option<f64>, min_gap: f32) -> Vec<Packet> {
    let n = a.len().min(b.len()).min(c.len());
    let alone = |label: &'static str, end: usize| vec![packet(Kind::Field(label), 0, 0, end, false)];
    if n < 8 || !dt.is_finite() || dt <= 0.0 { return alone("!SOURCE", n); }
    let (a, b, c) = (&a[..n], &b[..n], &c[..n]);
    let floor = if min_gap.is_finite() { min_gap.abs().max(1e-6) } else { 0.02 };
    let Some(mut levels) = levels(a, b, c, floor) else { return alone("!SOURCE", n); };
    let Some(mut states) = runs(a, b, c, &levels) else { return alone("!LIMIT", n); };
    // high-speed states the record has too few samples for are none: noise at a level, or a rate out of reach
    let measured = symbol_time(&states);
    let hs = measured.is_some_and(|ui| ui >= 2.5);
    let ui = rate.filter(|r| r.is_finite() && *r > 0.0).map(|r| 1.0 / r / dt).or(measured.filter(|_| hs));
    if ui.is_some_and(|ui| ui < 2.5) { return alone("!SAMPLE", n); }
    if !hs && levels.gap.is_finite() {
        levels.gap = f32::INFINITY;
        let Some(again) = runs(a, b, c, &levels) else { return alone("!LIMIT", n); };
        states = again;
    }
    let ui = ui.unwrap_or(2.0);
    let stable = (ui * 0.35).ceil().max(1.0) as usize;
    // a low-power state lasts longer than a symbol of the record, whatever rate is set
    let held = (measured.filter(|_| hs).unwrap_or(2.0).ceil() as usize).max(2);
    let mut out: Vec<Packet> = Vec::new();
    let mut burst = Burst::default();
    for run in states {
        if out.len() >= LIMIT - 1 { out.push(packet(Kind::Field("!LIMIT"), 0, run.start, n, false)); break; }
        let length = run.end - run.start;
        if (8..16).contains(&run.state) {
            if length < held { continue; }
            // the same state again after a few samples that were none is still that state
            let label = Kind::Field(LP[(run.state - 8) as usize]);
            match out.last_mut().filter(|p| p.kind == label && run.start as f64 - p.t1 < held as f64) {
                Some(p) => p.t1 = run.end as f64,
                None => out.push(packet(label, 0, run.start, run.end, true)),
            }
            burst = Burst::default();
            continue;
        }
        if !hs || run.state >= 6 || length < stable { continue; }
        let Some(last) = burst.previous.replace(run) else { burst.head = Some(run); continue; };
        if run.start.saturating_sub(last.end) as f64 > ui * 0.75 {
            // a hole: what follows is a burst of its own, and where its words begin is not known
            burst = Burst { previous: Some(run), head: Some(run), ..Burst::default() };
            continue;
        }
        let Some(s) = symbol(last.state, run.state) else { continue; };
        let start = last.end;
        burst.recent.push((s, start, run.end, burst.threes));
        if burst.recent.len() > 7 { burst.recent.remove(0); }
        burst.threes = if s == 3 { burst.threes + 1 } else { 0 };
        if burst.posted {
            if s == 4 {
                if let Some(p) = out.last_mut() { p.t1 = run.end as f64; }
                continue;
            }
            burst.posted = false;
        }
        let seven = burst.recent.len() == 7;
        let (first, before) = (burst.recent[0].1, burst.recent[0].3);
        let listed = |p: &Packet, from: usize| p.kind == Kind::Field("SYM") && p.t0 >= from as f64;
        if !burst.aligned && seven && burst.recent.iter().map(|x| x.0).eq(SYNC) && before >= 7 {
            // the symbols listed so far give way: those of the sync word to it, the threes before it to the preamble
            while out.last().is_some_and(|p| listed(p, first)) { out.pop(); }
            let (mut count, mut from) = (0, first);
            while let Some(p) = out.last().filter(|p| count < before && listed(p, 0) && p.value == 3) {
                (count, from) = (count + 1, p.t0 as usize);
                out.pop();
            }
            // with the state the burst began in, where the preamble is whole
            let from = burst.head.filter(|h| h.end == from).map_or(from, |h| h.start);
            out.push(packet(Kind::Field("PRE"), count as u32, from, first, true));
            out.push(packet(Kind::Field("SYNC"), 0, first, run.end, true));
            burst.aligned = true; burst.syncs = 1; burst.group.clear();
            continue;
        }
        if seven && burst.recent.iter().all(|x| x.0 == 4) {
            // no word has seven symbols 4 and no two words have them between them: the post, from where the last
            // word ended
            let from = if burst.aligned { burst.group.first().map_or(start, |g| g.1) } else { first };
            while out.last().is_some_and(|p| listed(p, from)) { out.pop(); }
            out.push(packet(Kind::Stop, 0, from, run.end, true));
            burst.aligned = false; burst.posted = true; burst.group.clear();
            continue;
        }
        if !burst.aligned {
            out.push(packet(Kind::Field("SYM"), s as u32, start, run.end, true));
            continue;
        }
        burst.group.push((s, start, run.end));
        if burst.group.len() == 7 {
            let from = burst.group[0].1;
            // the first symbol on the wire is the least significant of the word
            let symbols: [u8; 7] = std::array::from_fn(|i| burst.group[6 - i].0);
            if symbols == SYNC {
                out.push(packet(Kind::Field("SYNC"), burst.syncs, from, run.end, true));
                burst.syncs += 1;
            } else {
                let value = word(&symbols);
                out.push(packet(if value.is_some() { Kind::Word { cmd: false } } else { Kind::Field("!CODE") },
                    value.map_or(0, u32::from), from, run.end, value.is_some()));
            }
            burst.group.clear();
        }
    }
    if out.is_empty() { return alone(if measured.is_some() { "!SAMPLE" } else { "!SOURCE" }, n); }
    low_power_data(packets(out))
}

/// The escape command for low-power data, its eight bits with the first one sent as the least significant.
pub const LPDT: u32 = 0x87;

/// Low-power data in the low-power states, as a capture of a real lane has it between its bursts. From the stop
/// state (all lines high) the lane goes through A high, all low, C high, all low: the escape entry. Then a bit is
/// one line high (A: a one, C: a zero) and all lines low after it. The first eight bits are the command, the bits
/// after them bytes, each with its first bit as the least significant (the capture's command is 11100001 in the
/// order sent, which its instrument lists as 87h).
///
/// The states of the entry stay listed. The bits become `Field("ESC")` (the command) and `Data` (a byte); bits
/// that do not fill a byte, and the states the lane leaves the mode with, stay states.
fn low_power_data(out: Vec<Packet>) -> Vec<Packet> {
    let lp = |p: Option<&Packet>, name: &'static str| p.is_some_and(|p| p.kind == Kind::Field(name));
    const ENTRY: [&str; 5] = ["!LP111", "!LP100", "!LP000", "!LP001", "!LP000"];
    let mut done = Vec::with_capacity(out.len());
    let mut i = 0;
    while i < out.len() {
        if !(0..5).all(|k| lp(out.get(i + k), ENTRY[k])) {
            done.push(out[i].clone());
            i += 1;
            continue;
        }
        done.extend(out[i..i + 5].iter().cloned());
        i += 5;
        // a bit: one line high, then all low
        let bit = |i: usize| -> Option<bool> {
            let one = lp(out.get(i), "!LP100");
            ((one || lp(out.get(i), "!LP001")) && lp(out.get(i + 1), "!LP000")).then_some(one)
        };
        let mut first = true;
        while let Some(bits) = (0..8).map(|k| bit(i + 2 * k)).collect::<Option<Vec<bool>>>() {
            let value = bits.iter().enumerate().fold(0u32, |v, (k, &b)| v | (b as u32) << k);
            let kind = if first { Kind::Field("ESC") } else { Kind::Data };
            done.push(Packet { kind, value, value2: None, t0: out[i].t0, t1: out[i + 15].t1, ok: true });
            first = false;
            i += 16;
        }
    }
    done
}

/// The checksum of a packet header and of a payload: x^16 + x^12 + x^5 + 1, least significant bit first, seed FFFF.
pub fn crc16(bytes: impl IntoIterator<Item = u8>) -> u16 {
    bytes.into_iter().fold(0xffff, |crc, b| {
        (0..8).fold(crc ^ b as u16, |c, _| if c & 1 != 0 { (c >> 1) ^ 0x8408 } else { c >> 1 })
    })
}

/// The packet in the words of a burst, where they are one: a camera or display packet over C-PHY, as a capture of a
/// real lane has it in each of its 64 bursts. Three words of header (the data identifier in the high byte of the
/// first, the word count, the checksum of those four bytes), a sync word, the same three words again, and for a long
/// packet the payload, two bytes a word with the low one first and a fill byte where the count is odd, and a
/// checksum over all those bytes. A short packet (data type under 10h) has two data bytes in place of the count.
///
/// A burst is taken for a packet when the checksum of its header fits; else its words stay words. The fields:
/// `Field("DT")` (the data type; value2 = the virtual channel, the two top bits of the identifier), `Field("WC")`,
/// `Crc` (ok: it fits), `Field("COPY")` for the second header (ok: the same three words), `Data` for each byte of
/// the payload or of a short packet, `Crc` for the payload. A payload whose words are not as many as the count
/// says stays words. The low byte of the first word is not read (seen as 00; assumed reserved).
fn packets(out: Vec<Packet>) -> Vec<Packet> {
    let word = |p: Option<&Packet>| p.filter(|p| matches!(p.kind, Kind::Word { .. })).map(|p| p.value as u16);
    let is_sync = |p: &Packet, first: bool| p.kind == Kind::Field("SYNC") && (p.value == 0) == first;
    let sync = |p: Option<&Packet>, first: bool| p.is_some_and(|p| is_sync(p, first));
    let bytes_of = |w: u16| [w as u8, (w >> 8) as u8];
    let field = |kind: Kind, value: u32, value2: Option<u32>, t: (f64, f64), ok: bool| {
        Packet { kind, value, value2, t0: t.0, t1: t.1, ok }
    };
    let halves = |p: &Packet| {
        let middle = 0.5 * (p.t0 + p.t1);
        let [low, high] = bytes_of(p.value as u16);
        let byte = |v: u8, t: (f64, f64)| field(Kind::Data, v as u32, None, t, true);
        [byte(low, (p.t0, middle)), byte(high, (middle, p.t1))]
    };
    // the three words after the field at `i`
    let three = |i: usize| -> Option<[u16; 3]> {
        Some([word(out.get(i + 1))?, word(out.get(i + 2))?, word(out.get(i + 3))?])
    };
    let fits = |h: &[u16; 3]| crc16(bytes_of(h[0]).into_iter().chain(bytes_of(h[1]))) == h[2];
    let mut done = Vec::with_capacity(out.len());
    let mut i = 0;
    while i < out.len() {
        let Some(h) = three(i).filter(|h| sync(out.get(i), true) && fits(h)) else {
            done.push(out[i].clone());
            i += 1;
            continue;
        };
        let (id, short) = ((h[0] >> 8) as u32, (h[0] >> 8) & 0x3f < 0x10);
        let (w1, w2, w3) = (&out[i + 1], &out[i + 2], &out[i + 3]);
        done.push(out[i].clone());
        done.push(field(Kind::Field("DT"), id & 0x3f, Some(id >> 6), (w1.t0, w1.t1), true));
        if short {
            done.extend(halves(w2));
        } else {
            done.push(field(Kind::Field("WC"), h[1] as u32, None, (w2.t0, w2.t1), true));
        }
        done.push(field(Kind::Crc, h[2] as u32, None, (w3.t0, w3.t1), true));
        i += 4;
        // the second header, after its sync word
        if let Some(copy) = three(i).filter(|_| sync(out.get(i), false)) {
            done.push(out[i].clone());
            done.push(field(Kind::Field("COPY"), 0, None, (out[i + 1].t0, out[i + 3].t1), copy == h));
            i += 4;
        }
        // the payload of a long packet: as many words as the count says, and the checksum
        let count = (h[1] as usize).div_ceil(2);
        let payload: Vec<u16> = (0..=count).map_while(|k| word(out.get(i + k))).collect();
        if !short && payload.len() == count + 1 && word(out.get(i + count + 1)).is_none() {
            for (k, p) in out[i..i + count].iter().enumerate() {
                // (the fill byte of an odd count is no data)
                let keep = if 2 * k + 1 < h[1] as usize { 2 } else { 1 };
                done.extend(halves(p).into_iter().take(keep));
            }
            let all = payload[..count].iter().flat_map(|&w| bytes_of(w));
            let last = &out[i + count];
            done.push(field(Kind::Crc, last.value, None, (last.t0, last.t1), crc16(all) == payload[count]));
            i += count + 1;
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noise without a dependency: the same numbers in every run, evenly spread over -1..1.
    struct Noise(u32);
    impl Noise {
        fn next(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (self.0 >> 8) as f32 / 8_388_608.0 - 1.0
        }
    }

    /// Three lines as a probe sees a lane: the low-power levels `lp` (low, high), the high-speed levels `hs` (low,
    /// middle, high), each line `offset` away from them, `noise` volts to either side, edges of two samples.
    struct Lane { lp: [f32; 2], hs: [f32; 3], offset: [f32; 3], noise: f32, wires: [Vec<f32>; 3] }
    impl Lane {
        /// The levels of the capture the decoder was corrected with (seen there, rounded): nothing above 0.8 V,
        /// no state around 0 V but a high-speed one, the lines 22 mV apart at one level, 40 mV of noise peak to peak.
        fn captured() -> Lane {
            let (hs, offset) = ([-0.30, -0.01, 0.28], [-0.011, 0.003, 0.011]);
            Lane { lp: [-0.59, 0.57], hs, offset, noise: 0.02, wires: Default::default() }
        }
        /// Levels as the specification has them (assumed, not seen on a lane): the high-speed levels near the low
        /// one of the low-power states.
        fn specified() -> Lane {
            let (hs, offset) = ([0.1, 0.2, 0.3], [0.004, -0.004, 0.0]);
            Lane { lp: [0.0, 1.2], hs, offset, noise: 0.01, wires: Default::default() }
        }
        fn low_power(&mut self, states: &[u8], samples: usize) {
            for &bits in states {
                for (k, wire) in self.wires.iter_mut().enumerate() {
                    wire.resize(wire.len() + samples, self.lp[(bits >> (2 - k) & 1) as usize]);
                }
            }
        }
        fn high_speed(&mut self, states: &[u8], samples: usize) {
            for &s in states {
                for (wire, v) in self.wires.iter_mut().zip(wire_values(s)) {
                    wire.resize(wire.len() + samples, self.hs[(v / 0.2).round() as usize]);
                }
            }
        }
        fn finish(self) -> [Vec<f32>; 3] {
            let mut noise = Noise(1);
            let mut line = 0;
            self.wires.map(|w| {
                let offset = self.offset[line];
                line += 1;
                // an edge over two samples: the mean of three; two numbers of noise make small values the usual ones
                (0..w.len()).map(|i| (w[i.saturating_sub(1)] + w[i] + w[(i + 1).min(w.len() - 1)]) / 3.0 + offset
                    + self.noise * (noise.next() + noise.next()) / 2.0).collect()
            })
        }
    }

    /// The symbols of a burst as they are on the wire: the preamble, then a sync word before each group of words,
    /// each word with its least significant symbol first, then the post.
    fn burst(preamble: usize, groups: &[&[u16]], post: usize) -> Vec<u8> {
        let mut symbols = vec![3; preamble];
        for group in groups {
            symbols.extend(SYNC);
            for &value in *group { symbols.extend(symbols_for_word(value).iter().rev()); }
        }
        symbols.resize(symbols.len() + post, 4);
        symbols
    }

    fn text(p: &Packet) -> String {
        match p.kind {
            Kind::Field(label) if label.starts_with('!') => label[1..].to_string(),
            Kind::Field(label) => format!("{label} {}", p.value),
            Kind::Word { .. } => format!("{:04X}", p.value),
            Kind::Stop => "POST".to_string(),
            _ => format!("{:?}", p.kind),
        }
    }
    fn texts(packets: &[Packet]) -> Vec<String> {
        packets.iter().map(text).collect()
    }

    #[test]
    fn published_word_and_all_65536_values_use_the_real_mapper() {
        assert_eq!(word(&[0, 2, 2, 4, 1, 0, 0]), Some(0x7290));
        assert_eq!(symbols_for_word(0x7290), [0, 2, 2, 4, 1, 0, 0]);
        let mut seen = std::collections::HashSet::new();
        for value in 0..=u16::MAX {
            let s = symbols_for_word(value);
            assert!(seen.insert(s));
            assert_eq!(word(&s), Some(value));
        }
        assert_eq!(word(&SYNC), None);
        assert_eq!(word(&[4; 7]), None);
    }
    #[test]
    fn every_wire_transition_and_reference_burst_decodes_known_words() {
        let levels = Levels { gap: 0.02, spread: 0.6, low: 0.1, high: 0.3 };
        for previous in 0..6 { for s in 0..5 {
            let next = states_for_symbols(previous, &[s])[1];
            assert_eq!(symbol(previous, next), Some(s));
            let v = wire_values(next);
            assert_eq!(state(v[0], v[1], v[2], &levels), next);
        } }
        let symbols = burst(12, &[&[0x7290, 0x1234, 0xffff]], 7);
        let mut wires: [Vec<f32>; 3] = Default::default();
        for s in states_for_symbols(0, &symbols) {
            for _ in 0..20 { for (wire, value) in wires.iter_mut().zip(wire_values(s)) { wire.push(value); } }
        }
        let packets = decode(&wires[0], &wires[1], &wires[2], 20e-12, None, 0.02);
        assert_eq!(texts(&packets), ["PRE 12", "SYNC 0", "7290", "1234", "FFFF", "POST"]);
        // the preamble with the state the burst begins in, the post to the end of the record
        assert_eq!((packets[0].t0, packets[0].t1, packets[1].t1), (0.0, 13.0 * 20.0, 20.0 * 20.0));
        assert_eq!((packets[5].t0, packets[5].t1), (41.0 * 20.0, wires[0].len() as f64));
        let mid = decode(&wires[0][400..700], &wires[1][400..700], &wires[2][400..700], 20e-12, None, 0.02);
        assert!(!mid.iter().any(|p| p.kind == Kind::Word { cmd: false }), "no invented word alignment");
        assert!(mid.len() == 14 && mid.iter().all(|p| p.kind == Kind::Field("SYM") && p.ok), "{:?}", texts(&mid));
    }

    /// What a capture of a real lane has and the first decoder did not read: low-power levels that are not 0 V and
    /// 1.2 V, lines a little apart at the same level, noise, edges, low-power states before and after a burst, a
    /// second sync word inside it, a post longer than seven symbols. And the same lane with the levels of the
    /// specification.
    #[test]
    fn a_lane_with_its_own_levels_noise_and_low_power_states_decodes() {
        for (name, mut lane) in [("captured", Lane::captured()), ("specified", Lane::specified())] {
            // low-power data ahead, a bit a pulse of line A (1) or line C (0): most of the record is such states, and
            // two lines a noise apart at the low level are what the first decoder took for wire states
            let mut expected = Vec::new();
            for bit in [1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0] {
                lane.low_power(&[if bit == 1 { 0b100 } else { 0b001 }, 0b000], 250);
                expected.extend([if bit == 1 { "LP100" } else { "LP001" }, "LP000"]);
            }
            // out of it, the stop state, the request for a burst
            lane.low_power(&[0b100, 0b101, 0b111, 0b001, 0b000], 250);
            let header = [0x1140, 0x0005, 0xbeef];
            let symbols = burst(28, &[&header, &[0x1140, 0x0005, 0xbeef, 0x7290, 0xffff, 0x0000, 0xb000]], 38);
            lane.high_speed(&states_for_symbols(0, &symbols), 25);
            lane.low_power(&[0b111, 0b100, 0b000], 250);
            let [a, b, c] = lane.finish();
            expected.extend(["LP100", "LP101", "LP111", "LP001", "LP000", "PRE 28", "SYNC 0", "1140", "0005", "BEEF",
                "SYNC 1", "1140", "0005", "BEEF", "7290", "FFFF", "0000", "B000", "POST", "LP111", "LP100", "LP000"]);
            for rate in [None, Some(50e6)] {
                let packets = decode(&a, &b, &c, 0.8e-9, rate, 0.02);
                assert_eq!(texts(&packets), expected, "{name} {rate:?}");
                assert!(packets.iter().all(|p| p.ok), "{name}");
                // each packet where it is on the lines, within the two samples of an edge
                let near = |k: usize, t0: usize, t1: usize| {
                    let p = &packets[k];
                    let fits = (p.t0 - t0 as f64).abs() <= 2.0 && (p.t1 - t1 as f64).abs() <= 2.0;
                    assert!(fits, "{name} {}: {p:?}", expected[k]);
                };
                let (pre, hs) = (37, 37 * 250);
                near(1, 250, 500);
                near(pre - 1, hs - 250, hs);
                near(pre, hs, hs + 29 * 25);
                near(pre + 1, hs + 29 * 25, hs + 36 * 25);
                near(pre + 2, hs + 36 * 25, hs + 43 * 25);
                near(pre + 5, hs + 57 * 25, hs + 64 * 25);
                near(pre + 13, hs + 113 * 25, hs + 151 * 25);
                near(pre + 14, hs + 151 * 25, hs + 151 * 25 + 250);
            }
        }
    }

    /// A burst whose header checksum fits is a packet: a short one with its two data bytes, a long one with its
    /// count, its bytes without the fill byte and its payload checksum; the second header is one field. A header
    /// that does not fit leaves the words of its burst as they are.
    #[test]
    fn the_words_of_a_burst_are_a_packet_where_the_header_checksum_fits() {
        let header = |id: u8, count: u16| {
            let first = (id as u16) << 8;
            [first, count, crc16([0, id, count as u8, (count >> 8) as u8])]
        };
        let run = |groups: &[&[u16]]| {
            let mut lane = Lane::captured();
            lane.low_power(&[0b111, 0b001, 0b000], 250);
            lane.high_speed(&states_for_symbols(0, &burst(28, groups, 38)), 25);
            lane.low_power(&[0b111], 250);
            let [a, b, c] = lane.finish();
            let p = decode(&a, &b, &c, 0.8e-9, None, 0.02);
            let said = |x: &Packet| match x.kind {
                Kind::Field("DT") => format!("VC {} DT {:02X}", x.value2.unwrap(), x.value),
                Kind::Field("WC") => format!("WC {}", x.value),
                Kind::Field("COPY") => format!("COPY{}", if x.ok { "" } else { " !" }),
                Kind::Data => format!("{:02X}", x.value),
                Kind::Crc => format!("CRC {:04X}{}", x.value, if x.ok { "" } else { " !" }),
                _ => text(x),
            };
            p.iter().skip(3).map(said).collect::<Vec<_>>().join(" ")
        };
        // a short packet (data type 00 on virtual channel 1), as the first burst of the capture is
        let short = header(0x40, 0xedca);
        assert_eq!(short, [0x4000, 0xedca, 0x0e66], "the checksum the capture has");
        assert_eq!(run(&[&short, &short]), "PRE 28 SYNC 0 VC 1 DT 00 CA ED CRC 0E66 SYNC 1 COPY POST LP111");
        // a long packet of five bytes: three words, of which the last has the fill byte, and the checksum over six
        let long = header(0x11, 5);
        let payload = [0x2e16, 0xfd2f, 0x7ec7];
        let sum = crc16([0x16, 0x2e, 0x2f, 0xfd, 0xc7, 0x7e]);
        let second: Vec<u16> = long.iter().chain(&payload).copied().chain([sum]).collect();
        let want = ["PRE 28 SYNC 0 VC 0 DT 11 WC 5", "SYNC 1 COPY 16 2E 2F FD C7", "POST LP111"];
        let want = format!("{} CRC {:04X} {} CRC {:04X} {}", want[0], long[2], want[1], sum, want[2]);
        assert_eq!(run(&[&long, &second]), want);
        // a payload checksum that does not fit, and a second header that differs
        let mut bad = second.clone();
        (bad[1], bad[6]) = (6, sum ^ 1);
        let got = run(&[&long, &bad]);
        assert!(got.contains("COPY !") && got.ends_with(&format!("CRC {:04X} ! POST LP111", sum ^ 1)), "{got}");
        // a header whose checksum does not fit: words
        assert_eq!(run(&[&[0x1100, 0x0005, 0xbeef]]), "PRE 28 SYNC 0 1100 0005 BEEF POST LP111");
    }

    /// Low-power data: after the escape entry the bits are the command and bytes, the first bit sent the least
    /// significant; without the entry the states stay states, and so do bits that fill no byte.
    #[test]
    fn low_power_data_after_the_escape_entry_is_a_command_and_bytes() {
        let bits = |lane: &mut Lane, values: &[u8]| {
            for &v in values {
                for k in 0..8 {
                    lane.low_power(&[if v >> k & 1 == 1 { 0b100 } else { 0b001 }, 0b000], 250);
                }
            }
        };
        let read = |entry: bool, tail: usize| {
            let mut lane = Lane::captured();
            lane.low_power(if entry { &[0b111, 0b100, 0b000, 0b001, 0b000] } else { &[0b000] }, 250);
            bits(&mut lane, &[0x87, 0x08, 0x0f]);
            for _ in 0..tail {
                lane.low_power(&[0b100, 0b000], 250);
            }
            // out of the mode, as the capture has it: A high, A and C high, the stop state
            lane.low_power(&[0b100, 0b101, 0b111], 250);
            let [a, b, c] = lane.finish();
            let p = decode(&a, &b, &c, 0.8e-9, None, 0.02);
            let said = |x: &Packet| match x.kind {
                Kind::Field("ESC") => format!("ESC {:02X}", x.value),
                Kind::Data => format!("{:02X}", x.value),
                _ => text(x),
            };
            (p.iter().map(said).collect::<Vec<_>>().join(" "), p)
        };
        let (said, p) = read(true, 0);
        assert_eq!(said, "LP111 LP100 LP000 LP001 LP000 ESC 87 08 0F LP100 LP101 LP111");
        assert_eq!(p[5].value, LPDT);
        // the command where its sixteen states are: from the sixth state of the record, each 250 samples long
        assert!((p[5].t0 - 1250.0).abs() <= 2.0 && (p[5].t1 - 5250.0).abs() <= 2.0, "{:?}", p[5]);
        // three bits more: no byte, they stay states (the last of them runs into the one the lane leaves with)
        assert!(read(true, 3).0.ends_with("ESC 87 08 0F LP100 LP000 LP100 LP000 LP100 LP000 LP100 LP101 LP111"));
        // no entry: states
        assert!(!read(false, 0).0.contains("ESC"));
    }

    /// With the high-speed levels near the low one of the low-power states, the lines on their way from one wire
    /// state to the next can be all three under the limit of that level: for a sample or two they read as a
    /// low-power state, in every symbol of a post (two lines cross at the middle level, the third is there). That is
    /// none (it does not last), and it does not hide that one wire state follows the other.
    #[test]
    fn lines_between_two_wire_states_are_no_low_power_state() {
        let mut lane = Lane::specified();
        lane.low_power(&[0b111, 0b001, 0b000], 250);
        lane.high_speed(&states_for_symbols(0, &[4; 40]), 25);
        lane.low_power(&[0b111], 250);
        let [a, b, c] = lane.finish();
        let levels = levels(&a, &b, &c, 0.02).unwrap();
        let states = runs(&a, &b, &c, &levels).unwrap();
        // the burst: 41 wire states after three low-power states
        let between = |run: &&Run| run.start > 750 && run.end < 750 + 41 * 25 && (8..16).contains(&run.state);
        assert_eq!(states.iter().filter(between).count(), 40, "the case this test is for");
        assert!(states.iter().filter(between).all(|run| run.end - run.start <= 2));
        assert_eq!(symbol_time(&states), Some(25.0));
        let packets = decode(&a, &b, &c, 0.8e-9, None, 0.02);
        assert_eq!(texts(&packets), ["LP111", "LP001", "LP000", "POST", "LP111"]);
        // and a whole burst there
        let mut lane = Lane::specified();
        lane.low_power(&[0b111, 0b001, 0b000], 250);
        lane.high_speed(&states_for_symbols(0, &burst(28, &[&[]], 38)), 25);
        lane.low_power(&[0b111], 250);
        let [a, b, c] = lane.finish();
        let packets = decode(&a, &b, &c, 0.8e-9, None, 0.02);
        assert_eq!(texts(&packets), ["LP111", "LP001", "LP000", "PRE 28", "SYNC 0", "POST", "LP111"]);
    }

    #[test]
    fn low_power_states_alone_are_listed_and_lines_that_do_not_move_are_no_source() {
        let mut lane = Lane::captured();
        lane.low_power(&[0b111, 0b100, 0b000, 0b001, 0b000, 0b100, 0b000, 0b100, 0b101, 0b111], 250);
        let [a, b, c] = lane.finish();
        let packets = decode(&a, &b, &c, 0.8e-9, None, 0.02);
        let expected = ["LP111", "LP100", "LP000", "LP001", "LP000", "LP100", "LP000", "LP100", "LP101", "LP111"];
        assert_eq!(texts(&packets), expected);
        assert!(packets.iter().all(|p| p.ok));
        // one state all the time: nothing says which, and its noise is no state
        let mut lane = Lane::captured();
        lane.low_power(&[0b000], 4000);
        let [a, b, c] = lane.finish();
        assert_eq!(texts(&decode(&a, &b, &c, 0.8e-9, None, 0.02)), ["SOURCE"]);
        assert_eq!(texts(&decode(&a[..4], &b[..4], &c[..4], 0.8e-9, None, 0.02)), ["SOURCE"]);
        assert_eq!(texts(&decode(&a, &b, &c, f64::NAN, None, 0.02)), ["SOURCE"]);
    }

    #[test]
    fn a_burst_with_too_few_samples_a_symbol_is_said_and_not_read() {
        let symbols = burst(28, &[&[0x1234, 0x5678]], 14);
        let mut lane = Lane::captured();
        lane.high_speed(&states_for_symbols(0, &symbols), 2);
        let [a, b, c] = lane.finish();
        let packets = decode(&a, &b, &c, 0.8e-9, None, 0.02);
        assert_eq!(texts(&packets), ["SAMPLE"]);
        assert!(!packets[0].ok);
        // enough samples, but not for the rate that was set
        let mut lane = Lane::captured();
        lane.high_speed(&states_for_symbols(0, &symbols), 25);
        let [a, b, c] = lane.finish();
        assert_eq!(texts(&decode(&a, &b, &c, 0.8e-9, Some(1e9), 0.02)), ["SAMPLE"]);
        assert_eq!(texts(&decode(&a, &b, &c, 0.8e-9, None, 0.02)), ["PRE 28", "SYNC 0", "1234", "5678", "POST"]);
        // with low-power states around it they are still listed
        let mut lane = Lane::captured();
        lane.low_power(&[0b111, 0b001, 0b000], 250);
        lane.high_speed(&states_for_symbols(0, &symbols), 2);
        lane.low_power(&[0b111], 250);
        let [a, b, c] = lane.finish();
        assert_eq!(texts(&decode(&a, &b, &c, 0.8e-9, None, 0.02)), ["LP111", "LP001", "LP000", "LP111"]);
    }

    #[test]
    fn seven_symbols_that_are_no_word_are_a_code_error_and_the_next_word_is_read() {
        let mut symbols = burst(14, &[&[0x1234]], 0);
        symbols.extend([4, 4, 4, 0, 0, 0, 0]);
        symbols.extend(symbols_for_word(0x5678).iter().rev());
        symbols.resize(symbols.len() + 10, 4);
        let mut lane = Lane::captured();
        lane.low_power(&[0b001, 0b000], 250);
        lane.high_speed(&states_for_symbols(3, &symbols), 25);
        lane.low_power(&[0b111], 250);
        let [a, b, c] = lane.finish();
        let packets = decode(&a, &b, &c, 0.8e-9, None, 0.02);
        assert_eq!(texts(&packets), ["LP001", "LP000", "PRE 14", "SYNC 0", "1234", "CODE", "5678", "POST", "LP111"]);
        assert_eq!(packets.iter().filter(|p| !p.ok).map(text).collect::<Vec<_>>(), ["CODE"]);
    }
}
