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
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let p = oos_dsp::linecode::manchester(l.main(), l.dt, manchester_rate(&l.opt), l.thr, l.hy);
        if l.opt[1] == 0 {
            return Ok(p);
        }
        // the other convention: every bit is the other one. The bytes alone: the fields of an Ethernet frame the
        // decoder finds are those of 802.3
        let data = p.into_iter().filter(|x| x.kind == Kind::Data);
        Ok(data.map(|x| Packet { value: !x.value & 0xff, ..x }).collect())
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
