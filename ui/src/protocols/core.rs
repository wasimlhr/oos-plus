// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The core decoders: UART, I2C, SPI, CAN, LIN, Parallel and MIPI C-PHY. Built in until OOS 0.1.12, modules since;
//! their keys are the names of the old setups. Their settings are the shared rows (`Row`): SCPI (`:BUS<n>:RS232` ...),
//! the hardware protocol triggers of the FPGA (UART, SPI, I2C), SIM and the reference decoding use those fields by
//! name, so they stay where they were. They stand first in `MODULES` at fixed places, which `Proto::UART` ... name.

use super::*;
use oos_dsp::decode::{can, i2c, spi, uart};
use oos_dsp::decode_ext as x1;

/// Demo traffic on one line: `a` repeated every `period` seconds.
fn one_line(a: oos_dsp::sim::Logic, period: f64) -> Demo {
    Demo { period, lines: [Some(Box::new(move |t| a.v(t))), None, None, None] }
}

/// The trigger the bus demos start with: the first falling edge of the main line after `idle` seconds of quiet bus.
const fn idle_fall(idle: f32) -> Option<DemoTrig> {
    Some(DemoTrig { role: 0, slope: Slope::Fall, idle, level: 1.65 })
}

pub struct Uart;

impl Protocol for Uart {
    fn key(&self) -> &'static str {
        "Uart"
    }
    fn name(&self) -> &'static str {
        "UART"
    }
    fn diff_ok(&self) -> bool {
        true
    }
    fn shared(&self) -> &'static [Row] {
        &[Row::Baud, Row::Bits, Row::Parity, Row::Pol, Row::Fmt]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        1.0 / 115200.0
    }
    fn bit_time_sh(&self, _: &[usize; OPTS], sh: &Shared) -> f64 {
        1.0 / sh.baud
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(uart(l.main(), l.dt, l.sh.baud, l.sh.bits, l.sh.parity, l.thr, l.hy, l.sh.uart_inv))
    }
    fn bit_row(&self) -> bool {
        true
    }
    /// Start, data, parity and stop where the decoder sampled them.
    fn bits(&self, p: &Packet, _: &[usize; OPTS], sh: &Shared) -> Option<Vec<oos_dsp::decode::Bit>> {
        Some(oos_dsp::decode::uart_bits(p, sh.bits, sh.parity)).filter(|b| !b.is_empty())
    }
    /// The UART line of the demo is CH3 whatever the decoder (the host's default traffic); 8 data bits.
    fn setup(&self) -> Setup {
        Setup { rows: &[(Row::Bits, 8)], ..Setup::default() }
    }
}

pub struct I2c;

/// Whether an address is written as its seven bits or as the byte on the bus, with the read/write bit in it.
static I2C_OPTIONS: [Opt; 2] = [
    Opt { name: "R/W bit", choices: &["Left out", "Included"], default: 0 },
    Opt { name: "Format", choices: &["Hex", "ASCII"], default: 0 },
];

