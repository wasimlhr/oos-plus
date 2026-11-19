// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Drawing the screen: `Ui::render` and its layers (under and over the traces, the GPU jobs), the views, the
//! graticule, the axis labels, the traces with their brightness grading and the persistence maps. The parts drawn over
//! the traces are in `render_derived.rs` (mask, histogram, math, decode) and `render_marks.rs` (references, markers,
//! cursors); a new layer of the screen goes into `Ui::render_layers`.

use super::*;

impl Ui {
    pub fn render(&mut self, cv: &mut Canvas) {
        self.render_layers(&mut Layers { under: cv, over: None, jobs: Vec::new() });
    }

    /// The screen into `l`: with one canvas everything is drawn on it; with two (System > Graphics = GPU) the
    /// background, graticule and masks go under the traces, everything else over them, and the traces become jobs
    /// for the GPU (`gpu.rs`). The order of the drawing is the same either way.
    pub fn render_layers(&mut self, l: &mut Layers) {
        self.studio_frame_start();
        self.relayout();
        if let Some(o) = l.over.as_deref_mut() {
            o.no_clip();
            o.clear(0);
        }
        if self.start_up_active() {
            self.draw_start_up(l.under);
            // the power dialog over it (the power key works during the start-up)
            self.draw_overlay(l.over());
            return;
        }
        l.under.no_clip();
        l.under.clear(BG);
        let g = self.grid;
        if self.m.zoom && !self.xy_over() {
            let (o, _) = self.split_zoom(g);
            self.draw_grid(l.under, o);
            self.traces(l, o, false, true);
            self.draw_refs_at(l.over(), o, (self.m.time_per_div(), self.m.delay), true);
            self.draw_studio_overview_math(l.over(), o);
            // zoom window in the overview: dim the rest, frame the window
            let (xa, xb) = self.zoom_window_x(o);
            let cv = l.over();
            cv.set_clip(o);
            cv.blend(Rect::new(o.x, o.y, (xa - o.x).max(0), o.h), BG, 170);
            cv.blend(Rect::new(xb, o.y, (o.x + o.w - xb).max(0), o.h), BG, 170);
            // the window is the same slim frame with a handle at each end in every layout
            cv.rect(Rect::new(xa, o.y, (xb - xa).max(2), o.h), ACCENT);
            for edge in [xa, xb] {
                cv.round(Rect::new(edge - 3, o.y + o.h / 2 - 12, 7, 24), 3, ACCENT, 255);
            }
            if self.m.layout == Layout::Studio {
                self.draw_studio_overview_axis(cv, o);
                self.draw_studio_overview_cursors(cv, o);
            }
            cv.no_clip();
            for &lane in &self.wins {
                self.draw_grid(l.under, lane);
            }
            self.draw_lane_chrome(l.under);
        } else {
            for &w in &self.wins {
                self.draw_grid(l.under, w);
            }
            self.draw_lane_chrome(l.under);
        }
        if self.view_on() {
            // the analysers and XY draw their own picture: under what goes over it
            if self.xy {
                self.draw_xy(l.under);
            } else {
                self.draw_analyzer(l.under);
            }
            self.draw_view_btn(l.over());
        } else {
            // Every detail lane draws its own sources. The shared zoom overview
            // was drawn above; detail maps use the same lane dimensions.
            for i in 0..self.wins.len() {
                self.cur_win = i;
                self.area = self.wins[i];
                if !self.m.zoom {
                    self.draw_mask(l.under);
                    self.traces(l, self.area, false, false);
                } else {
                    self.traces(l, self.area, true, false);
                }
                let cv = l.over();
                if !self.dock_on() { self.draw_hist(cv); }
                self.draw_zones(cv);
                if self.wins.len() > 1 || i == 0 || self.stacked() {
                    if self.waves_on() {
                        self.draw_refs_at(cv, self.waves_plot(self.area), self.view(), false);
                    } else {
                        self.draw_refs(cv);
                    }
                }
                self.draw_math(cv);
                if i == self.la_win_of() {
                    self.draw_la(cv);
                }
                self.draw_decode(cv);
                if i == 0 {
                    self.draw_bus(cv);
                }
                self.draw_meas_labels(cv);
                if i == 0 {
                    self.draw_callouts(cv);
                }
                self.draw_cursors(cv);
                self.draw_search(cv);
                self.draw_bus_event(cv);
                self.draw_markers(cv);
                self.draw_axes(cv);
                self.draw_mask_pill(cv);
                if i == 0 {
                    self.draw_waves_board(cv);
                }
            }
            self.cur_win = 0;
            self.area = self.wave_area();
        }
        let cv = l.over();
        if self.m.zoom && !self.xy_over() && !self.view_on() {
            let (tz, dz) = self.view();
            self.draw_zoom_readout(cv, tz, dz);
        }
        if self.eye_open() {
            // eye_pane_rect maps Full to the plot while retaining floating panes.
            self.draw_eye_pane(cv);
        }
        self.draw_cal_banner(cv);
        self.draw_info(cv);
        self.draw_studio_overlay(cv);
        self.draw_studio_zoom_overlay(cv);
        self.draw_zoom_box(cv);
        self.draw_full_btn(cv);
        if self.plot_btns_shown() {
            self.draw_zoom_btn(cv);
            self.draw_win_btn(cv);
        }
        self.draw_auto_bar(cv);
        self.draw_strip(cv);
        // Full on the external display is the plot and the strip. The statistics card and a lower pane stay on the panel.
        if self.hdmi_snap_want && self.m.density == Density::Full {
            self.capture_hdmi(l);
        }
        let cv = l.over();
        if self.pane_h() > 0 {
            cv.set_view_clip(self.dock_on().then(|| self.dock_rect(dock::VKind::Pane)).flatten());
            self.draw_pane(cv);
            // in a dock the view's title strip has the close button
            if !self.dock_on() { self.draw_pane_close(cv); }
            cv.set_view_clip(None);
        }
        self.draw_results(cv);
        self.draw_studio_panel(cv);
        cv.set_view_clip(self.dock_on().then(|| self.dock_rect(dock::VKind::Table)).flatten());
        self.draw_table(cv);
        cv.set_view_clip(None);
        self.draw_view_tiles(cv);
        self.draw_dock(cv);
        self.draw_md_overlays(cv);
        self.draw_axis_highlight(cv);
        self.draw_full_chrome(cv);
        // waveform on the external display: the band between the bars, before the bars and the menus go over it
        // (the GPU puts it together from the layers itself)
        if self.hdmi_snap_want && self.m.density != Density::Full {
            self.capture_hdmi(l);
        }
        let cv = l.over();
        if !self.full_on() {
            self.draw_top(cv);
            self.draw_bottom(cv);
        }
        self.draw_menu(cv);
        self.draw_info_card(cv);
        self.draw_toast(cv);
        self.draw_overlay(cv);
        self.workspace_draw(cv);
        self.draw_studio_floating(cv);
        self.draw_studio_view_menu(cv);
        self.draw_studio_add_menu(cv);
        self.draw_studio_ctx(cv);
        // the skin recolours the finished frame (ui/skin.rs); the OOS skin leaves it as drawn
        self.skin_pass(cv);
        self.draw_pointer(cv);
        self.frames += 1;
    }

