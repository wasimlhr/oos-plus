// SPDX-License-Identifier: GPL-2.0-or-later
//! Display > Layout > Studio: a top bar with Run, Single, the trigger state and the acquisition chip, a bottom bar with
//! one chip per channel, stacked lanes with pinned measurement badges, a measurement panel and the zoom toolbar.
//! Everything here is drawn and tapped from state the other layouts already have (lanes: `workspace`, measurements:
//! `tools`, zoom: `Ui::split_zoom`); Studio only arranges and re-skins it.
use super::*;

/// One measurement as a badge or a panel row remembers it: by what it measures, so it survives other measurements
/// being added or removed.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum MeasKey {
    One(Meas, WaveId),
    Pair(usize, WaveId, WaveId),
    Cat(u16, WaveId, WaveId, WaveId, WaveId),
}

/// How the measurements share the screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(super) enum StudioView {
    /// the lanes and the pinned badges only
    #[default]
    Waveform,
    /// the panel at the right of the lanes
    Docked,
    /// the panel over the lanes
    Floating,
    /// analysing a loaded file: the record navigator on top, the references in lanes, the measurements docked
    Offline,
    /// the SIM panel docked at the right of the lanes
    Sim,
}

impl StudioView {
    /// The name kept in the setup: the views of a record session are kept as the plain waveform.
    pub(super) fn token(self) -> &'static str {
        match self {
            StudioView::Docked => "docked",
            StudioView::Floating => "floating",
            _ => "waveform",
        }
    }
    /// "only" was the table of all measurements in place of the lanes. That view is gone (the panel in a tab beside
    /// the waveform took its place, an arrangement of the dock): a set-up or a stored view that names it loads with
    /// the panel docked.
    pub(super) fn from_token(s: &str) -> StudioView {
        match s {
            "docked" | "only" => StudioView::Docked,
            "floating" => StudioView::Floating,
            _ => StudioView::Waveform,
        }
    }
    /// The views that run on a record session (a loaded file or a SIM): leaving them returns to live.
    pub(super) fn is_record(self) -> bool { matches!(self, StudioView::Offline | StudioView::Sim) }
    /// The views with a panel docked at the right of the lanes.
    pub(super) fn is_docked(self) -> bool { matches!(self, StudioView::Docked | StudioView::Offline | StudioView::Sim) }
}

/// The measurements panel (docked, in a tab or floating).
#[derive(Default)]
pub(super) struct StudioPanel {
    /// first row of the page shown
    pub first: usize,
    /// folded to its title
    pub collapsed: bool,
    /// where the floating panel is
    pub at: Option<(i32, i32)>,
    /// the table shows mean, min and max
    pub stats: bool,
    /// the detailed table is scrolled sideways by this much
    pub hx: i32,
    /// the cards view (list and detailed are the others)
    pub cards: bool,
    /// only the pinned measurements are listed
    pub pinned_only: bool,
    /// the search text, and the keyboard that is open for it
    pub query: String,
    pub query_osk: bool,
    /// the dropdown of the header that is open
    pub drop: Option<super::measpanel::PanelDrop>,
    /// the statistics button: 0 off, 1 mean and sigma, 2 min and max
    pub stats_mode: u8,
    /// pixels the list is scrolled past the top of row `first` (the table moves by pixels, the others by rows)
    pub sub: i32,
    /// the scroll speed after a flick, px/s (positive: the content moves up), and the last finger sample (time, y)
    pub vel: f32,
    pub last: Option<(f64, i32)>,
    /// `sub` at the start of the drag
    pub sub0: i32,
    /// the inline search field replaces the title
    pub searching: bool,
}

impl StudioPanel {
    /// Back to the top of the list, nothing flying.
    pub fn scroll_top(&mut self) {
        self.first = 0;
        self.sub = 0;
        self.vel = 0.0;
    }
}

#[derive(Default)]
pub(super) struct Studio {
    /// the time the measurement chip slid out (a tap on its arrow); it slides back by itself after `MEAS_OUT_S`
    pub meas_out: Option<f64>,
    /// the layout was just chosen: the lanes are stacked at the next frame (`studio_sync`)
    pub enter: bool,
    /// the measurements shown as badges; None until one is pinned or unpinned: the first three
    pub pinned: Option<Vec<MeasKey>>,
    /// The measurements the user took off the badge row. Every other one is pinned, a new one too: a measurement
    /// is pinned from the moment it is added. (`pinned` is the explicit list of an older set-up; a pin tap ends it.)
    pub unpinned: Vec<MeasKey>,
    /// measurements moved to the floating panel from a badge popup: listed there beside the pinned ones
    pub floated: Vec<MeasKey>,
    /// the badge row is hidden (Display > Screen > Pinned badges)
    pub hide_badges: bool,
    /// the badge whose action popup is open
    pub popup: Option<MeasKey>,
    /// the measurement a tap on its badge shows on the waveform (its level, span or shaded area); the next tap on
    /// the same badge opens its menu, and closing that menu takes the mark away
    pub mark: Option<MeasKey>,
    /// the menu of the marked badge was opened: the mark goes when it closes
    pub mark_menu: bool,
    pub view: StudioView,
    /// the Add menu under the top bar is open
    pub add_menu: bool,
    /// Measure is open as the page of every layout (categories, Selected, Settings), not as the quick Add panel
    pub meas_page: bool,
    /// the context menu of the plot (a long press on the lanes)
    pub ctx: Option<super::studioctx::Ctx>,
    /// Full was chosen for the eye (Side is the Studio default: the waveform stays in view)
    pub eye_full: bool,
    /// the "+ CH" picker (channels that are off) is open
    pub ch_picker: bool,
    /// the last tap on the pill of a channel that is on (channel, time): two taps in `PILL_AGAIN` are a double tap
    pub pill_tap: Option<(usize, f64)>,
    /// Compact view: smaller lane gaps, badge row and measurement panel
    pub compact: bool,
    pub panel: StudioPanel,
    /// the SIM panel lists the knobs instead of the presets
    pub sim_knobs: bool,
    /// the table and the panel list only the measurements of this source
    pub filter: Option<WaveId>,
    pub drag: Option<StudioDrag>,
    /// the display scale popup of this channel's lane is open
    pub yzoom: Option<usize>,
    /// where the lanes drew their channel badges last frame
    pub lane_badges: std::cell::RefCell<Vec<(usize, Rect)>>,
    /// where the decoders drew their names last frame (decoder, rectangle): a tap on one opens its Decode menu
    pub dec_labels: std::cell::RefCell<Vec<(usize, Rect)>>,
    /// where the handles of the references and the math channels were drawn last frame (`studio_draw_handles`)
    pub handles: std::cell::RefCell<Vec<(super::workspace::Trace, Rect)>>,
    /// Studio was the layout at the last frame: coming into it applies the entry rules (`studio_sync`)
    pub was: bool,
    /// Search 1 to 3 (Add > Search) by their place, the selected one, the event next / previous stand on in each
    pub searches: [Option<super::addrail::SearchSpec>; super::addrail::SEARCH_SLOTS],
    pub search_sel: usize,
    pub search_at: [usize; super::addrail::SEARCH_SLOTS],
    pub search_cache: std::cell::RefCell<[Option<(u64, std::sync::Arc<Vec<f64>>)>; super::addrail::SEARCH_SLOTS]>,
    /// the cursor readouts go to the badge row, not onto the graticule
    pub cur_badge: bool,
    /// moving one cursor moves the other with it
    pub cur_linked: bool,
    /// the traces share one graticule instead of a lane each (the plot's menu: Overlay traces)
    pub overlay: bool,
    /// the first pinned measurement shown as a badge (the Meas tab pages through them)
    pub badge_first: usize,
    /// lines the badge row has beyond its first (`studio_badge_wrap_now`, set at the start of a frame)
    pub badge_wrap: i32,
    /// the docking tree and the floating views (`dock.rs`)
    pub dock: super::dock::Dock,
    /// the divider under the overview strip is held: (finger y, strip height) at the press
    pub ov_drag: Option<(i32, i32)>,
    /// the gestures of the measurements list (`measdrag.rs`)
    pub md: super::measdrag::Md,
    /// the first fact of the spectrum list that is shown (the list pages when it does not fit)
    pub facts_first: usize,
    /// the rectangles of the labels on the trace drawn last (for the tests)
    pub label_rects: std::cell::RefCell<Vec<Rect>>,
    /// the axis strips: a finger on one, a wheel flash (`axis.rs`)
    pub axis: super::axis::AxisState,
    /// the trigger level slider is open (`trigslider.rs`), and what the finger on it is doing
    pub trig_slider: bool,
    /// the shared zoom: its source, and the horizontal scale of the references (`sharedzoom.rs`)
    pub sz: super::sharedzoom::SharedZoom,
    pub slider_part: Option<super::trigslider::SliderPart>,
}

/// The second group of the "+ CH" popup.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PickAdd {
    Math,
    Ref,
    Bus,
}
pub(super) const PICK_ADDS: [PickAdd; 3] = [PickAdd::Math, PickAdd::Ref, PickAdd::Bus];
impl PickAdd {
    pub(super) fn label(self) -> &'static str {
        match self { PickAdd::Math => "Add math", PickAdd::Ref => "Add ref", PickAdd::Bus => "Add bus" }
    }
}

/// The rectangles of the Studio top bar.
#[derive(Clone, Copy)]
pub(super) struct StudioTop {
    /// the numbered view switcher, zero wide unless there is more than one view
    pub vbox: Rect,
    /// the Add panel button
    pub add: Rect,
    /// Live (the way back from a record session), zero wide in live mode
    pub badge: Rect,
    /// the run / trigger status pill
    pub run: Rect,
    pub single: Rect,
    /// the mini SIM control, zero wide unless the SIM is the source
    pub sim: Rect,
    pub acq: Rect,
    /// the search controls, at the end of the read-out's room (no width without a search)
    pub search: Rect,
    /// the links as small pills left of the buttons: LAN, and WLAN and USB while their stick is in (else no width)
    pub lan: Rect,
    pub wlan: Rect,
    pub usb: Rect,
    pub measure: Rect,
    pub cursors: Rect,
    pub display: Rect,
    pub storage: Rect,
    pub gear: Rect,
    /// Autoset, Clear and Force
    pub autoset: Rect,
    pub clear: Rect,
    pub force: Rect,
    /// 0 full labels, 1 shortened, 2 the shortest
    pub tier: usize,
}

const TOP_TIERS: usize = 3;
/// The labels of the top bar per tier: Autoset, Clear, Force, + Add, Measure, Cursors, Display, Storage.
pub(super) const TOP_LABELS: [[&str; 8]; TOP_TIERS] = [
    ["Autoset", "Clear", "Force", "Add", "", "Cursors", "Display", "Storage"],
    ["Auto", "Clear", "Force", "Add", "", "Cur", "Disp", "Store"],
    ["Auto", "Clr", "Force", "Add", "", "Cur", "Disp", "Store"],
];

/// The rectangles of the Studio bottom bar.
#[derive(Clone, Copy)]
pub(super) struct StudioBottom {
    pub ch: [Rect; NCH],
    /// a small chip for each shown ref: a tap makes it the active source
    pub refs: [Rect; NREF],
    /// a small chip for each math channel that is on
    pub math: [Rect; NMATH],
    /// a pill for each decoder that is on: a tap opens the Decode sheet of that decoder
    pub dec: [Rect; DECODERS],
    /// "+ CH": the channels that are off, in one chip (zero wide when every channel is on)
    pub add: Rect,
    /// the button that shows the measurement badges again while they are hidden
    pub show: Option<Rect>,
    /// LA and GEN pills after + CH, like the channel pills: a tap opens their sheets (None: the model has no such part)
    pub la: Option<Rect>,
    pub gen: Option<Rect>,
    pub time: Rect,
    pub trig: Rect,
    pub panel: Option<Rect>,
}

/// A source pill is at most as wide as a channel pill was and shrinks to this; the pills are 5 px apart.
const PILL_MAX_W: i32 = 170;
const PILL_MIN_W: i32 = 18;
/// Width of the pill of a channel that is off: at least this, and with room to spare as much as holds its badge and
/// "Off". It is a button to switch the channel on, so it is not a sliver beside the measurement badges.
const OFF_PILL_W: i32 = 60;
const OFF_PILL_MAX_W: i32 = 96;
/// Width of the chip at the end of the badge row: a number and the arrow that says it opens something.
pub(super) const LIST_W: i32 = 58;

/// The chip that adds a source, and the short form of the generator's pill (its name alone).
const ADD_W: i32 = 62;
const GEN_W: i32 = 56;
/// The button that brings the hidden badges back: with its name, and as its eye alone where the bar is full.
const SHOW_W: i32 = 88;
const SHOW_ICON_W: i32 = 50;
/// The chip that adds a source as its plus alone, where the bar is full.
const ADD_SHORT_W: i32 = 40;
const CHIP_H: i32 = 30;
/// The pills of the bottom bar are the buttons pressed most: taller than a measurement badge of the row above them
/// (`ROW_H` - 6 = 34), and the bar is 3 px more on either side.
const BAR_CHIP_H: i32 = 40;
pub(super) const BOTTOM_H: i32 = BAR_CHIP_H + 6;
/// What was drawn into a pill of 30 px sits this much lower in one of the bar.
const BAR_DY: i32 = (BAR_CHIP_H - 30) / 2;
/// The two text lines of a pill need both fonts to fit the 30 px: at the Larger text size a pill keeps one line.
const TWO_LINE_MAX: i32 = 30;
const CHIP_Y: i32 = 5;

/// A play triangle, 12 px high, left edge at `x`.
pub(super) fn icon_play(cv: &mut Canvas, x: i32, cy: i32, c: Rgb) {
    cv.triangle([(x, cy - 6), (x, cy + 6), (x + 11, cy)], c);
}

pub(super) fn icon_stop(cv: &mut Canvas, x: i32, cy: i32, c: Rgb) {
    cv.fill(Rect::new(x, cy - 5, 10, 10), c);
}

/// Two cursor lines.
pub(super) fn icon_cursors(cv: &mut Canvas, x: i32, cy: i32, c: Rgb) {
    cv.fill(Rect::new(x, cy - 6, 2, 13), c);
    cv.fill(Rect::new(x + 7, cy - 6, 2, 13), c);
}

/// A ring with teeth.
pub(super) fn icon_gear(cv: &mut Canvas, cx: i32, cy: i32, c: Rgb, bg: Rgb) {
    for (dx, dy) in [(0, -8), (0, 8), (-8, 0), (8, 0), (-6, -6), (6, -6), (-6, 6), (6, 6)] {
        cv.fill(Rect::new(cx + dx - 1, cy + dy - 1, 3, 3), c);
    }
    cv.circle(cx, cy, 6, c);
    cv.circle(cx, cy, 4, bg);
    cv.circle(cx, cy, 2, c);
}

/// Three lines, one above the other: the Menu (the gear read as a sun at this size, and is the icon of Utility).
pub(super) fn icon_menu(cv: &mut Canvas, cx: i32, cy: i32, c: Rgb) {
    for dy in [-6, 0, 6] {
        cv.round(Rect::new(cx - 9, cy + dy - 1, 18, 3), 1, c, 255);
    }
}

/// A chip: the rounded body with a 1 px border; in the accent (the selected one) the border is 3 px.
pub(super) fn chip(cv: &mut Canvas, r: Rect, fill: Rgb, border: Rgb) {
    cv.round(r, 6, border, 255);
    cv.round(r.inset(if border == ACCENT { 3 } else { 1 }), 5, fill, 255);
}

impl Ui {
    /// After the settings of the last run were read (main.rs): Studio comes back with its view and its lanes.
    pub fn restore_view_state(&mut self) {
        self.studio.view = StudioView::from_token(&self.m.studio_view);
        let text = self.m.studio_dock.clone();
        self.dock_restore(&text);
        self.view_state_restore("");
        // Overlay or lanes come back as they were left
        self.studio.overlay = self.m.studio_overlay;
        self.studio.enter = self.m.layout == Layout::Studio;
    }

    /// Start of a frame: the lanes draw their badges again, and a layout change made in the Display menu is applied.
    pub(super) fn studio_frame_start(&mut self) {
        self.studio.lane_badges.get_mut().clear();
        self.studio.dec_labels.get_mut().clear();
        self.studio.handles.get_mut().clear();
        self.sync_view_state();
        // coming into Studio by any road (menu, recalled setup, restart) applies its entry rules: its bars show
        let is = self.m.layout == Layout::Studio;
        if is && !self.studio.was { self.studio.enter = true; }
        self.studio.was = is;
        // the view is kept with the setup like the layout; a record view is not (it needs its session)
        let token = self.studio.view.token();
        if self.m.studio_view != token { self.m.studio_view = token.to_string(); }
        if self.m.layout == Layout::Studio { self.studio_prune_ref_rows(); }
        self.studio.badge_wrap = self.studio_badge_wrap_now();
        self.dock_sync_setup();
        self.studio_sync();
    }

    /// Applies a layout change made in the Display menu: Studio always draws its sources as lanes.
    pub(super) fn studio_sync(&mut self) {
        self.studio_sync_views();
        if !std::mem::take(&mut self.studio.enter) { return; }
        if self.m.layout == Layout::Studio {
            // lanes, unless Overlay traces was chosen (it is kept with the setup)
            let overlay = self.studio.overlay;
            self.trace_mode(!overlay);
            // Studio is entered with its bars: a full-screen state left from another layout would hide them
            if self.m.density == Density::Full {
                self.set_full(false, false);
            }
        }
    }

