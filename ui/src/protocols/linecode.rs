// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Line codes on one line: Manchester (IEEE 802.3: a rising mid-bit edge is a 1) and MLT-3 (a change of level is a 1),
//! at the baud rate of the option row. Built in until OOS 0.1.12, modules since; their keys are the names of the old
//! setups, and a setup that kept the rate in the shared Baud row brings it along (`legacy`).

use super::*;
use oos_dsp::decode::Kind;

/// The baud rate of option row 0.
fn baud(opt: &[usize; OPTS]) -> f64 {
    crate::decode::BAUDS[opt[0].min(crate::decode::BAUDS.len() - 1)]
}

const OPTIONS: [Opt; 1] = [Opt { name: "Baud", choices: &BAUD_NAMES, default: BAUD_DEFAULT }];

/// The rates of the Manchester decoder: those of the list, and Auto, the rate of the signal itself. A Manchester
/// line carries its clock (an edge in the middle of every bit), so its rate need not be one of a list: a bus at
/// 125 kbit/s had no entry. Auto is last, so the entries of a set-up from before mean what they meant, and it is what
/// a new decoder starts with.
const MANCHESTER_RATES: [&str; 11] =
    ["1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "460800", "921600", "Auto"];
const MANCHESTER_AUTO: usize = MANCHESTER_RATES.len() - 1;
const MANCHESTER_OPTIONS: [Opt; 2] = [
    Opt { name: "Baud", choices: &MANCHESTER_RATES, default: MANCHESTER_AUTO },
    // which edge in the middle of a bit is a one: rising (IEEE 802.3, Ethernet) or falling (G. E. Thomas), the
    // two conventions in use
    Opt { name: "A one is", choices: &["Rising (802.3)", "Falling (Thomas)"], default: 0 },
];

/// The bit rate of the Manchester option row; 0 = from the signal (`oos_dsp::linecode::manchester`).
fn manchester_rate(opt: &[usize; OPTS]) -> f64 {
    if opt[0] >= MANCHESTER_AUTO { 0.0 } else { baud(opt) }
}

/// What the demo lines of this file carry.
const DEMO_BYTES: [u8; 6] = [0xa5, 0x3c, 0x0f, 0x99, 0x5a, 0xc3];

pub struct Manchester;

impl Protocol for Manchester {
    fn key(&self) -> &'static str {
        "Manchester"
    }
    fn name(&self) -> &'static str {
        "Manchester"
    }
    fn options(&self) -> &'static [Opt] {
        &MANCHESTER_OPTIONS
    }
    /// (With Auto the rate is not known before a record is read: the bit time of the list's usual entry.)
    fn bit_time(&self, opt: &[usize; OPTS]) -> f64 {
        let rate = manchester_rate(opt);
        1.0 / if rate > 0.0 { rate } else { crate::decode::BAUDS[BAUD_DEFAULT] }
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
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let p = oos_dsp::linecode::manchester(l.main(), l.dt, manchester_rate(&l.opt), l.thr, l.hy);
        if l.opt[1] == 0 {
            return Ok(p);
        }
        // the other convention: every bit is the other one. The bytes and the packets alone: the fields of an
        // Ethernet frame the decoder finds are those of 802.3
        let other = |x: Packet| match (x.kind, x.value2) {
            (Kind::Data, _) => Some(Packet { value: !x.value & 0xff, ..x }),
            (Kind::Field("PKT"), Some(n)) => {
                let mask = u32::MAX >> (32 - n.clamp(1, 32));
                Some(Packet { value: !x.value & mask, ..x })
            }
            _ => None,
        };
        Ok(p.into_iter().filter_map(other).collect())
    }
    /// A packet that is no whole number of bytes: its bits in the order they were sent ("0011100111101"), which
    /// says nothing of where its fields end; over 16 bits as one number with the first bit the most significant,
    /// and how many they are ("1F2A7 (17 b)").
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
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
    fn key(&self) -> &'static str {
        "Mlt3"
    }
    fn name(&self) -> &'static str {
        "MLT-3"
    }
    fn options(&self) -> &'static [Opt] {
        &OPTIONS
    }
    fn bit_time(&self, opt: &[usize; OPTS]) -> f64 {
        1.0 / baud(opt)
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(oos_dsp::linecode::mlt3(l.main(), l.dt, baud(&l.opt), l.thr, l.hy))
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
            let dt = m.bit_time(&opt) / 20.0;
            let line = demo.lines[0].as_ref().unwrap();
            let n = (2.0 * demo.period / dt) as usize;
            let v: Vec<f32> = (0..n).map(|i| line((i as f64 * dt) % demo.period) as f32).collect();
            let (lines, sh) = ([Some(&v[..]), None, None, None], Shared::default());
            let thr = if m.key() == "Mlt3" { 0.0 } else { 1.65 };
            let l = Lines { lines, dt, thr, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            let data = m.decode(&l).unwrap().into_iter().filter(|x| x.kind == Kind::Data);
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
            let (lines, sh) = ([Some(&v[..]), None, None, None], Shared::default());
            let l = Lines { lines, dt, thr: 1.65, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            m.decode(&l).unwrap().iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect()
        };
        let auto = values([MANCHESTER_AUTO, 0, 0, 0]);
        assert!(auto.len() >= 4, "{auto:02X?}");
        // at the rate of the signal the bytes are read the same as with the rate given; at 115200 they are not
        let other = values([MANCHESTER_AUTO, 1, 0, 0]);
        assert_eq!(other, auto.iter().map(|v| !v & 0xff).collect::<Vec<_>>());
        assert_ne!(values([7, 0, 0, 0]), auto, "115200 is 8 % off");
    }

    #[test]
    fn baud_names_are_the_bauds() {
        for (n, b) in BAUD_NAMES.iter().zip(crate::decode::BAUDS) {
            assert_eq!(n.parse::<f64>().unwrap(), b);
        }
    }
}