    /// The traces of a plot window: drawn now on one canvas, a job for the GPU with layers. `zoom`: the zoom
    /// window's maps.
    /// `all_sources`: every channel that is on (the zoom overview), else only those of the window being drawn.
    fn traces(&self, l: &mut Layers, area: Rect, zoom: bool, all_sources: bool) {
        let (ph, vmax, vmax_cg) =
            if zoom { (&self.phz, &self.vmaxz, self.vmaxz_cg) } else { (&self.ph, &self.vmax, self.vmax_cg) };
        if l.over.is_none() {
            return self.draw_traces(l.under, area, ph, vmax, vmax_cg, all_sources);
        }
        let show: [bool; NCH] = std::array::from_fn(|c| self.m.ch[c].on && (all_sources || self.shows(c)));
        if !show.iter().any(|&s| s) {
            return;
        }
        let graded = self.m.color_grade;
        let (mut alpha, mut colours, mut shift) = (Vec::new(), Vec::new(), [0u32; NCH]);
        if graded {
            let (lut, sh) = self.grade_lut(vmax_cg);
            colours = lut.to_vec();
            shift[0] = sh;
        } else {
            alpha = vec![0u8; NCH * 1024];
            for c in 0..NCH {
                let (lut, sh, least) = self.trace_lut(vmax[c]);
                for (i, a) in alpha[c * 1024..(c + 1) * 1024].iter_mut().enumerate() {
                    *a = lut[i].max(least) as u8;
                }
                shift[c] = sh;
            }
        }
        l.jobs.push(TraceJob { area, zoom, graded, show, alpha, colours, shift, col: CH_COLORS });
    }

