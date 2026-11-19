// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Serial protocol decoders on analog channels (threshold + hysteresis): UART, I2C, SPI, CAN, LIN, I2S,
//! MIL-STD-1553. Times in the packets are
//! fractional sample indices like everywhere in oos-dsp.

use crate::edges::{edges, states};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// UART character / SPI word / I2C data byte
    Data,
    /// I2C address byte (value = 7-bit address, `read` flag)
    Addr { read: bool },
    Start,
    RepStart,
    Stop,
    /// CAN identifier (value = 11- or 29-bit ID) / LIN protected identifier (value = 6-bit ID, `ok` = parity)
    Id { ext: bool, rtr: bool },
    /// CAN CRC-15 / LIN checksum (`ok` = it matches)
    Crc,
    /// CAN ACK slot (`ok` = acknowledged, i.e. dominant)
    Ack,
    /// LIN break field
    Break,
    /// I2S audio sample (value = the word, two's complement in `bits`; `right` = right channel)
    Sample { right: bool },
    /// MIL-STD-1553 word (value = 16 bits; `cmd` = command/status sync, else data sync; `ok` = odd parity)
    Word { cmd: bool },
    /// any other labelled field of the extended decoders (`decode_ext`, `decode_ext2`): the label is shown before the
    /// value ("CMD", "ADDR", "ROM", "SOF", "HDR", ...); `ok` = check passed. A label starting with '!' marks an
    /// event without a value (shown as a tag like Start/Stop).
    Field(&'static str),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Packet {
    pub t0: f64,
    pub t1: f64,
    pub kind: Kind,
    pub value: u32,
    /// second value (SPI MISO). I2C: [`I2C_TEN_BIT`] on a 10-bit address, [`I2C_NAK_END`] on the last byte of a read
    pub value2: Option<u32>,
    /// UART: stop bit and parity ok; I2C: acknowledged, or not acknowledged where that is the rule (the byte a
    /// controller ends a read with)
    pub ok: bool,
}

/// `value2` of an I2C address of ten bits: `value` holds the ten bits. A write has it in two bytes on the bus, the
/// read that follows a repeated start in one (its low eight bits are those of the write before it).
pub const I2C_TEN_BIT: u32 = 10;
/// `value2` of the data byte a controller ends a read with: it is not acknowledged, and that is no error (`ok`).
pub const I2C_NAK_END: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parity {
    None,
    Even,
    Odd,
}

/// UART: start bit, `bits` data bits LSB first, parity, 1 stop bit; idle high (`invert` = idle low). Samples each
/// bit in its middle, timed from the start-bit edge.
///
/// A record that begins inside a character shows data bits going low before any start bit, and nothing in those
/// bits says which edge is the first start bit. Unless the record shows the line idle before its first edge for as
/// long as all data bits (inside a character it is never idle that long before it goes low), every edge of the first
/// character time is tried as the first start bit, the readings with the fewest errors stay, and the characters are
/// listed from where those readings agree: the characters before that are left out rather than guessed.
pub fn uart(v: &[f32], dt: f64, baud: f64, bits: u32, parity: Parity, thr: f32, hyst: f32, invert: bool) -> Vec<Packet> {
    let spb = 1.0 / baud / dt;
    if spb < 2.5 {
        return Vec::new();
    }
    let st = states(v, thr, hyst);
    let level = |x: f64| -> Option<bool> { st.get(x as usize).copied().flatten().map(|b| b != invert) };
    let e = edges(v, thr, hyst);
    let nb = bits as f64 + if parity == Parity::None { 0.0 } else { 1.0 };
    // the transitions to the active (low) level that have a start bit's half behind them
    let starts: Vec<f64> = e.iter().filter(|x| x.rising == invert && level(x.t + 0.5 * spb) == Some(false)).map(|x| x.t).collect();
    // the characters when `starts[from]` is the first start bit
    let chain = |from: usize| -> Vec<Packet> {
        let mut out = Vec::new();
        let mut busy_until = 0.0;
        for &s in &starts[from..] {
            // not inside the previous frame
            if s < busy_until {
                continue;
            }
            let mut val = 0u32;
            let mut ones = 0;
            let mut complete = true;
            for b in 0..bits {
                match level(s + (1.5 + b as f64) * spb) {
                    Some(true) => {
                        val |= 1 << b;
                        ones += 1;
                    }
                    Some(false) => {}
                    None => complete = false,
                }
            }
            let par_ok = match parity {
                Parity::None => true,
                p => match level(s + (1.5 + bits as f64) * spb) {
                    Some(pb) => ((ones + pb as u32) % 2 == 0) == (p == Parity::Even),
                    None => {
                        complete = false;
                        false
                    }
                },
            };
            let stop = level(s + (1.5 + nb) * spb);
            if !complete || stop.is_none() {
                break;
            }
            out.push(Packet { t0: s, t1: s + (2.0 + nb) * spb, kind: Kind::Data, value: val, value2: None, ok: stop == Some(true) && par_ok });
            busy_until = s + (1.0 + nb + 0.5) * spb;
        }
        out
    };
    let Some(&first) = starts.first() else { return Vec::new() };
    // since when the record shows the line idle before that edge: its first sample, or the last return to idle
    let idle_since = e.iter().take_while(|x| x.t < first).filter(|x| x.rising != invert).map(|x| x.t).last().or((level(0.0) == Some(true)).then_some(0.0));
    if idle_since.is_some_and(|t| first - t >= (nb - 0.5) * spb) {
        return chain(0);
    }
    // a character the record starts inside ends within one character time: the first start bit is an edge of that
    // time, or the first one after it
    let within = starts.iter().take_while(|&&t| t <= (nb + 2.0) * spb).count();
    let tried = (within + 1).min(starts.len()).min(16);
    let readings: Vec<Vec<Packet>> = (0..tried).map(&chain).collect();
    let errors = |r: &Vec<Packet>| r.iter().filter(|p| !p.ok).count();
    let fewest = readings.iter().map(errors).min().unwrap_or(0);
    let kept: Vec<&Vec<Packet>> = readings.iter().filter(|r| errors(r) == fewest).collect();
    // two readings that share a start bit are the same from there on: the first character of the earliest reading
    // that every other one has too
    let agreed = kept[1..].iter().map(|other| kept[0].iter().position(|p| other.binary_search_by(|q| q.t0.total_cmp(&p.t0)).is_ok())).try_fold(0, |from, at| at.map(|at| from.max(at)));
    match agreed {
        Some(from) => kept[0][from..].to_vec(),
        // readings that never meet (a short record, or one pattern repeated): the earliest stands, as before
        None => kept[0].clone(),
    }
}

