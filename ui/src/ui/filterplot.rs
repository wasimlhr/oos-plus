// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the Filter pane, a window of its own like the Spectrum: the response of the designed filter of the selected
//! math channel as four plots, its gain and phase against the frequency and its impulse and step response against the
//! time. The filter is designed for a sample step of its own, 64 samples to the time constant of its cut-off, so the
//! picture is that of the filter and not of the record it is applied to; the numbers are `oos_dsp::filter::Filter`'s.

use super::*;
use oos_dsp::filter::{Filter, Response, Spec};

/// The samples of an impulse or step response.
const POINTS: usize = 512;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Plot {
    Gain,
    Phase,
    Impulse,
    Step,
}

impl Plot {
    const ALL: [Plot; 4] = [Plot::Gain, Plot::Phase, Plot::Impulse, Plot::Step];

    fn name(self) -> &'static str {
        match self {
            Plot::Gain => "Gain",
            Plot::Phase => "Phase",
            Plot::Impulse => "Impulse response",
            Plot::Step => "Step response",
        }
    }
}

impl Ui {
    /// The design the pane shows: None unless the selected math channel is a designed filter.
    pub(super) fn filter_pane_spec(&self) -> Option<Spec> {
        let filter = matches!(self.m.math, MathOp::Lpf | MathOp::Hpf | MathOp::Bpf | MathOp::Bsf);
        if !filter {
            return None;
        }
        crate::math::filter_spec(self.m.math, self.m.math_fc as f64, self.m.math_bw as f64, &self.m.math_filt)
    }

    /// The taps of a Custom filter: the reference chosen as Source B, and the step they are sampled at.
    fn filter_pane_taps(&self) -> Option<(Vec<f32>, f64)> {
        let c = self.m.math_ch(self.m.math_sel);
        if !c.uses_taps() || c.b < crate::math::MATH_REF_BASE {
            return None;
        }
        let w = self.refs.get(c.b - crate::math::MATH_REF_BASE)?.as_ref()?;
        (w.v.len() >= 2 && w.dt > 0.0).then(|| (w.v.to_vec(), w.dt))
    }

    /// Whether the pane has a filter to draw (a design, or the taps of a file).
    pub(super) fn filter_pane_ready(&self) -> bool {
        self.filter_pane_spec().is_some() || self.filter_pane_taps().is_some()
    }

    /// The Filter pane in `r`: the four plots side by side, or two by two when the pane is narrow, with a hint while
    /// the selected math channel is no designed filter.
    pub(super) fn draw_pane_filter(&self, cv: &mut Canvas, r: Rect) {
        let f = self.fonts();
        let (filter, f_ref, dt) = if let Some(spec) = self.filter_pane_spec() {
            // the frequency the picture is built round: the cut-off, the centre of a band, the symbol rate, the delay
            let f_ref = match spec.response {
                Response::RaisedCosine | Response::RootRaisedCosine => 1.0 / spec.symbol.max(1e-12),
                Response::Allpass => 0.5 / spec.delay.max(1e-12),
                _ if spec.f2 > spec.f1 && matches!(self.m.math, MathOp::Bpf | MathOp::Bsf) => (spec.f1 * spec.f2).sqrt(),
                _ => spec.f1,
            };
            if !f_ref.is_finite() || f_ref <= 0.0 {
                return;
            }
            let dt = 1.0 / (64.0 * f_ref);
            (Filter::design(&spec, dt), f_ref, dt)
        } else if let Some((taps, dt)) = self.filter_pane_taps() {
            // a filter file at its own step: the picture spans a thousandth of its rate to 0.4 of it
            match Filter::from_taps(&taps, dt, dt) {
                Some(filter) => (filter, 0.04 / dt, dt),
                None => return,
            }
        } else {
            let hint = "Filter: choose a low-, high-, band-pass or band-stop with a response, or a Custom file with its taps in Source B";
            cv.text(&f.s, r.x + 20, r.y + 24, hint, DIM);
            return;
        };
        let wide = r.w >= 880 || r.h < 260;
        let (cols, rows) = if wide { (4, 1) } else { (2, 2) };
        let (tw, th) = (r.w / cols, r.h / rows);
        for (k, plot) in Plot::ALL.into_iter().enumerate() {
            let (c, row) = (k as i32 % cols, k as i32 / cols);
            self.draw_filter_plot(cv, Rect::new(r.x + c * tw, r.y + row * th, tw, th), plot, &filter, f_ref, dt);
        }
    }

