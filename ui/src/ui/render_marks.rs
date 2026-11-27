// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The marks on the plot: the reference waveforms with their badges, the XY picture, the markers at the edges (trigger,
//! levels, offsets), the cursors with their readout, the info box and the search marks. A new mark gets its drawing
//! here and its call in `Ui::render_layers` (`render.rs`).

use super::*;

/// Search marks nearer than this many pixels (the width of a mark) are drawn as a strip, not one by one.
const SEARCH_MARK_GAP: i32 = 9;
/// The height of that strip, under the top edge of the plot.
pub(super) const SEARCH_STRIP_H: i32 = 4;
/// Height of the line of marks of one search: a mark and a pixel under it.
pub(super) const SEARCH_LANE_H: i32 = 9;

impl Ui {
    /// Vertical scale and the sample rate of this record. Time per division stays the one horizontal scale.
    pub(super) fn ref_badge_readout(r: &RefWave) -> (String, String) {
        let scale = format!("{}/div", si_short(r.vpd as f64, "V"));
        let rate = if r.dt.is_finite() && r.dt > 0.0 { si_short(1.0 / r.dt, "Sa/s") } else { "-".into() };
        (scale, rate)
    }

    pub(super) fn ref_badge_lines(&self, slot: usize, r: &RefWave, maximum: i32) -> (String, String) {
        let tag = format!("R{}", slot + 1);
        let mut name = if r.label.trim().is_empty() || r.label.trim() == tag {
            tag.clone()
        } else {
            format!("{} {}", tag, r.label.trim())
        };
        let width = (maximum - 12).max(16);
        if self.fonts().sb.width(&name) > width {
            while name.len() > tag.len() && self.fonts().sb.width(&format!("{}...", name)) > width {
                name.pop();
            }
            if self.fonts().sb.width(&format!("{}...", name)) <= width {
                name.push_str("...");
            } else {
                name = tag;
            }
        }
        let (scale, rate) = Self::ref_badge_readout(r);
        (name, format!("{} · {}", scale, rate))
    }

    /// Name, volts per division, and the record's sample rate, beside the trace.
    pub(super) fn draw_ref_badge(&self, cv: &mut Canvas, area: Rect, y: i32, col: Rgb, slot: usize, r: &RefWave) {
        let f = self.fonts();
        // Studio: a reference has the handle of every source, on its zero level (`studio_draw_handles`). The name,
        // the scale and the signal are on the reference's pill in the bottom bar, so nothing covers the trace
        if self.m.layout == Layout::Studio {
            return;
        }
        let maximum = (area.w - 16).clamp(36, 300);
        let (name, readout) = self.ref_badge_lines(slot, r, maximum);
        let text_w = f.sb.width(&name).max(f.s.width(&readout));
        let bw = (text_w + 12).clamp(36, maximum);
        let bh = REF_BADGE_H;
        let y0 = y.clamp(area.y + 2, (area.y + area.h - bh - self.ref_badge_pad(area)).max(area.y + 2));
        let x0 = (area.x + 80).min((area.x + area.w - bw - 4).max(area.x + 2));
        let b = Rect::new(x0, y0, bw, bh);
        cv.round(b, 3, col, 255);
        cv.set_clip(b);
        let ink = rgb(0, 0, 0);
        // two rows of their own: the name above, the scale and rate below it
        let row = (b.h - 4) / 2;
        cv.text_in(&f.sb, Rect::new(b.x, b.y + 2, b.w, row), 1, &name, ink);
        cv.text_in(&f.s, Rect::new(b.x, b.y + 2 + row, b.w, row), 1, &readout, ink);
        cv.set_clip(area);
    }

    /// Room a ref badge keeps clear under it: the time labels along the bottom of the lowest lane.
    pub(super) fn ref_badge_pad(&self, lane: Rect) -> i32 {
        let base = if self.stacked() && self.plot.cur_win + 1 < self.plot.wins.len() { 2 } else { self.fonts().s.size + 10 };
        // the Fit row and the With live note sit above the time labels of the last lane: the badge stays clear of them
        // (only a lane that reaches the bottom of the plot is under them)
        if !self.live_ref_note().is_empty() && !self.view_on() && !self.eye_open() && lane.bottom() >= self.plot.area.bottom() - 4
        {
            base.max(52)
        } else {
            base
        }
    }

    pub(super) fn draw_refs(&self, cv: &mut Canvas) {
        self.draw_refs_at(cv, self.plot.area, self.view(), false);
    }

