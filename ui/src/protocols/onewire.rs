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
    /// A bridge at address 18h is told to reset the 1-Wire line and to send Skip ROM and the command 44h, and is
    /// asked for its status after each: I2C at 400 kHz, a round every 800 us.
    fn demo(&self) -> Option<Demo> {
        use oos_dsp::sim::{i2c_logic, I2cXfer};
        let xfer = |read: bool, data: &[u8]| {
            I2cXfer { addr: 0x18, read, data: data.to_vec(), repeated: false, stop: true }
        };
        // (the status: the 1-Wire line is not busy; after the reset a device answered, presence pulse detected)
        let (write, status) = (|data: &[u8]| xfer(false, data), |v: u8| xfer(true, &[v]));
        let reset = [write(&[0xb4]), status(0x0a)];
        let bytes = [write(&[0xa5, 0xcc]), status(0x08), write(&[0xa5, 0x44]), status(0x08)];
        let round: Vec<I2cXfer> = reset.into_iter().chain(bytes).collect();
        let (sda, scl) = i2c_logic(&round, 400e3, 0.0, 3.3);
        let (sda, scl): (DemoLine, DemoLine) = (Box::new(move |t| sda.v(t)), Box::new(move |t| scl.v(t)));
        Some(Demo { period: 800e-6, lines: [Some(sda), Some(scl), None, None] })
    }
}

#[cfg(test)]
mod tests {
    use crate::model::Proto;

    /// The demo of the bridge reads as the 1-Wire operations its commands start.
    #[test]
    fn the_bridge_demo_is_a_reset_and_two_bytes() {
        use super::*;
        let demo = Ds2482.demo().unwrap();
        let dt = 2.5e-6 / 20.0;
        let wave = |k: usize| -> Vec<f32> {
            let line = demo.lines[k].as_ref().unwrap();
            (0..(demo.period / dt) as usize).map(|i| line(i as f64 * dt) as f32).collect()
        };
        let (sda, scl, sh) = (wave(0), wave(1), Shared::default());
        let lines = [Some(&sda[..]), Some(&scl[..]), None, None];
        let l = Lines { lines, dt, thr: 1.65, hy: 0.2, opt: [0; OPTS], sh, chans: [None; 4], src: [None; 4] };
        let p = Ds2482.decode(&l).unwrap();
        let said: Vec<String> = p.iter().map(|x| format!("{:?} {:02X}", x.kind, x.value)).collect();
        assert!(p.iter().all(|x| x.ok) && p.len() >= 4, "{said:?}");
        use oos_dsp::decode::Kind;
        let has = |kind: Kind, value: u32| p.iter().any(|x| x.kind == kind && x.value == value);
        assert!(has(Kind::Field("!RESET"), 0) && has(Kind::Field("ROM"), 0xcc) && has(Kind::Data, 0x44), "{said:?}");
    }

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
