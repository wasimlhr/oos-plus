// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Vehicle buses: CAN FD (arbitration at the CAN rate, data phase at the FD rate after BRS), FlexRay at 10 Mbit/s on
//! RxD and SENT (SAE J2716: sync pulse, status and data nibbles, CRC). Built in until OOS 0.1.12, modules since; their
//! keys are the names of the old setups, and a setup that kept the rates in the shared rows brings them along
//! (`legacy`). Checked against generated traffic.

use super::*;
use oos_dsp::decode;
use oos_dsp::decode::Kind;

/// Demo traffic on one line: `a` and then `idle` seconds.
fn one_line(a: oos_dsp::sim::Logic, idle: f64) -> Demo {
    let period = a.seg.last().map_or(1e-3, |s| s.0) + idle;
    Demo { period, lines: [Some(Box::new(move |t| a.v(t))), None, None, None] }
}

/// The CAN bit rates (`decode::CAN_RATES`).
const CAN_RATE_NAMES: [&str; 5] = ["125 kbit/s", "250 kbit/s", "500 kbit/s", "800 kbit/s", "1 Mbit/s"];
/// The data phase rates (`decode::FD_RATES`).
const FD_RATE_NAMES: [&str; 5] = ["1 M", "2 M", "4 M", "5 M", "8 M"];

const CANFD_OPTIONS: [Opt; 3] = [
    Opt { name: "Bit rate", choices: &CAN_RATE_NAMES, default: 2 },
    Opt { name: "Data rate", choices: &FD_RATE_NAMES, default: 1 },
    Opt { name: "Signal", choices: &["RX/TX", "CAN_H"], default: 0 },
];

pub struct CanFd;

impl Protocol for CanFd {
    /// A frame in the Events pane: its identifier (an error frame says so there), the number of its data bytes, the
    /// bytes, the CRC.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("ID", Col::LabelOr("ID ")), ("DLC", Col::Count), ("Data", Col::Data), ("CRC", Col::Label("CRC "))]
    }
    /// A frame begins with its start of frame; an error flag outside a frame is a message of its own.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| matches!(p.kind, Kind::Start | Kind::Field("!ERROR FRAME")))
    }
    /// The data length as the number of bytes the frame has (the code C of a frame of 24 bytes read "DLC C").
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        match p.kind {
            Kind::Field("DLC") => Some(format!("DLC {}", p.value2.unwrap_or(p.value))),
            _ => None,
        }
    }
    fn info(&self) -> &'static ModuleInfo {
        &oos_dsp::decode::CAN_FD
    }
    fn options(&self) -> &'static [Opt] {
        &CANFD_OPTIONS
    }
    /// A CAN FD frame with ID 123, bit rate switch and 12 bytes, 500 kbit/s and 2 Mbit/s.
    fn demo(&self) -> Option<Demo> {
        let fd =
            decode::gen::FdFrame { id: 0x123, ext: false, fd: true, brs: true, esi: false, data: (1..=12).collect() };
        Some(one_line(decode::gen::can_fd_logic(&[fd], 500e3, 2e6, 0.0, 3.3), 40e-6))
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.canrate, old.fdrate, usize::from(old.canh), 0])
    }
}

pub struct FlexRay;

/// The bit rates of a FlexRay cluster (`oos_dsp::decode::FLEXRAY_RATES`).
const FLEXRAY_OPTIONS: [Opt; 1] =
    [Opt { name: "Bit rate", choices: &["10 Mbit/s", "5 Mbit/s", "2.5 Mbit/s"], default: 0 }];