    pub(super) fn draw_refs_at(&self, cv: &mut Canvas, area: Rect, view: (f32, f32), all_sources: bool) {
        cv.set_clip(area);
        let ppx = area.w as f64 / HDIV as f64;
        if !(view.0 > 0.0) {
            cv.no_clip();
            return;
        }
        let mut badges: Vec<i32> = Vec::new();
        for (i, r) in self.refs.iter().enumerate() {
            let Some(r) = r else { continue };
            // Studio: a ref with its own horizontal scale keeps it in the overview too
            let (tdiv, delay) = if all_sources { self.studio_ref_own(i).unwrap_or(view) } else { view };
            let (tdiv, delay) = (tdiv as f64, delay as f64);
            let half = tdiv * HDIV as f64 / 2.0;
            let (t_left, t_right) = (delay - half, delay + half);
            let t0 = self.source_time_origin(WaveId::Ref(i), r.t0);
            if !all_sources
                && self.stacked()
                && self.trace_sources().get(self.plot.cur_win) != Some(&workspace::Trace::Reference(i))
            {
                continue;
            }
            // a split (not Stacked): each reference in the window its Windows row names
            if !all_sources && !self.stacked() && self.plot.wins.len() > 1 && self.ref_win_of(i) != self.plot.cur_win {
                continue;
            }
            if !r.on || r.v.is_empty() || !(r.dt > 0.0) {
                continue;
            }
            let col = REF_COLORS[r.color.min(REF_COLORS.len() - 1)].1;
            // samples inside this window, not a stride of the whole record (that aliases a long record)
            let n = r.v.len() as isize;
            let at = |t: f64| -> isize {
                let x = (t - t0) / r.dt;
                if !x.is_finite() {
                    return 0;
                }
                x.floor().clamp(-1.0, n as f64) as isize
            };
            let k0 = (at(t_left) - 1).clamp(0, n) as usize;
            let k1 = (at(t_right) + 2).clamp(0, n) as usize;
            if k1 <= k0 {
                continue;
            }
            let cols = (area.w as usize).saturating_mul(2).max(1);
            let step = ((k1 - k0) / cols).max(1);
            let mut prev: Option<(i32, i32)> = None;
            let mut first = None;
            for k in (k0..k1).step_by(step).chain(std::iter::once(k1 - 1)) {
                let v = r.v[k];
                if !v.is_finite() {
                    prev = None;
                    continue;
                }
                let t = t0 + k as f64 * r.dt - delay;
                let x = (area.x as f64 + area.w as f64 / 2.0 + t / tdiv * ppx).round() as i32;
                let y = self.zoom_y_of_divs(area, (v + r.offset) / r.vpd).round() as i32;
                if let Some((x0, y0)) = prev {
                    if (x - x0).abs() < area.w * 2 && (y - y0).abs() < area.h * 4 {
                        cv.line(x0, y0, x, y, col);
                    }
                }
                if first.is_none() && x >= area.x && x < area.x + area.w {
                    first = Some(y);
                }
                prev = Some((x, y));
            }
            if let Some(y) = first {
                // badges of references that start at the same height are fanned out instead of printed over each other
                // none in the overview of a zoom (it shows the whole record, the detail has the badges)
                if !all_sources {
                    let (lo, hi) = (area.y + 2, (area.y + area.h - REF_BADGE_H - self.ref_badge_pad(area)).max(area.y + 2));
                    let spot = badge_spot(&badges, y, lo, hi);
                    badges.push(spot);
                    self.draw_ref_badge(cv, area, spot, col, i, r);
                }
            }
        }
        cv.no_clip();
    }

    pub(super) fn draw_xy(&self, cv: &mut Canvas) {
        // the X and Y channels (CH1 and CH2 unless the Add panel chose others) in a square of 8 x 8 div in the middle of the grid
        let Some(rec) = &self.last[0] else { return };
        let (xc, yc) = (self.xy_src[0].min(n_ch() - 1), self.xy_src[1].min(n_ch() - 1));
        let (a, b) = (&rec.ch[xc], &rec.ch[yc]);
        let f = self.fonts();
        // a square that fits the area both ways: a narrow docked pane keeps its title and its left edge
        let side = self.plot.area.h.min(self.plot.area.w);
        let sq = Rect::new(self.plot.area.x + (self.plot.area.w - side) / 2, self.plot.area.y + (self.plot.area.h - side) / 2, side, side);
        cv.fill(sq, rgb(0x04, 0x04, 0x08));
        cv.rect(sq, GRID_AXIS);
        cv.vline(sq.x + side / 2, sq.y, sq.y + side - 1, GRID_LINE);
        cv.hline(sq.x, sq.x + side - 1, sq.y + side / 2, GRID_LINE);
        if a.is_empty() || b.is_empty() {
            cv.text_in(&f.m, sq, 1, &format!("XY needs CH{} and CH{}", xc + 1, yc + 1), DIM);
            return;
        }
        cv.set_clip(sq);
        let ppd = side as f32 / 8.0;
        let (ca, cb) = (&self.m.ch[xc], &self.m.ch[yc]);
        let col = rgb(0x9c, 0xf0, 0x9c);
        // Display > Dots: a dot per point (On, Auto), or each point joined to the one before (Off)
        let dots = self.m.dots.xy();
        let mut prev: Option<(i32, i32)> = None;
        for k in 0..a.len().min(b.len()) {
            if !a[k].is_finite() || !b[k].is_finite() {
                prev = None;
                continue;
            }
            let x = (sq.x as f32 + side as f32 / 2.0 + (a[k] + ca.offset) / ca.volts_per_div() * ppd) as i32;
            let y = (sq.y as f32 + side as f32 / 2.0 - (b[k] + cb.offset) / cb.volts_per_div() * ppd) as i32;
            if dots {
                cv.pixel(x, y, col, 160);
                cv.pixel(x + 1, y, col, 70);
                cv.pixel(x, y + 1, col, 70);
            } else if let Some((x0, y0)) = prev {
                cv.line(x0, y0, x, y, col);
            }
            prev = Some((x, y));
        }
        cv.no_clip();
        cv.text(&f.s, sq.x + 6, sq.y + 16, &format!("XY  X = CH{}  Y = CH{}", xc + 1, yc + 1), col);
    }

