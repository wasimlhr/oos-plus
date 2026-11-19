// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! MIPI I3C on SDA / SCL, with legacy I2C devices and the CCCs on the same bus. Built in until OOS 0.1.12, a module
//! since; the key is the name of the old setups. Checked against generated traffic.

use super::*;
use oos_dsp::decode_ext2 as x2;

pub struct I3c;

impl Protocol for I3c {
    /// A frame is a transfer: from a start condition to the next (a repeated start goes on with the same one).
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Start)
    }
    /// A common command code with its name ("CCC 90 GETSTATUS"); a code without one keeps its number alone.
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        if p.kind != oos_dsp::decode::Kind::Field("CCC") {
            return None;
        }
        let name = Some(x2::i3c_ccc_name(p.value as u8)).filter(|n| *n != "?")?;
        Some(format!("CCC {:02X} {}{}", p.value, name, if p.ok { "" } else { " !" }))
    }
    fn key(&self) -> &'static str {
        "I3c"
    }
    fn name(&self) -> &'static str {
        "I3C"
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("SDA"), Some("SCL"), None, None]
    }
    /// A bit at 10 MHz (SDR).
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        0.1e-6
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(x2::i3c(l.main(), l.need(1, "SCL source missing")?, &[], l.thr, l.hy))
    }
    /// SDA on CH3, SCL on CH4; on the LA the I2C lines of the demo (D13 SDA, D12 SCL).
    fn setup(&self) -> Setup {
        Setup { la: [Some(13), Some(12), None, None], ..Setup::default() }
    }
}

#[cfg(test)]
mod tests {
    /// A common command code has its name; one the table does not have keeps the text of its kind.
    #[test]
    fn a_common_command_code_has_its_name() {
        use super::*;
        use oos_dsp::decode::Kind;
        let ccc = |value: u32, ok: bool| Packet { t0: 0.0, t1: 1.0, kind: Kind::Field("CCC"), value, value2: None, ok };
        assert_eq!(I3c.text(&ccc(0x90, true), &[0; OPTS]).as_deref(), Some("CCC 90 GETSTATUS"));
        assert_eq!(I3c.text(&ccc(0x07, false), &[0; OPTS]).as_deref(), Some("CCC 07 ENTDAA !"));
        assert_eq!(I3c.text(&ccc(0xac, true), &[0; OPTS]), None);
    }

    // Break it catches: a setup written while I3C was built in losing its protocol
    #[test]
    fn old_setups_keep_protocol() {
        let m = crate::store::parse_setup("dec_on = true\ndec_proto = I3c\ndec_src = 1\n").unwrap();
        assert_eq!((m.dec_proto.name(), m.dec_src, m.bus_bit()), ("I3C", 1, 0.1e-6));
    }
}
