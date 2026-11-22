// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: the one Add button of the top bar and the compact grouped menu under it. It replaces the old "+ Add" panel
//! button and the Meas button: Measurement comes first, then the views (results table, plot, cursors, search), the sources
//! (math, reference, bus) and the screen (zoom, the measurement strip). Every row is a short way into a page that exists.
use super::addrail::AddKind;
use super::studio::icon_cursors;
use super::*;

const MENU_W: i32 = 252;
/// A row of a list of words (`MENU_ROW`; it was 34 px, then 40).
const ROW: i32 = super::workspace::MENU_ROW;
const SEP: i32 = 9;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AddRow {
    Measurement,
    ResultsTable,
    Plot,
    Cursors,
    Search,
    Math,
    Reference,
    Bus,
    Zoom,
    ZoomBox,
    // the Windows column: what Windows > Lower and the plots of Default offer
    Spectrum,
    Trend,
    Histogram,
    Xy,
    Eye,
    Events,
    BusView,
}

/// The second column of the menu, under its caption: the lower panes of the Default layout (Spectrum, Results, Events,
/// Trend, Bus view) and the plots drawn in the waveform. A tap puts one on the screen, the next takes it off.
pub(super) const ADD_WINDOWS: [AddRow; 7] =
    [AddRow::Spectrum, AddRow::Trend, AddRow::Histogram, AddRow::Xy, AddRow::Eye, AddRow::Events, AddRow::BusView];
/// The caption of the second column.
const CAP: i32 = 26;

/// The rows in their groups; a `None` is a separator.
/// Only what adds something to the screen: the zoom bar style is on the Zoom tab of Horizontal, the measurement
/// strip is View > Show pinned badges, and the measurement settings are Menu > Measure.
pub(super) const ADD_MENU: [Option<AddRow>; 13] = [
    Some(AddRow::Measurement), Some(AddRow::ResultsTable), None,
    Some(AddRow::Plot), Some(AddRow::Cursors), Some(AddRow::Search), None,
    Some(AddRow::Math), Some(AddRow::Reference), Some(AddRow::Bus), None,
    Some(AddRow::Zoom), Some(AddRow::ZoomBox),
];