    /// Display > Dots = Auto with the samples at least `MARK_SPACING_PX` apart (the interpolated time bases): each sample
    /// of the newest record marked over its line, a 2 x 2 square lighter than the trace. On the layer over the traces,
    /// so the phosphor maps and the pictures' grading stay those of the lines. The idea comes from fumingcaps' fork
    /// (0.2.2).
    pub(super) fn draw_sample_marks(&self, cv: &mut Canvas) {
        let Some(rec) = &self.last[0] else { return };
        if rec.pairs || rec.peak || rec.partial {
            return;
        }
        // the samples the scope took: the hardware frame's window when there is one (the record's points are then
        // interpolated between them), else the record's own points
        let (t0, dt, raw) = match &rec.raw {
            Some(r) if r.frame.envelope || r.frame.roll || r.frame.scan => return,
            Some(r) => (r.t0, r.frame.dt, Some(r)),
            None => (rec.t0, rec.dt, None),
        };
        let a = self.plot.area;
        let (tdiv, _) = self.view();
        if !(dt > 0.0) || !self.m.dots.marks(dt / tdiv as f64 * a.w as f64 / HDIV as f64) {
            return;
        }
        cv.set_clip(a);
        for c in (0..NCH).filter(|&c| self.m.ch[c].on && self.shows(c) && self.m.z_src != Some(c)) {
            let col = mix(CH_COLORS[c], rgb(255, 255, 255), 140);
            let v = match raw {
                Some(r) => r.frame.ch[c].get(r.i0..r.i1).unwrap_or(&[]),
                None => &rec.ch[c][..],
            };
            for (j, &v) in v.iter().enumerate().filter(|(_, v)| v.is_finite()) {
                let (x, y) = (self.x_of_time((t0 + j as f64 * dt) as f32), self.y_of_volts(c, v));
                if x >= a.x && x < a.x + a.w {
                    cv.fill(Rect::new(x - 1, y - 1, 2, 2), col);
                }
            }
        }
        cv.no_clip();
    }

