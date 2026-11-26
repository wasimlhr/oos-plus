// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The measurements that are on, in pages (Measure > Selected), and bounded pages of the readouts on the screen.

use super::studio::MeasKey;
use super::*;

const BAND_PAGE: usize = 18;
const BAND_ROWS: i32 = 3;
/// Cells of the band a page shows while Prev / Next take the end of its last two rows.
const BAND_NAV_PAGE: usize = BAND_PAGE - 2;
/// Rows under the band of the stock readouts (Default): Prev / Next and "Measurements a-b of N".
const NAV_ROWS: i32 = 2;
/// Rows of the Selected page: two columns of this many.
const SEL_ROWS: usize = 8;

/// The pages shown (ADR 0062): of the readouts on the screen and of Measure > Selected; a page past the last shows the
/// last.
#[derive(Default)]
pub(super) struct Pages {
    pub readouts: usize,
    pub selected: usize,
}

impl Ui {
    fn selected_all(&self) -> Vec<Item> {
        (0..self.measurement_count()).map(|index| Item::MeasRemove(index as u16)).collect()
    }

    fn selected_pages(&self) -> usize {
        self.measurement_count().max(1).div_ceil(2 * SEL_ROWS)
    }

    /// The Remove buttons of the page shown.
    pub(super) fn measurement_catalog(&self) -> Vec<Item> {
        let size = 2 * SEL_ROWS;
        self.selected_all()
            .into_iter()
            .skip(self.meas_pages.selected.min(self.selected_pages() - 1) * size)
            .take(size)
            .collect()
    }

    /// The box of the Selected page (as the measurement page) and its foot: Prev, Next, Clear all.
    fn selected_frame(&self) -> (Rect, Rect, Rect) {
        let r = Rect::new(8, TOP.h + 4, W - 16, self.frame().bottom.y - TOP.h - 8);
        let y = r.y + r.h - 48;
        (r, Rect::new(r.x + 12, y, 96, 40), Rect::new(r.x + 116, y, 96, 40))
    }

    /// Selected: every measurement that is on, two columns of "Remove <name>" with Set beside the standard ones, and
    /// Clear all at the foot.
    pub(super) fn measurement_chooser_layout(&self) -> Dlg {
        let (r, _, _) = self.selected_frame();
        let close = Rect::new(r.x + r.w - 56, r.y + 6, 44, DLG_CTRL);
        let col_w = (r.w - 24 - 12) / 2;
        let mut rows = Vec::new();
        for (i, item) in self.measurement_catalog().into_iter().enumerate() {
            let Item::MeasRemove(index) = item else { continue };
            let x = r.x + 12 + (i / SEL_ROWS) as i32 * (col_w + 12);
            let y = r.y + DLG_HEAD + 4 + (i % SEL_ROWS) as i32 * DLG_ROW;
            let standard = (index as usize) < self.m.meas.len();
            let w = if standard { col_w - 82 } else { col_w };
            rows.push(Row {
                it: item,
                label: Rect::new(0, 0, 0, 0),
                ctrl: Rect::new(x, y, w, DLG_CTRL),
                ctl: Ctl::Button,
            });
            if standard {
                rows.push(Row {
                    it: Item::MeasCfg(index),
                    label: Rect::new(0, 0, 0, 0),
                    ctrl: Rect::new(x + w + 4, y, 78, DLG_CTRL),
                    ctl: Ctl::Button,
                });
            }
        }
        if self.measurement_count() > 0 {
            rows.push(Row {
                it: Item::MeasClear,
                label: Rect::new(0, 0, 0, 0),
                ctrl: Rect::new(r.x + r.w - 12 - 160, r.y + r.h - 48, 160, DLG_CTRL),
                ctl: Ctl::Button,
            });
        }
        let back = self.parent.map(|_| Rect::new(close.x - 86, close.y, 78, DLG_CTRL));
        Dlg { r, close, back, rows, heads: Vec::new(), bar: None }
    }

    pub(super) fn draw_measurement_groups(&self, cv: &mut Canvas) {
        if self.menu != MenuId::MeasSelected {
            return;
        }
        let (r, prev, next) = self.selected_frame();
        let f = self.fonts();
        if self.selected_pages() > 1 {
            let page = self.meas_pages.selected.min(self.selected_pages() - 1);
            for (b, label) in [(prev, "Prev"), (next, "Next")] {
                btn(cv, b, BTN, BTN_BORDER);
                cv.text_in(&f.sb, b, 1, label, TEXT);
            }
            cv.text_in(
                &f.s,
                Rect::new(next.x + next.w + 12, next.y, 120, next.h),
                0,
                &format!("Page {} / {}", page + 1, self.selected_pages()),
                DIM,
            );
        }
        if self.measurement_count() == 0 {
            cv.text(&f.mb, r.x + 24, r.y + 100, "No measurements are on.", DIM);
            cv.text(&f.s, r.x + 24, r.y + 126, "Back to the measurement page, and tap one to add it.", DIM);
        }
    }

