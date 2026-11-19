// SPDX-License-Identifier: GPL-2.0-or-later

//! SIM presets that are a bus. A preset of a lane (PCIe, USB 3, SATA) is a pattern for an eye; these are the traffic
//! of a protocol, so that a decoder has something to read on a scope with nothing connected: the lines of the bus
//! are generated into reference slots, the selected decoder is set to the protocol on those references, and Play
//! plays them like any shown reference.
//!
//! The traffic is the demo traffic of the protocol's module ([`crate::protocols::Demo`]), which the host build puts
//! on its demo channels and which the tests decode; the UART, whose demo is the demo source's own signal, has a line
//! of text here.

use super::*;
use crate::protocols;

/// Samples to a bit, and periods of the traffic in a record (fewer when a period is long).
const SAMPLES_PER_BIT: f64 = 40.0;
const PERIODS: usize = 4;
/// Most samples of one line: 1.6 MB a line, of which a bus has four at most.
const MAX_SAMPLES: usize = 400_000;

/// The line of text the UART preset sends.
const UART_TEXT: &[u8] = b"OOS SIM 0.1\r\n";
/// The bit times of idle line between two lines of that text.
const UART_IDLE_BITS: f64 = 40.0;

/// The protocols with a bus preset, in the order of the Decode menu, with the name of the preset ("I2C bus").
pub(in crate::ui) fn buses() -> &'static [(Proto, &'static str)] {
    static B: std::sync::OnceLock<Vec<(Proto, &'static str)>> = std::sync::OnceLock::new();
    B.get_or_init(|| {
        let has = |p: &Proto| *p == Proto::UART || { let Proto::Module(i) = *p; protocols::demo(i).is_some() };
        // (the names live as long as the program: a preset's name is a `&'static str` like a lane's)
        let named = |p: Proto| -> (Proto, &'static str) { (p, Box::leak(format!("{} bus", p.name()).into_boxed_str())) };
        PROTOS.iter().copied().filter(has).map(named).collect()
    })
}

/// The traffic of a bus preset.
struct Traffic {
    dt: f64,
    /// samples of the line of each role the traffic has
    lines: [Option<Vec<f32>>; 4],
    /// one round of the traffic (s): Play shows a little more than one
    period: f64,
}

/// The traffic of the protocol `p` with the settings of its module's set-up.
fn traffic(p: Proto, baud: f64) -> Option<Traffic> {
    let Proto::Module(i) = p;
    let module = protocols::get(i);
    let record = |bit: f64, period: f64, line: &dyn Fn(usize, f64) -> Option<f64>| -> Traffic {
        let per_round = (period / (bit / SAMPLES_PER_BIT)).ceil().max(1.0);
        // whole rounds, as many as the record holds; a round too long for it is sampled more coarsely
        let rounds = ((MAX_SAMPLES as f64 / per_round) as usize).clamp(1, PERIODS);
        let n = ((per_round * rounds as f64) as usize).min(MAX_SAMPLES);
        let dt = period * rounds as f64 / n as f64;
        let lines = std::array::from_fn(|k| {
            line(k, 0.0)?;
            Some((0..n).map(|s| line(k, (s as f64 * dt).rem_euclid(period)).unwrap_or(0.0) as f32).collect())
        });
        Traffic { dt, lines, period }
    };
    if p == Proto::UART {
        let bit = 1.0 / baud;
        let wave = oos_dsp::sim::uart_logic(UART_TEXT, baud, 8, None, 0.0, 0.0, 3.3);
        // (the generator starts with ten bit times of idle line, and every character is ten bits)
        let period = (10.0 + 10.0 * UART_TEXT.len() as f64 + UART_IDLE_BITS) * bit;
        return Some(record(bit, period, &|k, t| (k == 0).then(|| wave.v(t))));
    }
    let demo = protocols::demo(i)?;
    let bit = module.bit_time(&protocols::opt_fit(i, protocols::opt_default(i)));
    Some(record(bit, demo.period, &|k, t| demo.lines[k].as_ref().map(|f| f(t))))
}

impl Ui {
    /// The bus preset that is chosen, when one is.
    pub(in crate::ui) fn sim_bus(&self) -> Option<(Proto, &'static str)> {
        let p = self.m.sim_bus?;
        buses().iter().copied().find(|b| b.0 == p)
    }

    /// The lines of the chosen bus into reference slots from the SIM slot on, and the selected decoder on them with
    /// the settings the traffic is made for. The slot of the main line and one round of the traffic, or why not.
    fn sim_bus_store(&mut self) -> Result<(usize, f64), String> {
        let (p, name) = self.sim_bus().ok_or("no bus preset is chosen")?;
        let Proto::Module(i) = p;
        let module = protocols::get(i);
        let setup = module.setup();
        // the decoder first: the traffic of the UART is at the rate its set-up gives
        let mut m = self.m.clone();
        (m.dec_proto, m.dec_thr, m.dec_la_in, m.dec_thr_line) = (p, setup.thr, false, [None; 3]);
        Self::dec_apply_rows(&mut m, setup.rows);
        m.dec_opt = protocols::opt_default(i);
        let baud = crate::decode::BAUDS[m.dec_baud.min(crate::decode::BAUDS.len() - 1)];
        let traffic = traffic(p, baud).ok_or("this protocol has no traffic to generate")?;
        let roles = module.roles();
        let count = traffic.lines.iter().flatten().count();
        if count > NREF {
            return Err("not enough reference slots".into());
        }
        // each line in a band of its own, from the top: a bus of three lines is three traces, not one tangle
        let band = 8.0 / count as f32;
        let mut slots: [Option<usize>; 4] = [None; 4];
        let mut at = 0;
        for (k, line) in traffic.lines.into_iter().enumerate() {
            let Some(v) = line else { continue };
            let slot = (self.m.sim_slot + at) % NREF;
            let (lo, hi) = v.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
            let vpd = super::nearest_vdiv(((hi - lo).max(0.2) / (0.6 * band)).max(1e-3));
            let middle = 4.0 - (at as f32 + 0.5) * band;
            let label = format!("{} {}", p.name(), roles[k].unwrap_or("line"));
            self.refs[slot] = Some(RefWave {
                src: 0,
                color: slot,
                label,
                t0: -0.5 * v.len() as f64 * traffic.dt,
                dt: traffic.dt,
                v: v.into(),
                vpd,
                offset: -0.5 * (hi + lo) + middle * vpd,
                on: true,
            });
            slots[k] = Some(slot);
            at += 1;
        }
        let main = slots[0].ok_or("the traffic has no main line")?;
        let source = |s: usize| DEC_REF_BASE + s;
        m.dec_src = source(main);
        if let Some(s) = slots[1] {
            m.dec_src2 = source(s);
        }
        if roles[2].is_some() {
            m.dec_cs = slots[2].map(source);
        }
        if roles[3].is_some() {
            m.dec_src4 = slots[3].map(source);
        }
        m.dec_on = true;
        self.m = m;
        self.invalidate_reference_eye();
        self.measure_refs();
        self.mark_changed();
        let _ = name;
        Ok((main, traffic.period))
    }

    /// "R1, R2": the slots a bus of `count` lines takes from the SIM slot on.
    fn sim_bus_slots_text(&self, count: usize) -> String {
        (0..count).map(|k| format!("R{}", (self.m.sim_slot + k) % NREF + 1)).collect::<Vec<_>>().join(", ")
    }

    /// Add to ref with a bus preset: its lines as references and the decoder on them, without playing them.
    pub(in crate::ui) fn sim_bus_add(&mut self) {
        let Some((p, name)) = self.sim_bus() else { return };
        match self.sim_bus_store() {
            Ok(_) => {
                let count = self.refs.iter().flatten().filter(|r| r.label.starts_with(p.name())).count();
                let slots = self.sim_bus_slots_text(count.clamp(1, 4));
                self.toast(&format!("{} to {}, decoder D{} on it", name, slots, self.m.dec_sel + 1));
            }
            Err(e) => self.toast(&format!("SIM: {e}")),
        }
    }

    /// Play with a bus preset: its lines as references, the decoder on them, and the references played as the
    /// acquisition, with a little more than one round of the traffic on the screen and the trigger on the main line.
    pub(in crate::ui) fn sim_bus_play(&mut self) -> Result<(), String> {
        if self.sim_mode() {
            // Live restores the settings from before Play; the preset of now comes along
            let bus = self.m.sim_bus;
            self.return_to_live();
            self.m.sim_bus = bus;
        }
        let (main, period) = self.sim_bus_store()?;
        let want = period * 1.25 / HDIV as f64;
        let scales = tscales();
        self.m.tscale = scales.iter().position(|&t| t as f64 >= want).unwrap_or(scales.len() - 1);
        self.m.delay = 0.0;
        self.m.set_trigger_wave(WaveId::Ref(main), false);
        self.sim_start_refs();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wave::Source;

    /// Every bus preset: Play puts its lines into references, sets the selected decoder on them, and the decoder
    /// reads the traffic without an error. Live goes back.
    #[test]
    fn every_bus_preset_plays_and_is_decoded() {
        // (the core decoders alone have five: UART, I2C, SPI, CAN, LIN)
        assert!(buses().len() >= 5, "{:?}", buses().iter().map(|b| b.1).collect::<Vec<_>>());
        for &(p, name) in buses() {
            let mut u = Ui::new(Source::Demo { t: 0.0 });
            u.m.layout = Layout::Studio;
            assert!(u.sim_preset_set(name), "{name}");
            assert_eq!((u.sim_preset_name(), u.m.sim_bus), (name, Some(p)));
            u.sim_play_set(true).unwrap_or_else(|e| panic!("{name}: {e}"));
            for _ in 0..6 {
                u.step(0.05);
            }
            assert!(u.sim_mode(), "{name}: the references are played");
            assert!(u.m.dec_on && u.m.dec_proto == p, "{name}: the decoder is on the protocol");
            let lines = u.refs.iter().flatten().filter(|r| r.on && r.label.starts_with(p.name())).count();
            let Proto::Module(i) = p;
            let want = if p == Proto::UART { 1 } else { protocols::demo(i).unwrap().lines.iter().flatten().count() };
            assert_eq!(lines, want, "{name}: a reference for each line");
            assert!(matches!(decode_wave(u.m.dec_src), Some(WaveId::Ref(_))), "{name}: decoded from a reference");
            let mut packets = Err("never asked");
            for _ in 0..400 {
                packets = u.decode_packets().map(|p| p.to_vec());
                if packets != Err(super::super::super::reference_decode::COMPUTING) {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
                u.step(0.05);
            }
            let packets = packets.unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(packets.len() >= 3, "{name}: {} packets", packets.len());
            // (a round that the record cuts off at its end may leave one field unfinished)
            let bad = packets.iter().filter(|x| !x.ok).count();
            let first = packets.iter().find(|x| !x.ok);
            assert!(bad <= 1, "{name}: {bad} of {} fields with an error: {first:?}", packets.len());
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            u.render(&mut cv);
            u.return_to_live();
            assert!(!u.sim_mode(), "{name}: Live");
        }
    }

    /// The presets of the lanes are as before, and choosing one leaves the bus; Add to ref with a bus preset adds its
    /// lines without playing.
    #[test]
    fn a_bus_preset_is_one_of_the_presets() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        let names = u.sim_options(Item::SimPreset).unwrap();
        assert_eq!(names.len(), LANES.len() + buses().len());
        assert!(names.iter().any(|n| n == "I2C bus") && names.iter().any(|n| n == "PCIe 4.0"));
        u.select_option(Item::SimPreset, "I2C bus");
        assert_eq!((u.value(Item::SimPreset).as_str(), u.m.sim_bus), ("I2C bus", Some(Proto::I2C)));
        // the knobs of a lane mean nothing to a bus
        let items = u.sim_items();
        assert!(!items.contains(&Item::SimRate) && !items.contains(&Item::SimPat) && items.contains(&Item::SimPlay));
        u.sim_activate(Item::SimAddRef);
        assert!(!u.sim_mode() && u.m.dec_on && u.m.dec_proto == Proto::I2C);
        assert_eq!(u.refs.iter().flatten().filter(|r| r.label.starts_with("I2C ")).count(), 2);
        // one step on from the last bus is the first lane again
        u.select_option(Item::SimPreset, "PCIe 4.0");
        assert_eq!((u.value(Item::SimPreset).as_str(), u.m.sim_bus), ("PCIe 4.0", None));
        assert!(u.sim_items().contains(&Item::SimRate));
        // the set-up keeps the preset
        u.select_option(Item::SimPreset, "LIN bus");
        let back = crate::store::parse_setup(&crate::store::setup_text(&u.m)).unwrap();
        assert_eq!(back.sim_bus, Some(Proto::LIN));
    }
}