    pub(super) fn draw_markers(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let a = self.plot.area;
        // Studio: the handle of every source of this window, over the traces (`studio_handle_rects`)
        if self.m.layout == Layout::Studio {
            self.studio_draw_handles(cv);
        }
        // channel ground markers at the left edge of the grid
        for c in 0..NCH {
            if !self.m.ch[c].on || !self.shows(c) {
                continue;
            }
            let y = self.y_of_volts(c, 0.0).clamp(a.y + (a.h / 2).min(9), a.y + a.h - (a.h / 2).min(9));
            let col = CH_COLORS[c];
            if self.m.layout == Layout::Studio { continue; }
            let r = Rect::new(a.x + 2, y - 8, 16, 16);
            cv.round(r, 3, col, 255);
            cv.triangle([(a.x + 17, y - 6), (a.x + 24, y), (a.x + 17, y + 6)], col);
            cv.text_in(&f.sb, r, 1, &format!("{}", c + 1), DARK_TEXT);
        }
        // trigger level marker at the right edge of the grid, its value next to it. Not with AC or LF reject trigger
        // coupling: the level then counts from the signal's mean, a mark at that level in volts would point nowhere
        // (the trigger card shows the level)
        let ts = self.m.trig_src;
        let lvl = self.trig_level_shown();
        let filtered = matches!(self.m.trig_coupling(), TrigCoup::Ac | TrigCoup::LfReject);
        if self.m.trig_ref.is_none()
            && self.m.ch[ts].on
            && self.shows(ts)
            && self.m.trig_line().is_none()
            && !filtered
            && self.trigger_absolute_level_visible(false)
        {
            let y = self.y_of_volts(ts, lvl).clamp(a.y + (a.h / 2).min(9), a.y + a.h - (a.h / 2).min(9));
            let x1 = self.trigger_tag_x(a.x + a.w - MARK_W, y);
            let r = Rect::new(x1 + 8, y - 8, 16, 16);
            cv.round(r, 3, ACCENT, 255);
            cv.triangle([(x1 + 9, y - 6), (x1 + 2, y), (x1 + 9, y + 6)], ACCENT);
            cv.text_in(&f.sb, r, 1, "T", DARK_TEXT);
            let s = si_short(lvl as f64, self.m.unit(ts));
            backed_text(cv, &f.sb, x1 - 4 - f.sb.width(&s), y + 5, &s, ACCENT);
            if self.touch.drag == Drag::Level || self.level_line_on() {
                cv.dashed_hline(a.x, a.x + a.w - 1, y, 6, 4, ACCENT);
            }
        }
        // second level: lower threshold, or level of source B / the clock
        let t = self.m.trig_type;
        let c2 = match t {
            TrigType::Slope | TrigType::Runt | TrigType::Window => Some((ts, "t")),
            TrigType::Delay => Some((self.m.trig_src2, "B")),
            TrigType::SetupHold => Some((self.m.trig_src2, "C")),
            _ => None,
        };
        if let Some((c, tag)) = c2 {
            let secondary = matches!(t, TrigType::Delay | TrigType::SetupHold);
            let source = if secondary { self.m.trigger_wave2() } else { self.m.trigger_wave() };
            if source == WaveId::Ch(c)
                && self.m.ch[c].on
                && self.shows(c)
                && self.trigger_absolute_level_visible(secondary)
            {
                let y =
                    self.y_of_volts(c, self.m.trig_level2).clamp(a.y + (a.h / 2).min(9), a.y + a.h - (a.h / 2).min(9));
                let x1 = self.trigger_tag_x(a.x + a.w - MARK_W, y);
                let r = Rect::new(x1 + 8, y - 8, 16, 16);
                cv.round(r, 3, mix(BG, ACCENT, 170), 255);
                cv.triangle([(x1 + 9, y - 6), (x1 + 2, y), (x1 + 9, y + 6)], mix(BG, ACCENT, 170));
                cv.text_in(&f.sb, r, 1, tag, DARK_TEXT);
                let s = si_short(self.m.trig_level2 as f64, self.m.unit(ts));
                backed_text(cv, &f.sb, x1 - 4 - f.sb.width(&s), y + 5, &s, mix(BG, ACCENT, 200));
                if self.touch.drag == Drag::Level2 {
                    cv.hline(a.x, a.x + a.w - 1, y, ACCENT);
                }
            }
        }
        self.draw_reference_trigger_markers(cv);
        // Trigger time is absolute in an imported record.
        let tx = self.x_of_time(self.replay.trigger_time.unwrap_or(0.0) as f32);
        let ty = a.y + 9;
        if tx < a.x {
            cv.triangle([(a.x + 1, ty - 4), (a.x + 9, ty - 9), (a.x + 9, ty + 1)], ACCENT);
        } else if tx > a.x + a.w {
            let x = a.x + a.w - 1;
            cv.triangle([(x, ty - 4), (x - 8, ty - 9), (x - 8, ty + 1)], ACCENT);
        } else {
            cv.triangle([(tx - 6, ty - 8), (tx + 6, ty - 8), (tx, ty)], ACCENT);
        }
        // drag feedback
        let tag = |cv: &mut Canvas, x: i32, y: i32, s: &str, col: Rgb| {
            let r = Rect::new(x, y, f.sb.width(s) + 20, 22);
            cv.framed(r, 5, 1, LINE, 4, BAR);
            cv.text_in(&f.sb, r, 1, s, col);
        };
        match self.touch.drag {
            Drag::Offset(c) if self.shows(c) => {
                let y = self.y_of_volts(c, 0.0);
                cv.dashed_hline(a.x, a.x + a.w - 1, y, 6, 4, CH_COLORS[c]);
                tag(
                    cv,
                    a.x + 8,
                    y - 30,
                    &format!("Offset {}", si(self.m.ch[c].offset as f64, self.m.unit(c))),
                    CH_COLORS[c],
                );
            }
            Drag::Delay if self.plot.cur_win == self.touch.win.min(self.plot.wins.len().max(1) - 1) => {
                tag(cv, a.x + a.w / 2 - 60, a.y + 12, &format!("Delay {}", si(self.view().1 as f64, "s")), ACCENT)
            }
            Drag::Level
                if self.shows(ts)
                    && !filtered
                    && self.m.trig_ref.is_none()
                    && self.trigger_absolute_level_visible(false) =>
            {
                let y = self.y_of_volts(ts, self.m.trig_level);
                tag(
                    cv,
                    a.x + a.w - 130,
                    y - 30,
                    &format!("Level {}", si(self.m.trig_level as f64, self.m.unit(ts))),
                    ACCENT,
                );
            }
            _ => {}
        }
    }

