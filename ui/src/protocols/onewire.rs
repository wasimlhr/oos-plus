// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! 1-Wire, directly on the data line (reset / presence, time slots, ROM commands, standard speed), and through a
//! DS2482 bridge: the I2C commands of the bridge (SDA / SCL) read as the 1-Wire operations they start. Built in until
//! OOS 0.1.12, modules since; their keys are the names of the old setups. Checked against generated traffic.

use super::*;
use oos_dsp::decode_ext as x1;

pub struct OneWire;

/// The ROM commands of 1-Wire, which every device has, by their names. The commands after them are those of a
/// device family and stay numbers.
const ROM_COMMANDS: [(u32, &str); 7] = [
    (0x33, "Read"),
    (0x55, "Match"),
    (0xcc, "Skip"),
    (0xf0, "Search"),
    (0xec, "Alarm search"),
    (0x69, "Overdrive match"),
    (0x3c, "Overdrive skip"),
];

impl Protocol for OneWire {
    /// A frame begins with a reset: what follows it is one transaction.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == oos_dsp::decode::Kind::Field("!RESET"))
    }
    /// A ROM command with its name ("ROM 55 Match"); one that is none of them keeps its number alone.
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        if p.kind != oos_dsp::decode::Kind::Field("ROM") {
            return None;
        }
        let name = ROM_COMMANDS.iter().find(|c| c.0 == p.value)?.1;
        Some(format!("ROM {:02X} {}{}", p.value, name, if p.ok { "" } else { " !" }))
    }
    /// A transaction in the Events pane: its ROM command, the family, the serial number and the CRC of the ROM it
    /// names, and the bytes after them.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[
            ("ROM", Col::Label("ROM ")),
            ("Family", Col::Label("FAM ")),
            ("Serial", Col::Label("SER ")),
            ("CRC", Col::Label("CRC ")),
            ("Data", Col::Data),
        ]
    }
    fn key(&self) -> &'static str {
        "OneWire"
    }
    fn name(&self) -> &'static str {
        "1-Wire"
    }
    /// The shortest standard-speed slot.
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        15e-6
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(x1::onewire(l.main(), l.dt, l.thr, l.hy, false))
    }
    /// Reset with presence, Read ROM, and the ROM of a DS18B20 (family 28) as far as the third byte.
    fn demo(&self) -> Option<Demo> {
        use x1::gen::Ow;
        let ow = [Ow::Reset { presence: true }, Ow::Write(0x33), Ow::Read(0x28), Ow::Read(0xFF), Ow::Read(0x4C), Ow::Read(0x3D)];
        let a = x1::gen::onewire_logic(&ow, 0.0, 3.3);
        let period = a.seg.last().map_or(1e-3, |s| s.0) + 200e-6;
        Some(Demo { period, lines: [Some(Box::new(move |t| a.v(t))), None, None, None] })
    }
}

pub struct Ds2482;

impl Protocol for Ds2482 {
    fn key(&self) -> &'static str {
        "Ds2482"
    }
    fn name(&self) -> &'static str {
        "DS2482"
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("SDA"), Some("SCL"), None, None]
    }
    /// An I2C bit at 400 kHz.
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        2.5e-6
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let scl = l.need(1, "SCL source missing")?;
        Ok(x1::ds2482(&oos_dsp::decode::i2c(l.main(), scl, l.thr, l.hy)))
    }
    /// SDA on CH3, SCL on CH4; on the LA the I2C lines of the demo (D13 SDA, D12 SCL).
    fn setup(&self) -> Setup {
        Setup { la: [Some(13), Some(12), None, None], ..Setup::default() }
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Proto;

    /// A ROM command has its name; a byte that is none keeps the text of its kind.
    #[test]
    fn a_rom_command_has_its_name() {
        use super::*;
        use oos_dsp::decode::Kind;
        let rom = |value: u32| Packet { t0: 0.0, t1: 1.0, kind: Kind::Field("ROM"), value, value2: None, ok: true };
        assert_eq!(OneWire.text(&rom(0x55), &[0; OPTS]).as_deref(), Some("ROM 55 Match"));
        assert_eq!(OneWire.text(&rom(0xf0), &[0; OPTS]).as_deref(), Some("ROM F0 Search"));
        assert_eq!(OneWire.text(&rom(0x12), &[0; OPTS]), None);
        assert_eq!(OneWire.text(&Packet { kind: Kind::Data, ..rom(0x55) }, &[0; OPTS]), None);
    }

    // Break it catches: a setup written while 1-Wire and the DS2482 were built in losing its protocol
    #[test]
    fn old_setups_keep_protocol() {
        for (key, name, roles) in [("OneWire", "1-Wire", 1), ("Ds2482", "DS2482", 2)] {
            let m = crate::store::parse_setup(&format!("dec_on = true\ndec_proto = {}\ndec_src = 1\n", key)).unwrap();
            assert_eq!((m.dec_proto.name(), m.dec_src), (name, 1));
            assert!(m.dec_on && Proto::from_key(key) == Some(m.dec_proto));
            assert_eq!(m.dec_proto.roles().iter().flatten().count(), roles);
        }
    }
}