    /// The screen once more, laid out for the size of `cv` (Display > HDMI = Layout): the same settings, menus and
    /// records, sharp at the monitor's resolution. The traces come from the panel's phosphor maps, stretched to the
    /// larger plot (`draw_traces`). The panel's layout is as it was afterwards.
    pub fn render_view(&mut self, cv: &mut Canvas) {
        self.render_view_layers(&mut Layers { under: cv, over: None, jobs: Vec::new() });
    }

    /// `render_view` into layers (System > Graphics = GPU): the traces become jobs at the monitor's size.
    pub fn render_view_layers(&mut self, l: &mut Layers) {
        // Display > Monitor view: another view than the panel's
        if self.monitor_view().is_some() {
            self.with_monitor_view(|u| u.render_view_layers_as_is(l));
            return;
        }
        self.render_view_layers_as_is(l);
    }

    /// `render_view_layers` of the view in place.
    fn render_view_layers_as_is(&mut self, l: &mut Layers) {
        let keep = (
            self.scr,
            self.grid,
            std::mem::take(&mut self.wins),
            self.area,
            self.cur_win,
            self.frames,
            self.hdmi_snap_want,
        );
        // what Studio keeps of a frame (`studio_frame_start`): the lines of its badge row, counted for the width of
        // the pass, and where it drew badges and labels, for the touch. The panel's stay the panel's. With the
        // monitor's count of lines the plot of the panel changed its height from one frame to the next, and its
        // traces were cleared each time (seen on a unit: HDMI Layout, twelve badges that fill two lines on the panel
        // and one on a monitor of 1920, and the traces missing in most pictures of the panel)
        let studio = (
            self.studio.badge_wrap,
            std::mem::take(self.studio.lane_badges.get_mut()),
            std::mem::take(self.studio.dec_labels.get_mut()),
        );
        self.scr = Screen { w: l.under.w, h: l.under.h };
        (self.view_pass, self.hdmi_snap_want) = (true, false);
        self.render_layers(l);
        self.view_pass = false;
        (self.scr, self.grid, self.wins, self.area, self.cur_win, self.frames, self.hdmi_snap_want) = keep;
        self.studio.badge_wrap = studio.0;
        (*self.studio.lane_badges.get_mut(), *self.studio.dec_labels.get_mut()) = (studio.1, studio.2);
    }

    /// Graticule in the classic scope style: dotted division lines (Full), a solid frame (Full, Light) and solid
    /// centre axes, with ticks every fifth of a division on the axes (both sides) and on the frame (inwards).
    fn draw_grid(&self, cv: &mut Canvas, g: Rect) {
        let dx = g.w as f32 / HDIV;
        let dy = g.h as f32 / VDIV;
        let (x1, y1) = (g.x + g.w - 1, g.y + g.h - 1);
        let xs = |k: i32, per: f32| (g.x + (k as f32 * dx / per).round() as i32).min(x1);
        let ys = |k: i32, per: f32| (g.y + (k as f32 * dy / per).round() as i32).min(y1);
        let cx = g.x + (g.w as f32 / 2.0).round() as i32;
        let cy = g.y + (g.h as f32 / 2.0).round() as i32;
        // Display > Grid brightness: towards the background
        let k = self.m.grid_bright.clamp(10, 100) * 255 / 100;
        let (dot, line) = (mix(BG, GRID_DOT, k), mix(BG, GRID_FRAME, k));
        if self.m.grid == Grid::Full {
            // dots about every 5 px along each division line (a whole number of dots per division, so they line
            // up with the ticks)
            let (nx, ny) = ((dx / 5.0).round().max(2.0), (dy / 5.0).round().max(2.0));
            for i in 1..HDIV as i32 {
                let x = xs(i, 1.0);
                if x != cx {
                    for k in 0..=(VDIV * ny) as i32 {
                        cv.pixel(x, ys(k, ny), dot, 255);
                    }
                }
            }
            for j in 1..VDIV as i32 {
                let y = ys(j, 1.0);
                if y != cy {
                    for k in 0..=(HDIV * nx) as i32 {
                        cv.pixel(xs(k, nx), y, dot, 255);
                    }
                }
            }
        }
        if self.m.grid != Grid::None && self.m.grid_frame {
            // frame with inward ticks every fifth of a division
            cv.rect(Rect::new(g.x, g.y, g.w, g.h), line);
            for k in 1..(HDIV as i32 * 5) {
                let (x, len) = (xs(k, 5.0), if k % 5 == 0 { 8 } else { 4 });
                cv.vline(x, g.y, g.y + len, line);
                cv.vline(x, y1 - len, y1, line);
            }
            for k in 1..(VDIV as i32 * 5) {
                let (y, len) = (ys(k, 5.0), if k % 5 == 0 { 8 } else { 4 });
                cv.hline(g.x, g.x + len, y, line);
                cv.hline(x1 - len, x1, y, line);
            }
        }
        // centre axes with ticks on both sides every fifth of a division, longer at the divisions
        cv.vline(cx, g.y, y1, line);
        cv.hline(g.x, x1, cy, line);
        for k in 0..=(HDIV as i32 * 5) {
            let len = if k % 5 == 0 { 6 } else { 3 };
            cv.vline(xs(k, 5.0), cy - len, cy + len, line);
        }
        for k in 0..=(VDIV as i32 * 5) {
            let len = if k % 5 == 0 { 6 } else { 3 };
            cv.hline(cx - len, cx + len, ys(k, 5.0), line);
        }
    }