/// The messages among decoded fields: runs of fields with no more than the usual idle time between them, as index
/// ranges that cover all fields in order. A start condition (a start of frame) or a break begins a message. So does
/// a field after a gap clearly longer than the usual one: more than 1.75 times the median gap, and more than the
/// median field is long, so that on a line without gaps a pause of one character ends the message.
pub fn bursts(p: &[Packet]) -> Vec<std::ops::Range<usize>> {
    if p.is_empty() {
        return Vec::new();
    }
    let median = |mut v: Vec<f64>| -> f64 {
        if v.is_empty() {
            return 0.0;
        }
        let k = v.len() / 2;
        *v.select_nth_unstable_by(k, f64::total_cmp).1
    };
    let gap = |k: usize| (p[k + 1].t0 - p[k].t1).max(0.0);
    let usual = median((0..p.len() - 1).map(gap).collect());
    let long = median(p.iter().map(|x| x.t1 - x.t0).filter(|&l| l > 0.0).collect());
    let limit = (1.75 * usual).max(long);
    let mut out = Vec::new();
    let mut from = 0;
    for k in 1..p.len() {
        if matches!(p[k].kind, Kind::Start | Kind::Break) || gap(k - 1) > limit {
            out.push(from..k);
            from = k;
        }
    }
    out.push(from..p.len());
    out
}

/// The messages among decoded fields of a protocol that says where a frame begins: a message runs from one such
/// field to the next. Fields before the first of them (a record that starts inside a frame) are a message too.
/// Unlike [`bursts`] no pause splits a frame: a LIN response that comes late, a stretched I2C clock and the idle
/// time inside a USB transaction belong to their frame.
pub fn frames(p: &[Packet], begins: impl Fn(&Packet) -> bool) -> Vec<std::ops::Range<usize>> {
    if p.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut from = 0;
    for k in 1..p.len() {
        if begins(&p[k]) {
            out.push(from..k);
            from = k;
        }
    }
    out.push(from..p.len());
    out
}

/// The bits of a byte `i2c` decoded: eight bits MSB first (of an address its seven bits and the read/write bit) and
/// the acknowledge bit, each around the rising clock edge it was sampled at. A byte's packet runs from its first
/// clock edge to its ninth; the seven between them are taken as evenly spaced, which a clock stretched inside a byte
/// is not (assumed; between bytes, where targets usually stretch, it does not matter).
///
/// A 10-bit address written is two bytes, 11110 with the two high bits and the write bit, then the low eight: 18
/// cells, taken as evenly spaced over both. When one of the two was not acknowledged the packet does not say which:
/// both acknowledge cells show it.
pub fn i2c_bits(p: &Packet) -> Vec<Bit> {
    let ten = p.value2 == Some(I2C_TEN_BIT);
    // the bytes on the bus, and whether the first is an address byte
    let (bytes, address): (Vec<u32>, bool) = match p.kind {
        Kind::Addr { read: false } if ten => (vec![0xf0 | (p.value >> 8 & 3) << 1, p.value & 0xff], true),
        Kind::Addr { read: true } if ten => (vec![0xf0 | (p.value >> 8 & 3) << 1 | 1], true),
        Kind::Addr { read } => (vec![(p.value << 1) | u32::from(read)], true),
        Kind::Data => (vec![p.value], false),
        _ => return Vec::new(),
    };
    let step = (p.t1 - p.t0) / (bytes.len() * 9 - 1) as f64;
    if !(step > 0.0) {
        return Vec::new();
    }
    let cell = |k: usize, kind: BitKind, level: bool, ok: bool| {
        let at = p.t0 + k as f64 * step;
        Bit { t0: at - step / 2.0, t1: at + step / 2.0, kind, level: Some(level), ok }
    };
    // acknowledged is a 0 on the line; the byte that ends a read is not acknowledged and is no error
    let nak = !p.ok || p.value2 == Some(I2C_NAK_END);
    let mut out = Vec::with_capacity(bytes.len() * 9);
    for (n, byte) in bytes.iter().enumerate() {
        for k in 0..8 {
            let kind = if address && n == 0 && k == 7 { BitKind::Rw } else { BitKind::Data };
            out.push(cell(n * 9 + k, kind, (byte >> (7 - k)) & 1 == 1, true));
        }
        out.push(cell(n * 9 + 8, BitKind::Ack, nak, p.ok));
    }
    out
}

/// What a bit of a character is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BitKind {
    Start,
    Data,
    Parity,
    Stop,
    /// I2C: the read/write bit of an address byte (1 = read)
    Rw,
    /// I2C: the acknowledge bit (0 = acknowledged)
    Ack,
}

/// One bit of a field as its decoder read it: from `t0` to `t1` in the time units of its packet, sampled in the
/// middle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bit {
    pub t0: f64,
    pub t1: f64,
    pub kind: BitKind,
    /// the logic value read (a start bit is 0, the idle line and a stop bit are 1, whatever the line's polarity);
    /// `None`: the packet does not say
    pub level: Option<bool>,
    /// false: the bit failed its check, or may be the one that did
    pub ok: bool,
}

/// The bits of a character `uart` decoded with these settings: start bit, data bits LSB first, parity, stop bit, each
/// where `uart` sampled it. The data bits are the character's value. A packet says that its checks passed, not which
/// one failed: without parity a failed character is a stop bit read as 0; with parity the parity bit and the stop
/// bit of a failed character have no level and both are marked.
pub fn uart_bits(p: &Packet, bits: u32, parity: Parity) -> Vec<Bit> {
    let bits = bits.min(32);
    let cells = 2 + bits + u32::from(parity != Parity::None);
    let spb = (p.t1 - p.t0) / cells as f64;
    if p.kind != Kind::Data || !(spb > 0.0) {
        return Vec::new();
    }
    let at = |k: u32| p.t0 + k as f64 * spb;
    let cell = |k: u32, kind: BitKind, level: Option<bool>, ok: bool| Bit { t0: at(k), t1: at(k + 1), kind, level, ok };
    let mut out = vec![cell(0, BitKind::Start, Some(false), true)];
    out.extend((0..bits).map(|b| cell(1 + b, BitKind::Data, Some((p.value >> b) & 1 == 1), true)));
    if parity == Parity::None {
        out.push(cell(1 + bits, BitKind::Stop, Some(p.ok), p.ok));
    } else if p.ok {
        let mask = if bits == 32 { u32::MAX } else { (1u32 << bits) - 1 };
        // the bit that makes the number of ones even (odd)
        let odd = (p.value & mask).count_ones() % 2 == 1;
        out.push(cell(1 + bits, BitKind::Parity, Some(odd == (parity == Parity::Even)), true));
        out.push(cell(2 + bits, BitKind::Stop, Some(true), true));
    } else {
        out.push(cell(1 + bits, BitKind::Parity, None, false));
        out.push(cell(2 + bits, BitKind::Stop, None, false));
    }
    out
}

/// I2C: START/STOP from SDA edges while SCL is high, bits sampled at SCL rising edges, 9th bit = ACK (low).
/// One threshold and hysteresis for both lines. A 10-bit address is one address packet, and the byte a controller
/// ends a read with is no error (`i2c_settle`).
pub fn i2c(sda: &[f32], scl: &[f32], thr: f32, hyst: f32) -> Vec<Packet> {
    i2c_at(sda, scl, thr, hyst, thr, hyst)
}