    /// What the badge at the left of the top bar says about the source of the waveforms.
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub(super) fn studio_source(&self) -> (&'static str, Rgb) {
        if self.record_mode() { return (if self.sim_mode() { "SIM" } else { "REF" }, ACCENT); }
        match self.src {
            Source::Demo { .. } => ("DEMO", ACCENT),
            Source::Replay { .. } => ("REPLAY", ACCENT),
            _ => ("LIVE", GREEN),
        }
    }

    /// Offline waveform analysis: the badge is the way back to live.
    pub(super) fn studio_back_badge(&self) -> bool {
        self.m.layout == Layout::Studio && self.studio.view == StudioView::Offline && self.record_mode()
    }

    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub(super) fn studio_badge_label(&self) -> &'static str {
        if self.studio_back_badge() { "Live" } else { self.studio_source().0 }
    }

    /// The chip of the acquisition: the sample rate and depth, or while analysing a file its name first.
    pub(super) fn studio_acq_caption(&self) -> String {
        let (caption, _) = self.acq_caption();
        // the source is part of the caption: DEMO and REPLAY here, SIM and REF come with the record caption
        if !self.record_mode() && !matches!(self.src, Source::Hw { .. } | Source::Live { .. }) {
            let name = match self.src { Source::Demo { .. } => "DEMO", _ => "REPLAY" };
            return format!("{name} \u{b7} {caption}");
        }
        // the SIM chip of the bar already names the SIM
        let caption = if self.session_is_sim && self.record_mode() { caption.trim_start_matches("SIM {b7} ").to_string() } else { caption };
        if !self.studio_back_badge() { return caption; }
        match self.refs.get(self.active_reference()).and_then(|r| r.as_ref()) {
            Some(r) => {
                let name = if r.label.is_empty() { format!("R{}", self.active_reference() + 1) } else { r.label.clone() };
                format!("{name} \u{b7} {}", caption.trim_start_matches("REF \u{b7} "))
            }
            None => "No file loaded".to_string(),
        }
    }

    /// The room the read-out asks of the bar before the other buttons shorten their labels: what its caption needs,
    /// never more than 260 px.
    pub(super) fn studio_acq_need(&self) -> i32 {
        (self.fonts().mb.width(&self.studio_acq_caption()) + 6).clamp(120, 260)
    }

    /// Keeps the record views consistent with the session: Offline ends with the record session, and shows
    /// the navigator (the zoom overview) for a loaded file until the user closes it.
    pub(super) fn studio_sync_views(&mut self) {
        if self.m.layout != Layout::Studio || self.studio.view != StudioView::Offline { return; }
        if !self.record_mode() {
            self.studio.view = StudioView::Waveform;
            self.relayout();
        } else if self.refs.iter().any(Option::is_some) && !self.m.zoom && !self.zoom_view.user_closed {
            self.set_zoom(true);
        }
    }

    /// A record view ends when the session does (`return_to_live`).
    pub(super) fn studio_left_record(&mut self) {
        if self.studio.view.is_record() {
            self.studio.view = StudioView::Waveform;
            self.studio.panel.scroll_top();
        }
    }

    /// The acquisition of a live source as three small columns in the top bar: the sample rate, the memory depth and
    /// the acquisition mode, each a label over its value. (A column of waveforms a second was taken out again: it
    /// counted the records the screen took, not what the hardware acquires, and read wrong on a unit.) None while a
    /// file or the SIM is analysed (the caption names the record then). Column widths hold the longest value of each,
    /// so the bar does not move while the numbers change.
    pub(super) fn studio_acq_cols(&self) -> Option<Vec<(&'static str, String, i32)>> {
        if self.record_mode() || self.studio_back_badge() { return None; }
        let f = self.fonts();
        let (sr, depth) = self.live_rate_depth();
        let mode = match self.m.acq { AcqMode::Normal => "Normal".to_string(), AcqMode::Peak => "Peak".into(), AcqMode::Average => format!("Avg {}", self.m.avg_count), AcqMode::HiRes => "High res".into() };
        let w = |label: &str, widest: &str| f.s.width(label).max(f.mb.width(widest)) + 14;
        Some(vec![
            ("Sample rate", si_digits(sr, "Sa/s", 3), w("Sample rate", "888 MSa/s")),
            ("Memory", si_digits(depth as f64, "pts", 3), w("Memory", "8.88 Mpts")),
            ("Mode", mode, w("Mode", "Avg 1024")),
        ])
    }

    /// The top bar at the first tier (full labels) that leaves the acquisition caption room, else the shortest: labels
    /// shorten before anything is dropped, and every button stays reachable.
    pub(super) fn studio_top(&self) -> StudioTop {
        // the status caption gets the room it needs (`studio_acq_need`, up to 260 px) before the buttons give up
        // width; when no tier has that much, the one with the most room is used and the caption is shortened to it
        // (the three columns of a live acquisition are drawn where the tier chosen for the caption has their room: they
        // do not ask for a shorter tier, which would take the buttons below a finger's width)
        let need = self.studio_acq_need();
        let mut t = self.studio_top_tier(0);
        let mut best = t.acq.w;
        for tier in 1..TOP_TIERS {
            if t.acq.w >= need { break; }
            let n = self.studio_top_tier(tier);
            if n.acq.w > best || n.acq.w >= need { best = n.acq.w; t = n; }
        }
        // the selected search names its type where shorter names of the buttons leave the room for it beside the
        // caption (without the View button the full names fit a bar on which the search had its count alone)
        let typed = self.studio_search_typed_w();
        if t.acq.w >= need && t.search.w < typed {
            if let Some(n) = (t.tier + 1..TOP_TIERS).map(|i| self.studio_top_tier(i)).find(|n| n.acq.w >= need && n.search.w >= typed) { t = n; }
        }
        t
    }

    fn studio_top_tier(&self, tier: usize) -> StudioTop {
        let f = self.fonts();
        let top = self.frame().top;
        let (pad, floor, gap) = [(28, 40, 6), (22, 38, 6), (14, 34, 4)][tier];
        let labels = &TOP_LABELS[tier];
        // the same sizes as the Default bar: the status pill at the left, small buttons of the bar height
        let btn_w = |label: &str| (f.mb.width(label) + pad).max(floor);
        let pill = Rect::new(8, 6, 92.max(30 + f.mb.width("TRIG'D") + 12), 28);
        // no Single button: the front panel has the key (as the Default layout has none), and the Menu has the row.
        // Its 58 px go to the read-out of the acquisition
        let single = Rect::new(pill.right(), CHIP_Y, 0, CHIP_H);
        let mut x = single.right() + gap;
        // Live: the way back from a loaded file or a SIM, as in the Default bar
        let badge = if self.record_mode() { Rect::new(x, CHIP_Y, btn_w("Live"), CHIP_H) } else { Rect::new(x, CHIP_Y, 0, CHIP_H) };
        if badge.w > 0 { x = badge.right() + gap; }
        // SIM is an entry of the Menu (Analysis), as in the Default layout (ADR 0007), except
        // - while the SIM is the source: the preset, a tap opens the SIM panel
        // - while live with a reference shown: a button that plays it, the twin of Live (ADR 0035)
        let sim = if self.session_is_sim && self.record_mode() {
            Rect::new(x, CHIP_Y, (f.mb.width(self.sim_preset_name()) + 56).min(if tier == 0 { 220 } else { 170 }), CHIP_H)
        } else if self.sim_return_button() {
            Rect::new(x, CHIP_Y, btn_w("SIM"), CHIP_H)
        } else { Rect::new(x, CHIP_Y, 0, CHIP_H) };
        if sim.w > 0 { x = sim.right() + gap; }
        // Autoset, Clear, Force are keys of the front panel (and entries of the Menu): the bar does not repeat them,
        // its room goes to the acquisition read-out
        let autoset = Rect::new(x, CHIP_Y, 0, CHIP_H);
        let (clear, force) = (autoset, autoset);
        // the right group is laid out from the edge: gear, Storage, Display, Cursors, Measure, + Add
        let mut r = top.right() - 8;
        let gear = Rect::new(r - 40, CHIP_Y, 40, CHIP_H);
        r = gear.x - gap;
        let mut place = |label: &str| { let b = Rect::new(r - btn_w(label), CHIP_Y, btn_w(label), CHIP_H); r = b.x - gap; b };
        let storage = place(labels[7]);
        let display = place(labels[6]);
        // Cursors: a tap puts them on, the next one opens their sheet (Add has them too, three taps deep)
        let cursors = place(labels[5]);
        // Add replaces the old "+ Add" and Meas buttons: one button, the menu has both
        let measure = Rect::new(-20, CHIP_Y, 0, CHIP_H);
        let add = place(labels[3]);
        // the numbered view switcher (Display > Views), left of + Add, only while there is more than one view
        let vw = super::views::studio_box_w(self.m.views);
        let vbox = Rect::new(add.x - if vw > 0 { gap + vw } else { 0 }, CHIP_Y, vw, CHIP_H);
        // the links, as the Default bar has them at its right end: a dot and a name each. The Default bar's box with
        // the address has no room here; the address is on the Network page a tap on LAN opens
        let mut lx = vbox.x - gap;
        let mut link = |label: &str, there: bool| {
            let w = if there { f.sb.width(label) + 26 } else { 0 };
            let b = Rect::new(lx - w, CHIP_Y, w, CHIP_H);
            if there {
                lx = b.x - 4;
            }
            b
        };
        let lan = link("LAN", true);
        let wlan = link("WLAN", self.net.wlan_stick);
        let usb = link("USB", self.stick.mounted);
        // the search (Add > Search): previous, how many events, next. It has the end of the read-out's room. Its label
        // gives way to the read-out, down to the count alone (`studio_top` shortens buttons where that names its type)
        let links_x = lx + 4 - gap;
        let room = links_x - 10 - x;
        let search_w = if self.studio.search().is_some() {
            let caption = self.studio_acq_need();
            let (full, least) = self.studio_search_need();
            2 * SEARCH_ARROW_W + 4 + full.min(room - 8 - caption - 2 * SEARCH_ARROW_W - 4).max(least)
        } else { 0 };
        let acq = Rect::new(x, CHIP_Y, (room - if search_w > 0 { search_w + 8 } else { 0 }).max(40), CHIP_H);
        let search = Rect::new(links_x - 10 - search_w, CHIP_Y, search_w, CHIP_H);
        let run = pill;
        StudioTop {
            vbox, add, badge, run, single, sim, acq, search, lan, wlan, usb, measure, cursors, display, storage,
            gear, autoset, clear, force, tier,
        }
    }

    /// Measure and Cursors as the top-bar buttons (`top_buttons`); the others are Studio's own.
    pub(super) fn studio_top_buttons(&self) -> Vec<(TopBtn, &'static str, Rect)> {
        let t = self.studio_top();
        vec![(TopBtn::Menu(MenuId::Display), "Display", t.display), (TopBtn::Menu(MenuId::Storage), "Storage", t.storage)]
    }

    pub(super) fn draw_studio_top(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let top = self.frame().top;
        cv.fill(top, BAR);
        cv.hline(top.x, top.right() - 1, top.bottom() - 1, LINE);
        let t = self.studio_top();
        let root = self.menu_root();
        let open = |m: MenuId| self.menu_open && root == m;
        // one status pill as in the Default bar; a tap is Run / Stop in every layout
        let (txt, col) = self.run_caption();
        let r = t.run;
        let pressed = self.touch.down && r.contains(self.touch.x0, self.touch.y0);
        cv.round(r, 6, mix(BAR, col, if pressed { 140 } else { 80 }), 255);
        cv.round(r.inset(1), 5, mix(BAR, col, if pressed { 80 } else { 36 }), 255);
        // the button says what a tap does: running shows pause bars, otherwise the play triangle
        let (gx, gy) = (r.x + 15, r.y + 14);
        if self.m.run == RunState::Run && !self.record_mode() {
            cv.fill(Rect::new(gx - 5, gy - 5, 3, 10), col);
            cv.fill(Rect::new(gx + 2, gy - 5, 3, 10), col);
        } else {
            cv.triangle([(gx - 4, gy - 6), (gx - 4, gy + 6), (gx + 6, gy)], col);
        }
        cv.text_in(&f.mb, Rect::new(r.x + 30, r.y, r.w - 30, r.h), 0, txt, col);
        if t.single.w > 0 {
            btn(cv, t.single, BTN, BTN_BORDER);
            cv.text_in(&f.s, t.single, 1, "Single", TEXT);
        }
        if t.badge.w > 0 {
            btn(cv, t.badge, mix(BAR, ACCENT, 50), ACCENT);
            cv.text_in(&f.mb, t.badge, 1, "Live", ACCENT);
        }
        if t.sim.w > 0 { self.draw_studio_sim_chip(cv, t.sim); }
        // sample rate, depth and mode (or the source and its record): a tap opens the page that sets them
        let caption = self.studio_acq_caption();
        let acq_open = self.menu_open && root == if self.record_mode() { MenuId::Ref } else { MenuId::Acquire };
        match self.studio_acq_cols().filter(|c| c.iter().map(|x| x.2).sum::<i32>() <= t.acq.w) {
            // sample rate, memory and mode: a small label over each value
            Some(cols) => {
                let mut x = t.acq.x;
                for (label, value, w) in cols {
                    cv.text_in(&f.s, Rect::new(x, 4, w - 6, 14), 0, label, DIM);
                    cv.text_fit(&f.mb, Rect::new(x, 18, w - 6, 18), 0, &value, if acq_open { ACCENT } else { TEXT });
                    x += w;
                }
            }
            None => cv.text_fit(&f.mb, Rect::new(t.acq.x, 11, t.acq.w, 18), 0, &caption, if acq_open { ACCENT } else { TEXT }),
        }
        // Add, Cursors, Display, Storage: the Default button style
        let cursors = open(MenuId::Cursor) || self.m.cursor != CursorMode::Off;
        let adding = self.studio_add_chip_on();
        let displaying = open(MenuId::Display);
        let storing = open(MenuId::Storage);
        let labels = &TOP_LABELS[t.tier];
        for (r, label, on) in [(t.add, labels[3], adding), (t.cursors, labels[5], cursors), (t.display, labels[6], displaying), (t.storage, labels[7], storing)].into_iter().filter(|(r, ..)| r.w > 0) {
            btn(cv, r, BTN, if on { ACCENT } else { BTN_BORDER });
            cv.text_in(&f.mb, r, 1, label, if on { ACCENT } else { TEXT });
        }
        self.draw_view_box(cv);
        self.draw_studio_search_chip(cv);
        // the links: a dot (green with an address, or a mounted stick) and a name, as the USB pill of the Default bar
        let lan_up = self.addr_of("eth0").filter(|_| self.net.eth_link).is_some();
        let wlan_up = self.addr_of(oos_hal::wlan::IFACE).is_some();
        let net = open(MenuId::Network);
        let links = [(t.lan, "LAN", lan_up, net), (t.wlan, "WLAN", wlan_up, net), (t.usb, "USB", true, storing)];
        for (r, label, up, on) in links {
            if r.w == 0 {
                continue;
            }
            cv.round(r, 5, if on { ACCENT } else { BTN_BORDER }, 255);
            cv.round(r.inset(1), 4, BG, 255);
            cv.circle(r.x + 10, r.y + r.h / 2, 3, if up { GREEN } else { DIM });
            cv.text_in(&f.sb, Rect::new(r.x + 18, r.y, r.w - 20, r.h), 0, label, TEXT);
        }
        // Autoset, Clear, Force: Force is dimmed while no trigger is waiting, and still works
        let waiting = !self.record_mode() && self.m.run == RunState::Run && self.m.status == TrigStatus::Wait;
        for (r, label, lit) in [(t.autoset, labels[0], true), (t.clear, labels[1], true), (t.force, labels[2], waiting)].into_iter().filter(|(r, ..)| r.w > 0) {
            let pressed = self.touch.down && self.top_hit(r).contains(self.touch.x0, self.touch.y0);
            btn(cv, r, if pressed { mix(BTN, ACCENT, 60) } else { BTN }, if pressed { ACCENT } else { BTN_BORDER });
            cv.text_in(&f.mb, r, 1, label, if lit { TEXT } else { DIM });
        }
        let gear_on = open(MenuId::Main) || open(MenuId::Utility) || self.m.touch_lock;
        btn(cv, t.gear, BTN, if gear_on { ACCENT } else { BTN_BORDER });
        icon_menu(cv, t.gear.x + t.gear.w / 2, t.gear.y + t.gear.h / 2, if gear_on { ACCENT } else { TEXT });
    }

    /// The rectangles of the Studio bottom bar: a chip per channel, the time base and the trigger.
    pub(super) fn studio_bottom(&self) -> StudioBottom {
        let bottom = self.frame().bottom;
        let (y, h) = (bottom.y + 3, BAR_CHIP_H);
        let ph = h;
        let n = n_ch();
        let right = bottom.right() - 4;
        // the hardware Panel button is gone from this bar: F6 (and the desktop window) still toggle the panel
        let panel: Option<Rect> = None;
        // the sources that are on get a pill; the channels that are off share one "+ CH" chip
        let shown = self.shown_refs();
        let maths: Vec<usize> = (0..NMATH).filter(|&k| self.m.math_shown(k)).collect();
        // the chip that adds a source: a channel that is off, math, a reference or a bus. One name, whatever is off
        let mut add_w = ADD_W;
        // SIM plays refs, not the live channels: their pills are not on the bar then (Live brings them back)
        let chans: Vec<usize> = if self.studio_ch_pills_hidden() { Vec::new() } else { (0..n).filter(|&c| self.m.ch[c].on).collect() };
        let mut show_w = if self.studio_badges_hidden() { SHOW_W } else { 0 };
        let caps = &oos_hal::model::platform().caps;
        let la_w = if caps.la { 52 } else { 0 };
        let gen_w = if caps.has_gen() && !self.m.gen_hidden { if self.m.afg_on { 124 } else { GEN_W } } else { 0 };
        // a decoder that is on has its pill too, whether its lines are on or not: its band goes with its channel,
        // its pill stays and leads to its settings
        let decs: Vec<usize> = (0..DECODERS).filter(|&k| self.m.dec_is_on(k)).collect();
        // the source pills (channels, refs, math, decoders) share what the fixed chips leave, by count
        let count = (chans.len() + shown.len() + maths.len() + decs.len()) as i32;
        // the Panel button's room goes to the time base and trigger; from the 4th source pill on they give some back
        let extra = if self.workspace.desktop_panel.is_some() { 25 } else { 0 };
        let give = (count - 3).clamp(0, 3) * 14;
        let trig = Rect::new(right - (196 + extra - give), y, 196 + extra - give, h);
        let time = Rect::new(trig.x - 9 - (150 + extra - give / 2), y, 150 + extra - give / 2, h);
        // the generator's pill takes its short form ("GEN") before a source pill is too narrow for its scale: with four
        // channels and the generator on, the pills kept their badge and lost every scale longer than "1 V"
        let aux = |gen_w: i32| [la_w, gen_w].iter().filter(|w| **w > 0).map(|w| w + 5).sum::<i32>();
        // the button that brings the hidden badges back takes its room from the pills: with four channels, the LA
        // and the generator each pill had 29 px left for its scale, and "500 mV" was cut to "500m" (seen on a unit,
        // 2026-11-18). Before a pill loses its scale the button is its eye alone, then the chip that adds a source
        // its plus alone
        if show_w > 0 && count > 0 {
            let need = self.studio_ch_pill_scale_min();
            let pill = |add_w: i32, show_w: i32| Self::studio_pill_width(time.x - 9 - (bottom.x + 4) - (add_w + 5 + show_w + aux(gen_w.min(GEN_W))), count);
            if pill(add_w, show_w) < need { show_w = SHOW_ICON_W; }
            if pill(add_w, show_w) < need { add_w = ADD_SHORT_W; }
        }
        let tight = count > 0 && Self::studio_pill_width(time.x - 9 - (bottom.x + 4) - (add_w + 5 + show_w + aux(gen_w)), count) < self.studio_ch_pill_need();
        let gen_w = if tight { gen_w.min(GEN_W) } else { gen_w };
        let aux_w = aux(gen_w);
        let room = time.x - 9 - (bottom.x + 4) - (add_w + 5 + show_w + aux_w);
        // a channel that is off has a small dim pill in its place: a tap switches it on. They give way when the
        // sources that are on need the room (and in SIM, which has no channel pills at all)
        let offs: Vec<usize> = if self.studio_ch_pills_hidden() { Vec::new() } else { (0..n).filter(|&c| !self.m.ch[c].on).collect() };
        let off_room = offs.len() as i32 * (OFF_PILL_W + 5);
        let show_off = !offs.is_empty() && room - off_room >= count * 96 + (count - 1).max(0) * 5;
        let room = if show_off { room - off_room } else { room };
        let pw = Self::studio_pill_width(room, count);
        // a channel pill ends after its chevron: it holds a scale and no name, so it is not as wide as a ref's may be
        let cw = pw.min(self.studio_ch_pill_need());
        // what the sources leave of their room goes to the pills of the channels that are off, in equal parts
        let spare = room - chans.len() as i32 * (cw + 5) - (count - chans.len() as i32) * (pw + 5);
        let ow = if show_off { (OFF_PILL_W + spare.max(0) / offs.len() as i32).min(OFF_PILL_MAX_W) } else { OFF_PILL_W };
        let mut ch = [Rect::new(0, y, 0, ph); NCH];
        let mut x = bottom.x + 4;
        for c in 0..n {
            if chans.contains(&c) {
                ch[c] = Rect::new(x, y, cw, ph);
                x += cw + 5;
            } else if show_off && offs.contains(&c) {
                ch[c] = Rect::new(x, y, ow, ph);
                x += ow + 5;
            }
        }
        let mut refs = [Rect::new(0, y, 0, ph); NREF];
        for &i in &shown {
            refs[i] = Rect::new(x, y, pw, ph);
            x += pw + 5;
        }
        let mut math = [Rect::new(0, y, 0, ph); NMATH];
        for &k in &maths {
            math[k] = Rect::new(x, y, pw, ph);
            x += pw + 5;
        }
        let mut dec = [Rect::new(0, y, 0, ph); DECODERS];
        for &k in &decs {
            dec[k] = Rect::new(x, y, pw, ph);
            x += pw + 5;
        }
        let add = Rect::new(x, y, add_w, h);
        let mut ax = add.right() + 5;
        let la = (la_w > 0).then(|| { let r = Rect::new(ax, y, la_w, h); ax += la_w + 5; r });
        let gen = (gen_w > 0).then(|| { let r = Rect::new(ax, y, gen_w, h); ax += gen_w + 5; r });
        let show = (show_w > 0).then(|| Rect::new(ax + 1, bottom.y, show_w - 6, bottom.h));
        StudioBottom { ch, refs, math, dec, add, show, la, gen, time, trig, panel }
    }

    /// The width a channel pill needs: the badge, the widest scale of the channels' units in the pill's font, and the
    /// chevron. The same for every channel, so a pill does not change its width with its scale.
    pub(super) fn studio_ch_pill_need(&self) -> i32 {
        let f = self.fonts();
        let scale = |c: usize| format!("500 m{}/div", self.m.ch[c].unit.name());
        let text = (0..n_ch()).map(|c| f.mb.width(&scale(c))).max().unwrap_or(0);
        // 8 to the badge of 42, 8 to the text, 8 to the chevron and its 20
        (86 + text).min(PILL_MAX_W)
    }

    /// The narrowest channel pill that still shows a scale whole: the badge and its margins (62 px, as the pill is
    /// drawn) and the widest scale of the channels' units in the small font.
    pub(super) fn studio_ch_pill_scale_min(&self) -> i32 {
        let f = self.fonts();
        62 + (0..n_ch()).map(|c| f.s.width(&format!("500 m{}", self.m.ch[c].unit.name()))).max().unwrap_or(0)
    }

    /// The live channels are not what SIM plays (it plays refs), so Studio shows no channel pills while it runs.
    pub(super) fn studio_ch_pills_hidden(&self) -> bool {
        self.m.layout == Layout::Studio && self.sim_mode()
    }

    /// The width of a source pill: `count` pills (5 px apart) share `room`, none wider than a channel pill was and none
    /// narrower than the smallest that still carries its name, so many pills shrink and a few stay roomy.
    pub(super) fn studio_pill_width(room: i32, count: i32) -> i32 {
        if count <= 0 { return PILL_MAX_W; }
        ((room - (count - 1) * 5) / count).clamp(PILL_MIN_W, PILL_MAX_W)
    }

    pub(super) fn studio_cards(&self) -> Cards {
        let b = self.studio_bottom();
        Cards { ch: b.ch, la: None, gen: None, refs: None, time: b.time, trig: b.trig }
    }

    pub(super) fn draw_studio_bottom(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let bottom = self.frame().bottom;
        cv.fill(bottom, BAR);
        self.draw_studio_badges(cv);
        cv.hline(bottom.x, bottom.right() - 1, bottom.y, LINE);
        let b = self.studio_bottom();
        let root = if self.menu_open { Some(self.menu_root()) } else { None };
        for c in 0..n_ch() {
            let r = b.ch[c];
            if r.w == 0 { continue; }
            let (col, ch) = (CH_COLORS[c], &self.m.ch[c]);
            let sel = root == Some(MenuId::Channel(c));
            let clip = self.channel_clipping(c);
            let border = if clip { RED } else if sel { ACCENT } else if ch.on { col } else { BTN_BORDER };
            chip(cv, r, if ch.on { BTN } else { BAR }, border);
            // the channel badge: filled while the channel is on; on / off: a dot in the corner
            cv.circle(r.x + 5, r.y + 5, 3, if ch.on { GREEN } else { DIM });
            cv.set_clip(r);
            // a narrow pill keeps the badge alone (CH1, then 1), a wider one adds the scale, the coupling line and the chevron
            let (label, bw) = self.studio_pill_badge(c, r.w);
            let (label, bw) = if f.sb.width(&label) + 4 <= bw { (label, bw) } else { ((c + 1).to_string(), bw) };
            let chev = r.w >= 130; // a narrower pill gives the chevron's room to its scale
            let by = r.y + BAR_DY;
            let badge = if r.w >= 70 { Rect::new(r.x + 8, by + 5, bw, 20) } else { Rect::new(r.x + (r.w - bw) / 2, by + 8, bw, 18) };
            // on without its trace (Hide trace): the badge as an outline, and the second line says so
            let hidden = ch.on && self.m.ch_hidden[c];
            cv.round(badge, 4, if ch.on { col } else { mix(BAR, col, 70) }, 255);
            if hidden { cv.round(badge.inset(1), 3, BTN, 255); }
            cv.text_in(&f.sb, badge, 1, &label, if hidden { col } else if ch.on { DARK_TEXT } else { mix(BAR, col, 200) });
            if r.w >= 70 {
                let full = if ch.on { format!("{}/div", si_short(ch.volts_per_div() as f64, ch.unit.name())) } else { "Off".into() };
                let short = if ch.on { si_short(ch.volts_per_div() as f64, ch.unit.name()) } else { "Off".into() };
                let (tx, tw) = (badge.right() + 8, (r.right() - if chev { 20 } else { 4 } - badge.right() - 8).max(1));
                cv.set_clip(Rect::new(badge.right() + 2, r.y, (tw + 6).min(r.right() - badge.right() - 2).max(1), r.h));
                // the second small line: coupling, probe and offset in the channel colour, dimmer; the offset goes first, then
                // the probe, then the line itself, and the scale loses its "/div" before it is dropped
                let two_lines = f.mb.size + f.s.size <= TWO_LINE_MAX;
                let text = if f.mb.width(&full) <= tw { full } else { short };
                if f.mb.width(&text) > tw {
                    // the small font, then without the space ("500mV"), then without the unit where its prefix says
                    // enough ("500m"). A scale that still has no room is left out: a number cut short is another
                    // number (cut by the pixel, 100 mV read "100n" on a unit, and cut by the character "1")
                    let tight = text.replace(' ', "");
                    // ("100mV" without its unit; "1V" has no prefix to keep, and "Off" is no scale)
                    let prefixed = ch.on && tight.chars().rev().nth(1).is_some_and(char::is_alphabetic);
                    let bare = prefixed.then(|| tight.chars().take(tight.chars().count() - 1).collect::<String>());
                    let fits = |t: &String| f.s.width(t) <= tw;
                    let shown = [Some(text.clone()), Some(tight.clone()), bare].into_iter().flatten().find(fits);
                    if let Some(text) = shown {
                        cv.text_in(&f.s, Rect::new(tx, r.y, tw, r.h), 0, &text, if ch.on { TEXT } else { DIM });
                    }
                } else {
                    let detail = if hidden { "trace hidden".to_string() } else { self.studio_ch_detail(c, tw) };
                    match Some(detail).filter(|d| ch.on && two_lines && f.s.width(d) <= tw) {
                        Some(d) => {
                            cv.text(&f.mb, tx, r.y + 13 + BAR_DY, &text, TEXT);
                            cv.text(&f.s, tx, r.y + 25 + BAR_DY, &d, mix(BAR, col, 170));
                        }
                        None => cv.text_in(&f.mb, Rect::new(tx, r.y, tw, r.h), 0, &text, if ch.on { TEXT } else { DIM }),
                    }
                }
            }
            cv.no_clip();
            if chev { chevron_down(cv, r.right() - 16, r.y + r.h / 2 - 2, DIM); }
        }
        for i in self.shown_refs() {
            let r = b.refs[i];
            let col = self.wave_color(WaveId::Ref(i));
            let active = self.selected_workspace_source() == Some(WaveId::Ref(i));
            chip(cv, r, BTN, if active { ACCENT } else { col });
            cv.circle(r.x + 5, r.y + 5, 3, GREEN);
            let scale = self.refs[i].as_ref().map_or(String::new(), |w| format!("{}/div", si_short(w.vpd as f64, "V")));
            self.studio_source_pill(cv, r, &format!("R{}", i + 1), &scale, &self.studio_ref_detail(i), col);
        }
        for k in 0..NMATH {
            let r = b.math[k];
            if r.w == 0 { continue; }
            let col = MATH_COLORS[k % MATH_COLORS.len()];
            chip(cv, r, BTN, if self.m.math_sel == k && self.selected_workspace_source().is_none() { ACCENT } else { col });
            cv.circle(r.x + 5, r.y + 5, 3, GREEN);
            self.studio_math_pill(cv, r, k, col);
        }
        // a decoder: its number in the colour of its source, its protocol, and its lines in the order of their roles
        for k in 0..DECODERS {
            let r = b.dec[k];
            if r.w == 0 { continue; }
            let m = self.m.dec_view(k);
            let col = self.dec_color(&m);
            let sel = root == Some(MenuId::Decode) && self.m.dec_sel == k;
            chip(cv, r, BTN, if sel { ACCENT } else { col });
            // the dot is dim while the channel of its line is off: the decoder is on and has nothing to read
            cv.circle(r.x + 5, r.y + 5, 3, if self.dec_shown(k) { GREEN } else { DIM });
            self.studio_source_pill(cv, r, &self.dec_tag(k), &m.dec_name(), &decband::dec_lines_text(&m), col);
        }
        if b.add.w > 0 {
            chip(cv, b.add, BTN, if self.studio.ch_picker { ACCENT } else { BTN_BORDER });
            // (it said "+ CH", "+ Add" or "Add source" by what was off, beside the Add of the top bar)
            let label = if b.add.w >= ADD_W { "+ Source" } else { "+" };
            cv.text_in(&f.sb, b.add, 1, label, if self.studio.ch_picker { ACCENT } else { TEXT });
        }
        // LA and GEN: a pill each, a dot for on / off and, for the generator, what it makes
        let aux_on = |m: &[MenuId]| root.is_some_and(|r| m.contains(&r));
        if let Some(r) = b.la {
            let (on, sel) = (self.m.la.on, aux_on(&[MenuId::La, MenuId::LaLines, MenuId::LaPat]));
            let col = la::line_color(0);
            chip(cv, r, if on { BTN } else { BAR }, if sel { ACCENT } else if on { col } else { BTN_BORDER });
            cv.circle(r.x + 5, r.y + 5, 3, if on { GREEN } else { DIM });
            cv.text_in(&f.sb, r, 1, "LA", if on { col } else { DIM });
        }
        if let Some(r) = b.gen {
            let (on, sel) = (self.m.afg_on, aux_on(&[MenuId::Afg]));
            chip(cv, r, if on { BTN } else { BAR }, if sel { ACCENT } else if on { GEN_COLOR } else { BTN_BORDER });
            cv.circle(r.x + 5, r.y + 5, 3, if on { GREEN } else { DIM });
            if on && r.w > GEN_W {
                let text = format!("{} {}", self.value(Item::AfgWave), self.value(Item::AfgFreq));
                cv.set_clip(r.inset(3));
                cv.text_in(&f.sb, Rect::new(r.x + 10, r.y, r.w - 12, r.h), 0, &format!("GEN"), GEN_COLOR);
                cv.text_in(&f.s, Rect::new(r.x + 40, r.y, r.w - 42, r.h), 0, &text, TEXT);
                cv.no_clip();
            } else {
                cv.text_in(&f.sb, r, 1, "GEN", if on { GEN_COLOR } else { DIM });
            }
        }
        if let Some(r) = b.show {
            chip(cv, Rect::new(r.x, r.y + 3, r.w, BAR_CHIP_H), BTN, BTN_BORDER);
            // the eye alone where the bar is full (`studio_bottom`)
            let named = r.w >= SHOW_W - 6;
            icon_eye(cv, if named { r.x + 15 } else { r.x + r.w / 2 }, r.y + r.h / 2, TEXT);
            // (it said "Meas", as the chip on the plot that opens the list does)
            if named { cv.text_in(&f.sb, Rect::new(r.x + 26, r.y, r.w - 28, r.h), 1, "Badges", TEXT); }
        }
        // the sections of the row are told apart by a line
        for x in [b.time.x - 5, b.trig.x - 5] {
            cv.vline(x, bottom.y + 6, bottom.bottom() - 6, LINE);
        }
        // time base: the scale, and under it what the TIME card of the Default layout says (the delay, or the zoom scale)
        let two = self.studio_pill_two_lines();
        let r = b.time;
        chip(cv, r, BTN, if root == Some(MenuId::Horizontal) { ACCENT } else { BTN_BORDER });
        let cy = r.y + r.h / 2;
        cv.hline(r.x + 10, r.x + 24, cy, TEXT);
        cv.line(r.x + 10, cy, r.x + 14, cy - 3, TEXT);
        cv.line(r.x + 10, cy, r.x + 14, cy + 3, TEXT);
        cv.line(r.x + 24, cy, r.x + 20, cy - 3, TEXT);
        cv.line(r.x + 24, cy, r.x + 20, cy + 3, TEXT);
        let scale = format!("{}/div", si_short(self.m.time_per_div() as f64, "s"));
        let label = if self.m.zoom && f.mb.width(&format!("Main {scale}")) <= r.w - 52 { format!("Main {scale}") } else { scale };
        let detail = self.studio_time_detail();
        if two && f.mb.width(&label) <= r.w - 52 && f.s.width(&detail) <= r.w - 52 {
            cv.text(&f.mb, r.x + 32, r.y + 13 + BAR_DY, &label, TEXT);
            cv.text(&f.s, r.x + 32, r.y + 25 + BAR_DY, &detail, if self.m.zoom || self.m.fine_t { ACCENT } else { DIM });
        } else {
            cv.text_fit(&f.mb, Rect::new(r.x + 32, r.y, r.w - 52, r.h), 0, &label, TEXT);
        }
        chevron_down(cv, r.right() - 16, cy - 2, DIM);
        // trigger: the slope, the source and the level; under them the type and the sweep, which the trigger card of
        // the Default layout names too. A pill of one line (larger text) has the type in front instead
        let r = b.trig;
        chip(cv, r, BTN, if root == Some(MenuId::Trigger) { ACCENT } else { BTN_BORDER });
        let source = self.m.trigger_wave();
        let col = self.wave_color(source);
        let edge = matches!(self.m.trig_type, TrigType::Edge | TrigType::Slope | TrigType::Timeout | TrigType::NthEdge | TrigType::Delay);
        let levels = if matches!(self.m.trig_type, TrigType::Runt | TrigType::Slope | TrigType::Window) {
            format!("{} / {}", si_short(self.m.trig_level2 as f64, "V"), si_short(self.m.trig_level as f64, "V"))
        } else { si_short(self.trig_level_shown() as f64, "V") };
        let src = self.trigger_source_label();
        // the base line of the main text: higher when the type and the sweep stand under it
        let base = r.y + BAR_DY + if two { 13 } else { 20 };
        // the text ends before the chevron
        let room = r.w - 9 - 20;
        let rest = if edge { 20 } else { 0 } + f.mb.width(&src) + 6 + f.mb.width(&levels);
        // one line: the type stands in front, and is cut short before the level is
        let kind = if two { String::new() } else { Self::studio_cut(&f.sb, &self.m.trig_type.name().to_uppercase(), room - rest - 6) };
        let mut x = r.x + 9;
        cv.set_clip(Rect::new(r.x + 2, r.y + 2, r.w - 2 - 20, r.h - 4));
        if !kind.is_empty() { x = cv.text(&f.sb, x, base, &kind, LABEL) + 6; }
        if edge {
            draw_slope(cv, x, base - 13, self.m.slope, col);
            x += 20;
        }
        x = cv.text(&f.mb, x, base, &src, col) + 6;
        // (the slope is the picture at the left; an arrow here said it a second time)
        let x = cv.text(&f.mb, x, base, &levels, TEXT) + 8;
        if two {
            // the type and the sweep, each a text of its own
            let (mut x, y) = (r.x + 9, r.y + 25 + BAR_DY);
            for (i, part) in self.studio_trig_detail().split(" \u{b7} ").enumerate() {
                if i > 0 { x = cv.text(&f.s, x, y, " \u{b7} ", LABEL); }
                x = cv.text(&f.s, x, y, part, LABEL);
            }
        } else {
            // one line: the sweep at the right, where it has the room. Auto, Normal or Single decide whether the
            // picture moves at all
            let sweep = match self.m.trig_mode { TrigMode::Auto => "Auto", TrigMode::Normal => "Normal", TrigMode::Single => "Single" };
            if x + f.s.width(sweep) <= r.right() - 24 {
                cv.text(&f.s, r.right() - 24 - f.s.width(sweep), base, sweep, DIM);
            }
        }
        cv.no_clip();
        chevron_down(cv, r.right() - 16, r.y + r.h / 2 - 2, DIM);
        self.draw_studio_ch_picker(cv);
    }

    /// The pills of the bottom bar have a small second line while both fonts fit the pill.
    pub(super) fn studio_pill_two_lines(&self) -> bool {
        let f = self.fonts();
        f.mb.size + f.s.size <= TWO_LINE_MAX
    }

    /// `s` as it fits `room` in the font `f`: whole, or cut with three dots, or nothing when not even two letters fit.
    pub(super) fn studio_cut(f: &Font, s: &str, room: i32) -> String {
        if f.width(s) <= room { return s.to_string(); }
        let mut t: String = s.to_string();
        while t.chars().count() > 2 && f.width(&format!("{t}...")) > room { t.pop(); }
        if f.width(&format!("{t}...")) <= room { format!("{}...", t.trim_end()) } else { String::new() }
    }

    /// The second line of the time pill, as the TIME card of the Default layout has it: the zoom scale while the zoom
    /// is on, else the delay. With the fine time base on it says so in the place of the word: the knob then moves
    /// in hundredths, and nothing else on the screen tells why.
    pub(super) fn studio_time_detail(&self) -> String {
        if self.m.zoom {
            format!("zoom {}/div", si_short(tscales()[self.m.zscale] as f64, "s"))
        } else if self.m.fine_t {
            format!("fine \u{b7} {}", si_short(self.m.delay as f64, "s"))
        } else {
            format!("delay {}", si_short(self.m.delay as f64, "s"))
        }
    }

    /// The second line of the trigger pill: the type and the sweep ("Edge {b7} Auto"), which the trigger card of the
    /// Default layout names.
    pub(super) fn studio_trig_detail(&self) -> String {
        format!("{} \u{b7} {}", self.m.trig_type.name(), self.value(Item::TrigMode))
    }

    /// The second line of a channel pill: "DC {b7} 1{d7} {b7} 4.75 V". Short of room the offset goes first, then the probe.
    pub(super) fn studio_ch_detail(&self, c: usize, room: i32) -> String {
        let ch = &self.m.ch[c];
        let f = self.fonts();
        let probe = format!("{}×", ch.probe);
        let full = format!("{} · {} · {}", ch.coupling.name(), probe, si_short(ch.offset as f64, self.m.unit(c)));
        if f.s.width(&full) <= room { return full; }
        let two = format!("{} · {}", ch.coupling.name(), probe);
        if f.s.width(&two) <= room { two } else { ch.coupling.name().to_string() }
    }

    /// What a ref pill says under its name: the signal or file name.
    pub(super) fn studio_ref_detail(&self, i: usize) -> String {
        self.refs.get(i).and_then(Option::as_ref).map(|r| r.label.clone()).unwrap_or_default()
    }

    /// A ref or math pill drawn as a channel pill is: the filled badge with its name, beside it what it shows (the
    /// scale, the operation) and under that a small line (the signal), then the chevron. A narrow pill keeps the
    /// badge alone, and a narrower one drops the letter of its name ("R10" becomes "10").
    pub(super) fn studio_source_pill(&self, cv: &mut Canvas, r: Rect, name: &str, main: &str, detail: &str, col: Rgb) {
        let f = self.fonts();
        cv.set_clip(r.inset(1));
        if r.w < 70 {
            let (name, _) = self.pill_name(name, r.w);
            let bw = (f.sb.width(&name) + 8).min(r.w - 4).max(8);
            let badge = Rect::new(r.x + (r.w - bw) / 2, r.y + 8 + BAR_DY, bw, 18);
            cv.round(badge, 4, col, 255);
            cv.text_in(&f.sb, badge, 1, &name, DARK_TEXT);
            cv.no_clip();
            return;
        }
        let (name, bw) = self.pill_name(name, r.w); let badge = Rect::new(r.x + 8, r.y + 5 + BAR_DY, bw, 20);
        cv.round(badge, 4, col, 255);
        cv.text_in(&f.sb, badge, 1, &name, DARK_TEXT);
        let chev = r.w >= 130; // a narrower pill gives the chevron's room to its scale
        let (tx, tw) = (badge.right() + 8, (r.right() - if chev { 20 } else { 4 } - badge.right() - 8).max(1));
        cv.set_clip(Rect::new(badge.right() + 2, r.y, (tw + 6).min(r.right() - badge.right() - 2).max(1), r.h));
        let two_lines = !detail.is_empty() && f.mb.size + f.s.size <= TWO_LINE_MAX && f.s.width(detail) <= tw;
        // a main line too long for the pill gives way to the small line, which is the signal's name
        // a scale loses its "/div" before it is cut
        let main = if f.mb.width(main) > tw { main.trim_end_matches("/div") } else { main };
        let main_fits = f.mb.width(main) <= tw;
        if two_lines && main_fits {
            cv.text(&f.mb, tx, r.y + 13 + BAR_DY, main, TEXT);
            cv.text(&f.s, tx, r.y + 25 + BAR_DY, detail, mix(BAR, col, 170));
        } else if main_fits && !main.is_empty() {
            cv.text_in(&f.mb, Rect::new(tx, r.y, tw, r.h), 0, main, TEXT);
        } else if !detail.is_empty() {
            cv.text_in(&f.s, Rect::new(tx, r.y, tw, r.h), 0, detail, mix(BAR, col, 200));
        } else {
            cv.text_in(&f.s, Rect::new(tx, r.y, tw, r.h), 0, main, TEXT);
        }
        cv.no_clip();
        if chev { chevron_down(cv, r.right() - 16, r.y + r.h / 2 - 2, DIM); }
    }

    /// The badges are hidden and there are some to show: the bottom bar keeps a button for them.
    pub(super) fn studio_badges_hidden(&self) -> bool {
        self.m.layout == Layout::Studio && self.studio.hide_badges
    }

    fn studio_bottom_tap(&mut self, x: i32, y: i32) {
        let b = self.studio_bottom();
        if b.show.is_some_and(|r| r.contains(x, y)) {
            self.studio.hide_badges = false;
            self.studio.md.hide_undo = None;
            self.relayout();
            return;
        }
        if b.add.w > 0 && b.add.contains(x, y) {
            self.studio.ch_picker = !self.studio.ch_picker;
            return;
        }
        for (pill, menu) in [(b.la, MenuId::La), (b.gen, MenuId::Afg)] {
            if pill.is_some_and(|r| Self::hit40(r).contains(x, y)) {
                return self.toggle_menu(menu);
            }
        }
        if let Some(k) = (0..NMATH).find(|&k| b.math[k].w > 0 && Self::hit40(b.math[k]).contains(x, y)) {
            // like a channel pill: select it and open its sheet; a second tap closes it
            let (prev, open) = (self.m.math_sel, self.menu_open && self.menu == MenuId::Math);
            self.m.math_select(k);
            if open && prev != k { return; }
            return self.toggle_menu(MenuId::Math);
        }
        if let Some(k) = (0..DECODERS).find(|&k| b.dec[k].w > 0 && Self::hit40(b.dec[k]).contains(x, y)) {
            // like a math pill: that decoder and its Decode sheet; a second tap closes it
            let (prev, open) = (self.m.dec_sel, self.menu_open && self.menu == MenuId::Decode);
            if prev != k { self.select_option(Item::DecSel, &self.dec_tag(k)); }
            if open && prev != k { return; }
            return self.toggle_menu(MenuId::Decode);
        }
        for c in 0..n_ch() {
            if !b.ch[c].contains(x, y) { continue; }
            if !self.m.ch[c].on {
                // the pill of a channel that is off: on and selected, no sheet
                if self.menu_open { self.close_menu(); }
                self.m.ch[c].on = true;
                self.settings_changed();
                self.relayout();
                self.select_workspace_source(WaveId::Ch(c));
                return;
            }
            // A tap on the pill of a channel that is on selects the channel: the knobs and its lane follow, and
            // nothing opens or is switched. A tap on the pill that is selected opens its sheet, so two taps on any
            // pill end with its sheet open, as two presses of a channel key do. The coloured badge was the
            // channel's switch and the rest of the pill opened the sheet at once: a tap meant to select a channel
            // switched it off, with its math trace and its measurements, or covered the plot (asked from a unit,
            // 2026-11-18). Off is the first row of a hold on the pill, a swipe of the pill off the screen, the
            // Display row of the sheet and the key.
            let selected = self.selected_workspace_source() == Some(WaveId::Ch(c));
            let sheet = self.menu_open && matches!(self.menu_root(), MenuId::Channel(_));
            let own = self.menu_open && self.menu_root() == MenuId::Channel(c);
            let again = self.studio.pill_tap.is_some_and(|(k, at)| k == c && self.now - at < PILL_AGAIN);
            self.studio.pill_tap = Some((c, self.now));
            self.select_workspace_source(WaveId::Ch(c));
            if own {
                // the second tap of a double tap does not close what the first one opened
                if !again { self.close_menu(); }
            } else if sheet || selected {
                // (the sheet of another channel that is open goes to this one)
                self.open_menu(MenuId::Channel(c));
            }
            return;
        }
        if let Some(i) = self.shown_refs().into_iter().find(|&i| b.refs[i].contains(x, y)) {
            let (prev, open) = (self.ref_page.sel, self.menu_open && self.menu == MenuId::Ref);
            self.select_workspace_source(WaveId::Ref(i));
            if open && prev != i { return; }
            return self.toggle_menu(MenuId::Ref);
        }
        if b.time.contains(x, y) {
            self.toggle_menu(MenuId::Horizontal);
        } else if b.trig.contains(x, y) {
            self.toggle_menu(MenuId::Trigger);
        }
    }

    /// The picker over the "+ CH" chip: one row per channel that is off.
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub(super) fn studio_ch_picker(&self) -> Option<Vec<(usize, Rect)>> {
        let (chans, _) = self.studio_picker_rows()?;
        (!chans.is_empty()).then_some(chans)
    }

    /// The popup of the "+ CH" chip: the channels that are off, then (set apart by a line) Add math, Add ref and
    /// Add bus. Rows 44 px high, the panel over the chip.
    pub(super) fn studio_picker_rows(&self) -> Option<(Vec<(usize, Rect)>, Vec<(PickAdd, Rect)>)> {
        if !self.studio.ch_picker { return None; }
        let b = self.studio_bottom();
        // the channels that are off; with every channel on, all of them (to turn off)
        let mut off: Vec<usize> = (0..n_ch()).filter(|&c| !self.m.ch[c].on).collect();
        if off.is_empty() { off = (0..n_ch()).collect(); }
        let (w, row) = (170, 44);
        let sep = if off.is_empty() { 0 } else { 10 };
        let total = (off.len() + PICK_ADDS.len()) as i32 * row + sep;
        let top = self.frame().bottom.y - 8 - total - 8;
        let x = b.add.x.min(self.scr.w - w - 4);
        let chans = off.iter().enumerate().map(|(i, &c)| (c, Rect::new(x + 4, top + 4 + i as i32 * row, w - 8, row))).collect();
        let y0 = top + 4 + off.len() as i32 * row + sep;
        let extras = PICK_ADDS.iter().enumerate().map(|(i, &k)| (k, Rect::new(x + 4, y0 + i as i32 * row, w - 8, row))).collect();
        Some((chans, extras))
    }

    /// A new math channel starts useful: the first free one, an addition of the first channel that is on and the next
    /// one (or the same, for one), so its trace is on the screen at once.
    pub(super) fn studio_math_start(&mut self) {
        let free = if self.m.math == MathOp::Off { Some(self.m.math_sel) } else { (0..NMATH).find(|&k| self.m.math_ch(k).op == MathOp::Off) };
        // every channel is defined: one hidden in this view comes back
        let Some(k) = free else {
            if let Some(k) = (0..NMATH).find(|&k| self.m.math_ch(k).op != MathOp::Off && self.m.math_hidden[k]) { self.m.math_hidden[k] = false; self.relayout(); }
            return;
        };
        if k != self.m.math_sel { self.m.math_select(k); }
        let on: Vec<usize> = (0..n_ch()).filter(|&c| self.m.ch[c].on).collect();
        let Some(&a) = on.first() else { return };
        let b = on.get(1).copied().unwrap_or(a);
        self.m.math = MathOp::Add;
        self.math.family = MathOp::Add.family();
        (self.m.math_a, self.m.math_b) = (a, b);
        self.m.math_offset = 0.0;
        self.math_autoscale();
        self.settings_changed();
        self.relayout();
    }

    /// What a row of the popup's second group opens: the flows of the + Add panel and the analysis menus.
    fn studio_pick_add(&mut self, k: PickAdd) {
        match k {
            PickAdd::Math => { self.studio_math_start(); self.open_submenu(MenuId::Math); }
            PickAdd::Ref => self.open_submenu(MenuId::Ref),
            PickAdd::Bus => self.open_submenu(MenuId::Decode),
        }
    }

    fn draw_studio_ch_picker(&self, cv: &mut Canvas) {
        let Some((rows, extras)) = self.studio_picker_rows() else { return };
        let f = self.fonts();
        let first = rows.first().map_or(extras[0].1, |r| r.1);
        let last = extras[extras.len() - 1].1;
        let p = Rect::new(first.x - 4, first.y - 4, first.w + 8, last.bottom() - first.y + 8 + if rows.is_empty() { 0 } else { 0 });
        cv.round(p, 8, BTN_BORDER, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        if !rows.is_empty() { cv.hline(extras[0].1.x + 6, extras[0].1.right() - 6, extras[0].1.y - 5, LINE); }
        for (k, r) in &extras {
            let col = match k { PickAdd::Math => MATH_COLORS[0], PickAdd::Ref => REF_COLORS[0].1, PickAdd::Bus => super::bus::BUS };
            let badge = Rect::new(r.x + 8, r.y + 10, 6, 24);
            cv.round(badge, 3, col, 255);
            cv.text_in(&f.m, Rect::new(badge.right() + 10, r.y, r.w - 30, r.h), 0, k.label(), TEXT);
        }
        for (c, r) in rows {
            let badge = Rect::new(r.x + 8, r.y + 10, 42, 24);
            cv.round(badge, 4, mix(BAR, CH_COLORS[c], 200), 255);
            cv.text_in(&f.sb, badge, 1, &format!("CH{}", c + 1), DARK_TEXT);
            cv.text_in(&f.m, Rect::new(badge.right() + 10, r.y, r.w - 64, r.h), 0, if self.m.ch[c].on { "Turn off" } else { "Turn on" }, TEXT);
        }
    }

    /// A tap in Studio's own bars and panels. False: the tap belongs to the plot, a menu or a dialog.
    pub(super) fn studio_tap(&mut self, x: i32, y: i32) -> bool {
        self.studio_sync();
        // the info card is above everything, the Add panel included
        if self.info_card_tap(x, y) { return true; }
        if self.power.open || self.wlan.osk.is_some() || self.dialog.is_some() || self.load_slot_ask { return false; }
        if self.menu_open && self.menu_rect().is_some_and(|d| d.contains(x, y)) { return false; }
        if self.studio_sz_taps(x, y) { return true; }
        if self.studio_add_tap(x, y) { return true; }
        if self.studio_ctx_tap(x, y) { return true; }
        if self.studio.ch_picker {
            let (rows, extras) = self.studio_picker_rows().unzip();
            self.studio.ch_picker = false;
            if let Some((k, _)) = extras.into_iter().flatten().find(|(_, r): &(PickAdd, Rect)| r.contains(x, y)) {
                self.studio_pick_add(k);
                return true;
            }
            if let Some((c, _)) = rows.into_iter().flatten().find(|(_, r): &(usize, Rect)| r.contains(x, y)) {
                let on = !self.m.ch[c].on;
                self.m.ch[c].on = on;
                self.settings_changed();
                self.relayout();
                if on { self.select_workspace_source(WaveId::Ch(c)); }
                return true;
            }
            // a tap on the chip that opened it only closes it; any other tap closes it and goes on
            if self.studio_bottom().add.contains(x, y) { return true; }
        }
        if self.md_hide_bar().is_some_and(|(bar, _)| bar.contains(x, y)) && self.md_undo_tap(x, y) { return true; }
        if self.md_undo_bar().is_some_and(|(bar, _)| bar.contains(x, y)) && self.md_undo_tap(x, y) { return true; }
        if self.studio_popup_tap(x, y) { return true; }
        if let Some(k) = self.dock_float_at(x, y) { return self.dock_float_tap(k, x, y); }
        if self.studio_zoom_tap(x, y) { return true; }
        if self.studio_badge_tap(x, y) { return true; }
        if self.studio_panel_tap(x, y) { return true; }
        if y >= self.frame().bottom.y - 2 {
            self.studio_bottom_tap(x, self.bar_y(y));
            return true;
        }
        if y < self.frame().top.bottom() {
            self.studio_top_tap(x, y);
            return true;
        }
        // the trigger level tag opens the slider (a tap that reaches here was on nothing else)
        if !self.view_on() && !self.m.touch_lock && self.trigger_tag_hit().is_some_and(|r| r.contains(x, y)) {
            self.close_menu();
            self.studio.trig_slider = true;
            return true;
        }
        false
    }

    /// The status pill: Run / Stop as in every layout. While the SIM panel is open and nothing plays yet, Run plays
    /// the SIM source (the same action as the panel's Play).
    pub(super) fn studio_run_stop(&mut self) {
        if self.studio.view == StudioView::Sim && !self.record_mode() { return self.sim_activate_play(); }
        self.key(key::RUNSTOP);
    }

    /// Single: one acquisition, or one SIM frame that is then held.
    pub(super) fn studio_single(&mut self) {
        if self.studio.view == StudioView::Sim && !self.record_mode() {
            if self.sim_generating() { return; }
            return self.sim_request_ui(false);
        }
        self.key(key::SINGLE);
    }

    fn sim_activate_play(&mut self) {
        self.sim_panel_play();
    }

    fn studio_top_tap(&mut self, x: i32, y: i32) {
        if self.studio_search_chip_tap(x, y) { return; }
        let t = self.studio_top();
        let hit = |r: Rect| self.top_hit(r).contains(x, y);
        if t.badge.w > 0 && hit(t.badge) {
            self.return_to_live();
        } else if t.vbox.w > 0 && hit(t.vbox) {
            self.view_box_tap(x, y);
        } else if hit(t.run) {
            self.studio_run_stop();
        } else if t.single.w > 0 && hit(t.single) {
            self.studio_single();
        } else if t.autoset.w > 0 && hit(t.autoset) {
            self.key(key::AUTO);
        } else if t.clear.w > 0 && hit(t.clear) {
            self.key(key::CLEAR);
        } else if t.force.w > 0 && hit(t.force) {
            self.key(key::FORCE);
        } else if t.sim.w > 0 && hit(t.sim) {
            if self.menu_open { self.close_menu(); }
            // live with a reference shown: the button plays it; in SIM the chip opens the SIM panel
            if self.sim_return_button() { self.sim_panel_play(); } else { self.studio_set_view(StudioView::Sim); }
        } else if hit(t.lan) || (t.wlan.w > 0 && hit(t.wlan)) {
            // the address and the services are on the Network page
            self.toggle_menu(MenuId::Network);
        } else if t.usb.w > 0 && hit(t.usb) {
            self.toggle_menu(MenuId::Storage);
        } else if hit(t.acq) {
            self.toggle_menu(if self.record_mode() { MenuId::Ref } else { MenuId::Acquire });
        } else if hit(t.add) {
            if self.studio.add_menu { self.studio.add_menu = false; } else { self.studio_add_open(); }
        } else if hit(t.measure) {
            self.open_meas_list();
        } else if t.cursors.w > 0 && hit(t.cursors) {
            self.studio_cursors_tap();
        } else if hit(t.display) {
            self.toggle_menu(MenuId::Display);
        } else if hit(t.storage) {
            self.toggle_menu(MenuId::Storage);
        } else if hit(t.gear) {
            if self.m.touch_lock { self.key(key::TOUCHLOCK); } else { self.toggle_menu(MenuId::Main); }
        }
    }
}

