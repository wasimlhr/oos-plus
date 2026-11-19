// SPDX-License-Identifier: GPL-2.0-or-later

//! The monitor's own trace maps (Display > HDMI = Layout or Waveform, ADR 0073). The monitor lays the screen out at
//! its size, up to 1920 x 1080; its plot is about twice the panel's in each direction. Stretching the panel's maps to
//! it made every trace pixel a block of two monitor pixels or more. While a monitor pass draws the traces, the panel's
//! maps carry a second set at the size the monitor drew them (`Phosphor::hi`): every clear and decay of the panel's
//! maps goes to it too, and the monitor's pass draws from it one pixel a cell. Frames go into it within a time budget
//! of a few ms a step (`BUDGET`): the fast path places the newest frames that fit again at its size, the records take
//! it while the budget lasts (the first of a step always), the deposit workers draw the panel's maps alone. So the
//! panel's frames, waveforms a second and screen rate stay those of Mirror; the monitor's picture is graded by its own
//! count of frames. Without a monitor pass for a few steps (no monitor, Mirror) the set is dropped and costs nothing.
//! `OOS_HDMI_TRACES=stretched` or `hdmi.traces = stretched` in system.conf keeps the stretched maps.

use super::*;
use std::cell::Cell;
use std::time::Duration;

/// Steps without a monitor pass after which the monitor's maps are dropped (the monitor gets every second frame).
const KEEP_STEPS: u32 = 8;

/// Time a step may spend drawing frames into the monitor's maps, beyond the first record. The monitor's picture
/// takes a sample of the frames; the panel's take them all, as without a monitor.
pub(super) const BUDGET: Duration = Duration::from_millis(3);

/// The grading of the monitor's maps, by their own counts, as `Ui::vmax` .. `Ui::per_frame_cg` grade the panel's.
#[derive(Clone, Copy)]
pub(super) struct HiGrade {
    vmax: [u32; NCH],
    vmaxz: [u32; NCH],
    vmax_cg: u32,
    vmaxz_cg: u32,
    per_frame: [u32; NCH],
    per_framez: [u32; NCH],
    per_frame_cg: [u32; 2],
}

impl Default for HiGrade {
    fn default() -> HiGrade {
        HiGrade {
            vmax: [1; NCH],
            vmaxz: [1; NCH],
            vmax_cg: 1,
            vmaxz_cg: 1,
            per_frame: [1; NCH],
            per_framez: [1; NCH],
            per_frame_cg: [1; 2],
        }
    }
}

#[derive(Default)]
pub(super) struct MonitorMaps {
    /// the sizes the monitor's last pass drew the main and the zoom maps at (`Ui::trace_maps`)
    drawn: Cell<[Option<(usize, usize)>; 2]>,
    /// steps since the monitor's last pass
    age: u32,
    grade: HiGrade,
    /// the fast path's differences for the monitor's main maps (`Ui::acc_t`)
    pub acc_t: [Vec<u16>; NCH],
    /// frames drawn into the monitor's maps this step, and the count that grades its picture (`Ui::pic_frames`)
    frames: u32,
    pic_frames: u32,
    /// the fast path's time on the monitor's maps this step, and its estimate per frame (ms; 0 not yet known)
    fast_time: Duration,
    fast_ms: f32,
}

/// Whether the monitor gets maps of its own: unless `OOS_HDMI_TRACES` or `hdmi.traces` in system.conf says
/// `stretched` (read once).
fn own_maps() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let env = std::env::var("OOS_HDMI_TRACES").ok();
        let conf = if env.is_some() { String::new() } else { crate::sysconf::fork_text() };
        crate::drawpipe::conf_value(env.as_deref(), &conf, "hdmi.traces").as_deref() != Some("stretched")
    })
}

impl Ui {
    /// The start of a monitor pass (`render_view_layers_as_is`, `with_waveform_pass`): its maps stay, and the sizes
    /// it draws them at are noted again.
    pub(super) fn hi_maps_pass(&mut self) {
        self.hdmi_maps.age = 0;
        self.hdmi_maps.drawn.set([None, None]);
    }

