// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Quad-SPI flash: CS#, SCK, IO0 and IO1; a single-line opcode, then the opcode decides the lines and the address of
//! what follows. Built in until OOS 0.1.12, a module since; the key is the name of the old setups. Checked against
//! generated traffic.

use super::*;
use oos_dsp::decode_ext2 as x2;

pub struct Qspi;

impl Protocol for Qspi {
    fn key(&self) -> &'static str {
        "Qspi"
    }
    fn name(&self) -> &'static str {
        "Quad-SPI"
    }
    fn roles(&self) -> [Option<&'static str>; 4] {
        [Some("IO0"), Some("SCK"), Some("CS#"), Some("IO1")]
    }
    fn bit_time(&self, _: &[usize; OPTS]) -> f64 {
        0.5e-6
    }
    /// Single-line opcode and three address bytes to start with; the opcode switches the lines from there.
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let cs = l.need(2, "CS# source missing")?;
        let mut io = vec![l.main()];
        if let Some(c) = l.lines[3] {
            io.push(c);
        }
        Ok(x2::qspi(cs, l.need(1, "SCK source missing")?, &io, &x2::QspiCfg::new(1, 1, 1, 3, 0, false), true, l.thr, l.hy))
    }
    /// IO0, SCK, CS#, IO1 on CH1 to CH4.
    fn setup(&self) -> Setup {
        Setup { src: [Some(0), Some(1), Some(2), Some(3)], ..Setup::default() }
    }
    /// A flash is asked for its identification (9F: three bytes on IO1) and for two bytes at address 001000 (03),
    /// at 1 MHz, every 150 us. Frames on one line each way: the four lines of the module carry them.
    fn demo(&self) -> Option<Demo> {
        let frame = |op: u8, addr: u32, data: &[u8]| -> Option<x2::gen::QspiFrame> {
            let cfg = x2::qspi_opcode_cfg(op, false, 3, 0)?;
            Some(x2::gen::QspiFrame { cfg, op, addr, mode: 0, data: data.to_vec(), miso: true })
        };
        let frames = [frame(0x9f, 0, &[0xef, 0x40, 0x18])?, frame(0x03, 0x00_1000, &[0xde, 0xad])?];
        let [cs, sck, io0, io1, _, _] = x2::gen::qspi(&frames, 1e6, 0.0, 3.3);
        let line = |l: oos_dsp::sim::Logic| -> Option<DemoLine> { Some(Box::new(move |t| l.v(t))) };
        Some(Demo { period: 150e-6, lines: [line(io0), line(sck), line(cs), line(io1)] })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Break it catches: a setup written while Quad-SPI was built in losing its protocol or its sources
    #[test]
    fn old_setups_keep_protocol_and_sources() {
        let m = crate::store::parse_setup("dec_on = true\ndec_proto = Qspi\ndec_src = 0\ndec_src2 = 1\ndec_cs = 2\ndec_src4 = 4\n").unwrap();
        assert_eq!((m.dec_proto.name(), m.dec_src, m.dec_src2, m.dec_cs, m.dec_src4), ("Quad-SPI", 0, 1, Some(2), Some(3)));
    }

    /// The demo reads as the two commands of its frames with their bytes.
    #[test]
    fn the_demo_is_an_identification_and_a_read() {
        let demo = Qspi.demo().unwrap();
        let dt = 0.5e-6 / 20.0;
        let wave = |k: usize| -> Vec<f32> {
            let line = demo.lines[k].as_ref().unwrap();
            (0..(demo.period / dt) as usize).map(|i| line(i as f64 * dt) as f32).collect()
        };
        let w: Vec<Vec<f32>> = (0..4).map(wave).collect();
        let lines = [Some(&w[0][..]), Some(&w[1][..]), Some(&w[2][..]), Some(&w[3][..])];
        let sh = Shared::default();
        let l = Lines { lines, dt, thr: 1.65, hy: 0.2, opt: [0; OPTS], sh, chans: [None; 4], src: [None; 4] };
        let p = Qspi.decode(&l).unwrap();
        let said: Vec<String> = p.iter().map(|x| format!("{:?} {:X}", x.kind, x.value)).collect();
        assert!(p.iter().all(|x| x.ok), "{said:?}");
        // (a byte that comes back on IO1 is the second value of its packet, as MISO is beside MOSI)
        let read = p.iter().filter(|x| x.kind == oos_dsp::decode::Kind::Data).map(|x| x.value2);
        assert_eq!(read.collect::<Vec<_>>(), [0xef, 0x40, 0x18, 0xde, 0xad].map(Some), "{said:?}");
        let field = |label: &'static str| oos_dsp::decode::Kind::Field(label);
        let has = |label: &'static str, v: u32| p.iter().any(|x| x.kind == field(label) && x.value == v);
        assert!(has("CMD", 0x9f) && has("CMD", 0x03) && has("ADDR", 0x1000), "{said:?}");
    }

    /// Without CS# the decoder says what is missing.
    #[test]
    fn needs_cs() {
        let v = vec![0.0f32; 100];
        let l = Lines { lines: [Some(&v), Some(&v), None, None], dt: 1e-8, thr: 1.65, hy: 0.1, opt: [0; OPTS], sh: Shared::default(), chans: [None; 4], src: [None; 4] };
        assert_eq!(Qspi.decode(&l).unwrap_err(), "CS# source missing");
    }
}