/// The cursor band and its C1 / C2 tags.
pub(super) const BAND: Rgb = rgb(0x2d, 0xd4, 0xcf);
/// Height of the T marker and the cursor tags, drawn on the top edge of the scale (no row of their own).
const TAG_H: i32 = 24;

impl Ui {
    /// Studio draws the trigger position and the cursor tags on the top edge of the scale.
    pub(super) fn studio_tags_on(&self) -> bool {
        self.m.layout == Layout::Studio && !self.m.zoom && !self.full_on()
    }

    /// Where a T or cursor tag centred on `x` sits: inside the scale, on its top edge.
    pub(super) fn studio_tag_rect(&self, x: i32) -> Rect {
        let g = self.plot.grid;
        Rect::new(x.clamp(g.x + 12, g.right() - 12) - 12, g.y + 1, 24, TAG_H)
    }

    /// Screen x of a time (relative to the trigger) in the main view of the plot.
    pub(super) fn studio_x_of_time(&self, t: f64) -> i32 {
        let g = self.plot.grid;
        let (tpd, delay) = self.view();
        g.x + ((0.5 + (t - delay as f64) / (HDIV as f64 * tpd as f64)) * g.w as f64).round() as i32
    }

    /// The zero level of a source in the window that is being drawn, as a screen line: the ground of a channel, the
    /// zero of a reference or of a math channel. None for a source this window does not draw, and for a spectrum.
    pub(super) fn studio_zero_y(&self, source: super::workspace::Trace) -> Option<i32> {
        use super::workspace::Trace;
        let y = match source {
            Trace::Channel(c) => { if !self.m.ch[c].on || !self.shows(c) { return None; } self.y_of_volts(c, 0.0) }
            Trace::Reference(i) => {
                let r = self.refs.get(i)?.as_ref().filter(|r| r.on && r.vpd > 0.0)?;
                if self.ref_win_of(i) != self.plot.cur_win { return None; }
                self.zoom_y_of_divs(self.plot.area, r.offset / r.vpd).clamp(-1e4, 1e4).round() as i32
            }
            Trace::Math(k) => {
                if !self.m.math_shown(k) || self.math_win_of(k) != self.plot.cur_win { return None; }
                let c = self.m.math_ch(k);
                if c.op == MathOp::Fft { return None; }
                self.zoom_y_of_divs(self.plot.area, c.offset).clamp(-1e4, 1e4).round() as i32
            }
        };
        Some(y)
    }