    /// Once a step, before the records are drawn: the monitor's maps at the sizes its last pass drew them, or none
    /// when no pass came for a while. New maps start from the panel's, stretched (`Phosphor::set_hi`).
    pub(super) fn hi_maps_sync(&mut self) {
        let mm = &mut self.hdmi_maps;
        mm.age = mm.age.saturating_add(1);
        (mm.frames, mm.fast_time) = (0, Duration::ZERO);
        (self.ph.hi_time, self.phz.hi_time) = (Duration::ZERO, Duration::ZERO);
        let want = if mm.age > KEEP_STEPS || !own_maps() { [None, None] } else { mm.drawn.get() };
        if self.ph.hi_size() != want[0] {
            self.ph.set_hi(want[0]);
            (mm.grade.vmax, mm.grade.vmax_cg) = (self.vmax, self.vmax_cg);
        }
        if self.phz.hi_size() != want[1] {
            self.phz.set_hi(want[1]);
            (mm.grade.vmaxz, mm.grade.vmaxz_cg) = (self.vmaxz, self.vmaxz_cg);
        }
        if want[0].is_none() {
            mm.acc_t = Default::default();
        }
    }

    /// This step's time on the monitor's maps.
    fn hi_spent(&self) -> Duration {
        self.ph.hi_time + self.phz.hi_time + self.hdmi_maps.fast_time
    }

    /// Before a record is drawn: the monitor's maps take it while the step's budget lasts (the first of a step
    /// always), else they are set aside until `hi_release`, and the record goes into the panel's maps alone.
    pub(super) fn hi_hold(&mut self) -> [Option<Box<Phosphor>>; 2] {
        if self.ph.hi.is_none() && self.phz.hi.is_none() {
            return [None, None];
        }
        if self.hdmi_maps.frames == 0 || self.hi_spent() < BUDGET {
            self.hdmi_maps.frames += 1;
            return [None, None];
        }
        [self.ph.hi.take(), self.phz.hi.take()]
    }

    pub(super) fn hi_release(&mut self, held: [Option<Box<Phosphor>>; 2]) {
        let [main, zoom] = held;
        if main.is_some() {
            self.ph.hi = main;
        }
        if zoom.is_some() {
            self.phz.hi = zoom;
        }
    }

    /// How many of the fast path's `n` frames (the newest) the monitor's maps take: what fits the rest of the budget
    /// at the last pace, one while the pace is not known.
    pub(super) fn hi_fast_take(&mut self, n: usize) -> usize {
        let left = BUDGET.saturating_sub(self.hi_spent()).as_secs_f32() * 1e3;
        let mm = &mut self.hdmi_maps;
        if mm.fast_ms <= 0.0 {
            return n.min(1);
        }
        let k = ((left / mm.fast_ms) as usize).min(n);
        if k == 0 {
            // a pace above the budget is tried again now and then: the time base or the channels may have changed
            mm.fast_ms *= 0.9;
        }
        k
    }

    /// The fast path drew `n` frames into the monitor's maps in `t`.
    pub(super) fn hi_fast_done(&mut self, n: usize, t: Duration) {
        let mm = &mut self.hdmi_maps;
        mm.fast_time += t;
        mm.frames += n as u32;
        if n > 0 {
            let ms = t.as_secs_f32() * 1e3 / n as f32;
            mm.fast_ms = if mm.fast_ms <= 0.0 { ms } else { mm.fast_ms + 0.2 * (ms - mm.fast_ms) };
        }
    }

    /// The fast path's time on the monitor's maps this step (left out of the time its own take follows).
    pub(super) fn hi_fast_time(&self) -> Duration {
        self.hdmi_maps.fast_time
    }

    /// The frames the monitor's maps took in the last step (`:SYSTem:STATe?`).
    pub fn monitor_frames(&self) -> u32 {
        self.hdmi_maps.frames
    }

    /// After the step's frames: the count that grades the monitor's picture, as `pic_frames` for the panel's.
    pub(super) fn hi_note_frames(&mut self, persistent: bool) {
        let mm = &mut self.hdmi_maps;
        if persistent {
            mm.pic_frames = 0;
        } else if mm.frames > 0 {
            mm.pic_frames = mm.frames;
        }
    }

