// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! MIL-STD-1553 and ARINC 429.
//!
//! A new decoder of the family goes into this file, a new family into a file of its own next to it (with its `mod` and
//! `pub use` line in `decode.rs`); the ui reaches it through a protocol module registered in `ui/src/protocols/mod.rs`
//! (ADR 0002).

use super::{Kind, Packet};
use crate::edges::{edges, states};
use crate::protocols::{BusWhat, Mirror, ModuleInfo, OPTS};
use crate::trigger::Slope;

/// MIL-STD-1553 on one line.
pub static MIL1553: ModuleInfo = ModuleInfo::new("Mil1553", "1553", "avionics")
    .starts_on(BusWhat::Id, 0x0c23)
    .trigger(mil1553_trigger)
    // the sync of a word goes either way
    .starts_with(0, Slope::Either)
    .bit(|_, _| 1e-6)
    .polarity(0, Mirror::Threshold)
    .decoder(|l| Ok(mil1553(l.main(), l.dt, l.thr, l.hy)));

/// Trigger > Bus > Frame: the command word a message begins with (the ui's `frame_start`). A status word and the
/// second command of a transfer between two terminals have the same sync, and by the generic rule each was a start
/// too: two points a message (host, the demo traffic: 134 for its 67 messages). Not yet verified on hardware.
fn mil1553_trigger(what: BusWhat, _value: u32, p: &Packet) -> Option<bool> {
    let command = p.kind == Kind::Word { cmd: true } && p.value2 == Some(MIL1553_COMMAND);
    (what == BusWhat::Frame).then_some(command)
}

/// What a word with the command sync of MIL-STD-1553 is (`Packet::value2` of a `Word { cmd: true }`): the command
/// that begins a message, the transmit command of a transfer between two terminals, or a status word.
pub const MIL1553_COMMAND: u32 = 1;
pub const MIL1553_COMMAND_2: u32 = 2;
pub const MIL1553_STATUS: u32 = 3;

/// Says of every word with the command sync what it is (`MIL1553_COMMAND` ...). A command and a status word have the
/// same sync and the same 16 bits: which one a word is follows from its place in the message. The formats of the
/// standard, with the times between words as dead time on the bus (`us` samples a microsecond):
///
/// - a receive command, its data words at once, and after the response time the status word of the terminal;
/// - a transmit command, and after the response time the status word and the data words at once;
/// - between two terminals: a receive command and a transmit command at once, the status and the data of the one
///   that sends, the status of the one that receives;
/// - a mode command (subaddress 0 or 31) has one data word where its mode code is 16 or more, else none;
/// - a broadcast (terminal 31) is not answered.
///
/// A message ends where it is complete, or at 14 us without a word (a terminal answers within 12). A word with the
/// command sync that no message waits for begins the next one. A record that begins inside a message has words
/// before its first command, which stay as they are.
fn mil1553_roles(w: &mut [Packet], us: f64) {
    let is_cmd = |p: &Packet| p.kind == Kind::Word { cmd: true };
    let dead = |w: &[Packet], j: usize| w[j].t0 - w[j - 1].t1;
    // words of one sender follow one another at once, an answer after 2 us of quiet bus at the least. Between the
    // two: a bus off its rate leaves up to 1.3 us between words that are sent at once (seen on a capture)
    let at_once = 1.8 * us;
    let mut i = 0;
    while i < w.len() {
        if !is_cmd(&w[i]) {
            i += 1;
            continue;
        }
        w[i].value2 = Some(MIL1553_COMMAND);
        let c = w[i].value;
        let (rt, transmit, sa, count) = (c >> 11, c >> 10 & 1 == 1, c >> 5 & 31, c & 31);
        let mode = sa == 0 || sa == 31;
        let mut j = i + 1;
        let two = !transmit && !mode && j < w.len() && is_cmd(&w[j]) && dead(w, j) < at_once;
        if two {
            w[j].value2 = Some(MIL1553_COMMAND_2);
            j += 1;
        }
        let words = if mode { (count >= 16) as u32 } else if count == 0 { 32 } else { count };
        // how many status words the message has, and whether the first comes before the data
        let want = if two { 2 } else { (rt != 31) as u32 };
        let first = transmit || two;
        let (mut data, mut seen) = (words, 0);
        while j < w.len() && dead(w, j) <= 14.0 * us && !(seen == want && data == 0) {
            if is_cmd(&w[j]) {
                let place = if first && seen == 0 { data == words } else { data == 0 };
                // (whatever the time before it: the word that begins the next message cannot follow at once)
                if seen == want || !place {
                    break;
                }
                w[j].value2 = Some(MIL1553_STATUS);
                seen += 1;
            } else if data == 0 {
                break;
            } else {
                data -= 1;
            }
            j += 1;
        }
        i = j;
    }
}

