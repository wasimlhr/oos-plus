// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! UART (with automatic bit rate and 9-bit multidrop / RS-485), I2C and SPI.
//!
//! A new decoder of the family goes into this file, a new family into a file of its own next to it (with its `mod` and
//! `pub use` line in `decode.rs`); the ui reaches it through a protocol module registered in `ui/src/protocols/mod.rs`
//! (ADR 0002).

use super::{Kind, Packet, Parity, I2C_NAK_END, I2C_TEN_BIT};
use crate::edges::{edges, states};
use crate::protocols::{every_word_is_a_frame, BusWhat, Lines, Mirror, ModuleInfo, OPTS};
use crate::trigger::Slope;

/// The baud rates of the Decode menu, as the options of a module choose them.
pub const BAUDS: [f64; 10] = [1200.0, 2400.0, 4800.0, 9600.0, 19200.0, 38400.0, 57600.0, 115200.0, 460800.0, 921600.0];

/// The baud rate of option row `k` ([`BAUDS`]).
pub fn opt_baud(opt: &[usize; OPTS], k: usize) -> f64 {
    BAUDS[opt[k].min(BAUDS.len() - 1)]
}

/// UART on one line or an RS-485 pair.
pub static UART: ModuleInfo = ModuleInfo::new("Uart", "UART", "serial")
    .differential()
    .trigger(every_word_is_a_frame)
    .bit(|_, sh| 1.0 / sh.baud)
    .decoder(|l| Ok(uart(l.main(), l.dt, l.sh.baud, l.sh.bits, l.sh.parity, l.thr, l.hy, l.sh.uart_inv)));

/// The most characters at the start of a record that `uart` leaves out because the record begins inside one.
pub const UART_LEFT_OUT: usize = 4;

/// UART: start bit, `bits` data bits LSB first, parity, 1 stop bit; idle high (`invert` = idle low). Samples each
/// bit in its middle, timed from the start-bit edge.
///
/// A record that begins inside a character shows data bits going low before any start bit, and nothing in those bits
/// says which edge is the first start bit. Unless the record shows the line idle before its first edge for as long as
/// all data bits (inside a character it is never idle that long before it goes low), every edge of the first
/// character time is tried as the first start bit, and the readings with the fewest errors stay. The characters are
/// listed from where those readings agree, and the ones before are left out rather than guessed, at most
/// `UART_LEFT_OUT`. When the record begins on the idle level, the reading from its first edge is among them, and its
/// first characters keep the beat of the regular stream after that point, they are real and stay.
pub fn uart(
    v: &[f32],
    dt: f64,
    baud: f64,
    bits: u32,
    parity: Parity,
    thr: f32,
    hyst: f32,
    invert: bool,
) -> Vec<Packet> {
    let spb = 1.0 / baud / dt;
    if spb < 2.5 {
        return Vec::new();
    }
    let st = states(v, thr, hyst);
    let level = |x: f64| -> Option<bool> { st.get(x as usize).copied().flatten().map(|b| b != invert) };
    let e = edges(v, thr, hyst);
    let nb = bits as f64 + if parity == Parity::None { 0.0 } else { 1.0 };
    // the transitions to the active (low) level that have a start bit's half behind them
    let starts: Vec<f64> =
        e.iter().filter(|x| x.rising == invert && level(x.t + 0.5 * spb) == Some(false)).map(|x| x.t).collect();
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
                    Some(pb) => (ones + pb as u32).is_multiple_of(2) == (p == Parity::Even),
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
            out.push(Packet::new(Kind::Data, s, s + (2.0 + nb) * spb, val, stop == Some(true) && par_ok));
            busy_until = s + (1.0 + nb + 0.5) * spb;
        }
        out
    };
    let Some(&first) = starts.first() else { return Vec::new() };
    // since when the record shows the line idle before that edge: its first sample, or the last return to idle
    let idle_since = e
        .iter()
        .take_while(|x| x.t < first)
        .filter(|x| x.rising != invert)
        .map(|x| x.t)
        .last()
        .or((level(0.0) == Some(true)).then_some(0.0));
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
    let agreed = kept[1..]
        .iter()
        .map(|other| kept[0].iter().position(|p| other.binary_search_by(|q| q.t0.total_cmp(&p.t0)).is_ok()))
        .try_fold(0, |from, at| at.map(|at| from.max(at)));
    // The line idle at the start of the record: its first edge is as likely the start bit of a character as a data bit
    // of one the record cut into. When the reading from it is among the best and its first characters keep the beat of
    // the regular stream behind the point where the readings agree, they are real and stay: a clean record that starts
    // a few bit times before `Hello` lists it whole, where dropping the characters before that point listed `llo`.
    if level(0.0) == Some(true)
        && errors(&readings[0]) == fewest
        && agreed.is_some_and(|from| from > 0 && uart_on_beat(&readings[0], from, spb))
    {
        return readings[0].clone();
    }
    match agreed {
        Some(from) => kept[0][from.min(UART_LEFT_OUT)..].to_vec(),
        // readings that never meet (a short record, or one pattern repeated): the earliest stands, as before
        None => kept[0].clone(),
    }
}

