// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Bode plot with the built-in generator (S models, `Board::gen`) or the external generator of the Ext generator page
//! (ui/extgen.rs: its model, and whether it is on the LAN or the USB-A port): OOS sends the commands of the model
//! (`crate::extgen`), steps a logarithmic sweep, sets the time base to about 5 periods per screen,
//! measures gain and phase of the output channel against the input channel by a sine fit on every captured sample
//! (`oos_dsp::bode`) and draws both curves. The generator connection runs in its own thread; a step is measured
//! once the generator has its frequency. With the built-in generator and input = output channel the gain is against
//! the set amplitude and there is no phase.
//!
//! Free sweep (`FREE`): an external generator sweeps on its own and marks the start of each sweep on a sync channel.
//! OOS sets the time base and memory for one sweep, triggers once on the sync edge and reads gain (and phase, with
//! an input channel and plain samples) over the whole sweep from that one record (`oos_dsp::bode::sweep_response`).

use super::menu_text::on_off;
use super::*;
pub use crate::extgen::{BUILTIN, FREE, GENERATORS};
use oos_dsp::bode;

#[derive(Default)]
pub struct State {
    pub running: bool,
    pub freqs: Vec<f64>,
    /// (frequency, gain dB, phase degrees)
    pub points: Vec<(f64, f64, f64)>,
    /// index of the point under the cursor
    pub cursor: usize,
    pub idx: usize,
    /// records to skip after a frequency step (generator and time base settle), counted from the first record of
    /// the step (`stamp`) on
    pub wait: u32,
    /// `Model::hw_stamp` of the current step: hardware records with an older stamp are from before it
    pub stamp: u64,
    /// an external generator has not yet done the commands of this step (`Ui::ext`): what is recorded until then
    /// is the frequency before
    pub pending: bool,
    /// the user's time base, trigger source and level, restored at the end
    pub saved: Option<(usize, usize, f32)>,
    /// the built-in generator's settings before the sweep (on, Hz, Vpp, V), restored at the end
    pub afg_saved: Option<(bool, f64, f64, f64, usize, usize)>,
    /// free sweep: waiting for the sync edge, or (`capturing`) a scan record of the sweep is being written
    pub free: bool,
    pub capturing: bool,
    /// the user's settings the free sweep changes, restored at the end
    pub free_saved: Option<FreeSaved>,
    /// why the last free sweep has no phase
    pub no_phase: Option<&'static str>,
}

/// What a free sweep changes: time base, delay, trigger, depth, Auto Roll, acquisition mode, run state.
#[derive(Clone, Copy)]
pub struct FreeSaved {
    tscale: usize,
    delay: f32,
    trig_type: TrigType,
    trig_src: usize,
    trig_la: Option<u8>,
    trig_level: f32,
    slope: Slope,
    trig_mode: TrigMode,
    mdepth: usize,
    auto_roll: bool,
    acq: AcqMode,
    run: RunState,
}

/// How a free sweep is recorded: time base (index), delay (s), depth (index), and whether the time base is a scan
/// (min/max data, the trigger at the centre).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FreePlan {
    pub tscale: usize,
    pub delay: f32,
    pub mdepth: usize,
    pub scan: bool,
}

/// Time base, delay and memory depth for one free sweep of `sw` on the channels shown in `m`. Normal records put the
/// trigger one division from the left edge, the sweep and 2 % in the 9 divisions after it, and the smallest depth
/// with 20 samples per period at the stop frequency (at least 4). From the scan time bases on the trigger sits at the
/// centre (where the scan puts it) and the sweep in the right half; the record holds min/max pairs.
pub fn free_plan(m: &Model, sw: &bode::Sweep) -> Result<FreePlan, String> {
    let ts = tscales();
    let pick = |want: f64| ts.iter().position(|&t| t as f64 >= want * 0.999);
    let mut c = m.clone();
    (c.auto_roll, c.acq, c.trig_mode) = (false, AcqMode::Normal, TrigMode::Single);
    if let Some(i) = pick(sw.time * 1.02 / 9.0) {
        (c.tscale, c.delay) = (i, 4.0 * ts[i]);
        if board().slow_mode(&c) == crate::board::Slow::Off {
            let mut best = (0, 0.0);
            for d in 1..mdepths().len() {
                c.mdepth = d;
                let (sr, _) = c.srate_depth();
                if sr > best.1 {
                    best = (d, sr);
                }
                if sr >= 20.0 * sw.stop {
                    break;
                }
            }
            if best.1 < 4.0 * sw.stop {
                return Err(format!(
                    "Sweep time does not fit the memory: {} at {} needs {}, the memory gives {}",
                    si_short(sw.time, "s"),
                    si_short(sw.stop, "Hz"),
                    si_short(4.0 * sw.stop, "Sa/s"),
                    si_short(best.1, "Sa/s")
                ));
            }
            return Ok(FreePlan { tscale: i, delay: c.delay, mdepth: best.0, scan: false });
        }
    }
    let Some(i) = pick(sw.time * 1.02 / 5.0) else {
        let longest = ts.last().map_or(0.0, |&t| t as f64);
        return Err(format!("Sweep time too long: at most {}", si_short(longest * 5.0 / 1.02, "s")));
    };
    (c.tscale, c.delay, c.mdepth) = (i, 0.0, 0);
    Ok(FreePlan { tscale: i, delay: 0.0, mdepth: 0, scan: board().slow_mode(&c) != crate::board::Slow::Off })
}

/// What the Generator row offers: the scope's own, the one of the Ext generator page, or one that sweeps on its own.
const SOURCES: [&str; 3] = ["Built-in", "External", "Free-running sweep"];

impl Ui {
    pub(super) fn bode_items(&self) -> Vec<Item> {
        if self.bode_gen() == FREE {
            use Item::*;
            let mut v = vec![BodeOn, BodeRun, BodeStart, BodeStop, BodeSwTime, BodeSwLog, BodePoints, BodeIn];
            // no input channel: the gain is against a level
            if self.m.bode_sw_in == 0 {
                v.push(BodeRef);
            }
            v.extend([BodeOut, BodeGen, BodeSync, BodeSyncEdge, BodeSyncLvl]);
            if !self.bode.points.is_empty() {
                v.extend([BodeCur, BodeSave]);
            }
            return v;
        }
        let mut v = vec![
            Item::BodeOn,
            Item::BodeMode,
            Item::BodeRun,
            Item::BodeStart,
            Item::BodeStop,
            Item::BodePoints,
            Item::BodeAmp,
            Item::BodeIn,
            Item::BodeOut,
            Item::BodeGen,
        ];
        // a cursor, and saving, once there is something to read
        if !self.bode.points.is_empty() {
            v.extend([Item::BodeCur, Item::BodeSave]);
        }
        // the generator itself (model, connection, address) has a page of its own
        if self.bode_gen() != BUILTIN {
            v.push(Item::Open(MenuId::ExtGen));
        }
        v
    }

    pub(super) fn bode_kind(it: Item) -> Option<Kind> {
        Some(match it {
            Item::BodeCur => Kind::Value,
            Item::BodeRun | Item::BodeSave => Kind::Action,
            Item::BodeOn | Item::BodeIn | Item::BodeOut | Item::BodeGen | Item::BodeSync => Kind::Choice,
            _ => return None,
        })
    }

