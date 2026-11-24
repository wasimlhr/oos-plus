// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the live channels on the monitor of the HDMI Layout as lines. The panel's traces are density maps of its
//! own size (persistence, colour grade); on a monitor of 1920 x 1080 they are stretched, and their edges are blocks.
//! The lines are drawn from the record at the monitor's resolution, as the reference and math traces are
//! (`Display > HDMI lines`). They have no persistence and no colour grade.

use super::*;

impl Ui {
    /// Every channel that is shown, as a polyline of the samples of the record that lie in the window of the screen,
    /// in its colour, in `area` (the lane of the plot). The samples are taken as the reference traces are: those in
    /// the window, a stride only where there are more than two for a pixel.
    pub(super) fn draw_live_lines(&self, cv: &mut Canvas, area: Rect, all_sources: bool) {
        let Some(rec) = self.last[0].as_ref().filter(|r| r.dt > 0.0) else { return };
        let (tdiv, delay) = self.view();
        if !(tdiv > 0.0) {
            return;
        }
        let half = tdiv as f64 * HDIV as f64 / 2.0;
        let (t_left, t_right) = (delay as f64 - half, delay as f64 + half);
        cv.set_clip(area);
        for c in (0..NCH).rev() {
            let v = &rec.ch[c];
            if !self.m.ch[c].on || (!all_sources && !self.shows(c)) || v.len() < 2 {
                continue;
            }
            let t0 = self.source_time_origin(WaveId::Ch(c), rec.t0);
            // the stored samples of the acquisition when the record is a plain resampling of them: the record has
            // 3600 points over the whole screen window, far too few for a zoom window (straight lines between them
            // turn the edges into triangles)
            let own = self.own_samples(rec, c, t0, (t_left, t_right), area.w);
            let (v, t0, dt) = match &own {
                Some((s, ts, sdt)) => (&s[..], *ts, *sdt),
                None => (&v[..], t0, rec.dt),
            };
            let at = |t: f64| ((t - t0) / dt).floor().clamp(-1.0, v.len() as f64) as isize;
            let k0 = (at(t_left) - 1).clamp(0, v.len() as isize) as usize;
            let k1 = (at(t_right) + 2).clamp(0, v.len() as isize) as usize;
            if k1 <= k0 + 1 {
                continue;
            }
            let step = ((k1 - k0) / (area.w as usize * 2).max(1)).max(1);
            let mut prev: Option<(i32, i32)> = None;
            for k in (k0..k1).step_by(step).chain(std::iter::once(k1 - 1)) {
                if !v[k].is_finite() {
                    prev = None;
                    continue;
                }
                let x = self.x_of_time((t0 + k as f64 * dt) as f32);
                let y = self.y_of_volts(c, v[k]).clamp(-10_000, 10_000);
                if let Some((x0, y0)) = prev {
                    cv.line(x0, y0, x, y, CH_COLORS[c]);
                }
                prev = Some((x, y));
            }
        }
        cv.no_clip();
    }