impl Protocol for I2c {
    /// A frame is a transfer: from a start condition to the next (a repeated start goes on with the same one).
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Start)
    }
    fn key(&self) -> &'static str {
        "I2c"
    }
    fn name(&self) -> &'static str {
        "I2C"
    }
    fn options(&self) -> &'static [Opt] {
        &I2C_OPTIONS
    }
    /// The data bytes as characters; an address stays a number.
    fn chars(&self, opt: &[usize; OPTS]) -> bool {
        opt[1] == 1
    }
    fn bit_row(&self) -> bool {
        true
    }
    /// The eight bits of a byte and its acknowledge, at the clock edges.
    fn bits(&self, p: &Packet, _: &[usize; OPTS], _: &Shared) -> Option<Vec<oos_dsp::decode::Bit>> {
        Some(oos_dsp::decode::i2c_bits(p)).filter(|b| !b.is_empty())
    }
    /// The address: its seven bits ("50 W"), or with R/W bit Included the first byte of the transfer as it is on
    /// the bus ("A0 W", "A1 R"). An address of ten bits is its ten bits in three digits ("2A5 W"), whatever R/W bit
    /// says: it has no one byte to show. " N" marks one that was not acknowledged. Triggers and searches take the
    /// seven bits.
    fn text(&self, p: &Packet, opt: &[usize; OPTS]) -> Option<String> {
        let oos_dsp::decode::Kind::Addr { read } = p.kind else { return None };
        let (rw, nak) = (if read { "R" } else { "W" }, if p.ok { "" } else { " N" });
        if p.value2 == Some(oos_dsp::decode::I2C_TEN_BIT) {
            return Some(format!("{:03X} {}{}", p.value, rw, nak));
        }
        let v = if opt[0] == 1 { (p.value << 1) | u32::from(read) } else { p.value };
        Some(format!("{:02X} {}{}", v, rw, nak))
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("SDA"), Some("SCL"), None, None]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        2.5e-6
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(i2c(l.main(), l.need(1, "source channel off")?, l.thr, l.hy))
    }
    /// The demo device's address.
    fn trigger_default(&self) -> (BusWhat, u32) {
        (BusWhat::Id, 0x50)
    }
    /// SDA on CH3, SCL on CH4 (LA: D13, D12); the trigger on the start (first SDA fall after a quiet bus).
    fn setup(&self) -> Setup {
        Setup { la: [Some(13), Some(12), None, None], trig: Some(DemoTrig { role: 0, slope: Slope::Fall, idle: 20e-6, level: 1.65 }), ..Setup::default() }
    }
    /// 400 kHz: write 0x50 [00 42], repeated start, read [4F 53] = "OS", every 200 us.
    fn demo(&self) -> Option<Demo> {
        use oos_dsp::sim::{i2c_logic, I2cXfer};
        let (sda, scl) = i2c_logic(
            &[
                I2cXfer { addr: 0x50, read: false, data: vec![0x00, 0x42], repeated: false, stop: false },
                I2cXfer { addr: 0x50, read: true, data: vec![0x4f, 0x53], repeated: true, stop: true },
            ],
            400e3,
            0.0,
            3.3,
        );
        Some(Demo { period: 200e-6, lines: [Some(Box::new(move |t| sda.v(t))), Some(Box::new(move |t| scl.v(t))), None, None] })
    }
}

pub struct Spi;

/// The order of the bits of a word on the line, and whether a high level of the data line is a 1.
static SPI_OPTIONS: [Opt; 3] = [
    Opt { name: "Bit order", choices: &["MSB first", "LSB first"], default: 0 },
    Opt { name: "Data polarity", choices: &["Active high", "Active low"], default: 0 },
    Opt { name: "Format", choices: &["Hex", "ASCII"], default: 0 },
];

impl Protocol for Spi {
    /// The mark where the chip select goes active: it begins a message and is no field of the bus.
    fn mark(&self, p: &Packet) -> bool {
        p.kind == oos_dsp::decode::Kind::Start
    }
    fn options(&self) -> &'static [Opt] {
        &SPI_OPTIONS
    }
    fn chars(&self, opt: &[usize; OPTS]) -> bool {
        opt[2] == 1
    }
    fn key(&self) -> &'static str {
        "Spi"
    }
    fn name(&self) -> &'static str {
        "SPI"
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("MOSI"), Some("Clock"), Some("CS"), Some("MISO")]
    }
    fn shared(&self) -> &'static [Row] {
        &[Row::SpiMode, Row::Bits, Row::SpiCs]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        0.5e-6
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let clk = l.need(1, "source channel off")?;
        let bit = 1e-9f64.max(l.dt);
        // a clock pause of the trigger's timeout starts a frame, else 20 samples
        let gap = if l.sh.spi_timeout > 0.0 { l.sh.spi_timeout / l.dt } else { 20.0 * bit / l.dt };
        // the decoder takes CS active low: an active-high CS is mirrored at the threshold
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
            for x in p.iter_mut().filter(|x| x.kind == oos_dsp::decode::Kind::Data) {
                x.value = !x.value & mask;
                x.value2 = x.value2.map(|v| !v & mask);
            }
        }
        // a frame starts where the chip select goes active: a start mark there makes each frame a message of its
        // own, as a start condition does for I2C. Without a chip select the words are messages by their pauses
        if let Some(c) = cs {
            use oos_dsp::decode::Kind;
            let edges = oos_dsp::edges::edges(c, l.thr, l.hy);
            let falls: Vec<f64> = edges.iter().filter(|e| !e.rising).map(|e| e.t).collect();
            // only a frame a word was read in: where the clock is too fast for the record the band stays empty and
            // says so, it does not fill with marks
            let ends = |k: usize, t: f64| falls.get(k + 1).is_none_or(|&next| t < next);
            let read = |k: usize| p.iter().any(|x| x.t0 >= falls[k] && ends(k, x.t0));
            let marks: Vec<Packet> = (0..falls.len())
                .filter(|&k| read(k))
                .map(|k| Packet { t0: falls[k], t1: falls[k], kind: Kind::Start, value: 0, value2: None, ok: true })
                .collect();
            p.extend(marks);
            // a mark before the word that begins at the same time
            p.sort_by(|a, b| a.t0.total_cmp(&b.t0).then((a.kind != Kind::Start).cmp(&(b.kind != Kind::Start))));
        }
        Ok(p)
    }
    /// A frame starts with the chip select.
    fn start_role(&self) -> usize {
        2
    }
    /// MOSI on CH3, clock on CH4, CS on CH2 (LA: D11, D10, D9); mode 0, 8 bits; the trigger on CS falling. MISO has
    /// no source until it is given one: a bus is read by its MOSI line alone.
    fn setup(&self) -> Setup {
        Setup {
            src: [Some(2), Some(3), Some(1), None],
            la: [Some(11), Some(10), Some(9), None],
            rows: &[(Row::SpiMode, 0), (Row::Bits, 8)],
            trig: Some(DemoTrig { role: 2, slope: Slope::Fall, idle: 0.0, level: 1.65 }),
            ..Setup::default()
        }
    }
    /// Mode 0, 2 MHz, words 9F/FF 00/1F 00/40 every 50 us.
    fn demo(&self) -> Option<Demo> {
        // `spi_logic`: CS, clock, MOSI, MISO
        let [cs, clk, mosi, _] = oos_dsp::sim::spi_logic(&[(0x9f, 0xff), (0x00, 0x1f), (0x00, 0x40)], 8, 0, 2e6, 0.0, 3.3);
        Some(Demo { period: 50e-6, lines: [Some(Box::new(move |t| mosi.v(t))), Some(Box::new(move |t| clk.v(t))), Some(Box::new(move |t| cs.v(t))), None] })
    }
}

