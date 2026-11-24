// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio "Add": one side panel for everything that can be put next to the waveform. The first page lists what can be
//! added (Cursors, Measure, Search, Results table, Plot, Math, Ref); each kind has a page with a small picture, one
//! line about it, its types and sources, and Add. The panel is the Settings Sheet, so the waveform stays visible and
//! the panel stays open: several things can be added in a row.
//!
//! Also here: the cursor and search settings pages (rows are `Item`s like every other sheet page), the cursor
//! readouts (a box at each cursor and a delta box) and the search itself (its own source, type and settings).
use super::meascatalog::wrap_lines;
use super::sheet::{SHEET_FOOT, SHEET_HEAD, SHEET_ROW};
use super::sheet::PanelKind;
use super::studio::chip;
use super::*;
use std::sync::Arc;

const PAD: i32 = 12;
/// Height of one row of the first page (the whole row is the target).
const RAIL_ROW: i32 = 56;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum AddKind {
    Cursors,
    Measure,
    Search,
    Table,
    Plot,
    Math,
    Ref,
    Bus,
}

/// What the first page lists: the kind, its name and the line under it.
pub(super) const ADD_KINDS: [(AddKind, &str, &str); 5] = [
    (AddKind::Cursors, "Cursors", "Two cursors read time and level"),
    (AddKind::Measure, "Measure", "Amplitude, period, rise time and more"),
    (AddKind::Search, "Search", "Mark every edge on the waveform"),
    (AddKind::Table, "Results table", "Measurements, search events, decode"),
    (AddKind::Plot, "Plot", "XY, eye, histogram, spectrum, trend"),
];

/// Table types of the Results table page: name and what it shows.
pub(super) const TABLES: [(&str, &str); 3] = [
    ("Measurements", "Every measurement with its statistics, docked at the right."),
    ("Search events", "The time of every search event; tap a row to go there."),
    ("Decoder events", "What the decoders read, one row an event, under the waveform."),
];

/// The kinds of cursors of the Cursors page, as the Cursor menu of the Default layout names its modes: the name, what
/// it reads, the mode.
pub(super) const CURSORS: [(&str, &str, CursorMode); 3] = [
    ("Track", "Two cursors on the chosen trace: time and level at each and the differences.", CursorMode::Track),
    ("Time", "Two vertical cursors: the time at each, the time between them and its rate.", CursorMode::Time),
    ("Volt", "Two horizontal cursors: the level at each and the difference between them.", CursorMode::Volt),
];

/// Plot types of the Plot page.
pub(super) const PLOTS: [(&str, &str, PanelKind); 5] = [
    ("XY", "One channel against another: pick X and Y.", PanelKind::Cursor),
    ("Eye", "An eye diagram beside the waveform.", PanelKind::Eye),
    ("Histogram", "How often each level occurs, drawn in the waveform.", PanelKind::Histogram),
    ("Spectrum", "The frequency content, under the waveform.", PanelKind::Spectrum),
    ("Trend", "A measurement over time.", PanelKind::Trend),
];

/// What the Add panel remembers: the page, the chosen types and the chosen source.
#[derive(Default)]
pub(super) struct AddUi {
    pub kind: Option<AddKind>,
    pub table: usize,
    pub plot: usize,
    /// the kind of cursors chosen on the Cursors page while they are off (`CURSORS`)
    pub cursor: usize,
    pub src: Option<WaveId>,
    /// the second source of a page: the Y of XY
    pub src_b: Option<WaveId>,
    /// the measurement chip of the Trend page
    pub meas: usize,
}

/// The buttons at the foot of a page.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RailAct {
    Add,
    Remove,
    Settings,
    /// Measure: open the list of measurements
    Choose,
}

/// A search of Studio: what it looks for (one of `SEARCH_TYPES`) on its own source, with the settings the trigger has
/// for that type.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) struct SearchSpec {
    pub src: WaveId,
    /// the level; the upper one of Slope and Runt
    pub level: f32,
    /// 0 rising, 1 falling, 2 either; of Pulse and Runt the polarity, 0 positive, 1 negative
    pub slope: u8,
    pub kind: TrigType,
    /// the lower level of Slope and Runt
    pub level2: f32,
    pub when: WhenKind,
    /// the time, or the lower limit of Inside and Outside; the upper limit
    pub t1: f32,
    pub t2: f32,
    /// Equal (with `when` Inside) or Not equal (Outside): within `EQ_TOL` of `t1`, or not
    pub eq: bool,
    /// the marks are not drawn; the search, its count and its events stay
    pub hidden: bool,
    /// Bus: what of the active decoder's packets it marks, and the value an ID or data is compared with
    pub bus: BusWhat,
    pub val: u64,
}

/// How many searches Studio has at once: Search 1 to 3. The top bar has room for three counts, and each has a colour
/// of its own that is no channel's.
pub(super) const SEARCH_SLOTS: usize = 3;
/// The colour of each search: its marks, its count in the top bar.
pub(super) const SEARCH_COLS: [Rgb; SEARCH_SLOTS] = [AMBER, BLUE, GREEN];

impl super::studio::Studio {
    /// The selected search.
    pub fn search(&self) -> Option<SearchSpec> {
        self.searches[self.search_sel]
    }

    /// Changes the selected search (there is one).
    pub fn search_put(&mut self, sp: SearchSpec) {
        self.searches[self.search_sel] = Some(sp.fit());
    }

    /// Selects search `slot` when it is there, else the first that is.
    pub fn search_select(&mut self, slot: usize) {
        let there = |i: &usize| self.searches.get(*i).is_some_and(Option::is_some);
        self.search_sel = Some(slot).filter(there).or_else(|| (0..SEARCH_SLOTS).find(there)).unwrap_or(0);
    }

    /// The searches that are there, with their place (Search 1 is 0).
    pub fn search_list(&self) -> impl Iterator<Item = (usize, SearchSpec)> + '_ {
        self.searches.iter().enumerate().filter_map(|(i, s)| Some((i, (*s)?)))
    }
}

/// The rectangles of the Add panel.
pub(super) struct RailGeo {
    pub rows: Vec<(AddKind, Rect)>,
    pub preview: Rect,
    pub desc: Rect,
    pub types: Vec<(usize, Rect)>,
    pub srcs: Vec<(WaveId, Rect)>,
    /// the second row of sources (the Y of XY)
    pub srcs_b: Vec<(WaveId, Rect)>,
    /// the measurement chips of the Trend page
    pub meas: Vec<(usize, Rect)>,
    /// the labels of the source rows ("X", "Y"), with the room they take at the left
    pub src_labels: Vec<(&'static str, Rect)>,
    pub note: Rect,
    pub foot: Vec<(RailAct, Rect)>,
}

const SLOPES: [&str; 3] = ["Rising", "Falling", "Either"];
const POLS: [&str; 2] = ["Positive", "Negative"];
/// What a search looks for: the trigger's types of one source that the software trigger finds in a record.
pub(super) const SEARCH_TYPES: [TrigType; 7] = [TrigType::Edge, TrigType::Pulse, TrigType::Slope, TrigType::Timeout, TrigType::Runt, TrigType::Window, TrigType::Bus];
/// Where a Window search marks: the signal comes between the two levels, leaves them, or was between them for a time.
const WINS: [&str; 3] = ["Enter", "Exit", "Inside"];
/// A time is equal to the limit within this part of it: no two pulses of a record are the same to the sample.
pub(super) const EQ_TOL: f32 = 0.05;
/// The time conditions by their number (`WhenKind as usize`, which a view stores), and their names in the rows.
pub(super) const WHENS: [WhenKind; 5] = [WhenKind::Off, WhenKind::Gt, WhenKind::Lt, WhenKind::In, WhenKind::Out];
/// The names of the time conditions by their number (`SearchSpec::cond`).
const COND_NAMES: [&str; 7] = ["Off", ">", "<", "Inside", "Outside", "Equal", "Not equal"];

/// The types that look at pulses and have a polarity where the others have a slope.
pub(super) fn polar(kind: TrigType) -> bool {
    matches!(kind, TrigType::Pulse | TrigType::Runt)
}

impl SearchSpec {
    /// An edge search; the settings of the other types are the trigger's defaults.
    pub fn edge(src: WaveId, level: f32, slope: u8) -> Self {
        SearchSpec { src, level, slope, kind: TrigType::Edge, level2: level, when: WhenKind::Lt, t1: 20e-6, t2: 40e-6, eq: false, hidden: false, bus: BusWhat::Frame, val: 0 }
    }

    /// The names of the slope row: slopes, the two polarities of Pulse and Runt, or where a Window search marks.
    pub fn slopes(&self) -> &'static [&'static str] {
        if self.kind == TrigType::Window { &WINS } else if polar(self.kind) { &POLS } else { &SLOPES }
    }

    /// The time condition as one number, which a view stores and the row lists: `WhenKind as u8`, 5 for Equal and
    /// 6 for Not equal.
    pub fn cond(&self) -> u8 {
        match (self.eq, self.when) {
            (true, WhenKind::In) => 5,
            (true, WhenKind::Out) => 6,
            _ => self.when as u8,
        }
    }

    /// The search as one of `kind`. A Window search starts at Enter: the slope it had says nothing about a window.
    pub fn of_kind(self, kind: TrigType) -> Self {
        let slope = if kind == TrigType::Window && self.kind != kind { 0 } else { self.slope };
        SearchSpec { kind, slope, ..self }
    }

    pub fn with_cond(self, cond: u8) -> Self {
        let (when, eq) = match cond {
            5 => (WhenKind::In, true),
            6 => (WhenKind::Out, true),
            c => (WHENS[(c as usize).min(4)], false),
        };
        SearchSpec { when, eq, ..self }
    }

    /// The conditions of the type in the order of the row: Equal and Not equal after the two of one limit.
    pub fn conds(&self) -> Vec<u8> {
        let mut v: Vec<u8> = self.whens().iter().map(|&w| w as u8).collect();
        if let Some(at) = v.iter().position(|&c| c == WhenKind::Lt as u8) { v.splice(at + 1..at + 1, [5, 6]); }
        v
    }

