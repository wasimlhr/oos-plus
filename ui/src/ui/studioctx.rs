// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: the context menu of the plot. A long press (or a right click) on the lanes opens a popover at the press
//! point: two columns of rows with an icon and a label, the view at the left and what acts on the lane under the
//! finger at the right. The state of Overlay / Stacked is shown; the actions that need a place (zoom here, trigger
//! level here, cursor here) take the place of the finger, and the ones that need a lane name it. A row is 40 px, a
//! finger's height. What the bars have one tap away (Run, Single, the trigger sheet, the sources) is not repeated
//! here. The menu is kept on the screen and closes on a tap outside.
use super::studio::{icon_cursors, icon_expand};
use super::workspace::Action;
use super::*;

/// Width of a column, height of a row and of the title.
const CTX_COL: i32 = 232;
const CTX_ROW: i32 = 40;
const CTX_HEAD: i32 = 30;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum CtxRow {
    Overlay,
    Stacked,
    FitLanes,
    Fit,
    ZoomHere,
    ZoomBox,
    ZoomReset,
    ZoomClose,
    Autoset,
    TrigOn(WaveId),
    LevelHere(WaveId),
    MeasOn(WaveId),
    CursorHere,
    /// a callout on the lane, at the time under the press
    NoteHere(WaveId),
    /// every view in tabs of one group, or back from it
    Layout,
}

/// Where the menu was opened and the lane under that point.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct Ctx {
    pub x: i32,
    pub y: i32,
    pub lane: Option<WaveId>,
}

impl Ui {
    /// The lane under a point: in stacked lanes the one whose area it is, otherwise the selected source.
    pub(super) fn ctx_lane(&self, x: i32, y: i32) -> Option<WaveId> {
        let n = n_ch();
        if self.wins.len() > 1 {
            if let Some(c) = (0..n).find(|&c| self.m.ch[c].on && self.ch_area(c).contains(x, y)) { return Some(WaveId::Ch(c)); }
        }
        let sel = if self.record_mode() { WaveId::Ref(self.ref_sel) } else { WaveId::Ch(self.m.active) };
        match sel { WaveId::Ch(c) if c < n && self.m.ch[c].on => Some(sel), WaveId::Ref(_) => Some(sel), _ => (0..n).find(|&c| self.m.ch[c].on).map(WaveId::Ch) }
    }

    pub(super) fn studio_ctx_open(&mut self, x: i32, y: i32) {
        if self.menu_open { self.close_menu(); }
        self.studio.view_menu = false;
        self.studio.add_menu = false;
        self.studio.popup = None;
        let lane = self.ctx_lane(x, y);
        self.studio.ctx = Some(Ctx { x, y, lane });
    }

    /// The two columns of rows: the view, and what acts on the lane under the press (which the rows name).
    fn ctx_columns(&self, lane: Option<WaveId>) -> [Vec<(CtxRow, String)>; 2] {
        let name = |id: WaveId| self.wave_name(id);
        let mut view = vec![(CtxRow::Overlay, "Overlay traces".to_string()), (CtxRow::Stacked, "Stacked traces".to_string()),
            (CtxRow::FitLanes, "Fit all lanes".to_string()), (CtxRow::Fit, "Fit record".to_string()),
            (CtxRow::ZoomHere, "Zoom here".to_string()), (CtxRow::ZoomBox, "Zoom box".to_string())];
        if self.m.zoom {
            view.push((CtxRow::ZoomReset, "Reset zoom".to_string()));
            view.push((CtxRow::ZoomClose, "Close zoom".to_string()));
        }
        let mut here = Vec::new();
        if let Some(id) = lane {
            here.push((CtxRow::TrigOn(id), format!("Trigger on {}", name(id))));
            here.push((CtxRow::LevelHere(id), "Set level here".to_string()));
            here.push((CtxRow::MeasOn(id), format!("Measurement on {}", name(id))));
        }
        here.push((CtxRow::CursorHere, "Cursor here".to_string()));
        if let Some(id) = lane { here.push((CtxRow::NoteHere(id), "Add note here".to_string())); }
        here.push((CtxRow::Autoset, "Autoset".to_string()));
        here.push((CtxRow::Layout, if self.dock_full_state() { "Restore layout" } else { "Maximise view" }.to_string()));
        [view, here]
    }

