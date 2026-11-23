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
            let at = |t: f64| ((t - t0) / rec.dt).floor().clamp(-1.0, v.len() as f64) as isize;
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
                let x = self.x_of_time((t0 + k as f64 * rec.dt) as f32);
                let y = self.y_of_volts(c, v[k]).clamp(-10_000, 10_000);
                if let Some((x0, y0)) = prev {
                    cv.line(x0, y0, x, y, CH_COLORS[c]);
                }
                prev = Some((x, y));
            }
        }
        cv.no_clip();
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