    pub(super) fn bode_label(it: Item) -> Option<&'static str> {
        Some(match it {
            Item::Open(MenuId::Bode) => "Bode",
            Item::BodeOn => "View",
            Item::BodeRun => "Start / stop sweep",
            Item::BodeSave => "Save CSV",
            Item::BodeIn => "Input (ref)",
            Item::BodeOut => "Output",
            Item::BodeGen => "Generator",
            Item::BodeCur => "Cursor",
            Item::BodeSync => "Sync",
            _ => return None,
        })
    }

    pub(super) fn bode_value(&self, it: Item) -> Option<String> {
        let m = &self.m;
        Some(match it {
            Item::BodeOn => on_off(m.view == AView::Bode),
            Item::BodeRun => String::new(),
            Item::BodeIn if self.bode_gen() == FREE => match m.bode_sw_in {
                0 => "Off".into(),
                k => format!("CH{}", k),
            },
            Item::BodeIn => self.wave_name(m.bode_in),
            Item::BodeOut => self.wave_name(m.bode_out),
            Item::BodeGen => SOURCES[self.bode_source()].into(),
            Item::BodeCur => self.bode_cursor_text(),
            Item::BodeSync => format!("CH{}", m.bode_sync + 1),
            _ => return None,
        })
    }

    pub(super) fn bode_options(&self, it: Item) -> Option<Vec<String>> {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        Some(match it {
            Item::BodeOn => v(&["Off", "On"]),
            Item::BodeIn if self.bode_gen() == FREE => opt_ch_names("Off"),
            Item::BodeIn | Item::BodeOut => self.wave_names(),
            Item::BodeSync => ch_names(),
            Item::BodeGen => v(&SOURCES),
            _ => return None,
        })
    }

    pub(super) fn bode_adjust(&mut self, it: Item, d: i32) -> bool {
        let free = self.bode_gen() == FREE;
        // the free sweep's input is an optional channel (`bode_sw_in`); otherwise both are any wave, a ref too
        if it == Item::BodeOut || (it == Item::BodeIn && !free) {
            if it == Item::BodeIn {
                self.m.bode_in = self.step_wave(self.m.bode_in, d);
            } else {
                self.m.bode_out = self.step_wave(self.m.bode_out, d);
            }
            return true;
        }
        // a generator that measures in another mode (the free sweep always in dB): the points of the last sweep are of
        // the mode before, so they go, as with a change of the mode itself (setting_items.rs)
        if it == Item::BodeGen {
            let before = self.bode_mode();
            // the external generator is the model of its page
            let next = (self.bode_source() as i32 + d).rem_euclid(SOURCES.len() as i32) as usize;
            self.m.bode_gen = [BUILTIN, self.ext_model(), FREE][next];
            if self.bode_mode() != before {
                self.bode.points.clear();
                self.bode.cursor = 0;
            }
            return true;
        }
        let m = &mut self.m;
        match it {
            Item::BodeOn => super::analysers::toggle_view(m, AView::Bode),
            Item::BodeIn if free => m.bode_sw_in = opt_src_step(m.bode_sw_in, d),
            Item::BodeSync => m.bode_sync = src_step(m.bode_sync, NCH, d),
            Item::BodeCur => {
                let n = self.bode.points.len() as i32;
                self.bode.cursor = (self.bode.cursor as i32 + d).clamp(0, (n - 1).max(0)) as usize;
            }
            _ => return false,
        }
        true
    }

    /// Multi 2 with the menu closed: moves the cursor of a Bode plot that has points; false where there is none.
    pub(super) fn bode_cursor_step(&mut self, d: i32) -> bool {
        if self.m.view != AView::Bode || self.bode.points.is_empty() {
            return false;
        }
        self.adjust(Item::BodeCur, d);
        true
    }

    pub(super) fn bode_activate(&mut self, it: Item) -> bool {
        if it == Item::BodeSave {
            let p = self.new_file(&self.save_dir(), "bode", "csv");
            let r = crate::store::write(&p, self.bode_csv().as_bytes());
            self.toast_saved(&p, r);
            return true;
        }
        if it != Item::BodeRun {
            return false;
        }
        if self.bode.running {
            self.bode_finish("Bode sweep stopped");
        } else {
            self.bode_begin();
        }
        true
    }

    fn bode_gen(&self) -> usize {
        self.m.bode_gen.min(GENERATORS.len() - 1)
    }

    /// The generator as the Generator row names it (an index of `SOURCES`).
    fn bode_source(&self) -> usize {
        match self.bode_gen() {
            BUILTIN => 0,
            FREE => 2,
            _ => 1,
        }
    }

    /// The mode the points are in: a free sweep always measures gain in dB.
    fn bode_mode(&self) -> oos_dsp::bode::Mode {
        if self.bode_gen() == FREE {
            oos_dsp::bode::Mode::Loop
        } else {
            self.m.bode_mode
        }
    }

    fn bode_send_freq(&mut self) {
        let f = self.bode.freqs[self.bode.idx];
        if self.bode_gen() == BUILTIN {
            self.afg_set(Some(true), Some(f), Some(self.m.bode_amp), Some(0.0));
        } else {
            let l = crate::extgen::lines(GENERATORS[self.ext_model()].step, &self.ext_vars(f, self.m.bode_amp));
            self.ext_send(l);
            self.bode.pending = true;
        }
        // about 5 periods on the screen
        let want = bode::tdiv_for(f, 5.0) as f32;
        let ts = tscales();
        self.m.tscale = ts.iter().position(|&t| t >= want * 0.999).unwrap_or(ts.len() - 1);
        self.bode.wait = 2;
        self.m.hw_stamp += 1;
        self.bode.stamp = self.m.hw_stamp;
        self.settings_changed();
    }

    pub(super) fn bode_begin(&mut self) {
        self.bode.no_phase = None;
        if self.bode_gen() == FREE {
            return self.free_begin();
        }
        if !self.bode_sources_are_refs() && self.bode_gen() == BUILTIN && !oos_hal::model::platform().caps.has_gen() {
            let name = &oos_hal::model::platform().caps.name;
            return self.toast(&format!("No built-in generator on the {}: choose an external one", name));
        }
        self.m.view = AView::Bode;
        self.bode.freqs = bode::log_sweep(self.m.bode_start, self.m.bode_stop, self.m.bode_points);
        self.bode.points.clear();
        self.bode.idx = 0;
        // two stored records: the sweep is the sine fit of those samples, with no generator and no front end
        if self.bode_sources_are_refs() {
            let rec = Self::blank_record();
            let freqs = self.bode.freqs.clone();
            for &f in &freqs {
                let _ = self.bode_point(&rec, f);
            }
            self.bode.idx = self.bode.freqs.len();
            self.toast(&format!("Bode sweep done: {} points", self.bode.points.len()));
            return;
        }
        self.bode.saved = Some((self.m.tscale, self.m.trig_src, self.m.trig_level));
        // trigger on a live input at its centre, free running if nothing triggers
        let (inp, out) = (self.m.bode_in, self.m.bode_out);
        if let WaveId::Ch(c) = inp {
            self.m.ch[c].on = true;
        }
        if let WaveId::Ch(c) = out {
            self.m.ch[c].on = true;
        }
        let trig = match inp {
            WaveId::Ch(c) => c,
            WaveId::Ref(_) => match out {
                WaveId::Ch(c) => c,
                WaveId::Ref(_) => self.m.trig_src,
            },
        };
        (self.m.trig_type, self.m.trig_src, self.m.trig_level, self.m.trig_mode, self.m.run) =
            (TrigType::Edge, trig, 0.0, TrigMode::Auto, RunState::Run);
        self.bode.running = true;
        if self.bode_gen() == BUILTIN {
            self.bode.afg_saved = Some((
                self.m.afg_on,
                self.m.afg_freq,
                self.m.afg_ampl,
                self.m.afg_offset,
                self.m.afg_wave,
                self.m.afg_mod,
            ));
            // the sweep measures unmodulated sines
            (self.m.afg_wave, self.m.afg_mod) = (0, 0);
            self.bode_send_freq();
        } else {
            // the connection of the Ext generator page if it is open, else one for this sweep
            self.ext.shared.lock().unwrap().err = None;
            if !self.ext_linked() {
                self.ext_connect(false);
                self.ext.by_bode = true;
            }
            let g = &GENERATORS[self.ext_model()];
            let v = self.ext_vars(self.bode.freqs[0], self.m.bode_amp);
            self.ext_send(crate::extgen::lines(g.setup, &v));
            self.bode_send_freq();
            self.ext_send(crate::extgen::lines(g.on, &v));
            self.ext.out = true;
        }
        self.toast("Bode sweep started");
    }

    pub(super) fn bode_finish(&mut self, msg: &str) {
        self.bode.running = false;
        self.bode.pending = false;
        if self.ext.by_bode {
            self.ext_disconnect();
        }
        (self.bode.free, self.bode.capturing) = (false, false);
        if let Some(s) = self.bode.free_saved.take() {
            let m = &mut self.m;
            (m.tscale, m.delay, m.trig_type, m.trig_src, m.trig_la, m.trig_level, m.slope) =
                (s.tscale, s.delay, s.trig_type, s.trig_src, s.trig_la, s.trig_level, s.slope);
            (m.trig_mode, m.mdepth, m.auto_roll, m.acq, m.run) = (s.trig_mode, s.mdepth, s.auto_roll, s.acq, s.run);
            self.dirty_map = true;
            self.settings_changed();
        }
        if let Some((on, f, a, o, w, md)) = self.bode.afg_saved.take() {
            (self.m.afg_wave, self.m.afg_mod) = (w, md);
            self.afg_set(Some(on), Some(f), Some(a), Some(o));
        }
        if let Some((ts, src, lvl)) = self.bode.saved.take() {
            (self.m.tscale, self.m.trig_src, self.m.trig_level) = (ts, src, lvl);
            self.settings_changed();
        }
        self.toast(msg);
    }

    /// Every wave this sweep reads is a stored ref.
    fn bode_sources_are_refs(&self) -> bool {
        let (inp, out) = (self.m.bode_in, self.m.bode_out);
        let one = inp == out && self.bode_gen() == BUILTIN;
        let ids = if one { [out, out] } else { [inp, out] };
        ids.iter().all(|id| matches!(id, WaveId::Ref(_)))
    }

    /// One frequency. False when a source has no samples yet, so the sweep waits. A ref is its own `dt`,
    /// with no coupling and no bandwidth limit.
    fn bode_point(&mut self, rec: &Record, f: f64) -> bool {
        let (inp, out) = (self.m.bode_in, self.m.bode_out);
        let mode = self.m.bode_mode;
        if inp == out && self.bode_gen() == BUILTIN {
            let Some(s) = self.wave_samples(rec, out) else { return false };
            if let Some((a, _, _)) = bode::sine_fit(&s.v, s.dt, f).filter(|_| self.m.afg_ampl > 0.0) {
                let ratio = 2.0 * a / self.m.afg_ampl;
                let mid = match mode {
                    oos_dsp::bode::Mode::Psrr => -20.0 * ratio.log10(),
                    oos_dsp::bode::Mode::Z => ratio,
                    oos_dsp::bode::Mode::Loop => 20.0 * ratio.log10(),
                };
                self.bode.points.push((f, mid, f64::NAN));
            }
            return true;
        }
        let Ok(t) = self.take_waves(rec, &[inp, out], &[]) else { return false };
        if let Some((g, ph)) = bode::response(mode, &t.v[0], &t.v[1], t.dt, f) {
            self.bode.points.push((f, g, ph));
        }
        true
    }

    /// One sweep step per new record: measure, then the next frequency.
    pub(super) fn bode_update(&mut self) {
        if !self.bode.running {
            return;
        }
        if self.bode.free {
            return self.free_update();
        }
        // what the generator thread says is for this sweep only while it steps the external generator
        let (e, n, done) = if self.ext_in_sweep() {
            let mut s = self.ext.shared.lock().unwrap();
            (s.err.take(), s.note.take(), s.done)
        } else {
            (None, None, 0)
        };
        if let Some(e) = e {
            self.bode_finish(&e);
            return;
        }
        if let Some(n) = n {
            self.toast(&n);
        }
        let Some(rec) = self.last[0].clone() else { return };
        if rec.raw.as_ref().is_some_and(|r| r.frame.stamp < self.bode.stamp) {
            return;
        }
        if self.bode.pending {
            if done < self.ext.sent {
                return;
            }
            // the generator has the frequency from here on: the settling records are counted from now
            (self.bode.pending, self.bode.wait) = (false, 2);
        }
        if self.bode.wait > 0 {
            self.bode.wait -= 1;
            return;
        }
        let f = self.bode.freqs[self.bode.idx];
        if !self.bode_point(&rec, f) {
            return;
        }
        self.bode.idx += 1;
        if self.bode.idx >= self.bode.freqs.len() {
            self.bode_finish(&format!("Bode sweep done: {} points", self.bode.points.len()));
        } else {
            self.bode_send_freq();
        }
    }

    /// The free sweep as set in the menu.
    fn free_sweep(&self) -> bode::Sweep {
        let m = &self.m;
        bode::Sweep { start: m.bode_start, stop: m.bode_stop, time: m.bode_sw_time, log: m.bode_sw_log }
    }

    /// Free sweep: one record of the whole sweep, triggered once on the sync edge (`free_plan`).
    fn free_begin(&mut self) {
        let sw = self.free_sweep();
        let Some(out) = self.m.bode_out.ch() else {
            return self.toast("Free sweep: the output must be a channel, not a reference");
        };
        let (sync, inp) = (self.m.bode_sync, self.m.bode_sw_in.checked_sub(1));
        if !sw.valid() {
            return self.toast("Free sweep: the stop frequency must be above the start");
        }
        if sync == out || inp == Some(sync) {
            return self.toast("Free sweep: the sync channel must not be the input or output");
        }
        if inp == Some(out) {
            return self.toast("Free sweep: input and output must be different channels");
        }
        let mut want = self.m.clone();
        for c in [Some(out), Some(sync), inp].into_iter().flatten() {
            want.ch[c].on = true;
        }
        let plan = match free_plan(&want, &sw) {
            Ok(p) => p,
            Err(e) => return self.toast(&e),
        };
        let m = &mut self.m;
        self.bode.free_saved = Some(FreeSaved {
            tscale: m.tscale,
            delay: m.delay,
            trig_type: m.trig_type,
            trig_src: m.trig_src,
            trig_la: m.trig_la,
            trig_level: m.trig_level,
            slope: m.slope,
            trig_mode: m.trig_mode,
            mdepth: m.mdepth,
            auto_roll: m.auto_roll,
            acq: m.acq,
            run: m.run,
        });
        m.ch = want.ch;
        (m.tscale, m.delay, m.mdepth, m.auto_roll, m.acq) =
            (plan.tscale, plan.delay, plan.mdepth, false, AcqMode::Normal);
        let slope = if m.bode_sync_slope == Slope::Fall { Slope::Fall } else { Slope::Rise };
        (m.trig_type, m.trig_src, m.trig_la, m.trig_level, m.slope) =
            (TrigType::Edge, sync, None, m.bode_sync_level, slope);
        (m.trig_mode, m.run, m.view) = (TrigMode::Single, RunState::Run, AView::Bode);
        m.hw_stamp += 1;
        self.bode.stamp = m.hw_stamp;
        self.bode.points.clear();
        self.bode.cursor = 0;
        (self.bode.running, self.bode.free, self.bode.capturing) = (true, true, false);
        self.dirty_map = true;
        self.settings_changed();
        let slow = if plan.scan { " (slow time base: gain only)" } else { "" };
        self.toast(&format!("Waiting for the sync edge on CH{}: start the sweep{}", sync + 1, slow));
    }

    /// Free sweep: the SINGLE record of the sweep, once it is complete.
    fn free_update(&mut self) {
        let Some(rec) = self.last[0].clone() else { return };
        if rec.raw.as_ref().is_some_and(|r| r.frame.stamp < self.bode.stamp) {
            return;
        }
        if rec.partial {
            // a scan writes the sweep picture from the left; SINGLE stops on the complete one
            self.bode.capturing = true;
            return;
        }
        if self.m.run != RunState::Stop {
            return;
        }
        let msg = self.free_analyse(&rec);
        self.bode_finish(&msg);
    }

    /// Time from the trigger point of sample 0 of `meas_slice`.
    fn meas_t0(&self, rec: &Record, c: usize) -> f64 {
        match (&rec.raw, self.meas_frame(rec, c)) {
            (Some(r), Some(_)) => r.t0,
            _ => rec.t0,
        }
    }

    /// Gain (and phase) of the free sweep in `rec`: the sync edge is time 0 of the sweep. Returns the message.
    fn free_analyse(&mut self, rec: &Record) -> String {
        let sw = self.free_sweep();
        let m = &self.m;
        let Some(out) = m.bode_out.ch() else {
            return "Free sweep: the output must be a channel, not a reference".into();
        };
        let (sync, inp) = (m.bode_sync, m.bode_sw_in.checked_sub(1));
        let (vo, dt) = self.meas_slice(rec, out);
        let (vs, dts) = self.meas_slice(rec, sync);
        let vi = inp.map(|c| self.meas_slice(rec, c));
        if vo.is_empty() || vs.is_empty() || dts != dt || vi.is_some_and(|(v, d)| v.is_empty() || d != dt) {
            return "Free sweep: a channel is missing in the record".into();
        }
        // min/max pairs at the scan time bases
        let envelope = self.meas_frame(rec, out).is_some_and(|(f, _, _)| f.envelope);
        let hyst = m.trig_hyst_div() * m.ch[sync].volts_per_div();
        let rising = m.bode_sync_slope != Slope::Fall;
        let Some(ts) = bode::sync_start(vs, dt, self.meas_t0(rec, sync), m.bode_sync_level, hyst, rising, sw.time)
        else {
            return format!("No sync edge on CH{} in the record: check its edge and level", sync + 1);
        };
        let r = bode::sweep_response(
            &sw,
            &bode::SweepRecord { out: vo, inp: vi.map(|v| v.0), dt, t0: self.meas_t0(rec, out) - ts, envelope },
            m.bode_points,
        );
        // without an input: the output in dBV (RMS of the sine) against the reference level
        let ref_dbv = m.bode_ref;
        self.bode.points = r
            .points
            .iter()
            .map(|p| {
                let g = match p.inp {
                    Some(a) => 20.0 * (p.out / a).log10(),
                    None => 20.0 * (p.out / 2f64.sqrt()).log10() - ref_dbv,
                };
                (p.f, g, p.phase.unwrap_or(f64::NAN))
            })
            .filter(|p| p.1.is_finite())
            .collect();
        self.bode.cursor = 0;
        self.bode.no_phase = if inp.is_none() {
            Some("No phase without an input channel")
        } else if envelope {
            Some("No phase at slow time bases (min/max data)")
        } else {
            None
        };
        let why: Vec<&str> = [
            (r.too_fast, "sweep too fast for the low end"),
            (r.too_high, "sample rate too low for the top"),
            (r.outside, "sweep not in the record"),
        ]
        .iter()
        .filter(|w| w.0 > 0)
        .map(|w| w.1)
        .collect();
        let n = self.bode.points.len();
        match (n, why.is_empty()) {
            (0..=2, true) => format!("Too few points ({}): no signal on CH{}", n, out + 1),
            (0..=2, false) => format!("Too few points ({}): {}", n, why.join(", ")),
            (_, true) => format!("Sweep captured: {} points", n),
            (_, false) => format!(
                "Sweep captured: {} points; {} left out: {}",
                n,
                r.too_fast + r.too_high + r.outside,
                why.join(", ")
            ),
        }
    }

    /// SCPI: start/stop the sweep and the result as "f,middle,phase_deg;..." (empty while none).
    /// The middle field is dB for the control loop and for PSRR (and always for a free sweep). In impedance mode it
    /// is the linear Vout/Vin ratio.
    pub fn bode_run(&mut self, on: bool) {
        if on != self.bode.running {
            self.bode_activate(Item::BodeRun);
        }
    }
    /// The points as a CSV file (#53): a header, then frequency, gain and phase per line; the gain in dB, or the
    /// linear Vout/Vin ratio of an impedance sweep, and no phase where there is none.
    pub(super) fn bode_csv(&self) -> String {
        let gain = if self.bode_mode() == oos_dsp::bode::Mode::Z { "Vout/Vin" } else { "Gain (dB)" };
        let mut s = format!("Frequency (Hz),{},Phase (deg)\n", gain);
        for &(hz, g, p) in &self.bode.points {
            let p = if p.is_nan() { String::new() } else { format!("{:.2}", p) };
            s.push_str(&format!("{},{:.3},{}\n", hz, g, p));
        }
        s
    }

    pub fn bode_text(&self) -> String {
        self.bode
            .points
            .iter()
            .map(|(f, g, p)| format!("{},{:.3},{:.2}", bode::fmt_hz(*f), g, p))
            .collect::<Vec<_>>()
            .join(";")
    }

    /// The point under the cursor: frequency, gain, phase.
    fn bode_cursor_text(&self) -> String {
        let z = self.bode_mode() == oos_dsp::bode::Mode::Z;
        match self.bode.points.get(self.bode.cursor) {
            Some(&(hz, g, p)) if z && p.is_nan() => format!("{} {:.3}", si_short(hz, "Hz"), g),
            Some(&(hz, g, p)) if z => format!("{} {:.3} {:.0}\u{b0}", si_short(hz, "Hz"), g, p),
            Some(&(hz, g, p)) if p.is_nan() => format!("{} {:.1} dB", si_short(hz, "Hz"), g),
            Some(&(hz, g, p)) => format!("{} {:.1} dB {:.0}\u{b0}", si_short(hz, "Hz"), g, p),
            None => "-".into(),
        }
    }

    /// Gain (top) and phase (bottom) over log frequency.
    pub(super) fn draw_bode(&self, cv: &mut Canvas) {
        let a = self.plot.area;
        let f = self.fonts();
        let (f0, f1) = (self.m.bode_start.ln(), self.m.bode_stop.ln());
        let x_of = |hz: f64| a.x + ((hz.ln() - f0) / (f1 - f0) * a.w as f64).round() as i32;
        let (gt, gb) = (a.y + 8, a.y + a.h / 2 - 8);
        let (pt, pb) = (a.y + a.h / 2 + 8, a.y + a.h - 24);
        let (gmin, gmax) = self.bode_gain_range();
        let gy = |g: f64| gb - ((g - gmin) / (gmax - gmin) * (gb - gt) as f64).round() as i32;
        let py = |p: f64| pb - ((p + 180.0) / 360.0 * (pb - pt) as f64).round() as i32;
        // decade lines
        let mut d = 10f64.powf(self.m.bode_start.log10().floor());
        while d <= self.m.bode_stop * 1.001 {
            if d >= self.m.bode_start * 0.999 {
                let x = x_of(d);
                cv.vline(x, gt, gb, LINE);
                cv.vline(x, pt, pb, LINE);
                cv.text(&f.s, x + 2, a.y + a.h - 6, &si_short(d, "Hz"), DIM);
            }
            d *= 10.0;
        }
        self.draw_bode_values(cv, (gt, gb, gmin, gmax), (pt, pb));
        let col_g = self.wave_color(self.m.bode_out);
        let mut prev: Option<((i32, i32), (i32, i32))> = None;
        for &(hz, g, p) in &self.bode.points {
            let (x, yg, yp) = (x_of(hz), gy(g), py(p));
            if let Some(((x0, g0), (_, p0))) = prev {
                cv.line(x0, g0, x, yg, col_g);
                if !p.is_nan() && p0 != i32::MIN && (yp - p0).abs() < (pb - pt) / 2 {
                    cv.line(x0, p0, x, yp, ACCENT);
                }
            }
            cv.fill(Rect::new(x - 1, yg - 1, 3, 3), col_g);
            let yp = if p.is_nan() { i32::MIN } else { yp };
            if !p.is_nan() {
                cv.fill(Rect::new(x - 1, yp - 1, 3, 3), ACCENT);
            }
            prev = Some(((x, yg), (x, yp)));
        }
        if let Some(&(hz, ..)) = self.bode.points.get(self.bode.cursor) {
            let x = x_of(hz);
            cv.dashed_vline(x, gt, pb, 4, 3, CURSOR);
            cv.text(&f.s, a.x + 6, a.y + a.h / 2 + 2, &self.bode_cursor_text(), CURSOR);
        }
        let free = self.bode_gen() == FREE;
        let refn = match self.m.bode_sw_in {
            0 if free => format!("{:.1} dBV", self.m.bode_ref),
            k if free => format!("CH{}", k),
            _ if self.m.bode_in == self.m.bode_out && self.bode_gen() == BUILTIN => "generator".to_string(),
            _ => self.wave_name(self.m.bode_in),
        };
        let mode = self.bode_mode();
        let (what, span) = match mode {
            // + 0.0: a range that ends at -0 reads 0
            oos_dsp::bode::Mode::Psrr => ("PSRR", format!("{:.0} .. {:.0} dB", gmin + 0.0, gmax + 0.0)),
            oos_dsp::bode::Mode::Z => ("Vout/Vin", format!("{:.3} .. {:.3}", gmin + 0.0, gmax + 0.0)),
            oos_dsp::bode::Mode::Loop => ("Gain", format!("{:.0} .. {:.0} dB", gmin + 0.0, gmax + 0.0)),
        };
        cv.text(
            &f.s,
            a.x + 6,
            gt + 14,
            &format!("{} {} / {}  {}", what, self.wave_name(self.m.bode_out), refn, span),
            col_g,
        );
        // a free sweep has a phase only from the fit of both channels on plain samples
        let phase_text = match (free, self.m.bode_sw_in, self.bode.no_phase) {
            (true, 0, _) => "No phase without an input channel",
            (true, _, Some(why)) => why,
            (true, _, None) => "Phase -180 .. 180\u{b0} (fit of both channels)",
            _ => "Phase -180 .. 180\u{b0}",
        };
        cv.text(&f.s, a.x + 6, pt + 14, phase_text, ACCENT);
        let status = self.bode_status(free, mode);
        // an open loop passes one of these two; a filter sweep neither
        let mg = oos_dsp::bode::margins(&self.bode.points);
        if mode == oos_dsp::bode::Mode::Loop && !self.bode.running && (mg.gain.is_some() || mg.phase.is_some()) {
            let gm = mg.gain.map_or("Gain margin -".to_string(), |(v, hz)| {
                format!("Gain margin {:.1} dB at {}", v, si_short(hz, "Hz"))
            });
            let pm = mg.phase.map_or("phase margin -".to_string(), |(v, hz)| {
                format!("phase margin {:.0}\u{b0} at {}", v, si_short(hz, "Hz"))
            });
            cv.text(&f.s, a.x + 6, pt + 30, &format!("{}, {}", gm, pm), TEXT);
        }
        // right-aligned, but left of the full-screen, zoom and more buttons in the corner, and inside the plot
        let right = (self.full_btn().0.x - 8).min(a.x + a.w - 8);
        let w = (f.s.width(&status) + 16).min((right - a.x - 8).max(40));
        let r = Rect::new(right - w, a.y + 22, w, 20);
        cv.round(r, 4, PANEL_BG, 230);
        cv.set_clip(r);
        cv.text_in(&f.s, r, 1, &status, TEXT);
        cv.no_clip();
    }

    /// The values of both axes (#387): a line at every round step of the gain (dB, or V/V for impedance) and of the
    /// phase, each labelled at the left edge, above its line or below it where a title stands, else left out.
    fn draw_bode_values(&self, cv: &mut Canvas, gain: (i32, i32, f64, f64), phase: (i32, i32)) {
        let a = self.plot.area;
        let f = self.fonts();
        let (gt, gb, gmin, gmax) = gain;
        let (pt, pb) = phase;
        let mid = a.y + a.h / 2;
        // the rows the titles, the cursor's text and the margins take
        let taken = [(gt, gt + 16), (mid - 12, mid + 4), (pt, pt + 16), (pt + 18, pt + 32)];
        let free = |top: i32| top >= a.y && !taken.iter().any(|&(t, b)| top < b && top + 14 > t);
        let label = |cv: &mut Canvas, y: i32, text: &str, col| {
            if free(y - 15) {
                cv.text(&f.s, a.x + 6, y - 3, text, col);
            } else if free(y + 1) {
                cv.text(&f.s, a.x + 6, y + 13, text, col);
            }
        };
        let z = self.bode_mode() == oos_dsp::bode::Mode::Z;
        for (g, text) in gain_values(gmin, gmax, z) {
            let y = gb - ((g - gmin) / (gmax - gmin) * (gb - gt) as f64).round() as i32;
            cv.hline(a.x, a.x + a.w, y, LINE);
            label(cv, y, &text, DIM);
        }
        for (p, text) in PHASE_VALUES {
            let y = pb - ((p + 180.0) / 360.0 * (pb - pt) as f64).round() as i32;
            cv.hline(a.x, a.x + a.w, y, LINE);
            label(cv, y, text, DIM);
        }
    }

    /// The ends of the gain axis for the points measured so far.
    fn bode_gain_range(&self) -> (f64, f64) {
        // loop and PSRR: the measured range rounded to 10 dB. Impedance is a linear ratio, padded by about 10 %.
        let z = self.bode_mode() == oos_dsp::bode::Mode::Z;
        let (gmin, gmax) = self.bode.points.iter().fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        if z {
            if gmin > gmax {
                (0.0, 2.0)
            } else {
                let pad = (gmax - gmin).abs().max(gmax.abs()).max(1e-3) * 0.1;
                (gmin - pad, gmax + pad)
            }
        } else if gmin > gmax {
            (-40.0, 10.0)
        } else {
            ((gmin / 10.0).floor() * 10.0 - 10.0, (gmax / 10.0).ceil() * 10.0)
        }
    }

    /// The status line of the Bode page: what the sweep does, its result, or how to start it.
    fn bode_status(&self, free: bool, mode: oos_dsp::bode::Mode) -> String {
        let sync_text = || {
            let edge = if self.m.bode_sync_slope == Slope::Fall { "falling" } else { "rising" };
            format!(
                "waiting for the sync edge on CH{} ({}, {})",
                self.m.bode_sync + 1,
                edge,
                si_short(self.m.bode_sync_level as f64, "V")
            )
        };
        let idle = || {
            if free {
                let law = if self.m.bode_sw_log { "log" } else { "lin" };
                format!(
                    "free sweep {} .. {} in {}, {} (Bode > Start)",
                    si_short(self.m.bode_start, "Hz"),
                    si_short(self.m.bode_stop, "Hz"),
                    si_short(self.m.bode_sw_time, "s"),
                    law
                )
            } else if self.bode_gen() == BUILTIN {
                "built-in generator (Bode > Start)".to_string()
            } else {
                let at = if self.m.bode_usb { "USB" } else { self.m.bode_addr.as_str() };
                format!("generator {} at {} (Bode > Start)", GENERATORS[self.ext_model()].name, at)
            }
        };
        if self.bode.running && self.bode.free {
            if self.bode.capturing {
                format!("recording the sweep ({})", si_short(self.m.bode_sw_time, "s"))
            } else {
                sync_text()
            }
        } else if self.bode.running {
            format!(
                "sweeping {} of {}: {}",
                self.bode.idx + 1,
                self.bode.freqs.len(),
                si_short(self.bode.freqs[self.bode.idx], "Hz")
            )
        } else if mode == oos_dsp::bode::Mode::Loop {
            if let Some(&(hz, g, p)) =
                self.bode.points.iter().min_by(|a, b| (a.1 + 3.0).abs().total_cmp(&(b.1 + 3.0).abs()))
            {
                let ph = if p.is_nan() { String::new() } else { format!(", {:.0}\u{b0}", p) };
                format!("{} points; nearest -3 dB: {} ({:.1} dB{})", self.bode.points.len(), si_short(hz, "Hz"), g, ph)
            } else {
                idle()
            }
        } else if self.bode.points.is_empty() {
            idle()
        } else {
            format!("{} points", self.bode.points.len())
        }
    }
}

