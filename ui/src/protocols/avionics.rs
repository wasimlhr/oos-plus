// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Avionics buses: ARINC 429 (bipolar RZ, 32-bit words: label, SDI, data, SSM, parity; 12.5 or 100 kbit/s or from
//! the pulse spacing) and MIL-STD-1553 (Manchester II at 1 Mbit/s, command / status and data words with their syncs).
//! Built in until OOS 0.1.12, modules since; their keys are the names of the old setups, and a setup that kept the
//! ARINC rate in the shared row brings it along (`legacy`). Checked against generated traffic.

use super::*;
use oos_dsp::decode::Kind;
use oos_dsp::decode_ext3 as x3;

/// The ARINC 429 bit rates (`decode::ARINC_RATES`).
const ARATE_NAMES: [&str; 3] = ["Auto", "12.5 k", "100 k"];

const ARINC_OPTIONS: [Opt; 1] = [Opt { name: "Bit rate", choices: &ARATE_NAMES, default: 0 }];

/// The bit rate of the options; `None` = from the pulse spacing.
fn arinc_rate(opt: &[usize; OPTS]) -> Option<f64> {
    let r = crate::decode::ARINC_RATES[opt[0]];
    (r > 0.0).then_some(r)
}

pub struct Arinc429;

impl Protocol for Arinc429 {
    /// A word in the Events pane: its label, SDI, data and sign/status matrix.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[("Label", Col::Label("L ")), ("SDI", Col::Label("SDI ")), ("Data", Col::Data), ("SSM", Col::Label("SSM "))]
    }
    /// A word is a message: it begins with its label.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| matches!(p.kind, oos_dsp::decode::Kind::Id { .. }))
    }
    fn key(&self) -> &'static str {
        "Arinc429"
    }
    fn name(&self) -> &'static str {
        "ARINC 429"
    }
    fn diff_ok(&self) -> bool {
        true
    }
    fn options(&self) -> &'static [Opt] {
        &ARINC_OPTIONS
    }
    /// Auto: high speed (the finer sampling).
    fn bit_time(&self, opt: &[usize; OPTS]) -> f64 {
        1.0 / arinc_rate(opt).unwrap_or(crate::decode::ARINC_RATES[2])
    }
    /// Auto: room for a low-speed word.
    fn frame_span(&self, opt: &[usize; OPTS]) -> Option<f64> {
        arinc_rate(opt).is_none().then(|| 200.0 / crate::decode::ARINC_RATES[1])
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(x3::arinc429(l.main(), l.dt, arinc_rate(&l.opt), l.thr, l.hy))
    }
    /// The label in octal.
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        matches!(p.kind, Kind::Id { .. }).then(|| format!("L {:03o}", p.value))
    }
    /// A word starts with its label.
    fn trigger_match(&self, what: BusWhat, _value: u32, p: &Packet) -> Option<bool> {
        (what == BusWhat::Frame && matches!(p.kind, Kind::Id { .. })).then_some(true)
    }
    /// Label 310 (octal) of the demo.
    fn trigger_default(&self) -> (BusWhat, u32) {
        (BusWhat::Id, 0o310)
    }
    /// A word starts with a pulse of either sign.
    fn start_edge(&self) -> Slope {
        Slope::Either
    }
    /// One line against ground (+-5 V): HI / LO at +-2.5 V.
    fn setup(&self) -> Setup {
        Setup { thr: 2.5, ..Setup::default() }
    }
    /// Three high-speed words, 8 bit times apart, +-5 V.
    fn demo(&self) -> Option<Demo> {
        use x3::gen;
        let w = [gen::arinc429_word(0o203, 0, 0x0_2710, 3, false), gen::arinc429_word(0o310, 1, 0x2_5a5a, 3, false), gen::arinc429_word(0o206, 0, 0x0_1388, 0, false)];
        let l = gen::arinc429(&w, 100e3, 5.0, 8);
        let period = l.seg.last().map_or(1e-3, |s| s.0);
        Some(Demo { period, lines: [Some(Box::new(move |t| l.v(t))), None, None, None] })
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.arate, 0, 0, 0])
    }
}