/// The characters of `r` before `from` keep the beat of the characters after it: those come at one distance (a
/// regular stream, half a bit of tolerance), and each one before lies a whole number of those distances earlier.
fn uart_on_beat(r: &[Packet], from: usize, spb: f64) -> bool {
    let tail = &r[from..];
    if tail.len() < 3 {
        return false;
    }
    let d = tail.windows(2).take(from + 2).map(|w| w[1].t0 - w[0].t0).fold(f64::INFINITY, f64::min);
    let regular = tail.windows(2).take(from + 2).all(|w| (w[1].t0 - w[0].t0 - d).abs() <= 0.5 * spb);
    regular && (0..from).all(|k| (r[k].t0 - (tail[0].t0 - (from - k) as f64 * d)).abs() <= 0.5 * spb)
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
        Kind::Addr { read: false } if ten => (vec![0xf0 | ((p.value >> 8) & 3) << 1, p.value & 0xff], true),
        Kind::Addr { read: true } if ten => (vec![0xf0 | ((p.value >> 8) & 3) << 1 | 1], true),
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

/// I2C on SDA and SCL.
pub static I2C: ModuleInfo = ModuleInfo::new("I2c", "I2C", "serial")
    .roles([Some("SDA"), Some("SCL"), None, None])
    .starts_on(BusWhat::Id, 0x50)
    .bit(|_, _| 2.5e-6)
    .decoder(|l| Ok(i2c(l.main(), l.need(1, "source channel off")?, l.thr, l.hy)));

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
                    out.push(Packet::new(if in_frame { Kind::RepStart } else { Kind::Start }, t, t, 0, true));
                    in_frame = true;
                    first = true;
                } else {
                    out.push(Packet::new(Kind::Stop, t, t, 0, true));
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
                        Packet::new(Kind::Addr { read: bits & 1 == 1 }, t0, t, bits >> 1, ack)
                    } else {
                        Packet::new(Kind::Data, t0, t, bits, ack)
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

/// SPI: data, clock and an optional chip select.
pub static SPI: ModuleInfo = ModuleInfo::new("Spi", "SPI", "serial")
    .roles([Some("MOSI"), Some("Clock"), Some("CS"), Some("MISO")])
    .trigger(every_word_is_a_frame)
    // a frame starts with the chip select
    .starts_with(2, Slope::Falling)
    .bit(|_, _| 0.5e-6)
    .decoder(spi_lines);

/// What the band of an SPI decoder says where most frames end before a word is whole: the record has too few samples
/// for the clock (a slower time base at the same memory), or the frames are shorter than the Data bits set.
pub const SPI_CUT: &str = "Frames shorter than a word: too few samples for the clock, or Data bits";

/// SPI on the lines of its module, with the shared rows: a clock pause of the trigger's timeout starts a frame (else
/// 20 samples), and an active-high CS is mirrored at the threshold, since `spi` takes CS active low. The options
/// (the ui's protocol module): 0 bit order (MSB first, LSB first), 1 data polarity (active high, active low). A
/// second data line (MISO, role 3) with a source makes a word MOSI/MISO.
fn spi_lines(l: &Lines) -> Result<Vec<Packet>, &'static str> {
    let clk = l.need(1, "source channel off")?;
    let bit = 1e-9f64.max(l.dt);
    let gap = if l.sh.spi_timeout > 0.0 { l.sh.spi_timeout / l.dt } else { 20.0 * bit / l.dt };
    let cs_hi: Option<Vec<f32>> = match (l.sh.spi_cs_high, l.lines[2]) {
        (true, Some(c)) => Some(c.iter().map(|&x| 2.0 * l.thr - x).collect()),
        _ => None,
    };
    let cs = cs_hi.as_deref().or(l.lines[2]);
    // the second data line, where it has a source: a word is then MOSI/MISO
    let mut p = spi(clk, l.main(), l.lines[3], cs, l.sh.spi_mode, l.sh.bits, l.opt[0] == 0, l.thr, l.hy, gap);
    if l.opt[1] == 1 {
        // a low level is a 1: every bit of a word the other way round
        let mask = if l.sh.bits >= 32 { u32::MAX } else { (1u32 << l.sh.bits) - 1 };
        for x in p.iter_mut().filter(|x| x.kind == Kind::Data) {
            x.value = !x.value & mask;
            x.value2 = x.value2.map(|v| !v & mask);
        }
    }
    // a frame starts where the chip select goes active: a start mark there makes each frame a message of its
    // own, as a start condition does for I2C. Without a chip select the words are messages by their pauses
    if let Some(c) = cs {
        let es = edges(c, l.thr, l.hy);
        let falls: Vec<f64> = es.iter().filter(|e| !e.rising).map(|e| e.t).collect();
        // only a frame a word was read in: where the clock is too fast for the record the band stays empty and
        // says so, it does not fill with marks
        let ends = |k: usize, t: f64| falls.get(k + 1).is_none_or(|&next| t < next);
        let read = |k: usize| p.iter().any(|x| x.t0 >= falls[k] && ends(k, x.t0));
        let read: Vec<bool> = (0..falls.len()).map(read).collect();
        if spi_frames_cut(clk, c, l.sh.spi_mode, l.thr, l.hy, &p) {
            return Err(SPI_CUT);
        }
        let marks: Vec<Packet> = (0..falls.len())
            .filter(|&k| read[k])
            .map(|k| Packet { t0: falls[k], t1: falls[k], kind: Kind::Start, value: 0, value2: None, ok: true })
            .collect();
        p.extend(marks);
        // a mark before the word that begins at the same time
        p.sort_by(|a, b| a.t0.total_cmp(&b.t0).then((a.kind != Kind::Start).cmp(&(b.kind != Kind::Start))));
    }
    Ok(p)
}

