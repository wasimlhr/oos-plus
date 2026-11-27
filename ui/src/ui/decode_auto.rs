// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:DECode:AUTO`: set the selected decoder up for its signal in one step. It picks the threshold between the levels
//! of the signal, Polarity Auto where the decoder has it, and the time base and memory depth that give at least
//! `MIN_SAMPLES_PER_BIT` samples a bit with a record that holds a whole frame, within what the unit offers (a depth
//! is never cut: where none fits it says so and leaves them). Then it arms one acquisition. Every change is named in
//! the report, and nothing else is touched.

use super::*;
use oos_dsp::diagnosis::{si, MIN_SAMPLES_PER_BIT};

thread_local! {
    /// What the last `:DECode:AUTO` did, one sentence a line. The commands run on the thread of the screen.
    static REPORT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

impl Ui {
    /// Sets the selected decoder up for its signal and returns what it did, one sentence a line: each setting it
    /// changed with its old value, each it kept, and what it could not do. The same lines are kept for
    /// [`Ui::decode_auto_text`].
    pub fn decode_auto(&mut self) -> Vec<String> {
        let mut say: Vec<String> = Vec::new();
        let Proto::Module(i) = self.m.dec_proto;
        let module = crate::protocols::get(i);
        let info = module.info();
        say.push(format!("decoder {} {}", self.m.dec_sel + 1, self.m.dec_proto.name()));
        if !self.m.dec_on {
            self.m.dec_on = true;
            say.push("switched the decoder on".into());
        }
        // the threshold, between the levels of the signal the screen holds
        if self.m.dec_on_la() {
            say.push("threshold kept: the logic analyser lines have their own".into());
        } else if let Some(thr) = self.auto_threshold(info) {
            let was = self.m.dec_thr;
            if let Some((_, lines)) = self.dec_signal_middles(&self.m).filter(|_| thr.1) {
                for (k, t) in lines.into_iter().enumerate() {
                    if t.is_some() {
                        self.m.dec_thr_line[k] = t;
                    }
                }
            }
            self.m.dec_thr = thr.0;
            say.push(format!("threshold {} (was {})", si(thr.0 as f64, "V"), si(was as f64, "V")));
        } else {
            say.push("threshold kept: no signal on the source in the record (run the scope once, check the probe)".into());
        }
        // the polarity: both ways, the better one
        if let Some((slot, _)) = info.polarity {
            let choices = module.options()[slot].choices;
            if choices.len() > 2 {
                if self.m.dec_opt[slot] != 2 {
                    say.push(format!("polarity Auto (was {})", choices[self.m.dec_opt[slot].min(2) as usize]));
                    self.m.dec_opt[slot] = 2;
                } else {
                    say.push("polarity Auto kept".into());
                }
            } else {
                say.push(format!(
                    "polarity kept at {}: Auto is not offered for {}, set it to Inverted if the words fail",
                    choices[(self.m.dec_opt[slot] as usize).min(choices.len() - 1)],
                    self.m.dec_proto.name()
                ));
            }
        }
        // the time base and the memory depth
        self.auto_timebase(info, &mut say);
        // one acquisition
        if self.m.run == RunState::Stop {
            self.m.set_sweep(TrigMode::Single);
            self.m.run = RunState::Run;
            self.scpi_arm_operation();
            say.push("armed a single acquisition (trigger sweep Single): ask again when the trigger is stopped".into());
        } else {
            say.push("the scope is running: the next record is decoded with these settings".into());
        }
        self.settings_changed();
        REPORT.with(|r| *r.borrow_mut() = say.clone());
        say
    }

    /// The report of the last [`Ui::decode_auto`] and, below it, the diagnosis of the selected decoder now.
    pub fn decode_auto_text(&self) -> String {
        let mut out = REPORT.with(|r| r.borrow().clone());
        if let Ok(d) = self.decoder_diagnosis(self.m.dec_sel) {
            out.extend(d.lines.into_iter().map(|l| format!("diagnosis: {l}")));
        }
        out.join("\n")
    }

    /// The threshold for the decoder: the middle of the signal, or for a bipolar line (a threshold that is a
    /// magnitude) half its peak; and whether the lines other than the main one have thresholds of their own to set.
    fn auto_threshold(&self, info: &oos_dsp::protocols::ModuleInfo) -> Option<(f32, bool)> {
        let (main, _) = self.dec_signal_middles(&self.m)?;
        if info.polarity.is_some_and(|(_, m)| m == oos_dsp::protocols::Mirror::Zero) {
            let wave = self.dec_wave(self.m.dec_role_src(0)?)?;
            let peak = wave.iter().copied().filter(|x| x.is_finite()).fold(0.0f32, |a, x| a.max(x.abs()));
            return (peak > 0.0).then_some((peak / 2.0, false));
        }
        Some((main, true))
    }

    /// Time base and memory depth for the decoder's bit rate: the smallest time base whose screen holds a frame, and
    /// the smallest depth that gives at least `MIN_SAMPLES_PER_BIT` samples a bit at it (the rate the unit takes
    /// with these channels on). Kept when they do already; said, not clamped, when no pair does.
    fn auto_timebase(&mut self, info: &oos_dsp::protocols::ModuleInfo, say: &mut Vec<String>) {
        let sh = crate::protocols::shared(&self.m);
        let bit = info.bit_time(&self.m.dec_opts(), &sh);
        if bit <= 0.0 {
            say.push("time base and depth kept: the protocol has no fixed bit rate to size them by".into());
            return;
        }
        let need = MIN_SAMPLES_PER_BIT / bit;
        let frame = info.frame_span(&self.m.dec_opts(), &sh).unwrap_or(200.0 * bit);
        let ok = |m: &Model| {
            let window = m.hw_time_per_div() as f64 * HDIV as f64;
            window >= frame && m.srate_depth().0 >= need && board().slow_mode(m) != crate::board::Slow::Roll
        };
        let (was_t, was_d) = (self.m.time_per_div(), mdepths()[self.m.mdepth].0);
        if ok(&self.m) {
            let (sr, depth) = self.m.srate_depth();
            say.push(format!(
                "time base {}/div and depth {} kept: {} samples a bit at {} ({} deep)",
                si(was_t as f64, "s"),
                was_d,
                (sr * bit).round(),
                si(sr, "Sa/s"),
                depth
            ));
            return;
        }
        let mut trial = self.m.clone();
        let mut best: Option<(usize, usize, u64)> = None;
        for (ts, &tdiv) in tscales().iter().enumerate() {
            if (tdiv as f64) * (HDIV as f64) < frame {
                continue;
            }
            trial.tscale = ts;
            trial.tfine = 1.0;
            for j in 0..mdepths().len() {
                trial.mdepth = j;
                if ok(&trial) {
                    let d = trial.srate_depth().1;
                    if best.is_none_or(|b| d < b.2) {
                        best = Some((ts, j, d));
                    }
                }
            }
            if best.is_some() {
                break;
            }
        }
        match best {
            Some((ts, j, depth)) => {
                self.m.tscale = ts;
                self.m.tfine = 1.0;
                self.m.mdepth = j;
                let (sr, _) = self.m.srate_depth();
                say.push(format!(
                    "time base {}/div (was {}) and depth {} (was {}): {} samples a bit at {}, {} deep, the record holds {} for a frame of {}",
                    si(self.m.time_per_div() as f64, "s"),
                    si(was_t as f64, "s"),
                    mdepths()[j].0,
                    was_d,
                    (sr * bit).round(),
                    si(sr, "Sa/s"),
                    depth,
                    si(self.m.time_per_div() as f64 * HDIV as f64, "s"),
                    si(frame, "s")
                ));
            }
            None => say.push(format!(
                "time base and depth kept: no pair holds a frame of {} at {} (10 samples a bit of {}); the unit takes at most {} with {} channels on: turn channels off, or lower the bit rate",
                si(frame, "s"),
                si(need, "Sa/s"),
                si(bit, "s"),
                si(self.m.max_srate(), "Sa/s"),
                self.m.n_on()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A UART line of 0 / 3.3 V (115200 baud, characters 0x55 and 0xa5) as a record of the screen, the decoder set
    /// to a threshold outside it and Normal polarity on 1553 (which has the row).
    fn ui_with_signal() -> Ui {
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        ui.set_dec_proto(Proto::UART);
        ui.m.run = RunState::Stop;
        ui.m.ch[0].on = true;
        ui.m.dec_src = 0;
        ui.m.dec_thr = 7.0;
        let (dt, n) = (1e-6, 4000);
        let mut v = vec![3.3f32; n];
        for (k, b) in [0x55u8, 0xa5].iter().enumerate() {
            let start = 100e-6 + k as f64 * 100e-6;
            for bit in 0..9 {
                if bit == 0 || b >> (bit - 1) & 1 == 0 {
                    let at = |i: usize| ((start + i as f64 / 115200.0) / dt).round() as usize;
                    v[at(bit)..at(bit + 1)].fill(0.0);
                }
            }
        }
        let frame = std::sync::Arc::new(crate::board::Frame {
            seq: 1,
            dt,
            pre: n / 2,
            ch: [v, Vec::new(), Vec::new(), Vec::new()],
            roll: false,
            scan: false,
            partial: false,
            envelope: false,
            acq: (0.0, 0),
            stamp: 0,
            basic: Default::default(),
            la: None,
            fixed_ti: None,
            hw_avg: false,
        });
        ui.last[0] = Some(crate::wave::to_record(&frame, &ui.m, 3600, 0.2, -0.1));
        ui
    }

    // Break it catches: a threshold that stays outside the signal, a report that does not give the old value, or one
    // that says nothing of a setting it kept
    #[test]
    fn auto_moves_the_threshold_between_the_levels_and_reports_the_old_one() {
        let mut ui = ui_with_signal();
        let report = ui.decode_auto().join("\n");
        assert!((ui.m.dec_thr - 1.65).abs() < 0.02, "{} {}", ui.m.dec_thr, report);
        assert!(report.contains("threshold 1.65 V (was 7.00 V)"), "{report}");
        // the protocol has no polarity row: nothing is said of one
        assert!(!report.contains("polarity"), "{report}");
        assert_eq!(ui.m.dec_proto, Proto::UART);
        assert!(ui.decode_auto_text().starts_with("decoder 1 UART"));
    }

    // Break it catches: a decoder that is Normal where it has the row, or ARINC 429 given an Auto it has not
    #[test]
    fn auto_chooses_polarity_auto_where_it_is_offered() {
        let mut ui = ui_with_signal();
        ui.set_dec_proto(Proto::from_key("Mil1553").unwrap());
        let report = ui.decode_auto().join("\n");
        assert_eq!(ui.m.dec_opt[0], 2, "{report}");
        assert!(report.contains("polarity Auto (was Normal)"), "{report}");
        ui.set_dec_proto(Proto::from_key("Arinc429").unwrap());
        let report = ui.decode_auto().join("\n");
        assert_eq!(ui.m.dec_opt[1], 0, "{report}");
        assert!(report.contains("Auto is not offered"), "{report}");
    }
}