    pub(super) fn measurement_group_tap(&mut self, x: i32, y: i32) -> bool {
        if !(self.menu_open && self.menu == MenuId::MeasSelected) {
            return false;
        }
        let pages = self.selected_pages();
        if pages > 1 {
            let (_, prev, next) = self.selected_frame();
            if prev.contains(x, y) || next.contains(x, y) {
                self.meas_pages.selected = if prev.contains(x, y) {
                    self.meas_pages.selected.min(pages - 1).saturating_sub(1)
                } else {
                    (self.meas_pages.selected + 1).min(pages - 1)
                };
                return true;
            }
        }
        false
    }

    fn raw_strip_rows(&self) -> i32 {
        self.meas_groups().iter().map(|&source| self.meas_strip_rows(source)).sum()
    }

    pub(super) fn paged_strip(&self) -> bool {
        if self.m.strip_dock == StripDock::Hidden {
            return false;
        }
        if self.results_panel() || self.results_pane_on() || self.studio_owns_measurements() {
            return false;
        }
        match self.m.strip_dock {
            StripDock::Left | StripDock::Right => {
                let room = self.strip_column().h - 42;
                self.strip_side_lines().len() > (room / 20).max(1) as usize
            }
            _ => self.raw_strip_rows() > BAND_ROWS,
        }
    }

    pub(super) fn visible_measurement_rows(&self, raw: i32) -> i32 {
        if raw > BAND_ROWS && !matches!(self.m.strip_dock, StripDock::Left | StripDock::Right) {
            self.band_rows()
        } else {
            raw
        }
    }

    /// The band has more than one page: Prev / Next take the last cell of its second and third row.
    /// Default (the stock band) has its own row for them under the band instead.
    fn band_nav(&self) -> bool {
        !self.stock_readouts() && self.measurement_count() > BAND_PAGE
    }

    /// Rows of the band: what its values need, at most three. One page has no row of its own for page controls. The
    /// stock band (Default) always has three rows and the two of its page controls.
    fn band_rows(&self) -> i32 {
        if self.stock_readouts() {
            BAND_ROWS + NAV_ROWS
        } else if self.band_nav() {
            BAND_ROWS
        } else {
            (self.measurement_count().div_ceil(tools::STRIP_PER_ROW) as i32).clamp(1, BAND_ROWS)
        }
    }

    /// The cell of value `i` of a band page: row and column. With page controls the last column of rows two and three
    /// is theirs.
    pub(super) fn band_cell(&self, i: usize) -> (i32, i32) {
        let per = tools::STRIP_PER_ROW;
        let i = if self.band_nav() && i >= 2 * per - 1 { i + 1 } else { i };
        ((i / per) as i32, (i % per) as i32)
    }

    fn measurement_page_size(&self) -> usize {
        match self.m.strip_dock {
            StripDock::Left | StripDock::Right => ((self.strip_column().h - 42) / 20).max(1) as usize,
            _ => {
                if self.band_nav() {
                    BAND_NAV_PAGE
                } else {
                    BAND_PAGE
                }
            }
        }
    }

    fn page_bounds(&self, total: usize, size: usize) -> (usize, usize, usize) {
        let pages = total.max(1).div_ceil(size.max(1));
        let first = self.meas_pages.readouts.min(pages - 1) * size;
        (first, (first + size).min(total), pages)
    }

    fn page_buttons(&self) -> Option<(Rect, Rect)> {
        if !self.paged_strip() {
            return None;
        }
        match self.m.strip_dock {
            StripDock::Left | StripDock::Right => {
                let r = self.strip_column();
                Some((Rect::new(r.x + r.w - 96, r.y + 4, 42, 32), Rect::new(r.x + r.w - 50, r.y + 4, 42, 32)))
            }
            _ if self.stock_readouts() => {
                let y = self.strip_band_y() + BAND_ROWS * STRIP_ROW;
                Some((Rect::new(W - 170, y, 74, 32), Rect::new(W - 88, y, 74, 32)))
            }
            _ => {
                if !self.band_nav() {
                    return None;
                }
                // the last cell of rows two and three: Prev, the page, Next
                let pitch = (W - 12) / tools::STRIP_PER_ROW as i32;
                let (x, y) = (6 + 5 * pitch, self.strip_band_y() + STRIP_ROW);
                Some((Rect::new(x, y, 52, 2 * STRIP_ROW - 2), Rect::new(x + pitch - 4 - 52, y, 52, 2 * STRIP_ROW - 2)))
            }
        }
    }

