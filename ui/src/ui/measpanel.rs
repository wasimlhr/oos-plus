// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: the controls, the cards view and the dropdowns of the docked measurements panel. The controls (source, gate,
//! search, Pinned only, Add, statistics) are one bar: in the title strip when the panel is wide, in one slim toolbar
//! under it when it is narrow, with what does not fit behind a ... button. The views are the strip of cells, the
//! table (measview.rs) and the cards; there is no footer and no paging: the list scrolls by swipe (measscroll.rs).
use super::dock::{VKind, STRIP_H};
use super::studio::{chip, icon_pin, PanelGeo, StudioView};
use super::*;

/// Height of the toolbar of a narrow panel: its buttons are a finger high (it was 32 px).
pub(super) const TOOL_H: i32 = 40;
/// A panel at least this wide has its controls in the title strip.
pub(super) const WIDE_BAR_FROM: i32 = 760;
/// A card of the cards view and the space between cards.
pub(super) const CARD_H: i32 = 60;
const CARD_GAP: i32 = 4;
/// A cell of the strip view: one line, as the measurement strip of the Default layout has them. So many pixels
/// wide at least, and so high (a finger finds it with the cell beside it as its margin).
const CELL_W: i32 = 160;
pub(super) const CELL_H: i32 = 26;
const CELL_GAP: i32 = 2;

/// The dropdown that is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PanelDrop {
    Source,
    Gate,
    /// the ... button: the controls that did not fit
    More,
}

/// The controls of the bar, in the order of the bar.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum BarItem {
    Source,
    Gate,
    Search,
    Pinned,
    Add,
    Stats,
}
const BAR_ORDER: [BarItem; 6] = [BarItem::Source, BarItem::Gate, BarItem::Search, BarItem::Pinned, BarItem::Stats, BarItem::Add];
/// The order in which controls go behind the ... button when the room is short. Add (+) stays: it sits next to the view switch.
const BAR_DROP: [BarItem; 4] = [BarItem::Gate, BarItem::Stats, BarItem::Source, BarItem::Pinned];
const MORE_W: i32 = 34;

/// What a row of the ... list does: a control of the bar, or one choice of the gate or of the statistics.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum MoreAct {
    Item(BarItem),
    /// the gate: the full record, or the screen
    Gate(bool),
    /// the statistics: 0 off, 1 mean and sigma, 2 minimum and maximum
    Stats(u8),
}

/// One control: what is drawn, and the (larger) rectangle that takes the touch.
#[derive(Clone, Copy, Debug)]
pub(super) struct BarSlot {
    pub item: BarItem,
    pub r: Rect,
    pub hit: Rect,
}

pub(super) struct PanelBar {
    pub slots: Vec<BarSlot>,
    pub more: Option<BarSlot>,
    pub hidden: Vec<BarItem>,
    /// in the title strip (else in the toolbar under it)
    pub wide: bool,
    /// the labelled chips (else compact ones)
    pub full: bool,
    /// the search field and its close button, when the search is open: it replaces the title (or the toolbar)
    pub field: Option<(Rect, Rect)>,
    /// the left edge of what the bar uses
    pub left: i32,
}

/// How the panel shows its rows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PanelView {
    List,
    Cards,
    Detailed,
}

impl Ui {
    pub(super) fn panel_is_cards(&self) -> bool {
        self.m.layout == Layout::Studio && self.studio.panel.cards && !self.m.studio_detail
            && matches!(self.studio.view, StudioView::Docked | StudioView::Floating | StudioView::Offline)
    }

    /// The strip view: the measurements as cells of one line each, in as many columns as the panel is wide. It is the
    /// first of the three views. It was a list of one measurement a row, which showed two of them in a low panel
    /// under the plot (the owner on the unit).
    pub(super) fn panel_is_strip(&self) -> bool {
        self.m.layout == Layout::Studio && !self.studio.panel.cards && !self.m.studio_detail
            && matches!(self.studio.view, StudioView::Docked | StudioView::Floating | StudioView::Offline)
    }

    /// The panel lays its measurements out in cells: the cards, or the strip.
    pub(super) fn panel_cells(&self) -> bool { self.panel_is_cards() || self.panel_is_strip() }

    /// The cells of a list `w` wide: columns, the height of a cell, the space between two.
    pub(super) fn panel_cell_geo(&self, w: i32) -> (i32, i32, i32) {
        if self.panel_is_strip() { ((w / CELL_W).clamp(1, 6), CELL_H, CELL_GAP) } else { (Self::panel_card_cols(w), CARD_H, CARD_GAP) }
    }

    pub(super) fn panel_view(&self) -> PanelView {
        if self.m.studio_detail { PanelView::Detailed } else if self.studio.panel.cards { PanelView::Cards } else { PanelView::List }
    }