    /// The popover: anchored at the press, flipped and clamped to stay on the screen.
    pub(super) fn studio_ctx_rect(&self) -> Option<Rect> {
        let c = self.studio.ctx?;
        let cols = self.ctx_columns(c.lane);
        let rows = cols.iter().map(|x| x.len() as i32).max().unwrap_or(0);
        let w = 2 * CTX_COL + 12;
        let h = (CTX_HEAD + rows * CTX_ROW + 8).min(self.scr.h - 8);
        let x = if c.x + 8 + w <= self.scr.w - 4 { c.x + 8 } else { (c.x - 8 - w).clamp(4, (self.scr.w - w - 4).max(4)) };
        let y = c.y.min(self.scr.h - h - 4).max(4);
        Some(Rect::new(x, y, w, h))
    }

    pub(super) fn studio_ctx_rows(&self) -> Vec<(CtxRow, String, Rect)> {
        let (Some(p), Some(c)) = (self.studio_ctx_rect(), self.studio.ctx) else { return Vec::new() };
        let cols = self.ctx_columns(c.lane);
        // a tall list on a short screen shrinks its rows
        let rows = cols.iter().map(|x| x.len() as i32).max().unwrap_or(1).max(1);
        let row_h = ((p.h - CTX_HEAD - 8) / rows).min(CTX_ROW).max(24);
        let mut out = Vec::new();
        for (k, col) in cols.into_iter().enumerate() {
            let x = p.x + 4 + k as i32 * (CTX_COL + 4);
            for (i, (row, label)) in col.into_iter().enumerate() {
                out.push((row, label, Rect::new(x, p.y + CTX_HEAD + 2 + i as i32 * row_h, CTX_COL, row_h)));
            }
        }
        out
    }

    fn ctx_icon(&self, cv: &mut Canvas, row: CtxRow, cx: i32, cy: i32, c: Rgb) {
        let f = self.fonts();
        match row {
            CtxRow::Overlay => { cv.line(cx - 7, cy + 3, cx, cy - 4, c); cv.line(cx, cy - 4, cx + 7, cy + 3, c); cv.line(cx - 7, cy - 1, cx, cy + 5, c); }
            CtxRow::Stacked => { cv.hline(cx - 7, cx + 7, cy - 4, c); cv.hline(cx - 7, cx + 7, cy, c); cv.hline(cx - 7, cx + 7, cy + 4, c); }
            CtxRow::Fit => icon_expand(cv, cx, cy, c),
            CtxRow::FitLanes => { cv.vline(cx, cy - 7, cy + 7, c); for d in [-1, 1] { cv.line(cx, cy + 7 * d, cx - 4, cy + 3 * d, c); cv.line(cx, cy + 7 * d, cx + 4, cy + 3 * d, c); } }
            CtxRow::ZoomHere | CtxRow::ZoomReset => { cv.circle(cx - 2, cy - 2, 5, c); cv.circle(cx - 2, cy - 2, 3, BAR); cv.line(cx + 2, cy + 2, cx + 7, cy + 7, c); }
            CtxRow::ZoomBox => { cv.round_outline(Rect::new(cx - 7, cy - 5, 14, 10), 1, c); cv.fill(Rect::new(cx - 7, cy - 5, 4, 2), c); }
            CtxRow::ZoomClose => { cv.line(cx - 5, cy - 5, cx + 5, cy + 5, c); cv.line(cx - 5, cy + 5, cx + 5, cy - 5, c); }
            CtxRow::Autoset => cv.text_in(&f.sb, Rect::new(cx - 10, cy - 9, 20, 18), 1, "A", c),
            CtxRow::TrigOn(_) => { cv.hline(cx - 7, cx - 2, cy + 4, c); cv.vline(cx - 2, cy - 4, cy + 4, c); cv.hline(cx - 2, cx + 7, cy - 4, c); }
            CtxRow::LevelHere(_) => { cv.hline(cx - 8, cx + 8, cy, c); cv.triangle([(cx - 4, cy - 5), (cx + 4, cy - 5), (cx, cy - 1)], c); }
            CtxRow::MeasOn(_) => { for (i, h) in [6, 11, 8].into_iter().enumerate() { cv.fill(Rect::new(cx - 7 + i as i32 * 5, cy + 6 - h, 3, h), c); } }
            CtxRow::CursorHere => icon_cursors(cv, cx - 5, cy, c),
            CtxRow::NoteHere(_) => { cv.round_outline(Rect::new(cx - 11, cy - 6, 14, 10), 2, c); cv.line(cx - 4, cy + 4, cx - 8, cy + 8, c); }
            CtxRow::Layout => { cv.rect(Rect::new(cx - 6, cy - 6, 12, 12), c); cv.hline(cx - 6, cx + 5, cy - 3, c); }
        }
    }