    pub(super) fn measurement_navigation_tap(&mut self, x: i32, y: i32) -> bool {
        if let Some((prev, next)) = self.page_buttons() {
            let total = if matches!(self.m.strip_dock, StripDock::Left | StripDock::Right) {
                self.strip_side_lines().len()
            } else {
                self.meas_shown_count()
            };
            let (_, _, pages) = self.page_bounds(total, self.measurement_page_size());
            if prev.contains(x, y) || next.contains(x, y) {
                self.meas_pages.readouts = if prev.contains(x, y) {
                    self.meas_pages.readouts.min(pages - 1).saturating_sub(1)
                } else {
                    (self.meas_pages.readouts + 1).min(pages - 1)
                };
                return true;
            }
        }
        if let Some((_, body)) = self.results_rects() {
            let total = self.meas_shown_count();
            let fit = self.results_page_size();
            if total > fit && !self.results.folded {
                let (prev, next) = Self::results_page_buttons(body);
                let pages = total.div_ceil(fit);
                if prev.contains(x, y) || next.contains(x, y) {
                    self.meas_pages.readouts = if prev.contains(x, y) {
                        self.meas_pages.readouts.min(pages - 1).saturating_sub(1)
                    } else {
                        (self.meas_pages.readouts + 1).min(pages - 1)
                    };
                    return true;
                }
            }
        }
        false
    }

    /// The name of a measurement in `room` pixels of the strip's small font: the long one where it fits, else the short
    /// one (Positive Duty Cycle, +Duty). A pair or a catalog row has one name only.
    pub(super) fn strip_name(&self, key: Option<MeasKey>, long: &str, room: i32) -> String {
        if self.stock_readouts() || self.fonts().s.width(long) <= room {
            return long.to_string();
        }
        key.map_or_else(|| long.to_string(), |k| self.studio_badge_name(k, long))
    }

    /// The measurement index (as Measure > Selected numbers them) of each readout of `meas_rows`, in that order. A
    /// catalogue entry without a row (`cat_row` None) has no readout and is skipped.
    fn readout_indices(&self) -> Vec<usize> {
        let (n1, n2) = (self.m.meas.len(), self.m.meas2.len());
        (0..n1 + n2)
            .chain(
                self.m
                    .cat
                    .iter()
                    .enumerate()
                    .filter(|(_, &(id, a, b, c, d))| self.cat_row(id, a, b, c, d).is_some())
                    .map(|(j, _)| n1 + n2 + j),
            )
            .collect()
    }

    /// The readouts of the measurement strip as drawn (`draw_strip_band`, `draw_strip_side`,
    /// `draw_paged_measurement_strip`): each one's box and its measurement index, and the part of the strip that holds
    /// them. Empty while the strip shows no measurements (none on, the Results panel or pane holds them).
    pub(super) fn strip_readouts(&self) -> (Vec<(Rect, usize)>, Option<Rect>) {
        let mut cells = Vec::new();
        if self.strip_rows() == 0 || self.results_panel() || self.results_pane_on() || self.measurement_count() == 0 {
            return (cells, None);
        }
        let rows = self.meas_rows();
        let index = self.readout_indices();
        let paged = self.paged_strip();
        match self.m.strip_dock {
            StripDock::Left | StripDock::Right => {
                let col = self.strip_column();
                // the lines of the column: the measurements first, in their groups
                let mut order = Vec::new();
                for id in self.meas_groups() {
                    order.extend((0..rows.len()).filter(|&k| rows[k].src == id).map(|k| index[k]));
                }
                let lines = self.strip_side_lines().len();
                if paged {
                    let (first, end, _) = self.page_bounds(lines, self.measurement_page_size());
                    for (row, k) in (first..end.min(order.len())).enumerate() {
                        cells.push((Self::paged_side_line(col, row), order[k]));
                    }
                } else {
                    cells.extend(
                        order.iter().enumerate().map_while(|(k, &i)| Some((self.side_line(col, k, lines)?, i))),
                    );
                }
                (cells, Some(col))
            }
            _ => {
                let y0 = self.strip_band_y();
                if paged {
                    // oos-plus: the page holds the rows not hidden, placed as `draw_paged_measurement_strip` places
                    // them (`paged_band_cell`)
                    let shown: Vec<usize> = (0..rows.len()).filter(|&k| !self.meas_row_hidden(&rows[k])).collect();
                    let (first, end, _) = self.page_bounds(shown.len(), self.measurement_page_size());
                    let page = shown[first..end].iter().enumerate();
                    cells.extend(page.map(|(k, &i)| (self.paged_band_cell(y0, k), index[i])));
                    return (cells, Some(Rect::new(0, y0, self.scr.w, BAND_ROWS * STRIP_ROW)));
                }
                let strip_row = self.lay().strip_row;
                let mut y = y0;
                for id in self.meas_groups() {
                    let mine: Vec<usize> = (0..rows.len()).filter(|&k| rows[k].src == id).map(|k| index[k]).collect();
                    for part in mine.chunks(tools::STRIP_PER_ROW) {
                        cells.extend(self.band_cells(y, part.len()).zip(part.iter().copied()));
                        y += strip_row;
                    }
                }
                (cells, Some(Rect::new(0, y0, self.scr.w, y - y0)))
            }
        }
    }