    pub(super) fn draw_cursors(&self, cv: &mut Canvas) {
        let source = self.cursor_source();
        if !self.workspace_source_visible_here(source)
            && !(self.m.cursor == CursorMode::Track && self.workspace_source_visible_here(self.cursor_b_source()))
        {
            return;
        }
        let f = self.fonts();
        let a = self.plot.area;
        cv.set_clip(a);
        let picked = self.workspace.cursor_sel.and_then(|k| self.cursor_tags().into_iter().find(|t| t.0 == k).map(|t| t.1));
        let tint_a = self.cursor_tint(source);
        let tint_b = self.cursor_tint(self.cursor_b_source());
        let tagbox = |cv: &mut Canvas, x: i32, y: i32, s: &str, col: Rgb| {
            let r = Rect::new(x, y, f.s.width(s) + 8, 14);
            cv.round(r, 2, col, 255);
            cv.text_in(&f.s, r, 1, s, DARK_TEXT);
            if picked == Some(r) { cv.round_outline(Rect::new(r.x - 2, r.y - 2, r.w + 4, r.h + 4), 3, ACCENT); }
        };
        match self.m.cursor {
            CursorMode::Off => {}
            CursorMode::Time | CursorMode::Track | CursorMode::Xy => {
                if self.m.cursor == CursorMode::Xy {
                    for (frac, name) in [(self.m.cur_c, "Y1"), (self.m.cur_d, "Y2")] {
                        let y = a.y + (frac * a.h as f32) as i32;
                        cv.dashed_hline(a.x, a.x + a.w - 1, y, 4, 3, tint_a);
                        tagbox(cv, self.right_tag_x(a, y - 16, f.s.width(name) + 8), y - 16, name, tint_a);
                    }
                }
                let (ta, tb) = self.cursor_times();
                for (frac, name, t) in [(self.m.cur_a, "X1", ta), (self.m.cur_b, "X2", tb)] {
                    // a cursor that the zoomed view does not show keeps its time and is not drawn
                    if !(0.0..=1.0).contains(&frac) {
                        continue;
                    }
                    let x = a.x + (frac * a.w as f32) as i32;
                    // time is global: the line crosses every lane; it has the colour of the source it reads
                    let col = if name == "X1" || self.m.cursor != CursorMode::Track { tint_a } else { tint_b };
                    cv.dashed_vline(x, a.y, a.y + a.h - 1, 4, 3, col);
                    tagbox(cv, x + 2, a.y + a.h - 36, name, col);
                    // Each tracking mark follows its own native source and lane.
                    if self.m.cursor == CursorMode::Track {
                        let id = if name == "X1" { source } else { self.cursor_b_source() };
                        if self.workspace_source_visible_here(id) {
                            if let Some(v) = self.wave_at_source(id, t) {
                                let y = self.y_of_wave(id, v);
                                cv.dashed_hline(x - 10, x + 10, y, 2, 2, col);
                            }
                        }
                    }
                }
            }
            CursorMode::Volt => {
                for (frac, name) in [(self.m.cur_a, "Y1"), (self.m.cur_b, "Y2")] {
                    let y = a.y + (frac * a.h as f32) as i32;
                    cv.dashed_hline(a.x, a.x + a.w - 1, y, 4, 3, tint_a);
                    tagbox(cv, self.right_tag_x(a, y - 16, f.s.width(name) + 8), y - 16, name, tint_a);
                }
            }
        }
        self.draw_studio_cursor_readouts(cv, a);
        cv.no_clip();
    }

    /// Left edge of a cursor tag at the right end of the plot: where it clears the corner buttons.
    pub(super) fn right_tag_x(&self, a: Rect, y: i32, w: i32) -> i32 {
        let mut x = a.x + a.w - 26;
        for r in self.corner_btns() {
            if x + w > r.x - 2 && x < r.right() + 2 && y < r.bottom() + 2 && y + 14 > r.y - 2 {
                x = r.x - 6 - w;
            }
        }
        x
    }

