// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! LIN, FlexRay and SENT (SAE J2716).
//!
//! A new decoder of the family goes into this file, a new family into a file of its own next to it (with its `mod` and
//! `pub use` line in `decode.rs`); the ui reaches it through a protocol module registered in `ui/src/protocols/mod.rs`
//! (ADR 0002).

use super::{flexray_frame_crc, flexray_header_crc, sent_crc, Kind, Packet};
use crate::edges::{edges, states};
use crate::protocols::{BusWhat, Mirror, ModuleInfo, OPTS};

/// SENT clock ticks (s) of the Decode menu; 0 = any tick (from the sync pulses).
pub const SENT_TICKS: [f64; 8] = [3e-6, 5e-6, 10e-6, 20e-6, 30e-6, 50e-6, 90e-6, 0.0];

/// The SENT clock tick of the options; `None` = from the sync pulses.
pub fn sent_tick(opt: &[usize; OPTS]) -> Option<f64> {
    let t = SENT_TICKS[opt[0]];
    (t > 0.0).then_some(t)
}

/// LIN protected identifier: ID with the two parity bits P0 = ID0^ID1^ID2^ID4, P1 = !(ID1^ID3^ID4^ID5).
pub fn lin_pid(id: u8) -> u8 {
    let b = |i: u8| (id >> i) & 1;
    let p0 = b(0) ^ b(1) ^ b(2) ^ b(4);
    let p1 = 1 ^ (b(1) ^ b(3) ^ b(4) ^ b(5));
    (id & 0x3f) | p0 << 6 | p1 << 7
}

/// LIN checksum: inverted 8-bit sum with carry wrap-around over `bytes` (classic: data; enhanced: PID + data).
pub fn lin_checksum(bytes: &[u8]) -> u8 {
    let mut s: u16 = 0;
    for &b in bytes {
        s += b as u16;
        if s > 0xff {
            s -= 0xff;
        }
    }
    !(s as u8)
}

/// LIN on one line or a pair. Option 0 is the checksum (`LinChecksum`: either, enhanced, classic).
pub static LIN: ModuleInfo = ModuleInfo::new("Lin", "LIN", "automotive")
    .differential()
    .starts_on(BusWhat::Id, 0x21)
    .bit(|_, sh| 1.0 / sh.baud)
    .polarity(1, Mirror::Threshold)
    .decoder(|l| {
        use LinChecksum::*;
        let checksum = [Either, Enhanced, Classic][l.opt[0].min(2)];
        Ok(lin_with(l.main(), l.dt, l.sh.baud, checksum, l.thr, l.hy))
    });

/// Which checksum a LIN frame is checked against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinChecksum {
    /// the classic or the enhanced one: a bus of mixed nodes, or one whose version is not known
    Either,
    /// LIN 2.x: over the protected identifier and the data (the diagnostic frames 3C and 3D keep the classic one)
    Enhanced,
    /// LIN 1.x: over the data alone
    Classic,
}

/// LIN frames with either checksum accepted (`lin_with`).
pub fn lin(v: &[f32], dt: f64, baud: f64, thr: f32, hyst: f32) -> Vec<Packet> {
    lin_with(v, dt, baud, LinChecksum::Either, thr, hyst)
}

/// The longest a LIN frame may take, in bit times from the start of its break: 1.4 times the header of 34 bits and
/// a response of eight data bytes and the checksum (LIN 2.x, T_Frame_Maximum).
const LIN_FRAME_BITS: f64 = 1.4 * (34.0 + 90.0);