    /// (V/div, offset, unit, colour) of the ref that labels the volt axis when no channel is shown here: the selected
    /// ref, else the first one this window holds.
    pub(super) fn ref_axis_source(&self) -> Option<(f32, f32, &'static str, Rgb)> {
        let here: Vec<usize> =
            self.shown_refs().into_iter().filter(|&i| self.workspace_source_visible_here(WaveId::Ref(i))).collect();
        let i = here
            .iter()
            .copied()
            .find(|&i| self.selected_workspace_source() == Some(WaveId::Ref(i)))
            .or(here.first().copied())?;
        let r = self.refs[i].as_ref()?;
        Some((r.vpd, r.offset, "V", self.wave_color(WaveId::Ref(i))))
    }

    /// Stacked lanes: where one ends and the next begins, and whose it is. A bar in the source's colour runs down
    /// the left edge of each lane, and a solid line fills the gap between two lanes.
    fn draw_lane_chrome(&self, cv: &mut Canvas) {
        if !self.stacked() || self.wins.len() < 2 { return; }
        let sources = self.trace_sources();
        for (i, lane) in self.wins.iter().enumerate() {
            let col = match sources.get(i) {
                Some(workspace::Trace::Channel(c)) => CH_COLORS[*c],
                Some(workspace::Trace::Reference(r)) => self.wave_color(WaveId::Ref(*r)),
                Some(workspace::Trace::Math(k)) => MATH_COLORS[*k % MATH_COLORS.len()],
                None => continue,
            };
            cv.fill(Rect::new(lane.x, lane.y, 3, lane.h), mix(BG, col, 190));
            if let Some(next) = self.wins.get(i + 1) {
                let gap = (next.y - lane.bottom()).max(1);
                cv.fill(Rect::new(lane.x, lane.bottom(), lane.w, gap), mix(BG, TEXT, 90));
            }
        }
    }

    /// The ADC converts 7500 codes per division around the middle of the screen, so its 16-bit range ends 4.37
    /// divisions above and below it. A channel whose record reaches either end is clipping: what it shows there is
    /// the limit of the converter, not the signal, and every amplitude measured from it is too small.
    pub const CLIP_DIVS: f32 = 4.35;

    /// Channel `c` is on and its record reaches the end of the ADC range.
    pub fn channel_clipping(&self, c: usize) -> bool {
        let Some(ch) = self.m.ch.get(c).filter(|ch| ch.on) else { return false };
        if self.record_mode() { return false; }
        let a = &self.ana[c];
        if !a.valid { return false; }
        let vpd = ch.volts_per_div().max(1e-12) as f64;
        let (top, bottom) = ((a.vmax + ch.offset as f64) / vpd, (a.vmin + ch.offset as f64) / vpd);
        top >= Self::CLIP_DIVS as f64 || bottom <= -(Self::CLIP_DIVS as f64)
    }

    /// The channels that clip now, for the warning in the plot.
    pub fn clipping_channels(&self) -> Vec<usize> {
        (0..n_ch()).filter(|&c| self.channel_clipping(c)).collect()
    }

    /// The math channel whose lane holds the point (stacked lanes only).
    pub(crate) fn math_lane_at(&self, x: i32, y: i32) -> Option<usize> {
        if !self.stacked() { return None; }
        let lane = self.wins.iter().position(|w| w.contains(x, y))?;
        match self.trace_sources().get(lane) { Some(workspace::Trace::Math(k)) if self.m.math_ch(*k).op != MathOp::Fft => Some(*k), _ => None }
    }

    /// The scale of the math channel drawn in this lane: volts per division, offset in volts, unit, colour.
    fn math_axis_source(&self) -> Option<(f32, f32, &'static str, Rgb)> {
        let k = (0..NMATH).find(|&k| self.m.math_shown(k) && self.math_win_of(k) == self.cur_win)?;
        let c = self.m.math_ch(k);
        if c.op == MathOp::Fft { return None; }
        let vpd = *math_scales().get(c.scale)? as f32;
        Some((vpd, c.offset * vpd, c.op.unit(), MATH_COLORS[k % MATH_COLORS.len()]))
    }