/// A bus the record does not show: more frames of `cs` (active low) have capture edges of the clock and no whole word
/// than frames with a word. What a long frame still gives is then no reading of the bus (200 kbit/s in a record of
/// 125 kSa/s read "05 05 05 05" for a frame of 16 words, and nothing for the frames of one word around it). A frame
/// runs from a falling edge of `cs` to the next rising one; a record whose first known `cs` is low starts with one
/// (also when it starts in the hysteresis band, where `edges` has no edge for the settling).
fn spi_frames_cut(clk: &[f32], cs: &[f32], mode: u8, thr: f32, hy: f32, words: &[Packet]) -> bool {
    let rising = matches!(mode, 0 | 3);
    let clocks: Vec<f64> = edges(clk, thr, hy).iter().filter(|e| e.rising == rising).map(|e| e.t).collect();
    let cs_e = edges(cs, thr, hy);
    let low_at_start = states(cs, thr, hy).into_iter().flatten().next() == Some(false);
    // one pass over the edges of CS: each frame's start and end
    let (mut frames, mut open) = (Vec::new(), low_at_start.then_some(0.0));
    for e in &cs_e {
        match (e.rising, open) {
            (false, None) => open = Some(e.t),
            (true, Some(start)) => {
                frames.push((start, e.t));
                open = None;
            }
            _ => {}
        }
    }
    frames.extend(open.map(|start| (start, f64::INFINITY)));
    let (mut read, mut cut) = (0, 0);
    // the words come in the order of their start, as the clocks do
    for (start, end) in frames {
        if words.get(words.partition_point(|w| w.t0 < start)).is_some_and(|w| w.t0 < end) {
            read += 1;
        } else if clocks.get(clocks.partition_point(|&t| t <= start)).is_some_and(|&t| t < end) {
            cut += 1;
        }
    }
    cut > read
}