    /// The maps a plot window's traces come from, with the counts they are graded against: in a monitor pass the
    /// monitor's own (noting the size of `area` for the next steps), else the panel's.
    pub(super) fn trace_maps(&self, zoom: bool, area: Rect) -> (&Phosphor, &[u32; NCH], u32) {
        if self.view_pass {
            let mut drawn = self.hdmi_maps.drawn.get();
            drawn[zoom as usize] = Some((area.w.max(0) as usize, area.h.max(0) as usize));
            self.hdmi_maps.drawn.set(drawn);
            let g = &self.hdmi_maps.grade;
            if let Some(hi) = if zoom { self.phz.hi.as_deref() } else { self.ph.hi.as_deref() } {
                return if zoom { (hi, &g.vmaxz, g.vmaxz_cg) } else { (hi, &g.vmax, g.vmax_cg) };
            }
        }
        if zoom {
            (&self.phz, &self.vmaxz, self.vmaxz_cg)
        } else {
            (&self.ph, &self.vmax, self.vmax_cg)
        }
    }

    /// The main and the zoom maps of the monitor's picture (the GPU's monitor frame): its own where it has them.
    pub fn monitor_maps(&self) -> (&Phosphor, &Phosphor) {
        (self.ph.hi.as_deref().unwrap_or(&self.ph), self.phz.hi.as_deref().unwrap_or(&self.phz))
    }