    /// Cursor read-outs: (label, value) pairs.
    pub(super) fn cursor_values(&self) -> Vec<(String, String)> {
        let (source, scale, offset, unit) = match self.cursor_source_context() {
            Some(context) => context,
            None if self.m.cursor == CursorMode::Track => {
                let source = self.cursor_source();
                (source, 1.0, 0.0, self.cursor_source_unit(source))
            }
            None => return Vec::new(),
        };
        let area = self.source_area(source).unwrap_or(self.plot.area);
        match self.m.cursor {
            CursorMode::Off => Vec::new(),
            CursorMode::Xy => {
                let (ta, tb) = self.cursor_times();
                let v = |c: f32| self.zoom_divs_at(area, c) * scale - offset;
                let (va, vb) = (v(self.m.cur_c), v(self.m.cur_d));
                vec![
                    ("X1".into(), si(ta, "s")),
                    ("X2".into(), si(tb, "s")),
                    ("Y1".into(), si(va as f64, unit)),
                    ("Y2".into(), si(vb as f64, unit)),
                    ("\u{394}X".into(), si(tb - ta, "s")),
                    ("\u{394}Y".into(), si((vb - va) as f64, unit)),
                ]
            }
            CursorMode::Track => {
                let (ta, tb) = self.cursor_times();
                let second = self.cursor_b_source();
                let (ya, yb) = (self.wave_at_source(source, ta), self.wave_at_source(second, tb));
                let (ua, ub) = (unit, self.cursor_source_unit(second));
                let txt = |v: Option<f32>, u: &str| v.map_or("----".to_string(), |v| si(v as f64, u));
                let dy = ya.zip(yb).filter(|_| ua == ub).map(|(a, b)| b - a);
                vec![
                    ("X1".into(), si(ta, "s")),
                    ("Y1".into(), txt(ya, ua)),
                    ("X2".into(), si(tb, "s")),
                    ("Y2".into(), txt(yb, ub)),
                    ("\u{394}X".into(), si(tb - ta, "s")),
                    ("\u{394}Y".into(), txt(dy, if ua == ub { ua } else { "" })),
                ]
            }
            CursorMode::Time => {
                let (ta, tb) = self.cursor_times();
                let d = tb - ta;
                vec![
                    ("X1".into(), si(ta, "s")),
                    ("X2".into(), si(tb, "s")),
                    ("\u{394}X".into(), si(d, "s")),
                    ("1/\u{394}X".into(), Self::inv_dt(d)),
                ]
            }
            CursorMode::Volt => {
                let va = self.zoom_divs_at(area, self.m.cur_a) * scale - offset;
                let vb = self.zoom_divs_at(area, self.m.cur_b) * scale - offset;
                vec![
                    ("Y1".into(), si(va as f64, unit)),
                    ("Y2".into(), si(vb as f64, unit)),
                    ("\u{394}Y".into(), si((vb - va) as f64, unit)),
                ]
            }
        }
    }

