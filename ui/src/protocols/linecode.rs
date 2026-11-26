// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Line codes on one line: Manchester (IEEE 802.3: a rising mid-bit edge is a 1) and MLT-3 (a change of level is a 1),
//! at the baud rate of the option row. Built in until OOS 0.1.12, modules since; their keys are the names of the old
//! setups, and a setup that kept the rate in the shared Baud row brings it along (`legacy`).

use super::*;
use oos_dsp::decode::Kind;
#[cfg(test)]
use oos_dsp::linecode::manchester_fields;
use oos_dsp::linecode::MANCHESTER_AUTO;

const OPTIONS: [Opt; 1] = [Opt { name: "Baud", choices: &BAUD_NAMES, default: BAUD_DEFAULT }];

/// The rates of the Manchester decoder: those of the list, and Auto, the rate of the signal itself. A Manchester
/// line carries its clock (an edge in the middle of every bit), so its rate need not be one of a list: a bus at
/// 125 kbit/s had no entry. Auto is last, so the entries of a set-up from before mean what they meant, and it is what
/// a new decoder starts with (`oos_dsp::linecode::MANCHESTER_AUTO`).
const MANCHESTER_RATES: [&str; 11] =
    ["1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "460800", "921600", "Auto"];
const MANCHESTER_OPTIONS: [Opt; 4] = [
    Opt { name: "Baud", choices: &MANCHESTER_RATES, default: MANCHESTER_AUTO },
    // which edge in the middle of a bit is a one: rising (IEEE 802.3, Ethernet) or falling (G. E. Thomas), the
    // two conventions in use
    Opt { name: "A one is", choices: &["Rising (802.3)", "Falling (Thomas)"], default: 0 },
    // the fields of a packet (a line that is quiet between its packets): so many bits of sync, so many of header,
    // then words of Data bits, and what is left is the trailer. Both at 0: a packet is not cut
    Opt { name: "Sync bits", choices: &FIELD_BITS, default: 0 },
    Opt { name: "Header bits", choices: &FIELD_BITS, default: 0 },
];
/// The widths a sync or a header of a Manchester packet can have.
const FIELD_BITS: [&str; 17] =
    ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16"];

/// What the demo lines of this file carry.
const DEMO_BYTES: [u8; 6] = [0xa5, 0x3c, 0x0f, 0x99, 0x5a, 0xc3];

pub struct Manchester;

impl Protocol for Manchester {
    /// The decoder, its rate (Auto: the signal's own), the other convention and the fields of a packet are
    /// `oos_dsp::linecode::MANCHESTER`'s.
    fn info(&self) -> &'static ModuleInfo {
        &oos_dsp::linecode::MANCHESTER
    }
    fn options(&self) -> &'static [Opt] {
        &MANCHESTER_OPTIONS
    }
    /// Six bytes at the rate of the list's usual entry, least significant bit first, a one rising in the middle of
    /// its bit, 0 to 3.3 V, with a pause of 24 bit times between the rounds.
    fn demo(&self) -> Option<Demo> {
        let half = 0.5 / crate::decode::BAUDS[BAUD_DEFAULT];
        let bits = DEMO_BYTES.iter().flat_map(|b| (0..8).map(move |i| b >> i & 1 == 1));
        let halves: Vec<bool> = bits.flat_map(|one| [!one, one]).collect();
        let period = (halves.len() + 16 + 48) as f64 * half;
        let line = oos_dsp::sim::bits_logic(&halves, half, 16, 0.0, 3.3);
        Some(Demo { period, lines: [Some(Box::new(move |t| line.v(t))), None, None, None] })
    }
    /// Data bits is the width of a word of a packet that is cut into fields.
    fn shared(&self) -> &'static [Row] {
        &[Row::Bits]
    }
    /// A packet of a line that is quiet between its packets is a message: it begins with its mark, as an SPI frame
    /// does with its chip select.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == Kind::Start)
    }
    /// The mark where a packet begins is no field of the line.
    fn mark(&self, p: &Packet) -> bool {
        p.kind == Kind::Start
    }
    /// A packet that is no whole number of bytes: its bits in the order they were sent ("0011100111101"), which
    /// says nothing of where its fields end; over 16 bits as one number with the first bit the most significant,
    /// and how many they are ("1F2A7 (17 b)").
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        // (the trailer of a packet cut into fields has its number of bits beside its value, which is not said)
        if p.kind == Kind::Field("TRL") {
            return Some(format!("TRL {:X}", p.value));
        }
        let n = p.value2.filter(|_| p.kind == Kind::Field("PKT"))? as usize;
        Some(if n <= 16 {
            format!("{:0n$b}", p.value)
        } else {
            format!("{:0w$X} ({} b)", p.value, n, w = n.div_ceil(4))
        })
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.baud, 0, 0, 0])
    }
}

