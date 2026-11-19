// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Line codes, logic, FIR, and 4-level slicing used by math channels, measurements and the decoder.

use crate::decode::{opt_baud, Kind, Packet, BAUDS};
use crate::edges::edges;
use crate::eye;
use crate::protocols::{every_word_is_a_frame, Lines, ModuleInfo, OPTS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    And,
    Or,
    Xor,
    Nand,
    Nor,
}

/// Windowed-sinc low-pass FIR. `fc` is the cut-off in hertz.
pub fn fir_lowpass(a: &[f32], dt: f64, fc: f64) -> Vec<f32> {
    if a.is_empty() || !(dt > 0.0) || !(fc > 0.0) {
        return a.to_vec();
    }
    let nyq = 0.5 / dt;
    let f = (fc / nyq).clamp(0.01, 0.49);
    let m = ((3.3 / f).round() as usize | 1).clamp(5, 255);
    let mid = (m / 2) as f64;
    let mut h = vec![0.0f64; m];
    let mut s = 0.0;
    for (k, tap) in h.iter_mut().enumerate() {
        let x = k as f64 - mid;
        let sinc = if x.abs() < 1e-12 { 2.0 * f } else { (2.0 * f * x * std::f64::consts::PI).sin() / (std::f64::consts::PI * x) };
        let w = 0.54 - 0.46 * (2.0 * std::f64::consts::PI * k as f64 / (m - 1) as f64).cos();
        *tap = sinc * w;
        s += *tap;
    }
    if s.abs() > 1e-18 {
        for t in &mut h {
            *t /= s;
        }
    }
    conv(a, &h.iter().map(|&x| x as f32).collect::<Vec<_>>())
}

/// Convolution of `a` with kernel `h`, same length as `a` (kernel centred, edges use available samples).
pub fn conv(a: &[f32], h: &[f32]) -> Vec<f32> {
    if a.is_empty() || h.is_empty() {
        return a.to_vec();
    }
    let mid = h.len() / 2;
    a.iter()
        .enumerate()
        .map(|(i, _)| {
            let mut y = 0.0f64;
            for (k, &hk) in h.iter().enumerate() {
                let j = i as isize + k as isize - mid as isize;
                if j >= 0 && (j as usize) < a.len() && a[j as usize].is_finite() {
                    y += a[j as usize] as f64 * hk as f64;
                }
            }
            y as f32
        })
        .collect()
}

fn mid_of(a: &[f32]) -> f32 {
    let f: Vec<f32> = a.iter().copied().filter(|x| x.is_finite()).collect();
    if f.is_empty() {
        return 0.0;
    }
    let (lo, hi) = f.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &x| (l.min(x), h.max(x)));
    0.5 * (lo + hi)
}

fn bit(x: f32, mid: f32) -> bool {
    x.is_finite() && x >= mid
}

/// Two-level slice: 0 V / 1 V.
pub fn a2d(a: &[f32]) -> Vec<f32> {
    let m = mid_of(a);
    a.iter().map(|&x| if bit(x, m) { 1.0 } else { 0.0 }).collect()
}

/// Hold each sample at the nearer of the trace min/max (staircase).
pub fn d2a(a: &[f32]) -> Vec<f32> {
    let f: Vec<f32> = a.iter().copied().filter(|x| x.is_finite()).collect();
    let (lo, hi) = f.iter().fold((0.0f32, 1.0f32), |(l, h), &x| (l.min(x), h.max(x)));
    let m = 0.5 * (lo + hi);
    a.iter().map(|&x| if bit(x, m) { hi } else { lo }).collect()
}

pub fn gate(a: &[f32], b: &[f32], g: Gate) -> Vec<f32> {
    let (ma, mb) = (mid_of(a), mid_of(b));
    a.iter()
        .zip(b)
        .map(|(&x, &y)| {
            let (p, q) = (bit(x, ma), bit(y, mb));
            let o = match g {
                Gate::And => p && q,
                Gate::Or => p || q,
                Gate::Xor => p ^ q,
                Gate::Nand => !(p && q),
                Gate::Nor => !(p || q),
            };
            if o { 1.0 } else { 0.0 }
        })
        .collect()
}

pub fn not(a: &[f32]) -> Vec<f32> {
    let m = mid_of(a);
    a.iter().map(|&x| if bit(x, m) { 0.0 } else { 1.0 }).collect()
}