    /// Status line inside the top of the plot: touch lock, history position and search count on the left; a source that
    /// is not the live hardware on the right.
    pub(super) fn draw_info(&self, cv: &mut Canvas) {
        if self.eye_open() && self.m.eye_place == EyePlace::Full && self.workspace.eye_float.is_none() {
            return;
        }
        let f = self.fonts();
        let g = self.plot.grid;
        let y = g.y + 14;
        // only what makes the picture something else than a live measurement; the rates are on System info
        let record_caption = self.record_caption();
        let src = if self.record_mode() {
            record_caption.as_str()
        } else {
            match self.src.name() {
                "DEMO" | "REPLAY" if self.m.layout == Layout::Studio => "",
                "DEMO" => "Demo signals",
                "REPLAY" => "Replay",
                _ => "",
            }
        };
        if !src.is_empty() {
            // Studio writes "Full" or "Exit" left of the corner button: the caption ends before that word
            let word = if self.m.layout == Layout::Studio { f.s.width("Full") + 10 } else { 0 };
            backed_text(cv, &f.s, self.full_btn().0.x - 16 - word - f.s.width(src), y, src, DIM);
        }
        let mut x = g.x + 8;
        // a channel at the end of the ADC range: said in red, first on the line
        let clip = self.clipping_channels();
        if !clip.is_empty() {
            let names: Vec<String> = clip.iter().map(|c| format!("CH{}", c + 1)).collect();
            x = backed_text(cv, &f.sb, x, y, &format!("Clipping: {} \u{b7} raise V/div or move the trace", names.join(" ")), RED) + 16;
        }
        if self.m.touch_lock {
            let hint = if self.m.layout == Layout::Studio { "Touch locked \u{b7} tap the gear to unlock" } else { "Touch locked \u{b7} tap Unlock" };
            x = backed_text(cv, &f.sb, x, y, hint, AMBER) + 16;
        }
        let seg = self.seg.view.filter(|_| self.seg_hw_on()).and_then(|v| match (v.phase, self.seg.pos) {
            (crate::board::SegPhase::Recording, _) => Some(format!("Recording {}/{}", v.recorded, v.total)),
            (crate::board::SegPhase::Reading, _) => Some(format!("Reading {}/{}", v.read, v.recorded)),
            (crate::board::SegPhase::Done, Some(i)) => Some(self.seg_label(i)),
            _ => None,
        });
        if let Some(t) = seg {
            x = backed_text(cv, &f.sb, x, y, &t, BLUE) + 16;
        } else if let Some(i) = self.history.pos {
            x = backed_text(cv, &f.sb, x, y, &format!("History {}/{}", i + 1, self.history.recs.len()), BLUE) + 16;
        }
        if self.search_on {
            let events = self.search_events();
            let label = if self.m.trig_ref.is_some() && !self.reference_search_supported() {
                "Search: condition needs hardware channels".into()
            } else if self.m.trig_ref.is_some() && self.reference_search_busy() {
                format!("Search {} · full record · computing…", self.trigger_source_label())
            } else {
                format!(
                    "Search {}{}: {} events",
                    self.trigger_source_label(),
                    if self.m.trig_ref.is_some() { " · full record" } else { "" },
                    events.len()
                )
            };
            backed_text(cv, &f.sb, x, y, &label, AMBER);
        }
        let note = self.live_ref_note();
        if !note.is_empty() && !self.view_on() && !self.eye_open() {
            let y = g.y + g.h - 36;
            let fit = Rect::new(g.x + 8, y - 13, 52, 24);
            btn(cv, fit, BTN, ACCENT);
            cv.text_in(&f.s, fit, 1, "Fit", TEXT);
            let lab = Rect::new(fit.x + fit.w + 8, y - 12, (g.x + g.w - fit.x - fit.w - 16).max(1), 22);
            cv.set_clip(lab);
            let text = self.fit_ref_note(&note, lab.w);
            // over a trace of the last lane: backed, like the time labels
            cv.blend(Rect::new(lab.x - 3, lab.y + 2, f.s.width(&text) + 6, lab.h - 4), BG, 190);
            cv.text_in(&f.s, lab, 0, &text, LABEL);
            cv.no_clip();
        }
    }

    /// The "With live" note cut to `width`, naming how many references no longer fit ("+2").
    pub(super) fn fit_ref_note(&self, note: &str, width: i32) -> String {
        let f = self.fonts();
        if f.s.width(note) <= width {
            return note.to_string();
        }
        let Some((head, rest)) = note.split_once(" · ") else { return note.to_string() };
        let parts: Vec<&str> = rest.split("   ").collect();
        for keep in (0..parts.len()).rev() {
            let text = if keep == 0 {
                format!("{head} · +{}", parts.len())
            } else {
                format!("{head} · {}   +{}", parts[..keep].join("   "), parts.len() - keep)
            };
            if f.s.width(&text) <= width || keep == 0 {
                return text;
            }
        }
        note.to_string()
    }

    /// Search markers: a tick at the top of the plot for every event. Mark events of the trigger condition has amber
    /// ones; each search of Studio has a line of marks of its own in its colour, one line under the other, so two
    /// searches that find the same edge both show. The selected search draws its lines down the plot.
    pub(super) fn draw_search(&self, cv: &mut Canvas) {
        if self.studio_search_marks().is_some() {
            for (slot, _) in self.studio.search_list().filter(|(_, s)| !s.hidden) {
                let top = self.plot.area.y + slot as i32 * SEARCH_LANE_H;
                let events = self.studio_search_events_of(slot);
                // under the lines of marks of every search
                let lines = (slot == self.studio.search_sel).then_some(self.plot.area.y + super::addrail::SEARCH_SLOTS as i32 * SEARCH_LANE_H);
                self.draw_search_marks(cv, &events, top, super::addrail::SEARCH_COLS[slot], lines);
            }
        } else if self.search_on {
            self.draw_search_marks(cv, &self.search_events(), self.plot.area.y, AMBER, Some(self.plot.area.y + SEARCH_LANE_H));
        }
    }

    /// The searches in the overview of a zoomed view: each has a line of ticks at the top, in its colour and in the
    /// order of the numbers, over the whole of the main view. The zoomed part has the marks themselves; the overview
    /// says where in the record the events are, which the zoom window is moved to (it had no marks at all).
    pub(super) fn draw_search_overview(&self, cv: &mut Canvas, o: Rect) {
        let view = (self.m.time_per_div(), self.m.delay);
        let mut line = |slot: usize, events: &[f64], col: Rgb| {
            let top = o.y + 1 + slot as i32 * (SEARCH_STRIP_H + 1);
            for column in Self::search_columns(events, o.w, view) { cv.vline(o.x + column, top, top + SEARCH_STRIP_H - 1, col); }
        };
        if self.studio_search_marks().is_some() {
            for (slot, _) in self.studio.search_list().filter(|(_, s)| !s.hidden) { line(slot, &self.studio_search_events_of(slot), super::addrail::SEARCH_COLS[slot]); }
        } else if self.search_on {
            line(0, &self.search_events(), AMBER);
        }
    }