/// MIL-STD-1553: Manchester II at 1 Mbit/s on a bipolar bus signal (threshold normally 0 V). A word = 3 us sync
/// (command/status: 1.5 us positive then 1.5 us negative; data: the inverse), 16 data bits MSB first (1 = positive
/// then negative half), odd parity. `ok` = halves complementary and parity odd. A word with the command sync says
/// in `value2` what it is (`mil1553_roles`).
///
/// Every bit has an edge in its middle, and the decoder follows those edges from bit to bit: the place of the next
/// one is the last one found plus the bit time seen so far. The bits were sampled on a 1 us grid from the edge in
/// the sync alone, which a bus a few percent off its rate leaves before the word ends (seen on a capture: words of
/// 20.8 us, of which the later bits were read in the wrong half).
pub fn mil1553(v: &[f32], dt: f64, thr: f32, hyst: f32) -> Vec<Packet> {
    let us = 1e-6 / dt; // samples per microsecond
    let mut out = Vec::new();
    if us < 4.0 {
        return out;
    }
    let st = states(v, thr, hyst);
    let level = |t: f64| st.get(t as usize).copied().flatten();
    let e = edges(v, thr, hyst);
    let mut busy_until = 0.0;
    for b in &e {
        // sync: the edge in the middle of the 3 us sync. Data bits never hold a level longer than 1 us, so a level
        // held for >= 1.3 us on both sides of the edge only occurs there (the bus idles at 0 V before a word, which
        // gives no usable edge at the start of the sync)
        let mid = b.t;
        if mid < busy_until {
            continue;
        }
        let first_pos = !b.rising; // positive before the middle edge
                                   // the bus must really be driven (beyond the hysteresis), not idle at 0 V where the comparator keeps its state
        let driven =
            |t: f64, lv: bool| v.get(t as usize).is_some_and(|&x| if lv { x > thr + hyst } else { x < thr - hyst });
        let held = |from: f64, to: f64, lv: bool| (0..=6).all(|i| driven(from + (to - from) * i as f64 / 6.0, lv));
        if !held(mid - 1.3 * us, mid - 0.15 * us, first_pos) || !held(mid + 0.15 * us, mid + 1.3 * us, !first_pos) {
            continue;
        }
        // the edge nearest to `t`, when it is within 0.3 bit times of it: the one in the middle of a bit, not one of
        // the edges between two bits, which lie half a bit away
        let edge_at = |t: f64, bit: f64| -> Option<(f64, bool)> {
            let k = e.partition_point(|x| x.t < t);
            let near = [k.checked_sub(1), Some(k)].into_iter().flatten().filter_map(|i| e.get(i));
            let off = |x: &(f64, bool)| (x.0 - t).abs();
            near.map(|x| (x.t, x.rising)).filter(|x| off(x) <= 0.3 * bit).min_by(|a, b| off(a).total_cmp(&off(b)))
        };
        let mut val = 0u32;
        let mut ones = 0;
        let mut ok = true;
        let mut complete = true;
        // the middle of the bit before (of the sync: its edge, 2 bit times before the middle of the first bit)
        let (mut last, mut bit, mut step) = (mid, us, 2.0);
        for i in 0..17 {
            let want = last + step * bit;
            let one = match edge_at(want, bit) {
                // a one is positive, then negative: its edge falls
                Some((t, rising)) => {
                    bit = 0.5 * bit + 0.5 * (t - last) / step;
                    last = t;
                    !rising
                }
                // no edge where a bit has its middle: the halves as they are read on the grid, and an error
                None => match (level(want - 0.25 * bit), level(want + 0.25 * bit)) {
                    // a bit has its edge in the middle; halves that differ without one there are not a bit either
                    (Some(h1), Some(_)) => {
                        ok = false;
                        last = want;
                        h1
                    }
                    _ => {
                        complete = false;
                        break;
                    }
                },
            };
            step = 1.0;
            if i < 16 {
                val = val << 1 | one as u32;
            }
            ones += one as u32;
        }
        if !complete {
            continue;
        }
        ok &= ones % 2 == 1;
        let end = last + 0.5 * bit;
        out.push(Packet::new(Kind::Word { cmd: first_pos }, mid - 1.5 * us, end, val, ok));
        busy_until = end - 0.5 * us;
    }
    mil1553_roles(&mut out, us);
    out
}