/// Invert NRZI (USB-style: a level change is 0, a hold is 1) to NRZ 0/1.
pub fn unnrzi(a: &[f32]) -> Vec<f32> {
    let bits = a2d(a);
    let mut prev = bits.first().copied().unwrap_or(0.0);
    bits.iter()
        .map(|&x| {
            let b = if (x - prev).abs() > 0.5 { 0.0 } else { 1.0 };
            prev = x;
            b
        })
        .collect()
}

/// Four histogram representatives, low to high, of equal voltage bands.
/// Empty bands are NaN. Occupied bands alone are not a PAM4 validity test:
/// a noisy three-level signal can occupy all four fixed voltage bands.
pub fn pam4_levels(f: &[f32], vmin: f32, vmax: f32) -> [f64; 4] {
    let span = (vmax - vmin).max(1e-12);
    let mut hist = [0u32; 256];
    for &x in f {
        hist[(((x - vmin) / span) * 255.0) as usize] += 1;
    }
    let mut out = [0.0f64; 4];
    for q in 0..4 {
        let (a, b) = (q * 64, (q + 1) * 64);
        let i = (a..b).max_by_key(|&i| hist[i]).unwrap_or(a);
        out[q] = if hist[i] == 0 { f64::NAN }
            else { (vmin as f64 + span as f64 * (i as f64 + 0.5) / 255.0).clamp(vmin as f64, vmax as f64) };
    }
    out
}

/// Weighted mean of 1-, 2- and 3-UI spacings (min spacing as the 1-UI seed).
pub fn unit_interval_runs(spacings: &[f64]) -> Option<f64> {
    let mut s: Vec<f64> = spacings.iter().copied().filter(|x| x.is_finite() && *x > 0.0).collect();
    if s.len() < 2 {
        return None;
    }
    s.sort_by(f64::total_cmp);
    let seed = s[s.len() / 20].max(s[0]);
    if !(seed > 0.0) {
        return None;
    }
    let mut num = 0.0;
    let mut den = 0.0;
    for &x in &s {
        if x < 1.5 * seed {
            num += x;
            den += 1.0;
        } else if x < 2.5 * seed {
            num += x;
            den += 2.0;
        } else if x < 3.5 * seed {
            num += x;
            den += 3.0;
        }
    }
    if den > 0.0 {
        Some(num / den)
    } else {
        eye::unit_interval(&s)
    }
}

/// RMS of each sample minus the mean of its UI bin (fold tightness).
pub fn fold_rms(v: &[f32], dt: f64, ui: f64) -> f64 {
    if !(ui > dt) || v.len() < 8 {
        return f64::NAN;
    }
    let nbin = ((ui / dt).round() as usize).clamp(4, 4096);
    let (mut sum, mut cnt) = (vec![0.0f64; nbin], vec![0u32; nbin]);
    for (i, &x) in v.iter().enumerate() {
        if !x.is_finite() {
            continue;
        }
        let b = ((i as f64 * dt / ui).fract() * nbin as f64).floor() as usize % nbin;
        sum[b] += x as f64;
        cnt[b] += 1;
    }
    let mut e = 0.0;
    let mut n = 0.0;
    for (i, &x) in v.iter().enumerate() {
        if !x.is_finite() {
            continue;
        }
        let b = ((i as f64 * dt / ui).fract() * nbin as f64).floor() as usize % nbin;
        if cnt[b] == 0 {
            continue;
        }
        let m = sum[b] / cnt[b] as f64;
        e += (x as f64 - m).powi(2);
        n += 1.0;
    }
    if n > 0.0 {
        (e / n).sqrt()
    } else {
        f64::NAN
    }
}

/// PRBS of `width` bits, tap mask (x^width + taps + 1). PRBS-11 uses width 11, taps bit 9.
pub fn prbs_bits(width: u32, tap_bit: u32, n: usize) -> Vec<bool> {
    let mask = (1u32 << width) - 1;
    let mut s = mask;
    (0..n)
        .map(|_| {
            let b = (s >> (width - 1)) & 1;
            let t = ((s >> (width - 1)) ^ (s >> (tap_bit - 1))) & 1;
            s = ((s << 1) | t) & mask;
            b != 0
        })
        .collect()
}

pub fn prbs11(n: usize) -> Vec<bool> {
    prbs_bits(11, 9, n)
}

