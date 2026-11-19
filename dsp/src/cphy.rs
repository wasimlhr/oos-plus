// SPDX-License-Identifier: GPL-2.0-or-later
//! MIPI C-PHY six-wirestate, single-trio electrical symbols and 16b/7s words, with the low-power states around them.
//!
//! Mapper: MIPI DevCon 2016, "Implementing MIPI C-PHY", p12. Symbol/state
//! rules and the independent 0224100 -> 7290 example: MIPI "Introduction from
//! Basic Theory to Practical Implementation", pp14-16. This is not base-five
//! decoding, eighteen-wirestate mode, lane merging, or a CSI/DSI packet parser.
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
    out
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