/// ARINC 429 low speed (12 to 14.5 kbit/s; the usual rate) and high speed bit rates.
pub const ARINC429_LOW: f64 = 12.5e3;

pub const ARINC429_HIGH: f64 = 100e3;

/// One RZ pulse: start, end (fractional samples), HI (true) or LO.
type Pulse = (f64, f64, bool);

fn mid(p: &Pulse) -> f64 {
    0.5 * (p.0 + p.1)
}

/// ARINC 429 on one line against ground, or a pair.
pub static ARINC429: ModuleInfo = ModuleInfo::new("Arinc429", "ARINC 429", "avionics")
    .differential()
    .trigger(arinc429_trigger)
    .starts_on(BusWhat::Id, 0o310)
    // a word starts with a pulse of either sign
    .starts_with(0, Slope::Either)
    // auto: high speed (the finer sampling), with room for a low-speed word
    .bit(|opt, _| 1.0 / arinc_rate(opt).unwrap_or(ARINC_RATES[2]))
    .span(|opt, _| arinc_rate(opt).is_none().then(|| 200.0 / ARINC_RATES[1]))
    .polarity(1, Mirror::Zero)
    .decoder(|l| Ok(arinc429(l.main(), l.dt, arinc_rate(&l.opt), l.thr, l.hy)));

/// ARINC 429 bit rates of the Decode menu; 0 = from the pulse spacing.
pub const ARINC_RATES: [f64; 3] = [0.0, ARINC429_LOW, ARINC429_HIGH];

/// The ARINC 429 bit rate of the options; `None` = from the pulse spacing.
pub fn arinc_rate(opt: &[usize; OPTS]) -> Option<f64> {
    let r = ARINC_RATES[opt[0]];
    (r > 0.0).then_some(r)
}

/// ARINC 429's rule of the protocol trigger: a word starts with its label.
fn arinc429_trigger(what: BusWhat, _value: u32, p: &Packet) -> Option<bool> {
    (what == BusWhat::Frame && matches!(p.kind, Kind::Id { .. })).then_some(true)
}