    /// Axis labels inside the plot: volts of the active channel along the left edge (not on the top and bottom
    /// lines, where the status line and the time labels are), times along the bottom edge. A volt label is left out
    /// rather than drawn over another label, the status line, the time labels, the mask pill or the decode band; it
    /// keeps clear of the channel markers at the left edge.
    fn draw_axes(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let a = self.area;
        let c = if self.m.ch[self.m.active].on && self.shows(self.m.active) {
            Some(self.m.active)
        } else {
            (0..NCH).find(|&i| self.m.ch[i].on && self.shows(i))
        };
        // with no channel to label, the shown ref gives the axis (R1 in the lane or window that holds it)
        let axis = match c {
            Some(c) => {
                let ch = &self.m.ch[c];
                Some((ch.volts_per_div(), ch.offset, ch.unit.name(), CH_COLORS[c]))
            }
            None => self.ref_axis_source().or_else(|| if self.stacked() { self.math_axis_source() } else { None }),
        };
        // a lane is one source with its own scale: its labels are not given up for a status line or time labels
        // that are not in it, and its top and bottom values are always there
        let lanes = self.stacked() && self.wins.len() > 1;
        let last_lane = self.cur_win + 1 >= self.wins.len() || self.studio_lane_has_own_time();
        if let Some((base_vpd, base_offset, unit, color)) = axis {
            let (factor, center) = self.zoom_vertical_transform(a);
            let vpd = base_vpd / factor;
            let offset = base_offset - center * base_vpd;
            let span = (VDIV / 2.0 * vpd).abs() + offset.abs();
            let mut taken: Vec<Rect> = Vec::new();
            if !lanes || (self.cur_win == 0 && self.m.layout != Layout::Studio) { taken.push(Rect::new(a.x, a.y, a.w, 20)); }
            if !lanes || last_lane { taken.push(Rect::new(a.x, a.y + a.h - 22, a.w, 22)); }
            if self.m.mask_on && self.mask.is_some() && self.shows(self.m.mask_src) {
                taken.push(Rect::new(a.x, a.y + 22, a.w, 28));
            }
            // the height of the bands of this window, their bit rows with them
            let bands: i32 = (0..DECODERS)
                .filter(|&k| self.dec_shown(k) && self.dec_band_win(k) == Some(self.cur_win))
                .map(|k| self.dec_band_h(&self.m.dec_view(k)))
                .sum();
            if bands > 0 {
                taken.push(Rect::new(a.x, a.y + a.h - (self.dec_foot() - 4) - bands, a.w, bands));
            }
            // the channel ground markers at the left edge: a label on their line is left out (as on the stock
            // screen, where the marker takes the place of the "0" label)
            for k in 0..NCH {
                if self.m.ch[k].on && self.shows(k) {
                    let y = self.y_of_volts(k, 0.0).clamp(a.y + (a.h / 2).min(9), a.y + a.h - (a.h / 2).min(9));
                    taken.push(Rect::new(a.x, y - 9, MARK_W + 2, 18));
                }
            }
            // Studio: every source has a handle there, the references and the math channels too
            if self.m.layout == Layout::Studio {
                for (_, r) in self.studio_handle_rects() {
                    taken.push(Rect::new(a.x, r.y, r.right() - a.x + 2, r.h));
                }
            }
            // what a lane has at its right edge: the corner buttons with their words, and the trigger tag with its value
            let mut right_obstacles: Vec<Rect> = Vec::new();
            // the trigger tag with its value: a scale label at its height would say the level a second time beside it
            let mut trigger_tag: Option<Rect> = None;
            if lanes {
                if self.m.layout == Layout::Studio {
                    for b in [self.full_btn().0, self.zoom_btn().0] { right_obstacles.push(Rect::new(b.x - 62, b.y - 2, b.w + 66, b.h + 4)); }
                }
                let ts = self.m.trig_src;
                if self.m.trig_ref.is_none() && self.m.ch[ts].on && self.shows(ts) {
                    let y = self.y_of_volts(ts, self.trig_level_shown()).clamp(a.y + (a.h / 2).min(9), a.y + a.h - (a.h / 2).min(9));
                    let x1 = self.trigger_tag_x(a.x + a.w - MARK_W, y);
                    let left = x1 - 8 - f.sb.width(&si_short(self.trig_level_shown() as f64, self.m.unit(ts)));
                    trigger_tag = Some(Rect::new(left, y - 11, a.x + a.w - left, 22));
                }
            }
            // labels right at the left edge of the graticule, centred on their division line
            let x = a.x + 6;
            // a lane: the values of its top and bottom edge first (its full scale), then the centre, the quarters and
            // the rest for as long as they fit between them
            let order: Vec<i32> = if lanes { vec![0, VDIV as i32, 4, 2, 6, 1, 3, 5, 7] } else { (1..VDIV as i32).collect() };
            for j in order {
                let line = a.y + (j as f32 * a.h as f32 / VDIV).round() as i32;
                // an edge label sits just inside its edge
                let y = if j == 0 { a.y + f.s.size - 2 } else if j == VDIV as i32 { a.y + a.h - 6 - if last_lane { 26 } else { 0 } } else { line };
                let v = (VDIV / 2.0 - j as f32) * vpd - offset;
                let s = axis_label(v, vpd, offset, span, unit);
                // an offset that is not on the steps of the scale (a fitted math channel) would spell every digit:
                // three significant ones say as much
                let s = if s.chars().count() > 9 { si_short(v as f64, unit) } else { s };
                let w = f.s.width(&s);
                // a lane has its scale at the right edge; what already sits there (the corner buttons, the trigger tag)
                // moves a label to its left instead of losing it
                let mut x = if lanes { a.x + a.w - 6 - w } else { x };
                let mut r = Rect::new(x - 3, y + 4 - f.s.size, w + 6, f.s.size + 4);
                if lanes {
                    if trigger_tag.is_some_and(|t| overlaps(t, r)) { continue; }
                    for _ in 0..2 {
                        if let Some(o) = right_obstacles.iter().find(|o| overlaps(**o, r)) {
                            x = o.x - 6 - w;
                            r = Rect::new(x - 3, y + 4 - f.s.size, w + 6, f.s.size + 4);
                        }
                    }
                }
                if taken.iter().any(|t| overlaps(*t, r)) {
                    continue;
                }
                taken.push(r);
                backed_text(cv, &f.s, x, y + 4, &s, scale(color, 210));
            }
        }
        if self.stacked() && self.cur_win + 1 < self.wins.len() && !self.studio_lane_has_own_time() {
            return;
        }
        let (tdiv, delay) = self.view();
        let span = (HDIV / 2.0 * tdiv).abs() + delay.abs();
        // a narrow window (side by side) labels every second division, the centre always
        let every = if a.w < 700 { 2 } else { 1 };
        if self.m.layout == Layout::Studio {
            // round times at any delay (a zoom window sits where it is put): the labels are whole steps from the
            // trigger, not the window's centre plus whole divisions
            let mut step = tdiv as f64 * every as f64;
            // a lane with a time/div of its own (a fitted record) is not on the 1-2-5 ladder: its labels are
            if self.studio_lane_has_own_time() { step = nice_step_below(step); }
            let mut free = i32::MIN;
            for (k, t) in studio_time_ticks_at(tdiv, delay, step) {
                let x = a.x + (((t - delay as f64) / tdiv as f64 + HDIV as f64 / 2.0) * a.w as f64 / HDIV as f64).round() as i32;
                let zero = k == 0;
                let s = if zero { "0 s".to_string() } else { axis_label64(t, step as f32, 0.0, span, "s") };
                let w = f.s.width(&s);
                let tx = (x - w / 2).clamp(a.x + 6, a.x + a.w - 6 - w);
                // a label pushed against an edge must not run into its neighbour
                if tx < free + 8 { continue; }
                free = tx + w;
                backed_text(cv, &f.s, tx, a.y + a.h - 6, &s, if zero { ACCENT } else { AXIS_TEXT });
            }
            return;
        }
        let mut free = i32::MIN;
        for i in 0..=HDIV as i32 {
            if (i - HDIV as i32 / 2).rem_euclid(every) != 0 {
                continue;
            }
            let x = a.x + (i as f32 * a.w as f32 / HDIV).round() as i32;
            let t = delay as f64 + (i as f64 - HDIV as f64 / 2.0) * tdiv as f64;
            let zero = t.abs() < tdiv as f64 * 1e-3;
            let s = if zero { "0 s".to_string() } else { axis_label64(t, tdiv, delay, span, "s") };
            let w = f.s.width(&s);
            let tx = (x - w / 2).clamp(a.x + 6, a.x + a.w - 6 - w);
            if tx < free + 8 {
                continue;
            }
            free = tx + w;
            backed_text(cv, &f.s, tx, a.y + a.h - 6, &s, if zero { ACCENT } else { AXIS_TEXT });
        }
    }