    /// One plot in `r`: a title, the axes with their labels and the curve.
    fn draw_filter_plot(&self, cv: &mut Canvas, r: Rect, plot: Plot, filter: &Filter, f_ref: f64, dt: f64) {
        if r.w < 120 || r.h < 70 {
            return;
        }
        let f = self.fonts();
        let g = Rect::new(r.x + 46, r.y + 24, r.w - 58, r.h - 44);
        cv.text(&f.s, r.x + 10, r.y + 17, plot.name(), TEXT);
        cv.rect(g, LINE);
        match plot {
            Plot::Gain | Plot::Phase => {
                let (lo, hi) = (f_ref / 100.0, (f_ref * 10.0).min(0.45 / dt));
                let pts = filter.response(lo, hi, 160, true);
                let (ln0, ln1) = (lo.ln(), hi.ln());
                let x_of = |hz: f64| g.x + ((hz.ln() - ln0) / (ln1 - ln0) * g.w as f64).round() as i32;
                let (v0, v1, unit) = if plot == Plot::Gain { (-60.0, 6.0, "dB") } else { (-180.0, 180.0, "\u{b0}") };
                let y_of = |v: f64| g.bottom() - ((v.clamp(v0, v1) - v0) / (v1 - v0) * g.h as f64).round() as i32;
                let mut d = 10f64.powf(lo.log10().ceil());
                while d <= hi {
                    let x = x_of(d);
                    cv.vline(x, g.y, g.bottom(), LINE);
                    let label = si_short(d, "Hz");
                    if x + 2 + f.s.width(&label) <= r.right() - 4 {
                        cv.text(&f.s, x + 2, r.bottom() - 6, &label, DIM);
                    }
                    d *= 10.0;
                }
                let ticks: &[f64] =
                    if plot == Plot::Gain { &[0.0, -20.0, -40.0, -60.0] } else { &[-180.0, -90.0, 0.0, 90.0, 180.0] };
                for &v in ticks {
                    cv.hline(g.x, g.right(), y_of(v), LINE);
                    cv.text(&f.s, r.x + 6, y_of(v) + 4, &format!("{v:.0}"), DIM);
                }
                cv.text(&f.s, r.right() - f.s.width(unit) - 10, r.y + 17, unit, DIM);
                let mut prev: Option<(i32, i32, f64)> = None;
                for &(hz, gain, phase) in &pts {
                    let v = if plot == Plot::Gain { gain } else { phase };
                    let (x, y) = (x_of(hz), y_of(v));
                    // a phase that wraps is not joined across the jump
                    if let Some((x0, y0, pv)) = prev {
                        if (v - pv).abs() < 180.0 {
                            cv.line(x0, y0, x, y, ACCENT);
                        }
                    }
                    prev = Some((x, y, v));
                }
            }
            Plot::Impulse | Plot::Step => {
                let points = filter.taps().max(POINTS).min(4096);
                let v = if plot == Plot::Impulse { filter.impulse(points) } else { filter.step(points) };
                let (mn, mx) = v.iter().fold((0f32, 0f32), |(a, b), &x| (a.min(x), b.max(x)));
                let (mn, mx) = if plot == Plot::Step { (mn.min(0.0), mx.max(1.0)) } else { (mn, mx) };
                let span = (mx - mn).max(1e-9);
                let (lo, hi) = (mn - span * 0.08, mx + span * 0.08);
                let y_of = |x: f32| g.bottom() - ((x - lo) / (hi - lo) * g.h as f32).round() as i32;
                let x_of = |i: usize| g.x + (i as f64 / (points - 1) as f64 * g.w as f64).round() as i32;
                cv.hline(g.x, g.right(), y_of(0.0), LINE);
                if plot == Plot::Step {
                    cv.hline(g.x, g.right(), y_of(1.0), LINE);
                }
                cv.text(&f.s, r.x + 6, y_of(mx) + 4, &format!("{mx:.2}"), DIM);
                cv.text(&f.s, r.x + 6, y_of(mn) + 4, &format!("{mn:.2}"), DIM);
                for i in 1..points {
                    cv.line(x_of(i - 1), y_of(v[i - 1]), x_of(i), y_of(v[i]), ACCENT);
                }
                let span_t = si_short(points as f64 * dt, "s");
                cv.text(&f.s, g.x, r.bottom() - 6, "0", DIM);
                cv.text(&f.s, g.right() - f.s.width(&span_t), r.bottom() - 6, &span_t, DIM);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::math::{FiltResp, FILT_RESPS};
    use crate::ui::addmenu::AddRow;

    fn pane_picture(u: &mut Ui) -> Vec<u8> {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        u.step(0.05);
        cv = Canvas::new(W, H);
        u.render(&mut cv);
        crate::store::png_of(&cv)
    }

    fn keep(png: &[u8], name: &str) {
        if let Some(dir) = std::env::var_os("OOS_SHOTS") {
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(std::path::Path::new(&dir).join(name), png).unwrap();
        }
    }

    fn studio_with_filter() -> Ui {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.studio.enter = true;
        for _ in 0..10 {
            u.step(0.05);
        }
        u.math_add_filter();
        u
    }

    /// Add filter opens the Filter pane; every response draws in it and changes the picture; with no filter the pane
    /// says what to do and the menu of an empty channel offers Add filter. `OOS_SHOTS=dir` keeps the pictures.
    #[test]
    fn the_filter_pane_shows_every_response() {
        let mut u = studio_with_filter();
        assert_eq!((u.m.math, u.m.pane), (MathOp::Lpf, tables::PANE_FILTER));
        let mut seen = Vec::new();
        for resp in FILT_RESPS.into_iter().filter(|&r| !matches!(r, FiltResp::Simple | FiltResp::Custom)) {
            u.m.math_filt.resp = resp;
            let png = pane_picture(&mut u);
            assert!(!seen.contains(&png), "{resp:?} looks like an earlier response");
            keep(&png, &format!("filterpane-{resp:?}.png").to_lowercase());
            seen.push(png);
        }
        // not a filter: the hint
        u.m.math = MathOp::Off;
        assert!(u.filter_pane_spec().is_none());
        keep(&pane_picture(&mut u), "filterpane-hint.png");
        // the plain response has no design to draw
        u.m.math = MathOp::Lpf;
        u.m.math_filt.resp = FiltResp::Simple;
        assert!(u.filter_pane_spec().is_none());
    }

    /// The empty channel's sheet offers Add filter; one tap makes the filter and shows its pane.
    #[test]
    fn add_filter_makes_a_filter_and_opens_its_pane() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.studio.enter = true;
        for _ in 0..10 {
            u.step(0.05);
        }
        u.open_menu(MenuId::Math);
        assert_eq!(u.m.math, MathOp::Off);
        assert!(u.items(MenuId::Math).contains(&Item::MathAddFilter));
        u.activate(Item::MathAddFilter);
        assert_eq!((u.m.math, u.m.pane), (MathOp::Lpf, tables::PANE_FILTER));
        assert!(u.m.math_shown(u.m.math_sel));
        assert!(!u.items(MenuId::Math).contains(&Item::MathAddFilter));
    }

    /// Add > Filter with no filter on makes one on a free math channel, and a second one lands on the next free one
    /// while the first stays as it was.
    #[test]
    fn add_filter_takes_a_free_math_channel() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.studio.enter = true;
        for _ in 0..10 {
            u.step(0.05);
        }
        u.math_set_shown(0, true);
        assert_eq!(u.m.math_ch(0).op, MathOp::Add);
        u.studio.add_menu = true;
        let (_, r) = u.studio_add_rows().into_iter().find(|(row, _)| *row == AddRow::Filter).expect("the row");
        assert!(u.studio_add_tap(r.x + r.w / 2, r.y + r.h / 2));
        assert_eq!(u.m.math_sel, 1, "M1 is in use, so the filter is M2");
        assert_eq!((u.m.math, u.m.pane), (MathOp::Lpf, tables::PANE_FILTER));
        assert_eq!(u.m.math_ch(0).op, MathOp::Add, "M1 is as it was");
    }

    /// The expand arrow of the Spectrum and Filter windows takes the window over the area; the way back restores it.
    #[test]
    fn the_spectrum_and_filter_windows_can_be_maximised() {
        use crate::ui::dock::VKind;
        for pane in [tables::PANE_SPECTRUM, tables::PANE_FILTER] {
            let mut u = studio_with_filter();
            u.select_option(Item::PaneSel, tables::PANES[pane]);
            for _ in 0..4 {
                u.step(0.05);
            }
            assert!(u.dock_shown(VKind::Pane), "pane {pane} shown");
            let before = u.dock_rect(VKind::Pane).unwrap();
            u.dock_maximise(VKind::Pane);
            for _ in 0..4 {
                u.step(0.05);
            }
            let after = u.dock_rect(VKind::Pane).unwrap();
            assert!(after.w * after.h > before.w * before.h, "pane {pane}: {before:?} -> {after:?}");
        }
    }

    /// The Spectrum and Filter windows have a settings button left of the expand button, which opens the menu that
    /// sets them; other windows have none.
    #[test]
    fn the_spectrum_and_filter_windows_have_a_settings_button() {
        use crate::ui::dock::VKind;
        let mut u = studio_with_filter();
        for _ in 0..4 {
            u.step(0.05);
        }
        let r = u.dock_rect(VKind::Pane).unwrap();
        let gear = u.dock_gear_rect(VKind::Pane, r).expect("the Filter window has one");
        let extra = u.dock_extra_rect(VKind::Pane, r).expect("the expand button");
        assert!(gear.right() <= extra.x + 1 && gear.x < extra.x, "{gear:?} left of {extra:?}");
        u.dock_gear_do(VKind::Pane);
        assert_eq!(u.menu, MenuId::Math, "the math channel of the filter");
        u.close_menu();
        u.select_option(Item::PaneSel, "Spectrum");
        for _ in 0..4 {
            u.step(0.05);
        }
        let r = u.dock_rect(VKind::Pane).unwrap();
        assert!(u.dock_gear_rect(VKind::Pane, r).is_some());
        u.dock_gear_do(VKind::Pane);
        assert_eq!(u.menu, MenuId::Spectrum);
        u.close_menu();
        u.dock_gear_do(VKind::Hist);
        assert_eq!(u.menu, MenuId::Display, "the histogram is a row of Display");
        u.close_menu();
        u.rail_plot_toggle(2);
        for _ in 0..6 {
            u.step(0.05);
        }
        let r = u.dock_rect(VKind::Hist).expect("the Histogram window");
        assert!(u.dock_gear_rect(VKind::Hist, r).is_some(), "the Histogram window has a settings button");
        keep(&pane_picture(&mut u), "histogram-gear.png");
        u.close_menu();
        u.select_option(Item::PaneSel, "Trend");
        for _ in 0..4 {
            u.step(0.05);
        }
        let r = u.dock_rect(VKind::Pane).unwrap();
        assert!(u.dock_gear_rect(VKind::Pane, r).is_none(), "no settings button on the Trend window");
    }

    /// Save to reference puts the result of a math channel into a reference slot, where it can be measured.
    #[test]
    fn a_math_result_is_saved_to_a_reference() {
        let mut u = studio_with_filter();
        for _ in 0..40 {
            u.step(0.05);
            if u.math_record(u.m.math_sel).ok().flatten().is_some() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        assert!(u.items(MenuId::Math).contains(&Item::MathToRef));
        u.math_to_ref();
        let w = u.refs[0].as_ref().expect("saved into R1");
        assert_eq!(w.label, "M1");
        assert!(w.v.len() > 100 && w.dt > 0.0);
    }

    /// A Custom filter takes its taps from the reference of Source B: four taps of a quarter are a moving average over
    /// four samples at the step of the record, and the Filter window draws them.
    #[test]
    fn a_custom_filter_uses_the_taps_of_a_reference() {
        let mut u = studio_with_filter();
        let (dt, n) = (1e-6, 2000usize);
        let taps = RefWave {
            src: 0,
            color: 0,
            label: "taps".into(),
            t0: 0.0,
            dt,
            v: std::sync::Arc::new(vec![0.25; 4]),
            vpd: 1.0,
            offset: 0.0,
            on: true,
        };
        u.ref_put(0, taps);
        u.m.math_filt.resp = FiltResp::Custom;
        u.m.math_b = crate::math::MATH_REF_BASE;
        let k = u.m.math_sel;
        u.m.maths[k] = u.m.math_ch(k);
        assert!(u.m.math_ch(k).uses_taps() && u.filter_pane_ready());
        assert!(u.items(MenuId::Math).contains(&Item::MathB), "the taps are chosen as Source B");
        // the Filter window shows the four plots of the taps
        let png = pane_picture(&mut u);
        keep(&png, "filterpane-custom.png");
        // the same filter by the job: the moving average of a ramp
        let ramp: Vec<f32> = (0..n).map(|i| i as f32).collect();
        let job_taps = oos_dsp::filter::Filter::from_taps(&[0.25f32; 4], dt, dt).unwrap();
        let out = job_taps.apply(&ramp);
        assert!((out[1000] - 1000.0).abs() < 1.0, "a ramp through a symmetric average stays: {}", out[1000]);
    }

    /// Auto scale with no result yet waits for it and fits then, instead of fitting to nothing: one press is enough.
    #[test]
    fn auto_scale_waits_for_the_result_of_the_math_channel() {
        let mut u = studio_with_filter();
        let k = u.m.math_sel;
        for _ in 0..60 {
            u.step(0.05);
            std::thread::sleep(std::time::Duration::from_millis(20));
            if !u.math.state.borrow().fit_pending[k] && u.math_record(k).ok().flatten().is_some() {
                break;
            }
        }
        let w = u.math_wave_of(k);
        let (mn, mx) = w.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        let scale = crate::math::math_scales()[u.m.math_ch(k).scale];
        assert!(mx > mn && scale >= ((mx - mn) as f64 / 6.0) * 0.99 && scale <= ((mx - mn) as f64 / 6.0) * 2.6, "{scale} for {mn}..{mx}");
    }
}