pub fn mac_text(hi32: u32, lo16: u32) -> String {
    let b = [
        (hi32 >> 24) as u8,
        (hi32 >> 16) as u8,
        (hi32 >> 8) as u8,
        hi32 as u8,
        (lo16 >> 8) as u8,
        lo16 as u8,
    ];
    format!("{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}", b[0], b[1], b[2], b[3], b[4], b[5])
}

pub fn ip_text(v: u32) -> String {
    format!("{}.{}.{}.{}", v >> 24, (v >> 16) & 255, (v >> 8) & 255, v & 255)
}

fn pkt(t0: f64, t1: f64, kind: Kind, value: u32, value2: Option<u32>, ok: bool) -> Packet {
    Packet { t0, t1, kind, value, value2, ok }
}

fn bits_to_bytes(bits: &[(f64, bool)]) -> Vec<(f64, f64, u8)> {
    let mut out = Vec::new();
    for chunk in bits.chunks(8) {
        if chunk.len() < 8 {
            break;
        }
        let mut v = 0u8;
        for (k, &(_, b)) in chunk.iter().enumerate() {
            if b {
                v |= 1 << k;
            }
        }
        out.push((chunk[0].0, chunk[7].0, v));
    }
    out
}

fn ethernet_fields(bytes: &[(f64, f64, u8)], out: &mut Vec<Packet>) {
    // look for SFD 0xD5 after 0x55s
    let sfd = bytes.iter().position(|b| b.2 == 0xd5);
    let Some(i) = sfd else { return };
    if i + 15 >= bytes.len() {
        return;
    }
    let b = |k: usize| bytes[i + 1 + k].2 as u32;
    let t0 = bytes[i + 1].0;
    let t1 = bytes[i + 6].1;
    let dst_hi = b(0) << 24 | b(1) << 16 | b(2) << 8 | b(3);
    let dst_lo = b(4) << 8 | b(5);
    out.push(pkt(t0, t1, Kind::Field("MAC"), dst_hi, Some(dst_lo), true));
    let s0 = bytes[i + 7].0;
    let s1 = bytes[i + 12].1;
    let src_hi = b(6) << 24 | b(7) << 16 | b(8) << 8 | b(9);
    let src_lo = b(10) << 8 | b(11);
    out.push(pkt(s0, s1, Kind::Field("SRC"), src_hi, Some(src_lo), true));
    let et = (b(12) << 8 | b(13)) as u16;
    out.push(pkt(bytes[i + 13].0, bytes[i + 14].1, Kind::Field("TYPE"), et as u32, None, true));
    if et == 0x0800 && i + 35 < bytes.len() {
        let sip = b(26) << 24 | b(27) << 16 | b(28) << 8 | b(29);
        let dip = b(30) << 24 | b(31) << 16 | b(32) << 8 | b(33);
        out.push(pkt(bytes[i + 27].0, bytes[i + 30].1, Kind::Field("IP"), sip, None, true));
        out.push(pkt(bytes[i + 31].0, bytes[i + 34].1, Kind::Field("DIP"), dip, None, true));
    }
}

/// The option of the Manchester rate that is Auto: the rate of the signal itself. A Manchester line carries its clock
/// (an edge in the middle of every bit), so its rate need not be one of a list. It comes after the rates of
/// [`BAUDS`], so the entries of a set-up from before mean what they meant (the ui's module lists the choices).
pub const MANCHESTER_AUTO: usize = BAUDS.len();
/// The rate of the list the bit time of a Manchester decoder on Auto is taken from, before a record is read (115200).
const MANCHESTER_USUAL: usize = 7;

/// The bit rate of the Manchester option row 0; 0 = from the signal (`manchester`).
pub fn manchester_rate(opt: &[usize; OPTS]) -> f64 {
    if opt[0] >= MANCHESTER_AUTO {
        0.0
    } else {
        opt_baud(opt, 0)
    }
}

/// Manchester on one line. Options (the ui's module): 0 the rate (`MANCHESTER_AUTO`: from the signal), 1 which edge
/// in the middle of a bit is a one (0 rising, IEEE 802.3; 1 falling, G. E. Thomas), 2 and 3 the sync and header bits
/// of a packet; the shared Data bits are the width of its words.
pub static MANCHESTER: ModuleInfo = ModuleInfo::new("Manchester", "Manchester", "linecode")
    // (with Auto the rate is not known before a record is read: the bit time of the list's usual entry)
    .bit(|opt, _| {
        let rate = manchester_rate(opt);
        1.0 / if rate > 0.0 { rate } else { BAUDS[MANCHESTER_USUAL] }
    })
    .decoder(manchester_lines);