pub struct Mil1553;

/// The flags of a MIL-STD-1553 status word: message error, instrumentation, service request, broadcast command
/// received, busy, subsystem flag, dynamic bus control acceptance, terminal flag.
const MIL1553_FLAGS: [(u32, &str); 8] = [
    (0x400, "ME"),
    (0x200, "INS"),
    (0x100, "SRQ"),
    (0x10, "BCR"),
    (0x8, "BUSY"),
    (0x4, "SSF"),
    (0x2, "DBC"),
    (0x1, "TF"),
];

impl Protocol for Mil1553 {
    /// A command word says its terminal, the direction, the subaddress and the number of data words ("RT 17 R SA 02
    /// WC 02"; a mode command its mode code, "MC 18"); a status word its terminal and the flags that are set ("ST 17
    /// TF"); a data word its four digits. The numbers of a command are decimal, as the standard writes them.
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        use oos_dsp::decode::{MIL1553_COMMAND, MIL1553_COMMAND_2, MIL1553_STATUS};
        let (v, bad) = (p.value, if p.ok { "" } else { " !P" });
        match (p.kind, p.value2) {
            (Kind::Word { cmd: false }, _) => Some(format!("{:04X}{}", v, bad)),
            (Kind::Word { cmd: true }, Some(MIL1553_COMMAND | MIL1553_COMMAND_2)) => {
                let (dir, sa, n) = (if v >> 10 & 1 == 1 { "T" } else { "R" }, v >> 5 & 31, v & 31);
                let mode = sa == 0 || sa == 31;
                let (what, n) = if mode { ("MC", n) } else { ("WC", if n == 0 { 32 } else { n }) };
                Some(format!("RT {:02} {} SA {:02} {} {:02}{}", v >> 11, dir, sa, what, n, bad))
            }
            (Kind::Word { cmd: true }, Some(MIL1553_STATUS)) => {
                let set = MIL1553_FLAGS.iter().filter(|f| v & f.0 != 0);
                let flags: String = set.map(|f| format!(" {}", f.1)).collect();
                Some(format!("ST {:02}{}{}", v >> 11, flags, bad))
            }
            _ => None,
        }
    }
    /// A message begins with its command word.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == Kind::Word { cmd: true } && p.value2 == Some(oos_dsp::decode::MIL1553_COMMAND))
    }
    /// A message in the Events pane: the terminal, the direction, the subaddress and the count of its command (two
    /// of each for a transfer between two terminals), its data words, and what the status says.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[
            ("RT", Col::Part("RT ", 1)),
            ("T/R", Col::Part("RT ", 2)),
            ("SA", Col::Part("RT ", 4)),
            ("WC/MC", Col::Part("RT ", 6)),
            ("Data", Col::Data),
            ("Status", Col::Label("ST ")),
        ]
    }
    fn key(&self) -> &'static str {
        "Mil1553"
    }
    fn name(&self) -> &'static str {
        "1553"
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        1e-6
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(oos_dsp::decode::mil1553(l.main(), l.dt, l.thr, l.hy))
    }
    /// Trigger > Bus > Frame: the command word a message begins with (`frame_start`). A status word and the second
    /// command of a transfer between two terminals have the same sync, and by the generic rule each was a start
    /// too: two points a message (host, the demo traffic: 134 for its 67 messages). Not yet verified on hardware.
    fn trigger_match(&self, what: BusWhat, _value: u32, p: &Packet) -> Option<bool> {
        let command = p.kind == Kind::Word { cmd: true } && p.value2 == Some(oos_dsp::decode::MIL1553_COMMAND);
        (what == BusWhat::Frame).then_some(command)
    }
    /// The command word of the demo.
    fn trigger_default(&self) -> (BusWhat, u32) {
        (BusWhat::Id, 0x0c23)
    }
    /// The sync of a word goes either way.
    fn start_edge(&self) -> Slope {
        Slope::Either
    }
    /// A bipolar bus: threshold at 0 V; the trigger on the first rising edge after 3 us of quiet bus (the command sync).
    fn setup(&self) -> Setup {
        Setup { thr: 0.0, trig: Some(DemoTrig { role: 0, slope: Slope::Rise, idle: 3e-6, level: 2.0 }), ..Setup::default() }
    }
    /// A message every 120 us, +-7 V: terminal 1 is asked for three words of its subaddress 1, and answers after
    /// 7 us with its status word and the three words.
    fn demo(&self) -> Option<Demo> {
        let data = [(false, 0xbeef, 0.0), (false, 0x1234, 0.0), (false, 0x00ff, 4.0)];
        let words = [&[(true, 0x0c23, 7.0), (true, 0x0800, 0.0)][..], &data[..]].concat();
        let l = oos_dsp::sim::mil1553_message(&words, 7.0);
        Some(Demo { period: 120e-6, lines: [Some(Box::new(move |t| l.v(t))), None, None, None] })
    }
}