/// The Bode page: a frequency sweep of the generator (it runs on its own, not from a record).
pub(super) struct BodePage;

impl super::analysers::Analyser for BodePage {
    fn view(&self) -> AView {
        AView::Bode
    }

    fn feature(&self) -> (&'static str, &'static str) {
        ("bode", "Bode")
    }

    fn name(&self) -> &'static str {
        "Bode plot"
    }

    fn menu(&self) -> MenuId {
        MenuId::Bode
    }

    fn scpi(&self) -> Option<&'static str> {
        Some("BODE")
    }

    fn draw(&self, ui: &Ui, cv: &mut Canvas) {
        ui.draw_bode(cv);
    }
}

/// The values of the phase axis and their labels, every 90 degrees.
const PHASE_VALUES: [(f64, &str); 5] =
    [(-180.0, "-180\u{b0}"), (-90.0, "-90\u{b0}"), (0.0, "0\u{b0}"), (90.0, "90\u{b0}"), (180.0, "180\u{b0}")];

/// The values of the gain axis from `gmin` to `gmax` at a round step, with their labels: dB, or the linear ratio
/// of an impedance sweep (`z`) in V/V with as many decimals as the step needs. A value of -0 reads 0.
fn gain_values(gmin: f64, gmax: f64, z: bool) -> Vec<(f64, String)> {
    let step = axis_step(gmax - gmin, 5);
    let digits = if z { (-step.log10().floor()).max(0.0) as usize } else { 0 };
    let first = (gmin / step).ceil() as i64;
    let last = (gmax / step + 1e-6).floor() as i64;
    (first..=last)
        .map(|k| {
            let g = k as f64 * step + 0.0;
            (g, if z { format!("{:.*} V/V", digits, g) } else { format!("{:.0} dB", g) })
        })
        .collect()
}