/// ARINC 429 (ARINC Specification 429 Part 1): bipolar return-to-zero, a 1 = HI and a 0 = LO for the first half of
/// the bit, NULL for the second; `v` = A − B (or one line against ground at half the amplitude). HI is above `thr`,
/// LO below −`thr` (`thr` > 0, each with the hysteresis `hyst`), NULL in between. `rate` = bit rate (`ARINC429_LOW`,
/// `ARINC429_HIGH`), None = from the pulse spacing. Words are 32 bits, separated by at least 4 bit times of NULL.
///
/// Emits per word: Id (label = bits 1-8, sent MSB first, conventionally shown in octal), Field("SDI") (bits 9-10),
/// Data (bits 11-29, bit 11 = LSB), Field("SSM") (bits 30-31) and Field("PAR") (bit 32, `ok` = odd parity over the
/// word). A word with a wrong number of bits or a bit out of its slot gives Field("!ERR"); words cut by the record
/// start or end are skipped.
pub fn arinc429(v: &[f32], dt: f64, rate: Option<f64>, thr: f32, hyst: f32) -> Vec<Packet> {
    let mut out = Vec::new();
    let thr = thr.abs();
    if thr.is_nan() || thr <= 0.0 || dt.is_nan() || dt <= 0.0 {
        return out;
    }
    // HI pulses through +thr, LO pulses through -thr
    let mut pulses: Vec<Pulse> = Vec::new();
    for hi in [true, false] {
        let mut start = None;
        for e in edges(v, if hi { thr } else { -thr }, hyst) {
            if e.rising == hi {
                start = Some(e.t);
            } else if let Some(s) = start.take() {
                pulses.push((s, e.t, hi));
            }
        }
    }
    pulses.sort_by(|a, b| a.0.total_cmp(&b.0));
    if pulses.is_empty() {
        return out;
    }
    let median = |mut x: Vec<f64>| {
        x.sort_by(f64::total_cmp);
        x.get(x.len() / 2).copied()
    };
    let bit = match rate {
        Some(r) => 1.0 / r / dt,
        None => {
            // pulses are half a bit wide, inside a word they follow one bit apart
            let w = median(pulses.iter().map(|p| p.1 - p.0).collect()).unwrap_or(0.0);
            median(pulses.windows(2).map(|q| mid(&q[1]) - mid(&q[0])).filter(|&d| d < 3.0 * w).collect())
                .unwrap_or(2.0 * w)
        }
    };
    if !bit.is_finite() || bit < 4.0 {
        return out;
    }
    let n = v.len() as f64;
    let mut i = 0;
    while i < pulses.len() {
        // a word: pulses less than 2.5 bits apart (the gap between words is at least 5 bits pulse to pulse)
        let mut j = i + 1;
        while j < pulses.len() && mid(&pulses[j]) - mid(&pulses[j - 1]) < 2.5 * bit {
            j += 1;
        }
        arinc429_word(&pulses[i..j], bit, n, &mut out);
        i = j;
    }
    out
}

fn arinc429_word(p: &[Pulse], bit: f64, n: f64, out: &mut Vec<Packet>) {
    let (first, last) = (p[0], p[p.len() - 1]);
    let inside = first.0 >= 2.0 * bit && n - last.1 >= 2.0 * bit;
    let m0 = mid(&first);
    // the bit time of this word, and every pulse in its slot
    let b = (mid(&last) - m0) / 31.0;
    let slots = p.len() == 32
        && (b / bit - 1.0).abs() < 0.2
        && p.iter().enumerate().all(|(k, q)| ((mid(q) - m0) / b - k as f64).abs() < 0.3);
    if !slots {
        if inside {
            out.push(Packet::tag("!ERR", first.0, last.1, false));
        }
        return;
    }
    let bits: Vec<bool> = p.iter().map(|q| q.2).collect();
    let t0 = m0 - 0.25 * b; // start of bit 1
    let at = |k: usize| t0 + k as f64 * b;
    let lsb = |a: usize, len: usize| (0..len).fold(0u32, |acc, k| acc | (bits[a + k] as u32) << k);
    let label = bits[..8].iter().fold(0u32, |acc, &x| acc << 1 | x as u32);
    let odd = bits.iter().filter(|&&x| x).count() % 2 == 1;
    out.push(Packet::new(Kind::Id { ext: false, rtr: false }, at(0), at(8), label, true));
    out.push(Packet::field("SDI", at(8), at(10), lsb(8, 2), true));
    out.push(Packet::new(Kind::Data, at(10), at(29), lsb(10, 19), true));
    out.push(Packet::field("SSM", at(29), at(31), lsb(29, 2), true));
    out.push(Packet::field("PAR", at(31), at(32), bits[31] as u32, odd));
}