/// The Manchester decoder of the module: a line without a pause is one stream of bytes (with the fields of an
/// Ethernet frame where it has them); a line that is quiet between its packets gives each packet from its mark
/// (`Start`), cut into fields where the options say how (`manchester_fields`), else as its bytes, or as its bits
/// (`Field("PKT")`) where they are no whole bytes.
fn manchester_lines(l: &Lines) -> Result<Vec<Packet>, &'static str> {
    let (rate, other) = (manchester_rate(&l.opt), l.opt[1] == 1);
    let (packets, half) = manchester_packets(l.main(), l.dt, rate, l.thr, l.hy);
    if packets.len() <= 1 {
        let p = manchester(l.main(), l.dt, rate, l.thr, l.hy);
        if !other {
            return Ok(p);
        }
        // the other convention: every bit is the other one. The bytes alone: the fields of an Ethernet frame the
        // decoder finds are those of 802.3
        let data = p.into_iter().filter(|x| x.kind == Kind::Data);
        return Ok(data.map(|x| Packet { value: !x.value & 0xff, ..x }).collect());
    }
    let (sync, header, word) = (l.opt[2], l.opt[3], l.sh.bits as usize);
    let mut out = Vec::new();
    for bits in packets.iter().filter(|b| !b.is_empty()) {
        let bits: Vec<(f64, bool)> = bits.iter().map(|&(t, one)| (t, one != other)).collect();
        let (n, t0) = (bits.len(), bits[0].0 - half);
        out.push(Packet::new(Kind::Start, t0, t0, 0, true));
        if sync + header > 0 {
            out.extend(manchester_fields(&bits, half, sync, header, word));
        } else if n > 32 || n % 8 == 0 {
            out.extend(manchester_fields(&bits[..n - n % 8], half, 0, 0, 8));
        } else {
            let value = bits.iter().fold(0u32, |v, b| v << 1 | b.1 as u32);
            out.push(Packet::field("PKT", t0, bits[n - 1].0 + half, value, true).with_value2(n as u32));
        }
    }
    Ok(out)
}

/// A Manchester packet cut into its fields: `sync` bits, `header` bits, words of `word` bits, and the bits that fill
/// no word as the trailer. A field is its bits with the first one sent as the least significant, as a byte of the
/// line is. `bits`: the middle of each bit (a sample index) and its value; `half`: half a bit time in samples.
pub fn manchester_fields(bits: &[(f64, bool)], half: f64, sync: usize, header: usize, word: usize) -> Vec<Packet> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut take = |kind: Kind, n: usize, counted: bool, out: &mut Vec<Packet>| {
        let n = n.min(bits.len() - at);
        if n == 0 {
            return;
        }
        let f = &bits[at..at + n];
        let value = f.iter().enumerate().fold(0u32, |v, (k, b)| v | (b.1 as u32) << k);
        let value2 = counted.then_some(n as u32);
        out.push(Packet { t0: f[0].0 - half, t1: f[n - 1].0 + half, kind, value, value2, ok: true });
        at += n;
    };
    take(Kind::Field("SYNC"), sync, false, &mut out);
    take(Kind::Field("HDR"), header, false, &mut out);
    let word = word.clamp(1, 32);
    let words = bits.len().saturating_sub(sync + header) / word;
    (0..words).for_each(|_| take(Kind::Data, word, false, &mut out));
    take(Kind::Field("TRL"), word - 1, true, &mut out);
    out
}