#[cfg(test)]
mod tests {
    /// A word of MIL-STD-1553 as the band writes it: a command by its fields, a status word by its terminal and
    /// its flags, a data word by its digits.
    #[test]
    fn a_1553_word_reads_as_its_fields() {
        use super::*;
        use oos_dsp::decode::{MIL1553_COMMAND as C, MIL1553_COMMAND_2 as C2, MIL1553_STATUS as S};
        let text = |cmd: bool, value: u32, value2: Option<u32>, ok: bool| {
            Mil1553.text(&Packet { t0: 0.0, t1: 1.0, kind: Kind::Word { cmd }, value, value2, ok }, &[0; OPTS])
        };
        assert_eq!(text(true, 0x8842, Some(C), true).as_deref(), Some("RT 17 R SA 02 WC 02"));
        assert_eq!(text(true, 0x8c12, Some(C2), true).as_deref(), Some("RT 17 T SA 00 MC 18"));
        // a count of 0 is 32 words; a mode code of 0 is 0
        assert_eq!(text(true, 0x8840, Some(C), true).as_deref(), Some("RT 17 R SA 02 WC 32"));
        assert_eq!(text(true, 0x8be0, Some(C), true).as_deref(), Some("RT 17 R SA 31 MC 00"));
        assert_eq!(text(true, 0x8801, Some(S), true).as_deref(), Some("ST 17 TF"));
        assert_eq!(text(true, 0x9000, Some(S), true).as_deref(), Some("ST 18"));
        assert_eq!(text(true, 0x9404, Some(S), false).as_deref(), Some("ST 18 ME SSF !P"));
        assert_eq!(text(false, 0x0008, None, true).as_deref(), Some("0008"));
        // a word with the command sync that has no place in a message keeps the text of its kind
        assert_eq!(text(true, 0x8842, None, true), None);
    }

    // Break it catches: a setup written while ARINC 429 and 1553 were built in losing its protocol or its bit rate
    #[test]
    fn old_setups_keep_protocol_and_rate() {
        let load = |text: &str| crate::store::parse_setup(&format!("dec_on = true\n{}", text)).unwrap();
        let m = load("dec_proto = Arinc429\ndec_arate = 1\n");
        assert_eq!((m.dec_proto.name(), m.dec_opts()[0]), ("ARINC 429", 1));
        assert_eq!(m.bus_bit(), 1.0 / 12.5e3);
        let m = load("dec_proto = Mil1553\ndec_thr = 0\n");
        assert_eq!((m.dec_proto.name(), m.dec_thr, m.bus_bit()), ("1553", 0.0, 1e-6));
        assert!(m.dec_on);
    }
}