/// SPI: words of `bits` bits sampled on the capture edge of `mode` (0/3: rising, 1/2: falling); a word ends after
/// `bits` edges; `cs` (active low) delimits frames, without CS a clock pause longer than `gap` samples restarts.
pub fn spi(
    clk: &[f32],
    mosi: &[f32],
    miso: Option<&[f32]>,
    cs: Option<&[f32]>,
    mode: u8,
    bits: u32,
    msb_first: bool,
    thr: f32,
    hyst: f32,
    gap: f64,
) -> Vec<Packet> {
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
        let selected = cs_s.as_ref().is_none_or(|s| !at(s, e.t));
        if !selected {
            n = 0;
            continue;
        }
        // frame boundary: CS edge or clock pause since the last bit
        let restart =
            if cs.is_some() { cs_e.iter().any(|c| !c.rising && c.t > last && c.t < e.t) } else { e.t - last > gap };
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
            out.push(Packet { value2: mi.as_ref().map(|_| b), ..Packet::new(Kind::Data, t0, e.t, a, true) });
            n = 0;
        }
    }
    out
}

/// Standard UART bit rates `uart_baud` snaps to (incl. MIDI 31250 and DMX512 250000).
pub const STD_BAUD: [f64; 25] = [
    300.0, 600.0, 1200.0, 2400.0, 4800.0, 9600.0, 14400.0, 19200.0, 28800.0, 31250.0, 38400.0, 57600.0, 76800.0,
    115200.0, 230400.0, 250000.0, 460800.0, 500000.0, 921600.0, 1e6, 1.5e6, 2e6, 2.5e6, 3e6, 4e6,
];

/// Bit rate of an asynchronous serial signal: the shortest pulse width that occurs consistently (at least 2 pulses
/// and 5 % of all, so single glitches do not count) is the first estimate of a bit time; it is refined by a least
/// squares fit over all pulses that are near integer multiples of it (1..12 bits). The result is snapped to the
/// nearest `STD_BAUD` within ±3 %, else returned as estimated. None without enough pulses or below 2 samples/bit.
pub fn uart_baud(v: &[f32], dt: f64, thr: f32, hyst: f32) -> Option<f64> {
    let e = edges(v, thr, hyst);
    let w: Vec<f64> = e.windows(2).map(|p| p[1].t - p[0].t).filter(|&x| x > 0.0).collect();
    if w.len() < 3 || dt.is_nan() || dt <= 0.0 {
        return None;
    }
    let mut s = w.clone();
    s.sort_by(|a, b| a.total_cmp(b));
    let need = (s.len() / 20).max(2);
    let mut tb = None;
    for &c in &s {
        if c < 1.5 {
            continue;
        }
        let support = s.partition_point(|&x| x <= 1.25 * c) - s.partition_point(|&x| x < 0.8 * c);
        if support >= need {
            tb = Some(c);
            break;
        }
    }
    let mut tb = tb?;
    for _ in 0..3 {
        let (mut sw, mut sn) = (0.0, 0.0);
        for &x in &w {
            let n = (x / tb).round();
            if (1.0..=12.0).contains(&n) && (x / tb - n).abs() < 0.3 {
                sw += x;
                sn += n;
            }
        }
        if sn > 0.0 {
            tb = sw / sn;
        }
    }
    let baud = 1.0 / (tb * dt);
    let best = STD_BAUD.iter().copied().min_by(|a, b| ((a - baud).abs() / a).total_cmp(&((b - baud).abs() / b)))?;
    Some(if (best - baud).abs() / best <= 0.03 { best } else { baud })
}