    /// The time conditions of the type: Off is Runt's alone (any runt), Timeout has its time and no condition.
    pub fn whens(&self) -> &'static [WhenKind] {
        match self.kind {
            TrigType::Runt => &WHENS,
            TrigType::Pulse | TrigType::Slope => &WHENS[1..],
            // (a Window search has a time only for how long the signal was inside)
            TrigType::Window if self.slope == 2 => &WHENS[1..],
            _ => &[],
        }
    }

    /// The settings as the type takes them: a polarity is not Either, only Runt is without a time condition, the
    /// lower level and the lower limit are not above the upper ones (a type of one level has no lower one).
    pub fn fit(mut self) -> Self {
        self.val = self.val.min(BUS_VAL_MAX);
        if !SEARCH_TYPES.contains(&self.kind) { self.kind = TrigType::Edge; }
        if self.slope as usize >= self.slopes().len() { self.slope = 0; }
        if self.when == WhenKind::Off && self.kind != TrigType::Runt { self.when = WhenKind::Gt; }
        self.level2 = if self.kind.two_level() { self.level2.min(self.level) } else { self.level };
        if !(self.t1 > 0.0) { self.t1 = 20e-6; }
        self.eq &= matches!(self.when, WhenKind::In | WhenKind::Out);
        self.t2 = self.t2.max(self.t1);
        self
    }

    /// The condition for the software trigger, on its first channel.
    pub fn trigger(&self) -> oos_dsp::trigger::Trigger {
        use oos_dsp::trigger::{Slope as S, Trigger as T, When};
        let slope = [S::Rising, S::Falling, S::Either][self.slope.min(2) as usize];
        let pol = if self.slope == 1 { Pol::Neg } else { Pol::Pos };
        let when = if self.eq { self.when.cond(self.t1 * (1.0 - EQ_TOL), self.t1 * (1.0 + EQ_TOL)) } else { self.when.cond(self.t1, self.t2) };
        let timed = when.unwrap_or(When::Gt(0.0));
        let (src, level, lo, hi) = (0, self.level, self.level.min(self.level2), self.level.max(self.level2));
        match self.kind {
            TrigType::Pulse => T::Pulse { src, pol, level, when: timed },
            TrigType::Slope => T::Slope { src, slope, lo, hi, when: timed },
            TrigType::Timeout => T::Timeout { src, slope, level, time: self.t1 as f64 },
            TrigType::Runt => T::Runt { src, pol, lo, hi, when },
            TrigType::Window => {
                use oos_dsp::trigger::WinWhen;
                T::Window { src, lo, hi, when: [WinWhen::Enter, WinWhen::Exit, WinWhen::Inside(timed)][self.slope.min(2) as usize] }
            }
            _ => T::Edge { src, slope, level },
        }
    }
}

/// Search 1 stops at this many events; the count then says "+".
pub(super) const SEARCH_MAX: usize = 100_000;

impl Ui {
    // ------------------------------------------------------------------------------------------------ the pages

    /// What a page has types for: the names, and the one chosen.
    fn rail_types(&self, kind: AddKind) -> (Vec<&'static str>, usize) {
        match kind {
            AddKind::Table => (TABLES.iter().map(|t| t.0).collect(), self.sheet.add.table.min(TABLES.len() - 1)),
            AddKind::Plot => (PLOTS.iter().map(|t| t.0).collect(), self.sheet.add.plot.min(PLOTS.len() - 1)),
            AddKind::Cursors => (CURSORS.iter().map(|t| t.0).collect(), self.rail_cursor_kind()),
            _ => (Vec::new(), 0),
        }
    }

    /// The kind of cursors the Cursors page stands at: the one on the screen, else the one chosen on the page.
    pub(super) fn rail_cursor_kind(&self) -> usize {
        CURSORS.iter().position(|c| c.2 == self.m.cursor).unwrap_or(self.sheet.add.cursor.min(CURSORS.len() - 1))
    }

    /// The source chosen on the Cursors and Search pages: the one tapped, else the selected waveform.
    pub(super) fn rail_source(&self) -> Option<WaveId> {
        let shown = self.sheet_shown_sources();
        self.sheet.add.src.filter(|s| shown.contains(s))
            .or_else(|| Some(self.cursor_source()).filter(|s| shown.contains(s)))
            .or_else(|| shown.first().copied())
    }

    /// The source rows of a page: a label for the row ("" for none) and the waves it offers. Cursors and Search
    /// pick one source; the plots pick what they draw: XY has X and Y, Trend a source and a measurement.
    fn rail_src_rows(&self, kind: AddKind) -> Vec<(&'static str, Vec<WaveId>)> {
        let shown = self.sheet_shown_sources();
        let chans: Vec<WaveId> = shown.iter().copied().filter(|id| matches!(id, WaveId::Ch(_))).collect();
        match kind {
            AddKind::Cursors | AddKind::Search => vec![("", shown)],
            AddKind::Plot => match self.sheet.add.plot.min(PLOTS.len() - 1) {
                0 => vec![("X", chans.clone()), ("Y", chans)],
                2 | 4 => vec![("", chans)],
                _ => vec![("", shown)],
            },
            _ => Vec::new(),
        }
    }

    /// What the chips of a Plot page say is chosen: the tapped one, else what the plot uses now, else the first on.
    pub(super) fn rail_plot_choice(&self, row: usize) -> Option<WaveId> {
        let rows = self.rail_src_rows(AddKind::Plot);
        let cands = &rows.get(row)?.1;
        let plot = self.sheet.add.plot.min(PLOTS.len() - 1);
        let tapped = if row == 0 { self.sheet.add.src } else { self.sheet.add.src_b };
        let now = match (plot, row) {
            (0, 0) => Some(WaveId::Ch(self.xy_src[0])),
            (0, _) => Some(WaveId::Ch(self.xy_src[1])),
            (1, _) => Some(self.m.eye_src),
            (2, _) => self.hist_src().map(WaveId::Ch),
            (3, _) => Some(self.m.spec_src),
            _ => Some(self.cursor_source()),
        };
        tapped.filter(|s| cands.contains(s))
            .or(now.filter(|s| cands.contains(s)))
            .or_else(|| {
                // Y defaults to a channel other than X
                let x = if row == 1 { self.rail_plot_choice(0) } else { None };
                cands.iter().copied().find(|c| Some(*c) != x).or_else(|| cands.first().copied())
            })
    }

    /// The measurement chips of the Trend page.
    pub(super) const TREND_MEAS: [Meas; 4] = [Meas::Vpp, Meas::Freq, Meas::Vrms, Meas::Vavg];

    fn rail_has_meas_chips(&self, kind: AddKind) -> bool {
        kind == AddKind::Plot && self.sheet.add.plot.min(PLOTS.len() - 1) == 4
    }

    fn rail_has_sources(&self, kind: AddKind) -> bool { !self.rail_src_rows(kind).is_empty() }

    pub(super) fn rail_geo(&self) -> RailGeo {
        let (r, _, _) = self.sheet_chrome();
        let w = r.w - 2 * PAD;
        let x = r.x + PAD;
        let mut g = RailGeo { rows: Vec::new(), preview: Rect::new(0, 0, 0, 0), desc: Rect::new(0, 0, 0, 0), types: Vec::new(), srcs: Vec::new(), srcs_b: Vec::new(), meas: Vec::new(), src_labels: Vec::new(), note: Rect::new(0, 0, 0, 0), foot: Vec::new() };
        let Some(kind) = self.sheet.add.kind else {
            g.rows = ADD_KINDS.iter().enumerate().map(|(i, (k, _, _))| (*k, Rect::new(x, r.y + SHEET_HEAD + 6 + i as i32 * (RAIL_ROW + 2), w, RAIL_ROW))).collect();
            return g;
        };
        let (names, _) = self.rail_types(kind);
        // the picture gives way a little when the page has two rows of sources
        let rows = self.rail_src_rows(kind);
        let below = (names.len() as i32 + 2) / 3 * 44 + rows.len() as i32 * 44 + if self.rail_has_meas_chips(kind) { 44 } else { 0 } + 46;
        let avail = r.h - SHEET_HEAD - 6 - 6 - 40 - SHEET_FOOT - below;
        let ph = avail.clamp(60, 88);
        let mut y = r.y + SHEET_HEAD + 6;
        g.preview = Rect::new(x, y, w, ph);
        y += ph + 6;
        g.desc = Rect::new(x, y, w, 36);
        y += 40;
        if !names.is_empty() {
            let cols = 3;
            let cw = (w - (cols - 1) * 4) / cols;
            for i in 0..names.len() as i32 {
                g.types.push((i as usize, Rect::new(x + (i % cols) * (cw + 4), y + (i / cols) * 44, cw, 40)));
            }
            y += (names.len() as i32 + cols - 1) / cols * 44 + 4;
        }
        let src_rows = self.rail_src_rows(kind);
        if !src_rows.is_empty() {
            let labelled = src_rows.len() > 1 && !src_rows[0].0.is_empty();
            let lw = if labelled { 22 } else { 0 };
            for (k, (label, cands)) in src_rows.iter().enumerate() {
                let n = cands.len().clamp(1, 8) as i32;
                let cw = (w - lw - (n - 1) * 4) / n;
                let row: Vec<(WaveId, Rect)> = cands.iter().take(8).enumerate().map(|(i, &id)| (id, Rect::new(x + lw + i as i32 * (cw + 4), y, cw, 40))).collect();
                if labelled { g.src_labels.push((*label, Rect::new(x, y, lw, 40))); }
                if k == 0 { g.srcs = row; } else { g.srcs_b = row; }
                y += 44;
            }
        }
        if self.rail_has_meas_chips(kind) {
            // the four usual ones, and "More": the list of every measurement of Display > Trend chart
            let n = Self::TREND_MEAS.len() as i32 + 1;
            let cw = (w - (n - 1) * 4) / n;
            g.meas = (0..=Self::TREND_MEAS.len()).map(|i| (i, Rect::new(x + i as i32 * (cw + 4), y, cw, 40))).collect();
            y += 44;
        }
        g.note = Rect::new(x, y + 2, w, 44);
        let acts = self.rail_acts(kind);
        let fy = r.bottom() - SHEET_FOOT + 6;
        g.foot = match acts.len() {
            0 => Vec::new(),
            1 => vec![(acts[0], Rect::new(x, fy, w, 44))],
            _ => {
                let half = (w - 8) / 2;
                vec![(acts[0], Rect::new(x, fy, half, 44)), (acts[1], Rect::new(x + half + 8, fy, half, 44))]
            }
        };
        g
    }

    /// The buttons of a page: Add, or Settings and Remove once it is on the screen.
    fn rail_acts(&self, kind: AddKind) -> Vec<RailAct> {
        match kind {
            AddKind::Cursors if self.m.cursor != CursorMode::Off => vec![RailAct::Settings, RailAct::Remove],
            AddKind::Search if self.studio.searches.iter().all(Option::is_some) => vec![RailAct::Settings, RailAct::Remove],
            // another one while a place is free; a search is removed on its sheet
            AddKind::Search if self.studio.search().is_some() => vec![RailAct::Add, RailAct::Settings],
            AddKind::Table if self.rail_table_shown(self.sheet.add.table) => vec![RailAct::Remove],
            AddKind::Plot if self.rail_plot_shown(self.sheet.add.plot) => {
                if self.rail_plot_menu(self.sheet.add.plot).is_some() { vec![RailAct::Settings, RailAct::Remove] } else { vec![RailAct::Remove] }
            }
            AddKind::Cursors | AddKind::Search | AddKind::Table | AddKind::Plot => vec![RailAct::Add],
            AddKind::Measure => vec![RailAct::Choose],
            _ => Vec::new(),
        }
    }

    /// The table the page opens on: one that is on the screen, else the events of what is on (a decoder, then a
    /// search), else the measurements.
    pub(super) fn rail_table_default(&self) -> usize {
        if let Some(i) = (0..TABLES.len()).rev().find(|&i| self.rail_table_shown(i)) { return i; }
        if self.rail_kind_on(AddKind::Bus) { 2 } else if self.studio.search().is_some() { 1 } else { 0 }
    }

    fn rail_table_shown(&self, i: usize) -> bool {
        match i {
            0 => self.panel_shown(PanelKind::Measurements),
            1 => self.table.kind == Some(tables::TableKind::Search),
            _ => self.events_pane_on(),
        }
    }