/// A round step (1, 2 or 5 times a power of ten) that divides `span` into at most `lines` parts.
fn axis_step(span: f64, lines: usize) -> f64 {
    let raw = (span / lines as f64).max(f64::MIN_POSITIVE);
    let p = 10f64.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|k| k * p).find(|&s| s >= raw).unwrap_or(10.0 * p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A gain of NaN among the points: the point nearest -3 dB was chosen with `partial_cmp(..).unwrap()`, which
    /// panicked; a NaN distance is now the largest.
    #[test]
    fn status_passes_over_a_nan_gain() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.bode.points = vec![(1e3, f64::NAN, f64::NAN), (2e3, -2.5, -40.0), (3e3, -6.0, -60.0)];
        let s = ui.bode_status(false, oos_dsp::bode::Mode::Loop);
        assert!(s.contains("nearest -3 dB: 2 kHz (-2.5 dB"), "{s}");
    }

    /// A free log sweep 20 Hz .. 20 kHz in 0.5 s, its record handed in as the SINGLE record: CH1 the generator, CH2
    /// a first-order low-pass at 1 kHz, CH4 the sync pulse at the start.
    #[test]
    fn free_sweep_reads_one_record() {
        let mut ui = crate::ui::testkit::demo_ui();
        (ui.m.bode_gen, ui.m.bode_start, ui.m.bode_stop, ui.m.bode_points) = (FREE, 20.0, 20e3, 40);
        (ui.m.bode_sw_time, ui.m.bode_sw_log, ui.m.bode_sw_in, ui.m.bode_out, ui.m.bode_sync, ui.m.bode_sync_level) =
            (0.5, true, 1, WaveId::Ch(1), 3, 1.5);
        let (ts0, trig0, mode0) = (ui.m.tscale, ui.m.trig_src, ui.m.trig_mode);
        assert!(
            ui.items(MenuId::Bode).contains(&Item::BodeSyncLvl) && !ui.items(MenuId::Bode).contains(&Item::BodeAmp)
        );
        ui.activate(Item::BodeRun);
        assert!(ui.bode.running && ui.bode.free, "{:?}", ui.toast);
        assert!(ui.toast.as_ref().unwrap().0.contains("sync edge on CH4"));
        assert_eq!(
            (ui.m.trig_src, ui.m.trig_mode, ui.m.trig_type, ui.m.run),
            (3, TrigMode::Single, TrigType::Edge, RunState::Run)
        );
        assert!(ui.m.ch[0].on && ui.m.ch[1].on && ui.m.ch[3].on);
        // 100 ms/div: the trigger one division from the left, 9 for the sweep; a depth with 20 samples per period
        let tdiv = tscales()[ui.m.tscale] as f64;
        assert!((tdiv - 0.1).abs() < 1e-6 && (ui.m.delay as f64 - 0.4).abs() < 1e-6, "{} {}", tdiv, ui.m.delay);
        assert!(ui.m.srate_depth().0 >= 400e3, "{:?}", ui.m.srate_depth());
        let mut cv = Canvas::new(PANEL_W, PANEL_H);
        ui.render(&mut cv);
        // the record: from one division before the trigger, 200 kSa/s
        let sw = ui.free_sweep();
        let (dt, t0) = (5e-6, ui.m.delay as f64 - 5.0 * tdiv);
        let n = (10.0 * tdiv / dt) as usize;
        let x: Vec<f32> =
            (0..n).map(|k| (sw.phase_at((t0 + k as f64 * dt).clamp(0.0, sw.time)) + 0.3).sin() as f32).collect();
        let alpha = 1.0 - (-2.0 * std::f64::consts::PI * 1e3 * dt).exp();
        let mut y = 0.0f64;
        let out: Vec<f32> = x
            .iter()
            .map(|&v| {
                y += alpha * (v as f64 - y);
                y as f32
            })
            .collect();
        let sync: Vec<f32> =
            (0..n).map(|k| if (0.0..1e-3).contains(&(t0 + k as f64 * dt)) { 3.3 } else { 0.0 }).collect();
        ui.last[0] = Some(Record {
            t0,
            dt,
            ch: [x, out, Vec::new(), sync],
            triggered: true,
            partial: false,
            raw: None,
            la: None,
            pairs: false,
        });
        ui.m.run = RunState::Stop;
        ui.bode_update();
        assert!(!ui.bode.running && !ui.bode.free);
        // 0.5 s over three decades passes the lowest frequencies in less than a period each
        let t = ui.toast.as_ref().unwrap().0.clone();
        assert!(t.starts_with("Sweep captured") && t.contains("too fast for the low end"), "{}", t);
        assert!(
            ui.bode.points.len() >= 30 && ui.bode.points[0].0 > 50.0 && ui.bode.points[0].0 < 100.0,
            "{:?}",
            ui.bode.points
        );
        // the exact response of that sampled low-pass: alpha / (1 - (1 - alpha) e^-jw)
        let truth = |f: f64| {
            let w = 2.0 * std::f64::consts::PI * f * dt;
            let (re, im) = (1.0 - (1.0 - alpha) * w.cos(), (1.0 - alpha) * w.sin());
            (20.0 * (alpha / (re * re + im * im).sqrt()).log10(), -im.atan2(re).to_degrees())
        };
        for &(f, g, p) in &ui.bode.points {
            let (tg, tp) = truth(f);
            assert!((g - tg).abs() < 0.5 && (p - tp).abs() < 3.0, "{} Hz: {} dB {} deg", f, g, p);
        }
        assert_eq!(
            (ui.m.tscale, ui.m.trig_src, ui.m.trig_mode, ui.m.run),
            (ts0, trig0, mode0, RunState::Run),
            "settings restored"
        );
        ui.render(&mut cv);
        // the same without the input channel: against 1 V peak = -3.01 dBV, no phase
        (ui.m.bode_sw_in, ui.m.bode_ref) = (0, -3.0);
        assert!(ui.items(MenuId::Bode).contains(&Item::BodeRef));
        ui.activate(Item::BodeRun);
        ui.m.run = RunState::Stop;
        ui.bode_update();
        assert!(ui.bode.points.len() >= 30 && ui.bode.points.iter().all(|p| p.2.is_nan()));
        assert!(ui.bode.points.iter().all(|&(f, g, _)| (g - truth(f).0).abs() < 0.5));
        assert_eq!(ui.bode.no_phase, Some("No phase without an input channel"));
        ui.render(&mut cv);
    }

    #[test]
    fn free_sweep_refuses_what_does_not_fit() {
        let mut ui = crate::ui::testkit::demo_ui();
        (ui.m.bode_gen, ui.m.bode_start, ui.m.bode_stop, ui.m.bode_sw_time) = (FREE, 1e3, 200e6, 0.5);
        ui.activate(Item::BodeRun);
        assert!(!ui.bode.running && ui.toast.as_ref().unwrap().0.contains("does not fit the memory"), "{:?}", ui.toast);
        // sync on the output
        (ui.m.bode_stop, ui.m.bode_sync) = (20e3, ui.m.bode_out.ch().unwrap());
        ui.activate(Item::BodeRun);
        assert!(!ui.bode.running && ui.toast.as_ref().unwrap().0.contains("sync channel"));
        // 500 s: a scan at 200 s/div, the trigger at the centre
        (ui.m.bode_sync, ui.m.bode_sw_time) = (3, 500.0);
        let m = ui.m.clone();
        let p = free_plan(&m, &ui.free_sweep()).unwrap();
        assert!(p.scan && p.delay == 0.0 && (tscales()[p.tscale] - 200.0).abs() < 1e-3, "{:?}", p);
        // 2 s: a scan too (normal records end at 100 ms/div)
        ui.m.bode_sw_time = 2.0;
        let p = free_plan(&m, &ui.free_sweep()).unwrap();
        assert!(p.scan && (tscales()[p.tscale] - 0.5).abs() < 1e-6, "{:?}", p);
        // a record that never comes: waiting, then stopped by hand, settings back
        let ts0 = ui.m.tscale;
        ui.activate(Item::BodeRun);
        assert!(ui.bode.running);
        let mut cv = Canvas::new(PANEL_W, PANEL_H);
        ui.render(&mut cv);
        ui.activate(Item::BodeRun);
        assert!(
            !ui.bode.running
                && ui.m.tscale == ts0
                && ui.m.trig_mode == TrigMode::Auto
                && ui.m.auto_roll == Model::default().auto_roll
        );
        // the demo signals through the whole loop: whatever CH4 does, the sweep waits or ends with a message
        ui.m.bode_sw_time = 0.05;
        ui.activate(Item::BodeRun);
        assert!(ui.bode.running);
        for _ in 0..40 {
            ui.step(0.05);
            ui.render(&mut cv);
        }
        assert!(ui.bode.running || (ui.m.tscale == ts0 && ui.m.trig_mode == TrigMode::Auto), "{:?}", ui.toast);
    }

    /// The axes show their values (#387): round steps of the gain and the phase, labelled where no title stands.
    #[test]
    fn the_axes_show_their_values() {
        assert_eq!(
            [axis_step(50.0, 5), axis_step(60.0, 5), axis_step(0.3, 5), axis_step(2.2, 5)],
            [10.0, 20.0, 0.1, 0.5]
        );
        let texts = |v: Vec<(f64, String)>| v.into_iter().map(|(_, t)| t).collect::<Vec<_>>();
        assert_eq!(texts(gain_values(-60.0, -0.0, false)), ["-60 dB", "-40 dB", "-20 dB", "0 dB"]);
        assert_eq!(texts(gain_values(-20.0, 10.0, false)), ["-20 dB", "-10 dB", "0 dB", "10 dB"]);
        // impedance: the linear ratio, with its unit and the decimals of its step
        assert_eq!(texts(gain_values(0.18, 1.12, true)), ["0.2 V/V", "0.4 V/V", "0.6 V/V", "0.8 V/V", "1.0 V/V"]);
        assert_eq!(texts(gain_values(0.0, 2.0, true)), ["0.0 V/V", "0.5 V/V", "1.0 V/V", "1.5 V/V", "2.0 V/V"]);
        assert_eq!(PHASE_VALUES.map(|p| p.1), ["-180\u{b0}", "-90\u{b0}", "0\u{b0}", "90\u{b0}", "180\u{b0}"]);
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.view = AView::Bode;
        // a first-order low pass at 10 kHz, 1 kHz .. 1 MHz
        ui.bode.points = (0..=30)
            .map(|i| {
                let hz = 1e3 * 1e3f64.powf(i as f64 / 30.0);
                let x = hz / 1e4;
                (hz, -10.0 * (1.0 + x * x).log10(), -x.atan().to_degrees())
            })
            .collect();
        ui.bode.cursor = 10;
        let mut cv = Canvas::new(crate::ui::W, crate::ui::H);
        ui.render(&mut cv);
        if let Some(dir) = std::env::var_os("OOS_SHOTS") {
            std::fs::write(std::path::Path::new(&dir).join("bode-axes.png"), crate::store::png_of(&cv)).unwrap();
        }
        let (gmin, gmax) = ui.bode_gain_range();
        assert_eq!((gmin, gmax), (-60.0, 0.0));
    }

    /// The points of a sweep go to a CSV file on the storage (#53): a header, one line a point, no phase where there
    /// is none; the row is there only once there are points.
    #[test]
    fn the_points_are_saved_as_csv() {
        let dir = crate::ui::testkit::Scratch::new("bode-csv");
        let mut ui = crate::ui::testkit::demo_ui();
        ui.files.dir = dir.to_path_buf();
        assert!(!ui.bode_items().contains(&Item::BodeSave), "nothing to save yet");
        ui.bode.points = vec![(1e3, -0.043, -5.71), (1e4, -3.010, -45.0), (1e5, -20.043, f64::NAN)];
        assert!(ui.bode_items().contains(&Item::BodeSave));
        ui.activate(Item::BodeSave);
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.path()).collect();
        assert_eq!(files.len(), 1, "{files:?}");
        assert!(files[0].file_name().unwrap().to_string_lossy().starts_with("bode"), "{files:?}");
        assert_eq!(
            std::fs::read_to_string(&files[0]).unwrap(),
            "Frequency (Hz),Gain (dB),Phase (deg)\n1000,-0.043,-5.71\n10000,-3.010,-45.00\n100000,-20.043,\n"
        );
    }

    /// A generator that measures in another mode drops the points of the last sweep, so neither the plot nor the CSV
    /// shows linear ratios as dB; one in the same mode keeps them.
    #[test]
    fn a_generator_of_another_mode_drops_the_points() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.bode_mode = oos_dsp::bode::Mode::Z;
        ui.m.bode_gen = BUILTIN;
        ui.bode.points = vec![(1e3, 0.5, 10.0)];
        assert!(ui.bode_csv().starts_with("Frequency (Hz),Vout/Vin,"));
        // to the generator after the built-in one: an external one, still in impedance mode
        let next = (BUILTIN + 1) % GENERATORS.len();
        if next != FREE {
            ui.adjust(Item::BodeGen, 1);
            assert_eq!(ui.bode.points.len(), 1, "same mode: the points stay");
        }
        while ui.bode_gen() != FREE {
            ui.adjust(Item::BodeGen, 1);
        }
        assert!(ui.bode.points.is_empty(), "the free sweep measures in dB: the impedance points go");
        assert!(!ui.bode_items().contains(&Item::BodeSave));
    }

    /// With the menu closed, multi 2 moves the cursor of a Bode plot that has points (#50); without points, or with
    /// multi 1, the knob opens the menu as before.
    #[test]
    fn multi_2_moves_the_cursor_with_the_menu_closed() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.view = AView::Bode;
        ui.close_menu();
        ui.knob_turn(key::MULTI2_CW, 1);
        assert!(ui.menu_open, "no points: the menu opens");
        ui.close_menu();
        ui.bode.points = (0..10).map(|i| (1e3 * (i + 1) as f64, -(i as f64), -10.0 * i as f64)).collect();
        ui.knob_turn(key::MULTI2_CW, 3);
        assert!(!ui.menu_open && ui.bode.cursor == 3, "cursor {}", ui.bode.cursor);
        ui.knob_turn(key::MULTI2_CW, -1);
        assert_eq!(ui.bode.cursor, 2);
        ui.knob_turn(key::MULTI2_CW, 50);
        assert_eq!(ui.bode.cursor, 9, "the last point");
        ui.knob_turn(key::MULTI1_CW, 1);
        assert!(ui.menu_open, "multi 1 opens the menu as before");
    }

    /// The amplitude takes any value from the number pad, as a tap on it opens (#52): 350 mVpp, 1.25 Vpp; outside
    /// 10 mVpp to 20 Vpp it is refused.
    #[test]
    fn the_amplitude_takes_any_value_from_the_pad() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.view = AView::Bode;
        ui.open_menu(MenuId::Bode);
        let typed = |ui: &mut Ui, keys: &[&str]| {
            ui.open_row_pad(Item::BodeAmp);
            for k in keys {
                let i = ui.row_pad_keys().iter().position(|x| x == k).unwrap_or_else(|| panic!("no key {k}"));
                ui.row_pad_key(i);
            }
        };
        typed(&mut ui, &["3", "5", "0", "mVpp"]);
        assert!((ui.m.bode_amp - 0.35).abs() < 1e-12 && ui.value(Item::BodeAmp) == "350 mVpp", "{}", ui.m.bode_amp);
        typed(&mut ui, &["1", ".", "2", "5", "Vpp"]);
        assert_eq!((ui.m.bode_amp, ui.value(Item::BodeAmp)), (1.25, "1.25 Vpp".into()));
        typed(&mut ui, &["5", "0", "Vpp"]);
        assert_eq!(ui.m.bode_amp, 1.25, "50 Vpp is refused");
        // the knob still steps 1-2-5 from there: up to the next step, down to the one below, whatever lies between
        ui.adjust(Item::BodeAmp, 1);
        assert_eq!(ui.m.bode_amp, 2.0);
        for (x, d, want) in [(1.999, 1, 2.0), (1.0005, 1, 2.0), (1.25, -1, 1.0), (1.999, -1, 1.0), (2.0, 1, 5.0)] {
            ui.m.bode_amp = x;
            ui.adjust(Item::BodeAmp, d);
            assert_eq!(ui.m.bode_amp, want, "{x} by {d}");
        }
    }

    #[test]
    fn free_sweep_menu_settings() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.bode_gen = FREE;
        assert_eq!(ui.value(Item::BodeGen), "Free-running sweep");
        assert_eq!(
            (ui.value(Item::BodeSwTime), ui.value(Item::BodeSwLog), ui.value(Item::BodeSync), ui.value(Item::BodeIn)),
            ("1 s".into(), "Log".into(), "CH4".into(), "CH1".into())
        );
        ui.adjust(Item::BodeSwTime, 2);
        ui.adjust(Item::BodeSwLog, 1);
        ui.adjust(Item::BodeSyncEdge, 1);
        ui.adjust(Item::BodeSyncLvl, 5);
        ui.adjust(Item::BodeIn, -1);
        assert_eq!(
            (ui.m.bode_sw_time, ui.m.bode_sw_log, ui.m.bode_sync_slope, ui.m.bode_sw_in == 0),
            (5.0, false, Slope::Fall, true)
        );
        assert!((ui.m.bode_sync_level - 1.5).abs() < 1e-6);
        ui.adjust(Item::BodeSwTime, -100);
        assert_eq!(ui.m.bode_sw_time, 1e-3);
        ui.adjust(Item::BodeSwTime, 100);
        assert_eq!(ui.m.bode_sw_time, 500.0);
    }

    // Break it catches: an amplitude knob that starts its ladder at 1 V and so never goes below it, although
    // 10 mVpp .. 20 Vpp are allowed.
    #[test]
    fn the_amplitude_knob_covers_10_mv_to_20_v() {
        let mut ui = crate::ui::testkit::demo_ui();
        assert_eq!(ui.m.bode_amp, 1.0);
        ui.adjust(Item::BodeAmp, -1);
        assert_eq!((ui.m.bode_amp, ui.value(Item::BodeAmp)), (0.5, "500 mVpp".into()));
        ui.adjust(Item::BodeAmp, -100);
        assert_eq!(ui.m.bode_amp, 0.01);
        ui.adjust(Item::BodeAmp, 1);
        assert_eq!(ui.m.bode_amp, 0.02);
        ui.adjust(Item::BodeAmp, 100);
        assert_eq!(ui.m.bode_amp, 20.0);
        ui.adjust(Item::BodeAmp, -1);
        assert_eq!(ui.m.bode_amp, 10.0);
    }

    #[test]
    fn the_external_generator_is_the_one_of_its_page() {
        let mut ui = crate::ui::testkit::demo_ui();
        assert_eq!(ui.value(Item::BodeGen), "Built-in");
        assert!(!ui.items(MenuId::Bode).contains(&Item::Open(MenuId::ExtGen)), "not for the built-in generator");
        ui.adjust(Item::BodeGen, 1);
        assert_eq!((ui.value(Item::BodeGen), ui.m.bode_gen), ("External".into(), ui.m.ext_model));
        assert!(ui.items(MenuId::Bode).contains(&Item::Open(MenuId::ExtGen)), "the way to the generator's page");
        ui.adjust(Item::BodeGen, 1);
        assert_eq!((ui.value(Item::BodeGen), ui.m.bode_gen), ("Free-running sweep".into(), FREE));
        ui.adjust(Item::BodeGen, 1);
        assert_eq!(ui.m.bode_gen, BUILTIN);
        ui.adjust(Item::BodeGen, 1);
        // an older setup names the model in the Bode line: it is the external generator
        ui.m.bode_gen = 5;
        assert_eq!(ui.value(Item::BodeGen), "External");
        let mut cv = Canvas::new(PANEL_W, PANEL_H);
        ui.m.view = AView::Bode;
        ui.render(&mut cv);
        // on USB with no device: the sweep stops with a message that says so, the settings come back
        ui.m.bode_usb = true;
        let ts0 = ui.m.tscale;
        ui.activate(Item::BodeRun);
        assert!(ui.bode.running);
        for _ in 0..100 {
            ui.step(0.05);
            if !ui.bode.running {
                break;
            }
        }
        assert!(!ui.bode.running && ui.m.tscale == ts0);
        let t = ui.toast.as_ref().unwrap().0.clone();
        assert!(t.starts_with("generator USB: no USB device found"), "{}", t);
        assert!(!ui.ext_linked(), "the sweep closes the connection it opened");
        // the setup file: the connection on a line of its own; an older file without it is on the LAN
        let text = crate::store::setup_text(&ui.m);
        assert!(text.contains("bode_link = usb\n"), "{}", text);
        let m = crate::store::parse_setup(&text).unwrap();
        assert!(m.bode_usb && m.bode_addr == "192.168.1.100:5555");
        let old: String = text.lines().filter(|l| !l.starts_with("bode_link")).map(|l| format!("{}\n", l)).collect();
        assert!(!crate::store::parse_setup(&old).unwrap().bode_usb);
    }

    /// A generator with an error queue is asked after every step: the sweep waits for the answer, and ends with
    /// what the generator said when it refuses a frequency.
    #[test]
    fn a_frequency_the_generator_refuses_ends_the_sweep() {
        let (addr, got) = crate::extgen::tests::fake_generator("OWON,DGE2070,1,x", "100000Hz");
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        (ui.m.bode_addr, ui.m.bode_start, ui.m.bode_stop, ui.m.bode_points) = (addr, 1e3, 1e5, 3);
        (ui.m.ext_model, ui.m.bode_gen) = (6, 6);
        ui.activate(Item::BodeRun);
        assert!(ui.bode.running && ui.bode.pending);
        for _ in 0..400 {
            ui.step(0.05);
            if !ui.bode.running {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(!ui.bode.running, "sweep did not end ({} of {})", ui.bode.idx, ui.bode.freqs.len());
        let t = ui.toast.as_ref().unwrap().0.clone();
        assert_eq!(t, "The generator refused SOURce1:FREQuency:FIXed 100000Hz: -222,\"Data out of range\"");
        // the two frequencies before it were measured, each after the generator had it
        assert_eq!(ui.bode.points.len(), 2, "{:?}", ui.bode.points);
        // without the error queries, and the mark of the connection the sweep closed at its end
        let l: Vec<String> = got.lock().unwrap().iter().filter(|l| *l != "SYSTem:ERRor?" && *l != crate::extgen::tests::CLOSED).cloned().collect();
        assert_eq!(
            l,
            [
                "OUTPut1:IMPedance INFinity",
                "SOURce1:FUNCtion:SHAPe SIN",
                "SOURce1:VOLTage:LEVel:IMMediate:OFFSet 0mV",
                "SOURce1:VOLTage:LEVel:IMMediate:AMPLitude 1Vpp",
                "SOURce1:FREQuency:FIXed 1000Hz",
                "OUTPut1:STATe ON",
                "SOURce1:FREQuency:FIXed 10000Hz",
                "SOURce1:FREQuency:FIXed 100000Hz",
            ]
        );
    }
}