    /// Draws the handles of the window that is being drawn: an outlined badge with the name of each source on its
    /// zero level (`studio_handle_rects`). Channels, references and math channels have the same one, in Stacked and
    /// in Overlay; they are drawn after every trace, so none runs through a name.
    pub(super) fn studio_draw_handles(&self, cv: &mut Canvas) {
        use super::workspace::Trace;
        for (source, r) in self.studio_handle_rects() {
            let (name, col) = (self.studio_handle_name(source), match source {
                Trace::Channel(c) => CH_COLORS[c],
                Trace::Reference(i) => self.wave_color(WaveId::Ref(i)),
                Trace::Math(k) => MATH_COLORS[k % MATH_COLORS.len()],
            });
            match source {
                Trace::Channel(c) => self.studio.lane_badges.borrow_mut().push((c, r)),
                _ => self.studio.handles.borrow_mut().push((source, r)),
            }
            cv.round(r, 4, col, 255);
            cv.round(r.inset(1), 3, BG, 255);
            cv.text_in(&self.fonts().sb, r, 1, &name, col);
        }
    }

    /// The T marker over the lanes, the cursor tags and the shaded band between the cursors.
    pub(super) fn draw_studio_overlay(&self, cv: &mut Canvas) {
        if !self.studio_tags_on() || self.view_on() || self.eye_open() { return; }
        let f = self.fonts();
        let g = self.plot.grid;
        cv.set_clip(Rect::new(g.x - 12, g.y, g.w + 24, g.h));
        // the shaded band between two time cursors
        if matches!(self.m.cursor, CursorMode::Time | CursorMode::Track | CursorMode::Xy) {
            // a cursor keeps its time while zoomed: its fraction may lie outside the view, then it is not drawn
            let fr = [self.m.cur_a, self.m.cur_b];
            let xs = fr.map(|frac| g.x + (frac.clamp(0.0, 1.0) * g.w as f32) as i32);
            let (x0, x1) = (xs[0].min(xs[1]), xs[0].max(xs[1]));
            cv.set_clip(Rect::new(g.x, g.y, g.w, g.h));
            let off_same_side = (fr[0] < 0.0 && fr[1] < 0.0) || (fr[0] > 1.0 && fr[1] > 1.0);
            for &lane in &self.plot.wins {
                if !off_same_side { cv.blend(Rect::new(x0, lane.y, (x1 - x0).max(1), lane.h), BAND, 46); }
            }
            cv.set_clip(Rect::new(g.x - 12, g.y, g.w + 24, g.h));
            for (k, (x, name)) in [(xs[0], "X1"), (xs[1], "X2")].into_iter().enumerate() {
                if !(0.0..=1.0).contains(&fr[k]) { continue; }
                let r = self.studio_tag_rect(x);
                cv.round(r, 3, BAND, 255);
                cv.text_in(&f.sb, r, 1, name, DARK_TEXT);
                if self.workspace.cursor_sel == Some(k) { cv.round_outline(Rect::new(r.x - 2, r.y - 2, r.w + 4, r.h + 4), 3, ACCENT); }
                cv.vline(x, r.bottom(), r.bottom() + 3, BAND);
            }
        }
        // T: where the trigger is in the record, dashed through every lane; at an edge when it is off the screen
        let tx = self.studio_x_of_time(0.0);
        let r = self.studio_tag_rect(tx);
        cv.round(r, 3, AMBER, 255);
        cv.round(r.inset(1), 2, BG, 255);
        cv.text_in(&f.sb, r, 1, "T", AMBER);
        if (g.x..g.right()).contains(&tx) {
            cv.triangle([(tx - 3, r.bottom()), (tx + 3, r.bottom()), (tx, r.bottom() + 3)], AMBER);
            for &lane in &self.plot.wins {
                let y0 = if lane.y <= r.bottom() { r.bottom() + 4 } else { lane.y };
                cv.dashed_vline(tx, y0, lane.y + lane.h - 1, 4, 4, AMBER);
            }
        } else if tx < g.x {
            cv.triangle([(r.x - 1, r.y + r.h / 2), (r.x + 4, r.y + 2), (r.x + 4, r.bottom() - 2)], AMBER);
        } else {
            cv.triangle([(r.right() + 1, r.y + r.h / 2), (r.right() - 4, r.y + 2), (r.right() - 4, r.bottom() - 2)], AMBER);
        }
        cv.no_clip();
    }
}