/// LIN frames: break, sync 0x55, protected ID, up to eight data bytes and the checksum. Bytes are UART 8N1, idle
/// high.
///
/// The bit time of a frame is measured on its sync field, as a node does: its five falling edges are two bits apart.
/// A dominant pulse of at least 10 of those bit times before it is the break (13 are sent). A frame whose sync
/// cannot be read that way is read at the rate of the other frames of the record (their median), and only a record
/// without one readable sync at `baud`: a pulse of 11 of those bit times is its break, and the byte after it is
/// listed as a failed sync. The value of the break packet is the measured bit rate in bit/s (0 when it was not
/// measured).
///
/// A frame is what follows its break within the longest time a frame may take, at most eight data bytes and the
/// checksum: a byte that comes later is not part of it (it was taken for its checksum).
pub fn lin_with(v: &[f32], dt: f64, baud: f64, checksum: LinChecksum, thr: f32, hyst: f32) -> Vec<Packet> {
    let nominal = 1.0 / baud / dt;
    let mut out = Vec::new();
    let e = edges(v, thr, hyst);
    let st = states(v, thr, hyst);
    let level = |x: f64| -> Option<bool> { st.get(x as usize).copied().flatten() };
    // the falling edges, each with how long the line stays low after it
    let low = |w: &[crate::edges::Edge]| (!w[0].rising && w[1].rising).then(|| (w[0].t, w[1].t - w[0].t));
    let falls: Vec<(f64, f64)> = e.windows(2).filter_map(low).collect();
    // the bit time of the sync field after the pulse `k`, when that pulse is a break and a sync follows it: five
    // falling edges two bits apart, soon after the break
    let sync = |k: usize| -> Option<f64> {
        let (b0, low) = falls[k];
        let f = falls.get(k + 1..k + 6)?;
        let bit = (f[4].0 - f[0].0) / 8.0;
        let even = f.windows(2).all(|p| ((p[1].0 - p[0].0) / (2.0 * bit) - 1.0).abs() < 0.15);
        (bit >= 2.5 && even && low >= 10.0 * bit && f[0].0 - (b0 + low) <= 15.0 * bit).then_some(bit)
    };
    // the rate of the bus, for a frame whose own sync cannot be read
    let mut rates: Vec<f64> = (0..falls.len()).filter_map(sync).collect();
    rates.sort_by(|a, b| a.total_cmp(b));
    let bus = rates.get(rates.len() / 2).copied().unwrap_or(nominal);
    let mut busy_until = f64::NEG_INFINITY;
    for (k, &(b0, low)) in falls.iter().enumerate() {
        if b0 < busy_until {
            continue;
        }
        let b1 = b0 + low;
        let (bit, measured) = match sync(k) {
            Some(bit) => (bit, true),
            None if bus >= 2.5 && low >= 11.0 * bus => (bus, false),
            None => continue,
        };
        let rate = if measured { (1.0 / (bit * dt)).round() as u32 } else { 0 };
        out.push(Packet::new(Kind::Break, b0, b1, rate, true));
        busy_until = b1;
        // the bytes of the frame: sync, protected identifier, up to eight data bytes, the checksum
        let last = b0 + LIN_FRAME_BITS * bit;
        let mut bytes: Vec<Packet> = Vec::new();
        let mut from = b1;
        for &(s, low) in &falls[k + 1..] {
            if s < from {
                continue;
            }
            // the next break, a byte that comes too late, or a frame that is full
            if low >= 10.0 * bit || s + 10.0 * bit > last || bytes.len() == 11 {
                break;
            }
            let bits: Option<Vec<bool>> = (0..9).map(|b| level(s + (1.5 + b as f64) * bit)).collect();
            let Some(bits) = bits else { break };
            let value = (0..8).fold(0u32, |v, b| v | (bits[b] as u32) << b);
            // (a stop bit that is low is a framing error)
            bytes.push(Packet::new(Kind::Data, s, s + 10.0 * bit, value, bits[8]));
            from = s + 9.5 * bit;
            busy_until = s + 10.0 * bit;
        }
        let mut bytes = bytes.into_iter();
        let Some(first) = bytes.next() else { continue };
        // a sync that could not be measured is listed with what was read in its place
        out.push(Packet { ok: first.ok && measured && first.value == 0x55, ..first });
        let Some(second) = bytes.next() else { continue };
        let pid = second.value as u8;
        let id = pid & 0x3f;
        let kind = Kind::Id { ext: false, rtr: false };
        out.push(Packet { kind, value: id as u32, ok: second.ok && lin_pid(id) == pid, ..second });
        let rest: Vec<Packet> = bytes.collect();
        let Some((cs, data)) = rest.split_last() else { continue };
        out.extend(data.iter().cloned());
        let values: Vec<u8> = data.iter().map(|p| p.value as u8).collect();
        let mut with_pid = vec![pid];
        with_pid.extend(&values);
        let (classic, enhanced) = (cs.value as u8 == lin_checksum(&values), cs.value as u8 == lin_checksum(&with_pid));
        let sum_ok = match checksum {
            LinChecksum::Either => classic || enhanced,
            LinChecksum::Classic => classic,
            LinChecksum::Enhanced => {
                if id >= 0x3c {
                    classic
                } else {
                    enhanced
                }
            }
        };
        out.push(Packet { kind: Kind::Crc, ok: cs.ok && sum_ok, ..cs.clone() });
    }
    out
}