/// Differential pair (RS-485 / RS-422 A/B, FlexRay BP/BM, CAN H/L): A − B sample by sample (the shorter length).
/// Decode the result with a threshold of 0 V and a hysteresis of the receiver (RS-485: ±200 mV → 0.4).
pub fn diff(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

/// 9-bit UART: the ninth bit marks an address.
pub static UART9: ModuleInfo = ModuleInfo::new("Uart9", "UART 9-bit", "serial")
    .differential()
    .trigger(every_word_is_a_frame)
    .bit(|opt, _| 1.0 / opt_baud(opt, 0))
    .polarity(1, Mirror::Threshold)
    .decoder(|l| Ok(uart9(l.main(), l.dt, opt_baud(&l.opt, 0), l.thr, l.hy, false)));

/// 9-bit multidrop UART (start, 9 data bits LSB first, stop; the 9th bit marks an address character, as in RS-485
/// multidrop / MDB): `Addr { read: false }` with value = the low 8 bits when the 9th bit is set, else Data;
/// value2 = the 9th bit; `ok` = stop bit.
pub fn uart9(v: &[f32], dt: f64, baud: f64, thr: f32, hyst: f32, invert: bool) -> Vec<Packet> {
    uart(v, dt, baud, 9, Parity::None, thr, hyst, invert)
        .into_iter()
        .map(|p| {
            let a = (p.value >> 8) & 1;
            Packet {
                kind: if a == 1 { Kind::Addr { read: false } } else { Kind::Data },
                value: p.value & 0xff,
                value2: Some(a),
                ..p
            }
        })
        .collect()
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
        assert!(uart(
            &sample(|t| uart_logic(msg, baud, 8, None, 1.0, 0.0, 3.3).v(t), 0.0, dt, 1200),
            dt,
            baud * 1.5,
            8,
            Parity::None,
            1.65,
            0.2,
            false
        )
        .iter()
        .any(|x| !x.ok));
    }

    /// Fields are messages by the idle time between them: a pause clearly longer than the usual one ends a message,
    /// whatever the sender's pace, and a start condition begins one.
    #[test]
    fn bursts_are_split_at_the_long_pauses() {
        let at = |t0: f64, kind: Kind| Packet {
            t0,
            t1: t0 + if kind == Kind::Data { 1.0 } else { 0.0 },
            kind,
            value: 0,
            value2: None,
            ok: true,
        };
        // a slow sender: a character every 3.3 character times, a pause of 5.5 after the fifth (seen on a unit)
        let slow: Vec<Packet> =
            [0.0, 3.3, 6.6, 9.9, 13.2, 19.7, 23.0, 26.3].iter().map(|&t| at(t, Kind::Data)).collect();
        assert_eq!(bursts(&slow), [0..5, 5..8]);
        // characters back to back: a pause of one character is enough, half a character is not
        let dense: Vec<Packet> = [0.0, 1.0, 2.0, 3.5, 4.5, 6.6, 7.6].iter().map(|&t| at(t, Kind::Data)).collect();
        assert_eq!(bursts(&dense), [0..5, 5..7]);
        // a start condition begins a message without a pause
        let framed =
            [at(0.0, Kind::Start), at(0.0, Kind::Data), at(1.0, Kind::Stop), at(1.0, Kind::Start), at(1.0, Kind::Data)];
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

    // Break it catches: a record that starts inside a character listed false first characters (FF, ED on a 9600 Bd
    // text line of a DHO924S) made of the data bits behind the first falling edge.
    /// UART frames with the given idle after each stop bit (repeated), from 10 bit times of idle; and the end time.
    fn uart_gapped(msg: &[u8], baud: f64, gaps: &[f64]) -> (crate::sim::Logic, f64) {
        let tb = 1.0 / baud;
        let mut l = crate::sim::Logic::new(0.0, 3.3, tb / 50.0);
        l.push(0.0, true);
        let mut t = 10.0 * tb;
        for (i, &b) in msg.iter().enumerate() {
            l.push(t, false);
            t += tb;
            for k in 0..8 {
                l.push(t, (b >> k) & 1 == 1);
                t += tb;
            }
            l.push(t, true);
            t += tb * (1.0 + gaps[i % gaps.len()]);
        }
        (l, t)
    }

    // Break it catches: a record that begins on the idle line a few bit times before a character lost it with the
    // characters after it (`llo` for `Hello`), and a record cut anywhere into a stream listed a false character or left
    // out more than `UART_LEFT_OUT`. Text and random bytes, back to back, with regular gaps and with irregular ones.
    #[test]
    fn uart_cut_anywhere_lists_no_false_character_and_keeps_a_clean_start() {
        let baud = 9600.0;
        let spbs = 10usize;
        let dt = 1.0 / baud / spbs as f64;
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut rnd = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        let mut msgs: Vec<Vec<u8>> = vec![b"Hello OOS\r\n0123".to_vec(), b"AT+CGMR\r\nOK\r\n".to_vec()];
        for _ in 0..4 {
            msgs.push((0..20).map(|_| (rnd() >> 32) as u8).collect());
        }
        let irregular: Vec<f64> = (0..37).map(|_| (rnd() >> 40) as f64 / (1u64 << 24) as f64 * 4.0).collect();
        for gaps in [vec![0.0], vec![0.5], vec![3.0], irregular] {
            let regular = gaps.len() == 1;
            for msg in &msgs {
                let (l, end) = uart_gapped(msg, baud, &gaps);
                let whole = sample(|t| l.v(t), 0.0, dt, ((end * baud + 10.0) * spbs as f64) as usize);
                let truth = uart(&whole, dt, baud, 8, Parity::None, 1.65, 0.2, false);
                assert_eq!(truth.iter().map(|p| p.value as u8).collect::<Vec<_>>(), *msg);
                let first = truth[0].t0 as usize;
                for cut in first - 9 * spbs..first + 24 * spbs {
                    let p = uart(&whole[cut..], dt, baud, 8, Parity::None, 1.65, 0.2, false);
                    // the characters whose start bit lies in the record
                    let real: Vec<&Packet> = truth.iter().filter(|q| q.t0 as usize >= cut).collect();
                    for q in &p {
                        let at = q.t0 + cut as f64;
                        let ok = real.iter().any(|r| (r.t0 - at).abs() <= 2.0 && r.value == q.value);
                        assert!(
                            ok,
                            "gaps {gaps:?}, cut {}: false character {:#04x}",
                            cut as i64 - first as i64,
                            q.value
                        );
                    }
                    let listed_from = p.first().map_or(f64::INFINITY, |q| q.t0 + cut as f64);
                    let left_out = real.iter().filter(|r| r.t0 + 2.0 < listed_from).count();
                    assert!(
                        left_out <= UART_LEFT_OUT,
                        "gaps {gaps:?}, cut {}: {left_out} left out",
                        cut as i64 - first as i64
                    );
                    // a regular stream: a record that begins on the idle line before the first character keeps it
                    if regular && cut < first {
                        assert_eq!(left_out, 0, "gaps {gaps:?}: {} samples of idle lost a character", first - cut);
                    }
                }
            }
        }
    }

    #[test]
    fn uart_lists_no_character_the_start_of_the_record_leaves_open() {
        let baud = 9600.0;
        let dt = 1.0 / baud / 10.0;
        let msg = b"Hello OOS
0123";
        let l = uart_logic(msg, baud, 8, None, 3.0, 0.0, 3.3);
        let whole = sample(|t| l.v(t), 0.0, dt, 2000);
        let all = uart(&whole, dt, baud, 8, Parity::None, 1.65, 0.2, false);
        assert_eq!(all.iter().map(|x| x.value as u8).collect::<Vec<_>>(), msg);
        // cut inside the first character, at every sample from its start bit to its stop bit: what is listed is the
        // end of the message, without a wrong character in front, and at most `UART_LEFT_OUT` go with the cut one
        let first = all[0].t0 as usize;
        for cut in first + 2..first + 98 {
            let p = uart(&whole[cut..], dt, baud, 8, Parity::None, 1.65, 0.2, false);
            let text: Vec<u8> = p.iter().map(|x| x.value as u8).collect();
            assert!(
                msg.ends_with(&text) && text.len() + 1 + UART_LEFT_OUT >= msg.len(),
                "cut {} samples into the character: {:?}",
                cut - first,
                text
            );
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

    /// A bus of one word a frame, a frame every 500 us at 200 kHz, every eighth frame 16 words long, recorded from
    /// `from` for `len` seconds at `dt` and decoded with Data bits `bits`, in `mode`, with CS active high or low; the
    /// first `band` samples of CS sit in the hysteresis band.
    fn spi_burst(
        from: f64,
        len: f64,
        dt: f64,
        bits: u32,
        mode: u8,
        cs_high: bool,
        band: usize,
    ) -> Result<Vec<Packet>, &'static str> {
        use crate::protocols::{Lines, Shared, OPTS};
        let short = spi_logic(&[(0xa5, 0)], 8, mode, 200e3, 0.0, 3.3);
        let words: Vec<(u32, u32)> = (0..16).map(|k| (k, 0)).collect();
        let long = spi_logic(&words, 8, mode, 200e3, 0.0, 3.3);
        let (period, burst) = (500e-6, 8.0 * 500e-6);
        let line = |i: usize| -> Vec<f32> {
            let v = |t: f64| {
                let t = t.rem_euclid(burst);
                let v = if t < 2.0 * period { long[i].v(t) } else { short[i].v(t.rem_euclid(period)) };
                if i == 0 && cs_high {
                    3.3 - v
                } else {
                    v
                }
            };
            sample(v, from, dt, (len / dt) as usize)
        };
        let [mut cs, clk, mosi] = [0, 1, 2].map(line);
        cs.iter_mut().take(band).for_each(|x| *x = 1.65);
        let sh = Shared {
            baud: 9600.0,
            bits,
            parity: Parity::None,
            spi_mode: mode,
            can_rate: 500e3,
            canh: false,
            usb_speed: crate::decode::UsbSpeed::Auto,
            cphy_rate: 0.0,
            uart_inv: false,
            spi_timeout: 0.0,
            spi_cs_high: cs_high,
        };
        let lines = [Some(&mosi[..]), Some(&clk[..]), Some(&cs[..]), None];
        SPI.decode(&Lines { lines, dt, thr: 1.65, hy: 0.2, opt: [0; OPTS], sh, chans: [None; 4], src: [None; 4] })
    }

    /// An SPI bus the record has too few samples for: the frames of one word give nothing and a long frame gives
    /// words that are none. The decoder says what is wrong; it lists no such words. In every mode, with CS active low
    /// and high.
    #[test]
    fn spi_says_when_the_frames_are_shorter_than_a_word() {
        for mode in 0..4 {
            for cs_high in [false, true] {
                let case = format!("mode {mode}, CS active {}", if cs_high { "high" } else { "low" });
                // 2.5 samples a bit: every word is read
                let p = spi_burst(0.0, 40e-3, 2e-6, 8, mode, cs_high, 0).unwrap_or_else(|e| panic!("{case}: {e}"));
                assert!(p.iter().filter(|x| x.kind == Kind::Data).count() > 100, "{case}");
                // 8 us a sample: under one sample a bit
                assert_eq!(spi_burst(0.0, 40e-3, 8e-6, 8, mode, cs_high, 0).err(), Some(SPI_CUT), "{case}");
                // enough samples, and Data bits 16 on frames of 8: the same note
                assert_eq!(spi_burst(0.0, 40e-3, 2e-6, 16, mode, cs_high, 0).err(), Some(SPI_CUT), "{case}");
            }
        }
    }

    /// A record that starts in a frame: that frame counts like the others. Here it is cut, as is the next one, and
    /// the long frame after them reads words that are none. Also when CS starts in the hysteresis band and settles low.
    #[test]
    fn spi_counts_the_frame_the_record_starts_in() {
        let p = 500e-6;
        // from inside the short frame at 6 periods, through the long frame of the next burst
        for band in [0, 2] {
            assert_eq!(spi_burst(6.0 * p + 20e-6, 3.6 * p, 8e-6, 8, 0, false, band).err(), Some(SPI_CUT), "{band}");
        }
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