/// Height of the row of pinned badges under the lanes.
const ROW_H: i32 = 40;
const ROW_H_COMPACT: i32 = 26;
/// Lines of badges the row has at most.
const BADGE_LINES: i32 = 2;
/// Width of the previous and next buttons of the search in the top bar.
pub(super) const SEARCH_ARROW_W: i32 = 46;
/// Width of the action popup of a badge, and the height of its rows.
const POPUP_W: i32 = 300;
const POPUP_ROW: i32 = super::workspace::MENU_ROW;
/// Two taps on a pill within this time are a double tap: the second does not close the sheet the first opened.
const PILL_AGAIN: f64 = 0.45;

/// A thumbtack: a flat cap, a barrel, a rim and a needle. Filled when pinned, outline when not. `x` is the left edge of the
/// 12 px wide glyph and `cy` its middle (it is 17 px high).
pub(super) fn icon_pin(cv: &mut Canvas, x: i32, cy: i32, filled: bool, c: Rgb) {
    let (cap, barrel, rim) = (Rect::new(x + 1, cy - 8, 10, 4), Rect::new(x + 3, cy - 4, 6, 5), Rect::new(x + 1, cy + 1, 10, 2));
    if filled {
        cv.fill(cap, c);
        cv.fill(barrel, c);
        cv.fill(rim, c);
        cv.fill(Rect::new(x + 5, cy + 3, 2, 6), c);
    } else {
        cv.round_outline(cap, 1, c);
        cv.round_outline(barrel, 1, c);
        cv.fill(rim, c);
        cv.vline(x + 5, cy + 3, cy + 8, c);
    }
}

fn icon_eye(cv: &mut Canvas, cx: i32, cy: i32, c: Rgb) {
    cv.round_outline(Rect::new(cx - 8, cy - 5, 17, 11), 5, c);
    cv.circle(cx, cy, 2, c);
}

impl Ui {
    /// The measurements in the order of the list: single, pair, catalogue.
    pub(super) fn studio_keys(&self) -> Vec<MeasKey> {
        let m = &self.m;
        m.meas.iter().map(|&(k, id)| MeasKey::One(k, id))
            .chain(m.meas2.iter().map(|&(k, a, b)| MeasKey::Pair(k, a, b)))
            .chain(m.cat.iter().map(|&(i, a, b, c, d)| MeasKey::Cat(i, a, b, c, d)))
            .collect()
    }

    /// A source that shows a signal now: a channel that is on, a ref that is shown. While SIM plays references the
    /// channels stay on and are not acquired: they show no signal, and their measurements were a row of "----" for
    /// each of them under the simulated bus.
    pub(super) fn studio_source_live(&self, id: WaveId) -> bool {
        match id {
            WaveId::Ch(c) => c < n_ch() && self.m.ch[c].on && self.sim_refs.is_empty(),
            WaveId::Ref(i) => self.refs.get(i).and_then(Option::as_ref).is_some_and(|r| r.on),
        }
    }

    /// The source a measurement reads first.
    pub(super) fn studio_key_source(key: MeasKey) -> WaveId {
        match key { MeasKey::One(_, id) | MeasKey::Pair(_, id, _) | MeasKey::Cat(_, id, _, _, _) => id }
    }

    /// The measurements of the badge row: the pinned ones (the row shows as many as fit, a "+N" chip counts the rest).
    pub(super) fn studio_badge_keys(&self) -> Vec<MeasKey> {
        self.studio_pinned()
    }

    /// The measurements that are pinned: every one the user has not unpinned, so a measurement is pinned from the
    /// moment it is added. Only those of a source that is live: no "CH1 ----" while CH1 is off and a ref is the signal.
    pub(super) fn studio_pinned(&self) -> Vec<MeasKey> {
        let keys: Vec<MeasKey> = self.studio_keys().into_iter().filter(|k| self.studio_source_live(Self::studio_key_source(*k))).collect();
        match &self.studio.pinned {
            Some(pinned) => keys.into_iter().filter(|k| pinned.contains(k)).collect(),
            None => keys.into_iter().filter(|k| !self.studio.unpinned.contains(k)).collect(),
        }
    }

    /// Measurements of a ref that was hidden or removed leave the list with it.
    pub(super) fn studio_prune_ref_rows(&mut self) {
        let gone = |s: &Ui, id: WaveId| matches!(id, WaveId::Ref(_)) && !s.studio_source_live(id);
        if !(self.m.meas.iter().any(|&(_, id)| gone(self, id)) || self.m.meas2.iter().any(|&(_, a, b)| gone(self, a) || gone(self, b))) { return; }
        let (meas, meas2) = (self.m.meas.clone(), self.m.meas2.clone());
        self.m.meas = meas.into_iter().filter(|&(_, id)| !gone(self, id)).collect();
        self.m.meas2 = meas2.into_iter().filter(|&(_, a, b)| !gone(self, a) && !gone(self, b)).collect();
        let keys = self.studio_keys();
        if let Some(p) = &mut self.studio.pinned { p.retain(|k| keys.contains(k)); }
        self.studio.unpinned.retain(|k| keys.contains(k));
        self.studio.floated.retain(|k| keys.contains(k));
        self.studio.popup = None;
        self.studio.panel.scroll_top();
        self.relayout();
    }

    pub(super) fn studio_is_pinned(&self, key: MeasKey) -> bool {
        self.studio_pinned().contains(&key)
    }

    pub(super) fn studio_set_pin(&mut self, key: MeasKey, on: bool) {
        // an explicit list (an older set-up) becomes what it leaves out, so what is added from now on is pinned
        if let Some(pinned) = self.studio.pinned.take() {
            self.studio.unpinned = self.studio_keys().into_iter().filter(|k| !pinned.contains(k)).collect();
        }
        self.studio.unpinned.retain(|k| *k != key);
        if !on { self.studio.unpinned.push(key); }
        self.relayout();
    }