/// FlexRay at 10 Mbit/s on RxD or a pair.
pub static FLEXRAY: ModuleInfo = ModuleInfo::new("FlexRay", "FlexRay", "automotive")
    .differential()
    .starts_on(BusWhat::Id, 0x2a)
    .bit(|opt, _| 1.0 / flexray_rate(opt))
    .polarity(1, Mirror::Threshold)
    .decoder(|l| Ok(flexray(l.main(), l.dt, flexray_rate(&l.opt), l.thr, l.hy, false)));

/// The bit rates of a FlexRay cluster, the option row of the ui's module.
pub const FLEXRAY_RATES: [f64; 3] = [10e6, 5e6, 2.5e6];

/// The rate the option says.
fn flexray_rate(opt: &[usize; OPTS]) -> f64 {
    FLEXRAY_RATES[opt[0].min(FLEXRAY_RATES.len() - 1)]
}

/// FlexRay frames (FlexRay Protocol Specification V3.0.1) on one channel at `rate` (normally 10 Mbit/s), as on the
/// transceiver RxD pin (idle high; `invert` = low) or a BP − BM difference from `diff` (idle 0 V reads as high).
/// TSS (low 3..15 bits), FSS (one high bit), every byte preceded by a byte start sequence (high, low; the falling
/// edge resynchronises), header, payload, CRC, FES (low, high).
///
/// Emits Field("TSS") (value = length in bits, `ok` = 3..15), Field("IND") (reserved, payload preamble, null frame,
/// sync, startup indicator; MSB = reserved bit, `ok` = reserved bit 0), Id (frame ID, `ok` = not 0), Field("LEN")
/// (payload words, value2 = bytes), Field("HCRC") (`ok` = header CRC matches), Field("CYC") (cycle count), Data per
/// payload byte, Crc (frame CRC-24, value2 = channel 0 = A / 1 = B whose init vector matched, `ok` = one matched);
/// a missing byte start sequence or a wrong FES: Field("!ERR"). A TSS counts after at least 10 bits of idle (the
/// channel idle delimiter is 11); a frame cut by the record end gives its fields so far, without an error.
pub fn flexray(v: &[f32], dt: f64, rate: f64, thr: f32, hyst: f32, invert: bool) -> Vec<Packet> {
    let spb = 1.0 / rate / dt;
    let mut out = Vec::new();
    if spb.is_nan() || spb < 4.0 {
        return out;
    }
    let st = states(v, thr, hyst);
    let high = |x: f64| st.get(x as usize).copied().flatten() != Some(invert);
    let e = edges(v, thr, hyst);
    let fall: Vec<f64> = e.iter().filter(|x| x.rising == invert).map(|x| x.t).collect();
    let rise: Vec<f64> = e.iter().filter(|x| x.rising != invert).map(|x| x.t).collect();
    // TSS candidates: falling edges after the channel idle (at least 10 bits high, longer than any high run inside a
    // frame, so the tail of a frame cut by the record start is not taken for one), and a low first decision after as
    // long an undefined start (a differential bus idles at 0 V)
    let idle = 10.0 * spb;
    let mut starts: Vec<f64> = fall
        .iter()
        .copied()
        .filter(|&f| f - rise.partition_point(|&x| x < f).checked_sub(1).map_or(0.0, |k| rise[k]) >= idle)
        .collect();
    if let Some(i) = st.iter().position(|s| s.is_some()) {
        if st[i].map(|b| b != invert) == Some(false) && i as f64 >= idle {
            starts.insert(0, i as f64);
        }
    }
    let mut busy = f64::NEG_INFINITY;
    for &f in &starts {
        if f < busy {
            continue;
        }
        let Some(&r) = rise.get(rise.partition_point(|&x| x <= f)) else { break };
        let tss = (r - f) / spb;
        if !(2.5..=16.5).contains(&tss) {
            continue;
        }
        // bits of the frame: (level, start)
        let mut bits: Vec<(bool, f64)> = Vec::new();
        let mut exp = r + 2.0 * spb; // first BSS falling edge: after FSS and the high BSS bit
        let mut nbytes = 5usize;
        let mut lost = false;
        while bits.len() < 8 * nbytes {
            let j = fall.partition_point(|&x| x < exp - 0.75 * spb);
            let sync = match fall.get(j) {
                Some(&x) if x <= exp + 0.75 * spb => x,
                _ => {
                    lost = true;
                    break;
                }
            };
            let s0 = sync + spb;
            for k in 0..8 {
                let t = s0 + k as f64 * spb;
                bits.push((high(t + 0.5 * spb), t));
            }
            exp = sync + 10.0 * spb;
            if bits.len() == 40 {
                let len = bits[16..23].iter().fold(0usize, |a, b| a << 1 | b.0 as usize);
                nbytes = 5 + 2 * len + 3;
            }
        }
        if bits.len() < 40 {
            continue; // not a frame
        }
        busy = exp + spb;
        let fv = |a: usize, n: usize| bits[a..a + n].iter().fold(0u32, |acc, b| acc << 1 | b.0 as u32);
        let fld = |kind, a: usize, n: usize, ok| Packet::new(kind, bits[a].1, bits[a + n - 1].1 + spb, fv(a, n), ok);
        out.push(Packet::field("TSS", f, r, tss.round() as u32, (2.5..=15.5).contains(&tss)));
        out.push(fld(Kind::Field("IND"), 0, 5, !bits[0].0));
        let id = fv(5, 11);
        out.push(fld(Kind::Id { ext: false, rtr: false }, 5, 11, id != 0));
        let words = fv(16, 7);
        out.push(fld(Kind::Field("LEN"), 16, 7, true).with_value2(2 * words));
        out.push(fld(Kind::Field("HCRC"), 23, 11, fv(23, 11) == flexray_header_crc(fv(3, 20))));
        out.push(fld(Kind::Field("CYC"), 34, 6, true));
        let nfull = bits.len() / 8;
        for k in 5..nfull.min(nbytes - 3) {
            out.push(fld(Kind::Data, 8 * k, 8, true));
        }
        // a frame cut by the record end is not an error
        let end = v.len() as f64;
        if lost {
            if exp + 0.75 * spb < end {
                out.push(Packet::tag("!ERR", exp - 0.75 * spb, exp + 0.75 * spb, false));
            }
            continue;
        }
        let bytes: Vec<u8> = (0..nbytes).map(|k| fv(8 * k, 8) as u8).collect();
        let rx = fv(8 * (nbytes - 3), 24);
        let body = &bytes[..nbytes - 3];
        let ch = if flexray_frame_crc(body, false) == rx {
            Some(0)
        } else if flexray_frame_crc(body, true) == rx {
            Some(1)
        } else {
            None
        };
        out.push(Packet { value2: ch, ..fld(Kind::Crc, 8 * (nbytes - 3), 24, ch.is_some()) });
        // FES: low bit from the end of the last byte, then high
        if exp + spb < end && (high(exp - 0.5 * spb) || !high(exp + 0.5 * spb)) {
            out.push(Packet::tag("!ERR", exp - spb, exp + spb, false));
        }
    }
    out
}