    /// `grade_refs` and the halving of saturated maps for the monitor's maps.
    pub(super) fn hi_grade_refs(&mut self) {
        let n = self.hdmi_maps.pic_frames;
        let on: [bool; NCH] = std::array::from_fn(|c| self.m.ch[c].on);
        let g = &mut self.hdmi_maps.grade;
        for (k, hi) in [self.ph.hi.as_deref_mut(), self.phz.hi.as_deref_mut()].into_iter().enumerate() {
            let Some(hi) = hi else { continue };
            let (vmax, per_frame) =
                if k == 0 { (&mut g.vmax, &mut g.per_frame) } else { (&mut g.vmaxz, &mut g.per_framez) };
            let mut full = false;
            for c in (0..NCH).filter(|&c| on[c]) {
                let mx = hi.map[c].iter().copied().max().unwrap_or(0) as u32;
                vmax[c] = picture_ref(vmax[c], &mut per_frame[c], mx, n);
                full |= mx > 0xc000;
            }
            let vmax_cg = if k == 0 { &mut g.vmax_cg } else { &mut g.vmaxz_cg };
            if self.m.color_grade {
                *vmax_cg = picture_ref(*vmax_cg, &mut g.per_frame_cg[k], hi.merged_max(on), n);
            }
            if full {
                hi.halve();
                vmax.iter_mut().for_each(|v| *v = (*v / 2).max(1));
                *vmax_cg = (*vmax_cg / 2).max(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MW: i32 = 1920;
    const MH: i32 = 1080;

    /// CH1 alone, stopped on a record of a square wave: a level at +1.5 div for the first half of the screen, an edge
    /// at the trigger point, a level at -1.5 div after it; the monitor in `mode`.
    fn square_ui(mode: HdmiMode) -> Ui {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        for c in 0..NCH {
            u.m.ch[c].on = c == 0;
        }
        u.m.hdmi = mode;
        u.set_hdmi_present(true);
        u.m.run = RunState::Stop;
        u.m.persist = 0;
        u.last[0] = Some(square(&u.m));
        u
    }

    fn square(m: &Model) -> Record {
        let (n, tdiv, vpd) = (4000usize, m.time_per_div() as f64, m.ch[0].volts_per_div());
        let dt = tdiv * HDIV as f64 / n as f64;
        let t0 = m.delay as f64 - tdiv * HDIV as f64 / 2.0;
        let level = |k: usize| if t0 + k as f64 * dt < 0.0 { 1.5 * vpd } else { -1.5 * vpd };
        let v: Vec<f32> = (0..n).map(|k| level(k) - m.ch[0].offset).collect();
        let ch = std::array::from_fn(|c| if c == 0 { v.clone() } else { Vec::new() });
        Record { t0, dt, ch, triggered: true, partial: false, raw: None, la: None, pairs: false }
    }

    /// Panel frames and monitor passes as the main loop runs them (a monitor frame with every second panel frame).
    fn run(u: &mut Ui, mode: HdmiMode, frames: usize) -> (Canvas, Canvas) {
        let (mut cv, mut hc) = (Canvas::new(PANEL_W, PANEL_H), Canvas::new(MW, MH));
        for k in 0..frames {
            u.step(1.0 / 30.0);
            u.render(&mut cv);
            if k % 2 == 1 {
                monitor(u, mode, &mut hc);
            }
        }
        (cv, hc)
    }

    fn monitor(u: &mut Ui, mode: HdmiMode, hc: &mut Canvas) {
        match mode {
            HdmiMode::Layout => u.render_view(hc),
            HdmiMode::Waveform => u.render_waveform(hc),
            HdmiMode::Mirror => {}
        }
    }

    /// The monitor's main plot window.
    fn monitor_area(u: &mut Ui, mode: HdmiMode) -> Rect {
        let at = |u: &mut Ui| {
            u.relayout();
            u.wave_area()
        };
        match mode {
            HdmiMode::Layout => {
                let keep = (u.scr, u.grid, std::mem::take(&mut u.wins), u.area, u.cur_win);
                (u.scr, u.view_pass) = (Screen { w: MW, h: MH }, true);
                let a = at(u);
                u.view_pass = false;
                (u.scr, u.grid, u.wins, u.area, u.cur_win) = keep;
                a
            }
            _ => u.with_waveform_pass(MW, MH, at),
        }
    }

    /// Lit pixels of the trace in column `x` (or row `y`) of `area` in the picture `a`: those that differ from the same
    /// picture without the trace, `b`.
    fn lit(a: &Canvas, b: &Canvas, area: Rect, x: Option<i32>, y: Option<i32>) -> usize {
        let pts: Vec<(i32, i32)> = match (x, y) {
            (Some(x), _) => (area.y + 2..area.bottom() - 2).map(|y| (x, y)).collect(),
            (_, Some(y)) => (area.x + 2..area.right() - 2).map(|x| (x, y)).collect(),
            _ => unreachable!(),
        };
        pts.iter().filter(|&&(x, y)| a.px[(y * a.w + x) as usize] != b.px[(y * b.w + x) as usize]).count()
    }

    // Break it catches (the owner's 1080p monitor, 2026-11-19: "aliased and broken as in 8 bit pixel"): the
    // monitor's traces stretched from the panel's maps, each trace pixel a block of two monitor pixels or more; the
    // monitor's maps at another size than its plot; the panel's picture changed by them.
    #[test]
    fn the_monitor_draws_a_trace_one_of_its_pixels_wide() {
        for mode in [HdmiMode::Waveform, HdmiMode::Layout] {
            let mut u = square_ui(mode);
            let (panel, pic) = run(&mut u, mode, 6);
            let area = monitor_area(&mut u, mode);
            let (aw, ah) = (area.w as usize, area.h as usize);
            assert!(
                aw > 1500 && ah > 2 * u.ph.h,
                "{:?}: the monitor's plot {:?}, the panel's map {}x{}",
                mode,
                area,
                u.ph.w,
                u.ph.h
            );
            assert_eq!(u.ph.hi_size(), Some((aw, ah)), "{:?}", mode);
            // the same frames without the trace: the record without samples
            let mut bare = square_ui(mode);
            bare.last[0].as_mut().unwrap().ch[0].clear();
            let (panel_bare, pic_bare) = run(&mut bare, mode, 6);
            assert!(panel.px != panel_bare.px && pic.px != pic_bare.px, "{:?}: a trace on both", mode);
            // the level before the edge: one monitor row in a column a quarter in; the edge: one column in a row
            // between the levels
            let (x, y) = (area.x + area.w / 4, area.y + area.h / 2 + 8);
            if let Ok(d) = std::env::var("HDMI_DUMP") {
                std::fs::write(format!("{}/{:?}.png", d, mode), crate::store::png_of(&pic)).unwrap();
                std::fs::write(format!("{}/{:?}_bare.png", d, mode), crate::store::png_of(&pic_bare)).unwrap();
            }
            assert_eq!(lit(&pic, &pic_bare, area, Some(x), None), 1, "{:?}: the level's thickness", mode);
            assert_eq!(lit(&pic, &pic_bare, area, None, Some(y)), 1, "{:?}: the edge's width", mode);
            // the fault: the panel's maps stretched make each trace pixel a block
            let hi = u.ph.hi.take();
            let mut stretched = Canvas::new(MW, MH);
            monitor(&mut u, mode, &mut stretched);
            if let Ok(d) = std::env::var("HDMI_DUMP") {
                std::fs::write(format!("{}/{:?}_stretched.png", d, mode), crate::store::png_of(&stretched)).unwrap();
            }
            assert!(lit(&stretched, &pic_bare, area, Some(x), None) >= 2, "{:?}: the test sees the stretch", mode);
            u.ph.hi = hi;
            // the panel's picture is the one it has without a monitor
            let mut alone = square_ui(HdmiMode::Mirror);
            alone.set_hdmi_present(false);
            let (cv, _) = run(&mut alone, HdmiMode::Mirror, 6);
            assert!(alone.ph.hi.is_none());
            assert!(cv.px == panel.px, "{:?}: the panel's picture changed", mode);
        }
    }

    // Break it catches: the panel's maps missing frames of the fast path or changed by the monitor's; the monitor's
    // maps taking more (or other) than the newest frames that fit their budget at the pace measured, or placing them
    // at the panel's size; a pace above the budget never tried again; dense and sparse frames, four channels; the
    // newest through `draw_frame` into both, its statistics once.
    #[test]
    fn the_fast_path_gives_the_monitor_s_maps_the_newest_frames_that_fit() {
        for &(tdiv, n, dt) in &[(1e-6, 12_500usize, 0.8e-9), (1e-7, 1_000, 3.2e-9)] {
            let pre = n / 2;
            let frame = |seq: u64| {
                let ch: [Vec<f32>; NCH] = std::array::from_fn(|c| {
                    let f = 2e6 * (c + 1) as f64;
                    (0..n)
                        .map(|k| {
                            (std::f64::consts::TAU * f * (k as f64 - pre as f64 + 0.1 * seq as f64) * dt).sin() as f32
                        })
                        .collect()
                });
                crate::board::Frame {
                    seq,
                    dt,
                    pre,
                    ch,
                    roll: false,
                    envelope: false,
                    scan: false,
                    partial: false,
                    acq: (1.0 / dt, n),
                    stamp: 0,
                    la: None,
                    fixed_ti: None,
                    hw_avg: false,
                    basic: Default::default(),
                }
            };
            let shared: crate::board::HwShared = Default::default();
            let mut ui = Ui::new(Source::Hw { shared, seen: 0 });
            ui.m.tscale = tscales().iter().position(|&x| (x as f64 - tdiv).abs() < tdiv * 1e-3).unwrap();
            ui.m.trig_level = 0.0;
            for c in 0..NCH {
                ui.m.ch[c].on = true;
                ui.m.ch[c].offset = 0.0;
            }
            ui.relayout();
            assert!(ui.fast_ok());
            let frames: Vec<_> = (1..=24).map(frame).collect();
            let (w, h) = (ui.ph.w, ui.ph.h);
            let (hw, hh) = (1830, 846);
            let m = ui.m.clone();
            let want = |w: usize, h: usize, k: usize| {
                let mut want = Phosphor::new(w, h);
                for f in &frames[frames.len() - k..] {
                    let f = std::sync::Arc::new(f.clone());
                    let p = fast_plan(&m, w, h, &f, [false; NCH]);
                    for c in 0..NCH {
                        fast_deposit(&m, w, h, c, &mut want.map[c], 0, &f, &p.plan);
                    }
                }
                want
            };
            let want_panel = want(w, h, 24);
            ui.ph.set_hi(Some((hw, hh)));
            // pace not known: the newest frame; 1 ms a frame: three in the 3 ms; 10 ms a frame: none, tried again
            for (pace, k) in [(0.0, 1), (1.0, 3), (10.0, 0)] {
                ui.ph.clear();
                (ui.hdmi_maps.frames, ui.hdmi_maps.fast_time, ui.hdmi_maps.fast_ms) = (0, Duration::ZERO, pace);
                assert_eq!(ui.fast_draw_all(frames.clone()), 24);
                assert_eq!(ui.hdmi_maps.frames, k, "{} s/div, {} ms a frame", tdiv, pace);
                let (hi, want_hi) = (ui.ph.hi.as_deref().unwrap(), want(hw, hh, k as usize));
                for c in 0..NCH {
                    assert!(ui.ph.map[c] == want_panel.map[c], "{} s/div CH{}: the panel's maps differ", tdiv, c + 1);
                    assert!(k == 0 || want_hi.map[c].iter().any(|&x| x > 0), "{} s/div CH{}: nothing", tdiv, c + 1);
                    assert!(hi.map[c] == want_hi.map[c], "{} s/div CH{}: the monitor's maps differ", tdiv, c + 1);
                }
            }
            assert!(ui.hdmi_maps.fast_ms == 9.0, "a pace above the budget is tried again");
            // the newest frame of a screen, `draw_frame`: into both, its statistics once
            let f = frame(25);
            let mut alone = Phosphor::new(hw, hh);
            crate::drawpipe::draw_frame(&mut alone, &ui.m, &f, None);
            let (mut edges, mut off) = (ui.fast_edges, ui.fast_off);
            crate::drawpipe::draw_frame(
                &mut Phosphor::new(w, h),
                &ui.m,
                &f,
                Some(crate::drawpipe::Stats { edges: &mut edges, off: &mut off }),
            );
            ui.ph.clear();
            assert!(ui.fast_draw(&f));
            assert_eq!(ui.fast_edges, edges, "{} s/div: the statistics counted once", tdiv);
            assert!((0..NCH).all(|c| ui.ph.hi.as_deref().unwrap().map[c] == alone.map[c]), "{} s/div", tdiv);
        }
    }

    /// What the monitor's maps cost (not a check: `cargo test --release -- --ignored cost_of_the_monitor_s_maps
    /// --nocapture`): per frame, the fast path's drawing (`draw_frame`) into the panel's map alone and with a
    /// 1830 x 846 monitor map; a screen of the fast path (32 frames) for the panel's maps alone and with the monitor's
    /// in their budget; per screen, what the maps cost whatever was drawn (clear, the grading's maximum).
    #[test]
    #[ignore]
    fn cost_of_the_monitor_s_maps() {
        let ms = |t: std::time::Instant, n: usize| t.elapsed().as_secs_f64() * 1e3 / n as f64;
        let (w, h, hw, hh) = (1004, 440, 1830, 846);
        for &(tdiv, n, dt) in
            &[(1e-6, 12_500usize, 0.8e-9), (1e-5, 125_000, 0.8e-9), (1e-7, 1_000, 3.2e-9), (1e-8, 125, 1.6e-9)]
        {
            let pre = n / 2;
            let frames: Vec<crate::board::Frame> = (0..32u64)
                .map(|seq| {
                    let ch: [Vec<f32>; NCH] = std::array::from_fn(|c| {
                        let f = 2e6 * (c + 1) as f64 * 1e-6 / tdiv;
                        (0..n)
                            .map(|k| {
                                (std::f64::consts::TAU * f * (k as f64 - pre as f64 + 0.1 * seq as f64) * dt).sin()
                                    as f32
                            })
                            .collect()
                    });
                    crate::board::Frame {
                        seq,
                        dt,
                        pre,
                        ch,
                        roll: false,
                        envelope: false,
                        scan: false,
                        partial: false,
                        acq: (1.0 / dt, n),
                        stamp: 0,
                        la: None,
                        fixed_ti: None,
                        hw_avg: false,
                        basic: Default::default(),
                    }
                })
                .collect();
            let tscale = tscales().iter().position(|&x| (x as f64 - tdiv).abs() < tdiv * 1e-3).unwrap();
            let mut m = Model { tscale, trig_level: 0.0, ..Default::default() };
            for c in 0..NCH {
                m.ch[c].on = true;
                m.ch[c].offset = 0.0;
            }
            let mut panel = Phosphor::new(w, h);
            let mut both = Phosphor::new(w, h);
            both.set_hi(Some((hw, hh)));
            for ph in [&mut panel, &mut both] {
                frames.iter().for_each(|f| _ = crate::drawpipe::draw_frame(ph, &m, f, None));
            }
            let t = std::time::Instant::now();
            frames.iter().for_each(|f| _ = crate::drawpipe::draw_frame(&mut panel, &m, f, None));
            let a = ms(t, frames.len());
            let t = std::time::Instant::now();
            frames.iter().for_each(|f| _ = crate::drawpipe::draw_frame(&mut both, &m, f, None));
            let b = ms(t, frames.len());
            println!(
                "{:e} s/div, {} points, 4 channels: draw_frame {:.3} ms panel, {:.3} ms with the monitor ({:.2}x)",
                tdiv,
                n,
                a,
                b,
                b / a
            );
            let shared: crate::board::HwShared = Default::default();
            let mut ui = Ui::new(Source::Hw { shared, seen: 0 });
            ui.m = m.clone();
            ui.relayout();
            let screen = |ui: &mut Ui| {
                (ui.hdmi_maps.frames, ui.hdmi_maps.fast_time, ui.ph.hi_time) = (0, Duration::ZERO, Duration::ZERO);
                let t = std::time::Instant::now();
                ui.fast_draw_all(frames.clone());
                (t.elapsed().as_secs_f64() * 1e3, ui.hdmi_maps.frames)
            };
            let k = 10;
            screen(&mut ui);
            let a = (0..k).map(|_| screen(&mut ui).0).sum::<f64>() / k as f64;
            ui.ph.set_hi(Some((hw, hh)));
            screen(&mut ui);
            let both: Vec<(f64, u32)> = (0..k).map(|_| screen(&mut ui)).collect();
            let b = both.iter().map(|x| x.0).sum::<f64>() / k as f64;
            let took = both.iter().map(|x| x.1).sum::<u32>() as f64 / k as f64;
            println!(
                "  fast path, a screen of {} frames: {:.2} ms panel, {:.2} ms with the monitor's maps ({:.1} frames)",
                frames.len(),
                a,
                b,
                took
            );
        }
        let mut ph = Phosphor::new(w, h);
        ph.set_hi(Some((hw, hh)));
        let mut hi = ph.hi.take().unwrap();
        let k = 20;
        for (what, p) in [("panel", &mut ph), ("monitor", &mut *hi)] {
            let t = std::time::Instant::now();
            (0..k).for_each(|_| p.clear());
            let clear = ms(t, k);
            let t = std::time::Instant::now();
            (0..k).for_each(|_| {
                _ = std::hint::black_box((0..NCH).map(|c| p.map[c].iter().copied().max().unwrap_or(0)).max())
            });
            let max = ms(t, k);
            println!("{} maps {}x{}, 4 channels: clear {:.3} ms, maximum {:.3} ms", what, p.w, p.h, clear, max);
        }
    }

    // Break it catches: records drawn into the monitor's maps after the step's budget is spent, or not the first of a
    // step; the maps lost or swapped when set aside; the monitor's picture graded by the panel's count of frames.
    #[test]
    fn records_go_into_the_monitor_s_maps_while_the_budget_lasts() {
        let mut u = square_ui(HdmiMode::Waveform);
        run(&mut u, HdmiMode::Waveform, 4);
        let rec = square(&u.m);
        let draw = |u: &mut Ui| {
            let held = u.hi_hold();
            let view = (u.m.time_per_div(), u.m.delay);
            u.ph.draw_ex(0, &u.m, view, rec.t0, rec.dt, &rec.ch[0], 24, None, false);
            u.hi_release(held);
        };
        let sum = |p: &Phosphor| p.map[0].iter().map(|&x| x as u64).sum::<u64>();
        u.hi_maps_sync();
        let (hi0, hz) = (sum(u.ph.hi.as_deref().unwrap()), u.phz.hi_size());
        // the first record of a step goes in whatever the time spent
        u.ph.hi_time = BUDGET * 2;
        draw(&mut u);
        let hi1 = sum(u.ph.hi.as_deref().unwrap());
        assert!(hi1 > hi0);
        // the budget spent: the panel's maps alone
        let p1 = sum(&u.ph);
        draw(&mut u);
        assert!(sum(&u.ph) > p1);
        assert_eq!(sum(u.ph.hi.as_deref().unwrap()), hi1);
        assert_eq!(u.phz.hi_size(), hz, "the zoom's maps put back");
        // within the budget again: both
        u.ph.hi_time = Duration::ZERO;
        draw(&mut u);
        assert!(sum(u.ph.hi.as_deref().unwrap()) > hi1);
        // two records went into the monitor's maps: they grade its picture, three the panel's
        u.hi_note_frames(false);
        assert_eq!(u.hdmi_maps.pic_frames, 2);
        u.hi_note_frames(true);
        assert_eq!(u.hdmi_maps.pic_frames, 0);
    }

    // Break it catches: the monitor's maps kept (and drawn into, at their cost) without monitor passes, in Mirror or
    // after the monitor went; not there again when it comes back.
    #[test]
    fn the_monitor_s_maps_go_with_the_monitor() {
        let mut u = square_ui(HdmiMode::Waveform);
        run(&mut u, HdmiMode::Waveform, 4);
        assert!(u.ph.hi.is_some());
        u.m.hdmi = HdmiMode::Mirror;
        run(&mut u, HdmiMode::Mirror, KEEP_STEPS as usize + 2);
        assert!(u.ph.hi.is_none() && u.phz.hi.is_none());
        assert!(u.hdmi_maps.acc_t.iter().all(|a| a.is_empty()));
        u.m.hdmi = HdmiMode::Layout;
        run(&mut u, HdmiMode::Layout, 4);
        assert!(u.ph.hi.is_some());
    }
}
