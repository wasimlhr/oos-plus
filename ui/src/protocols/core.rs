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
    /// A transfer in the Events pane: the address it begins with, and what follows.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("Address", Col::Addr), ("Data", Col::Rest)]
    }
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

/// What the band of an SPI decoder says where most frames end before a word is whole: the record has too few samples
/// for the clock (a slower time base at the same memory), or the frames are shorter than the Data bits set.
pub const SPI_CUT: &str = "Frames shorter than a word: too few samples for the clock, or Data bits";

impl Protocol for Spi {
    /// A frame in the Events pane: the words of each data line.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("MOSI", Col::First), ("MISO", Col::Second)]
    }
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
            let read: Vec<bool> = (0..falls.len()).map(read).collect();
            // a bus the record does not show: most frames have clock edges and not one whole word. What a long frame
            // still gives is then no reading of the bus (seen on a unit: 200 kbit/s in a record of 125 kSa/s read
            // "05 05 05 05" for a frame of 16 words, and nothing for the frames of one word around it)
            let rising = matches!(l.sh.spi_mode, 0 | 3);
            let clocks = oos_dsp::edges::edges(clk, l.thr, l.hy);
            let clocks: Vec<f64> = clocks.iter().filter(|e| e.rising == rising).map(|e| e.t).collect();
            let clocked = |k: usize| {
                let end = edges.iter().find(|e| e.rising && e.t > falls[k]).map_or(f64::INFINITY, |e| e.t);
                clocks.get(clocks.partition_point(|&t| t <= falls[k])).is_some_and(|&t| t < end)
            };
            let cut = (0..falls.len()).filter(|&k| !read[k] && clocked(k)).count();
            if cut > read.iter().filter(|&&r| r).count() {
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
    /// A frame in the Events pane: its identifier (an error frame says so there), the number of its data bytes, the
    /// bytes, the CRC.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("ID", Col::LabelOr("ID ")), ("DLC", Col::Count), ("Data", Col::Data), ("CRC", Col::Label("CRC "))]
    }
    /// A frame begins with its start of frame; an error flag outside a frame is a message of its own.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        use oos_dsp::decode::Kind;
        Some(|p| matches!(p.kind, Kind::Start | Kind::Field("!ERROR FRAME")))
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
    /// A frame in the Events pane: its identifier, the data of the response, the checksum.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("ID", Col::Label("ID ")), ("Data", Col::Data), ("Checksum", Col::Label("CS "))]
    }
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
    /// A counter of three bits, 0 to 7 and round again, with a clock of 500 kHz whose rising edge stands in the
    /// middle of each value. The lines are a bank as the decoder takes one: D0 and D1 on the first two, the clock
    /// on the third (its role), D2 on the fourth.
    fn demo(&self) -> Option<Demo> {
        const STEP: f64 = 2e-6;
        let data = |bit: u32| -> Option<DemoLine> {
            Some(Box::new(move |t: f64| if ((t / STEP) as u32 >> bit) & 1 == 1 { 3.3 } else { 0.0 }))
        };
        let clock: DemoLine = Box::new(|t: f64| if (t / STEP).fract() >= 0.5 { 3.3 } else { 0.0 });
        Some(Demo { period: 8.0 * STEP, lines: [data(0), data(1), Some(clock), data(2)] })
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
    /// A low-power state, the preamble and the post last for a time and are boxes with their name, not the marks
    /// of an event: "LP-001" (the lines A, B, C), "PRE 28" (its symbols), "SYNC", "POST". A word is its 16 bits.
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        use oos_dsp::decode::Kind;
        match p.kind {
            Kind::Field(label) if label.starts_with("!LP") => Some(format!("LP-{}", &label[3..])),
            Kind::Field("PRE") => Some(format!("PRE {}", p.value)),
            Kind::Field("SYNC") => Some("SYNC".into()),
            // the header of a packet: the virtual channel and the data type, the word count; the second header,
            // which says the same, as one field; a byte as its two digits (a word has four)
            Kind::Field("DT") => Some(format!("VC {} DT {:02X}", p.value2.unwrap_or(0), p.value)),
            Kind::Field("WC") => Some(format!("WC {}", p.value)),
            Kind::Field("COPY") => Some(format!("COPY{}", if p.ok { "" } else { " !" })),
            // the command of the escape mode, named where it is the one for low-power data
            Kind::Field("ESC") if p.value == oos_dsp::cphy::LPDT => Some(format!("ESC {:02X} LPDT", p.value)),
            Kind::Field("ESC") => Some(format!("ESC {:02X}", p.value)),
            Kind::Data => Some(format!("{:02X}", p.value)),
            Kind::Crc => Some(format!("CRC {:04X}{}", p.value, if p.ok { "" } else { " !" })),
            Kind::Word { .. } => Some(format!("{:04X}", p.value)),
            Kind::Stop => Some("POST".into()),
            _ => None,
        }
    }
    /// A burst that is a packet in the Events pane: its virtual channel, data type and word count, its bytes, and
    /// the checksums of the header and of the payload.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[
            ("VC", Col::Part("VC ", 1)),
            ("DT", Col::Part("VC ", 3)),
            ("WC", Col::Label("WC ")),
            ("Data", Col::Data),
            ("CRC", Col::Label("CRC ")),
        ]
    }
    /// A message is what a lane does from one stop state (all three lines high) to the next: a burst with the
    /// low-power states that ask for it, or low-power data.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Field("!LP111"))
    }
    /// A frame is a burst, found at its first sync word: the decoder counts the sync words of a burst from 0, and
    /// the later ones stand between the two copies of a packet header.
    fn trigger_match(&self, what: BusWhat, _: u32, p: &Packet) -> Option<bool> {
        (what == BusWhat::Frame).then(|| p.kind == oos_dsp::decode::Kind::Field("SYNC") && p.value == 0)
    }
    /// A lane at a symbol a nanosecond: the stop state, the request for a burst (C high, all low), a burst with a
    /// short packet (virtual channel 1, data type 00, the data 01 00) and the stop state again. The low-power levels
    /// are 0 and 1.2 V, the three of a burst 0, 0.2 and 0.4 V.
    fn demo(&self) -> Option<Demo> {
        use oos_dsp::cphy::{crc16, states_for_symbols, symbols_for_word, wire_values};
        const SYMBOL: f64 = 1e-9;
        // what the lines hold and until when: low-power states of 12 symbol times each, then a state a symbol
        let mut held: Vec<(f64, [f32; 3])> = Vec::new();
        let mut t = 0.0;
        let mut hold = |time: f64, lines: [f32; 3]| {
            t += time;
            held.push((t, lines));
        };
        let low_power = |lines: [u8; 3]| lines.map(|l| 1.2 * l as f32);
        hold(12.0 * SYMBOL, low_power([1, 1, 1]));
        hold(12.0 * SYMBOL, low_power([0, 0, 1]));
        hold(12.0 * SYMBOL, low_power([0, 0, 0]));
        let header = [0x4000u16, 0x0001, crc16([0x00, 0x40, 0x01, 0x00])];
        let mut symbols = vec![3u8; 14];
        for _ in 0..2 {
            symbols.extend([3, 4, 4, 4, 4, 4, 3]);
            // (a word with its least significant symbol first)
            header.iter().for_each(|&w| symbols.extend(symbols_for_word(w).iter().rev()));
        }
        symbols.extend([4; 14]);
        states_for_symbols(0, &symbols).into_iter().for_each(|state| hold(SYMBOL, wire_values(state)));
        hold(12.0 * SYMBOL, low_power([1, 1, 1]));
        let period = t;
        let line = |k: usize| -> Option<DemoLine> {
            let held = held.clone();
            Some(Box::new(move |t: f64| held[held.partition_point(|h| h.0 <= t).min(held.len() - 1)].1[k] as f64))
        };
        Some(Demo { period, lines: [line(0), line(1), line(2), None] })
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

    /// An SPI bus the record has too few samples for: the frames of one word give nothing and a long frame gives
    /// words that are none. The decoder says what is wrong; it lists no such words.
    #[test]
    fn spi_says_when_the_frames_are_shorter_than_a_word() {
        use oos_dsp::sim::{sample, spi_logic};
        let m = Spi;
        // one word a frame, a frame every 50 us, clock 200 kHz; every eighth frame has 16 words
        let short = spi_logic(&[(0xa5, 0)], 8, 0, 200e3, 0.0, 3.3);
        let words: Vec<(u32, u32)> = (0..16).map(|k| (k, 0)).collect();
        let long = spi_logic(&words, 8, 0, 200e3, 0.0, 3.3);
        let (period, burst) = (500e-6, 8.0 * 500e-6);
        let line = |i: usize, dt: f64| -> Vec<f32> {
            let v = |t: f64| {
                let t = t.rem_euclid(burst);
                if t < 2.0 * period { long[i].v(t) } else { short[i].v(t.rem_euclid(period)) }
            };
            sample(v, 0.0, dt, (40e-3 / dt) as usize)
        };
        let decode = |dt: f64, bits: u32| {
            let [cs, clk, mosi] = [0, 1, 2].map(|i| line(i, dt));
            let sh = Shared { bits, spi_mode: 0, ..Shared::default() };
            let lines = [Some(&mosi[..]), Some(&clk[..]), Some(&cs[..]), None];
            m.decode(&Lines { lines, dt, thr: 1.65, hy: 0.2, opt: [0; OPTS], sh, chans: [None; 4], src: [None; 4] })
        };
        // 2.5 samples a bit, as on the unit: every word is read
        let p = decode(2e-6, 8).unwrap();
        let data: Vec<u32> = p.iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect();
        let first: Vec<u32> = (0..16).chain([0xa5, 0xa5]).collect();
        assert_eq!((data.len(), &data[..18]), (10 * 22, &first[..]));
        // 8 us a sample: under one sample a bit
        assert_eq!(decode(8e-6, 8).err(), Some(SPI_CUT));
        // enough samples, and Data bits 16 on frames of 8: the same note
        assert_eq!(decode(2e-6, 16).err(), Some(SPI_CUT));
    }

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

    /// The demo lane of C-PHY reads as its low-power states and the short packet it carries.
    #[test]
    fn the_cphy_demo_is_a_burst_with_a_short_packet() {
        let demo = Cphy.demo().unwrap();
        let dt = 1e-9 / 20.0;
        let wave = |k: usize| -> Vec<f32> {
            let line = demo.lines[k].as_ref().unwrap();
            (0..(2.0 * demo.period / dt) as usize).map(|i| line((i as f64 * dt) % demo.period) as f32).collect()
        };
        let (a, b, c, sh) = (wave(0), wave(1), wave(2), Shared::default());
        let lines = [Some(&a[..]), Some(&b[..]), Some(&c[..]), None];
        let l = Lines { lines, dt, thr: 0.02, hy: 0.0, opt: [0; OPTS], sh, chans: [None; 4], src: [None; 4] };
        let p = Cphy.decode(&l).unwrap();
        let text = |x: &Packet| Cphy.text(x, &[0; OPTS]).unwrap_or_else(|| format!("{:?}", x.kind));
        let said = p.iter().map(text).collect::<Vec<_>>().join(" ");
        let round = "LP-111 LP-001 LP-000 PRE 14 SYNC VC 1 DT 00 01 00 CRC";
        assert!(p.iter().all(|x| x.ok) && said.starts_with(round), "{said}");
        assert_eq!(said.matches("SYNC COPY POST").count(), 2, "{said}");
    }

    /// The C-PHY fields that last for a time have their name as text, a burst is found at its first sync word only,
    /// and a message begins at the stop state.
    #[test]
    fn cphy_names_its_fields_and_a_burst_is_a_frame_once() {
        let pk = |kind: Kind, value: u32| Packet { t0: 0.0, t1: 1.0, kind, value, value2: None, ok: true };
        let text = |kind: Kind, value: u32| Cphy.text(&pk(kind, value), &[0; OPTS]);
        assert_eq!(text(Kind::Field("!LP001"), 0).as_deref(), Some("LP-001"));
        assert_eq!(text(Kind::Field("PRE"), 28).as_deref(), Some("PRE 28"));
        assert_eq!(text(Kind::Field("SYNC"), 1).as_deref(), Some("SYNC"));
        assert_eq!(text(Kind::Word { cmd: false }, 0x7290).as_deref(), Some("7290"));
        // the fields of a packet
        let dt = Packet { value2: Some(1), ..pk(Kind::Field("DT"), 0x2b) };
        assert_eq!(Cphy.text(&dt, &[0; OPTS]).as_deref(), Some("VC 1 DT 2B"));
        assert_eq!(text(Kind::Field("WC"), 22).as_deref(), Some("WC 22"));
        assert_eq!(text(Kind::Field("COPY"), 0).as_deref(), Some("COPY"));
        assert_eq!(text(Kind::Field("ESC"), 0x87).as_deref(), Some("ESC 87 LPDT"));
        assert_eq!(text(Kind::Field("ESC"), 0x78).as_deref(), Some("ESC 78"));
        assert_eq!(text(Kind::Data, 0xca).as_deref(), Some("CA"));
        assert_eq!(text(Kind::Crc, 0x0e66).as_deref(), Some("CRC 0E66"));
        assert_eq!(text(Kind::Stop, 0).as_deref(), Some("POST"));
        // a symbol outside a word and the errors keep the text of their kind
        assert!(text(Kind::Field("SYM"), 3).is_none() && text(Kind::Field("!CODE"), 0).is_none());
        let frame = |kind: Kind, value: u32| Cphy.trigger_match(BusWhat::Frame, 0, &pk(kind, value));
        assert_eq!((frame(Kind::Field("SYNC"), 0), frame(Kind::Field("SYNC"), 1)), (Some(true), Some(false)));
        assert_eq!(frame(Kind::Field("PRE"), 28), Some(false));
        assert!(Cphy.trigger_match(BusWhat::Data, 0x7290, &pk(Kind::Word { cmd: false }, 0x7290)).is_none());
        let begins = Cphy.frame_start().unwrap();
        assert!(begins(&pk(Kind::Field("!LP111"), 0)));
        assert!(!begins(&pk(Kind::Field("!LP000"), 0)) && !begins(&pk(Kind::Field("SYNC"), 0)));
    }
}
