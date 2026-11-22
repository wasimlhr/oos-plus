// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the response of the designed filter of the selected math channel as a plot over the waveform: its gain
//! and phase against the frequency, its impulse and step response against the time (`Math > Plot`). The filter is
//! designed for a sample step of its own, 64 samples to the time constant of its cut-off, so the picture is that of
//! the filter and not of the record it is applied to; the numbers are `oos_dsp::filter::Filter`'s.

use super::*;
use oos_dsp::filter::{Filter, Response};

/// The samples of an impulse or step response.
const POINTS: usize = 512;

impl Ui {
    /// The design the plot shows: None while the plot is off, the waveform is not on, or the selected math channel is
    /// not a designed filter.
    fn filter_plot_spec(&self) -> Option<oos_dsp::filter::Spec> {
        let filter = matches!(self.m.math, MathOp::Lpf | MathOp::Hpf | MathOp::Bpf | MathOp::Bsf);
        if self.math.plot == FiltPlot::Off || self.view_on() || !filter {
            return None;
        }
        crate::math::filter_spec(self.m.math, self.m.math_fc as f64, self.m.math_bw as f64, &self.m.math_filt)
    }

    /// The panel of the response plot, bottom right of the waveform: nothing while the plot is off, the waveform is
    /// not on, or the selected math channel is not a designed filter.
    pub(super) fn draw_filter_plot(&self, cv: &mut Canvas) {
        let kind = self.math.plot;
        let Some(spec) = self.filter_plot_spec() else {
            return;
        };
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
        let filter = Filter::design(&spec, dt);
        let a = self.wave_area();
        let w = (a.w * 2 / 5).clamp(260, 520).min(a.w - 16);
        let h = (a.h * 2 / 5).clamp(150, 300).min(a.h - 16);
        if w < 160 || h < 100 {
            return;
        }
        let r = Rect::new(a.x + a.w - w - 8, a.y + a.h - h - 8, w, h);
        let f = self.fonts();
        cv.round(r, 6, PANEL_BG, 235);
        let g = Rect::new(r.x + 52, r.y + 26, r.w - 64, r.h - 46);
        cv.text(&f.s, r.x + 10, r.y + 17, &format!("Filter {}", kind.name().to_lowercase()), TEXT);
        cv.rect(g, LINE);
        match kind {
            FiltPlot::Gain | FiltPlot::Phase => {
                let (lo, hi) = (f_ref / 100.0, (f_ref * 10.0).min(0.45 / dt));
                let pts = filter.response(lo, hi, 160, true);
                let (ln0, ln1) = (lo.ln(), hi.ln());
                let x_of = |hz: f64| g.x + ((hz.ln() - ln0) / (ln1 - ln0) * g.w as f64).round() as i32;
                let (v0, v1, unit) =
                    if kind == FiltPlot::Gain { (-60.0, 6.0, "dB") } else { (-180.0, 180.0, "\u{b0}") };
                let y_of = |v: f64| g.bottom() - ((v.clamp(v0, v1) - v0) / (v1 - v0) * g.h as f64).round() as i32;
                let mut d = 10f64.powf(lo.log10().ceil());
                while d <= hi {
                    let x = x_of(d);
                    cv.vline(x, g.y, g.bottom(), LINE);
                    cv.text(&f.s, x + 2, r.bottom() - 6, &si_short(d, "Hz"), DIM);
                    d *= 10.0;
                }
                let ticks: &[f64] = if kind == FiltPlot::Gain {
                    &[0.0, -20.0, -40.0, -60.0]
                } else {
                    &[-180.0, -90.0, 0.0, 90.0, 180.0]
                };
                for &v in ticks {
                    cv.hline(g.x, g.right(), y_of(v), LINE);
                    cv.text(&f.s, r.x + 6, y_of(v) + 4, &format!("{v:.0}"), DIM);
                }
                cv.text(&f.s, r.right() - f.s.width(unit) - 10, r.y + 17, unit, DIM);
                let mut prev: Option<(i32, i32, f64)> = None;
                for &(hz, gain, phase) in &pts {
                    let v = if kind == FiltPlot::Gain { gain } else { phase };
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
            FiltPlot::Impulse | FiltPlot::Step => {
                let v = if kind == FiltPlot::Impulse { filter.impulse(POINTS) } else { filter.step(POINTS) };
                let (mn, mx) = v.iter().fold((0f32, 0f32), |(a, b), &x| (a.min(x), b.max(x)));
                let (mn, mx) = if kind == FiltPlot::Step { (mn.min(0.0), mx.max(1.0)) } else { (mn, mx) };
                let span = (mx - mn).max(1e-9);
                let (lo, hi) = (mn - span * 0.08, mx + span * 0.08);
                let y_of = |x: f32| g.bottom() - ((x - lo) / (hi - lo) * g.h as f32).round() as i32;
                let x_of = |i: usize| g.x + (i as f64 / (POINTS - 1) as f64 * g.w as f64).round() as i32;
                cv.hline(g.x, g.right(), y_of(0.0), LINE);
                if kind == FiltPlot::Step {
                    cv.hline(g.x, g.right(), y_of(1.0), LINE);
                }
                cv.text(&f.s, r.x + 6, y_of(mx) + 4, &format!("{mx:.2}"), DIM);
                cv.text(&f.s, r.x + 6, y_of(mn) + 4, &format!("{mn:.2}"), DIM);
                for i in 1..POINTS {
                    cv.line(x_of(i - 1), y_of(v[i - 1]), x_of(i), y_of(v[i]), ACCENT);
                }
                let span_t = si_short(POINTS as f64 * dt, "s");
                cv.text(&f.s, g.x, r.bottom() - 6, "0", DIM);
                cv.text(&f.s, g.right() - f.s.width(&span_t), r.bottom() - 6, &span_t, DIM);
            }
            FiltPlot::Off => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(u: &mut Ui) -> Vec<u8> {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        u.step(0.05);
        cv = Canvas::new(W, H);
        u.render(&mut cv);
        crate::store::png_of(&cv)
    }

    /// Every response and every view of the plot draws and changes the picture against the plot off; the plain filter has
    /// no Plot row. `OOS_SHOTS=dir` keeps the pictures.
    #[test]
    fn every_response_and_view_draws_a_plot() {
        use crate::math::{FiltResp, FILT_RESPS};
        let mut u = crate::ui::testkit::demo_ui();
        u.m.math = MathOp::Lpf;
        u.m.math_fc = 1e6;
        u.m.math_filt.resp = FiltResp::Butterworth;
        u.math.plot = FiltPlot::Off;
        let off = picture(&mut u);
        for resp in FILT_RESPS.into_iter().filter(|&r| r != FiltResp::Simple) {
            u.m.math_filt.resp = resp;
            for kind in [FiltPlot::Gain, FiltPlot::Phase, FiltPlot::Impulse, FiltPlot::Step] {
                u.math.plot = kind;
                let on = picture(&mut u);
                assert_ne!(on, off, "{resp:?} {kind:?}");
                if let Some(dir) = std::env::var_os("OOS_SHOTS") {
                    let _ = std::fs::create_dir_all(&dir);
                    let name = format!("filter-{resp:?}-{}.png", kind.name()).to_lowercase();
                    std::fs::write(std::path::Path::new(&dir).join(name), &on).unwrap();
                }
            }
        }
        // the plain filter has no design to plot, and its menu has no Plot row
        u.m.math_filt.resp = FiltResp::Simple;
        assert!(crate::math::filter_spec(MathOp::Lpf, 1e6, 0.0, &u.m.math_filt).is_none());
        assert!(!u.items(MenuId::Math).contains(&Item::MathFiltPlot));
        u.m.math_filt.resp = FiltResp::Bessel;
        assert!(u.items(MenuId::Math).contains(&Item::MathFiltPlot));
        // a math channel that is not a filter draws no plot, whatever the response and the plot say
        u.math.plot = FiltPlot::Gain;
        assert!(u.filter_plot_spec().is_some());
        u.m.math = MathOp::Off;
        assert!(u.filter_plot_spec().is_none());
    }

    /// Add filter on an empty math channel makes a low-pass of the active channel with its gain plot, and the sheet of
    /// the empty channel offers it. `OOS_SHOTS=dir` keeps the pictures of Studio before and after.
    #[test]
    fn add_filter_makes_a_filter_with_its_plot() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.studio.enter = true;
        for _ in 0..10 {
            u.step(0.05);
        }
        u.open_menu(MenuId::Math);
        assert_eq!(u.m.math, MathOp::Off);
        assert!(u.items(MenuId::Math).contains(&Item::MathAddFilter));
        let shot = |u: &mut Ui, name: &str| {
            let png = picture(u);
            if let Some(dir) = std::env::var_os("OOS_SHOTS") {
                let _ = std::fs::create_dir_all(&dir);
                std::fs::write(std::path::Path::new(&dir).join(name), png).unwrap();
            }
        };
        shot(&mut u, "addfilter-before.png");
        u.activate(Item::MathAddFilter);
        assert_eq!((u.m.math, u.math.plot), (MathOp::Lpf, FiltPlot::Gain));
        assert!(u.m.math_shown(u.m.math_sel));
        assert!(!u.items(MenuId::Math).contains(&Item::MathAddFilter));
        shot(&mut u, "addfilter-after.png");
    }
}