    fn rail_plot_shown(&self, i: usize) -> bool {
        let (_, _, k) = PLOTS[i.min(PLOTS.len() - 1)];
        if i == 0 { self.xy } else { self.panel_shown(k) }
    }

    fn rail_plot_menu(&self, i: usize) -> Option<MenuId> {
        match PLOTS[i.min(PLOTS.len() - 1)].0 {
            "Eye" => Some(MenuId::Eye),
            "Spectrum" => Some(MenuId::Spectrum),
            _ => None,
        }
    }

    /// What the line under the buttons says about the state of the page.
    fn rail_note(&self, kind: AddKind) -> String {
        match kind {
            AddKind::Cursors => match self.m.cursor {
                CursorMode::Off => "Cursors are off".to_string(),
                _ => format!("{} cursors on {}", CURSORS[self.rail_cursor_kind()].0, self.wave_name(self.cursor_source())),
            },
            AddKind::Search => match self.studio.search() {
                Some(sp) => format!("Search {} on {}: {}", self.studio.search_sel + 1, self.wave_name(sp.src), self.studio_search_count()),
                None => "No search yet".to_string(),
            },
            AddKind::Table => {
                let i = self.sheet.add.table.min(TABLES.len() - 1);
                if self.rail_table_shown(i) { "On the screen".to_string() } else if i == 2 && !self.rail_kind_on(AddKind::Bus) { "Needs a decoder: Add > Bus".to_string() } else { String::new() }
            }
            AddKind::Plot => if self.rail_plot_shown(self.sheet.add.plot) { "On the screen".to_string() } else { String::new() },
            AddKind::Measure => match self.measurement_count() { 0 => "No measurements yet".to_string(), 1 => "1 measurement on the screen".to_string(), n => format!("{n} measurements on the screen") },
            _ => String::new(),
        }
    }

    // ------------------------------------------------------------------------------------------------ drawing

    pub(super) fn draw_sheet_add_panel(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let kind = self.sheet.add.kind;
        let title = match kind { Some(k) => ADD_KINDS.iter().find(|e| e.0 == k).map_or("Add", |e| e.1), None => "Add" };
        self.sheet_draw_chrome(cv, title);
        let g = self.rail_geo();
        let Some(kind) = kind else {
            for &(k, r) in &g.rows {
                let (_, name, sub) = ADD_KINDS.iter().find(|e| e.0 == k).copied().unwrap();
                let on = self.rail_kind_on(k);
                btn(cv, r, BTN, if on { ACCENT } else { BTN_BORDER });
                cv.set_clip(Rect::new(r.x, r.y, r.w - 30, r.h));
                cv.text(&f.mb, r.x + 12, r.y + 24, name, TEXT);
                cv.text(&f.s, r.x + 12, r.y + 44, sub, DIM);
                cv.no_clip();
                if on { cv.text_in(&f.sb, Rect::new(r.right() - 64, r.y, 40, 30), 2, "On", ACCENT); }
                let (cx, cy) = (r.right() - 14, r.y + r.h / 2);
                cv.line(cx - 3, cy - 6, cx + 3, cy, DIM);
                cv.line(cx + 3, cy, cx - 3, cy + 6, DIM);
            }
            return;
        };
        let id = self.rail_preview_id(kind);
        self.draw_rail_preview(cv, g.preview, id);
        let desc = self.rail_desc(kind);
        for (i, line) in wrap_lines(&f.s, &desc, g.desc.w).into_iter().take(2).enumerate() {
            cv.text(&f.s, g.desc.x, g.desc.y + 14 + i as i32 * 16, &line, DIM);
        }
        let (names, cur) = self.rail_types(kind);
        for &(i, r) in &g.types {
            let on = i == cur;
            if on {
                cv.round(r, 5, rgb(0x2e, 0x2e, 0x32), 255);
                cv.round_outline(r, 5, ACCENT);
            } else {
                btn(cv, r, BTN, BTN_BORDER);
            }
            cv.text_in(&f.sb, r, 1, names[i], if on { TEXT } else { DIM });
        }
        let is_plot = kind == AddKind::Plot;
        let chosen = if is_plot { self.rail_plot_choice(0) } else { self.rail_source() };
        let chosen_b = if is_plot { self.rail_plot_choice(1) } else { None };
        for (row, pick) in [(&g.srcs, chosen), (&g.srcs_b, chosen_b)] {
            for &(sid, r) in row {
                let on = Some(sid) == pick;
                if on { cv.round(r, 5, self.wave_color(sid), 255); } else { btn(cv, r, BTN, self.wave_color(sid)); }
                cv.text_in(&f.mb, r, 1, &self.wave_name(sid), if on { DARK_TEXT } else { TEXT });
            }
        }
        for &(label, r) in &g.src_labels {
            cv.text_in(&f.sb, r, 1, label, DIM);
        }
        let mchosen = self.sheet.add.meas.min(Self::TREND_MEAS.len() - 1);
        for &(i, r) in &g.meas {
            let on = i == mchosen && i < Self::TREND_MEAS.len();
            if on { cv.round(r, 5, rgb(0x2e, 0x2e, 0x32), 255); cv.round_outline(r, 5, ACCENT); } else { btn(cv, r, BTN, BTN_BORDER); }
            if i == Self::TREND_MEAS.len() {
                cv.text_in(&f.sb, r, 1, "More...", TEXT);
                continue;
            }
            cv.text_in(&f.sb, r, 1, Self::TREND_MEAS[i].name(), if on { TEXT } else { DIM });
        }
        if self.rail_has_sources(kind) && g.srcs.is_empty() {
            cv.text_in(&f.m, Rect::new(g.note.x, g.note.y - 44, g.note.w, 40), 0, "No source is on: turn on a channel", DIM);
        }
        let note = self.rail_note(kind);
        if !note.is_empty() {
            cv.text_in(&f.m, g.note, 0, &note, if note.starts_with("Needs") { AMBER } else { TEXT });
        }
        if !g.foot.is_empty() {
            let (r, _, _) = self.sheet_chrome();
            cv.hline(r.x + 6, r.right() - 7, r.bottom() - SHEET_FOOT, LINE);
        }
        for &(act, b) in &g.foot {
            let red = act == RailAct::Remove;
            btn(cv, b, if red { mix(DLG_BG, RED, 30) } else if matches!(act, RailAct::Add | RailAct::Choose) { mix(BTN, ACCENT, 60) } else { BTN }, if red { RED } else if matches!(act, RailAct::Add | RailAct::Choose) { ACCENT } else { BTN_BORDER });
            let label = match act { RailAct::Add => "Add", RailAct::Remove => "Remove", RailAct::Settings => "Settings", RailAct::Choose => "Choose a measurement" };
            cv.text_in(&f.mb, b, 1, label, if red { RED } else { TEXT });
        }
    }

    /// A kind that is on the screen now.
    fn rail_kind_on(&self, k: AddKind) -> bool {
        match k {
            AddKind::Cursors => self.m.cursor != CursorMode::Off,
            AddKind::Measure => self.measurement_count() > 0,
            AddKind::Search => self.studio.search().is_some(),
            AddKind::Table => (0..TABLES.len()).any(|i| self.rail_table_shown(i)),
            AddKind::Plot => (0..PLOTS.len()).any(|i| self.rail_plot_shown(i)),
            AddKind::Math => self.m.math != MathOp::Off,
            AddKind::Ref => self.refs.iter().any(Option::is_some),
            AddKind::Bus => (0..DECODERS).any(|k| self.m.dec_is_on(k)),
        }
    }