    /// A tap on the measurement strip: Measure > Selected, with the readout tapped in focus. False off the strip.
    pub(super) fn strip_tap(&mut self, x: i32, y: i32) -> bool {
        let (cells, area) = self.strip_readouts();
        if !area.is_some_and(|a| a.contains(x, y)) {
            return false;
        }
        let hit = cells.iter().find(|(r, _)| r.contains(x, y)).map(|&(_, i)| i);
        self.meas_pages.selected = hit.map_or(0, |i| i / (2 * SEL_ROWS));
        self.open_menu(MenuId::MeasSelected);
        if let Some(i) = hit {
            let rows = self.dlg_layout().map(|d| d.rows).unwrap_or_default();
            self.focus = rows.iter().position(|r| r.it == Item::MeasRemove(i as u16)).unwrap_or(0);
        }
        true
    }

    pub(super) fn draw_paged_measurement_strip(&self, cv: &mut Canvas) -> bool {
        if !self.paged_strip() {
            return false;
        }
        let size = self.measurement_page_size();
        match self.m.strip_dock {
            StripDock::Left | StripDock::Right => {
                let lines = self.strip_side_lines();
                let (first, end, _) = self.page_bounds(lines.len(), size);
                let r = self.strip_column();
                cv.fill(r, BG);
                cv.text(&self.fonts().sb, r.x + 8, r.y + 16, "Measurements", TEXT);
                cv.text(
                    &self.fonts().s,
                    r.x + 8,
                    r.y + 34,
                    &format!("{}-{} of {} · {}", first + 1, end, lines.len(), self.measurement_gate_label()),
                    DIM,
                );
                let mut current_source = lines[..first]
                    .iter()
                    .rev()
                    .find_map(|line| line.pill.as_ref().map(|p| p.0.clone()))
                    .unwrap_or_default();
                for (row, line) in lines[first..end].iter().enumerate() {
                    let bar = Self::paged_side_line(r, row);
                    let y = bar.y;
                    cv.fill(bar, BAR);
                    if let Some((source, _)) = &line.pill {
                        current_source = source.clone();
                    }
                    if !line.align && line.pill.is_none() {
                        current_source.clear();
                    }
                    let named = |n: &str| {
                        if current_source.is_empty() {
                            n.to_string()
                        } else {
                            format!("{current_source} {n}")
                        }
                    };
                    let right = r.x + r.w - 8;
                    let value_x = right - self.fonts().sb.width(&line.value);
                    let room = (value_x - r.x - 16).max(0);
                    // the long name where it fits, else the short one
                    let fits = self.stock_readouts() || self.fonts().s.width(&named(&line.name)) <= room;
                    let name = if fits { named(&line.name) } else { named(&line.short) };
                    cv.set_clip(Rect::new(r.x + 8, y, room, 19));
                    cv.text(&self.fonts().s, r.x + 8, y + 14, &name, LABEL);
                    cv.no_clip();
                    cv.text(&self.fonts().sb, value_x, y + 14, &line.value, line.col);
                }
            }
            _ => {
                let keys = self.studio_keys();
                let rows: Vec<(Option<MeasKey>, tools::MeasRow)> = self
                    .meas_rows()
                    .into_iter()
                    .enumerate()
                    .map(|(i, r)| (keys.get(i).copied(), r))
                    .filter(|(_, r)| !self.meas_row_hidden(r))
                    .collect();
                let (first, end, _) = self.page_bounds(rows.len(), size);
                let y0 = self.strip_band_y();
                {
                    for (index, (key, item)) in rows[first..end].iter().enumerate() {
                        let r = self.paged_band_cell(y0, index);
                        let y = r.y;
                        cv.fill(r, BAR);
                        let color = self.wave_color(item.src);
                        let source = self.wave_name(item.src);
                        // the name starts after the source, however wide its name is; the stock band (Default) has a
                        // fixed place for it
                        let sx = cv.text(&self.fonts().sb, r.x + 3, y + 12, &source, color) + 6;
                        let nx = if self.stock_readouts() { r.x + 28 } else { sx };
                        let value = item.text();
                        let vx = r.x + r.w - 3 - self.fonts().sb.width(&value);
                        let room = (vx - nx - 5).max(0);
                        cv.set_clip(Rect::new(nx, y, room, 16));
                        cv.text(&self.fonts().s, nx, y + 12, &self.strip_name(*key, &item.name, room), LABEL);
                        cv.no_clip();
                        cv.text(&self.fonts().sb, vx, y + 12, &value, color);
                    }
                }
                if self.stock_readouts() {
                    let gate = self.measurement_gate_label();
                    let text = format!("Measurements {}-{} of {} · {}", first + 1, end, rows.len(), gate);
                    cv.text(&self.fonts().sb, 12, y0 + BAND_ROWS * STRIP_ROW + 21, &text, TEXT);
                } else if let Some((prev, next)) = self.page_buttons() {
                    let (_, _, pages) = self.page_bounds(rows.len(), size);
                    let mid = Rect::new(prev.right(), prev.y, next.x - prev.right(), prev.h);
                    let page = format!("{}/{}", self.meas_pages.readouts.min(pages - 1) + 1, pages);
                    cv.text_in(&self.fonts().sb, mid, 1, &page, TEXT);
                }
                // Meter and cursor rows follow the band.
                let extra_y = y0 + self.band_rows() * STRIP_ROW;
                for (row, (label, value)) in self.dvm_row().iter().enumerate() {
                    if row > 1 {
                        break;
                    }
                    let x = 12 + row as i32 * 420;
                    let next = cv.text(&self.fonts().s, x, extra_y + 12, label, DIM) + 8;
                    cv.text(&self.fonts().sb, next, extra_y + 12, value, TEXT);
                }
                if self.m.cursor != CursorMode::Off {
                    let y = extra_y + if self.m.dvm_on || self.m.cnt_on { STRIP_ROW } else { 0 };
                    let mut x = 12;
                    if let Some((label, color)) = self.cursor_badge() {
                        let pill = Rect::new(8, y + 1, 42, 14);
                        cv.round(pill, 3, color, 255);
                        cv.text_in(&self.fonts().sb, pill, 1, &label, DARK_TEXT);
                        x = 58;
                    }
                    for (label, value) in self.cursor_values() {
                        x = cv.text(&self.fonts().s, x, y + 12, &label, DIM) + 8;
                        x = cv.text(&self.fonts().sb, x, y + 12, &value, TEXT) + 24;
                    }
                }
            }
        }
        if let Some((prev, next)) = self.page_buttons() {
            // words where the buttons have the room (a column at a side, the stock band's own row), arrows in a cell
            let words = self.stock_readouts() || matches!(self.m.strip_dock, StripDock::Left | StripDock::Right);
            for (rect, label) in [(prev, if words { "Prev" } else { "<" }), (next, if words { "Next" } else { ">" })] {
                btn(cv, rect, BTN, BTN_BORDER);
                cv.text_in(&self.fonts().sb, rect, 1, label, TEXT);
            }
        }
        true
    }