/// IEEE 802.3 Manchester: a 1 is low→high in the bit cell (mid-cell rising). `bitrate` 0 = from the edge spacings.
///
/// A line that is quiet between its packets is read packet by packet: edges are never more than a bit time apart
/// inside a packet, so a pause of more than 1.5 bit times ends one, and where the bits lie is found anew in the
/// next (a line of 12-bit packets every 14.5 bit times, as a capture of a real one has it, was read as one stream
/// from its first edge, half a bit off in every second packet). A packet of up to 32 bits that is no whole number
/// of bytes is one `Field("PKT")`: its bits with the first one sent as the most significant, value2 = how many.
/// Else its bytes are `Data` as before, the first bit sent the least significant, and a record without a pause is
/// one stream with the fields of an Ethernet frame where it has them.
pub fn manchester(v: &[f32], dt: f64, bitrate: f64, thr: f32, hyst: f32) -> Vec<Packet> {
    let (packets, half) = manchester_packets(v, dt, bitrate, thr, hyst);
    let bytes_of = |bits: &[(f64, bool)]| -> Vec<Packet> {
        bits_to_bytes(bits).iter().map(|&(s, e, v)| pkt(s, e, Kind::Data, v as u32, None, true)).collect()
    };
    if let [bits] = &packets[..] {
        let mut out = bytes_of(bits);
        ethernet_fields(&bits_to_bytes(bits), &mut out);
        return out;
    }
    let mut out = Vec::new();
    for bits in &packets {
        let n = bits.len();
        if n == 0 || n > 32 || n % 8 == 0 {
            out.extend(bytes_of(bits));
            continue;
        }
        let value = bits.iter().fold(0u32, |v, &(_, one)| v << 1 | one as u32);
        out.push(pkt(bits[0].0 - half, bits[n - 1].0 + half, Kind::Field("PKT"), value, Some(n as u32), true));
    }
    out
}

/// The bits of a Manchester line, packet by packet (see `manchester`): for each stretch between two pauses of more
/// than 1.5 bit times its bits, each with the place of its middle (a sample index) and whether its edge rises; and
/// half a bit time in samples, so a bit lies from its middle less that to its middle plus that. A record without a
/// pause is one stretch; none at all where the line has too few edges or no rate.
pub fn manchester_packets(v: &[f32], dt: f64, bitrate: f64, thr: f32, hyst: f32) -> (Vec<Vec<(f64, bool)>>, f64) {
    let e = edges(v, thr, hyst);
    if e.len() < 4 {
        return (Vec::new(), 0.0);
    }
    let spacings: Vec<f64> = e.windows(2).map(|w| (w[1].t - w[0].t) * dt).collect();
    let half = if bitrate > 0.0 {
        0.5 / bitrate
    } else {
        eye::unit_interval(&spacings).unwrap_or(0.0)
    };
    if !(half > dt) {
        return (Vec::new(), 0.0);
    }
    let bit_t = 2.0 * half;
    let decode_from = |start: usize, end: usize| -> Vec<(f64, bool)> {
        let mut bits = Vec::new();
        let mut t_mid = e[start].t * dt;
        let mut i = start;
        while i < end {
            let t = e[i].t * dt;
            if (t - t_mid).abs() <= 0.35 * bit_t {
                bits.push((e[i].t, e[i].rising));
                t_mid += bit_t;
            } else if t > t_mid + 0.35 * bit_t {
                t_mid += bit_t;
                continue;
            }
            i += 1;
        }
        bits
    };
    // the bits of the edges `start..end`: of the two ways to take them (the first edge in the middle of a bit, or
    // between two), the one in which more cells have their edge
    let bits_of = |start: usize, end: usize| -> Vec<(f64, bool)> {
        let a = decode_from(start, end);
        let b = if end > start + 1 { decode_from(start + 1, end) } else { Vec::new() };
        if b.len() > a.len() { b } else { a }
    };
    // where a pause ends a packet
    let mut cuts: Vec<usize> = (1..e.len()).filter(|&i| (e[i].t - e[i - 1].t) * dt > 1.5 * bit_t).collect();
    cuts.insert(0, 0);
    cuts.push(e.len());
    (cuts.windows(2).map(|w| bits_of(w[0], w[1])).collect(), half / dt)
}

/// MLT-3 on one line. The line has no frames: every byte starts one, as for the UART (Trigger > Bus > Frame never
/// fired).
pub static MLT3: ModuleInfo = ModuleInfo::new("Mlt3", "MLT-3", "linecode")
    .trigger(every_word_is_a_frame)
    .bit(|opt, _| 1.0 / opt_baud(opt, 0))
    .decoder(|l| Ok(mlt3(l.main(), l.dt, opt_baud(&l.opt, 0), l.thr, l.hy)));