    fn rail_preview_id(&self, kind: AddKind) -> &'static str {
        match kind {
            AddKind::Cursors => ["cursors", "cursors_time", "cursors_volt"][self.rail_cursor_kind()],
            AddKind::Search => "search",
            AddKind::Table => ["table_meas", "table_search", "table_decode"][self.sheet.add.table.min(2)],
            AddKind::Plot => ["xy", "eye", "hist", "spectrum", "trend"][self.sheet.add.plot.min(4)],
            AddKind::Measure => "measure",
            AddKind::Math => "math",
            AddKind::Ref => "ref",
            AddKind::Bus => "bus",
        }
    }

    fn rail_desc(&self, kind: AddKind) -> String {
        match kind {
            AddKind::Cursors => CURSORS[self.rail_cursor_kind()].1.into(),
            AddKind::Search => "Marks every edge of the source on the waveform and counts them. Level and slope are set to the signal.".into(),
            AddKind::Table => TABLES[self.sheet.add.table.min(TABLES.len() - 1)].1.into(),
            AddKind::Plot => PLOTS[self.sheet.add.plot.min(PLOTS.len() - 1)].1.into(),
            AddKind::Measure => "Amplitude, frequency, rise time and more, read from a waveform. Pick one and it joins the readout.".into(),
            AddKind::Math => "A waveform calculated from others.".into(),
            AddKind::Ref => "A waveform kept to compare with.".into(),
            AddKind::Bus => "Decode I2C, SPI, UART and the other serial buses.".into(),
        }
    }

    /// The small picture of a page: a dark graticule with a drawing of what the page adds.
    pub(super) fn draw_rail_preview(&self, cv: &mut Canvas, r: Rect, id: &str) {
        let f = self.fonts();
        cv.round(r, 6, LINE, 255);
        let i = r.inset(1);
        cv.fill(i, rgb(0x08, 0x0a, 0x10));
        for k in 1..8 { cv.vline(i.x + i.w * k / 8, i.y + 1, i.bottom() - 2, GRID_LINE); }
        for k in 1..4 { cv.hline(i.x + 1, i.right() - 2, i.y + i.h * k / 4, GRID_LINE); }
        let mid = i.y + i.h / 2;
        let amp = (i.h / 2 - 12) as f32;
        let at = |t: f32| i.x + 4 + (t * (i.w - 8) as f32) as i32;
        let curve = |cv: &mut Canvas, col: Rgb, f: &dyn Fn(f32) -> f32| {
            let mut prev: Option<(i32, i32)> = None;
            for px in 0..=(i.w - 8) {
                let t = px as f32 / (i.w - 8) as f32;
                let p = (i.x + 4 + px, mid - (f(t) * amp) as i32);
                if let Some(q) = prev { cv.line(q.0, q.1, p.0, p.1, col); }
                prev = Some(p);
            }
        };
        let sine = |t: f32| (t * std::f32::consts::TAU * 2.0).sin();
        let square = |t: f32| if ((t * 4.0 + 0.125) % 1.0) < 0.5 { 0.8 } else { -0.8 };
        let yellow = CH_COLORS[0];
        match id {
            "cursors_volt" => {
                curve(cv, yellow, &sine);
                for (v, name) in [(0.3f32, "A"), (0.7f32, "B")] {
                    let y = i.y + (i.h as f32 * v) as i32;
                    let mut x = i.x + 2;
                    while x < i.right() - 3 { cv.hline(x, (x + 3).min(i.right() - 3), y, CURSOR); x += 7; }
                    let b = Rect::new(i.x + 6, y - 16, 40, 14);
                    cv.round(b, 2, CURSOR, 255);
                    cv.text_in(&f.s, b, 1, name, DARK_TEXT);
                }
                let b = Rect::new(i.right() - 60, i.y + i.h / 2 - 8, 54, 16);
                cv.round(b, 2, BAR, 255);
                cv.text_in(&f.s, b, 1, "\u{394}v", TEXT);
            }
            "cursors" | "cursors_time" => {
                curve(cv, yellow, &sine);
                for (t, name) in [(0.3f32, "A"), (0.7f32, "B")] {
                    let x = at(t);
                    cv.dashed_vline(x, i.y + 2, i.bottom() - 3, 4, 3, CURSOR);
                    let b = Rect::new(x + 3, i.y + 4, 40, 14);
                    cv.round(b, 2, CURSOR, 255);
                    cv.text_in(&f.s, b, 1, name, DARK_TEXT);
                }
                let b = Rect::new(i.x + i.w / 2 - 42, i.bottom() - 22, 84, 16);
                cv.round(b, 2, BAR, 255);
                cv.text_in(&f.s, b, 1, if id == "cursors" { "\u{394}t  \u{394}v" } else { "\u{394}t  1/\u{394}t" }, TEXT);
            }
            "search" => {
                curve(cv, yellow, &square);
                for k in 0..4 {
                    let x = at(0.125 + k as f32 * 0.25);
                    cv.triangle([(x - 4, i.y + 3), (x + 4, i.y + 3), (x, i.y + 10)], AMBER);
                    cv.dashed_vline(x, i.y + 11, i.bottom() - 3, 2, 6, mix(BG, AMBER, 90));
                }
            }
            "table_meas" | "table_search" | "table_decode" => {
                for row in 0..5 {
                    let y = i.y + 8 + row * 15;
                    let head = row == 0;
                    cv.fill(Rect::new(i.x + 8, y, i.w - 16, 12), if head { LINE } else { mix(BG, LINE, 90) });
                    let cols: [i32; 4] = if id == "table_decode" { [30, 70, 100, 60] } else { [60, 60, 60, 60] };
                    let mut x = i.x + 14;
                    for (c, wcol) in cols.iter().enumerate() {
                        let col = if head { TEXT } else if id == "table_search" && c == 0 { AMBER } else { DIM };
                        cv.fill(Rect::new(x, y + 5, (*wcol * 2 / 3).max(8), 3), col);
                        x += *wcol + 10;
                    }
                }
            }
            "xy" => {
                let mut prev: Option<(i32, i32)> = None;
                let side = (i.h - 12) / 2;
                for k in 0..=120 {
                    let t = k as f32 / 120.0 * std::f32::consts::TAU;
                    let p = (i.x + i.w / 2 + (t.sin() * side as f32 * 1.4) as i32, mid - ((2.0 * t + 0.8).sin() * side as f32) as i32);
                    if let Some(q) = prev { cv.line(q.0, q.1, p.0, p.1, rgb(0x9c, 0xf0, 0x9c)); }
                    prev = Some(p);
                }
            }
            "eye" => {
                let s = |t: f32| t * t * (3.0 - 2.0 * t);
                for k in 0..6 {
                    let d = (k as f32 - 2.5) * 0.012;
                    curve(cv, yellow, &|t: f32| s((t + d).clamp(0.0, 1.0)) * 1.6 - 0.8);
                    curve(cv, yellow, &|t: f32| 0.8 - s((t + d).clamp(0.0, 1.0)) * 1.6);
                }
                curve(cv, yellow, &|_| 0.8);
                curve(cv, yellow, &|_| -0.8);
            }
            "hist" => {
                for row in 0..9 {
                    let y = i.y + 6 + row * (i.h - 12) / 9;
                    let d = (row as f32 - 4.0) / 2.2;
                    let wbar = ((-d * d).exp() * (i.w as f32 * 0.55)) as i32;
                    cv.fill(Rect::new(i.x + 6, y, wbar.max(3), (i.h - 12) / 9 - 2), mix(BG, yellow, 200));
                }
            }
            "spectrum" => {
                curve(cv, yellow, &|t: f32| -0.75 + 0.1 * (t * 53.0).sin().abs() + if (t - 0.2).abs() < 0.012 { 1.2 } else if (t - 0.4).abs() < 0.012 { 0.7 } else if (t - 0.6).abs() < 0.012 { 0.4 } else { 0.0 });
            }
            "measure" => {
                curve(cv, yellow, &square);
                // the levels a rise time is read between, and the readout that carries the value
                let (lo, hi) = (mid + (amp * 0.8) as i32, mid - (amp * 0.8) as i32);
                for y in [hi, lo] { cv.dashed_hline(i.x + 3, i.right() - 4, y, 3, 3, mix(BG, TEXT, 120)); }
                let b = Rect::new(i.right() - 98, i.y + 6, 92, 16);
                cv.round(b, 3, BAR, 255);
                cv.text_in(&f.s, b, 1, "Pk-Pk 1.6 V", TEXT);
                let b = Rect::new(i.x + 6, i.bottom() - 22, 80, 16);
                cv.round(b, 3, BAR, 255);
                cv.text_in(&f.s, b, 1, "Freq 1 kHz", TEXT);
            }
            "trend" => {
                curve(cv, yellow, &|t: f32| 0.5 * (t * 7.0).sin() + 0.2 * (t * 23.0).sin() - 0.1);
            }
            _ => {}
        }
    }

    // ------------------------------------------------------------------------------------------------ taps

    pub(super) fn sheet_add_panel_tap(&mut self, x: i32, y: i32) -> bool {
        let g = self.rail_geo();
        let Some(kind) = self.sheet.add.kind else {
            if let Some(&(k, _)) = g.rows.iter().find(|(_, r)| r.contains(x, y)) { self.rail_open(k); }
            return true;
        };
        if let Some(&(i, _)) = g.types.iter().find(|(_, r)| r.contains(x, y)) {
            if kind == AddKind::Cursors {
                // cursors that are on change their kind at once; else it is the kind Add puts on
                self.sheet.add.cursor = i;
                if self.m.cursor != CursorMode::Off { self.select_option(Item::CurMode, CURSORS[i].0); }
            } else if kind == AddKind::Table { self.sheet.add.table = i; } else {
                if self.sheet.add.plot != i { (self.sheet.add.src, self.sheet.add.src_b) = (None, None); }
                self.sheet.add.plot = i;
            }
            return true;
        }
        if let Some(&(id, _)) = g.srcs.iter().find(|(_, r)| r.contains(x, y)) {
            self.sheet.add.src = Some(id);
            self.rail_plot_live(kind);
            return true;
        }
        if let Some(&(id, _)) = g.srcs_b.iter().find(|(_, r)| r.contains(x, y)) {
            self.sheet.add.src_b = Some(id);
            self.rail_plot_live(kind);
            return true;
        }
        if let Some(&(i, _)) = g.meas.iter().find(|(_, r)| r.contains(x, y)) {
            if i == Self::TREND_MEAS.len() {
                // every measurement on every source: the picker of Display > Trend chart 1
                self.rail_plot_apply_sources(self.sheet.add.plot);
                self.rail_plot_add(self.sheet.add.plot);
                self.open_picker(Item::TrendSel(0));
                return true;
            }
            self.sheet.add.meas = i;
            self.rail_plot_live(kind);
            return true;
        }
        if let Some(&(act, _)) = g.foot.iter().find(|(_, r)| r.contains(x, y)) { self.rail_do(kind, act); }
        true
    }

    /// A row of the first page: Math and Ref open the page that already exists, the others get their own.
    pub(super) fn rail_open_pub(&mut self, k: AddKind) { self.rail_open(k) }
    pub(super) fn rail_kind_on_pub(&self, k: AddKind) -> bool { self.rail_kind_on(k) }

    fn rail_open(&mut self, k: AddKind) {
        match k {
            AddKind::Math => { self.studio_math_start(); self.open_submenu(MenuId::Math); }
            AddKind::Ref => self.open_submenu(MenuId::Ref),
            AddKind::Bus => self.open_submenu(MenuId::Decode),
            _ => {
                self.sheet.add.kind = Some(k);
                (self.sheet.add.src, self.sheet.add.src_b) = (None, None);
            }
        }
    }

    /// The Choose button of the Measure page: the list of measurements, in the same sheet.
    fn rail_measure_choose(&mut self) {
        self.ensure_meas_own();
        self.open_submenu(MenuId::MeasAdd);
        self.sheet_add_open();
    }

    /// A source tapped while its plot is on the screen changes the plot at once.
    fn rail_plot_live(&mut self, kind: AddKind) {
        if kind == AddKind::Plot && self.rail_plot_shown(self.sheet.add.plot) {
            self.rail_plot_apply_sources(self.sheet.add.plot);
            self.relayout();
        }
    }

    /// Gives the plot the sources the chips show: XY its X and Y, the histogram its channel, the eye and the
    /// spectrum their source, Trend a measurement of the source in its first chart.
    pub(super) fn rail_plot_apply_sources(&mut self, i: usize) {
        let (a, b) = (self.rail_plot_choice(0), self.rail_plot_choice(1));
        match i {
            0 => {
                if let (Some(WaveId::Ch(x)), Some(WaveId::Ch(y))) = (a, b) { self.xy_src = [x, y]; }
            }
            1 => if let Some(id) = a { if id != self.m.eye_src { let n = self.wave_name(id); self.select_option(Item::EyeSrc, &n); } },
            2 => if let Some(WaveId::Ch(c)) = a { self.m.hist_src = Some(c); },
            3 => if let Some(id) = a { if id != self.m.spec_src { let n = self.wave_name(id); self.select_option(Item::SpecSrc, &n); } },
            _ => if let Some(src) = a {
                let k = Self::TREND_MEAS[self.sheet.add.meas.min(Self::TREND_MEAS.len() - 1)];
                if self.meas_add(k, src).is_ok() {
                    if let Some(p) = self.m.meas.iter().position(|e| *e == (k, src)) { self.m.trend_sel[0] = 2 + p; }
                }
            },
        }
    }

    fn rail_do(&mut self, kind: AddKind, act: RailAct) {
        match (kind, act) {
            (AddKind::Cursors, RailAct::Add) => {
                let (s, kind) = (self.rail_source(), self.sheet.add.cursor.min(CURSORS.len() - 1));
                self.studio_cursors_add(s);
                // (the cursors come on the trace; the kind chosen on the page is set through the row of the menu)
                if CURSORS[kind].2 != self.m.cursor { self.select_option(Item::CurMode, CURSORS[kind].0); }
            }
            (AddKind::Cursors, RailAct::Settings) => self.open_submenu(MenuId::Cursor),
            (AddKind::Cursors, RailAct::Remove) => self.studio_cursors_remove(),
            (AddKind::Search, RailAct::Add) => { let s = self.rail_source(); self.studio_search_add(s); }
            (AddKind::Search, RailAct::Settings) => self.open_submenu(MenuId::Search),
            (AddKind::Search, RailAct::Remove) => self.studio_search_remove(),
            (AddKind::Table, RailAct::Add) => self.rail_table_add(self.sheet.add.table),
            (AddKind::Table, RailAct::Remove) => self.rail_table_remove(self.sheet.add.table),
            (AddKind::Plot, RailAct::Add) => { self.rail_plot_apply_sources(self.sheet.add.plot); self.rail_plot_add(self.sheet.add.plot) }
            (AddKind::Measure, RailAct::Choose) => self.rail_measure_choose(),
            (AddKind::Plot, RailAct::Remove) => self.rail_plot_remove(self.sheet.add.plot),
            (AddKind::Plot, RailAct::Settings) => if let Some(m) = self.rail_plot_menu(self.sheet.add.plot) { self.open_submenu(m); },
            _ => {}
        }
        self.relayout();
    }

    fn rail_table_add(&mut self, i: usize) {
        match i {
            0 => self.studio_set_view(super::studio::StudioView::Docked),
            1 => {
                if self.studio.search().is_none() {
                    let s = self.rail_source();
                    self.studio_search_add(s);
                }
                self.table.kind = Some(tables::TableKind::Search);
                self.table.top = 0;
            }
            _ => {
                if !self.rail_kind_on(AddKind::Bus) { return self.toast("Turn a decoder on first: Add > Bus"); }
                self.set_events_pane(true);
            }
        }
    }

    fn rail_table_remove(&mut self, i: usize) {
        match i {
            0 => self.studio_set_view(super::studio::StudioView::Waveform),
            1 => self.table.kind = None,
            // the events of the decoders are the Events pane
            _ => self.set_events_pane(false),
        }
    }

    /// A row of the Windows column of the Add menu: the plot (an index of `PLOTS`) is on the screen, or it is put on /
    /// taken off by one tap.
    pub(super) fn rail_plot_on(&self, i: usize) -> bool { self.rail_plot_shown(i) }
    pub(super) fn rail_plot_toggle(&mut self, i: usize) {
        if self.rail_plot_shown(i) { self.rail_plot_remove(i) } else { self.rail_plot_add(i) }
        self.relayout();
    }
    /// The same for the events of the decoders (an index of `TABLES`).
    pub(super) fn rail_table_on(&self, i: usize) -> bool { self.rail_table_shown(i) }
    pub(super) fn rail_table_toggle(&mut self, i: usize) {
        if self.rail_table_shown(i) { self.rail_table_remove(i) } else { self.rail_table_add(i) }
        self.relayout();
    }

    fn rail_plot_add(&mut self, i: usize) {
        if i == 0 { return self.select_option(Item::XyMode, "XY"); }
        self.studio_apply_panel(PLOTS[i.min(PLOTS.len() - 1)].2);
    }

    fn rail_plot_remove(&mut self, i: usize) {
        match PLOTS[i.min(PLOTS.len() - 1)].0 {
            "XY" => self.select_option(Item::XyMode, "YT"),
            "Eye" => if self.m.view == AView::Eye { self.adjust(Item::EyeOn, 1) },
            "Histogram" => self.select_option(Item::Histogram, "Off"),
            _ => self.select_option(Item::PaneSel, "Traces"),
        }
    }

    // ------------------------------------------------------------------------------------------------ cursors

    /// 1/dt as a frequency; "----" when the cursors sit on the same time.
    pub(super) fn inv_dt(dt: f64) -> String {
        if dt.is_finite() && dt.abs() >= 1e-15 { si(1.0 / dt.abs(), "Hz") } else { "----".into() }
    }

    /// One tap: two waveform cursors on the chosen trace (the selected one when none is given).
    pub(super) fn studio_cursors_add(&mut self, src: Option<WaveId>) {
        if let Some(s) = src { if self.set_cursor_source(s) { self.select_workspace_source(s); } }
        self.m.cur_b_source = None;
        self.m.cursor = CursorMode::Track;
        // apart: a third and two thirds of the visible span
        (self.m.cur_a, self.m.cur_b, self.m.cur_c, self.m.cur_d) = (1.0 / 3.0, 2.0 / 3.0, 1.0 / 3.0, 2.0 / 3.0);
        self.cursor_view = None;
        self.settings_changed();
        self.relayout();
    }

    pub(super) fn studio_cursors_remove(&mut self) {
        self.m.cursor = CursorMode::Off;
        self.m.cur_b_source = None;
        if self.menu_open && self.menu == MenuId::Cursor { self.close_menu(); }
        self.settings_changed();
        self.relayout();
    }

    /// The Cursors button: the first tap puts the cursors on, the next ones open their settings.
    pub(super) fn studio_cursors_tap(&mut self) {
        if self.m.cursor == CursorMode::Off {
            if self.menu_open { self.close_menu(); }
            self.studio_cursors_add(None);
        } else {
            self.toggle_menu(MenuId::Cursor);
        }
    }

    fn set_cursor_type(&mut self, mode: CursorMode) {
        let was = self.m.cursor;
        self.m.cursor = mode;
        let off_view = |f: f32| !(0.0..=1.0).contains(&f);
        if was == CursorMode::Off || (was == CursorMode::Volt) != (mode == CursorMode::Volt) || off_view(self.m.cur_a) || off_view(self.m.cur_b) {
            (self.m.cur_a, self.m.cur_b) = (0.3, 0.7);
        }
        if mode == CursorMode::Xy && was != CursorMode::Xy { (self.m.cur_c, self.m.cur_d) = (0.3, 0.7); }
        self.settings_changed();
        self.relayout();
    }

    pub(super) fn addrail_cursor_items(&self) -> Vec<Item> {
        // the Mode row is the one of the Default menu (Off, Time, Volt, Track, XY); the rows after it are Studio's
        if self.m.cursor == CursorMode::Off { return vec![Item::CurMode, Item::CurSrc]; }
        let mut v = vec![Item::CurMode, Item::CurSrc, Item::CurSecond, Item::CurReadout, Item::CurLink, Item::CurA, Item::CurB];
        if self.m.cursor == CursorMode::Xy { v.extend([Item::CurC, Item::CurD]); }
        if self.m.cursor != CursorMode::Volt { v.extend([Item::CurSnap, Item::CurSnapNow]); }
        v.push(Item::CurRemove);
        v
    }

    /// The cursor readouts at each cursor and the delta box, drawn into the lane `a`.
    pub(super) fn draw_studio_cursor_readouts(&self, cv: &mut Canvas, a: Rect) {
        if self.m.layout != Layout::Studio || self.studio.cur_badge { return; }
        let f = self.fonts();
        let source = self.cursor_source();
        let second = self.cursor_b_source();
        let (ta, tb) = self.cursor_times();
        let (unit, unit_b) = (self.cursor_source_unit(source), self.cursor_source_unit(second));
        let boxed = |cv: &mut Canvas, x: i32, y: i32, text: &str| {
            let w = f.s.width(text) + 10;
            let r = Rect::new(x.clamp(a.x + 2, (a.right() - w - 2).max(a.x + 2)), y, w, 18);
            cv.round(r, 3, BAR, 255);
            cv.round_outline(r, 3, CURSOR);
            cv.text_in(&f.s, r, 1, text, TEXT);
        };
        let mode = self.m.cursor;
        let vertical = matches!(mode, CursorMode::Time | CursorMode::Track | CursorMode::Xy);
        let (mut va, mut vb) = (None, None);
        if vertical {
            if mode == CursorMode::Track {
                va = self.wave_at_source(source, ta);
                vb = self.wave_at_source(second, tb);
            }
            for (k, (frac, t, v, u)) in [(self.m.cur_a, ta, va, unit), (self.m.cur_b, tb, vb, unit_b)].into_iter().enumerate() {
                if !(0.0..=1.0).contains(&frac) { continue; }
                let x = a.x + (frac * a.w as f32) as i32;
                let text = match (mode, v) {
                    (CursorMode::Track, Some(v)) => format!("{}  {}", si(t, "s"), si(v as f64, u)),
                    (CursorMode::Track, None) => format!("{}  ----", si(t, "s")),
                    _ => si(t, "s"),
                };
                // the first value is on the row of the cursor's own tag (X1), so it starts right of it: under the tag
                // a time before the trigger lost its minus sign
                boxed(cv, x + if k == 0 { 30 } else { 3 }, a.y + 4 + k as i32 * 20, &text);
            }
        }
        // the levels of the horizontal bars
        let area = self.source_area(source).unwrap_or(self.plot.area);
        let (scale, offset) = self.cursor_source_context().map_or((1.0, 0.0), |c| (c.1, c.2));
        let mut dv: Option<f64> = None;
        if matches!(mode, CursorMode::Volt | CursorMode::Xy) {
            let (fa, fb) = if mode == CursorMode::Volt { (self.m.cur_a, self.m.cur_b) } else { (self.m.cur_c, self.m.cur_d) };
            let level = |fr: f32| (self.zoom_divs_at(area, fr) * scale - offset) as f64;
            let (la, lb) = (level(fa), level(fb));
            for (fr, l) in [(fa, la), (fb, lb)] {
                let y = a.y + (fr * a.h as f32) as i32;
                boxed(cv, a.x + 4, (y - 20).max(a.y + 2), &si(l, unit));
            }
            dv = Some(lb - la);
        } else if let (Some(x), Some(y)) = (va, vb) {
            if unit == unit_b { dv = Some((y - x) as f64); }
        }
        // the delta box
        let mut lines: Vec<String> = Vec::new();
        if vertical {
            let dt = tb - ta;
            lines.push(format!("\u{394}t {}   1/\u{394}t {}", si(dt, "s"), Self::inv_dt(dt)));
        }
        if let Some(dv) = dv {
            let mut line = format!("\u{394}v {}", si(dv, unit));
            if vertical && (tb - ta).abs() > 0.0 { line.push_str(&format!("   \u{394}v/\u{394}t {}", si(dv / (tb - ta), &format!("{unit}/s")))); }
            lines.push(line);
        }
        if lines.is_empty() { return; }
        let w = lines.iter().map(|l| f.s.width(l)).max().unwrap_or(0) + 16;
        let h = 8 + lines.len() as i32 * 16;
        // both time cursors outside the view: nothing to read here
        let off = |fr: f32| !(0.0..=1.0).contains(&fr);
        if vertical && off(self.m.cur_a) && off(self.m.cur_b) { return; }
        // at the top left of the lane (right of its volts labels), with the colour of its channel
        let r = Rect::new(a.x + 50, a.y + 6, w + 8, h);
        cv.round(r, 4, CURSOR, 255);
        cv.round(r.inset(1), 3, BAR, 255);
        cv.fill(Rect::new(r.x + 3, r.y + 4, 4, r.h - 8), self.wave_color(source));
        for (i, l) in lines.iter().enumerate() { cv.text(&f.s, r.x + 14, r.y + 16 + i as i32 * 16, l, TEXT); }
    }

    /// The cursors in the overview of a zoomed view: a line at each cursor's time.
    pub(super) fn draw_studio_overview_cursors(&self, cv: &mut Canvas, o: Rect) {
        if self.m.layout != Layout::Studio || !matches!(self.m.cursor, CursorMode::Time | CursorMode::Track | CursorMode::Xy) { return; }
        let f = self.fonts();
        let (ta, tb) = self.cursor_times();
        let tdiv = self.m.time_per_div();
        if !(tdiv > 0.0) { return; }
        let ppx = o.w as f32 / HDIV;
        for (t, name) in [(ta, "X1"), (tb, "X2")] {
            let x = o.x + (o.w as f32 / 2.0 + (t as f32 - self.m.delay) / tdiv * ppx) as i32;
            if x < o.x || x >= o.right() { continue; }
            cv.dashed_vline(x, o.y, o.bottom() - 1, 4, 3, CURSOR);
            let b = Rect::new(x + 2, o.y + 2, 24, 14);
            cv.round(b, 2, CURSOR, 255);
            cv.text_in(&f.s, b, 1, name, DARK_TEXT);
        }
    }

    // ------------------------------------------------------------------------------------------------ search

    pub(super) fn addrail_search_items(&self) -> Vec<Item> {
        let sp = self.studio.search();
        // a bus search has no source of its own: it marks what the active decoder decoded
        if let Some(s) = sp.filter(|s| s.kind == TrigType::Bus) {
            let mut v = vec![Item::SrchWhich, Item::SrchType, Item::SrchBusWhat];
            if matches!(s.bus, BusWhat::Id | BusWhat::Data) { v.push(Item::SrchBusVal); }
            v.extend([Item::SrchShow, Item::SrchEvent, Item::SrchFromTrig, Item::SrchToTrig, Item::SearchTable, Item::SrchRemove]);
            return v;
        }
        let mut v = vec![Item::SrchWhich, Item::SrchType, Item::SrchSrc, Item::SrchSlope, Item::SrchLevel];
        if sp.is_some_and(|s| s.kind.two_level()) { v.push(Item::SrchLevel2); }
        v.push(Item::Srch50);
        // the time of the type, as the trigger menu has it: one limit, two for Inside and Outside, none for Runt's Off
        match sp {
            Some(s) if s.kind == TrigType::Timeout => v.push(Item::SrchTime),
            Some(s) if !s.whens().is_empty() => {
                v.push(Item::SrchWhen);
                if s.when != WhenKind::Off { v.push(Item::SrchTime); }
                if matches!(s.when, WhenKind::In | WhenKind::Out) && !s.eq { v.push(Item::SrchTime2); }
            }
            _ => {}
        }
        v.extend([Item::SrchShow, Item::SrchEvent, Item::SrchFromTrig, Item::SrchToTrig, Item::SearchTable, Item::SrchRemove]);
        v
    }

    /// A search is added with the defaults the signal asks for: Edge on `src`, the level at 50 %, every edge. It takes
    /// the selected place when that is free (a remote command names its search), else the first free one, and is the
    /// selected search then.
    pub(super) fn studio_search_add(&mut self, src: Option<WaveId>) {
        let Some(src) = src.or_else(|| self.rail_source()) else { return self.toast("No source is on") };
        let free = |i: &usize| self.studio.searches[*i].is_none();
        let Some(slot) = Some(self.studio.search_sel).filter(free).or_else(|| (0..SEARCH_SLOTS).find(free)) else {
            return self.toast("Three searches are the most");
        };
        self.studio.search_sel = slot;
        self.studio.search_put(SearchSpec::edge(src, 0.0, 2));
        self.studio_search_set_50();
        self.studio.search_at[slot] = 0;
    }

    /// Search `slot` is the one the sheet, the count, previous / next and the list are of.
    pub(super) fn studio_search_select(&mut self, slot: usize) {
        let before = self.studio.search_sel;
        self.studio.search_select(slot);
        if self.studio.search_sel != before { self.table.top = 0; }
    }

    /// Removes the selected search; the first of the others is the selected one then.
    pub(super) fn studio_search_remove(&mut self) {
        let slot = self.studio.search_sel;
        self.studio.searches[slot] = None;
        self.studio.search_at[slot] = 0;
        self.studio.search_select(0);
        self.table.top = 0;
        if self.studio.search().is_some() { return; }
        if self.table.kind == Some(tables::TableKind::Search) && !self.search_on { self.table.kind = None; }
        if self.menu_open && self.menu == MenuId::Search { self.close_menu(); }
    }

    /// The selected search looks for `kind`. A type of two levels gets them from the signal when the search had one.
    pub(super) fn studio_search_set_kind(&mut self, kind: TrigType) {
        let Some(sp) = self.studio.search() else { return };
        self.studio.search_put(sp.of_kind(kind));
        if kind.two_level() && !sp.kind.two_level() { self.studio_search_set_50(); }
        self.studio.search_at[self.studio.search_sel] = 0;
    }

    /// What the source's samples reach: the lowest and the highest.
    fn studio_search_span(&self, src: WaveId) -> Option<(f32, f32)> {
        let (w, _, _) = self.deskew_samples(src)?;
        let (mut lo, mut hi) = (f32::MAX, f32::MIN);
        for &v in w.iter().filter(|v| v.is_finite()) { lo = lo.min(v); hi = hi.max(v); }
        (lo <= hi).then_some((lo, hi))
    }

    /// The level to the middle of the source. The two levels of Slope go to 10 % and 90 % (what a rise time is
    /// measured between), those of Runt and Window to 25 % and 75 % (a pulse that ends between them is a runt).
    pub(super) fn studio_search_set_50(&mut self) {
        let Some(sp) = self.studio.search() else { return };
        if sp.kind == TrigType::Bus { return; }
        let Some((lo, hi)) = self.studio_search_span(sp.src) else { return };
        let at = |f: f32| lo + f * (hi - lo);
        let (level, level2) = match sp.kind {
            TrigType::Slope => (at(0.9), at(0.1)),
            TrigType::Runt | TrigType::Window => (at(0.75), at(0.25)),
            _ => (at(0.5), sp.level2),
        };
        self.studio.search_put(SearchSpec { level, level2, ..sp });
    }

    fn studio_search_vpd(&self, src: WaveId) -> f32 {
        match src {
            WaveId::Ch(c) if c < n_ch() => self.m.ch[c].volts_per_div(),
            WaveId::Ref(i) => self.refs.get(i).and_then(|r| r.as_ref()).map_or(1.0, |r| r.vpd),
            _ => 1.0,
        }
    }

    /// The events of the selected search.
    pub(super) fn studio_search_events(&self) -> Arc<Vec<f64>> {
        self.studio_search_events_of(self.studio.search_sel)
    }

    /// The events of the search at `slot` (times relative to the trigger, as the trigger search has them). Cached for
    /// the record, each search by itself: a frame asks for them and searches nothing.
    pub(super) fn studio_search_events_of(&self, slot: usize) -> Arc<Vec<f64>> {
        let none = || Arc::new(Vec::new());
        let Some(sp) = self.studio.searches.get(slot).copied().flatten() else { return none() };
        if sp.kind == TrigType::Bus { return self.studio_bus_search_events(slot, sp); }
        let Some((w, dt, t0)) = self.deskew_samples(sp.src) else { return none() };
        if w.len() < 2 || !(dt > 0.0) { return none(); }
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (w.as_ptr() as usize, w.len(), dt.to_bits(), t0.to_bits()).hash(&mut h);
        format!("{sp:?}").hash(&mut h);
        for k in 0..16 { w[k * (w.len() - 1) / 15].to_bits().hash(&mut h); }
        let key = h.finish();
        if let Some((k, ev)) = &self.studio.search_cache.borrow()[slot] { if *k == key { return ev.clone(); } }
        // the conditions are the software trigger's (`oos_dsp::trigger`), with the hysteresis the trigger search has
        let hyst = (0.1 * self.studio_search_vpd(sp.src)).max(1e-9);
        let inp = oos_dsp::trigger::Input { ch: std::array::from_fn(|c| if c == 0 { w } else { &[][..] }), dt, hyst: [hyst; NCH] };
        let mut out: Vec<f64> = oos_dsp::trigger::find_all(&sp.trigger(), &inp).into_iter().map(|i| t0 + i * dt).collect();
        out.truncate(SEARCH_MAX);
        let ev = Arc::new(out);
        self.studio.search_cache.borrow_mut()[slot] = Some((key, ev.clone()));
        ev
    }

    /// The events of a bus search: the packets of the active decoder (the one the Decode menu edits) that satisfy
    /// `ModuleInfo::trigger_match(what, value, packet)`, the condition the bus trigger has, at the end of each packet
    /// (UART and SPI data take the relation of the trigger's serial settings, I2C the frame around an address).
    /// No decoder, or one that is off, or a record that is not decoded yet, has none. Cached for the packets, each
    /// search by itself.
    fn studio_bus_search_events(&self, slot: usize, sp: SearchSpec) -> Arc<Vec<f64>> {
        let none = || Arc::new(Vec::new());
        if !self.m.dec_on { return none(); }
        let Ok(packets) = self.decode_packets_of(&self.m) else { return none() };
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (packets.len(), packets.first().map(|p| p.t0.to_bits()), packets.last().map(|p| p.t1.to_bits())).hash(&mut h);
        format!("{:?}|{:?}|{:?}|{:?}", self.m.dec_cfg(), sp.bus, sp.val, self.m.ser).hash(&mut h);
        let key = h.finish();
        if let Some((k, ev)) = &self.studio.search_cache.borrow()[slot] { if *k == key { return ev.clone(); } }
        let mut m = self.m.clone();
        (m.trig_bus, m.trig_bus_val) = (sp.bus, sp.val);
        let mut out = crate::trig::points(&m, &packets);
        out.sort_by(f64::total_cmp);
        out.truncate(SEARCH_MAX);
        let ev = Arc::new(out);
        self.studio.search_cache.borrow_mut()[slot] = Some((key, ev.clone()));
        ev
    }

    /// What the selected search looked at when that is not the whole record, in seconds: (searched, record). The scope reads a
    /// stopped record and a deep one in part, the screen with a margin (`stop_plan`, `screen_window` of the board), so
    /// 64 000 events of a running 1 Mpts record were 24 000 after Stop with nothing to say why (unit 2026-11-20).
    /// None for the whole record, and where the frame does not say how long its record is.
    pub(super) fn studio_search_part(&self) -> Option<(f64, f64)> {
        let sp = self.studio.search()?;
        if sp.kind == TrigType::Bus { return None; }
        let WaveId::Ch(_) = sp.src else { return None };
        let (w, dt, _) = self.deskew_samples(sp.src)?;
        let (rate, points) = self.last[0].as_ref()?.raw.as_ref()?.frame.acq;
        if !(rate > 0.0) || points == 0 {
            return None;
        }
        let (searched, record) = (w.len() as f64 * dt, points as f64 / rate);
        // (a source without samples, a channel that is off, is not a part of the record: "0 in 0 s")
        (searched > 0.0 && searched < 0.99 * record).then_some((searched, record))
    }

    /// The count of the selected search as the top bar writes it: "64000 events", "24000 in 300 us" for a part of the record,
    /// "100000+ events" where the search ended at `SEARCH_MAX`.
    pub(super) fn studio_search_count(&self) -> String {
        let n = self.studio_search_events().len();
        let more = if n >= SEARCH_MAX { "+" } else { "" };
        match self.studio_search_part() {
            Some((searched, _)) => format!("{n}{more} in {}", si_short(searched, "s")),
            None => format!("{n}{more} event{}", if n == 1 { "" } else { "s" }),
        }
    }

    /// The events to draw and list while Studio has a search of its own.
    pub(super) fn studio_search_marks(&self) -> Option<Arc<Vec<f64>>> {
        (self.m.layout == Layout::Studio && self.studio.search().is_some()).then(|| self.studio_search_events())
    }

    /// Next (1) or previous (-1) event: the view goes there.
    pub(super) fn studio_search_go(&mut self, d: i32) {
        let ev = self.studio_search_events();
        if ev.is_empty() { return self.toast("No events"); }
        let n = ev.len() as i32;
        let at = (self.studio.search_at[self.studio.search_sel] as i32 + d).rem_euclid(n) as usize;
        self.studio.search_at[self.studio.search_sel] = at;
        self.table.picked = Some(at);
        self.go_to_time(ev[at]);
    }

    /// The trigger as a search: its type where the search has it (else its edge), its levels, times and slope.
    pub(super) fn search_of_trigger(&self) -> SearchSpec {
        let m = &self.m;
        let src = match m.trigger_wave() { w @ WaveId::Ch(_) => w, _ => WaveId::Ch(m.trig_src.min(n_ch() - 1)) };
        let kind = if SEARCH_TYPES.contains(&m.trig_type) { m.trig_type } else { TrigType::Edge };
        let slope = match m.slope { Slope::Rise => 0, Slope::Fall => 1, _ => 2 };
        let slope = if kind == TrigType::Window { m.win_mode as u8 } else if polar(kind) { (m.trig_pol == Pol::Neg) as u8 } else { slope };
        let hidden = self.studio.search().is_some_and(|s| s.hidden);
        let (bus, val) = if m.trig_type == TrigType::Bus { (m.trig_bus, m.trig_bus_val) } else { (BusWhat::Frame, 0) };
        SearchSpec { src, level: m.trig_level, slope, kind, level2: m.trig_level2, when: m.trig_when, t1: m.trig_t1, t2: m.trig_t2, eq: false, hidden, bus, val }.fit()
    }

    fn studio_search_copy_from_trigger(&mut self) {
        if self.studio.search().is_some() { self.studio.search_put(self.search_of_trigger()); }
    }

    fn studio_search_copy_to_trigger(&mut self) {
        let Some(sp) = self.studio.search() else { return };
        if sp.kind == TrigType::Bus {
            (self.m.trig_type, self.m.trig_bus, self.m.trig_bus_val) = (TrigType::Bus, sp.bus, sp.val);
            return self.settings_changed();
        }
        let WaveId::Ch(c) = sp.src else { return self.toast("The trigger takes a channel") };
        let m = &mut self.m;
        m.trig_type = sp.kind;
        m.trig_ref = None;
        m.trig_src = c;
        m.trig_level = sp.level;
        if sp.kind.two_level() { m.trig_level2 = sp.level2; }
        if sp.kind != TrigType::Edge { (m.trig_when, m.trig_t1, m.trig_t2) = (sp.when, sp.t1, sp.t2); }
        // the trigger has no Equal: it gets the two limits Equal stands for
        if sp.eq { (m.trig_t1, m.trig_t2) = (sp.t1 * (1.0 - EQ_TOL), sp.t1 * (1.0 + EQ_TOL)); }
        if sp.kind == TrigType::Window {
            m.win_mode = [WinMode::Enter, WinMode::Exit, WinMode::Inside][sp.slope.min(2) as usize];
        } else if polar(sp.kind) {
            m.trig_pol = if sp.slope == 1 { Pol::Neg } else { Pol::Pos };
        } else {
            m.slope = match sp.slope { 0 => Slope::Rise, 1 => Slope::Fall, _ => Slope::Either };
        }
        self.settings_changed();
    }

    /// What the badge of the search at `slot` says, with less at each `detail`: its number, its type and its count as
    /// the count is written ("1 Pulse \u{b7} 64000 events"), the count as a number, then without the type. A count
    /// of a part of the record keeps saying so ("24000 in 300 \u{b5}s"). A search that is not selected has its number
    /// and its count.
    fn studio_search_badge_text(&self, slot: usize, detail: usize) -> String {
        let n = self.studio_search_events_of(slot).len();
        let number = format!("{n}{}", if n >= SEARCH_MAX { "+" } else { "" });
        if slot != self.studio.search_sel { return format!("{} \u{b7} {number}", slot + 1); }
        let kind = self.studio.searches[slot].map_or("", |s| s.kind.name());
        let count = if detail == 0 || self.studio_search_part().is_some() { self.studio_search_count() } else { number };
        if detail < 2 { format!("{} {kind} \u{b7} {count}", slot + 1) } else { format!("{} \u{b7} {count}", slot + 1) }
    }

    /// The width of a badge that says `text`: no narrower than a finger.
    fn studio_search_badge_w(&self, text: &str) -> i32 {
        (self.fonts().mb.width(text) + 16).max(40)
    }

    /// The room the badges of the searches ask for between previous and next: with the selected one in full, and the
    /// least they take. The searches that are not selected say their number and count.
    pub(super) fn studio_search_need(&self) -> (i32, i32) {
        let (sel, others) = (self.studio.search_sel, self.studio_search_others_w());
        // (a count that names a part of the record is longer than the least a count takes, and is not cut)
        let (full, least) = (self.studio_search_badge_text(sel, 0), self.studio_search_badge_text(sel, 2));
        let w = |a: &str, b: &str| self.studio_search_badge_w(a).max(self.studio_search_badge_w(b)) + others;
        (w("1 Timeout \u{b7} 00000 events", &full), w("1 \u{b7} 0000", &least))
    }

    /// The width the search controls need for the selected badge to name its type (`studio_search_badges`); 0 without
    /// a search.
    pub(super) fn studio_search_typed_w(&self) -> i32 {
        if self.studio.search().is_none() { return 0; }
        let typed = self.fonts().mb.width(&self.studio_search_badge_text(self.studio.search_sel, 1)) + 16;
        2 * super::studio::SEARCH_ARROW_W + 4 + self.studio_search_others_w() + typed
    }

    /// The width of the badges of the searches that are not selected, each with its gap.
    fn studio_search_others_w(&self) -> i32 {
        let sel = self.studio.search_sel;
        self.studio.search_list().filter(|&(i, _)| i != sel).map(|(i, _)| self.studio_search_badge_w(&self.studio_search_badge_text(i, 2)) + 2).sum()
    }

    /// The badges of the searches in Studio's top bar, between previous and next: one for each search, in the order of
    /// their numbers, as its touch area (the height of the bar) with what it says. The selected one names its type
    /// and has the room the others leave; it says less before it is cut.
    pub(super) fn studio_search_badges(&self) -> Vec<(usize, Rect, String)> {
        let (top, room) = (self.frame().top, self.studio_top().search);
        let sel = self.studio.search_sel;
        let short = |i: usize| self.studio_search_badge_text(i, 2);
        let own = room.w - 2 * super::studio::SEARCH_ARROW_W - 4 - self.studio_search_others_w();
        let detail = (0..2).find(|&d| self.fonts().mb.width(&self.studio_search_badge_text(sel, d)) + 16 <= own).unwrap_or(2);
        let mut x = room.x + super::studio::SEARCH_ARROW_W + 2;
        self.studio.search_list().map(|(i, _)| {
            let text = if i == sel { self.studio_search_badge_text(i, detail) } else { short(i) };
            let r = Rect::new(x, top.y, if i == sel { own } else { self.studio_search_badge_w(&text) }, top.h);
            x = r.right() + 2;
            (i, r, text)
        }).collect()
    }

    /// The search controls of Studio's top bar: previous, the badge of the selected search and next as their touch
    /// areas (the height of the bar), and what the badge says. They were at the left of the badge row, where they
    /// took the place of two measurements and were as low as a badge.
    pub(super) fn studio_search_chip(&self) -> Option<(Rect, Rect, Rect, String)> {
        self.studio.search()?;
        let (top, room) = (self.frame().top, self.studio_top().search);
        let arrow = super::studio::SEARCH_ARROW_W;
        let (_, label, text) = self.studio_search_badges().into_iter().find(|b| b.0 == self.studio.search_sel)?;
        Some((Rect::new(room.x, top.y, arrow, top.h), label, Rect::new(room.right() - arrow, top.y, arrow, top.h), text))
    }

    pub(super) fn draw_studio_search_chip(&self, cv: &mut Canvas) {
        let Some((prev, _, next, _)) = self.studio_search_chip() else { return };
        let f = self.fonts();
        // drawn as the buttons of the bar are: the touch area is the whole height of the bar
        let bar = self.studio_top().search;
        let shrink = |r: Rect| Rect::new(r.x, bar.y, r.w, bar.h);
        let (prev, next) = (shrink(prev), shrink(next));
        chip(cv, prev, BTN, BTN_BORDER);
        chip(cv, next, BTN, BTN_BORDER);
        let (cx, cy) = (prev.x + prev.w / 2, prev.y + prev.h / 2);
        cv.triangle([(cx + 5, cy - 7), (cx + 5, cy + 7), (cx - 5, cy)], TEXT);
        let (cx, cy) = (next.x + next.w / 2, next.y + next.h / 2);
        cv.triangle([(cx - 5, cy - 7), (cx - 5, cy + 7), (cx + 5, cy)], TEXT);
        // each search in its colour; the selected one is the bright one
        for (slot, r, text) in self.studio_search_badges() {
            let (r, col, on) = (shrink(r), SEARCH_COLS[slot], slot == self.studio.search_sel);
            chip(cv, r, if on { mix(BAR, col, 48) } else { BAR }, if on { col } else { mix(BAR, col, 120) });
            cv.set_clip(r.inset(2));
            // (a search whose marks are hidden says so by a dim badge)
            let hidden = self.studio.searches[slot].is_some_and(|s| s.hidden);
            cv.text_in(&f.mb, r, 1, &text, if hidden { mix(BAR, col, 120) } else { col });
            cv.no_clip();
        }
    }

    /// A tap in the top bar on the search controls. True when it was taken. A badge selects its search and opens its
    /// sheet; the badge of the selected search closes the sheet again.
    pub(super) fn studio_search_chip_tap(&mut self, x: i32, y: i32) -> bool {
        let Some((prev, _, next, _)) = self.studio_search_chip() else { return false };
        if prev.contains(x, y) { self.studio_search_go(-1); }
        else if next.contains(x, y) { self.studio_search_go(1); }
        else if let Some((slot, _, _)) = self.studio_search_badges().into_iter().find(|b| b.1.contains(x, y)) {
            if slot == self.studio.search_sel || !(self.menu_open && self.menu == MenuId::Search) { self.toggle_menu(MenuId::Search); }
            self.studio_search_select(slot);
        }
        else { return false; }
        true
    }

    // ------------------------------------------------------------------------------------------------ items

    pub(super) fn addrail_kind(it: Item) -> Option<Kind> {
        Some(match it {
            Item::CurSrc | Item::CurSecond | Item::CurReadout | Item::CurLink | Item::CurSnap | Item::SrchWhich | Item::SrchType | Item::SrchSrc | Item::SrchSlope | Item::SrchWhen | Item::SrchShow | Item::SrchBusWhat => Kind::Choice,
            Item::SrchLevel | Item::SrchLevel2 | Item::SrchTime | Item::SrchTime2 | Item::SrchEvent | Item::SrchBusVal => Kind::Value,
            Item::CurRemove | Item::CurSnapNow | Item::Srch50 | Item::SrchFromTrig | Item::SrchToTrig | Item::SrchRemove => Kind::Action,
            _ => return None,
        })
    }

    pub(super) fn addrail_label(it: Item) -> Option<&'static str> {
        Some(match it {
            Item::CurSrc => "Source",
            Item::CurSecond => "Second source",
            Item::CurReadout => "Readout",
            Item::CurLink => "Move",
            Item::CurSnap => "Snap to edge",
            Item::CurSnapNow => "Snap cursors now",
            Item::CurRemove => "Remove cursors",
            Item::SrchWhich => "Search",
            Item::SrchType => "Type",
            Item::SrchSrc => "Source",
            Item::SrchSlope => "Slope",
            Item::SrchLevel => "Level",
            Item::SrchLevel2 => "Lower",
            Item::SrchWhen => "When",
            Item::SrchTime => "Time",
            Item::SrchTime2 => "Upper limit",
            Item::SrchShow => "Marks",
            Item::Srch50 => "Set to 50 %",
            Item::SrchFromTrig => "Copy from trigger",
            Item::SrchToTrig => "Copy to trigger",
            Item::SrchEvent => "Event",
            Item::SrchBusWhat => "Match",
            Item::SrchBusVal => "Value",
            Item::SrchRemove => "Remove search",
            _ => return None,
        })
    }

    pub(super) fn addrail_value(&self, it: Item) -> Option<String> {
        Some(match it {
            Item::CurSrc => self.wave_name(self.cursor_source()),
            Item::CurSecond => self.m.cur_b_source.map_or("Same".into(), |s| self.wave_name(s)),
            Item::CurReadout => if self.studio.cur_badge { "Badge" } else { "Graticule" }.into(),
            Item::CurLink => if self.studio.cur_linked { "Linked" } else { "Independent" }.into(),
            Item::CurSnap => format!("{:?}", self.m.cur_snap),
            Item::SrchWhich => (self.studio.search_sel + 1).to_string(),
            Item::SrchSrc => self.studio.search().map_or(String::new(), |s| self.wave_name(s.src)),
            Item::SrchType => self.studio.search().map_or(String::new(), |s| s.kind.name().into()),
            Item::SrchSlope => self.studio.search().map_or(String::new(), |s| s.slopes()[(s.slope as usize).min(s.slopes().len() - 1)].into()),
            Item::SrchLevel | Item::SrchLevel2 => self.studio.search().map_or(String::new(), |s| {
                si_short(if it == Item::SrchLevel { s.level } else { s.level2 } as f64, match s.src { WaveId::Ch(c) if c < n_ch() => self.m.unit(c), _ => "V" })
            }),
            Item::SrchWhen => self.studio.search().map_or(String::new(), |s| COND_NAMES[s.cond() as usize].into()),
            Item::SrchShow => if self.studio.search().is_some_and(|s| s.hidden) { "Hidden" } else { "Shown" }.into(),
            Item::SrchBusWhat => self.studio.search().map_or(String::new(), |s| bus_what_name(s.bus).into()),
            Item::SrchBusVal => self.studio.search().map_or(String::new(), |s| format!("0x{:X}", s.val)),
            Item::SrchTime | Item::SrchTime2 => self.studio.search().map_or(String::new(), |s| si_short(if it == Item::SrchTime { s.t1 } else { s.t2 } as f64, "s")),
            Item::SrchEvent => {
                let n = self.studio_search_events().len();
                if n == 0 { "0 / 0".into() } else { format!("{} / {}", self.studio.search_at[self.studio.search_sel].min(n - 1) + 1, n) }
            }
            _ => return None,
        })
    }

    pub(super) fn addrail_options(&self, it: Item) -> Option<Vec<String>> {
        let names = || self.sheet_shown_sources().into_iter().map(|s| self.wave_name(s)).collect::<Vec<_>>();
        Some(match it {
            Item::CurSrc | Item::SrchSrc => names(),
            Item::CurSecond => std::iter::once("Same".to_string()).chain(names()).collect(),
            Item::CurReadout => vec!["Graticule".into(), "Badge".into()],
            Item::CurLink => vec!["Independent".into(), "Linked".into()],
            Item::CurSnap => CursorSnap::ALL.iter().map(|s| format!("{s:?}")).collect(),
            Item::SrchWhich => self.studio.search_list().map(|(i, _)| (i + 1).to_string()).collect(),
            Item::SrchType => SEARCH_TYPES.iter().map(|t| t.name().to_string()).collect(),
            Item::SrchSlope => self.studio.search().map_or(&SLOPES[..], |s| s.slopes()).iter().map(|s| s.to_string()).collect(),
            Item::SrchWhen => self.studio.search().map_or(Vec::new(), |s| s.conds()).iter().map(|&c| COND_NAMES[c as usize].to_string()).collect(),
            Item::SrchShow => vec!["Shown".into(), "Hidden".into()],
            Item::SrchBusWhat => BUS_WHATS.iter().map(|&w| bus_what_name(w).to_string()).collect(),
            Item::SrchBusVal => KEYPAD.iter().map(|k| k.to_string()).collect(),
            Item::SrchLevel | Item::SrchLevel2 | Item::SrchTime | Item::SrchTime2 | Item::SrchEvent => Vec::new(),
            _ => return None,
        })
    }

    pub(super) fn addrail_adjust(&mut self, it: Item, d: i32) -> bool {
        match it {
            // Studio: the Mode row of the Default menu, with Studio's placing of the cursors when they come on or change
            // between time and voltage
            Item::CurMode if self.m.layout == Layout::Studio => {
                let all = [CursorMode::Off, CursorMode::Time, CursorMode::Volt, CursorMode::Track, CursorMode::Xy];
                let now = all.iter().position(|&m| m == self.m.cursor).unwrap_or(0);
                let next = all[(now as i32 + d).rem_euclid(all.len() as i32) as usize];
                if next == CursorMode::Off { self.m.cur_b_source = None; }
                self.set_cursor_type(next);
            }
            Item::CurSrc | Item::SrchSrc => {
                let shown = self.sheet_shown_sources();
                if shown.is_empty() { return true; }
                let cur = if it == Item::CurSrc { self.cursor_source() } else { self.studio.search().map_or(shown[0], |s| s.src) };
                let i = shown.iter().position(|&s| s == cur).unwrap_or(0) as i32;
                let to = shown[(i + d).rem_euclid(shown.len() as i32) as usize];
                if it == Item::CurSrc {
                    self.set_cursor_source(to);
                } else if let Some(sp) = self.studio.search() {
                    self.studio.search_put(SearchSpec { src: to, ..sp });
                    self.studio_search_set_50();
                }
            }
            Item::CurSecond => {
                let shown = self.sheet_shown_sources();
                let mut opts: Vec<Option<WaveId>> = vec![None];
                opts.extend(shown.into_iter().map(Some));
                let i = opts.iter().position(|&s| s == self.m.cur_b_source).unwrap_or(0) as i32;
                self.m.cur_b_source = opts[(i + d).rem_euclid(opts.len() as i32) as usize];
            }
            Item::CurReadout => if d.rem_euclid(2) != 0 { self.studio.cur_badge = !self.studio.cur_badge; },
            Item::CurLink => if d.rem_euclid(2) != 0 { self.studio.cur_linked = !self.studio.cur_linked; },
            Item::CurSnap => {
                let at = CursorSnap::ALL.iter().position(|&s| s == self.m.cur_snap).unwrap_or(0) as i32;
                self.m.cur_snap = CursorSnap::ALL[(at + d).rem_euclid(CursorSnap::ALL.len() as i32) as usize];
            }
            // the next of the searches that are there
            Item::SrchWhich => {
                let there: Vec<usize> = self.studio.search_list().map(|(i, _)| i).collect();
                if let Some(at) = there.iter().position(|&i| i == self.studio.search_sel) {
                    self.studio_search_select(there[(at as i32 + d).rem_euclid(there.len() as i32) as usize]);
                }
            }
            Item::SrchType => if let Some(sp) = self.studio.search() {
                let at = SEARCH_TYPES.iter().position(|&t| t == sp.kind).unwrap_or(0) as i32;
                self.studio_search_set_kind(SEARCH_TYPES[(at + d).rem_euclid(SEARCH_TYPES.len() as i32) as usize]);
            },
            Item::SrchSlope => if let Some(sp) = self.studio.search() { self.studio.search_put(SearchSpec { slope: (sp.slope as i32 + d).rem_euclid(sp.slopes().len() as i32) as u8, ..sp }); },
            Item::SrchWhen => if let Some(sp) = self.studio.search() {
                let all = sp.conds();
                let at = all.iter().position(|&c| c == sp.cond()).unwrap_or(0) as i32;
                if !all.is_empty() { self.studio.search_put(sp.with_cond(all[(at + d).rem_euclid(all.len() as i32) as usize])); }
            },
            Item::SrchBusWhat => if let Some(sp) = self.studio.search() {
                let at = BUS_WHATS.iter().position(|&w| w == sp.bus).unwrap_or(0) as i32;
                self.studio.search_put(SearchSpec { bus: BUS_WHATS[(at + d).rem_euclid(BUS_WHATS.len() as i32) as usize], ..sp });
            },
            Item::SrchBusVal => if let Some(sp) = self.studio.search() {
                self.studio.search_put(SearchSpec { val: (sp.val as i64 + d as i64).clamp(0, BUS_VAL_MAX as i64) as u64, ..sp });
            },
            Item::SrchShow => if let Some(sp) = self.studio.search() { if d.rem_euclid(2) != 0 { self.studio.search_put(SearchSpec { hidden: !sp.hidden, ..sp }); } },
            // the upper level takes the lower one along, the lower one stops at the upper (as the trigger's two do)
            Item::SrchLevel | Item::SrchLevel2 => if let Some(sp) = self.studio.search() {
                let step = d as f32 * 0.1 * self.studio_search_vpd(sp.src);
                let two = sp.kind.two_level();
                self.studio.search_put(if it == Item::SrchLevel { SearchSpec { level: sp.level + step, ..sp } } else { SearchSpec { level2: if two { (sp.level2 + step).min(sp.level) } else { sp.level2 + step }, ..sp } });
            },
            // on the trigger's ladder of times
            Item::SrchTime | Item::SrchTime2 => if let Some(sp) = self.studio.search() {
                let l: Vec<f64> = crate::model::time_ladder().iter().map(|&x| x as f64).collect();
                let v = self.ladder_move(it, if it == Item::SrchTime { sp.t1 } else { sp.t2 } as f64, d, &l) as f32;
                self.studio.search_put(if it == Item::SrchTime { SearchSpec { t1: v, t2: sp.t2.max(v), ..sp } } else { SearchSpec { t2: v, t1: sp.t1.min(v), ..sp } });
            },
            Item::SrchEvent => self.studio_search_go(d.signum()),
            _ => return false,
        }
        true
    }

    pub(super) fn addrail_activate(&mut self, it: Item) -> bool {
        match it {
            Item::CurSnapNow => {
                if self.cursor_snap_now() {
                    self.settings_changed();
                }
            }
            Item::CurRemove => self.studio_cursors_remove(),
            Item::Srch50 => self.studio_search_set_50(),
            Item::SrchFromTrig => self.studio_search_copy_from_trigger(),
            Item::SrchToTrig => self.studio_search_copy_to_trigger(),
            Item::SrchRemove => self.studio_search_remove(),
            _ => return false,
        }
        true
    }
}

/// The two rows of the badge area are 44 px; keep the constant referenced so the sheet geometry stays in one place.
#[allow(dead_code)]
const _ROW: i32 = SHEET_ROW;