    /// The samples of the acquisition behind the record, in the window `(t_left, t_right)`, as (values, time of the
    /// first, step): windowed sinc between them when they lie more than two pixels apart, as the record does at fast
    /// time bases. `None` when the record is not a plain resampling (peak detect, averaging, roll, scan) or
    /// when the record's own points are already dense enough.
    fn own_samples(
        &self,
        rec: &Record,
        c: usize,
        t0: f64,
        win: (f64, f64),
        width: i32,
    ) -> Option<(Vec<f32>, f64, f64)> {
        let raw = rec.raw.as_ref()?;
        let f = &raw.frame;
        let v = &f.ch[c];
        let plain = !(rec.peak || f.envelope || f.roll || f.scan) && self.m.acq == AcqMode::Normal;
        if !plain || v.len() < 2 || !(f.dt > 0.0) || rec.dt <= f.dt * 1.5 {
            return None;
        }
        // time of frame sample 0, on the axis of the record shifted as `t0` is
        let ts = raw.t0 - raw.i0 as f64 * f.dt + (t0 - rec.t0);
        let a = (((win.0 - ts) / f.dt).floor() as i64 - 8).clamp(0, v.len() as i64 - 1) as usize;
        let b = (((win.1 - ts) / f.dt).ceil() as i64 + 9).clamp(a as i64 + 1, v.len() as i64) as usize;
        let per_px = (b - a) as f64 / width.max(1) as f64; // samples per pixel
        if b <= a + 1 || per_px > 4.0 {
            return None; // more than the lines can show: the record's points do
        }
        let up = ((1.0 / per_px / 2.0).ceil() as usize).clamp(1, 16); // points per sample, two pixels apart at most
        if up == 1 {
            return Some((v[a..b].to_vec(), ts + a as f64 * f.dt, f.dt));
        }
        let n = (b - a - 1) * up + 1;
        let mut out = Vec::with_capacity(n);
        oos_dsp::interp::lanczos8_grid(&v[..], a as f64, 1.0 / up as f64, n, &mut out);
        Some((out, ts + a as f64 * f.dt, f.dt / up as f64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor(u: &mut Ui) -> Vec<u8> {
        let mut cv = Canvas::new(1920, 1080);
        u.render_view(&mut cv);
        u.step(0.05);
        cv = Canvas::new(1920, 1080);
        u.render_view(&mut cv);
        crate::store::png_of(&cv)
    }

    /// A zoom window far inside the record: the lines are made from the samples of the acquisition, with sinc
    /// between them, not from the 3600 points of the whole screen window.
    #[test]
    fn a_zoom_window_draws_from_the_acquisition_samples_with_sinc_between_them() {
        let mut u = crate::ui::testkit::demo_ui();
        for _ in 0..5 {
            u.step(0.05);
        }
        // 1 ms of a 10 MHz square wave at 25 ns per sample, a record of 390 ns per point over 1.4 ms
        let mut ch: [Vec<f32>; NCH] = Default::default();
        ch[0] = (0..40_000).map(|i| if (i / 2) % 2 == 0 { 1.0 } else { -1.0 }).collect();
        let frame = std::sync::Arc::new(crate::board::Frame { dt: 25e-9, ch, ..Default::default() });
        let mut rec = u.last[0].clone().expect("a record");
        rec.dt = 390e-9;
        // the overview record of 390 ns per point is made of min/max pairs: still a resampling of the samples
        (rec.pairs, rec.peak) = (true, false);
        rec.raw = Some(crate::wave::Raw { frame, i0: 0, i1: 40_000, t0: rec.t0, conditioned: true });
        u.m.acq = AcqMode::Normal;
        // a window of 7 us: 280 samples over 1920 pixels
        let win = (rec.t0 + 100e-6, rec.t0 + 107e-6);
        let (v, t_first, dt) = u.own_samples(&rec, 0, rec.t0, win, 1920).expect("sparse in samples: sinc");
        assert!(dt < 25e-9 && dt > 0.0, "step {dt}");
        assert!(v.len() > 280 && v.iter().all(|y| y.is_finite()));
        assert!(t_first < win.0 && t_first + v.len() as f64 * dt > win.1);
        // a window of the whole record: the points of the record are dense enough
        assert!(u.own_samples(&rec, 0, rec.t0, (rec.t0, rec.t0 + 1.4e-3), 1920).is_none());
        // peak detect: never
        rec.peak = true;
        assert!(u.own_samples(&rec, 0, rec.t0, win, 1920).is_none());
    }

    /// The monitor of the HDMI Layout draws the live channels as lines or as the stretched phosphor, by `hdmi_lines`;
    /// the pictures differ and the setting is a SCPI one. `OOS_SHOTS=dir` keeps both pictures.
    #[test]
    fn the_monitor_draws_the_live_channels_as_lines_or_as_the_stretched_phosphor() {
        let mut u = crate::ui::testkit::demo_ui();
        u.m.hdmi = HdmiMode::Layout;
        for _ in 0..30 {
            u.step(0.05);
        }
        u.m.hdmi_lines = true;
        let lines = monitor(&mut u);
        u.m.hdmi_lines = false;
        let phosphor = monitor(&mut u);
        assert_ne!(lines, phosphor);
        if let Some(dir) = std::env::var_os("OOS_SHOTS") {
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(std::path::Path::new(&dir).join("monitor-lines.png"), &lines).unwrap();
            std::fs::write(std::path::Path::new(&dir).join("monitor-phosphor.png"), &phosphor).unwrap();
        }
    }
}