    /// Index of a measurement in the list (`meas_rows`).
    pub(super) fn studio_index(&self, key: MeasKey) -> Option<usize> {
        self.studio_keys().iter().position(|k| *k == key)
    }

    pub(super) fn studio_owns_measurements(&self) -> bool {
        self.m.layout == Layout::Studio
    }

    /// The Measurements panel is on screen (docked or floating): it lists every measurement, so the row has nothing to add.
    pub(super) fn studio_panel_shown(&self) -> bool {
        matches!(self.studio.view, StudioView::Docked | StudioView::Floating | StudioView::Offline)
    }

    /// Whether the row under the lanes is there: pinned measurements to show and the panel closed, unless hidden. The cursor
    /// read-out is never in it (the plot has its own).
    pub(super) fn studio_badges_visible(&self) -> bool {
        self.m.layout == Layout::Studio && !self.studio.hide_badges
            && !self.studio_pinned().is_empty() && !self.studio_panel_shown()
    }

    /// With no row, the way to the panel: a slim chip at the bottom right of the plot (Waveform view, some measurements).
    pub(super) fn studio_meas_chip(&self) -> Option<Rect> {
        if self.m.layout != Layout::Studio || self.studio.view != StudioView::Waveform || self.studio_badges_visible() || self.full_on()
            || self.measurement_count() == 0 { return None; }
        let w = self.dock_wave_rect();
        let wide = 28 + (74.0 * self.studio_meas_slide()) as i32;
        Some(Rect::new(w.right() - 8 - wide, w.bottom() - 26 - 34, wide, 34))
    }

    /// Height the badge row takes from the lanes.
    pub(super) fn studio_row_h(&self) -> i32 {
        if self.studio_badges_visible() { self.studio_row_px() } else { 0 }
    }

    /// The badge row: under the lanes, as wide as they are.
    pub(super) fn studio_row(&self) -> Rect {
        let (x, w) = self.plot_span();
        let h = self.studio_row_px();
        Rect::new(x, self.dock_wave_rect().bottom() - h, w, h)
    }

    /// Height of the badge row, and of the chips in it. Badges that do not fit one line get a second one before any of
    /// them goes behind "+N".
    pub(super) fn studio_row_px(&self) -> i32 {
        let base = if self.studio.compact { ROW_H_COMPACT } else { ROW_H };
        if self.studio_classic() && !self.studio_pinned().is_empty() { base.max(self.classic_row_px()) } else { base + (self.studio_badge_lines() - 1) * (base - 2) }
    }
    pub(super) fn studio_chip_h(&self) -> i32 {
        if self.studio_classic() && !self.studio_pinned().is_empty() { return self.studio_row_px() - 6; }
        (if self.studio.compact { ROW_H_COMPACT } else { ROW_H }) - 6
    }

    /// Lines of badges in the row: one, or two when they do not fit one.
    pub(super) fn studio_badge_lines(&self) -> i32 {
        1 + self.studio.badge_wrap.clamp(0, BADGE_LINES - 1)
    }

    /// The lines the row needs beyond its first, from the names of the badges and the room alone: every value counts
    /// as wide as "0.000 MHz", so a value that grows by a digit moves no badge to the next line and the plot keeps its
    /// height. The classic strip: the lines its sources wrap onto (classicbadges.rs).
    fn studio_badge_wrap_now(&self) -> i32 {
        if self.m.layout != Layout::Studio || !self.studio_badges_visible() { return 0; }
        let f = self.fonts();
        let rows = self.meas_rows();
        let names: Vec<(MeasKey, i32)> = self.studio_badge_keys().into_iter().filter_map(|key| {
            let r = self.studio_index(key).and_then(|i| rows.get(i)).filter(|r| !self.meas_row_hidden(r))?;
            Some((key, f.s.width(&self.studio_badge_name(key, &r.name))))
        }).collect();
        let (span_x, span_w) = self.plot_span();
        // the row ends before the buttons at its right (`studio_row_limit`, without the row's own rectangle)
        let (x0, limit) = (span_x + 4, span_x + span_w - self.studio_row_tail());
        let value = f.sb.width("0.000 MHz");
        if self.studio_classic() { return self.classic_wrap_now(span_w); }
        let mut x = x0;
        for (_, name) in &names {
            let w = 16 + name + 8 + value + 10;
            if x + w > limit { return BADGE_LINES - 1; }
            x += w + 5;
        }
        0
    }
    /// Width of the measurement panel and height of its rows: smaller in the compact view (not the SIM panel).
    pub(super) fn studio_panel_w(&self) -> i32 {
        if self.studio.compact && self.studio.view != StudioView::Sim { 210 } else { MEAS_PANEL_W_FULL }
    }
    pub(super) fn studio_panel_row(&self) -> i32 {
        if self.studio_detailed() { return self.detail_row_h(); }
        if self.panel_is_cards() { super::measpanel::CARD_H + 4 } else { super::measpanel::CELL_H + 2 }
    }
    /// The one chip at the right end of the row: how many measurements there are, or "+N" when N pinned ones have no
    /// place on the row, and an arrow. It opens the table of the measurements. It is as small as a badge: the row had
    /// a button "Meas . N" here over its whole height and a "+N" chip beside it, which did the same, and the two took
    /// the room of two measurements. Studio badges: on the last line (the row grows upwards: the chip stays); the
    /// classic strip: the last thing of a line that has the room after its pairs (`classic_layout`).
    pub(super) fn studio_list_chip(&self) -> Rect {
        let r = self.studio_row();
        if self.studio_classic() && !self.studio_pinned().is_empty() {
            return self.classic_layout().chip;
        }
        let h = self.studio_chip_h();
        Rect::new(r.right() - 4 - LIST_W, r.bottom() - 3 - h, LIST_W, h)
    }

    /// What the button at the right end of the row takes from it (`studio_row_limit`, without the row's rectangle).
    fn studio_row_tail(&self) -> i32 {
        4 + LIST_W + 6
    }

    /// Studio or Classic badges (Measure > Badge style).
    pub(super) fn studio_set_classic(&mut self, on: bool) {
        self.m.studio_classic = on;
        self.studio.popup = None;
        self.studio.badge_first = 0;
        self.relayout();
    }

    /// Hides the row of badges (Display > Screen > Pinned badges). The Undo bar and the chip of the bottom bar bring it back.
    pub(super) fn studio_hide_badges(&mut self) {
        self.studio.hide_badges = true;
        self.studio.md.hide_undo = Some(self.now);
        self.relayout();
    }

    /// Where the badges end: before the chip at the end of the row.
    pub(super) fn studio_row_limit(&self) -> i32 {
        self.studio_list_chip().x - 6
    }

    /// The row of badges as (measurement, rectangle), as many as fit.
    pub(super) fn studio_badges(&self) -> Vec<(MeasKey, Rect)> {
        if self.studio_classic() { return self.classic_layout().pairs; }
        self.studio_badge_fit().0
    }

    /// The badges that have a place in the lines of the row, and how many have none (the chip at the end of the row
    /// says how many).
    fn studio_badge_fit(&self) -> (Vec<(MeasKey, Rect)>, usize) {
        let f = self.fonts();
        let row = self.studio_row();
        let rows = self.meas_rows();
        let limit = self.studio_row_limit();
        let (ch, lines) = (self.studio_chip_h(), self.studio_badge_lines());
        let first = self.studio.badge_first.min(self.studio_badge_keys().len().saturating_sub(1));
        let keys: Vec<(MeasKey, i32)> = self.studio_badge_keys().into_iter().skip(first).filter_map(|key| {
            let r = self.studio_index(key).and_then(|i| rows.get(i)).filter(|r| !self.meas_row_hidden(r))?;
            Some((key, 16 + f.s.width(&self.studio_badge_name(key, &r.name)) + 8 + f.sb.width(&r.text()).max(f.sb.width("0.000 MHz")) + 10))
        }).collect();
        let place = || {
            let (mut x, mut line, mut out) = (row.x + 4, 0, Vec::new());
            for &(key, w) in &keys {
                // the chip is on the last line: the lines above it run to the end of the row
                let end = |line: i32| if line == lines - 1 { limit } else { row.right() - 4 };
                if x + w > end(line) {
                    line += 1;
                    x = row.x + 4;
                    if line >= lines || x + w > end(line) { break; }
                }
                // the last line is the one above the bottom bar: the row grows upwards
                out.push((key, Rect::new(x, row.bottom() - 3 - ch - (lines - 1 - line) * (ch + 4), w, ch)));
                x += w + 5;
            }
            out
        };
        let placed = place();
        let left = keys.len() - placed.len();
        (placed, left)
    }

    /// The pinned measurements that have no place on the row, with the chip that says so (`studio_list_chip`, which
    /// then reads "+N"): it opens the list of all measurements.
    pub(super) fn studio_badges_more(&self) -> Option<(Rect, usize)> {
        if !self.studio_badges_visible() { return None; }
        let n = if self.studio_classic() { self.classic_layout().overflow } else { self.studio_badge_fit().1 };
        (n > 0).then(|| (self.studio_list_chip(), n))
    }

    /// What the cursors read, as one chip after the badges: the differences only.
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub(super) fn studio_cursor_chip(&self, after: i32) -> Option<(Rect, String)> {
        let row = self.studio_row();
        self.studio_cursor_chip_at(after, row.bottom() - 3 - self.studio_chip_h(), self.studio_chip_h())
    }

    /// The same at a place: its top and its height.
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub(super) fn studio_cursor_chip_at(&self, after: i32, y: i32, h: i32) -> Option<(Rect, String)> {
        if self.m.cursor == CursorMode::Off { return None; }
        let badge = self.studio.cur_badge;
        let text = self.cursor_values().into_iter().filter(|(l, _)| badge || l.contains('\u{394}'))
            .map(|(l, v)| format!("{l} {v}")).collect::<Vec<_>>().join("   ");
        if text.is_empty() { return None; }
        let w = 30 + self.fonts().sb.width(&text) + 10;
        (after + w <= self.studio_row_limit()).then(|| (Rect::new(after, y, w, h), text))
    }

    /// The name a badge shows: the short one (Pk-pk, Ampl) for a measurement of one source, as the Default layout has.
    pub(super) fn studio_badge_name(&self, key: MeasKey, long: &str) -> String {
        match key { MeasKey::One(k, _) => super::meascatalog::meas_short_name(k).to_string(), _ => long.to_string() }
    }

    pub(super) fn draw_studio_badges(&self, cv: &mut Canvas) {
        self.draw_studio_meas_chip(cv);
        if !self.studio_badges_visible() { return; }
        let f = self.fonts();
        let row = self.studio_row();
        cv.fill(row, BG);
        let rows = self.meas_rows();
        if self.studio_classic() { self.draw_classic_badges(cv); }
        for (key, r) in self.studio_badges().into_iter().filter(|_| !self.studio_classic()) {
            let Some(m) = self.studio_index(key).and_then(|i| rows.get(i)) else { continue };
            let col = self.wave_color(m.src);
            let marked = self.studio.mark == Some(key);
            chip(cv, r, if marked { mix(BTN, ACCENT, 40) } else { BTN }, if self.studio.popup == Some(key) || marked { ACCENT } else { BTN_BORDER });
            let cy = r.y + r.h / 2;
            // the source is a bar of its colour at the edge of the badge; a badge in the bar is pinned by being there
            cv.round(Rect::new(r.x + 3, r.y + 4, 4, r.h - 8), 2, col, 255);
            let x = cv.text(&f.s, r.x + 14, cy + 4, &self.studio_badge_name(key, &m.name), DIM) + 8;
            cv.set_clip(Rect::new(x, r.y, (r.right() - x - 4).max(1), r.h));
            cv.text(&f.sb, x, cy + 4, &m.text(), TEXT);
            cv.no_clip();
        }
        self.draw_studio_tab(cv);
        // the list of all measurements: their number, or "+N" for those that have no place here, and an arrow
        let c = self.studio_list_chip();
        let more = self.studio_badges_more().map(|(_, n)| n);
        let (label, col) = match more { Some(n) => (format!("+{n}"), ACCENT), None => (self.measurement_count().to_string(), TEXT) };
        chip(cv, c, BTN, if more.is_some() { ACCENT } else { BTN_BORDER });
        cv.set_clip(c.inset(1));
        cv.text_in(&f.sb, Rect::new(c.x + 4, c.y, c.w - 20, c.h), 1, &label, col);
        cv.no_clip();
        chevron(cv, c.right() - 11, c.y + c.h / 2, 1, col);
    }

    /// The popup of a badge: where it is.
    pub(super) fn studio_popup_rect(&self) -> Option<Rect> {
        let key = self.studio.popup?;
        let (_, anchor) = self.studio_badges().into_iter().find(|(k, _)| *k == key)?;
        let h = 40 + 3 * POPUP_ROW + 10 + POPUP_ROW + 6;
        let x = (anchor.x + anchor.w / 2 - POPUP_W / 2).clamp(4, self.scr.w - POPUP_W - 4);
        Some(Rect::new(x, anchor.y - h - 8, POPUP_W, h))
    }

    /// Row `i` of the popup: 0 Configure, 1 Unpin, 2 Float, 3 Remove.
    pub(super) fn studio_popup_row(p: Rect, i: i32) -> Rect {
        let y = p.y + 40 + i * POPUP_ROW + if i == 3 { 10 } else { 0 };
        Rect::new(p.x + 4, y, p.w - 8, POPUP_ROW)
    }

    /// What floats over the lanes and the bars: the badge popup.
    pub(super) fn draw_studio_floating(&self, cv: &mut Canvas) {
        self.draw_studio_yzoom(cv);
        self.draw_studio_sz_pops(cv);
        let (Some(key), Some(p)) = (self.studio.popup, self.studio_popup_rect()) else { return };
        let f = self.fonts();
        let rows = self.meas_rows();
        let Some(m) = self.studio_index(key).and_then(|i| rows.get(i)) else { return };
        cv.round(p, 8, BTN_BORDER, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        if let Some((_, a)) = self.studio_badges().into_iter().find(|(k, _)| *k == key) {
            let x = a.x + a.w / 2;
            cv.triangle([(x - 7, p.bottom() - 1), (x + 7, p.bottom() - 1), (x, p.bottom() + 6)], BTN_BORDER);
            cv.triangle([(x - 6, p.bottom() - 2), (x + 6, p.bottom() - 2), (x, p.bottom() + 4)], PANEL_BG);
        }
        let tag = Rect::new(p.x + 10, p.y + 9, f.sb.width("CH4") + 10, 22);
        cv.round(tag, 4, self.wave_color(m.src), 255);
        cv.text_in(&f.sb, tag, 1, &self.wave_name(m.src), DARK_TEXT);
        let x = cv.text(&f.mb, tag.right() + 8, p.y + 25, &m.name, TEXT) + 10;
        cv.text(&f.mb, x, p.y + 25, &m.text(), TEXT);
        let (cx, cy) = (p.right() - 24, p.y + 20);
        cv.line(cx - 5, cy - 5, cx + 5, cy + 5, TEXT);
        cv.line(cx - 5, cy + 5, cx + 5, cy - 5, TEXT);
        cv.hline(p.x + 8, p.right() - 9, p.y + 38, LINE);
        for (i, (label, col)) in [("Configure...", TEXT), ("Unpin from waveform", TEXT), ("Move to floating panel", TEXT), ("Remove measurement", RED)].into_iter().enumerate() {
            let r = Self::studio_popup_row(p, i as i32);
            if i == 3 { cv.hline(p.x + 8, p.right() - 9, r.y - 5, LINE); }
            let (ix, iy) = (r.x + 18, r.y + r.h / 2);
            match i {
                0 => icon_gear(cv, ix, iy, col, PANEL_BG),
                1 => { icon_pin(cv, ix - 6, iy, false, col); cv.line(ix - 8, iy + 8, ix + 8, iy - 8, col); }
                2 => { cv.round_outline(Rect::new(ix - 8, iy - 6, 14, 12), 2, col); cv.line(ix, iy, ix + 8, iy - 8, col); }
                _ => { cv.round_outline(Rect::new(ix - 7, iy - 5, 13, 13), 2, col); cv.hline(ix - 9, ix + 8, iy - 7, col); }
            }
            cv.text_in(&f.mb, Rect::new(r.x + 42, r.y, r.w - 44, r.h), 0, label, col);
        }
    }

    /// A tap while a badge popup is open: it either does one of its actions or closes it. Always taken.
    fn studio_popup_tap(&mut self, x: i32, y: i32) -> bool {
        let Some(key) = self.studio.popup else { return false };
        let Some(p) = self.studio_popup_rect() else { self.studio.popup = None; return false };
        self.studio.popup = None;
        if !p.contains(x, y) { return true; }
        let hit = (0..4).find(|&i| Self::studio_popup_row(p, i).contains(x, y));
        let Some(index) = self.studio_index(key) else { return true };
        match hit {
            Some(0) => self.open_meas_cfg(index),
            Some(1) => self.studio_set_pin(key, false),
            Some(2) => {
                // only this measurement moves: off the badges, into the floating panel; the view stays unless no panel shows
                self.studio_set_pin(key, false);
                if !self.studio.floated.contains(&key) { self.studio.floated.push(key); }
                if self.studio.view == StudioView::Waveform { self.studio.view = StudioView::Floating; }
                self.relayout();
            }
            Some(3) => {
                self.studio_set_pin(key, false);
                self.remove_measurement(index);
            }
            _ => {}
        }
        true
    }

    /// A tap in the badge row: the eye hides it, a badge opens its popup.
    fn studio_badge_tap(&mut self, x: i32, y: i32) -> bool {
        if self.studio_meas_chip_tap(x, y) {
            return true;
        }
        if !self.studio_badges_visible() || !self.studio_row().contains(x, y) { return false; }
        if self.studio_badges_more().is_some_and(|(r, _)| Self::hit40(r).contains(x, y)) {
            self.studio_set_view(StudioView::Docked);
            return true;
        }
        if Self::hit40(self.studio_list_chip()).contains(x, y) {
            self.studio_set_view(StudioView::Docked);
            return true;
        }
        if self.studio_tab().is_some_and(|tab| tab.contains(x, y)) {
            // more pinned measurements than the row holds: the tab pages through them; otherwise it opens the list
            let total = self.studio_pinned().len();
            let shown = self.studio_badges().len();
            let first = self.studio.badge_first.min(total.saturating_sub(1));
            if first + shown < total { self.studio.badge_first = first + shown.max(1); }
            else if first > 0 { self.studio.badge_first = 0; }
            else { self.studio_set_view(StudioView::Docked); }
            return true;
        }
        if let Some((key, _)) = self.studio_badges().into_iter().find(|(_, r)| r.contains(x, y)) {
            // the first tap shows what the measurement measures on the waveform; a tap on the badge that is shown
            // opens its menu (a measurement with nothing to draw goes straight to the menu)
            let drawable = matches!(key, MeasKey::One(k, id) if self.meas_mark_exists(k, id));
            if self.studio.mark == Some(key) || !drawable {
                self.studio.popup = Some(key);
                self.studio.mark_menu = self.studio.mark == Some(key);
            } else {
                self.studio.mark = Some(key);
                self.studio.mark_menu = false;
            }
        }
        true
    }
}

impl Ui {
    /// Chooses how the measurements share the screen.
    pub(super) fn studio_set_view(&mut self, view: StudioView) {
        let was = self.studio.view;
        // a record view runs on a session of its own: leaving it returns to the live acquisition
        if was.is_record() && was != view && self.record_mode() { self.return_to_live(); }
        self.studio.view = view;
        if view != StudioView::Offline { self.studio.dock.float_on[super::dock::VKind::Measure.idx()] = false; }
        self.dock_view_changed();
        self.studio.panel.scroll_top();
        self.studio.filter = None;
        self.studio.popup = None;
        if view != was {
            match view {
                StudioView::Offline => self.studio_enter_offline(),
                StudioView::Sim => self.studio_enter_sim(),
                _ => {}
            }
        }
        self.relayout();
    }
}

/// Below this width the footer of the panel stacks its two buttons narrower, with shorter labels.
#[cfg_attr(not(test), allow(dead_code))] // only the tests use it
pub(super) const NARROW_PANEL_W: i32 = 250;
/// Width of the measurement panel, docked or floating.
const MEAS_PANEL_W_FULL: i32 = 270;
/// A touch that a panel owns, from its first point to its last.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum StudioDrag {
    /// the list moves with the finger
    Scroll { y0: i32, first0: usize, moved: bool },
    /// a touch on a panel that scrolls nothing: it ends in a tap
    Eat { x0: i32, y0: i32 },
    /// the Menu page moves with the finger
    MenuScroll { y0: i32, s0: i32, moved: bool },
    /// a row of the table swiped sideways
    Swipe { key: MeasKey, x0: i32, y0: i32 },
    /// a badge held and dragged
    BadgeDrag { key: MeasKey, x: i32, y: i32 },
    /// the detailed table dragged sideways
    HScroll { x0: i32, hx0: i32 },
    /// a tile of the Add measurement panel dragged to a channel
    Tile { k: usize, x0: i32, y0: i32, x: i32, y: i32, moved: bool },
}