    pub(super) fn panel_set_view(&mut self, v: PanelView) {
        self.studio.panel.cards = v == PanelView::Cards;
        self.studio.panel.drop = None;
        self.studio_set_detailed(v == PanelView::Detailed);
    }

    /// The statistics button says what a tap shows next: Off, Mean + sigma, Min Max.
    pub(super) fn panel_stats_label(&self) -> &'static str {
        match self.studio.panel.stats_mode { 1 => "Mean + \u{3c3}", 2 => "Min Max", _ => "Stats off" }
    }

    pub(super) fn panel_stats_cycle(&mut self) {
        self.panel_stats_set((self.studio.panel.stats_mode + 1) % 3);
    }

    pub(super) fn panel_stats_set(&mut self, mode: u8) {
        self.studio.panel.stats_mode = mode.min(2);
        self.studio.panel.stats = mode != 0;
        self.relayout();
    }

    /// The rows of the ... list: the controls that did not fit the bar. The gate and the statistics are there with
    /// their choices, the one in use marked, so one tap chooses. They were one row each that said the choice in use:
    /// a tap on the statistics went to the next one and closed the list, and the list had to be opened again to
    /// go on or to see what came (reported from a unit, 2026-11-18).
    pub(super) fn panel_more_rows(&self) -> Vec<(String, bool, MoreAct)> {
        let hidden = self.studio_panel_geo().map(|g| self.panel_bar(g.p).hidden).unwrap_or_default();
        let mut v = Vec::new();
        for it in hidden {
            match it {
                BarItem::Gate => for full in [true, false] {
                    v.push((format!("Gate: {}", if full { "Full record" } else { "Screen" }), self.m.meas_full_record == full, MoreAct::Gate(full)));
                },
                BarItem::Stats => for (mode, name) in [(0u8, "Statistics off"), (1, "Mean + \u{3c3}"), (2, "Min Max")] {
                    v.push((name.to_string(), self.studio.panel.stats_mode == mode, MoreAct::Stats(mode)));
                },
                _ => v.push((self.panel_item_label(it), self.panel_item_on(it), MoreAct::Item(it))),
            }
        }
        v
    }

    /// The panel `p` (under its title strip) has its controls in the strip.
    pub(super) fn panel_bar_wide(p: Rect) -> bool { p.w >= WIDE_BAR_FROM }

    /// Height the controls take from the content: the slim toolbar of a narrow panel, nothing for a wide one.
    pub(super) fn panel_tool_h(p: Rect) -> i32 { if Self::panel_bar_wide(p) { 0 } else { TOOL_H } }

    /// The controls of the panel `p`: as many as fit, whole; the others are behind the ... button.
    pub(super) fn panel_bar(&self, p: Rect) -> PanelBar {
        let wide = Self::panel_bar_wide(p);
        // the row of the bar: the title strip, or the toolbar
        let (y, h) = if wide { (p.y - STRIP_H + 2, STRIP_H - 4) } else { (p.y + 2, TOOL_H - 4) };
        let (hit_y, hit_h) = if wide { (p.y - STRIP_H, 40) } else { (p.y, TOOL_H) };
        let right = if wide {
            let r = Rect::new(p.x, p.y - STRIP_H, p.w, p.h + STRIP_H);
            self.dock_view_rect(VKind::Measure, r).map_or(p.right() - 100, |v| v.x) - 6
        } else { p.right() - 3 };
        let left = if wide { p.x + 140 } else { p.x + 3 };
        let slot = |item, x: i32, w: i32| BarSlot { item, r: Rect::new(x, y, w, h), hit: Rect::new(x - 1, hit_y, w + 2, hit_h) };
        if self.studio.panel.searching {
            let xw = 30;
            let (fx, fw) = if wide { (p.x + 28, right - p.x - 28) } else { (p.x + 3, p.w - 6) };
            let field = Rect::new(fx, y, fw.max(60), h);
            let close = Rect::new(field.right() - xw, y, xw, h);
            return PanelBar { slots: Vec::new(), more: None, hidden: Vec::new(), wide, full: false, field: Some((field, close)), left: fx };
        }
        let width = |full: bool, it: BarItem| match (it, full) {
            (BarItem::Source, true) => 104, (BarItem::Source, false) => 56,
            (BarItem::Gate, true) => 84, (BarItem::Gate, false) => 56,
            (BarItem::Stats, true) => 74,
            _ => 34,
        };
        let gap = 3;
        let avail = right - left;
        let sum = |full: bool, keep: &[BarItem], more: bool| -> i32 {
            keep.iter().map(|&i| width(full, i) + gap).sum::<i32>() + if more { MORE_W + gap } else { 0 } - gap
        };
        let all: Vec<BarItem> = BAR_ORDER.to_vec();
        let (full, keep, more) = if sum(true, &all, false) <= avail { (true, all, false) }
            else if sum(false, &all, false) <= avail { (false, all, false) }
            else {
                let mut keep = all;
                for d in BAR_DROP {
                    keep.retain(|&i| i != d);
                    if sum(false, &keep, true) <= avail { break; }
                }
                (false, keep, true)
            };
        let hidden: Vec<BarItem> = BAR_ORDER.into_iter().filter(|i| !keep.contains(i)).collect();
        let total = sum(full, &keep, more);
        let mut x = if wide { right - total } else { left };
        let start = x;
        let mut slots = Vec::new();
        for it in keep {
            let w = width(full, it);
            slots.push(slot(it, x, w));
            x += w + gap;
        }
        let more = more.then(|| slot(BarItem::Add, x, MORE_W));
        PanelBar { slots, more, hidden, wide, full, field: None, left: start }
    }

    /// The control under a point; `Some(None)` is the ... button (or the search field).
    pub(super) fn panel_bar_hit(&self, p: Rect, x: i32, y: i32) -> Option<Option<BarItem>> {
        let bar = self.panel_bar(p);
        let (hy, hh) = if bar.wide { (p.y - STRIP_H, 40) } else { (p.y, TOOL_H) };
        if let Some((field, close)) = bar.field {
            if Rect::new(close.x, hy, close.w, hh).contains(x, y) { return Some(Some(BarItem::Search)); }
            return Rect::new(field.x, hy, field.w, hh).contains(x, y).then_some(None);
        }
        if let Some(s) = bar.slots.iter().find(|s| s.hit.contains(x, y)) { return Some(Some(s.item)); }
        bar.more.filter(|m| m.hit.contains(x, y)).map(|_| None)
    }

    /// The rectangle a dropdown opens under: its control, or the ... button when the control is behind it.
    fn panel_anchor(&self, p: Rect, which: PanelDrop) -> Option<Rect> {
        let bar = self.panel_bar(p);
        let item = match which { PanelDrop::Source => BarItem::Source, PanelDrop::Gate => BarItem::Gate, PanelDrop::More => return bar.more.map(|m| m.r) };
        bar.slots.iter().find(|s| s.item == item).map(|s| s.r).or(bar.more.map(|m| m.r))
    }

    /// What a control does when it is tapped.
    pub(super) fn panel_bar_do(&mut self, it: BarItem) {
        match it {
            BarItem::Source => self.studio.panel.drop = Some(PanelDrop::Source),
            BarItem::Gate => self.studio.panel.drop = Some(PanelDrop::Gate),
            BarItem::Search => self.panel_search_open(),
            BarItem::Pinned => {
                self.studio.panel.pinned_only = !self.studio.panel.pinned_only;
                self.studio.panel.scroll_top();
            }
            BarItem::Add => self.open_meas_list(),
            BarItem::Stats => self.panel_stats_cycle(),
        }
    }

    /// The search icon: the field opens in place of the title, with the keyboard.
    pub(super) fn panel_search_open(&mut self) {
        self.studio.panel.searching = true;
        self.studio.panel.query_osk = true;
        let q = self.studio.panel.query.clone();
        self.wlan.osk = Some(wlan::Osk::for_search(&q));
    }

    /// The close button of the field: the search is cleared and the title is back.
    pub(super) fn panel_search_close(&mut self) {
        self.studio.panel.searching = false;
        self.studio.panel.query.clear();
        self.studio.panel.scroll_top();
    }

    fn panel_bar_tap_item(&mut self, hit: Option<BarItem>) {
        match hit {
            Some(BarItem::Search) if self.studio.panel.searching => self.panel_search_close(),
            Some(it) => self.panel_bar_do(it),
            None if self.studio.panel.searching => self.panel_search_open(),
            None => self.studio.panel.drop = Some(PanelDrop::More),
        }
    }

    /// The sources the rows come from, in the order they first appear.
    pub(super) fn panel_sources(&self) -> Vec<WaveId> {
        let mut v: Vec<WaveId> = Vec::new();
        for r in self.meas_rows().iter().filter(|r| !self.meas_row_hidden(r)) {
            if !v.contains(&r.src) { v.push(r.src); }
        }
        v
    }

    /// A control's name in the ... list.
    fn panel_item_label(&self, it: BarItem) -> String {
        match it {
            BarItem::Source => format!("Source: {}", self.panel_source_label()),
            BarItem::Gate => format!("Gate: {}", self.panel_gate_label()),
            BarItem::Search => "Search".to_string(),
            BarItem::Pinned => "Pinned only".to_string(),
            BarItem::Add => "Add measurement".to_string(),
            BarItem::Stats => self.panel_stats_label().to_string(),
        }
    }

    fn panel_item_on(&self, it: BarItem) -> bool {
        let st = &self.studio.panel;
        match it {
            BarItem::Source => self.studio.filter.is_some(),
            BarItem::Gate | BarItem::Add => false,
            BarItem::Search => !st.query.is_empty(),
            BarItem::Pinned => st.pinned_only,
            BarItem::Stats => st.stats_mode != 0,
        }
    }

    fn panel_drop_options(&self, which: PanelDrop) -> Vec<(String, bool)> {
        match which {
            PanelDrop::Source => {
                let mut v = vec![("All sources".to_string(), self.studio.filter.is_none())];
                for s in self.panel_sources() { v.push((self.wave_name(s), self.studio.filter == Some(s))); }
                v
            }
            PanelDrop::Gate => vec![("Full record".to_string(), self.m.meas_full_record), ("Screen".to_string(), !self.m.meas_full_record)],
            PanelDrop::More => self.panel_more_rows().into_iter().map(|(label, on, _)| (label, on)).collect(),
        }
    }

    pub(super) fn panel_source_label(&self) -> String {
        self.studio.filter.map_or("All sources".to_string(), |id| self.wave_name(id))
    }

    pub(super) fn panel_gate_label(&self) -> &'static str {
        if self.m.meas_full_record { "Full record" } else { "Screen" }
    }

    /// The popup list of the open dropdown: under its button, one row of `MENU_ROW` per option.
    pub(super) fn panel_drop_rect(&self) -> Option<Rect> {
        let which = self.studio.panel.drop?;
        let g = self.studio_panel_geo()?;
        let b = self.panel_anchor(g.p, which)?;
        let n = self.panel_drop_options(which).len() as i32;
        let w = b.w.max(if which == PanelDrop::More { 190 } else { 130 }).min(g.p.w - 8);
        let x = b.x.min(g.p.right() - 4 - w).max(g.p.x + 4);
        let h = n * super::workspace::MENU_ROW + 8;
        Some(Rect::new(x, (b.bottom() + 2).min(self.scr.h - h - 2), w, h))
    }

    pub(super) fn panel_drop_row(p: Rect, i: usize) -> Rect {
        Rect::new(p.x + 4, p.y + 4 + i as i32 * super::workspace::MENU_ROW, p.w - 8, super::workspace::MENU_ROW)
    }

    /// A tap while a dropdown is open: it picks an option or closes. Always taken.
    pub(super) fn panel_drop_tap(&mut self, x: i32, y: i32) -> bool {
        let Some(which) = self.studio.panel.drop else { return false };
        let Some(p) = self.panel_drop_rect() else { self.studio.panel.drop = None; return false };
        self.studio.panel.drop = None;
        if !p.contains(x, y) { return true; }
        let opts = self.panel_drop_options(which);
        let Some(i) = (0..opts.len()).find(|&i| Self::panel_drop_row(p, i).contains(x, y)) else { return true };
        match which {
            PanelDrop::Source => {
                let sources = self.panel_sources();
                self.studio.filter = if i == 0 { None } else { sources.get(i - 1).copied() };
                self.studio.panel.scroll_top();
            }
            PanelDrop::Gate => self.select_option(Item::MeasGate, if i == 0 { "Full record" } else { "Screen" }),
            PanelDrop::More => match self.panel_more_rows().get(i).map(|r| r.2) {
                Some(MoreAct::Item(it)) => self.panel_bar_do(it),
                Some(MoreAct::Gate(full)) => self.select_option(Item::MeasGate, if full { "Full record" } else { "Screen" }),
                Some(MoreAct::Stats(mode)) => self.panel_stats_set(mode),
                None => {}
            },
        }
        true
    }

    /// A tap on the toolbar of a narrow panel. False: not on it. (The bar in the title strip is taken by the dock.)
    pub(super) fn panel_head_tap(&mut self, g: &PanelGeo, x: i32, y: i32) -> bool {
        if Self::panel_bar_wide(g.p) { return false; }
        let Some(hit) = self.panel_bar_hit(g.p, x, y) else { return false };
        self.panel_bar_tap_item(hit);
        true
    }

    /// A tap on the bar in the title strip, from the dock. False: not on a control.
    pub(super) fn panel_strip_tap(&mut self, x: i32, y: i32) -> bool {
        let Some(g) = self.studio_panel_geo() else { return false };
        let Some(hit) = self.panel_bar_hit(g.p, x, y) else { return false };
        self.panel_bar_tap_item(hit);
        true
    }

    /// The text of the search keyboard went to the panel.
    pub(super) fn panel_set_query(&mut self, text: &str) {
        self.studio.panel.query = text.to_string();
        self.studio.panel.query_osk = false;
        self.studio.panel.searching = !text.is_empty();
        self.studio.panel.scroll_top();
    }

    /// "6 results . 3 pinned": only while something filters the list.
    pub(super) fn panel_caption(&self) -> Option<String> {
        let st = &self.studio.panel;
        if st.query.is_empty() && self.studio.filter.is_none() && !st.pinned_only { return None; }
        let rows = self.meas_rows();
        Some(format!("{} results \u{b7} {} pinned", self.studio_listed(&rows).len(), self.studio_pinned().len()))
    }

    /// The controls of the bar (and the search field), drawn where `bar` says.
    pub(super) fn draw_panel_bar(&self, cv: &mut Canvas, bar: &PanelBar) {
        let f = self.fonts();
        let st = &self.studio.panel;
        if let Some((field, close)) = bar.field {
            chip(cv, field, BTN, ACCENT);
            let (cx, cy) = (field.x + 12, field.y + field.h / 2 - 1);
            cv.circle(cx, cy, 4, TEXT);
            cv.line(cx + 3, cy + 3, cx + 7, cy + 7, TEXT);
            let text = Rect::new(field.x + 24, field.y, (close.x - field.x - 26).max(1), field.h);
            cv.set_clip(text);
            if st.query.is_empty() { cv.text_in(&f.s, text, 0, "Search measurements", DIM); } else { cv.text_in(&f.m, text, 0, &st.query, TEXT); }
            cv.no_clip();
            let (xc, yc) = (close.x + close.w / 2, close.y + close.h / 2);
            cv.line(xc - 4, yc - 4, xc + 4, yc + 4, TEXT);
            cv.line(xc - 4, yc + 4, xc + 4, yc - 4, TEXT);
            return;
        }
        for s in bar.slots.iter().chain(bar.more.iter()) {
            let r = s.r;
            let more = bar.more.is_some_and(|m| m.r == r);
            let on = !more && self.panel_item_on(s.item);
            let open = match s.item { BarItem::Source => st.drop == Some(PanelDrop::Source), BarItem::Gate => st.drop == Some(PanelDrop::Gate), _ => false } || (more && st.drop == Some(PanelDrop::More));
            let lit = on && matches!(s.item, BarItem::Pinned | BarItem::Search | BarItem::Stats);
            let fill = if lit { mix(BTN, ACCENT, if s.item == BarItem::Pinned { 70 } else { 35 }) } else { BTN };
            chip(cv, r, fill, if open || on { ACCENT } else { BTN_BORDER });
            let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
            if more {
                for dx in [-6, 0, 6] { cv.circle(cx + dx, cy, 2, TEXT); }
                continue;
            }
            match s.item {
                BarItem::Source | BarItem::Gate => {
                    let label = if s.item == BarItem::Source {
                        if bar.full { self.panel_source_label() } else { self.studio.filter.map_or("All".to_string(), |id| self.wave_name(id)) }
                    } else if bar.full { self.panel_gate_label().to_string() } else if self.m.meas_full_record { "Record".to_string() } else { "Screen".to_string() };
                    let room = r.w - if bar.full { 26 } else { 8 };
                    let font = if f.m.width(&label) <= room { &f.m } else { &f.s };
                    cv.set_clip(Rect::new(r.x + 3, r.y, r.w - if bar.full { 20 } else { 6 }, r.h));
                    cv.text_in(font, Rect::new(r.x + 8, r.y, r.w - 10, r.h), if bar.full { 0 } else { 1 }, &label, if on { ACCENT } else { TEXT });
                    cv.no_clip();
                    if bar.full { chevron_down(cv, r.right() - 14, cy - 3, DIM); }
                }
                BarItem::Search => {
                    let c = if on { ACCENT } else { TEXT };
                    cv.circle(cx - 2, cy - 2, 5, c);
                    cv.circle(cx - 2, cy - 2, 3, fill);
                    cv.line(cx + 2, cy + 2, cx + 6, cy + 6, c);
                    cv.line(cx + 3, cy + 2, cx + 7, cy + 6, c);
                }
                BarItem::Pinned => icon_pin(cv, cx - 6, cy, on, if on { ACCENT } else { TEXT }),
                BarItem::Add => {
                    cv.hline(cx - 5, cx + 5, cy, TEXT);
                    cv.vline(cx, cy - 5, cy + 5, TEXT);
                }
                BarItem::Stats => {
                    let label = if bar.full { self.panel_stats_label() } else { "\u{3c3}" };
                    let font = if f.m.width(label) <= r.w - 6 { &f.m } else { &f.s };
                    cv.set_clip(r.inset(2));
                    cv.text_in(font, r, 1, label, if on { ACCENT } else { TEXT });
                    cv.no_clip();
                }
            }
        }
    }

    /// The toolbar of a narrow panel: its row under the title strip.
    pub(super) fn draw_panel_toolbar(&self, cv: &mut Canvas, g: &PanelGeo) {
        if Self::panel_bar_wide(g.p) { return; }
        let bar = self.panel_bar(g.p);
        self.draw_panel_bar(cv, &bar);
        cv.hline(g.p.x + 4, g.p.right() - 5, g.p.y + TOOL_H - 1, LINE);
    }

    pub(super) fn draw_panel_drop(&self, cv: &mut Canvas) {
        let (Some(which), Some(p)) = (self.studio.panel.drop, self.panel_drop_rect()) else { return };
        let f = self.fonts();
        cv.round(p, 8, ACCENT, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        // the ... list: a line between its groups (the gate, the statistics, each other control)
        let groups: Vec<u8> = if which == PanelDrop::More {
            self.panel_more_rows().iter().map(|r| match r.2 { MoreAct::Gate(_) => 1, MoreAct::Stats(_) => 2, MoreAct::Item(_) => 0 }).collect()
        } else { Vec::new() };
        for (i, (label, cur)) in self.panel_drop_options(which).into_iter().enumerate() {
            let r = Self::panel_drop_row(p, i);
            if cur { cv.round(r, 5, mix(PANEL_BG, ACCENT, 40), 255); }
            cv.set_clip(r);
            cv.text_in(&f.m, Rect::new(r.x + 10, r.y, r.w - 12, r.h), 0, &label, if cur { ACCENT } else { TEXT });
            cv.no_clip();
            if i > 0 && groups.get(i).is_some_and(|g| *g != groups[i - 1]) { cv.hline(p.x + 10, p.right() - 11, r.y, BTN_BORDER); }
        }
    }

    // ------------------------------------------------------------------------------------------------ cards

    /// Columns of cards in a list `w` wide: as many as fit at the width a card needs for its value and its
    /// statistics, two at least (a narrow side panel) and six at most.
    pub(super) fn panel_card_cols(w: i32) -> i32 {
        (w / 168).clamp(2, 6)
    }

    /// The cards that are on the page, in columns. `pos` indexes `listed`.
    pub(super) fn panel_cards_layout(&self, first: usize, total: usize, list: Rect) -> Vec<(usize, Rect)> {
        let (cols, h, gap) = self.panel_cell_geo(list.w);
        let cw = (list.w - 10 - (cols - 1) * gap) / cols;
        let page = self.panel_cards_page(list.w, list.h);
        (first..total.min(first + page)).map(|pos| {
            let k = (pos - first) as i32;
            (pos, Rect::new(list.x + 2 + (k % cols) * (cw + gap), list.y + (k / cols) * (h + gap), cw, h))
        }).collect()
    }

    /// How many cards a page of the list holds.
    pub(super) fn panel_cards_page(&self, list_w: i32, list_h: i32) -> usize {
        let (cols, h, gap) = self.panel_cell_geo(list_w);
        (((list_h + gap) / (h + gap)).max(1) * cols) as usize
    }

    /// The strip view: a cell is one line, the source in its colour, the name of the measurement, and the value at
    /// the right in the colour of the source, as the strip of the Default layout writes them. A pinned one has an
    /// amber edge. A tap on a cell opens the settings of its measurement; a cell has no pin and no Remove of its own
    /// (the cards have the pin, a row of the table is swiped away, a badge has both in its menu).
    pub(super) fn draw_panel_strip(&self, cv: &mut Canvas, g: &PanelGeo, rows: &[tools::MeasRow], keys: &[studio::MeasKey], listed: &[usize], first: usize) {
        let f = self.fonts();
        let pinned = self.studio_pinned();
        for (pos, r) in self.panel_cards_layout(first, listed.len(), g.list) {
            let i = listed[pos];
            let row = &rows[i];
            let col = self.wave_color(row.src);
            cv.set_clip(g.list);
            cv.round(r, 4, BTN, 255);
            if keys.get(i).is_some_and(|k| pinned.contains(k)) { cv.round(Rect::new(r.x, r.y + 4, 3, r.h - 8), 1, AMBER, 255); }
            let src = self.wave_name(row.src);
            let value = row.text();
            let outer = cv.save_clip();
            cv.clip_within(r);
            let x = cv.text(&f.sb, r.x + 8, r.y + 18, &src, col) + 6;
            let vw = f.sb.width(&value);
            cv.text(&f.sb, r.right() - 8 - vw, r.y + 18, &value, col);
            // the long name where it fits beside the value, else the short one of the badges
            let room = r.right() - 8 - vw - 8 - x;
            let short = |long: &str| keys.get(i).map_or_else(|| long.to_string(), |&k| self.studio_badge_name(k, long));
            let name = if f.s.width(&row.name) <= room { row.name.clone() } else { short(&row.name) };
            cv.clip_within(Rect::new(x, r.y, room.max(1), r.h));
            cv.text(&f.s, x, r.y + 18, &name, LABEL);
            cv.restore_clip(outer);
        }
        cv.no_clip();
    }

    /// The pin of a card: top right.
    pub(super) fn panel_card_pin(r: Rect) -> Rect {
        Rect::new(r.right() - 30, r.y, 30, 30)
    }

    /// The cards: three lines each. The name and the pin; the value and, at the right, the source in its colour;
    /// two statistics and the count. No space is kept for anything else, so a panel shows as many as it can.
    pub(super) fn draw_panel_cards(&self, cv: &mut Canvas, g: &PanelGeo, rows: &[tools::MeasRow], keys: &[studio::MeasKey], listed: &[usize], first: usize) {
        let f = self.fonts();
        let pinned = self.studio_pinned();
        // one font for the values of every card of the list, from the width of the cards and the units, not from the
        // values of the moment (on a unit, 2026-11-20, "68.5 mV" stood smaller than "270 mV" on the card beside it)
        let cards = self.panel_cards_layout(first, listed.len(), g.list);
        let vf = match cards.first() {
            Some((_, r)) => Self::card_value_shared(f, r.w, listed.iter().filter_map(|&i| rows.get(i)).map(|row| (row.unit, f.sb.width(&self.wave_name(row.src))))),
            None => &f.lb,
        };
        cv.set_clip(g.list);
        for (pos, r) in cards {
            let i = listed[pos];
            let row = &rows[i];
            let is_pinned = keys.get(i).is_some_and(|k| pinned.contains(k));
            let col = self.wave_color(row.src);
            chip(cv, r, BTN, if is_pinned { mix(BTN_BORDER, AMBER, 90) } else { BTN_BORDER });
            // the name and the pin
            cv.set_clip(Rect::new(r.x + 4, r.y, r.w - 24, 20));
            cv.text(&f.s, r.x + 8, r.y + 15, &self.panel_row_name(keys.get(i).copied(), &row.name), LABEL);
            cv.set_clip(g.list);
            icon_pin_small(cv, r.right() - 16, r.y + 12, is_pinned, if is_pinned { AMBER } else { DIM });
            // the value, and the source at the right of its line: when the value is too wide for both (a card of a
            // side panel is 148 px) it has the whole line, and the source goes to the line under it
            let src = self.wave_name(row.src);
            let sw = f.sb.width(&src);
            let text = row.text();
            let st = self.studio_stat_values(i);
            let stats = self.studio.panel.stats_mode != 0 || st.is_some();
            // without statistics the third line says the source, and the value has the whole line. Beside it by the
            // widest value of the unit, so the source does not come and go as the value runs (on a unit, 2026-11-20,
            // "70.4 mV" lost it and "269 mV" beside it kept it)
            let beside = stats && vf.width(&Self::card_value_widest(row.unit)) <= Self::card_value_beside(r.w, sw);
            cv.set_clip(Rect::new(r.x + 4, r.y + 18, if beside { (r.w - 8 - sw - 10).max(1) } else { r.w - 8 }, 24));
            cv.text(vf, r.x + 8, r.y + 38, &text, TEXT);
            if beside {
                cv.set_clip(Rect::new(r.x + 4, r.y + 18, r.w - 8, 24));
                cv.text(&f.sb, r.right() - 8 - sw, r.y + 37, &src, col);
            } else if !stats {
                // no statistics: the line under the value says where it was measured
                cv.set_clip(Rect::new(r.x + 4, r.y + 40, r.w - 8, 18));
                cv.text(&f.sb, r.x + 8, r.y + 54, &src, col);
            }
            // the statistics: mean and deviation, or minimum and maximum, then the count while there is room
            let mut src_said = beside || !stats;
            if stats {
                let cell = |v: Option<f64>| v.map_or("----".to_string(), |v| row.fmt(v));
                let room = r.w - 16;
                let range = self.studio.panel.stats_mode == 2;
                let (a, b) = if range { (cell(st.map(|s| s.1)), cell(st.map(|s| s.2))) } else { (cell(st.map(|s| s.0)), cell(st.map(|s| s.3))) };
                let line = Self::card_stats_line(&f.s, range, row.unit, &a, &b, room);
                let n = self.results.rows.get(i).map_or(0, |s| s.n);
                // the source takes the place of the count when it did not fit beside the value
                let tail = if beside { if n > 0 { format!("n {n}") } else { String::new() } } else { src.clone() };
                let with_tail = !tail.is_empty() && f.s.width(&line) + 10 + f.sb.width(&tail) <= room;
                cv.set_clip(Rect::new(r.x + 4, r.y + 40, r.w - 8 - if with_tail { f.sb.width(&tail) + 8 } else { 0 }, 18));
                cv.text(&f.s, r.x + 8, r.y + 53, &line, DIM);
                if with_tail {
                    cv.set_clip(Rect::new(r.x + 4, r.y + 40, r.w - 8, 18));
                    cv.text(&f.sb, r.right() - 8 - f.sb.width(&tail), r.y + 53, &tail, if beside { DIM } else { col });
                    src_said |= !beside;
                }
            }
            // no room to say the source in words: the edge of the card has its colour
            if !src_said {
                cv.set_clip(g.list);
                cv.round(Rect::new(r.x, r.y + 6, 3, r.h - 12), 1, col, 255);
            }
            cv.set_clip(g.list);
        }
        cv.no_clip();
    }

    /// The statistics line of a card `room` px wide: minimum and maximum (`range`), or mean and deviation. The form is
    /// the longest that fits values as wide as "-00.0 m" and the unit: "min a  max b", then "a .. b" ("a \u{b1} b"
    /// for the mean), then the same with the unit once. It follows the width of the card and the unit, never the
    /// value of the moment, so a card does not change its form while it runs. Two cards beside each other in a side
    /// panel are 148 px wide: "min 145 mV  max 212 mV" ended at "max" there.
    pub(super) fn card_stats_line(f: &Font, range: bool, unit: &str, a: &str, b: &str, room: i32) -> String {
        let widest = format!("-00.0 m{unit}");
        let long = |a: &str, b: &str| if range { format!("min {a}  max {b}") } else { format!("\u{b5} {a}  \u{3c3} {b}") };
        let short = |a: &str, b: &str| if range { format!("{a} .. {b}") } else { format!("{a} \u{b1} {b}") };
        if f.width(&long(&widest, &widest)) <= room { return long(a, b); }
        if f.width(&short(&widest, &widest)) <= room { return short(a, b); }
        // the unit once: the first value without its unit, when both end in the same one
        let tail = |s: &str| s.rsplit_once(' ').map(|(n, u)| (n.to_string(), u.to_string()));
        let once = match (tail(a), tail(b)) {
            (Some((an, au)), Some((_, bu))) if au == bu => short(&an, b),
            _ => short(a, b),
        };
        if f.width(&short("-00.0", &widest)) <= room {
            return once;
        }
        // no room for the unit at all: the numbers, whose unit the value above shows
        let num = |s: &str| tail(s).map_or(s.to_string(), |(n, _)| n);
        short(&num(a), &num(b))
    }

    /// The largest font in which a card's value fits `room` px: large, then medium bold, then small bold.
    /// The widest value a card of `unit` shows: three digits with a sign and a prefix.
    pub(super) fn card_value_widest(unit: &str) -> String { format!("-00.0 m{unit}") }

    /// The room of a card's value beside its source tag `sw` px wide, on a card `w` px wide.
    pub(super) fn card_value_beside(w: i32, sw: i32) -> i32 { w - 8 - sw - 10 - 4 }

    /// The one font of the values of cards `w` px wide, given each card's unit and the width of its source tag: the
    /// largest in which the widest value of every unit ("-00.0 m" and the unit) fits beside its tag, else the largest
    /// in which it fits across the line, else the smallest.
    pub(super) fn card_value_shared<'a>(f: &'a crate::font::Fonts, w: i32, cards: impl Iterator<Item = (&'static str, i32)>) -> &'a Font {
        let cards: Vec<(String, i32)> = cards.map(|(unit, sw)| (Self::card_value_widest(unit), sw)).collect();
        let fonts = [&f.lb, &f.mb, &f.sb];
        fonts.into_iter().find(|ft| cards.iter().all(|(v, sw)| ft.width(v) <= Self::card_value_beside(w, *sw)))
            .or_else(|| fonts.into_iter().find(|ft| cards.iter().all(|(v, _)| ft.width(v) <= w - 12)))
            .unwrap_or(&f.sb)
    }

    /// The name a row shows: for a measurement of one source, its name (pair and catalogue names are whole already).
    pub(super) fn panel_row_name(&self, _key: Option<studio::MeasKey>, long: &str) -> String {
        long.to_string()
    }
}

/// The pin of a measurement card: about two thirds of the toolbar's, so it sits on the name line (the owner, 2026-11-20:
/// the pin was too big beside the name).
fn icon_pin_small(cv: &mut Canvas, x: i32, cy: i32, filled: bool, c: Rgb) {
    let (cap, barrel, rim) = (Rect::new(x + 2, cy - 6, 8, 3), Rect::new(x + 4, cy - 3, 4, 3), Rect::new(x + 2, cy, 8, 2));
    if filled {
        cv.fill(cap, c);
        cv.fill(barrel, c);
    } else {
        cv.round_outline(cap, 1, c);
        cv.round_outline(barrel, 1, c);
    }
    cv.fill(rim, c);
    cv.vline(x + 5, cy + 2, cy + 6, c);
    cv.vline(x + 6, cy + 2, cy + 5, c);
}