    /// Columns of a card docked under or above the plot: the rows sit side by side in the width the plot has.
    pub(super) fn results_cols(&self) -> usize {
        if !matches!(self.results_docked(), Some(StripDock::Below | StripDock::Above)) {
            return 1;
        }
        ((self.plot_span().1 - 12) / self.lay().results_w).clamp(1, 4) as usize
    }

    pub(super) fn results_page_size(&self) -> usize {
        // rows are taller at the larger text sizes: count with the row the card draws, so a page stays in the plot
        if matches!(self.results_docked(), Some(StripDock::Below | StripDock::Above)) {
            return 2 * self.results_cols();
        }
        let row = self.results_row(0);
        ((self.over_area().h - 98).max(row) / row).clamp(1, 8) as usize
    }

    pub(super) fn results_page_range(&self, total: usize) -> (usize, usize) {
        let (first, end, _) = self.page_bounds(total, self.results_page_size());
        (first, end)
    }

    pub(super) fn results_page_buttons(body: Rect) -> (Rect, Rect) {
        (
            Rect::new(body.x + body.w - 132, body.y + body.h - 35, 58, 30),
            Rect::new(body.x + body.w - 68, body.y + body.h - 35, 58, 30),
        )
    }

    pub(super) fn draw_results_page_buttons(&self, cv: &mut Canvas, body: Rect, total: usize) {
        if total <= self.results_page_size() || self.results.folded {
            return;
        }
        let (first, end) = self.results_page_range(total);
        cv.text(&self.fonts().s, body.x + 10, body.y + body.h - 14, &format!("{}-{} / {}", first + 1, end, total), DIM);
        let (prev, next) = Self::results_page_buttons(body);
        for (rect, label) in [(prev, "Prev"), (next, "Next")] {
            btn(cv, rect, BTN, BTN_BORDER);
            cv.text_in(&self.fonts().sb, rect, 1, label, TEXT);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    // Break it catches: a tap on a readout of the strip did nothing; to change or remove a measurement the user went
    // through Measure > Selected and looked for it there. Every dock, the strip in one page and in pages.
    #[test]
    fn a_tap_on_a_readout_opens_selected_with_it_in_focus() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.meas = vec![(Meas::Vpp, WaveId::Ch(0)), (Meas::Freq, WaveId::Ch(0))];
        ui.m.meas2.clear();
        ui.m.ch[1].on = true;
        ui.m.meas.push((Meas::Vrms, WaveId::Ch(1)));
        let focused = |ui: &Ui| ui.dlg_layout().map(|d| d.rows[ui.focus].it);
        for many in [false, true] {
            if many {
                for &kind in &ALL_MEAS {
                    if !ui.m.meas.contains(&(kind, WaveId::Ch(0))) {
                        ui.m.meas.push((kind, WaveId::Ch(0)));
                    }
                }
            }
            // oos-plus: a hidden strip has no readouts to tap
            for dock in StripDock::ALL.into_iter().filter(|d| *d != StripDock::Hidden) {
                ui.m.strip_dock = dock;
                ui.relayout();
                ui.step(0.02);
                let (cells, area) = ui.strip_readouts();
                let area = area.unwrap_or_else(|| panic!("{dock:?}: a strip"));
                assert!(!cells.is_empty(), "{dock:?} many {many}: readouts");
                assert_eq!(ui.paged_strip(), many, "{dock:?}");
                for &(r, i) in cells.iter().take(4) {
                    assert!(area.contains(r.x + r.w / 2, r.y + r.h / 2), "{dock:?}: {r:?} in {area:?}");
                    ui.tap(r.x + r.w / 2, r.y + r.h / 2);
                    assert!(ui.menu_open && ui.menu == MenuId::MeasSelected, "{dock:?} many {many}: index {i}");
                    assert_eq!(focused(&ui), Some(Item::MeasRemove(i as u16)), "{dock:?} many {many}");
                    ui.close_menu();
                }
                // the third readout is CH2's Vrms wherever the strip groups it
                assert!(cells.iter().any(|&(_, i)| i == 2) || many, "{dock:?}: CH2 Vrms is a readout");
            }
        }
        // beside the values (the source's pill of the first row): the list opens, its first row in focus
        ui.m.meas.truncate(3);
        ui.m.strip_dock = StripDock::Below;
        ui.relayout();
        ui.step(0.02);
        let (cells, area) = ui.strip_readouts();
        let area = area.unwrap();
        let (px, py) = (area.x + 12, cells[0].0.y + cells[0].0.h / 2);
        assert!(
            area.contains(px, py) && !cells.iter().any(|(r, _)| r.contains(px, py)),
            "the pill is beside the values"
        );
        ui.tap(px, py);
        assert!(ui.menu_open && ui.menu == MenuId::MeasSelected, "a tap beside the values");
        assert_eq!((ui.focus, ui.meas_pages.selected), (0, 0));
        ui.close_menu();
        // with the Results panel or pane the strip holds no measurements: a tap there is not this one
        for (panel, pane) in [(true, 0), (false, crate::ui::tables::PANE_RESULTS)] {
            (ui.results.on, ui.m.pane) = (panel, pane);
            ui.relayout();
            ui.step(0.02);
            assert!(ui.strip_readouts().1.is_none(), "panel {panel} pane {pane}");
            ui.tap(px, py);
            assert!(!(ui.menu_open && ui.menu == MenuId::MeasSelected), "panel {panel} pane {pane}: no Selected");
            ui.close_menu();
        }
    }

    #[test]
    fn seventy_three_selections_survive_and_every_page_is_reachable() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.meas.clear();
        ui.m.meas2.clear();
        for &kind in &ALL_MEAS {
            ui.activate(Item::MeasAdd(kind));
        }
        ui.sources.b = WaveId::Ch(1);
        for kind in 0..13 {
            ui.meas_pair_toggle(kind);
        }
        assert_eq!(ui.meas_rows().len(), 73);
        let original = ui.m.meas.clone();
        ui.relayout();
        assert!(ui.paged_strip());
        assert!(ui.plot.grid.h >= 300);
        let mut visited = HashSet::new();
        for page in 0..73usize.div_ceil(BAND_PAGE) {
            ui.meas_pages.readouts = page;
            let (first, end, _) = ui.page_bounds(73, BAND_PAGE);
            visited.extend(first..end);
        }
        assert_eq!(visited.len(), 73);
        for dock in StripDock::ALL.into_iter().filter(|d| *d != StripDock::Hidden) {
            ui.m.strip_dock = dock;
            ui.relayout();
            let mut canvas = Canvas::new(W, H);
            ui.render(&mut canvas);
            if let Ok(directory) = std::env::var("OOS_SHOT_DIR") {
                let _ = std::fs::write(
                    std::path::Path::new(&directory).join(format!("measurements-73-{dock:?}.png")),
                    crate::store::png_of(&canvas),
                );
            }
            assert_eq!(ui.m.meas, original);
            assert!(ui.page_buttons().is_some());
        }
    }