/// The parts of the docked panel.
pub(super) struct PanelGeo {
    pub p: Rect,
    pub collapse: Rect,
    pub list: Rect,
    pub page: usize,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub up: Rect,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub down: Rect,
    pub add: Rect,
    /// the statistics button
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub stats: Rect,
    /// a short panel: a header and a footer of one row each
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub short: bool,
}

/// The opposite of `icon_expand`: the two arrow heads meet in the middle.
pub(super) fn icon_collapse(cv: &mut Canvas, cx: i32, cy: i32, c: Rgb) {
    cv.line(cx - 6, cy + 6, cx + 6, cy - 6, c);
    cv.hline(cx + 1, cx + 6, cy - 1, c);
    cv.vline(cx + 1, cy - 6, cy - 1, c);
    cv.hline(cx - 6, cx - 1, cy + 1, c);
    cv.vline(cx - 1, cy + 1, cy + 6, c);
}

pub(super) fn icon_expand(cv: &mut Canvas, cx: i32, cy: i32, c: Rgb) {
    cv.line(cx - 6, cy + 6, cx + 6, cy - 6, c);
    cv.hline(cx + 1, cx + 6, cy - 6, c);
    cv.vline(cx + 6, cy - 6, cy - 1, c);
    cv.hline(cx - 6, cx - 1, cy + 6, c);
    cv.vline(cx - 6, cy + 1, cy + 6, c);
}

pub(super) fn icon_arrow(cv: &mut Canvas, cx: i32, cy: i32, up: bool, c: Rgb) {
    let d = if up { -1 } else { 1 };
    cv.triangle([(cx - 6, cy - 3 * d), (cx + 6, cy - 3 * d), (cx, cy + 3 * d)], c);
}

impl Ui {
    /// Index of the first row of the list that is shown, kept in range.
    pub(super) fn studio_first(&self, total: usize, page: usize) -> usize {
        self.studio.panel.first.min(total.saturating_sub(page))
    }

    /// The measurements the panel lists: all, or those of the chosen source.
    pub(super) fn studio_listed(&self, rows: &[tools::MeasRow]) -> Vec<usize> {
        let keys = self.studio_keys();
        let pinned = self.studio_pinned();
        let q = self.studio.panel.query.trim().to_lowercase();
        let mut v: Vec<usize> = (0..rows.len()).filter(|&i| !self.meas_row_hidden(&rows[i]) && self.studio.filter.is_none_or(|id| rows[i].src == id)
            && (!self.studio.panel.pinned_only || keys.get(i).is_some_and(|k| pinned.contains(k)))
            && (q.is_empty() || rows[i].name.to_lowercase().contains(&q) || self.wave_name(rows[i].src).to_lowercase().contains(&q))).collect();
        // with several sources the rows are grouped by source, in the order the sources first appear
        let mut order: Vec<WaveId> = Vec::new();
        for &i in &v { if !order.contains(&rows[i].src) { order.push(rows[i].src); } }
        if order.len() > 1 { v.sort_by_key(|&i| order.iter().position(|s| *s == rows[i].src).unwrap_or(0)); }
        v
    }

    /// The measurement panel (docked or floating) and the SIM panel: the room under their title strip, in the dock.
    pub(super) fn studio_panel_rect(&self) -> Option<Rect> {
        match self.studio.view {
            StudioView::Docked | StudioView::Offline | StudioView::Floating => self.dock_content(super::dock::VKind::Measure),
            StudioView::Sim => self.dock_content(super::dock::VKind::Sim),
            _ => None,
        }
    }

    /// Plot width less the docked panel: `plot_span` calls this.
    pub(super) fn studio_dock_w(&self) -> i32 {
        if self.dock_on() { return 0; }
        if self.sheet_active() { return self.sheet_dock_w(); }
        if self.m.layout == Layout::Studio && self.studio.view.is_docked() { self.studio_panel_w() + 4 } else { 0 }
    }

    pub(super) fn studio_panel_geo(&self) -> Option<PanelGeo> {
        let p = self.studio_panel_rect().filter(|_| matches!(self.studio.view, StudioView::Docked | StudioView::Offline | StudioView::Floating))?;
        Some(self.studio_panel_geo_in(p))
    }

    /// The parts of the panel `p`: the toolbar of a narrow one (a wide one has its controls in the title strip), the column
    /// heads of the table, and the list with all the rest of the room. There is no footer and no pager.
    pub(super) fn studio_panel_geo_in(&self, p: Rect) -> PanelGeo {
        let detailed = self.studio_detailed();
        let list_y = p.y + self.panel_tool_h(p) + if detailed { super::measview::COL_HEAD } else { 0 };
        let unit = self.studio_panel_row();
        let rows_h = (p.bottom() - 2 - list_y).max(unit);
        let page = if self.panel_cells() { self.panel_cards_page(p.w - 8, rows_h) } else { (rows_h / unit).max(1) as usize };
        let shown_h = if self.panel_cells() { (page as i32 / self.panel_cell_geo(p.w - 8).0) * unit } else { rows_h };
        let list = Rect::new(p.x + 4, list_y, p.w - 8, shown_h);
        let bar = self.panel_bar(p);
        let at = |it: super::measpanel::BarItem| bar.slots.iter().find(|s| s.item == it).map_or(Rect::new(0, 0, 0, 0), |s| s.r);
        let none = Rect::new(0, 0, 0, 0);
        PanelGeo { p, collapse: none, list, page, up: none, down: none, add: at(super::measpanel::BarItem::Add), stats: at(super::measpanel::BarItem::Stats), short: false }
    }

    pub(super) fn draw_scrollbar(cv: &mut Canvas, list: Rect, first: usize, page: usize, total: usize) {
        if total <= page { return; }
        let x = list.right() - 4;
        cv.vline(x, list.y, list.bottom() - 1, mix(BG, LINE, 120));
        let h = (list.h as i64 * page as i64 / total as i64).max(16) as i32;
        let y = list.y + ((list.h - h) as i64 * first as i64 / (total - page).max(1) as i64) as i32;
        cv.fill(Rect::new(x - 1, y, 3, h), DIM);
    }

    /// The button of the empty panel: under the words, in the middle of the list.
    pub(super) fn panel_empty_button(list: Rect) -> Rect {
        let (w, h) = (176.min(list.w - 16).max(0), 36);
        Rect::new(list.x + (list.w - w) / 2, list.y + list.h / 2 + 4, w, h)
    }

    /// Nothing is measured in this view: say so, with the way to add one.
    pub(super) fn draw_panel_empty(&self, cv: &mut Canvas, list: Rect) {
        let f = self.fonts();
        let b = Self::panel_empty_button(list);
        cv.text_in(&f.mb, Rect::new(list.x, b.y - 30, list.w, 24), 1, "No measurements", DIM);
        if b.w > 0 {
            chip(cv, b, BTN, BTN_BORDER);
            cv.text_in(&f.mb, b, 1, "+ Add measurement", TEXT);
        }
    }

    pub(super) fn draw_studio_panel(&self, cv: &mut Canvas) {
        if self.m.layout != Layout::Studio { return; }
        // an analysis view fills the plot area: the docked panel stays beside it, a floating panel does not
        if self.view_on() && !self.studio.view.is_docked() { return; }
        self.draw_studio_offline(cv);
        if self.studio.view == StudioView::Sim { return self.draw_studio_sim(cv); }
        let Some(p) = self.studio_panel_rect() else { return };
        let rows = self.meas_rows();
        let keys = self.studio_keys();
        let listed = self.studio_listed(&rows);
        cv.round(p, 8, if self.studio.view == StudioView::Floating { BTN_BORDER } else { CARD_BORDER }, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        if let Some(g) = self.studio_panel_geo() {
            let detailed = self.studio_detailed();
            self.draw_panel_toolbar(cv, &g);
            let first = self.studio_first(listed.len(), g.page);
            if detailed { self.draw_panel_detail(cv, &g, &rows, &keys, &listed, first); }
            cv.set_clip(g.list);
            if self.panel_is_strip() { self.draw_panel_strip(cv, &g, &rows, &keys, &listed, first); cv.set_clip(g.list); }
            else if !detailed { self.draw_panel_cards(cv, &g, &rows, &keys, &listed, first); }
            if rows.is_empty() { self.draw_panel_empty(cv, g.list); }
            cv.no_clip();
            // the thin mark that shows where the list is (there is no pager)
            Self::draw_scroll_mark(cv, g.list, self.panel_scroll_px(), self.panel_scroll_max(listed.len(), &g) + g.list.h);
            self.draw_panel_drop(cv);
        }
    }

    /// The tab that brings the panel back while only the lanes are shown.
    pub(super) fn studio_tab(&self) -> Option<Rect> {
        // the Meas tab duplicated the list button: that button shows the count now (draw_studio_badges)
        None
    }

    fn draw_studio_tab(&self, cv: &mut Canvas) {
        let Some(r) = self.studio_tab() else { return };
        let f = self.fonts();
        chip(cv, r, BTN, BTN_BORDER);
        cv.text_in(&f.sb, Rect::new(r.x + 8, r.y, r.w - 22, r.h), 0, &format!("Meas \u{b7} {}", self.measurement_count()), TEXT);
        chevron(cv, r.right() - 12, r.y + r.h / 2, 1, TEXT);
    }

    // ------------------------------------------------------------------------------------------------ taps

    /// Opens the settings of measurement `index`: its own page, or the measure menu for the kinds without one.
    pub(super) fn studio_configure(&mut self, index: usize) {
        self.open_meas_cfg(index)
    }

    /// Pages the list by one page.
    pub(super) fn studio_page(&mut self, total: usize, page: usize, down: bool) {
        let first = self.studio_first(total, page);
        self.studio.panel.first = if down { (first + page).min(total.saturating_sub(page)) } else { first.saturating_sub(page) };
    }

    /// A tap on the panel. False: it is not on it.
    pub(super) fn studio_panel_tap(&mut self, x: i32, y: i32) -> bool {
        if self.studio_offline_tap(x, y) { return true; }
        if self.studio.view == StudioView::Sim { return self.studio_sim_tap(x, y); }
        let rows = self.meas_rows();
        let keys = self.studio_keys();
        let listed = self.studio_listed(&rows);
        if let Some(g) = self.studio_panel_geo() {
            if !g.p.contains(x, y) { return false; }
            if self.panel_drop_tap(x, y) { return true; }
            if self.md_undo_tap(x, y) { return true; }
            if self.panel_head_tap(&g, x, y) { return true; }
            if g.collapse.contains(x, y) { self.studio_set_view(StudioView::Waveform); }
            else if g.add.contains(x, y) { self.open_meas_list(); }
            else if g.list.contains(x, y) {
                let first = self.studio_first(listed.len(), g.page);
                let hit = if self.studio_detailed() {
                    self.detail_layout(&g, listed.len(), first).into_iter().find(|(_, r)| r.contains(x, y)).map(|(pos, r)| (listed[pos], r))
                } else {
                    self.panel_cards_layout(first, listed.len(), g.list).into_iter().find(|(_, r)| r.contains(x, y)).map(|(pos, r)| (listed[pos], r))
                };
                if rows.is_empty() && Self::panel_empty_button(g.list).contains(x, y) { self.open_meas_list(); return true; }
                if let Some((i, cell)) = hit {
                    // a card and a cell have their pin; the rest, and a row of the table, open the settings (with Remove)
                    let pin = if self.panel_is_strip() { Self::panel_cell_pin(cell) } else { Self::panel_card_pin(cell) };
                    if self.panel_cells() && pin.contains(x, y) {
                        if let Some(&key) = keys.get(i) { let on = self.studio_is_pinned(key); self.studio_set_pin(key, !on); }
                    } else { self.studio_configure(i); }
                }
            }
            return true;
        }
        if let Some(tab) = self.studio_tab() {
            if Rect::new(tab.x - 4, tab.y - 2, tab.w + 8, tab.h + 6).contains(x, y) {
                self.studio_set_view(StudioView::Docked);
                return true;
            }
        }
        false
    }

    /// Touches that move the list or the floating panel. Everything else belongs to the plot (`touch_ev`).
    pub(super) fn studio_touch(&mut self, ph: Phase, x: i32, y: i32) -> bool {
        if self.m.layout != Layout::Studio { return false; }
        match ph {
            Phase::Down => {
                self.studio.sz.press = Some((self.now, x, y));
                self.studio.sz.fired = false;
                self.studio.drag = None;
                // the numeric pad is modal
                if self.numpad.is_some() { return self.studio_eat(x, y); }
                if self.trig_slider_down(x, y) { return true; }
                // the info card is modal
                if self.studio.md.info.is_some() { return self.studio_eat(x, y); }
                // full screen: its few buttons
                if self.m.layout == Layout::Studio && self.full_on() && self.full_chrome_rects().is_some_and(|(a, b)| Self::hit40(a).contains(x, y) || Self::hit40(b).contains(x, y)) { return self.studio_eat(x, y); }
                if self.dialog.is_some() || self.power.open || self.wlan.osk.is_some() || self.m.touch_lock
                    || self.studio.add_menu || self.studio.ctx.is_some() || self.studio.popup.is_some() || self.workspace.popup.is_some() || self.load_slot_ask { return false; }
                // the Add measurement panel is open: only its tiles can be dragged
                if self.menu_open { return self.md_down(x, y); }
                // the bars with an Undo are above the strips of the views
                if [self.md_hide_bar(), self.md_undo_bar()].into_iter().flatten().any(|(bar, _)| bar.contains(x, y)) { return self.studio_eat(x, y); }
                if self.ov_divider_down(x, y) { return true; }
                if self.dock_down(x, y) { return true; }
                if self.md_down(x, y) { return true; }
                if self.axis_down(x, y) { return true; }
                let rows = self.meas_rows();
                if self.studio.view == StudioView::Sim {
                    if let Some((_, _, page, _)) = self.studio_sim_list().filter(|(list, ..)| list.contains(x, y)) {
                        self.studio.drag = Some(StudioDrag::Scroll { y0: y, first0: self.studio_first(self.studio_sim_list().map_or(0, |l| l.3), page), moved: false });
                        return true;
                    }
                    return self.studio_panel_rect().is_some_and(|p| p.contains(x, y)) && self.studio_eat(x, y);
                }
                if self.panel_drop_rect().is_some_and(|r| r.contains(x, y)) { return self.studio_eat(x, y); }
                if let Some(g) = self.studio_panel_geo() {
                    if g.list.contains(x, y) {
                        self.panel_scroll_begin();
                        self.studio.drag = Some(StudioDrag::Scroll { y0: y, first0: self.studio_first(self.studio_listed(&rows).len(), g.page), moved: false });
                        return true;
                    }
                    return self.dock_float_at(x, y).is_some() && self.studio_eat(x, y);
                }
                if self.dock_float_at(x, y).is_some() { return self.studio_eat(x, y); }
                false
            }
            Phase::Move => {
                if let Some((_, x0, y0)) = self.studio.sz.press { if (x - x0).abs() > 12 || (y - y0).abs() > 12 { self.studio.sz.press = None; } }
                if self.trig_slider_move(y) { return true; }
                if self.ov_divider_move(y) { return true; }
                if self.axis_move(x, y) { return true; }
                if self.dock_move(x, y) { return true; }
                if self.md_move(x, y) { return true; }
                let Some(drag) = self.studio.drag else { return false };
                match drag {
                    StudioDrag::Scroll { y0, first0, moved } => {
                        // the list of the SIM panel moves by whole rows, the measurements panel by pixels
                        let sim = self.studio_sim_list().filter(|_| self.studio.view == StudioView::Sim);
                        let moved = moved || (y - y0).abs() > 8;
                        if let (true, Some((_, h, page, total))) = (moved, sim) {
                            self.studio.panel.first = (first0 as i32 + (y0 - y) / h).clamp(0, total.saturating_sub(page) as i32) as usize;
                        } else if moved {
                            self.panel_scroll_drag(y0, first0, y);
                        }
                        self.studio.drag = Some(StudioDrag::Scroll { y0, first0, moved });
                    }
                    StudioDrag::MenuScroll { y0, s0, moved } => {
                        let moved = moved || (y - y0).abs() > 8;
                        if moved { self.sheet.scroll = (s0 + y0 - y).clamp(0, self.menu_geo().max_scroll()); }
                        self.studio.drag = Some(StudioDrag::MenuScroll { y0, s0, moved });
                    }
                    StudioDrag::Eat { .. } => {}
                    StudioDrag::Swipe { .. } | StudioDrag::BadgeDrag { .. } | StudioDrag::Tile { .. } | StudioDrag::HScroll { .. } => {}
                }
                true
            }
            Phase::Up => {
                self.studio.sz.press = None;
                // the lift of a hold (`studio_hold_tick`) is not a tap
                if std::mem::take(&mut self.studio.sz.fired) {
                    if self.touch.down { return false; }
                    self.studio.drag = None;
                    return true;
                }
                if self.trig_slider_up(x, y) { return true; }
                self.axis_up();
                if self.ov_divider_up() { return true; }
                if self.dock_up(x, y) { return true; }
                let Some(drag) = self.studio.drag.take() else { return false };
                if matches!(drag, StudioDrag::Scroll { moved: true, .. }) { self.panel_scroll_end(); }
                if self.md_up(drag, x, y) { return true; }
                let moved = match drag {
                    StudioDrag::Swipe { .. } | StudioDrag::BadgeDrag { .. } | StudioDrag::HScroll { .. } => true,
                    StudioDrag::Tile { moved, .. } => moved,
                    StudioDrag::Scroll { moved, .. } | StudioDrag::MenuScroll { moved, .. } => moved,
                    StudioDrag::Eat { x0, y0 } => (x - x0).abs().max((y - y0).abs()) > 8,
                };
                if !moved { self.tap(x, y); }
                true
            }
        }
    }

    /// A touch on a panel that does nothing by itself: the plot behind must not take it.
    /// The gesture ends in a tap (`Phase::Up` of the owner below).
    pub(super) fn studio_eat(&mut self, x: i32, y: i32) -> bool {
        self.studio.drag = Some(StudioDrag::Eat { x0: x, y0: y });
        true
    }
}

// ---------------------------------------------------------------------------------------------------- zoom

/// What the zoom row and the zoom commands do.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum ZoomRow {
    Minus,
    Plus,
    /// vertical zoom in and out
    VPlus,
    VMinus,
    Fit,
    Center,
    ResetY,
    Close,
}