impl Protocol for FlexRay {
    fn options(&self) -> &'static [Opt] {
        &FLEXRAY_OPTIONS
    }
    /// A frame in the Events pane: its identifier, the cycle, the payload, the frame CRC.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("ID", Col::Label("ID ")), ("Cycle", Col::Label("CYC ")), ("Data", Col::Data), ("CRC", Col::Label("CRC "))]
    }
    /// A frame begins with its transmission start sequence.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Field("TSS"))
    }
    /// Which says nothing of the frame: its text begins with the indicators and the identifier.
    fn lead(&self, p: &Packet) -> bool {
        p.kind == oos_dsp::decode::Kind::Field("TSS")
    }
    /// (the rate of the option; the frame CRC of either channel passes: `oos_dsp::decode::flexray` tries both)
    fn info(&self) -> &'static ModuleInfo {
        &oos_dsp::decode::FLEXRAY
    }
    /// A static frame (ID 2A, cycle 5) with 8 payload bytes on RxD.
    fn demo(&self) -> Option<Demo> {
        let fr = decode::gen::FrFrame {
            ppi: false,
            nf: true,
            sync: false,
            startup: false,
            id: 0x2a,
            cycle: 5,
            payload: vec![0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88],
            channel_b: false,
        };
        Some(one_line(oos_dsp::sim::bits_logic(&decode::gen::flexray_bits(&fr, 11), 1e-7, 20, 0.0, 3.3), 10e-6))
    }
}

/// The SENT clock ticks (`decode::SENT_TICKS`; the last one: any tick, from the sync pulses).
const TICK_NAMES: [&str; 8] =
    ["3 \u{b5}s", "5 \u{b5}s", "10 \u{b5}s", "20 \u{b5}s", "30 \u{b5}s", "50 \u{b5}s", "90 \u{b5}s", "Auto"];

const SENT_OPTIONS: [Opt; 3] = [
    Opt { name: "Clock tick", choices: &TICK_NAMES, default: 0 },
    Opt { name: "Data nibbles", choices: &["1", "2", "3", "4", "5", "6"], default: 5 },
    // what the six data nibbles are: one number as they are sent, or the two 12-bit values of a sensor with two
    // channels (the second one is sent with its low nibble first)
    Opt { name: "Channels", choices: &["One value", "12 + 12 bit"], default: 0 },
];

pub struct Sent;