    fn draw_traces(
        &self,
        cv: &mut Canvas,
        area: Rect,
        ph: &Phosphor,
        vmaxs: &[u32; NCH],
        vmax_cg: u32,
        all_sources: bool,
    ) {
        if self.m.color_grade {
            return self.draw_graded(cv, area, ph, vmax_cg, all_sources);
        }
        let (w, h) = (ph.w, ph.h);
        for c in (0..NCH).rev() {
            if !self.m.ch[c].on || (!all_sources && !self.shows(c)) {
                continue;
            }
            let col = CH_COLORS[c];
            let (cr, cg, cb) = ((col >> 16) & 255, (col >> 8) & 255, col & 255);
            let (lut, shift, least) = self.trace_lut(vmaxs[c]);
            let map = &ph.map[c];
            let put = |p: &mut u32, v: u16| {
                if v == 0 {
                    return;
                }
                let a = lut[((v as usize) >> shift).min(1023)].max(least);
                // additive with saturation, but channels on top of each other mix towards the newer one
                let d = *p;
                let r = (((d >> 16) & 255) * (255 - a) / 255 + cr * a / 255).min(255);
                let g = (((d >> 8) & 255) * (255 - a) / 255 + cg * a / 255).min(255);
                let b = ((d & 255) * (255 - a) / 255 + cb * a / 255).min(255);
                *p = rgb(r, g, b);
            };
            match Self::stretch(area, w, h) {
                None => {
                    for y in 0..h {
                        let row = &map[y * w..(y + 1) * w];
                        let o = ((area.y + y as i32) * cv.w + area.x) as usize;
                        for (x, &v) in row.iter().enumerate() {
                            put(&mut cv.px[o + x], v);
                        }
                    }
                }
                Some((xs, ys)) => {
                    for (y, &sy) in ys.iter().enumerate() {
                        let o = ((area.y + y as i32) * cv.w + area.x) as usize;
                        for (x, &sx) in xs.iter().enumerate() {
                            put(&mut cv.px[o + x], map[sy * w + sx]);
                        }
                    }
                }
            }
        }
    }