    #[test]
    fn stopped_reference_readouts_follow_the_visible_window() {
        let mut ui = crate::ui::testkit::demo_ui();
        // screen gating: these tests check the visible-window gate (the default is the whole record)
        ui.m.meas_full_record = false;
        ui.set_offline(true);
        let samples: Vec<f32> = (0..4000)
            .map(|i| {
                let amplitude = if i < 2000 { 1.0 } else { 3.0 };
                if (i / 50) % 2 == 0 {
                    amplitude
                } else {
                    -amplitude
                }
            })
            .collect();
        ui.refs[0] = Some(RefWave { label: "changing amplitude".into(), ..RefWave::test(samples, 1e-9, 0.0) });
        ui.offline.slot = 0;
        ui.m.zoom = false;
        ui.m.tscale = tscales().iter().position(|&v| (v - 100e-9).abs() < 1e-12).unwrap();
        ui.m.tfine = 1.0;
        ui.m.delay = 500e-9;
        ui.refresh_stopped_reference();
        let first = ui.meas_at(WaveId::Ref(0), Meas::Vpp);
        ui.m.delay = 2.5e-6;
        ui.refresh_stopped_reference();
        let second = ui.meas_at(WaveId::Ref(0), Meas::Vpp);
        assert!((first - 2.0).abs() < 0.01, "first window {first}");
        assert!((second - 6.0).abs() < 0.01, "moved window {second}");
        assert_eq!(ui.m.run, RunState::Stop, "panning does not acquire or start playback");
    }