pub struct Mlt3;

impl Protocol for Mlt3 {
    /// (every byte starts a frame for the protocol trigger: `oos_dsp::linecode::MLT3`)
    fn info(&self) -> &'static ModuleInfo {
        &oos_dsp::linecode::MLT3
    }
    fn options(&self) -> &'static [Opt] {
        &OPTIONS
    }
    /// The same six bytes at the rate of the list's usual entry: a one steps the level on through 0, +1, 0, -1 V,
    /// a zero holds it; 16 bit times of quiet line before them and 24 after.
    fn demo(&self) -> Option<Demo> {
        let bit = 1.0 / crate::decode::BAUDS[BAUD_DEFAULT];
        let mut line = oos_dsp::sim::Levels { seg: vec![(0.0, 0.0)], tr: bit / 20.0 };
        let (mut at, mut t) = (0usize, 16.0 * bit);
        for one in DEMO_BYTES.iter().flat_map(|b| (0..8).map(move |i| b >> i & 1 == 1)) {
            if one {
                at += 1;
                line.seg.push((t, [0.0, 1.0, 0.0, -1.0][at % 4]));
            }
            t += bit;
        }
        // (back to where a round begins: the next round is the same picture)
        line.seg.push((t, 0.0));
        Some(Demo { period: t + 24.0 * bit, lines: [Some(Box::new(move |t| line.v(t))), None, None, None] })
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.baud, 0, 0, 0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oos_dsp::protocols::BusWhat;

    // Break it catches: a setup written while the line codes were built in losing its protocol or its baud rate
    #[test]
    fn old_setups_keep_protocol_and_baud() {
        for (key, name) in [("Manchester", "Manchester"), ("Mlt3", "MLT-3")] {
            let m = crate::store::parse_setup(&format!("dec_on = true\ndec_proto = {}\ndec_baud = 3\n", key)).unwrap();
            assert_eq!(m.dec_proto.name(), name);
            assert!(m.dec_on && m.dec_opt[0] == 3, "{} {:?}", key, m.dec_opt);
            assert_eq!(m.bus_bit(), 1.0 / 9600.0);
        }
    }

    /// A packet cut into fields by the settings: sync, header, words and the trailer, each with its first bit
    /// sent as the least significant. The five packets are those of a capture of a real line, and the values
    /// those its instrument lists for them.
    #[test]
    fn a_manchester_packet_is_cut_into_its_fields() {
        let cut = |wire: &str| -> Vec<(Kind, u32)> {
            let bit = |(k, b): (usize, u8)| (16.0 * k as f64 + 8.0, b == b'1');
            let bits: Vec<(f64, bool)> = wire.bytes().enumerate().map(bit).collect();
            manchester_fields(&bits, 8.0, 2, 3, 6).iter().map(|p| (p.kind, p.value)).collect()
        };
        let (sync, hdr, trl) = (Kind::Field("SYNC"), Kind::Field("HDR"), Kind::Field("TRL"));
        let rows = [
            ("0011100111101", 7, 0x3c, 2),
            ("0001100111101", 6, 0x3c, 2),
            ("0001100111011", 6, 0x1c, 3),
            ("0011100111011", 7, 0x1c, 3),
            ("0001100101101", 6, 0x34, 2),
        ];
        for (wire, header, data, trailer) in rows {
            assert_eq!(cut(wire), [(sync, 0), (hdr, header), (Kind::Data, data), (trl, trailer)], "{wire}");
        }
        // where a field lies: the header from the third bit to the fifth
        let bits: Vec<(f64, bool)> = (0..13).map(|k| (16.0 * k as f64 + 8.0, true)).collect();
        let p = manchester_fields(&bits, 8.0, 2, 3, 6);
        assert_eq!((p[0].t0, p[1].t0, p[1].t1), (0.0, 32.0, 80.0));
        // two words and no trailer; a packet shorter than its sync and header
        let kinds = |n: usize, word: usize| -> Vec<Kind> {
            manchester_fields(&bits[..n], 8.0, 2, 3, word).iter().map(|p| p.kind).collect()
        };
        assert_eq!(kinds(13, 4), [sync, hdr, Kind::Data, Kind::Data]);
        assert_eq!(kinds(4, 6), [sync, hdr]);
        assert_eq!(Manchester.text(&p[3], &[0; OPTS]).as_deref(), Some("TRL 3"));
    }

    /// A line of packets through the module: each packet begins with its mark; as its bits where nothing says how
    /// to cut it, as its fields with the settings, and with the other convention every bit the other one.
    #[test]
    fn a_line_of_manchester_packets_through_the_module() {
        let wires = ["0011100111101", "0001100111011"];
        // 16 samples a bit; a one rising in the middle; 2.5 bit times of quiet line after a packet
        let mut v = vec![0.0f32; 48];
        for wire in wires {
            for b in wire.bytes() {
                let one = b == b'1';
                v.extend(std::iter::repeat_n(if one { 0.0 } else { 1.0 }, 8));
                v.extend(std::iter::repeat_n(if one { 1.0 } else { 0.0 }, 8));
            }
            v.extend(std::iter::repeat_n(0.0, 40));
        }
        let said = |opt: [usize; OPTS], bits: u32| -> String {
            let sh = Shared { bits, ..shared(&crate::model::Model::default()) };
            let lines = [Some(&v[..]), None, None, None];
            let l = Lines { lines, dt: 1.0, thr: 0.5, hy: 0.05, opt, sh, chans: [None; 4], src: [None; 4] };
            let p = Manchester.info().decode(&l).unwrap();
            let text = |x: &Packet| match (x.kind, Manchester.text(x, &opt)) {
                (Kind::Start, _) => "|".to_string(),
                (_, Some(t)) => t,
                (Kind::Field(l), None) => format!("{l} {:X}", x.value),
                _ => format!("{:02X}", x.value),
            };
            p.iter().map(text).collect::<Vec<_>>().join(" ")
        };
        let auto = MANCHESTER_AUTO;
        assert_eq!(said([auto, 0, 0, 0], 8), "| 0011100111101 | 0001100111011");
        assert_eq!(said([auto, 0, 2, 3], 6), "| SYNC 0 HDR 7 3C TRL 2 | SYNC 0 HDR 6 1C TRL 3");
        assert_eq!(said([auto, 1, 0, 0], 8), "| 1100011000010 | 1110011000100");
        let p = Manchester.frame_start().unwrap();
        let start = Packet { t0: 0.0, t1: 0.0, kind: Kind::Start, value: 0, value2: None, ok: true };
        assert!(p(&start) && Manchester.mark(&start));
    }

    /// Packets of 10BASE-T: an Ethernet frame is its preamble, its addresses, its type, its bytes and its check, and
    /// a frame with a byte changed has a check that does not fit. A frame that the record cuts off has no check,
    /// and with the other convention for a one the packet is its bytes.
    #[test]
    fn an_ethernet_frame_in_manchester_packets_has_its_fields_and_its_check() {
        // (addresses of the kind a station gives itself, none of a real one)
        let mut frame: Vec<u8> = vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
        frame.extend([0x08, 0x00]);
        frame.extend((0..46u8).map(|i| i.wrapping_mul(7)));
        let fcs = oos_dsp::decode::usbpd_crc32(&frame);
        frame.extend(fcs.to_le_bytes());
        let mut changed = frame.clone();
        changed[14] ^= 0x02;
        // 16 samples a bit, a one rising in the middle, the first bit of a byte the least significant; 4 bit times
        // of quiet line between the packets
        let mut v = vec![0.0f32; 64];
        for f in [&frame, &changed, &frame] {
            let bytes = [0x55u8; 7].iter().chain([0xd5u8].iter()).chain(f.iter());
            for one in bytes.flat_map(|b| (0..8).map(move |k| b >> k & 1 == 1)) {
                v.extend(std::iter::repeat_n(if one { 0.0 } else { 1.0 }, 8));
                v.extend(std::iter::repeat_n(if one { 1.0 } else { 0.0 }, 8));
            }
            v.extend(std::iter::repeat_n(0.0, 64));
        }
        // the record ends in the third frame
        v.truncate(v.len() - 64 - 20 * 16);
        let decode = |opt: [usize; OPTS]| {
            let lines = [Some(&v[..]), None, None, None];
            let sh = shared(&crate::model::Model::default());
            let l = Lines { lines, dt: 1.0, thr: 0.5, hy: 0.05, opt, sh, chans: [None; 4], src: [None; 4] };
            Manchester.info().decode(&l).unwrap()
        };
        let p = decode([MANCHESTER_AUTO, 0, 0, 0]);
        let frames = oos_dsp::decode::frames(&p, |x| x.kind == Kind::Start);
        assert_eq!(frames.len(), 3);
        // Trigger > Bus > Frame has one point a frame, where it begins: the preamble behind it is no second one.
        // The other conditions are the generic ones
        let rule = Manchester.info().trigger;
        let starts: Vec<Kind> = p.iter().filter(|x| rule(BusWhat::Frame, 0, x) == Some(true)).map(|x| x.kind).collect();
        assert_eq!(starts, vec![Kind::Start; 3]);
        for what in [BusWhat::Id, BusWhat::Data, BusWhat::Error] {
            assert_eq!(rule(what, 0, &p[1]), None, "{what:?}");
        }
        let kinds = |r: &std::ops::Range<usize>| -> Vec<Kind> {
            p[r.clone()].iter().filter(|x| x.kind != Kind::Data).map(|x| x.kind).collect()
        };
        let (pre, mac, src, ty) = (Kind::Field("PRE"), Kind::Field("MAC"), Kind::Field("SRC"), Kind::Field("TYPE"));
        let whole = vec![Kind::Start, pre, mac, src, ty, Kind::Crc];
        assert_eq!(kinds(&frames[0]), whole);
        assert_eq!(kinds(&frames[1]), whole);
        assert_eq!(kinds(&frames[2]), whole[..5], "a frame the record cuts off has no check");
        let field = |r: &std::ops::Range<usize>, kind: Kind| -> Packet {
            p[r.clone()].iter().find(|x| x.kind == kind).cloned().unwrap()
        };
        let (to, from) = (field(&frames[0], mac), field(&frames[0], src));
        assert_eq!((to.value, to.value2, from.value, from.value2), (0x0200_0000, Some(1), 0x0200_0000, Some(2)));
        assert_eq!((field(&frames[0], pre).value, field(&frames[0], ty).value), (8, 0x0800));
        // 46 bytes behind the type, the check with its value, right in the first frame and wrong in the second
        let data = |r: &std::ops::Range<usize>| p[r.clone()].iter().filter(|x| x.kind == Kind::Data).count();
        assert_eq!((data(&frames[0]), data(&frames[1])), (46, 46));
        let (first, second) = (field(&frames[0], Kind::Crc), field(&frames[1], Kind::Crc));
        assert_eq!((first.value, first.ok, second.value, second.ok), (fcs, true, fcs, false));
        // every part lies where its bytes are: the fields of a frame follow each other without a gap
        for w in p[frames[0].clone()].windows(2).skip(1) {
            assert!((w[1].t0 - w[0].t1).abs() < 1.0, "{:?} {:?}", w[0], w[1]);
        }
        // the first bits of a preamble are lost or wrong where the line leaves its rest: the delimiter is found
        // all the same, three bits short, with the first bit the other one, or with a bit too many in front
        for damage in 0..3 {
            let bytes = [0x55u8; 7].iter().chain([0xd5u8].iter()).chain(frame.iter());
            let mut bits: Vec<bool> = bytes.flat_map(|b| (0..8).map(move |k| b >> k & 1 == 1)).collect();
            match damage {
                0 => drop(bits.drain(..3)),
                1 => bits[0] = false,
                _ => bits.insert(0, false),
            }
            let mut w = vec![0.0f32; 64];
            for one in bits {
                w.extend(std::iter::repeat_n(if one { 0.0 } else { 1.0 }, 8));
                w.extend(std::iter::repeat_n(if one { 1.0 } else { 0.0 }, 8));
            }
            w.extend(std::iter::repeat_n(0.0, 64));
            // (a second packet: a line with a pause)
            w.extend([1.0f32; 8].iter().chain([0.0f32; 8].iter()).cycle().take(16 * 8));
            w.extend(std::iter::repeat_n(0.0, 64));
            let lines = [Some(&w[..]), None, None, None];
            let (opt, sh) = ([MANCHESTER_AUTO, 0, 0, 0], shared(&crate::model::Model::default()));
            let l = Lines { lines, dt: 1.0, thr: 0.5, hy: 0.05, opt, sh, chans: [None; 4], src: [None; 4] };
            let q = Manchester.info().decode(&l).unwrap();
            let check = q.iter().find(|x| x.kind == Kind::Crc);
            let check = check.unwrap_or_else(|| panic!("damage {damage}: {:?}", &q[..6.min(q.len())]));
            assert_eq!((check.value, check.ok), (fcs, true), "damage {damage}");
        }
        // the other convention: no frame, the bytes
        let other = decode([MANCHESTER_AUTO, 1, 0, 0]);
        assert!(other.iter().all(|x| matches!(x.kind, Kind::Start | Kind::Data)));
        assert_eq!(other.iter().find(|x| x.kind == Kind::Data).map(|x| x.value), Some(0xaa));
    }

    /// A packet that is no whole number of bytes as its text.
    #[test]
    fn a_manchester_packet_is_one_value_with_its_bits() {
        let pkt = Packet { t0: 0.0, t1: 1.0, kind: Kind::Field("PKT"), value: 0x73d, value2: Some(13), ok: true };
        let text = |p: Packet| Manchester.text(&p, &[0; OPTS]);
        assert_eq!(text(pkt.clone()).as_deref(), Some("0011100111101"));
        assert_eq!(text(Packet { value: 5, value2: Some(3), ..pkt.clone() }).as_deref(), Some("101"));
        assert_eq!(text(Packet { value: 0x1f2a7, value2: Some(17), ..pkt.clone() }).as_deref(), Some("1F2A7 (17 b)"));
        assert_eq!(text(Packet { kind: Kind::Data, value2: None, ..pkt }), None);
    }

    /// The demo lines of Manchester and MLT-3 carry the six bytes, and a new decoder of each reads them.
    #[test]
    fn the_demo_lines_carry_their_bytes() {
        let modules: [&dyn Protocol; 2] = [&Manchester, &Mlt3];
        for m in modules {
            let demo = m.demo().unwrap();
            let opt: [usize; OPTS] = std::array::from_fn(|k| m.options().get(k).map_or(0, |o| o.default));
            let dt = m.info().bit_time(&opt, &shared(&crate::model::Model::default())) / 20.0;
            let line = demo.lines[0].as_ref().unwrap();
            let n = (2.0 * demo.period / dt) as usize;
            let v: Vec<f32> = (0..n).map(|i| line((i as f64 * dt) % demo.period) as f32).collect();
            let (lines, sh) = ([Some(&v[..]), None, None, None], shared(&crate::model::Model::default()));
            let thr = if m.key() == "Mlt3" { 0.0 } else { 1.65 };
            let l = Lines { lines, dt, thr, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            let data = m.info().decode(&l).unwrap().into_iter().filter(|x| x.kind == Kind::Data);
            let got: Vec<u32> = data.map(|x| x.value).collect();
            let want: Vec<u32> = DEMO_BYTES.iter().map(|&b| b as u32).collect();
            assert!(got.len() >= 6 && got[..6] == want[..], "{}: {:x?}", m.name(), got);
        }
    }

    /// The Manchester decoder takes the rate of the signal with Auto, which a new decoder has: a line at 125 kbit/s,
    /// which the list does not hold, is read; and with the other convention every bit is the other one.
    #[test]
    fn manchester_reads_a_rate_that_is_not_in_the_list() {
        let m = &Manchester;
        assert_eq!(m.options()[0].choices[m.options()[0].default], "Auto");
        // the bytes A5 3C 0F 99 as bits LSB first, a one rising in the middle of its bit (two half bits each)
        let bytes = [0xa5u8, 0x3c, 0x0f, 0x99, 0x5a, 0xc3];
        let bits = bytes.iter().flat_map(|b| (0..8).map(move |i| b >> i & 1 == 1));
        let halves: Vec<bool> = bits.flat_map(|one| [!one, one]).collect();
        let half = 0.5 / 125e3;
        let dt = half / 20.0;
        let wave = oos_dsp::sim::bits_logic(&halves, half, 8, 0.0, 3.3);
        let v = oos_dsp::sim::sample(|t| wave.v(t), 0.0, dt, 20 * (halves.len() + 20));
        let values = |opt: [usize; OPTS]| -> Vec<u32> {
            let (lines, sh) = ([Some(&v[..]), None, None, None], shared(&crate::model::Model::default()));
            let l = Lines { lines, dt, thr: 1.65, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            m.info().decode(&l).unwrap().iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect()
        };
        let auto = values([MANCHESTER_AUTO, 0, 0, 0]);
        assert!(auto.len() >= 4, "{auto:02X?}");
        let other = values([MANCHESTER_AUTO, 1, 0, 0]);
        assert_eq!(other, auto.iter().map(|v| !v & 0xff).collect::<Vec<_>>());
        // a rate that is set and 8 % off still reads the line: the clock follows the edge in the middle of every
        // bit. At less than half the rate of the line it does not
        assert_eq!(values([7, 0, 0, 0]), auto, "115200 is 8 % off");
        assert_ne!(values([6, 0, 0, 0]), auto, "57600");
    }

    #[test]
    fn baud_names_are_the_bauds() {
        for (n, b) in BAUD_NAMES.iter().zip(crate::decode::BAUDS) {
            assert_eq!(n.parse::<f64>().unwrap(), b);
        }
    }
}