/// The buttons of the zoom row: the row of the Default layout (`Ui::zoom_controls`), by name.
#[derive(Clone, Copy)]
pub(super) struct ZoomBar {
    /// the text at the left: "Zoom", the time per division, the position and the vertical factor
    pub label: Rect,
    /// H+ and H-: one step of the 1-2-5 sequence in and out
    pub plus: Rect,
    pub minus: Rect,
    /// V+ and V-: one step of the display scale of the zoomed lanes
    pub vplus: Rect,
    pub vminus: Rect,
    /// Box: arms the drawing of a zoom box on the plot
    pub boxr: Rect,
    /// Reset: the zoom window back to the whole screen
    pub fit: Rect,
    /// Close: zoom off
    pub close: Rect,
}

fn zoom_text(n: f32) -> String {
    let n = if n.is_finite() { n.max(1.0) } else { 1.0 };
    if n >= 9.95 { format!("{:.0}\u{d7}", n) } else { format!("{:.1}\u{d7}", n).replace(".0\u{d7}", "\u{d7}") }
}

impl Ui {
    /// Studio shows the zoom row while its plot is zoomed.
    pub(super) fn studio_zoom_active(&self) -> bool {
        self.m.layout == Layout::Studio && self.m.zoom && !self.xy_over() && !self.view_on()
    }

    /// How many times the detail view magnifies time against the main view.
    pub(super) fn studio_time_zoom(&self) -> f32 {
        self.m.time_per_div() / tscales()[self.m.zscale].max(1e-12)
    }

    /// The zoom row of Studio is the row of the Default layout, between the overview and the zoomed lanes: the text
    /// at the left, then H-, H+, V-, V+, Box, Reset and Close (`zoom_controls`, `draw_zoom_readout`). These are its
    /// buttons by name.
    pub(super) fn studio_zoom_bar(&self) -> Option<ZoomBar> {
        if !self.studio_zoom_active() { return None; }
        let row = self.zoom_readout();
        let c = self.zoom_controls();
        let of = |name: &str| c.iter().find(|(_, text)| *text == name).map_or(Rect::new(0, 0, 0, 0), |(r, _)| *r);
        let close = self.zoom_close();
        let label = Rect::new(row.x + 4, row.y, (of("H-").x - row.x - 10).max(1), row.h);
        Some(ZoomBar { label, plus: of("H+"), minus: of("H-"), vplus: of("V+"), vminus: of("V-"), boxr: of("Box"), fit: Rect::new(close.x - 76, close.y, 70, close.h), close })
    }

    /// The display scale popup of a lane (a tap on its channel badge while zoomed).
    pub(super) fn studio_yzoom_rect(&self) -> Option<Rect> {
        let c = self.studio.yzoom?;
        let (_, badge) = self.studio.lane_badges.borrow().iter().find(|(k, _)| *k == c).copied()?;
        let h = 40 + 46 + 44;
        let y = badge.y.min(self.frame().bottom.y - h - 4).max(self.frame().top.bottom() + 4);
        Some(Rect::new(badge.right() + 8, y, 250, h))
    }

    /// Minus, plus and Reset Y of that popup.
    pub(super) fn studio_yzoom_parts(&self) -> Option<(Rect, Rect, Rect, Rect)> {
        let p = self.studio_yzoom_rect()?;
        Some((Rect::new(p.right() - 38, p.y + 4, 32, 32), Rect::new(p.x + 10, p.y + 44, 48, 36),
            Rect::new(p.right() - 58, p.y + 44, 48, 36), Rect::new(p.x + 10, p.y + 40 + 46, p.w - 20, 36)))
    }

    pub(super) fn draw_studio_yzoom(&self, cv: &mut Canvas) {
        let (Some(p), Some((close, minus, plus, reset)), Some(c)) = (self.studio_yzoom_rect(), self.studio_yzoom_parts(), self.studio.yzoom) else { return };
        let f = self.fonts();
        cv.round(p, 8, BTN_BORDER, 255);
        cv.round(p.inset(1), 7, PANEL_BG, 255);
        cv.text(&f.mb, p.x + 12, p.y + 26, &format!("CH{} display scale", c + 1), TEXT);
        let (cx, cy) = (close.x + close.w / 2, close.y + close.h / 2);
        cv.line(cx - 5, cy - 5, cx + 5, cy + 5, TEXT);
        cv.line(cx - 5, cy + 5, cx + 5, cy - 5, TEXT);
        for (r, plus_sign) in [(minus, false), (plus, true)] {
            chip(cv, r, BTN, BTN_BORDER);
            cv.hline(r.x + r.w / 2 - 6, r.x + r.w / 2 + 6, r.y + r.h / 2, TEXT);
            if plus_sign { cv.vline(r.x + r.w / 2, r.y + r.h / 2 - 6, r.y + r.h / 2 + 6, TEXT); }
        }
        cv.text_in(&f.mb, Rect::new(minus.right(), minus.y, plus.x - minus.right(), minus.h), 1, &format!("Y zoom {}", zoom_text(self.zoom_view.vfactor)), TEXT);
        chip(cv, reset, BTN, BTN_BORDER);
        cv.text_in(&f.m, reset, 1, "Reset Y", TEXT);
    }

    /// What the zoomed lanes say about their own state: the vertical zoom, and a hint while a gesture is on.
    pub(super) fn draw_studio_zoom_overlay(&self, cv: &mut Canvas) {
        if !self.studio_zoom_active() { return; }
        let f = self.fonts();
        let lane_tag = |cv: &mut Canvas, lane: Rect, text: &str| {
            let w = f.sb.width(text) + 16;
            let r = Rect::new(self.right_tag_x(lane, lane.y + 6, w) - w + 26, lane.y + 4, w, 24);
            cv.round(r, 5, BTN_BORDER, 255);
            cv.round(r.inset(1), 4, PANEL_BG, 255);
            cv.text_in(&f.sb, r, 1, text, TEXT);
        };
        if (self.zoom_view.vfactor - 1.0).abs() > 1e-3 {
            for &lane in self.plot.wins.iter().filter(|l| self.studio_vzoom_covers(**l)) { lane_tag(cv, lane, &format!("Y zoom {}", zoom_text(self.zoom_view.vfactor))); }
        }
        if let (Some(hint), Some(&lane)) = (self.studio_zoom_samples_hint(), self.plot.wins.last()) {
            let w = f.sb.width(&hint) + 16;
            let r = Rect::new(lane.x + 52, lane.bottom() - 52, w, 20);
            cv.round(r, 5, BTN_BORDER, 255);
            cv.round(r.inset(1), 4, PANEL_BG, 255);
            cv.text_in(&f.sb, r, 1, &hint, AMBER);
        }
        // the hint of the gesture under the finger
        let lane = self.wave_area();
        let (cx, cy) = (lane.x + lane.w / 2, lane.y + lane.h / 3);
        let hint = if self.input.pinch.active { Some(format!("Time zoom \u{b7} {}", zoom_text(self.studio_time_zoom()))) }
            else if self.touch.down && self.touch.drag == Drag::Delay { Some("Pan time".to_string()) } else { None };
        if let Some(text) = hint {
            let r = Rect::new(cx - f.m.width(&text) / 2 - 12, cy - 14, f.m.width(&text) + 24, 28);
            cv.round(r, 6, BTN_BORDER, 255);
            cv.round(r.inset(1), 5, PANEL_BG, 255);
            cv.text_in(&f.m, r, 1, &text, TEXT);
        }
    }

    pub(super) fn studio_zoom_do(&mut self, row: ZoomRow) {
        let area = self.wave_area();
        match row {
            ZoomRow::Minus => self.zoom_horizontal_step(1, area, 0.5),
            ZoomRow::Plus => self.zoom_horizontal_step(-1, area, 0.5),
            ZoomRow::VPlus => self.zoom_vertical_step(1, area, 0.5),
            ZoomRow::VMinus => self.zoom_vertical_step(-1, area, 0.5),
            ZoomRow::Fit => self.reset_zoom(),
            ZoomRow::Center => self.studio_center_trigger(),
            ZoomRow::ResetY => { self.zoom_view.vfactor = 1.0; self.zoom_view.vcenter = 0.0; self.dirty_map = true; }
            ZoomRow::Close => self.set_zoom(false),
        }
    }

    /// Puts the trigger in the middle of the zoomed window.
    pub(super) fn studio_center_trigger(&mut self) {
        self.m.zdelay = 0.0;
        self.clamp_zoom();
        if self.m.zdelay != 0.0 {
            // the trigger is outside the main window: the window comes to it, so the box can too
            self.m.delay = 0.0;
            self.m.zdelay = 0.0;
            self.clamp_zoom();
        }
        self.dirty_map = true;
        self.settings_changed();
    }

    /// Seconds between the samples of the record the zoom shows (the held or captured record, or the ref of a SIM or file).
    pub(super) fn studio_zoom_sample_dt(&self) -> Option<f64> {
        if self.record_mode() {
            return self.refs.get(self.active_reference()).and_then(Option::as_ref).map(|r| r.dt).filter(|dt| *dt > 0.0);
        }
        let rec = self.last[0].as_ref()?;
        let dt = rec.raw.as_ref().map_or(rec.dt, |raw| raw.frame.dt);
        (dt > 0.0).then_some(dt)
    }

    /// The hint when the zoom asks for more than the record has: how many samples there are in a division.
    pub(super) fn studio_zoom_samples_hint(&self) -> Option<String> {
        if !self.studio_zoom_active() { return None; }
        let per_div = tscales()[self.m.zscale] as f64 / self.studio_zoom_sample_dt()?;
        // below about one sample in three pixels of a division the picture is the samples themselves
        (per_div < (self.plot.wins.first().map_or(self.plot.grid.w, |w| w.w) as f64 / HDIV as f64) / 3.0)
            .then(|| if per_div < 10.0 { format!("{per_div:.1} samples/div") } else { format!("{per_div:.0} samples/div") })
    }

    /// Taps on the zoom bar, its menu, the display scale popup and the lane badges. False: none of them.
    pub(super) fn studio_zoom_tap(&mut self, x: i32, y: i32) -> bool {
        if self.studio.yzoom.is_some() {
            let parts = self.studio_yzoom_parts();
            let inside = self.studio_yzoom_rect().is_some_and(|p| p.contains(x, y));
            if let (Some((close, minus, plus, reset)), true) = (parts, inside) {
                let area = self.wave_area();
                if close.contains(x, y) { self.studio.yzoom = None; }
                else if minus.contains(x, y) { self.zoom_vertical_step(-1, area, 0.5); }
                else if plus.contains(x, y) { self.zoom_vertical_step(1, area, 0.5); }
                else if reset.contains(x, y) { self.studio_zoom_do(ZoomRow::ResetY); }
                return true;
            }
            self.studio.yzoom = None;
            return true;
        }
        if let Some(bar) = self.studio_zoom_bar() {
            let y = self.zoom_row_y(x, y);
            if self.zoom_readout().contains(x, y) {
                if bar.plus.contains(x, y) { self.studio_zoom_do(ZoomRow::Plus); }
                else if bar.minus.contains(x, y) { self.studio_zoom_do(ZoomRow::Minus); }
                else if bar.vplus.contains(x, y) { self.studio_zoom_do(ZoomRow::VPlus); }
                else if bar.vminus.contains(x, y) { self.studio_zoom_do(ZoomRow::VMinus); }
                else if bar.close.contains(x, y) { self.studio_zoom_do(ZoomRow::Close); }
                else if bar.fit.contains(x, y) { self.reset_zoom(); }
                else if bar.boxr.contains(x, y) { self.arm_zoom_area(); }
                return true;
            }
        }
        // the name of a decoder beside its band ("1 UART CH1"): the Decode menu of that decoder. It was a text and
        // nothing to tap; the way to the menu was Add > Bus or the Menu
        let decoder = self.studio.dec_labels.borrow().iter().find(|(_, r)| Self::hit40(*r).contains(x, y)).map(|&(k, _)| k);
        if let Some(k) = decoder {
            if self.m.dec_sel != k { self.select_option(Item::DecSel, &self.dec_tag(k)); }
            self.toggle_menu(MenuId::Decode);
            return true;
        }
        // a lane's channel badge: its display scale while zoomed, its menu otherwise
        let hit = self.studio.lane_badges.borrow().iter().find(|(_, r)| Self::hit40(*r).contains(x, y)).map(|&(c, _)| c);
        if let Some(c) = hit {
            self.select_workspace_source(WaveId::Ch(c));
            if self.studio_zoom_active() { self.studio.yzoom = Some(c); } else { self.toggle_menu(MenuId::Channel(c)); }
            return true;
        }
        // the handle of a reference or a math channel: its sheet
        let hit = self.studio.handles.borrow().iter().find(|(_, r)| Self::hit40(*r).contains(x, y)).map(|&(s, _)| s);
        match hit {
            Some(super::workspace::Trace::Reference(i)) => {
                self.select_workspace_source(WaveId::Ref(i));
                self.toggle_menu(MenuId::Ref);
                return true;
            }
            Some(super::workspace::Trace::Math(k)) => {
                self.m.math_select(k);
                self.toggle_menu(MenuId::Math);
                return true;
            }
            _ => {}
        }
        false
    }
}

#[cfg(test)]
impl Ui {
    pub(super) fn studio_set_pin_all_for_test(&mut self) {
        let keys = self.studio_keys();
        for k in keys.into_iter().take(3) { self.studio_set_pin(k, true); }
    }
}