    pub(super) fn draw_studio_ctx(&self, cv: &mut Canvas) {
        let (Some(p), Some(c)) = (self.studio_ctx_rect(), self.studio.ctx) else { return };
        let f = self.fonts();
        cv.round(p, 8, BTN_BORDER, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        let title = c.lane.map_or("Plot".to_string(), |id| self.wave_name(id));
        let tcol = c.lane.map_or(TEXT, |id| self.wave_color(id));
        cv.text(&f.sb, p.x + 12, p.y + 20, &title, tcol);
        cv.hline(p.x + 8, p.right() - 9, p.y + CTX_HEAD - 2, LINE);
        // a line between the two columns
        cv.vline(p.x + 4 + CTX_COL + 1, p.y + CTX_HEAD + 4, p.bottom() - 8, LINE);
        let rows = self.studio_ctx_rows();
        for (row, label, r) in rows.iter() {
            let on = match row { CtxRow::Overlay => !self.trace_stacked(), CtxRow::Stacked => self.trace_stacked(), CtxRow::ZoomHere => self.m.zoom, _ => false };
            self.ctx_icon(cv, *row, r.x + 16, r.y + r.h / 2, if on { ACCENT } else { TEXT });
            cv.set_clip(Rect::new(r.x + 32, r.y, r.w - 62, r.h));
            cv.text_in(&f.m, Rect::new(r.x + 34, r.y, r.w - 62, r.h), 0, label, TEXT);
            cv.no_clip();
            if matches!(row, CtxRow::Overlay | CtxRow::Stacked) && on {
                // the check of the mode in force
                let (cx, cy) = (r.right() - 16, r.y + r.h / 2);
                cv.line(cx - 5, cy, cx - 2, cy + 4, ACCENT);
                cv.line(cx - 2, cy + 4, cx + 5, cy - 4, ACCENT);
            }
        }
    }

    /// The volts of lane `c` at screen row `y`: the inverse of `y_of_volts`, by bisection (it is monotonic).
    fn ctx_volts_at(&self, c: usize, y: i32) -> f32 {
        let vpd = self.m.ch[c].volts_per_div().max(1e-9);
        let (mut lo, mut hi) = (-20.0 * vpd - self.m.ch[c].offset, 20.0 * vpd - self.m.ch[c].offset);
        let up = self.y_of_volts(c, hi) < self.y_of_volts(c, lo);
        for _ in 0..30 {
            let mid = (lo + hi) / 2.0;
            let below = self.y_of_volts(c, mid) > y;
            if below == up { hi = mid; } else { lo = mid; }
        }
        (lo + hi) / 2.0
    }

    fn ctx_do(&mut self, row: CtxRow, at: Ctx) {
        match row {
            CtxRow::Overlay => self.workspace_action(Action::Overlay),
            CtxRow::Stacked => self.workspace_action(Action::Stacked),
            CtxRow::Fit => self.workspace_action(Action::FitRecord),
            CtxRow::FitLanes => self.fit_all_lanes(),
            CtxRow::ZoomHere => {
                // the zoom window goes to the time under the finger
                let a = self.area;
                let (tdiv, delay) = self.view();
                let t = delay + (at.x - a.x - a.w / 2) as f32 / (a.w as f32 / HDIV) * tdiv;
                if !self.m.zoom { self.set_zoom(true); }
                self.m.zdelay = t;
                self.clamp_zoom();
                self.relayout();
            }
            CtxRow::ZoomBox => self.arm_zoom_area(),
            CtxRow::ZoomReset => self.workspace_action(Action::ZoomReset),
            CtxRow::ZoomClose => self.workspace_action(Action::ZoomClose),
            CtxRow::Autoset => self.workspace_action(Action::Autoset),
            CtxRow::TrigOn(id) => self.workspace_action(Action::Trigger(id)),
            CtxRow::LevelHere(id) => {
                if let WaveId::Ch(c) = id {
                    self.set_trigger_source(id, false);
                    let v = self.ctx_volts_at(c, at.y);
                    self.m.trig_level = v;
                    self.settings_changed();
                    self.relayout();
                }
            }
            CtxRow::MeasOn(id) => { self.meas_src = id; self.open_meas_list(); }
            CtxRow::CursorHere => self.studio_cursors_add(None),
            CtxRow::NoteHere(id) => self.callout_add(id, at.x),
            CtxRow::Layout => { if !self.dock_exit_full() { self.dock_maximise(super::dock::VKind::Wave); } }
        }
    }

    /// A tap while the menu is open: a row does its action, a tap outside closes it. Always taken.
    pub(super) fn studio_ctx_tap(&mut self, x: i32, y: i32) -> bool {
        let Some(c) = self.studio.ctx else { return false };
        let hit = self.studio_ctx_rows().into_iter().find(|(_, _, r)| r.contains(x, y)).map(|(row, _, _)| row);
        self.studio.ctx = None;
        if let Some(row) = hit { self.ctx_do(row, c); }
        true
    }
}