/// I2C with a threshold and hysteresis of its own on the data line and on the clock.
pub fn i2c_at(sda: &[f32], scl: &[f32], sda_thr: f32, sda_hyst: f32, scl_thr: f32, scl_hyst: f32) -> Vec<Packet> {
    let scl_s = states(scl, scl_thr, scl_hyst);
    let sda_s = states(sda, sda_thr, sda_hyst);
    let mut ev: Vec<(f64, u8)> = Vec::new(); // 0 = SCL rising, 1 = SDA falling, 2 = SDA rising
    ev.extend(edges(scl, scl_thr, scl_hyst).iter().filter(|e| e.rising).map(|e| (e.t, 0)));
    ev.extend(edges(sda, sda_thr, sda_hyst).iter().map(|e| (e.t, if e.rising { 2 } else { 1 })));
    ev.retain(|e| e.0.is_finite());
    ev.sort_by(|a, b| a.0.total_cmp(&b.0));
    let at = |s: &Vec<Option<bool>>, t: f64| s.get((t + 0.5) as usize).copied().flatten().unwrap_or(true);
    let mut out = Vec::new();
    let mut in_frame = false;
    let mut first = true; // next byte is the address
    let (mut bits, mut nbits, mut t0) = (0u32, 0u32, 0.0);
    for (t, k) in ev {
        match k {
            1 | 2 if at(&scl_s, t) => {
                if k == 1 {
                    out.push(Packet { t0: t, t1: t, kind: if in_frame { Kind::RepStart } else { Kind::Start }, value: 0, value2: None, ok: true });
                    in_frame = true;
                    first = true;
                } else {
                    out.push(Packet { t0: t, t1: t, kind: Kind::Stop, value: 0, value2: None, ok: true });
                    in_frame = false;
                }
                nbits = 0;
                bits = 0;
            }
            0 if in_frame => {
                let b = at(&sda_s, t);
                if nbits == 0 {
                    t0 = t;
                }
                if nbits < 8 {
                    bits = bits << 1 | b as u32;
                    nbits += 1;
                } else {
                    let ack = !b;
                    let p = if first {
                        Packet { t0, t1: t, kind: Kind::Addr { read: bits & 1 == 1 }, value: bits >> 1, value2: None, ok: ack }
                    } else {
                        Packet { t0, t1: t, kind: Kind::Data, value: bits, value2: None, ok: ack }
                    };
                    out.push(p);
                    first = false;
                    nbits = 0;
                    bits = 0;
                }
            }
            _ => {}
        }
    }
    i2c_settle(out)
}

/// What the bytes of a transfer mean together, which the bit loop of `i2c_at` cannot know byte by byte:
///
/// - A 10-bit address: the first byte 11110xx0 and the byte after it are one address packet of ten bits
///   ([`I2C_TEN_BIT`]), acknowledged when both were. The read that follows a repeated start sends 11110xx1 alone:
///   it gets the ten bits of the write before it in the same transfer, when its two bits are the same. Such a byte
///   without that write before it stays what it is on the bus, an address of seven bits.
/// - The last byte of a read is not acknowledged by the controller: that is how it ends the read. It is `ok`, with
///   [`I2C_NAK_END`], when its address was acknowledged and a stop, a repeated start or the end of the record
///   follows. Any other byte that was not acknowledged stays an error.
fn i2c_settle(raw: Vec<Packet>) -> Vec<Packet> {
    let mut out: Vec<Packet> = Vec::with_capacity(raw.len());
    // the 10-bit address written in this transfer (kept over repeated starts, until the stop)
    let mut written: Option<u32> = None;
    let mut k = 0;
    while k < raw.len() {
        let p = &raw[k];
        let data_follows = raw.get(k + 1).is_some_and(|d| d.kind == Kind::Data);
        match p.kind {
            Kind::Start | Kind::Stop => written = None,
            Kind::Addr { read: false } if p.value & 0x7c == 0x78 && data_follows => {
                let low = &raw[k + 1];
                let value = (p.value & 3) << 8 | (low.value & 0xff);
                written = Some(value);
                let (value2, ok) = (Some(I2C_TEN_BIT), p.ok && low.ok);
                out.push(Packet { t0: p.t0, t1: low.t1, kind: p.kind, value, value2, ok });
                k += 2;
                continue;
            }
            Kind::Addr { read: true } if p.value & 0x7c == 0x78 => {
                let after_restart = out.last().is_some_and(|x| x.kind == Kind::RepStart);
                if let Some(value) = written.filter(|w| after_restart && w >> 8 == p.value & 3) {
                    out.push(Packet { value, value2: Some(I2C_TEN_BIT), ..p.clone() });
                    k += 1;
                    continue;
                }
            }
            _ => {}
        }
        out.push(p.clone());
        k += 1;
    }
    // the byte that ends a read
    let mut reading = false;
    for k in 0..out.len() {
        match out[k].kind {
            Kind::Start | Kind::RepStart | Kind::Stop => reading = false,
            Kind::Addr { read } => reading = read && out[k].ok,
            Kind::Data if reading && !out[k].ok => {
                let last = out.get(k + 1).is_none_or(|n| matches!(n.kind, Kind::Stop | Kind::RepStart | Kind::Start));
                if last {
                    out[k].ok = true;
                    out[k].value2 = Some(I2C_NAK_END);
                }
            }
            _ => {}
        }
    }
    out
}