    #[test]
    fn the_catalog_opens_settings_and_adds_from_the_time_group() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.meas.clear();
        ui.key(key::MEASURE);
        assert_eq!(ui.menu, MenuId::MeasAdd);
        let settings = ui.catalog_settings_rect();
        ui.tap(settings.x + 4, settings.y + 4);
        assert_eq!(ui.menu, MenuId::Measure, "settings stays reachable from the catalog");
        ui.close_menu();
        ui.key(key::MEASURE);
        ui.catalog_add(super::super::meascatalog::tile(crate::measures::std_id(Meas::LowTime)));
        assert!(ui.m.meas.iter().any(|e| e.0 == Meas::LowTime));
        ui.catalog_add(super::super::meascatalog::tile(crate::measures::std_id(Meas::LowTime)));
        assert!(!ui.m.meas.iter().any(|e| e.0 == Meas::LowTime), "adding again removes it");
    }

    #[test]
    fn eye_panes_leave_every_measurement_dock_and_page_accessible() {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.set_offline(true);
        let samples: Vec<f32> = (0..4000).map(|i| if (i / 50) % 2 == 0 { 0.5 } else { -0.5 }).collect();
        for slot in 0..2 {
            ui.refs[slot] = Some(RefWave {
                color: slot,
                label: format!("R{} clock", slot + 1),
                vpd: 0.2,
                ..RefWave::test(samples.clone(), 1e-9, 0.0)
            });
        }
        ui.m.meas = ALL_MEAS.iter().map(|&kind| (kind, WaveId::Ref(0))).collect();
        ui.m.meas2 = (0..13).map(|kind| (kind, WaveId::Ref(0), WaveId::Ref(1))).collect();
        ui.m.view = AView::Eye;
        ui.m.eye_src = WaveId::Ref(0);
        ui.m.eye_rate = 10e6;
        ui.m.zoom = false;
        for placement in [EyePlace::Side, EyePlace::Under] {
            ui.m.eye_place = placement;
            for dock in StripDock::ALL.into_iter().filter(|d| *d != StripDock::Hidden) {
                ui.m.strip_dock = dock;
                ui.meas_pages.readouts = 0;
                ui.relayout();
                let (wave, eye) = (ui.plot.grid, ui.eye_pane_rect());
                let strip = ui.strip_rect().expect("73 readouts have a strip");
                assert!(!overlaps(wave, eye), "{placement:?}/{dock:?}: wave {wave:?} eye {eye:?}");
                assert!(!overlaps(strip, wave), "{placement:?}/{dock:?}: strip {strip:?} wave {wave:?}");
                assert!(!overlaps(strip, eye), "{placement:?}/{dock:?}: strip {strip:?} eye {eye:?}");
                for rect in [wave, eye, strip] {
                    assert!(
                        rect.x >= 0 && rect.x + rect.w <= W && rect.y >= TOP.h && rect.y + rect.h <= Ui::MENU_BTN.y,
                        "{placement:?}/{dock:?}: {rect:?}"
                    );
                }
                let close = ui.eye_pane_close();
                assert!(eye.contains(close.x, close.y) && eye.contains(close.x + close.w - 1, close.y + close.h - 1));
                let (previous, next) = ui.page_buttons().expect("all selected readouts remain paged");
                assert!(!overlaps(next, eye) && !overlaps(previous, eye));
                ui.tap(next.x + next.w / 2, next.y + next.h / 2);
                assert_eq!(ui.meas_pages.readouts, 1, "{placement:?}/{dock:?}: Next remains operable");
                let mut canvas = Canvas::new(W, H);
                ui.render(&mut canvas);
                assert_eq!(ui.meas_rows().len(), 73);
                if let Ok(directory) = std::env::var("OOS_SHOT_DIR") {
                    let _ = std::fs::write(
                        std::path::Path::new(&directory).join(format!("eye-{placement:?}-measurements-{dock:?}.png")),
                        crate::store::png_of(&canvas),
                    );
                }
            }
        }
    }

    /// Default keeps upstream's readouts: the short names, "----" with no reason, the rows of a channel that is off,
    /// and a paged band with a row of its own (Prev, Next, "Measurements a-b of N"). Minimal and Studio keep the
    /// fork's: long names, a reason, those rows left out and Prev / Next in the band's last column.
    #[test]
    fn default_keeps_the_stock_readouts_and_minimal_and_studio_the_fork_ones() {
        use super::super::meascatalog::cat as Cat;
        for layout in LAYOUTS {
            let mut ui = crate::ui::testkit::demo_ui();
            ui.m.layout = layout;
            ui.m.strip_dock = StripDock::Below;
            ui.m.meas = vec![(Meas::PDuty, WaveId::Ch(0)), (Meas::Vupper, WaveId::Ch(1))];
            ui.m.meas2.clear();
            ui.m.cat = vec![(Cat::EyeHeightBer as u16, WaveId::Ref(0), WaveId::Ref(0), WaveId::Ref(0), WaveId::Ref(0))];
            ui.m.ch[1].on = false;
            let stock = layout == Layout::Default;
            let rows = ui.meas_rows();
            let names: Vec<&str> = rows.iter().take(2).map(|r| r.name.as_str()).collect();
            let eye = rows.last().expect("the catalog row").text();
            if stock {
                assert_eq!(names, ["+Duty", "Upper"], "{layout:?}");
                assert_eq!(eye, "----", "{layout:?}: no reason");
                assert_eq!(ui.meas_shown_count(), 3, "{layout:?}: CH2 is off and its row stays");
                assert!(ui.meas_groups().contains(&WaveId::Ch(1)), "{layout:?}");
            } else {
                assert_eq!(names, ["Positive Duty Cycle", "Upper Level"], "{layout:?}");
                assert!(eye.contains("R1") && eye.contains("empty"), "{layout:?}: {eye}");
                assert_eq!(ui.meas_shown_count(), 2, "{layout:?}: the row of CH2 is left out");
            }
            ui.m.ch[1].on = true;
            // a paged band: more measurements than three rows hold
            ui.m.cat.clear();
            ui.m.meas = ALL_MEAS.iter().take(20).map(|&kind| (kind, WaveId::Ch(0))).collect();
            ui.relayout();
            if !ui.paged_strip() {
                // Studio shows its measurements in its own panel
                assert_ne!(layout, Layout::Default);
                continue;
            }
            let (prev, next) = ui.page_buttons().expect("a paged band has Prev and Next");
            let band_bottom = ui.strip_band_y() + BAND_ROWS * STRIP_ROW;
            if stock {
                assert_eq!(ui.band_rows(), BAND_ROWS + NAV_ROWS);
                assert_eq!(ui.measurement_page_size(), BAND_PAGE);
                let row = |x| Rect::new(x, band_bottom, 74, 32);
                assert_eq!((prev, next), (row(W - 170), row(W - 88)), "Prev / Next in the row under the band");
                assert_eq!(ui.band_cell(BAND_PAGE - 1), (BAND_ROWS - 1, tools::STRIP_PER_ROW as i32 - 1));
            } else {
                assert_eq!(ui.band_rows(), BAND_ROWS, "{layout:?}");
                assert_eq!(ui.measurement_page_size(), BAND_NAV_PAGE, "{layout:?}");
                assert!(next.y + next.h <= band_bottom, "{layout:?}: Prev / Next inside the band");
            }
            ui.tap(next.x + next.w / 2, next.y + next.h / 2);
            assert_eq!(ui.meas_pages.readouts, 1, "{layout:?}: Next pages");
            let mut canvas = Canvas::new(W, H);
            ui.render(&mut canvas);
        }
    }
}