/// SENT (SAE J2716) on one line.
pub static SENT: ModuleInfo = ModuleInfo::new("Sent", "SENT", "automotive")
    .starts_on(BusWhat::Data, 0x7f480b)
    // about 4 ticks per data bit; auto: the shortest tick
    .bit(|opt, _| 4.0 * sent_tick(opt).unwrap_or(SENT_TICKS[0]))
    .decoder(|l| Ok(sent(l.main(), l.dt, sent_tick(&l.opt), l.opt[1] + 1, l.thr, l.hy)));

/// SENT (SAE J2716, fast channel): every nibble is the time between two falling edges in clock ticks, 12 + value.
/// A frame: synchronisation / calibration pulse of 56 ticks, status nibble, `nibbles` data nibbles (1..6), CRC
/// nibble, optional pause pulse (12..768 ticks). The tick is measured from each sync pulse; `tick` = the nominal
/// tick (s; a sync within +-25 % of 56 ticks counts), None = any tick (a frame counts only when all its nibbles are
/// valid).
///
/// Emits Field("SYNC") (value = measured tick in ns, `ok` = within +-20 % of the nominal), Field("STAT") (status
/// nibble), Data (the data nibbles, the first one most significant; value2 = their count), Crc (received CRC
/// nibble, `ok` = matches `sent_crc`, recommended method) and Field("PAUSE") (value = ticks). With a nominal tick a
/// nibble out of 0..15 after a sync gives Field("!ERR"); frames cut by the record end are skipped.
///
/// The slow channel: two bits of the status nibble of each frame carry a serial message over 16 or 18 frames
/// (`sent_slow`). At the end of the frame that completes one stand Field("SID") (its identifier), Field("SDATA")
/// (its data; value2 = their number of bits) and Field("SCRC") (its CRC, `ok` = it matches).
pub fn sent(v: &[f32], dt: f64, tick: Option<f64>, nibbles: usize, thr: f32, hyst: f32) -> Vec<Packet> {
    let mut out = Vec::new();
    // the status nibble of each frame in order and where the frame ends; `None` where the chain of frames breaks
    let mut chain: Vec<Option<(u32, f64)>> = Vec::new();
    if dt.is_nan() || dt <= 0.0 {
        return out;
    }
    let nn = nibbles.clamp(1, 6);
    let f: Vec<f64> = edges(v, thr, hyst).into_iter().filter(|e| !e.rising).map(|e| e.t).collect();
    let nom = tick.map(|t| t / dt).filter(|t| t.is_finite() && *t > 0.0);
    let is_sync = |d: f64| match nom {
        Some(t) => (d / (56.0 * t) - 1.0).abs() <= 0.25,
        None => d >= 56.0 * 2.0,
    };
    let nib = |d: f64, tk: f64| {
        let x = d / tk - 12.0;
        if x > -0.5 && x < 15.5 {
            Some(x.round() as u32)
        } else {
            None
        }
    };
    let mut i = 0;
    while i + 1 < f.len() {
        let d = f[i + 1] - f[i];
        let tk = d / 56.0;
        if !is_sync(d) || tk < 2.0 {
            i += 1;
            continue;
        }
        // a pause pulse of about the sync length: the sync follows
        if let Some(&f2) = f.get(i + 2) {
            let d2 = f2 - f[i + 1];
            if if nom.is_some() { is_sync(d2) } else { (d2 / d - 1.0).abs() < 0.1 } {
                i += 1;
                continue;
            }
        }
        // status, data, CRC
        let mut vals = Vec::new();
        let mut bad = None;
        for k in 0..nn + 2 {
            let Some(&e) = f.get(i + 2 + k) else { break };
            match nib(e - f[i + 1 + k], tk) {
                Some(x) => vals.push(x),
                None => {
                    bad = Some(k);
                    break;
                }
            }
        }
        if vals.len() < nn + 2 && (nom.is_none() || bad.is_none()) {
            i += 1;
            continue;
        }
        let tick_ok = nom.is_none_or(|t| (tk / t - 1.0).abs() <= 0.2);
        out.push(Packet::field("SYNC", f[i], f[i + 1], (tk * dt * 1e9).round() as u32, tick_ok));
        if let Some(k) = bad {
            out.push(Packet::tag("!ERR", f[i + 1 + k], f[i + 2 + k], false));
            chain.push(None);
            i += 1 + k;
            continue;
        }
        let e = |k: usize| f[i + 1 + k]; // start of nibble k (0 = status)
        out.push(Packet::field("STAT", e(0), e(1), vals[0], true));
        let data: Vec<u8> = vals[1..=nn].iter().map(|&x| x as u8).collect();
        let value = data.iter().fold(0u32, |a, &x| a << 4 | x as u32);
        out.push(Packet::new(Kind::Data, e(1), e(nn + 1), value, true).with_value2(nn as u32));
        out.push(Packet::new(
            Kind::Crc,
            e(nn + 1),
            e(nn + 2),
            vals[nn + 1],
            vals[nn + 1] == sent_crc(&data, false) as u32,
        ));
        // the edge that ends the CRC nibble
        i += nn + 3;
        chain.push(Some((vals[0], f[i])));
        // a pause pulse: followed by a sync of this tick
        if let (Some(&a), Some(&b)) = (f.get(i + 1), f.get(i + 2)) {
            let p = (a - f[i]) / tk;
            if (11.5..=768.5).contains(&p) && ((b - a) / (56.0 * tk) - 1.0).abs() < 0.1 {
                out.push(Packet::field("PAUSE", f[i], a, p.round() as u32, true));
                i += 1;
            }
        }
    }
    // the serial messages of the slow channel, each at the end of the frame that completes it (after the pause
    // pulse that begins there: the sort keeps the order of fields that begin together)
    let slow = sent_slow(&chain);
    if !slow.is_empty() {
        out.extend(slow);
        out.sort_by(|a, b| a.t0.total_cmp(&b.t0));
    }
    out
}