/// SPI: words of `bits` bits sampled on the capture edge of `mode` (0/3: rising, 1/2: falling); a word ends after
/// `bits` edges; `cs` (active low) delimits frames, without CS a clock pause longer than `gap` samples restarts.
pub fn spi(clk: &[f32], mosi: &[f32], miso: Option<&[f32]>, cs: Option<&[f32]>, mode: u8, bits: u32, msb_first: bool, thr: f32, hyst: f32, gap: f64) -> Vec<Packet> {
    let sample_rising = matches!(mode, 0 | 3);
    let mo = states(mosi, thr, hyst);
    let mi = miso.map(|m| states(m, thr, hyst));
    let cs_e = cs.map(|c| edges(c, thr, hyst)).unwrap_or_default();
    let cs_s = cs.map(|c| states(c, thr, hyst));
    let at = |s: &Vec<Option<bool>>, t: f64| s.get((t + 0.5) as usize).copied().flatten().unwrap_or(false);
    let mut out = Vec::new();
    let (mut a, mut b, mut n, mut t0, mut last) = (0u32, 0u32, 0u32, 0.0, f64::NEG_INFINITY);
    let push_bit = |w: u32, bit: bool, n: u32| if msb_first { w << 1 | bit as u32 } else { w | (bit as u32) << n };
    for e in edges(clk, thr, hyst) {
        if e.rising != sample_rising {
            continue;
        }
        let selected = cs_s.as_ref().map_or(true, |s| !at(s, e.t));
        if !selected {
            n = 0;
            continue;
        }
        // frame boundary: CS edge or clock pause since the last bit
        let restart = if cs.is_some() { cs_e.iter().any(|c| !c.rising && c.t > last && c.t < e.t) } else { e.t - last > gap };
        if restart {
            n = 0;
        }
        if n == 0 {
            t0 = e.t;
            a = 0;
            b = 0;
        }
        a = push_bit(a, at(&mo, e.t), n);
        if let Some(m) = &mi {
            b = push_bit(b, at(m, e.t), n);
        }
        n += 1;
        last = e.t;
        if n == bits {
            out.push(Packet { t0, t1: e.t, kind: Kind::Data, value: a, value2: mi.as_ref().map(|_| b), ok: true });
            n = 0;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::{i2c_logic, sample, spi_logic, uart_logic, I2cXfer};

    #[test]
    fn uart_8n1_and_parity() {
        let baud = 115200.0;
        let dt = 1.0 / baud / 10.0;
        let msg = b"OOS 0.1";
        let l = uart_logic(msg, baud, 8, None, 1.0, 0.0, 3.3);
        let v = sample(|t| l.v(t), 0.0, dt, 1200);
        let p = uart(&v, dt, baud, 8, Parity::None, 1.65, 0.2, false);
        assert_eq!(p.iter().map(|x| x.value as u8).collect::<Vec<_>>(), msg);
        assert!(p.iter().all(|x| x.ok));
        // even parity, 7 bits
        let l = uart_logic(b"AZ", baud, 7, Some(true), 0.5, 0.0, 3.3);
        let v = sample(|t| l.v(t), 0.0, dt, 400);
        let p = uart(&v, dt, baud, 7, Parity::Even, 1.65, 0.2, false);
        assert_eq!(p.iter().map(|x| (x.value, x.ok)).collect::<Vec<_>>(), [(b'A' as u32, true), (b'Z' as u32, true)]);
        // decoded as odd parity: parity errors
        assert!(uart(&v, dt, baud, 7, Parity::Odd, 1.65, 0.2, false).iter().all(|x| !x.ok));
        // wrong baud rate: framing errors
        assert!(uart(&sample(|t| uart_logic(msg, baud, 8, None, 1.0, 0.0, 3.3).v(t), 0.0, dt, 1200), dt, baud * 1.5, 8, Parity::None, 1.65, 0.2, false).iter().any(|x| !x.ok));
    }

    /// Fields are messages by the idle time between them: a pause clearly longer than the usual one ends a message,
    /// whatever the sender's pace, and a start condition begins one.
    #[test]
    fn bursts_are_split_at_the_long_pauses() {
        let at = |t0: f64, kind: Kind| Packet { t0, t1: t0 + if kind == Kind::Data { 1.0 } else { 0.0 }, kind, value: 0, value2: None, ok: true };
        // a slow sender: a character every 3.3 character times, a pause of 5.5 after the fifth (seen on a unit)
        let slow: Vec<Packet> = [0.0, 3.3, 6.6, 9.9, 13.2, 19.7, 23.0, 26.3].iter().map(|&t| at(t, Kind::Data)).collect();
        assert_eq!(bursts(&slow), [0..5, 5..8]);
        // characters back to back: a pause of one character is enough, half a character is not
        let dense: Vec<Packet> = [0.0, 1.0, 2.0, 3.5, 4.5, 6.6, 7.6].iter().map(|&t| at(t, Kind::Data)).collect();
        assert_eq!(bursts(&dense), [0..5, 5..7]);
        // a start condition begins a message without a pause
        let framed = [at(0.0, Kind::Start), at(0.0, Kind::Data), at(1.0, Kind::Stop), at(1.0, Kind::Start), at(1.0, Kind::Data)];
        assert_eq!(bursts(&framed), [0..3, 3..5]);
        assert!(bursts(&[]).is_empty());
        // a protocol that says where its frames begin: no pause splits one, and what stands before the first is one
        let (data, brk) = (Kind::Data, Kind::Break);
        let late = [at(0.0, data), at(1.0, brk), at(1.0, data), at(9.0, data), at(1.0, brk)];
        assert_eq!(frames(&late, |x| x.kind == Kind::Break), [0..1, 1..4, 4..5]);
        assert!(frames(&[], |_| true).is_empty());
    }

    /// The bits of an I2C byte lie on the clock edges of a decoded transfer: the data line read in the middle of
    /// each cell is the level the cell says, the eighth bit of an address is its read/write bit, the ninth the
    /// acknowledge.
    #[test]
    fn i2c_bits_match_the_lines() {
        let xfer = I2cXfer { addr: 0x50, read: true, data: vec![0xa5], repeated: false, stop: true };
        let (sda, scl) = i2c_logic(&[xfer], 100e3, 0.0, 3.3);
        let dt = 0.25e-6;
        let (d, c) = (sample(|t| sda.v(t), 0.0, dt, 1200), sample(|t| scl.v(t), 0.0, dt, 1200));
        let p = i2c(&d, &c, 1.65, 0.2);
        let bytes: Vec<&Packet> = p.iter().filter(|x| matches!(x.kind, Kind::Addr { .. } | Kind::Data)).collect();
        assert_eq!(bytes.len(), 2, "{:?}", p);
        for b in &bytes {
            let bits = i2c_bits(b);
            assert_eq!(bits.len(), 9);
            for x in &bits {
                let mid = ((x.t0 + x.t1) / 2.0).round() as usize;
                assert_eq!(Some(d[mid] > 1.65), x.level, "{:?} of {:?}", x, b);
            }
        }
        let address = i2c_bits(bytes[0]);
        let levels: Vec<u8> = address.iter().map(|x| u8::from(x.level == Some(true))).collect();
        // 0x50 read: 1010000 1, acknowledged
        assert_eq!(levels, [1, 0, 1, 0, 0, 0, 0, 1, 0]);
        assert_eq!((address[7].kind, address[8].kind, address[8].ok), (BitKind::Rw, BitKind::Ack, true));
        assert_eq!(i2c_bits(bytes[1])[7].kind, BitKind::Data);
        // a start condition has no bits
        assert!(i2c_bits(&p[0]).is_empty());
    }

    /// The bits of a character lie where `uart` sampled them and say what it read.
    #[test]
    fn uart_bits_of_a_character() {
        // 'H' = 0x48 at 8 samples a bit, 8N1
        let p = Packet { t0: 100.0, t1: 180.0, kind: Kind::Data, value: 0x48, value2: None, ok: true };
        let b = uart_bits(&p, 8, Parity::None);
        assert_eq!(b.len(), 10);
        assert_eq!((b[0].kind, b[0].t0, b[0].t1, b[0].level), (BitKind::Start, 100.0, 108.0, Some(false)));
        let data: Vec<_> = b[1..9].iter().map(|x| x.level).collect();
        assert_eq!(data, [0, 0, 0, 1, 0, 0, 1, 0].map(|v| Some(v == 1)));
        let stop = b[9];
        assert_eq!((stop.kind, stop.t0, stop.t1, stop.level, stop.ok), (BitKind::Stop, 172.0, 180.0, Some(true), true));
        // without parity a failed character is its stop bit read as 0
        let failed = *uart_bits(&Packet { ok: false, ..p.clone() }, 8, Parity::None).last().unwrap();
        assert_eq!((failed.level, failed.ok), (Some(false), false));
        // even parity: 0x48 has two ones, so the parity bit is 0
        let p = Packet { t1: 188.0, ..p };
        let b = uart_bits(&p, 8, Parity::Even);
        assert_eq!((b.len(), b[9].kind, b[9].level, b[10].kind), (11, BitKind::Parity, Some(false), BitKind::Stop));
        assert_eq!(uart_bits(&p, 8, Parity::Odd)[9].level, Some(true));
        // a failed character with parity does not say which of the two bits failed
        let b = uart_bits(&Packet { ok: false, ..p }, 8, Parity::Even);
        assert_eq!((b[9].level, b[9].ok, b[10].level, b[10].ok), (None, false, None, false));
    }

    /// The bits lie on the samples of a decoded line: each data bit's middle reads the level the bit says.
    #[test]
    fn uart_bits_match_the_line() {
        let baud = 115200.0;
        let dt = 1.0 / baud / 10.0;
        let l = uart_logic(b"H", baud, 8, None, 1.0, 0.0, 3.3);
        let v = sample(|t| l.v(t), 0.0, dt, 300);
        let p = uart(&v, dt, baud, 8, Parity::None, 1.65, 0.2, false);
        assert_eq!(p.len(), 1);
        for b in uart_bits(&p[0], 8, Parity::None) {
            let mid = ((b.t0 + b.t1) / 2.0) as usize;
            assert_eq!(Some(v[mid] > 1.65), b.level, "{:?}", b);
        }
    }

    // Break it catches: a record that starts inside a character listed false first characters (FF, ED) made of the
    // data bits behind the first falling edge (seen on a unit, 9600 Bd text, 2026-11-17).
    #[test]
    fn uart_lists_no_character_the_start_of_the_record_leaves_open() {
        let baud = 9600.0;
        let dt = 1.0 / baud / 10.0;
        let msg = b"Tektronix\nOOS";
        let l = uart_logic(msg, baud, 8, None, 3.0, 0.0, 3.3);
        let whole = sample(|t| l.v(t), 0.0, dt, 2000);
        let all = uart(&whole, dt, baud, 8, Parity::None, 1.65, 0.2, false);
        assert_eq!(all.iter().map(|x| x.value as u8).collect::<Vec<_>>(), msg);
        // cut inside the first character, at every sample from its start bit to its stop bit: what is listed is the
        // end of the message, without a wrong character in front, and at most two characters go with the cut one
        let first = all[0].t0 as usize;
        for cut in first + 2..first + 98 {
            let p = uart(&whole[cut..], dt, baud, 8, Parity::None, 1.65, 0.2, false);
            let text: Vec<u8> = p.iter().map(|x| x.value as u8).collect();
            assert!(msg.ends_with(&text) && text.len() + 3 >= msg.len(), "cut {} samples into the character: {:?}", cut - first, text);
            assert!(p.iter().all(|x| x.ok));
        }
        // a record that starts in the idle line a character before the first one keeps it
        let p = uart(&whole[first - 90..], dt, baud, 8, Parity::None, 1.65, 0.2, false);
        assert_eq!(p.iter().map(|x| x.value as u8).collect::<Vec<_>>(), msg);
    }

    /// A byte that was not acknowledged is an error, except the one a controller ends a read with: its address was
    /// acknowledged and a stop or a repeated start follows it. On a real bus every read ended in an "error".
    #[test]
    fn i2c_the_byte_that_ends_a_read_is_no_error() {
        let pk = |kind: Kind, value: u32, ok: bool| Packet { t0: 0.0, t1: 1.0, kind, value, value2: None, ok };
        let (rd, wr) = (Kind::Addr { read: true }, Kind::Addr { read: false });
        let settled = |raw: &[Packet]| -> Vec<(bool, Option<u32>)> {
            i2c_settle(raw.to_vec()).iter().filter(|x| x.kind == Kind::Data).map(|x| (x.ok, x.value2)).collect()
        };
        let end = (true, Some(I2C_NAK_END));
        // a read of two bytes, ended by a stop, by a repeated start, and by the end of the record
        let (start, stop) = (pk(Kind::Start, 0, true), pk(Kind::Stop, 0, true));
        let data = |value: u32, ok: bool| pk(Kind::Data, value, ok);
        for last in [Some(Kind::Stop), Some(Kind::RepStart), None] {
            let mut raw = vec![start.clone(), pk(rd, 0x50, true), data(0x10, true), data(0x12, false)];
            raw.extend(last.map(|k| pk(k, 0, true)));
            assert_eq!(settled(&raw), [(true, None), end], "{last:?}");
        }
        // a byte that is not acknowledged in the middle of a read is an error; the last one still ends it
        let raw = [start.clone(), pk(rd, 0x50, true), data(1, false), data(2, false), stop.clone()];
        assert_eq!(settled(&raw), [(false, None), end]);
        // nobody answered the address: what follows is no read that ends well
        let raw = [start.clone(), pk(rd, 0x50, false), data(0xff, false), stop.clone()];
        assert_eq!(settled(&raw), [(false, None)]);
        // a write that is not acknowledged is an error
        let raw = [start.clone(), pk(wr, 0x50, true), data(1, true), data(2, false), stop.clone()];
        assert_eq!(settled(&raw), [(true, None), (false, None)]);
        // the acknowledge cell of the bit row says what was on the line, and is an error only where the byte is one
        let ended = Packet { ok: true, value2: Some(I2C_NAK_END), ..data(0x12, true) };
        let cell = |p: &Packet| {
            let c = i2c_bits(p)[8];
            (c.level, c.ok)
        };
        assert_eq!(cell(&ended), (Some(true), true));
        assert_eq!(cell(&data(0x12, false)), (Some(true), false));
        assert_eq!(cell(&data(0x12, true)), (Some(false), true));
    }

    /// A 10-bit address is one address: written, its two bytes; read after a repeated start, its one byte with the
    /// low bits of the write before it. The cells of its bits lie on the clock edges of both bytes.
    #[test]
    fn i2c_ten_bit_addresses() {
        let dt = 0.25e-6;
        // write to 0x2A5 (11110 10 0, A5): 01; then read from it: 33 44; then a read header alone, to 0x79
        let (sda, scl) = i2c_logic(
            &[
                I2cXfer { addr: 0x7a, read: false, data: vec![0xa5, 0x01], repeated: false, stop: false },
                I2cXfer { addr: 0x7a, read: true, data: vec![0x33, 0x44], repeated: true, stop: true },
                I2cXfer { addr: 0x79, read: true, data: vec![0x55], repeated: false, stop: true },
            ],
            100e3,
            0.0,
            3.3,
        );
        let (d, c) = (sample(|t| sda.v(t), 0.0, dt, 4200), sample(|t| scl.v(t), 0.0, dt, 4200));
        let p = i2c(&d, &c, 1.65, 0.2);
        let k: Vec<(Kind, u32, Option<u32>, bool)> = p.iter().map(|x| (x.kind, x.value, x.value2, x.ok)).collect();
        let ten = Some(I2C_TEN_BIT);
        assert_eq!(
            k,
            [
                (Kind::Start, 0, None, true),
                (Kind::Addr { read: false }, 0x2a5, ten, true),
                (Kind::Data, 0x01, None, true),
                (Kind::RepStart, 0, None, true),
                (Kind::Addr { read: true }, 0x2a5, ten, true),
                (Kind::Data, 0x33, None, true),
                (Kind::Data, 0x44, Some(I2C_NAK_END), true),
                (Kind::Stop, 0, None, true),
                (Kind::Start, 0, None, true),
                // no write before it in this transfer: the byte as it is on the bus
                (Kind::Addr { read: true }, 0x79, None, true),
                (Kind::Data, 0x55, Some(I2C_NAK_END), true),
                (Kind::Stop, 0, None, true),
            ]
        );
        // the written address: 18 cells over its two bytes, each what the data line is in its middle
        let cells = i2c_bits(&p[1]);
        assert_eq!(cells.len(), 18);
        for x in &cells {
            let mid = ((x.t0 + x.t1) / 2.0).round() as usize;
            assert_eq!(Some(d[mid] > 1.65), x.level, "{:?}", x);
        }
        let levels: Vec<u8> = cells.iter().map(|x| u8::from(x.level == Some(true))).collect();
        assert_eq!(levels, [1, 1, 1, 1, 0, 1, 0, 0, 0, 1, 0, 1, 0, 0, 1, 0, 1, 0]);
        let kinds: Vec<BitKind> = cells.iter().map(|x| x.kind).collect();
        let (rw, ack, data) = (BitKind::Rw, BitKind::Ack, BitKind::Data);
        assert_eq!((kinds[7], kinds[8], kinds[16], kinds[17]), (rw, ack, data, ack));
        // the read header: one byte, 11110 10 1 and its acknowledge
        let cells = i2c_bits(&p[4]);
        let levels: Vec<u8> = cells.iter().map(|x| u8::from(x.level == Some(true))).collect();
        assert_eq!(levels, [1, 1, 1, 1, 0, 1, 0, 1, 0]);
        for x in &cells {
            let mid = ((x.t0 + x.t1) / 2.0).round() as usize;
            assert_eq!(Some(d[mid] > 1.65), x.level, "{:?}", x);
        }
    }

    #[test]
    fn i2c_write_then_read() {
        let rate = 100e3;
        let dt = 0.25e-6;
        let (sda, scl) = i2c_logic(
            &[
                I2cXfer { addr: 0x50, read: false, data: vec![0x00, 0x12], repeated: false, stop: false },
                I2cXfer { addr: 0x50, read: true, data: vec![0xab, 0xcd], repeated: true, stop: true },
            ],
            rate,
            0.0,
            3.3,
        );
        let a = sample(|t| sda.v(t), 0.0, dt, 2400);
        let c = sample(|t| scl.v(t), 0.0, dt, 2400);
        let p = i2c(&a, &c, 1.65, 0.3);
        let k: Vec<(Kind, u32, bool)> = p.iter().map(|x| (x.kind, x.value, x.ok)).collect();
        assert_eq!(
            k,
            [
                (Kind::Start, 0, true),
                (Kind::Addr { read: false }, 0x50, true),
                (Kind::Data, 0x00, true),
                (Kind::Data, 0x12, true),
                (Kind::RepStart, 0, true),
                (Kind::Addr { read: true }, 0x50, true),
                (Kind::Data, 0xab, true),
                (Kind::Data, 0xcd, true), // the controller does not acknowledge the last byte it reads: no error
                (Kind::Stop, 0, true),
            ]
        );
    }

    /// Clock and data can sit at different levels. A threshold that fits the data misses a smaller clock.
    #[test]
    fn i2c_separate_thresholds() {
        let rate = 100e3;
        let dt = 0.25e-6;
        let (sda, scl) = i2c_logic(
            &[I2cXfer { addr: 0x50, read: false, data: vec![0x42], repeated: false, stop: true }],
            rate,
            0.0,
            3.3,
        );
        let a = sample(|t| sda.v(t), 0.0, dt, 1600);
        let c: Vec<f32> = sample(|t| scl.v(t), 0.0, dt, 1600).into_iter().map(|v| v / 3.3).collect();
        let p = i2c_at(&a, &c, 1.65, 0.2, 0.4, 0.1);
        let k: Vec<(Kind, u32, bool)> = p.iter().map(|x| (x.kind, x.value, x.ok)).collect();
        assert_eq!(
            k,
            [
                (Kind::Start, 0, true),
                (Kind::Addr { read: false }, 0x50, true),
                (Kind::Data, 0x42, true),
                (Kind::Stop, 0, true),
            ]
        );
        assert!(i2c(&a, &c, 1.65, 0.2).iter().all(|x| !matches!(x.kind, Kind::Addr { .. } | Kind::Data)));
    }

    #[test]
    fn spi_all_modes() {
        let words = [(0x9f, 0xff), (0x00, 0xef), (0x5a, 0x40)];
        for mode in 0..4u8 {
            let [cs, clk, mosi, miso] = spi_logic(&words, 8, mode, 1e6, 0.0, 3.3);
            let dt = 25e-9;
            let s = |l: &crate::sim::Logic| sample(|t| l.v(t), 0.0, dt, 2400);
            let (vc, vk, vo, vi) = (s(&cs), s(&clk), s(&mosi), s(&miso));
            let p = spi(&vk, &vo, Some(&vi), Some(&vc), mode, 8, true, 1.65, 0.3, 1e9);
            assert_eq!(p.iter().map(|x| (x.value, x.value2.unwrap())).collect::<Vec<_>>(), words, "mode {}", mode);
            // without CS: frames from the clock alone
            let p = spi(&vk, &vo, None, None, mode, 8, true, 1.65, 0.3, 100.0);
            assert_eq!(p.iter().map(|x| x.value).collect::<Vec<_>>(), [0x9f, 0x00, 0x5a], "mode {} no CS", mode);
        }
    }
}

/// CRC-15/CAN (polynomial 0x4599) over a bit stream (SOF .. last data bit, unstuffed).
pub fn can_crc15(bits: &[bool]) -> u16 {
    let mut crc: u16 = 0;
    for &b in bits {
        let top = (crc >> 14) & 1 == 1;
        crc = (crc << 1) & 0x7fff;
        if top != b {
            crc ^= 0x4599;
        }
    }
    crc
}

/// CAN 2.0A/B data and remote frames at `bitrate`. `invert` = false: logic level as on the RX/TX pin (dominant low,
/// idle high); true for CAN_H probed against ground (dominant high). Hard sync on the start of frame after at least 7
/// recessive bit times, resynchronisation on every edge from recessive to dominant, sampling at 70 % of the bit, bit
/// de-stuffing, CRC check. The edge to dominant is the one a transceiver drives and a node synchronises on; the way
/// back is the bus letting go, and on a line that is slow there it crosses the threshold late (seen on a unit:
/// 1.1 us late on a bus at 500 kbit/s, and with a resynchronisation on it the next bit was sampled in its neighbour).
///
/// Six to twelve dominant bit times in a row are an error flag (or an overload flag), which no frame holds: where no
/// frame is being read they are Field("!ERROR FRAME"), in a record that holds a frame with the right CRC
/// (`can_keep_flags`).
pub fn can(v: &[f32], dt: f64, bitrate: f64, thr: f32, hyst: f32, invert: bool) -> Vec<Packet> {
    can_frames(v, dt, bitrate, thr, hyst, invert, false)
}

/// `can`, and of a frame that the end of the record cuts off the fields that are whole: its start, its identifier
/// and the data bytes read so far. For the protocol trigger, which has to find a field in a record that is shorter
/// than its frame (seen on a unit: a record of 56 us at 5 us/div, a frame of 112 us, and no trigger on its
/// identifier).
pub fn can_cut(v: &[f32], dt: f64, bitrate: f64, thr: f32, hyst: f32, invert: bool) -> Vec<Packet> {
    can_frames(v, dt, bitrate, thr, hyst, invert, true)
}

fn can_frames(v: &[f32], dt: f64, bitrate: f64, thr: f32, hyst: f32, invert: bool, cut: bool) -> Vec<Packet> {
    let spb = 1.0 / bitrate / dt;
    let mut out = Vec::new();
    if spb < 3.0 {
        return out;
    }
    let st = states(v, thr, hyst);
    // recessive = true
    let rec = |x: f64| -> Option<bool> { st.get(x as usize).copied().flatten().map(|b| b != invert) };
    let e = edges(v, thr, hyst);
    // the edges to dominant: the bit timing follows these alone
    let dominant: Vec<f64> = e.iter().filter(|x| x.rising == invert).map(|x| x.t).collect();
    let edge_near = |from: f64, to: f64| -> Option<f64> {
        let i = dominant.partition_point(|&t| t < from);
        dominant.get(i).copied().filter(|&t| t < to)
    };
    let mut busy_until = 0.0;
    let mut last_rec_start: Option<f64> = None;
    for (k, x) in e.iter().enumerate() {
        // a recessive run starts at a recessive-going edge
        let to_rec = x.rising != invert;
        if to_rec {
            last_rec_start = Some(x.t);
            continue;
        }
        if x.t < busy_until {
            continue;
        }
        // (dominant where a sixth bit is sampled too: five bits behind which the line is slow back to recessive
        // last longer than five and a half bit times, and are no flag)
        let sixth = rec(x.t + 5.7 * spb) == Some(false);
        if let Some(end) = e.get(k + 1).map(|n| n.t).filter(|&end| sixth && can_error_flag((end - x.t) / spb)) {
            out.push(Packet { t0: x.t, t1: end, kind: Kind::Field("!ERROR FRAME"), value: 0, value2: None, ok: false });
            busy_until = end;
            continue;
        }
        // SOF: dominant edge after >= 7 recessive bit times (or the record start when the bus was idle before)
        let idle_from = last_rec_start.unwrap_or(if k == 0 && rec(x.t - 1.0) == Some(true) { 0.0 } else { f64::INFINITY });
        if x.t - idle_from < 7.0 * spb {
            continue;
        }
        if let Some((pk, end)) = can_frame(x.t, spb, &rec, &edge_near, cut) {
            busy_until = end;
            out.extend(pk);
        }
    }
    can_keep_flags(&mut out);
    out
}

/// What is whole of a CAN frame that the record cuts off, from its de-stuffed bits so far (`can_cut`): start and
/// identifier once the format is known, and the data bytes read. None where `cut` is off or the identifier is not
/// whole.
fn can_cut_off(cut: bool, bits: &[(bool, f64, f64)], ext: bool, known: bool) -> Option<(Vec<Packet>, f64)> {
    let n = bits.len();
    if !cut || n < 14 || (ext && n < 33) {
        return None;
    }
    let val = |r: std::ops::Range<usize>| bits[r].iter().fold(0u32, |a, b| a << 1 | b.0 as u32);
    let pk = |kind, a: usize, b: usize, value: u32| {
        let (t0, t1) = (bits[a].1, bits[b].2);
        Packet { t0, t1, kind, value, value2: None, ok: true }
    };
    let (id, rtr, dlc_at, id_end) = match ext {
        true => (val(1..12) << 18 | val(14..32), bits[32].0, 35, 31),
        false => (val(1..12), bits[12].0, 15, 11),
    };
    let mut out = vec![pk(Kind::Start, 0, 0, 0), pk(Kind::Id { ext, rtr }, 1, id_end, id)];
    if known && !rtr {
        let data_at = dlc_at + 4;
        for i in (0..val(dlc_at..data_at).min(8) as usize).take_while(|i| data_at + 8 * i + 8 <= n) {
            out.push(pk(Kind::Data, data_at + 8 * i, data_at + 8 * i + 7, val(data_at + 8 * i..data_at + 8 * i + 8)));
        }
    }
    Some((out, bits[n - 1].2))
}

/// One CAN frame from its SOF edge at `sof`: the packets and the time after the ACK delimiter. `cut`: of a frame
/// that the record cuts off, what `can_cut_off` gives.
fn can_frame(
    sof: f64,
    spb: f64,
    rec: &dyn Fn(f64) -> Option<bool>,
    edge_near: &dyn Fn(f64, f64) -> Option<f64>,
    cut: bool,
) -> Option<(Vec<Packet>, f64)> {
    let mut start = sof;
    let mut raw: Vec<(bool, f64, f64)> = Vec::new(); // (level, bit start, bit end)
    let mut next = || -> Option<(bool, f64, f64)> {
        let b = rec(start + 0.7 * spb)?;
        let s = start;
        let nominal = start + spb;
        start = match edge_near(start + 0.5 * spb, start + 1.5 * spb) {
            Some(te) if (te - nominal).abs() < 0.5 * spb => te,
            _ => nominal,
        };
        Some((b, s, start))
    };
    // de-stuffed bits with their times
    let mut bits: Vec<(bool, f64, f64)> = Vec::new();
    let (mut run, mut last) = (0, None::<bool>);
    let mut need = 19usize; // SOF + ID 11 + RTR/SRR + IDE + r0 + DLC 4, grows when the header is known
    let (mut ext, mut known) = (false, false);
    while bits.len() < need {
        let Some(b) = next() else { return can_cut_off(cut, &bits, ext, known) };
        raw.push(b);
        if run == 5 {
            // stuff bit: must be the complement, not part of the data
            if Some(b.0) == last {
                return None; // stuff error
            }
            run = 1;
            last = Some(b.0);
            continue;
        }
        if Some(b.0) == last {
            run += 1;
        } else {
            run = 1;
            last = Some(b.0);
        }
        bits.push(b);
        if !known && bits.len() == 14 {
            // bit 13 = IDE (recessive = extended)
            ext = bits[13].0;
            if ext {
                need = 1 + 11 + 1 + 1 + 18 + 1 + 2 + 4;
            }
        }
        if !known && bits.len() == need {
            known = true;
            let dlc_at = need - 4;
            let dlc = bits[dlc_at..need].iter().fold(0u32, |a, b| a << 1 | b.0 as u32);
            let rtr = if ext { bits[32].0 } else { bits[12].0 };
            let n = if rtr { 0 } else { dlc.min(8) as usize };
            need += 8 * n + 15;
        }
    }
    let val = |r: std::ops::Range<usize>| bits[r].iter().fold(0u32, |a, b| a << 1 | b.0 as u32);
    let (id, rtr, dlc_at) = if ext { (val(1..12) << 18 | val(14..32), bits[32].0, 35) } else { (val(1..12), bits[12].0, 15) };
    let dlc = val(dlc_at..dlc_at + 4);
    let n = if rtr { 0 } else { dlc.min(8) as usize };
    let data_at = dlc_at + 4;
    let crc_at = data_at + 8 * n;
    let crc_calc = can_crc15(&bits[..crc_at].iter().map(|b| b.0).collect::<Vec<_>>());
    let crc_rx = val(crc_at..crc_at + 15) as u16;
    // stuffing covers SOF .. CRC sequence: a run of five at the end of the CRC is followed by one more stuff bit
    if run == 5 && next().is_none() {
        return can_cut_off(cut, &bits, ext, known);
    }
    // CRC delimiter, ACK slot, ACK delimiter are not stuffed
    let (Some(_), Some(ack), Some(ack_del)) = (next(), next(), next()) else {
        return can_cut_off(cut, &bits, ext, known);
    };
    let pk = |kind, a: usize, b: usize, value: u32, ok: bool| Packet { t0: bits[a].1, t1: bits[b].2, kind, value, value2: None, ok };
    let mut out = vec![Packet { t0: bits[0].1, t1: bits[0].2, kind: Kind::Start, value: 0, value2: None, ok: true }];
    let id_end = if ext { 31 } else { 11 };
    out.push(pk(Kind::Id { ext, rtr }, 1, id_end, id, true));
    for i in 0..n {
        out.push(pk(Kind::Data, data_at + 8 * i, data_at + 8 * i + 7, val(data_at + 8 * i..data_at + 8 * i + 8), true));
    }
    out.push(pk(Kind::Crc, crc_at, crc_at + 14, crc_rx as u32, crc_rx == crc_calc));
    out.push(Packet { t0: ack.1, t1: ack.2, kind: Kind::Ack, value: 0, value2: None, ok: !ack.0 });
    Some((out, ack_del.2))
}

/// The dominant time, in bit times, of an error or overload flag on a CAN bus: a frame has five equal bits in a row
/// at most, a flag has six, and twelve where the flags of several nodes follow one another.
pub const CAN_ERROR_FLAG: std::ops::RangeInclusive<f64> = 5.5..=12.6;

/// Whether a dominant pulse of `bits` bit times is an error flag.
pub fn can_error_flag(bits: f64) -> bool {
    CAN_ERROR_FLAG.contains(&bits)
}

/// Error flags count only in a record that holds a frame with the right CRC: a line that carries something else
/// than CAN, or CAN at another rate, has long low pulses too, and each of them would be an error frame (and fire a
/// trigger on errors). Without such a frame the flags leave the list.
pub fn can_keep_flags(out: &mut Vec<Packet>) {
    if !out.iter().any(|p| p.kind == Kind::Crc && p.ok) {
        out.retain(|p| p.kind != Kind::Field("!ERROR FRAME"));
    }
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
        out.push(Packet { t0: b0, t1: b1, kind: Kind::Break, value: rate, value2: None, ok: true });
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
            bytes.push(Packet { t0: s, t1: s + 10.0 * bit, kind: Kind::Data, value, value2: None, ok: bits[8] });
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
            LinChecksum::Enhanced => if id >= 0x3c { classic } else { enhanced },
        };
        out.push(Packet { kind: Kind::Crc, ok: cs.ok && sum_ok, ..cs.clone() });
    }
    out
}