/// MLT-3: cycling −A / 0 / +A; every change of level is a 1, a hold is a 0. The line is sliced to its three levels
/// with two comparators at ±A/2 around the middle of its outer levels (the 1st and 99th percentile), each with the
/// hysteresis `hyst` (at most A/2); one threshold sees only half of the changes. `_thr` is not used: the slicing
/// levels come from the signal.
pub fn mlt3(v: &[f32], dt: f64, bitrate: f64, _thr: f32, hyst: f32) -> Vec<Packet> {
    let mut f: Vec<f32> = v.iter().copied().filter(|x| x.is_finite()).collect();
    if f.len() < 8 {
        return Vec::new();
    }
    let (p_lo, p_hi) = (f.len() / 100, f.len() - 1 - f.len() / 100);
    let lo = *f.select_nth_unstable_by(p_lo, f32::total_cmp).1;
    let hi = *f.select_nth_unstable_by(p_hi, f32::total_cmp).1;
    let (mid, a) = (0.5 * (lo + hi), 0.5 * (hi - lo));
    if !(a > 0.0) {
        return Vec::new();
    }
    let h = hyst.clamp(0.0, 0.5 * a);
    // a change of level is a crossing of either comparator, in time order
    let (upper, lower) = (edges(v, mid + 0.5 * a, h), edges(v, mid - 0.5 * a, h));
    let mut e: Vec<f64> = upper.into_iter().chain(lower).map(|x| x.t).collect();
    e.sort_by(f64::total_cmp);
    if e.len() < 3 {
        return Vec::new();
    }
    let spacings: Vec<f64> = e.windows(2).map(|w| (w[1] - w[0]) * dt).collect();
    let ui = if bitrate > 0.0 { 1.0 / bitrate } else { eye::unit_interval(&spacings).unwrap_or(0.0) };
    if !(ui > dt) {
        return Vec::new();
    }
    let mut bits = Vec::new();
    let mut k = 0usize;
    let t0 = e[0] * dt;
    let t_end = e[e.len() - 1] * dt;
    let mut t = t0;
    while t + ui <= t_end + ui {
        while k + 1 < e.len() && e[k + 1] * dt <= t + 0.5 * ui {
            k += 1;
        }
        // the first change, where the bit grid starts, is a 1 as well
        let changed = (e[k] * dt - t).abs() < 0.5 * ui;
        bits.push((t / dt, changed));
        t += ui;
    }
    let bytes = bits_to_bytes(&bits);
    let mut out: Vec<Packet> = bytes.iter().map(|&(s, e, v)| pkt(s, e, Kind::Data, v as u32, None, true)).collect();
    ethernet_fields(&bytes, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pam_quartile_representatives_stay_inside_sample_extrema() {
        for samples in [vec![-1.0, 1.0, 1.0, 1.0], vec![-0.02, 0.404, 0.404, 0.404]] {
            let lo = samples.iter().copied().fold(f32::INFINITY, f32::min);
            let hi = samples.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            let levels = pam4_levels(&samples, lo, hi);
            assert!(levels.iter().filter(|level| level.is_finite()).all(|level| (lo as f64..=hi as f64).contains(level)));
            assert_eq!(levels[3], hi as f64);
            let occupied: Vec<f64> = levels.into_iter().filter(|level| level.is_finite()).collect();
            assert!(occupied.windows(2).all(|pair| pair[0] <= pair[1]));
        }
    }

    #[test]
    fn pam_quartile_constant_signal_does_not_invent_missing_levels() {
        for value in [0.0, 1.0, -0.25, 1e-9] {
            let levels = pam4_levels(&[value; 16], value, value);
            assert_eq!(levels[0], value as f64);
            assert!(levels[1..].iter().all(|level| level.is_nan()));
        }
    }

    #[test]
    fn pam_quartile_binary_signal_has_no_invented_middle_levels() {
        let levels = pam4_levels(&[-1.0, 1.0, -1.0, 1.0], -1.0, 1.0);
        assert!(levels[0].is_finite() && levels[3].is_finite());
        assert!(levels[1].is_nan() && levels[2].is_nan());
    }

    #[test]
    fn fir_keeps_dc_and_kills_nyquist() {
        let dc = vec![1.0f32; 200];
        let y = fir_lowpass(&dc, 1e-6, 1e3);
        assert!((y[100] - 1.0).abs() < 0.05, "{}", y[100]);
        let alt: Vec<f32> = (0..200).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let z = fir_lowpass(&alt, 1e-6, 1e3);
        assert!(z[100].abs() < 0.2, "{}", z[100]);
    }

    #[test]
    fn gates_and_nrzi() {
        let a = vec![0.0, 1.0, 1.0, 0.0];
        let b = vec![0.0, 0.0, 1.0, 1.0];
        assert_eq!(gate(&a, &b, Gate::And), [0.0, 0.0, 1.0, 0.0]);
        assert_eq!(gate(&a, &b, Gate::Xor), [0.0, 1.0, 0.0, 1.0]);
        assert_eq!(not(&a), [1.0, 0.0, 0.0, 1.0]);
        let n = unnrzi(&a);
        assert_eq!(n[0], 1.0);
        assert_eq!(n[1], 0.0);
    }

    #[test]
    fn mac_ip_strings() {
        assert_eq!(mac_text(0x11223344, 0x5566), "11:22:33:44:55:66");
        assert_eq!(ip_text(0x0a000001), "10.0.0.1");
    }

    #[test]
    fn prbs11_is_not_stuck() {
        let b = prbs11(40);
        assert!(b.iter().any(|&x| x) && b.iter().any(|&x| !x));
        assert_eq!(b.len(), 40);
    }

    #[test]
    fn manchester_reads_a_known_byte() {
        // 0xD5 = 10101011 LSB first, 802.3 cells, 1 = LH
        let bit_t = 1e-6;
        let dt = 1e-8;
        let mut v = Vec::new();
        let mut t = 0.0;
        let bits = [1, 0, 1, 0, 1, 0, 1, 1]; // LSB first 0xD5? 1+0+4+0+16+0+64+128 = 213 = 0xD5
        for &b in &bits {
            let n = (bit_t / dt) as usize;
            for k in 0..n {
                let first = (k as f64) < 0.5 * n as f64;
                let high = if b == 1 { !first } else { first };
                v.push(if high { 1.0 } else { 0.0 });
            }
            t += bit_t;
        }
        let p = manchester(&v, dt, 1.0 / bit_t, 0.5, 0.05);
        assert!(p.iter().any(|x| x.kind == Kind::Data && x.value == 0xd5), "{:?}", p);
        let _ = t;
    }

    /// A line of 12-bit packets with 2.5 bit times of quiet line between them: every packet is read from its own
    /// first bit, and is one value with its number of bits. One stream from the first edge had every second packet
    /// half a bit off.
    #[test]
    fn manchester_packets_are_read_one_by_one() {
        let packets = [0x7f2u32, 0x6f2, 0x673, 0x773, 0x6d2];
        let (bit, half) = (16usize, 8usize);
        let mut v = vec![0.0f32; 3 * bit];
        for p in packets {
            for k in (0..12).rev() {
                // a one: low, then high
                let one = p >> k & 1 == 1;
                v.extend(std::iter::repeat_n(if one { 0.0 } else { 1.0 }, half));
                v.extend(std::iter::repeat_n(if one { 1.0 } else { 0.0 }, half));
            }
            // 2.5 bit times of quiet line
            v.extend(std::iter::repeat_n(0.0, 2 * bit + half));
        }
        for rate in [0.0, 1.0 / bit as f64] {
            let p = manchester(&v, 1.0, rate, 0.5, 0.05);
            let whole = p.iter().filter(|x| x.kind == Kind::Field("PKT"));
            let got: Vec<(u32, Option<u32>)> = whole.map(|x| (x.value, x.value2)).collect();
            assert_eq!(got, packets.map(|x| (x, Some(12))), "rate {rate}: {:?}", p);
            assert_eq!(p.len(), 5);
            // a packet lies where its 12 bits are
            let (first, last) = ((3 * bit) as f64, (15 * bit) as f64);
            assert!((p[0].t0 - first).abs() <= 1.0 && (p[0].t1 - last).abs() <= 1.0, "{:?}", p[0]);
        }
    }

    #[test]
    fn mlt3_counts_every_change_of_level() {
        // bytes LSB first, a 1 steps the level 0, +1, 0, -1; 10 samples per bit after one idle bit
        let bytes = [0xffu8, 0xff, 0x55, 0xd5, 0x0f, 0xa3];
        let cycle = [0.0f32, 1.0, 0.0, -1.0];
        let mut v = vec![0.0f32; 10];
        let mut at = 0usize;
        for b in bytes {
            for k in 0..8 {
                if b >> k & 1 == 1 {
                    at += 1;
                }
                v.extend(std::iter::repeat_n(cycle[at % 4], 10));
            }
        }
        for (rate, thr) in [(1e7, 0.0), (1e7, 0.5), (0.0, 0.0)] {
            let p = mlt3(&v, 1e-8, rate, thr, 0.1);
            let got: Vec<u32> = p.iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect();
            assert_eq!(got, bytes.map(u32::from), "rate {rate} threshold {thr}");
        }
    }
}