/// The CRC of a SENT enhanced serial message (SAE J2716): polynomial x^6 + x^4 + x^3 + 1, seed 010101, over the 24
/// message bits and six zero bits after them.
pub fn sent_crc6(bits: &[bool]) -> u8 {
    let mut c = 0x15u8;
    for bit in bits.iter().copied().chain([false; 6]) {
        let top = c & 0x20 != 0;
        c = (c << 1 | bit as u8) & 0x3f;
        if top {
            c ^= 0x19;
        }
    }
    c
}

/// The serial messages of the SENT slow channel in a chain of frames (`sent`: the status nibble of each frame and
/// the time the frame ends; `None` breaks the chain). Bit 3 of the status nibble frames a message and bit 2 carries
/// its data, one bit a frame:
///
/// - the short message, 16 frames: bit 3 is 1 in the first frame and 0 in the other fifteen; bit 2 holds a 4-bit
///   identifier, 8 data bits and a 4-bit CRC (`sent_crc` over the three nibbles);
/// - the enhanced message, 18 frames: bit 3 holds 1111110, a configuration bit, four identifier bits, 0, four more
///   bits, 0; bit 2 holds a 6-bit CRC and 12 data bits. With the configuration bit 0 the identifier has 8 bits (the
///   two groups of four on bit 3) and the data 12; with 1 the identifier has 4 bits and the second group of four is
///   the top of 16 data bits (assumed: no capture of that form was at hand). The CRC (`sent_crc6`) is over the bits
///   of frames 7 to 18, bit 2 then bit 3 of each frame.
///
/// For each message: Field("SID"), Field("SDATA") (value2 = the number of data bits) and Field("SCRC"), without a
/// length, at the end of its last frame.
pub fn sent_slow(chain: &[Option<(u32, f64)>]) -> Vec<Packet> {
    let mut out = Vec::new();
    // the two bits of frame `k`, when the chain holds from `from` to it
    let b3 = |k: usize| chain.get(k).copied().flatten().map(|(s, _)| s & 8 != 0);
    let b2 = |k: usize| chain.get(k).copied().flatten().map(|(s, _)| s & 4 != 0);
    type Bits<'a> = &'a dyn Fn(usize) -> Option<bool>;
    let all = |mut range: std::ops::Range<usize>, get: Bits, want: bool| range.all(|k| get(k) == Some(want));
    let bit = |get: Bits, k: usize| get(k).unwrap_or(false) as u32;
    let word = |range: std::ops::Range<usize>, get: Bits| range.fold(0u32, |v, k| v << 1 | bit(get, k));
    let mut emit = |at: f64, id: u32, data: u32, bits: u32, crc: u32, ok: bool| {
        out.push(Packet::field("SID", at, at, id, true));
        out.push(Packet::field("SDATA", at, at, data, true).with_value2(bits));
        out.push(Packet::field("SCRC", at, at, crc, ok));
    };
    let mut k = 0;
    while k < chain.len() {
        let whole = |n: usize| (k..k + n).all(|j| chain.get(j).copied().flatten().is_some());
        let end = |n: usize| chain[k + n - 1].map_or(0.0, |x| x.1);
        let zero = |j: usize| b3(k + j) == Some(false);
        if whole(18) && all(k..k + 6, &b3, true) && zero(6) && zero(12) && zero(17) {
            let wide = b3(k + 7) == Some(true);
            let (high, low) = (word(k + 8..k + 12, &b3), word(k + 13..k + 17, &b3));
            let data12 = word(k + 6..k + 18, &b2);
            let (id, data, bits) = if wide { (high, low << 12 | data12, 16) } else { (high << 4 | low, data12, 12) };
            let crc = word(k..k + 6, &b2);
            let pair = |j: usize| [b2(j).unwrap_or(false), b3(j).unwrap_or(false)];
            let message: Vec<bool> = (k + 6..k + 18).flat_map(pair).collect();
            emit(end(18), id, data, bits, crc, crc == sent_crc6(&message) as u32);
            k += 18;
        } else if whole(16) && b3(k) == Some(true) && all(k + 1..k + 16, &b3, false) {
            let (id, data, crc) = (word(k..k + 4, &b2), word(k + 4..k + 12, &b2), word(k + 12..k + 16, &b2));
            let ok = crc == sent_crc(&[id as u8, (data >> 4) as u8, (data & 15) as u8], false) as u32;
            emit(end(16), id, data, 8, crc, ok);
            k += 16;
        } else {
            k += 1;
        }
    }
    out
}