impl Protocol for Sent {
    /// A frame in the Events pane: its status nibble, the data nibbles, the CRC, and in the frame that completes a
    /// message of the slow channel that message.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[
            ("Status", Col::Label("STAT ")),
            ("Data", Col::Data),
            ("CRC", Col::Label("CRC ")),
            ("Slow ID", Col::Label("SID ")),
            ("Slow data", Col::Label("SDATA ")),
        ]
    }
    /// A frame begins with its calibration pulse.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Field("SYNC"))
    }
    fn info(&self) -> &'static ModuleInfo {
        &oos_dsp::decode::SENT
    }
    fn options(&self) -> &'static [Opt] {
        &SENT_OPTIONS
    }
    /// The tick the sync pulse gave; the data as its nibbles, or as the two values of a sensor with two channels
    /// (the second one comes with its low nibble first: "3C1CB3" is 3C1 and 3BC); a message of the slow channel
    /// with the digits its identifier and its data have.
    fn text(&self, p: &Packet, opt: &[usize; OPTS]) -> Option<String> {
        let bad = if p.ok { "" } else { " !" };
        match p.kind {
            Kind::Field("SYNC") => Some(format!("SYNC {}{}", crate::model::si(p.value as f64 * 1e-9, "s"), bad)),
            Kind::Data if opt[2] == 1 && p.value2 == Some(6) => {
                let second = p.value & 0xfff;
                let turned = (second & 0xf) << 8 | (second & 0xf0) | second >> 8;
                Some(format!("{:03X} {:03X}", p.value >> 12, turned))
            }
            Kind::Data => Some(format!("{:0w$X}", p.value, w = p.value2.unwrap_or(1) as usize)),
            Kind::Field("SID") => Some(format!("SID {:02X}", p.value)),
            Kind::Field("SDATA") => {
                let digits = p.value2.unwrap_or(12).div_ceil(4) as usize;
                Some(format!("SDATA {:0w$X}", p.value, w = digits))
            }
            Kind::Field("SCRC") => Some(format!("SCRC {:02X}{}", p.value, bad)),
            _ => None,
        }
    }
    /// 5 V logic.
    fn setup(&self) -> Setup {
        Setup { thr: 2.5, ..Setup::default() }
    }
    /// Two fast-channel frames of 6 data nibbles with a pause pulse, 3 us tick, 5 V.
    fn demo(&self) -> Option<Demo> {
        let sf =
            |status: u8, data: [u8; 6]| decode::gen::SentFrame { status, data: data.to_vec(), crc: None, pause: 40 };
        Some(one_line(
            decode::gen::sent(
                &[sf(0x0, [0x7, 0xf, 0x3, 0x8, 0x0, 0xc]), sf(0x4, [0x7, 0xf, 0x4, 0x8, 0x0, 0xb])],
                3e-6,
                0.0,
                5.0,
            ),
            0.0,
        ))
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.tick, old.nibbles.clamp(1, 6) as usize - 1, 0, 0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A FlexRay cluster at 5 or 2.5 Mbit/s is read at the rate the option says, and not at another.
    #[test]
    fn flexray_reads_at_the_rate_of_its_option() {
        let fr = decode::gen::FrFrame {
            ppi: false,
            nf: true,
            sync: false,
            startup: false,
            id: 0x2a,
            cycle: 5,
            payload: vec![0x11, 0x22, 0x33, 0x44],
            channel_b: true,
        };
        let bits = decode::gen::flexray_bits(&fr, 11);
        let ids = |rate: f64, option: usize| -> Vec<(u32, bool)> {
            let l = oos_dsp::sim::bits_logic(&bits, 1.0 / rate, 20, 0.0, 3.3);
            let dt = 1.0 / rate / 20.0;
            let v = oos_dsp::sim::sample(|t| l.v(t), 0.0, dt, 20 * (bits.len() + 60));
            let (lines, opt) = ([Some(&v[..]), None, None, None], [option, 0, 0, 0]);
            let sh = shared(&crate::model::Model::default());
            let l = Lines { lines, dt, thr: 1.65, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            let p = FlexRay.info().decode(&l).unwrap();
            p.iter().filter(|x| matches!(x.kind, Kind::Id { .. } | Kind::Crc)).map(|x| (x.value, x.ok)).collect()
        };
        for (option, rate) in decode::FLEXRAY_RATES.iter().enumerate() {
            let got = ids(*rate, option);
            assert_eq!(got.first(), Some(&(0x2a, true)), "{} bit/s", rate);
            assert!(got.len() == 2 && got[1].1, "the frame CRC of channel B passes: {:x?}", got);
            assert_eq!(
                FlexRay.info().bit_time(&[option, 0, 0, 0], &shared(&crate::model::Model::default())),
                1.0 / rate
            );
        }
        // a cluster at 5 Mbit/s read at 10: no frame
        assert!(ids(5e6, 0).iter().all(|x| x.0 != 0x2a));
    }

    /// The FlexRay protocol trigger on Frame (`:TRIGger:BUS:WHEN FRAMe`) is the trigger on the transmission start
    /// sequence: of the decoded fields only the TSS of each frame matches, and the trigger point is the end of the TSS,
    /// the rising edge into the frame start sequence. Two frames, two points, none on the header, payload or CRC.
    #[test]
    fn flexray_frame_trigger_is_the_transmission_start_sequence() {
        let frame = |id: u16, tss: usize| {
            let fr = decode::gen::FrFrame {
                ppi: false,
                nf: true,
                sync: false,
                startup: false,
                id,
                cycle: 5,
                payload: vec![0x11, 0x22, 0x33, 0x44],
                channel_b: false,
            };
            decode::gen::flexray_bits(&fr, tss)
        };
        let mut bits = frame(0x2a, 11);
        bits.extend(std::iter::repeat_n(true, 30));
        bits.extend(frame(0x51, 5));
        let (rate, spb) = (10e6, 20usize);
        let l = oos_dsp::sim::bits_logic(&bits, 1.0 / rate, spb, 0.0, 3.3);
        let dt = 1.0 / rate / spb as f64;
        let v = oos_dsp::sim::sample(|t| l.v(t), 0.0, dt, spb * (bits.len() + 60));
        let mut m = crate::store::parse_setup(
            "dec_on = true
dec_proto = FlexRay
",
        )
        .unwrap();
        let sh = shared(&m);
        let lines = Lines {
            lines: [Some(&v[..]), None, None, None],
            dt,
            thr: 1.65,
            hy: 0.2,
            opt: [0; OPTS],
            sh,
            chans: [None; 4],
            src: [None; 4],
        };
        let p = FlexRay.info().decode(&lines).unwrap();
        let tss: Vec<&Packet> = p.iter().filter(|x| x.kind == Kind::Field("TSS")).collect();
        assert_eq!(tss.iter().map(|x| (x.value, x.ok)).collect::<Vec<_>>(), [(11, true), (5, true)]);
        m.trig_bus = oos_dsp::protocols::BusWhat::Frame;
        let points = crate::trig::points(&m, &p);
        assert_eq!(points, tss.iter().map(|x| x.t1).collect::<Vec<_>>(), "Frame matches the TSS fields only");
        for (x, n) in tss.iter().zip([11.0, 5.0]) {
            // the TSS is n bits low (packet times count samples); its end is the rising edge, half a bit either way
            assert!(((x.t1 - x.t0) / spb as f64 - n).abs() < 0.5, "TSS of {n} bits: {} samples", x.t1 - x.t0);
            let i = x.t1.round() as usize;
            assert!(v[i - spb / 2] < 1.65 && v[i + spb / 2] > 1.65, "the point sits on the rising edge");
        }
    }

    // Break it catches: a setup written while CAN FD, FlexRay and SENT were built in losing its protocol or rates
    #[test]
    fn old_setups_keep_protocol_and_options() {
        let load = |text: &str| crate::store::parse_setup(&format!("dec_on = true\n{}", text)).unwrap();
        let m = load("dec_proto = CanFd\ndec_canrate = 4\ndec_fdrate = 3\ndec_canh = true\n");
        assert_eq!((m.dec_proto.name(), m.dec_opts()[..3].to_vec()), ("CAN-FD", vec![4, 3, 1]));
        assert_eq!(m.bus_bit(), 1e-6);
        let m = load("dec_proto = FlexRay\n");
        assert_eq!((m.dec_proto.name(), m.bus_bit()), ("FlexRay", 0.1e-6));
        let m = load("dec_proto = Sent\ndec_tick = 2\ndec_nibbles = 4\n");
        assert_eq!((m.dec_proto.name(), m.dec_opts()[..2].to_vec()), ("SENT", vec![2, 3]));
        assert_eq!(m.bus_bit(), 40e-6);
        assert!(m.dec_on);
    }

    #[test]
    fn rate_names_are_the_rates() {
        for (n, r) in TICK_NAMES.iter().zip(crate::decode::SENT_TICKS) {
            assert_eq!(*n, if r > 0.0 { crate::model::si_short(r, "s") } else { "Auto".into() });
        }
        for (n, r) in CAN_RATE_NAMES.iter().zip(crate::decode::CAN_RATES) {
            assert_eq!(
                n.split(' ').next().unwrap().parse::<f64>().unwrap() * if n.contains('M') { 1e6 } else { 1e3 },
                r
            );
        }
        for (n, r) in FD_RATE_NAMES.iter().zip(crate::decode::FD_RATES) {
            assert_eq!(n.split(' ').next().unwrap().parse::<f64>().unwrap() * 1e6, r);
        }
    }
}