    /// The columns of a plot `w` wide showing `view` (time per division, delay) that have an event. At most one a
    /// pixel, without walking the events that are off the screen.
    fn search_columns(events: &[f64], w: i32, (scale, delay): (f32, f32)) -> Vec<i32> {
        let left = delay as f64 - scale as f64 * HDIV as f64 * 0.5;
        let per_pixel = scale as f64 * HDIV as f64 / w.max(1) as f64;
        let mut first = events.partition_point(|&time| time < left);
        let mut columns = Vec::new();
        for column in 0..w {
            let next = events.partition_point(|&time| time < left + (column + 1) as f64 * per_pixel);
            if next > first {
                columns.push(column);
            }
            first = next;
        }
        columns
    }

    /// One line of marks with its top at `top`; from `lines` down a dashed line under each mark that stands alone.
    fn draw_search_marks(&self, cv: &mut Canvas, events: &[f64], top: i32, mark_col: Rgb, lines: Option<i32>) {
        let a = self.plot.area;
        let columns = Self::search_columns(events, a.w, self.view());
        for (i, &column) in columns.iter().enumerate() {
            let x = a.x + column;
            let near = |other: Option<&i32>| other.is_some_and(|&o| (o - column).abs() < SEARCH_MARK_GAP);
            if near(i.checked_sub(1).and_then(|p| columns.get(p))) || near(columns.get(i + 1)) {
                // events too close to tell apart: a strip at the top says where they are. A mark and a dashed line
                // in every column drew the lines side by side, and their dashes made bands across the whole plot
                // (64 000 edges of a 40 MHz square at 20 us/div, unit 2026-11-20)
                cv.vline(x, top + 1, top + SEARCH_STRIP_H, mark_col);
            } else {
                cv.triangle([(x - 4, top + 1), (x + 4, top + 1), (x, top + 8)], mark_col);
                if let Some(from) = lines { cv.dashed_vline(x, from, a.y + a.h - 1, 2, 6, mix(BG, mark_col, 90)); }
            }
        }
        // the event raised in the list (tapped there, or stepped to): a larger mark in the colour of the text and a
        // line down the plot, over the others
        let (scale, delay) = self.view();
        let Some(t) = lines.and(self.search_picked(events)) else { return };
        let column = ((t - (delay as f64 - scale as f64 * HDIV as f64 * 0.5)) / (scale as f64 * HDIV as f64) * a.w as f64).floor();
        if (0.0..a.w as f64).contains(&column) {
            let x = a.x + column as i32;
            cv.triangle([(x - 7, top), (x + 7, top), (x, top + 12)], TEXT);
            cv.vline(x, top + 12, a.y + a.h - 1, mix(BG, TEXT, 140));
        }
    }
}

#[cfg(test)]
mod sample_marks_tests {
    use super::*;
    use crate::wave::Record;

    /// The pixels `draw_sample_marks` lights on an empty canvas with CH1's record of `n` samples over the screen.
    fn lit(dots: Dots, n: usize) -> usize {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.dots = dots;
        let (tdiv, _) = ui.view();
        let window = tdiv as f64 * HDIV as f64;
        let dt = window / n as f64;
        let v: Vec<f32> = (0..n).map(|k| if k % 2 == 0 { 0.5 } else { -0.5 }).collect();
        ui.last[0] = Some(Record {
            t0: -window / 2.0,
            dt,
            ch: [v, Vec::new(), Vec::new(), Vec::new()],
            triggered: true,
            partial: false,
            raw: None,
            la: None,
            pairs: false,
            peak: false,
        });
        let mut cv = Canvas::new(PANEL_W, PANEL_H);
        ui.draw_sample_marks(&mut cv);
        cv.px.iter().filter(|&&p| p != 0).count()
    }

    // Break it catches: marks with On or Off, marks over a dense record (a dotted band over the line), or none where the
    // samples lie far apart in Auto
    #[test]
    fn auto_marks_only_samples_that_lie_far_apart() {
        // about 1000 px of plot: 50 samples lie 20 px apart, 2000 samples half a pixel
        assert!(lit(Dots::Auto, 50) >= 4 * 40, "Auto, sparse: {}", lit(Dots::Auto, 50));
        assert_eq!(lit(Dots::Auto, 2000), 0);
        assert_eq!(lit(Dots::On, 50), 0);
        assert_eq!(lit(Dots::Off, 50), 0);
    }
}