pub struct Can;

impl Protocol for Can {
    /// A frame begins with its start of frame.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Start)
    }
    fn key(&self) -> &'static str {
        "Can"
    }
    fn name(&self) -> &'static str {
        "CAN"
    }
    fn diff_ok(&self) -> bool {
        true
    }
    fn shared(&self) -> &'static [Row] {
        &[Row::CanRate, Row::CanH]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        2e-6
    }
    fn bit_time_sh(&self, _: &[usize; OPTS], sh: &Shared) -> f64 {
        1.0 / sh.can_rate
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(can(l.main(), l.dt, l.sh.can_rate, l.thr, l.hy, l.sh.canh))
    }
    /// The standard frame of the demo.
    fn trigger_default(&self) -> (BusWhat, u32) {
        (BusWhat::Id, 0x123)
    }
    /// 500 kbit/s on CH3 (RX level); the trigger on the start of frame after the bus idle.
    fn setup(&self) -> Setup {
        Setup { rows: &[(Row::CanRate, 2), (Row::CanH, 0)], trig: idle_fall(20e-6), ..Setup::default() }
    }
    /// A standard and an extended data frame every 500 us.
    fn demo(&self) -> Option<Demo> {
        use oos_dsp::sim::{can_logic, CanFrame};
        let l = can_logic(
            &[CanFrame { id: 0x123, ext: false, rtr: false, data: vec![0x11, 0x22, 0x33, 0x44] }, CanFrame { id: 0x18da_f110, ext: true, rtr: false, data: vec![0x02, 0x10, 0x03] }],
            500e3,
            0.0,
            3.3,
        );
        Some(one_line(l, 500e-6))
    }
}

/// LIN: which checksum a frame is checked against. Either is what the decoder did before the option was there.
const LIN_OPTIONS: [Opt; 1] =
    [Opt { name: "Checksum", choices: &["Either", "Enhanced (2.x)", "Classic (1.x)"], default: 0 }];

pub struct Lin;

impl Protocol for Lin {
    /// A frame begins with its break; the response may come late.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Break)
    }
    fn key(&self) -> &'static str {
        "Lin"
    }
    fn name(&self) -> &'static str {
        "LIN"
    }
    fn diff_ok(&self) -> bool {
        true
    }
    /// The bit rate of a frame is measured on its sync field; Baud is for a frame whose sync cannot be read.
    fn shared(&self) -> &'static [Row] {
        &[Row::Baud]
    }
    fn options(&self) -> &'static [Opt] {
        &LIN_OPTIONS
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        1.0 / 19200.0
    }
    fn bit_time_sh(&self, _: &[usize; OPTS], sh: &Shared) -> f64 {
        1.0 / sh.baud
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        use oos_dsp::decode::LinChecksum::*;
        let checksum = [Either, Enhanced, Classic][l.opt[0].min(2)];
        Ok(oos_dsp::decode::lin_with(l.main(), l.dt, l.sh.baud, checksum, l.thr, l.hy))
    }
    /// The frame of the demo.
    fn trigger_default(&self) -> (BusWhat, u32) {
        (BusWhat::Id, 0x21)
    }
    /// 19200 baud on CH3; the trigger on the break after the bus idle.
    fn setup(&self) -> Setup {
        Setup { rows: &[(Row::Baud, 4)], trig: idle_fall(20e-6), ..Setup::default() }
    }
    /// One frame with enhanced checksum, a 12 V bus scaled to 3.3 V, every 12 ms.
    fn demo(&self) -> Option<Demo> {
        Some(one_line(oos_dsp::sim::lin_logic(&[(0x21, vec![0x10, 0x20, 0x30, 0x40], true)], 19200.0, 0.0, 3.3), 12e-3))
    }
}