    /// One part of `step` into `times.parts` (`:SYSTem:STATe?`).
    pub(super) fn prof_add(&mut self, i: usize, d: std::time::Duration) {
        self.times.parts[i] += 0.05 * (d.as_secs_f32() * 1e3 - self.times.parts[i]);
    }

    /// Brightness of a trace pixel by its count (`draw_traces`): floor + sqrt(hits / brightest), the intensity
    /// setting raises floor and gain. The count shifted right by the second value indexes the table; a pixel that
    /// was hit at all gets at least the third.
    /// The counts the picture is graded against (`picture_ref`), per channel and for colour grade; whether the main and
    /// the zoom map are near saturation.
    pub(super) fn grade_refs(&mut self) -> (bool, bool) {
        let n = self.pic_frames;
        let (mut full, mut fullz) = (false, false);
        for c in 0..NCH {
            if self.m.ch[c].on {
                let mx = self.ph.map[c].iter().copied().max().unwrap_or(0) as u32;
                self.vmax[c] = picture_ref(self.vmax[c], &mut self.per_frame[c], mx, n);
                full |= mx > 0xc000;
                if self.m.zoom {
                    let mz = self.phz.map[c].iter().copied().max().unwrap_or(0) as u32;
                    self.vmaxz[c] = picture_ref(self.vmaxz[c], &mut self.per_framez[c], mz, n);
                    fullz |= mz > 0xc000;
                }
            }
        }
        if self.m.color_grade {
            let on: [bool; NCH] = std::array::from_fn(|c| self.m.ch[c].on);
            self.vmax_cg = picture_ref(self.vmax_cg, &mut self.per_frame_cg[0], self.ph.merged_max(on), n);
            if self.m.zoom {
                self.vmaxz_cg = picture_ref(self.vmaxz_cg, &mut self.per_frame_cg[1], self.phz.merged_max(on), n);
            }
        }
        (full, fullz)
    }

    pub(super) fn trace_lut(&self, vmax: u32) -> ([u32; 1024], u32, u32) {
        let inten = self.m.intensity as f32 / 100.0;
        let floor = 30.0 + 140.0 * inten;
        let gain = 0.35 + 1.3 * inten;
        let vmax = vmax.max(1);
        let mut shift = 0;
        while (vmax >> shift) > 1023 {
            shift += 1;
        }
        let top = ((vmax >> shift).max(1)) as f32;
        let mut lut = [0u32; 1024];
        for (i, l) in lut.iter_mut().enumerate() {
            let v = (i as f32 + 0.5) / top;
            *l = if i == 0 && shift == 0 {
                0
            } else {
                (floor + (255.0 - floor) * (v * gain).min(1.0).sqrt()).min(255.0) as u32
            };
        }
        (lut, shift, floor as u32 / 2)
    }