/// I2S (Philips): SD sampled on the rising SCK edge, MSB first, the word starts one SCK period after the WS edge
/// (WS low = left, high = right). `bits` = word length to keep (MSB-aligned; longer slots are cut, shorter ones end at
/// the next WS edge). Without `ws` the words are cut every `bits` clocks (left only).
pub fn i2s(sck: &[f32], ws: Option<&[f32]>, sd: &[f32], bits: u32, thr: f32, hyst: f32) -> Vec<Packet> {
    let mut out = Vec::new();
    let ce: Vec<f64> = edges(sck, thr, hyst).into_iter().filter(|e| e.rising).map(|e| e.t).collect();
    let sds = states(sd, thr, hyst);
    let wss = ws.map(|w| states(w, thr, hyst));
    let level = |st: &Vec<Option<bool>>, t: f64| st.get(t as usize).copied().flatten();
    // per rising clock edge: WS level and SD bit
    let mut cur: Option<(bool, f64, u32, u32)> = None; // (right, t0, value, bits so far)
    let mut prev_ws: Option<bool> = None;
    let flush = |cur: &mut Option<(bool, f64, u32, u32)>, t1: f64, out: &mut Vec<Packet>| {
        if let Some((right, t0, v, n)) = cur.take() {
            if n > 0 {
                // MSB-aligned word of `bits`; shorter slots are padded with zeros
                let v = if n < bits { v << (bits - n) } else { v };
                out.push(Packet { t0, t1, kind: Kind::Sample { right }, value: v, value2: None, ok: n >= bits });
            }
        }
    };
    for (k, &t) in ce.iter().enumerate() {
        let w = match &wss {
            Some(st) => level(st, t),
            None => Some(false),
        };
        let Some(w) = w else { continue };
        let Some(b) = level(&sds, t) else { continue };
        let period = if k > 0 { t - ce[k - 1] } else { 0.0 };
        if wss.is_some() {
            if prev_ws.is_some() && prev_ws != Some(w) {
                // WS changed: the previous word ends with this bit (the one-clock delay), the new word starts next
                if let Some(c) = cur.as_mut() {
                    if c.3 < bits {
                        c.2 = c.2 << 1 | b as u32;
                    }
                    c.3 += 1;
                }
                flush(&mut cur, t + period / 2.0, &mut out);
                cur = Some((w, t + period / 2.0, 0, 0));
                prev_ws = Some(w);
                continue;
            }
            prev_ws = Some(w);
            if let Some(c) = cur.as_mut() {
                if c.3 < bits {
                    c.2 = c.2 << 1 | b as u32;
                }
                c.3 += 1;
            }
        } else {
            let c = cur.get_or_insert((false, t - period / 2.0, 0, 0));
            c.2 = c.2 << 1 | b as u32;
            c.3 += 1;
            if c.3 == bits {
                flush(&mut cur, t + period / 2.0, &mut out);
            }
        }
    }
    // the last word has no WS edge after it: keep it when complete
    if cur.map_or(false, |c| c.3 >= bits) {
        let t1 = ce.last().copied().unwrap_or(0.0);
        flush(&mut cur, t1, &mut out);
    }
    out
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
        let driven = |t: f64, lv: bool| v.get(t as usize).map_or(false, |&x| if lv { x > thr + hyst } else { x < thr - hyst });
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
                    (Some(h1), Some(h2)) => {
                        ok &= h1 != h2;
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
        let kind = Kind::Word { cmd: first_pos };
        out.push(Packet { t0: mid - 1.5 * us, t1: end, kind, value: val, value2: None, ok });
        busy_until = end - 0.5 * us;
    }
    mil1553_roles(&mut out, us);
    out
}