pub struct Parallel;

impl Protocol for Parallel {
    fn key(&self) -> &'static str {
        "Parallel"
    }
    fn name(&self) -> &'static str {
        "Parallel"
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("D0"), None, Some("Clock"), None]
    }
    fn shared(&self) -> &'static [Row] {
        &[Row::Bits]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        0.5e-6
    }
    /// Data bits lines from the main source upwards, skipping the clock.
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let d0 = l.src[0].unwrap_or(0);
        let n = l.chans.len();
        let lines: Vec<&[f32]> = (0..n).map(|k| (d0 + k) % n).filter(|&c| Some(c) != l.src[2]).take(l.sh.bits.clamp(1, 4) as usize).map(|c| l.chans[c].ok_or("source channel off")).collect::<Result<_, _>>()?;
        Ok(x1::parallel(&lines, l.lines[2], oos_dsp::trigger::Slope::Rising, l.thr, l.hy))
    }
    /// D0..D2 on CH1..CH3, the clock on CH4, every channel on (LA: D0).
    fn setup(&self) -> Setup {
        Setup { src: [Some(0), None, Some(3), None], la: [Some(0), None, None, None], rows: &[(Row::Bits, 3)], show_all: true, ..Setup::default() }
    }
}

pub struct Cphy;

impl Protocol for Cphy {
    fn key(&self) -> &'static str {
        "Cphy"
    }
    fn name(&self) -> &'static str {
        "C-PHY"
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("A"), Some("B"), Some("C"), None]
    }
    fn shared(&self) -> &'static [Row] {
        &[Row::CphyRate]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        1e-9
    }
    /// Auto: a symbol a nanosecond.
    fn bit_time_sh(&self, _: &[usize; OPTS], sh: &Shared) -> f64 {
        if sh.cphy_rate > 0.0 { 1.0 / sh.cphy_rate } else { 1e-9 }
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let c = l.lines[2].ok_or("C-PHY C source missing")?;
        Ok(oos_dsp::cphy::decode(l.main(), l.need(1, "source channel off")?, c, l.dt, (l.sh.cphy_rate > 0.0).then_some(l.sh.cphy_rate), l.thr.abs().max(1e-6)))
    }
    /// A, B, C on CH1..CH3, the receiver's 20 mV threshold, 16-bit words; the analog channels, not the LA.
    fn setup(&self) -> Setup {
        Setup { src: [Some(0), Some(1), Some(2), None], thr: 0.02, rows: &[(Row::Bits, 16)], la_off: true, ..Setup::default() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oos_dsp::decode::Kind;

    /// The I2C address is its seven bits, or the byte on the bus when the R/W bit is included; other fields keep
    /// the text of their kind.
    #[test]
    fn the_i2c_address_has_the_read_write_bit_on_request() {
        let addr = |read: bool| Packet { t0: 0.0, t1: 1.0, kind: Kind::Addr { read }, value: 0x50, value2: None, ok: true };
        let (out, with) = ([0; OPTS], [1, 0, 0, 0]);
        assert_eq!(I2c.text(&addr(false), &out).as_deref(), Some("50 W"));
        assert_eq!(I2c.text(&addr(true), &out).as_deref(), Some("50 R"));
        assert_eq!(I2c.text(&addr(false), &with).as_deref(), Some("A0 W"));
        assert_eq!(I2c.text(&addr(true), &with).as_deref(), Some("A1 R"));
        assert_eq!(I2c.text(&Packet { ok: false, ..addr(false) }, &with).as_deref(), Some("A0 W N"));
        assert!(I2c.text(&Packet { kind: Kind::Data, ..addr(false) }, &with).is_none());
        assert_eq!(I2c.options()[0].choices[I2c.options()[0].default], "Left out");
    }
}