    /// Colour of a colour-graded pixel by the summed count (`draw_graded`), and the shift of the index.
    fn grade_lut(&self, vmax: u32) -> ([u32; 1024], u32) {
        let gain = 0.35 + 1.3 * self.m.intensity as f32 / 100.0;
        let vmax = vmax.max(1);
        let mut shift = 0;
        while (vmax >> shift) > 1023 {
            shift += 1;
        }
        let top = (vmax >> shift).max(1) as f32;
        let pal = heat_palette();
        let mut lut = [0u32; 1024];
        for (i, l) in lut.iter_mut().enumerate() {
            let v = ((i as f32 + 0.5) / top * gain).min(1.0).sqrt();
            *l = pal[((v * 255.0) as usize).min(255)];
        }
        (lut, shift)
    }

    /// A plot of another size than its phosphor map (the HDMI layout): for each of its columns and rows the cell of
    /// the map it shows (the nearest). None: the same size, one pixel per cell.
    fn stretch(area: Rect, w: usize, h: usize) -> Option<(Vec<usize>, Vec<usize>)> {
        let (aw, ah) = (area.w.max(0) as usize, area.h.max(0) as usize);
        if (aw, ah) == (w, h) || w == 0 || h == 0 {
            return None;
        }
        Some(((0..aw).map(|x| x * w / aw).collect(), (0..ah).map(|y| y * h / ah).collect()))
    }

    /// Colour-graded persistence: the hit maps of all enabled channels are added up and every pixel is coloured by its
    /// density relative to the densest pixel (blue = rare .. red = frequent). Per frame only a 1024-entry LUT is
    /// built; per pixel it is an add per channel and one table look-up.
    pub(super) fn draw_graded(&self, cv: &mut Canvas, area: Rect, ph: &Phosphor, vmax: u32, all_sources: bool) {
        let maps: Vec<&[u16]> = (0..NCH)
            .filter(|&c| self.m.ch[c].on && (all_sources || self.shows(c)))
            .map(|c| ph.map[c].as_slice())
            .collect();
        if maps.is_empty() {
            return;
        }
        let (lut, shift) = self.grade_lut(vmax);
        let w = ph.w;
        if let Some((xs, ys)) = Self::stretch(area, w, ph.h) {
            for (y, &sy) in ys.iter().enumerate() {
                let o = ((area.y + y as i32) * cv.w + area.x) as usize;
                for (x, &sx) in xs.iter().enumerate() {
                    let s: u32 = maps.iter().map(|m| m[sy * w + sx] as u32).sum();
                    if s != 0 {
                        cv.px[o + x] = lut[((s >> shift) as usize).min(1023)];
                    }
                }
            }
            return;
        }
        for y in 0..ph.h {
            let o = ((area.y + y as i32) * cv.w + area.x) as usize;
            let row = y * w;
            for (x, d) in cv.px[o..o + w].iter_mut().enumerate() {
                let s: u32 = maps.iter().map(|m| m[row + x] as u32).sum();
                if s != 0 {
                    *d = lut[((s >> shift) as usize).min(1023)];
                }
            }
        }
    }
}

/// The count a trace pixel is graded against (full brightness), from the brightest pixel `mx` of the picture. It
/// follows a higher `mx` at once and a lower one slowly (`prev`, no flicker). Minimum persistence (`frames` > 0, the
/// frames in the picture): the picture holds as many frames as arrived since the last screen refresh, in batches, so
/// its counts rise and fall with that number. Graded against the per-frame maximum (`per_frame`, followed the same
/// way) times `frames`, equal frames give equal brightness whatever their number. Without this a refresh with one
/// frame after one with several came out dim: the trace pumped at the batch rate (docs/fork/trigger-lockin.md).
fn picture_ref(prev: u32, per_frame: &mut u32, mx: u32, frames: u32) -> u32 {
    let follow = |v: u32, mx: u32| if mx >= v { mx } else { (v * 7 + mx) / 8 }.max(1);
    if frames == 0 {
        return follow(prev, mx);
    }
    *per_frame = follow(*per_frame, mx.div_ceil(frames));
    per_frame.saturating_mul(frames).max(1)
}