impl Ui {
    pub(super) fn add_row_label(&self, r: AddRow) -> &'static str {
        match r {
            AddRow::Measurement => "Measurement",
            AddRow::ResultsTable => "Results table",
            AddRow::Plot => "Plot",
            AddRow::Cursors => "Cursors",
            AddRow::Search => "Search",
            AddRow::Math => "Math",
            AddRow::Reference => "Reference",
            AddRow::Bus => "Bus",
            AddRow::Zoom => "Zoom",
            AddRow::ZoomBox => "Zoom box",
            AddRow::Spectrum => "Spectrum",
            AddRow::Trend => "Trend",
            AddRow::Histogram => "Histogram",
            AddRow::Xy => "XY",
            AddRow::Eye => "Eye",
            AddRow::Events => "Events",
            AddRow::BusView => "Bus view",
        }
    }

    /// The row says "On" when what it adds is on the screen.
    fn add_row_on(&self, r: AddRow) -> bool {
        match r {
            AddRow::Measurement => self.measurement_count() > 0,
            AddRow::ResultsTable => self.rail_kind_on_pub(AddKind::Table),
            AddRow::Plot => self.rail_kind_on_pub(AddKind::Plot),
            AddRow::Cursors => self.m.cursor != CursorMode::Off,
            AddRow::Search => self.studio.search().is_some(),
            AddRow::Math => self.m.math != MathOp::Off,
            AddRow::Reference => self.refs.iter().any(Option::is_some),
            AddRow::Bus => self.rail_kind_on_pub(AddKind::Bus),
            AddRow::Zoom => self.m.zoom,
            AddRow::ZoomBox => self.zoom_box.arm,
            AddRow::Spectrum => self.rail_plot_on(3),
            AddRow::Trend => self.rail_plot_on(4),
            AddRow::Histogram => self.rail_plot_on(2),
            AddRow::Xy => self.rail_plot_on(0),
            AddRow::Eye => self.rail_plot_on(1),
            AddRow::Events => self.rail_table_on(2),
            AddRow::BusView => self.m.pane == tables::PANE_BUS,
        }
    }

    /// The menu: under the Add button, as high as its rows, kept on the screen.
    pub(super) fn studio_add_rect(&self) -> Rect {
        let top = self.frame().top;
        let btn = self.studio_top().add;
        let h = 8 + ADD_MENU.iter().map(|r| if r.is_some() { ROW } else { SEP }).sum::<i32>();
        let w = MENU_W * 2;
        let x = (btn.right() - w).clamp(4, self.scr.w - w - 4);
        Rect::new(x, top.bottom() + 6, w, h.min(self.scr.h - top.bottom() - 12))
    }

    /// The rows with their rectangles.
    pub(super) fn studio_add_rows(&self) -> Vec<(AddRow, Rect)> {
        let p = self.studio_add_rect();
        let mut y = p.y + 4;
        let mut out = Vec::new();
        for r in ADD_MENU {
            match r {
                Some(row) => { out.push((row, Rect::new(p.x + 4, y, MENU_W - 8, ROW))); y += ROW; }
                None => y += SEP,
            }
        }
        for (k, row) in ADD_WINDOWS.into_iter().enumerate() {
            out.push((row, Rect::new(p.x + MENU_W + 4, p.y + 4 + CAP + k as i32 * ROW, MENU_W - 8, ROW)));
        }
        out
    }

    pub(super) fn studio_add_open(&mut self) {
        if self.menu_open { self.close_menu(); }
        self.studio.popup = None;
        self.studio.add_menu = true;
    }

    fn add_row_icon(&self, cv: &mut Canvas, row: AddRow, cx: i32, cy: i32, c: Rgb) {
        let f = self.fonts();
        match row {
            AddRow::Measurement => { for (i, h) in [6, 11, 8].into_iter().enumerate() { cv.fill(Rect::new(cx - 7 + i as i32 * 5, cy + 6 - h, 3, h), c); } }
            AddRow::ResultsTable => { cv.rect(Rect::new(cx - 7, cy - 6, 14, 12), c); cv.hline(cx - 7, cx + 6, cy, c); cv.vline(cx, cy - 6, cy + 5, c); }
            AddRow::Plot => { cv.line(cx - 7, cy + 5, cx - 2, cy - 3, c); cv.line(cx - 2, cy - 3, cx + 2, cy + 3, c); cv.line(cx + 2, cy + 3, cx + 7, cy - 5, c); }
            AddRow::Cursors => icon_cursors(cv, cx - 5, cy, c),
            AddRow::Search => { cv.circle(cx - 2, cy - 2, 5, c); cv.circle(cx - 2, cy - 2, 3, PANEL_BG); cv.line(cx + 2, cy + 2, cx + 7, cy + 7, c); }
            AddRow::Math => cv.text_in(&f.sb, Rect::new(cx - 10, cy - 9, 20, 18), 1, "fx", c),
            AddRow::Reference => cv.text_in(&f.sb, Rect::new(cx - 10, cy - 9, 20, 18), 1, "R", c),
            AddRow::Bus => { for dy in [-5, 0, 5] { cv.hline(cx - 7, cx + 7, cy + dy, c); } cv.vline(cx - 3, cy - 7, cy + 7, c); cv.vline(cx + 3, cy - 7, cy + 7, c); }
            AddRow::ZoomBox => { cv.round_outline(Rect::new(cx - 7, cy - 5, 14, 10), 1, c); cv.fill(Rect::new(cx - 7, cy - 5, 4, 2), c); }
            AddRow::Spectrum => { for (i, h) in [4, 10, 6, 12, 3].into_iter().enumerate() { cv.vline(cx - 8 + i as i32 * 4, cy + 6 - h, cy + 6, c); } }
            AddRow::Trend => { cv.line(cx - 7, cy + 4, cx - 2, cy - 1, c); cv.line(cx - 2, cy - 1, cx + 1, cy + 2, c); cv.line(cx + 1, cy + 2, cx + 7, cy - 5, c); }
            AddRow::Histogram => { for (i, h) in [4, 8, 12, 7].into_iter().enumerate() { cv.fill(Rect::new(cx - 8 + i as i32 * 4, cy + 6 - h, 3, h), c); } }
            AddRow::Xy => { cv.vline(cx - 6, cy - 6, cy + 6, c); cv.hline(cx - 6, cx + 6, cy + 6, c); cv.line(cx - 4, cy + 4, cx + 5, cy - 5, c); }
            AddRow::Eye => { cv.circle(cx, cy, 6, c); cv.circle(cx, cy, 4, PANEL_BG); cv.circle(cx, cy, 2, c); }
            AddRow::Events => { for dy in [-5, 0, 5] { cv.hline(cx - 7, cx + 7, cy + dy, c); } cv.fill(Rect::new(cx - 7, cy - 6, 3, 3), c); }
            AddRow::BusView => { cv.hline(cx - 7, cx + 7, cy - 3, c); cv.hline(cx - 7, cx + 7, cy + 3, c); cv.vline(cx - 2, cy - 6, cy + 6, c); }
            AddRow::Zoom => { cv.circle(cx - 2, cy - 2, 5, c); cv.circle(cx - 2, cy - 2, 3, PANEL_BG); cv.line(cx + 2, cy + 2, cx + 7, cy + 7, c); cv.hline(cx - 4, cx, cy - 2, c); cv.vline(cx - 2, cy - 4, cy, c); }
        }
    }

    pub(super) fn draw_studio_add_menu(&self, cv: &mut Canvas) {
        if !self.studio.add_menu { return; }
        let f = self.fonts();
        let p = self.studio_add_rect();
        cv.round(p, 8, BTN_BORDER, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        let btn = self.studio_top().add;
        let x = btn.x + btn.w / 2;
        cv.triangle([(x - 7, p.y + 1), (x + 7, p.y + 1), (x, p.y - 6)], BTN_BORDER);
        cv.triangle([(x - 6, p.y + 2), (x + 6, p.y + 2), (x, p.y - 4)], PANEL_BG);
        let mut sep_y = p.y + 4;
        for r in ADD_MENU {
            match r {
                Some(_) => sep_y += ROW,
                None => { cv.hline(p.x + 10, p.x + MENU_W - 11, sep_y + SEP / 2, LINE); sep_y += SEP; }
            }
        }
        cv.vline(p.x + MENU_W, p.y + 10, p.bottom() - 10, LINE);
        cv.text_in(&f.sb, Rect::new(p.x + MENU_W + 16, p.y + 4, MENU_W - 24, CAP), 0, "Windows", DIM);
        for (row, r) in self.studio_add_rows() {
            let on = self.add_row_on(row);
            self.add_row_icon(cv, row, r.x + 18, r.y + r.h / 2, if on { ACCENT } else { TEXT });
            cv.set_clip(Rect::new(r.x + 34, r.y, r.w - 70, r.h));
            cv.text_in(&f.m, Rect::new(r.x + 36, r.y, r.w - 70, r.h), 0, self.add_row_label(row), TEXT);
            cv.no_clip();
            if on && row != AddRow::Measurement { cv.text_in(&f.sb, Rect::new(r.right() - 40, r.y, 34, r.h), 2, "On", ACCENT); }
            if row == AddRow::Measurement && self.measurement_count() > 0 { cv.text_in(&f.sb, Rect::new(r.right() - 44, r.y, 38, r.h), 2, &self.measurement_count().to_string(), DIM); }
        }
    }

    /// A tap while the Add menu is open: a row does its action, a tap outside closes it. Always taken.
    pub(super) fn studio_add_tap(&mut self, x: i32, y: i32) -> bool {
        if !self.studio.add_menu { return false; }
        let p = self.studio_add_rect();
        let chip = self.top_hit(self.studio_top().add);
        self.studio.add_menu = false;
        if chip.contains(x, y) || !p.contains(x, y) { return true; }
        let Some((row, _)) = self.studio_add_rows().into_iter().find(|(_, r)| r.contains(x, y)) else { return true };
        match row {
            AddRow::Measurement => self.open_meas_list(),
            // the page with the three tables (measurements, search events, decoder events). The row docked the
            // table of the measurements and said nothing of the other two, which no row of this menu led to
            AddRow::ResultsTable => self.add_open_table(),
            AddRow::Plot => self.add_open_kind(AddKind::Plot),
            AddRow::Cursors => self.add_open_kind(AddKind::Cursors),
            AddRow::Search => self.add_open_kind(AddKind::Search),
            AddRow::Math => self.add_open_kind(AddKind::Math),
            AddRow::Reference => self.add_open_kind(AddKind::Ref),
            AddRow::Bus => self.add_open_kind(AddKind::Bus),
            AddRow::Zoom => { let on = !self.m.zoom; self.set_zoom(on); }
            AddRow::ZoomBox => self.arm_zoom_area(),
            AddRow::Spectrum => self.rail_plot_toggle(3),
            AddRow::Trend => self.rail_plot_toggle(4),
            AddRow::Histogram => self.rail_plot_toggle(2),
            AddRow::Xy => self.rail_plot_toggle(0),
            AddRow::Eye => self.rail_plot_toggle(1),
            AddRow::Events => self.rail_table_toggle(2),
            AddRow::BusView => {
                let on = self.m.pane == tables::PANE_BUS;
                self.select_option(Item::PaneSel, if on { "Traces" } else { "Bus view" });
            }
        }
        true
    }

    /// The Results table page, on the table that makes sense now: one that is on the screen, else the events of a
    /// decoder or of a search that is on, else the measurements.
    fn add_open_table(&mut self) {
        self.add_open_kind(AddKind::Table);
        self.sheet.add.table = self.rail_table_default();
    }

    /// A page of the Add panel, straight from the menu.
    fn add_open_kind(&mut self, k: AddKind) {
        self.open_menu(MenuId::AddPanel);
        self.sheet.add.kind = None;
        self.rail_open_pub(k);
    }

    /// The chip of the Add button: lit while its menu is open.
    pub(super) fn studio_add_chip_on(&self) -> bool { self.studio.add_menu || (self.menu_open && self.menu == MenuId::AddPanel) }
}

