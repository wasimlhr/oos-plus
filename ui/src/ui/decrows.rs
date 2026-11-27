// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What the decoders found, as rows: the Events pane under the plot. The rows are the
//! messages of the record (`oos_dsp::decode::bursts`), its single fields, or the lines of a character stream; the
//! pane's head chooses, and can leave only the rows with a failed check. A message has its time, its decoder, the
//! number of its fields, its text, for a character line the bytes in hex, and whether its checks passed. The pane
//! fits its size: a narrow one keeps number, time, text and status, a small one time and text, and its head has one
//! button that goes through the views. A tap on a row moves the view to it and chooses it: the row, its box in the
//! band and its time span on the plot are marked, and a tap on a box of the band chooses its row.

use super::dock::{VKind, STRIP_H};
use super::*;
use crate::protocols::Col;
use oos_dsp::decode::{Kind as DecKind, Packet};

/// Height of the pane's head and of its column titles. The buttons of the head are a finger tall (`TOUCH_MIN`).
const HEAD: i32 = workspace::TOUCH_MIN + 4;
const TITLES: i32 = 20;
/// The touch area of the list's bar at the right edge of its rows, and how long the bar shows after a scroll.
const BAR_AREA: i32 = 24;
const BAR_SHOWN_S: f64 = 1.5;
/// Height of a pane with its head and one line of text.
const EMPTY_H: i32 = HEAD + 22;
/// Below this width the pane has no columns for the decoder, the length and the hex bytes.
const NARROW_W: i32 = 560;
/// Below this width it lists the time and the text alone.
const SMALL_W: i32 = 360;

impl MsgRow {
    /// What is wrong with the row in words: what the module says of it, else how many fields failed a check; empty
    /// for a row without a failed check.
    pub fn error_text(&self) -> String {
        if self.why.is_empty() {
            errors_text(self.errors)
        } else {
            self.why.to_string()
        }
    }
}

/// One row of decoder results.
#[derive(Clone, Debug, PartialEq)]
pub struct MsgRow {
    /// the number of the row in the list of the record, from 1: the same in the pane, the CSV file and the answer of
    /// `:TABLe:DATA? EVENTS`
    pub index: usize,
    /// its decoder (0 = D1)
    pub dec: usize,
    /// start of its first field and end of its last one (s from the trigger)
    pub t: f64,
    pub t1: f64,
    /// its decoder, as the band on the plot names it ("B1 UART"; Default: "D1 UART")
    pub who: String,
    /// fields with a value in it
    pub len: usize,
    pub text: String,
    /// the bytes in hex, where the text shows characters; else empty
    pub hex: String,
    /// fields that failed their check
    pub errors: usize,
    /// what failed, where the module says it of one field ([`crate::protocols::Protocol::field_error`]); else empty,
    /// and the pane writes the count
    pub why: &'static str,
    /// the columns of the protocol for this row and their cells ([`crate::protocols::Protocol::columns`]); empty for
    /// a protocol without them and for a row that is no frame (a field, a line of text)
    pub cols: &'static [(&'static str, Col)],
    pub cells: Vec<String>,
}

/// The row chosen by a tap: its decoder and its time span, in the record it was chosen in.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct EvSel {
    pub dec: usize,
    pub t0: f64,
    pub t1: f64,
    /// waveforms since clear and the place in the history: another record has other messages
    rec: (u64, Option<usize>),
}

/// The buttons in the head of the Events pane.
struct EvHead {
    /// the views: all three, or the one chosen (`cycle`: a tap goes on to the next)
    views: Vec<(Rect, EvView)>,
    cycle: bool,
    /// Errors only, where there is room for it
    errors: Option<Rect>,
    /// where the buttons end
    right: i32,
    /// the buttons are in the title strip of the docked panel, not in a row of their own
    strip: bool,
}

/// The lines of a character stream: each ends with its line feed, the rest of the record is the last one.
fn lines(p: &[Packet]) -> Vec<std::ops::Range<usize>> {
    let mut out = Vec::new();
    let mut from = 0;
    for (k, x) in p.iter().enumerate() {
        if x.kind == DecKind::Data && x.value == 0x0a {
            out.push(from..k + 1);
            from = k + 1;
        }
    }
    if from < p.len() {
        out.push(from..p.len());
    }
    out
}

/// The rows of one decoder's packets.
fn rows_of(m: &Model, dec: usize, who: &str, p: &[Packet], view: EvView) -> Vec<MsgRow> {
    let ascii = decband::ascii(m);
    let hex = |x: &Packet| format!("{:02X}", x.value);
    if view == EvView::Fields {
        // (a mark the module adds for the screen is no field of the bus)
        let mark = |x: &Packet| m.dec_proto.module().is_some_and(|module| module.mark(x));
        return p
            .iter()
            .filter(|x| !mark(x))
            .map(|x| {
                let tag = packet_tag(m, x);
                let text = tag.map_or_else(|| packet_text(m, x), str::to_string);
                // beside a character its value; a byte without a character is shown as its value already
                let character = ascii && tag.is_none() && x.kind == DecKind::Data && text != hex(x);
                let hex = if character { hex(x) } else { String::new() };
                let (len, errors) = (usize::from(tag.is_none()), usize::from(!x.ok));
                // (a field with parts of its own: the columns of its module)
                let module = m.dec_proto.module();
                let cols = module.map_or(&[][..], |module| module.field_columns());
                let cells = module.filter(|_| !cols.is_empty()).map_or_else(Vec::new, |module| module.field_cells(x));
                let why = module.and_then(|module| module.field_error(x)).unwrap_or("");
                MsgRow {
                    index: 0,
                    dec,
                    t: x.t0,
                    t1: x.t1,
                    who: who.to_string(),
                    len,
                    text,
                    hex,
                    errors,
                    why,
                    cols,
                    cells,
                }
            })
            .collect();
    }
    let cols = if view == EvView::Messages { m.dec_proto.module().map_or(&[][..], |x| x.columns()) } else { &[] };
    let groups = if view == EvView::Lines && ascii { lines(p) } else { decband::messages(m, p) };
    groups
        .into_iter()
        .map(|g| {
            let pk = &p[g];
            let values = || pk.iter().filter(|x| packet_tag(m, x).is_none());
            let why = if view == EvView::Lines && ascii {
                ""
            } else {
                m.dec_proto.module().map_or("", |x| x.message_error(pk))
            };
            // (a message the module finds wrong counts as one that failed a check)
            let errors = pk.iter().filter(|x| !x.ok).count().max(usize::from(!why.is_empty()));
            MsgRow {
                index: 0,
                dec,
                t: pk[0].t0,
                t1: pk.iter().map(|x| x.t1).fold(pk[0].t1, f64::max),
                who: who.to_string(),
                len: values().count(),
                text: decband::message_text(m, pk),
                hex: if ascii { values().map(hex).collect::<Vec<_>>().join(" ") } else { String::new() },
                errors,
                why,
                cols,
                cells: cells_of(m, cols, pk),
            }
        })
        .collect()
}

/// The cells of the frame `pk` for the columns `cols` of its protocol, with the texts the band writes.
fn cells_of(m: &Model, cols: &[(&'static str, Col)], pk: &[Packet]) -> Vec<String> {
    if cols.is_empty() {
        return Vec::new();
    }
    let text = |x: &Packet| packet_tag(m, x).map_or_else(|| packet_text(m, x), str::to_string);
    let texts: Vec<String> = pk.iter().map(text).collect();
    // the first field a column takes by its label, its place or its kind: the data of the frame follow it (the
    // sync byte of a LIN frame, which is data to the decoder, stands before its identifier)
    let takes = |k: usize| {
        cols.iter().any(|(_, c)| match *c {
            Col::Label(l) | Col::LabelOr(l) | Col::Part(l, _) => texts[k].starts_with(l),
            Col::At(n) => k == n,
            Col::Addr => matches!(pk[k].kind, DecKind::Addr { .. }),
            _ => false,
        })
    };
    let head = (0..pk.len()).find(|&k| takes(k));
    let after = |k: usize| head.is_none_or(|h| k > h);
    // (a data word of MIL-STD-1553 is data as a byte is)
    let data_kind = |k: usize| matches!(pk[k].kind, DecKind::Data | DecKind::Word { cmd: false });
    let is_data = |k: usize| data_kind(k) && after(k) && packet_tag(m, &pk[k]).is_none();
    let data: Vec<Packet> = (0..pk.len()).filter(|&k| is_data(k)).map(|k| pk[k].clone()).collect();
    // a word of two data lines is written "9F/FF"
    let half = |second: bool| {
        let part = |t: &String| t.split('/').nth(usize::from(second)).unwrap_or("").to_string();
        let parts = (0..pk.len()).filter(|&k| is_data(k)).map(|k| part(&texts[k])).filter(|t| !t.is_empty());
        parts.collect::<Vec<_>>().join(" ")
    };
    let address = || (0..pk.len()).find(|&k| matches!(pk[k].kind, DecKind::Addr { .. }));
    let msg = m.dec_proto.module().map_or_else(Vec::new, |module| module.message_cells(pk));
    cols.iter()
        .map(|(_, c)| match *c {
            Col::Label(l) => texts.iter().filter_map(|t| t.strip_prefix(l)).collect::<Vec<_>>().join(" "),
            Col::LabelOr(l) if texts.iter().any(|t| t.starts_with(l)) => {
                texts.iter().filter_map(|t| t.strip_prefix(l)).collect::<Vec<_>>().join(" ")
            }
            // (without the start of frame, which every frame has)
            Col::LabelOr(_) => {
                let other = (0..pk.len()).filter(|&k| pk[k].kind != DecKind::Start).map(|k| texts[k].as_str());
                other.collect::<Vec<_>>().join(" ")
            }
            Col::Part(l, n) => {
                let labelled = texts.iter().filter(|t| t.starts_with(l));
                labelled.filter_map(|t| t.split(' ').nth(n)).collect::<Vec<_>>().join(" ")
            }
            Col::At(n) => texts.get(n).cloned().unwrap_or_default(),
            Col::Addr => address().map_or_else(String::new, |k| texts[k].clone()),
            Col::Data => decband::message_text(m, &data),
            Col::Count => data.len().to_string(),
            Col::Rest => decband::message_text(m, head.map_or(pk, |h| &pk[h + 1..])),
            Col::First => half(false),
            Col::Second => half(true),
            // (a column of one field, not of a frame)
            Col::Own => String::new(),
            Col::Msg(n) => msg.get(n).cloned().unwrap_or_default(),
        })
        .collect()
}

/// Height the Events pane wants for `rows` rows.
pub(super) fn want_h(rows: usize) -> i32 {
    if rows == 0 { EMPTY_H } else { HEAD + TITLES + rows as i32 * tables::ROW_H }
}

/// What the last column says of a row: how many of its fields failed a check. A row without one says nothing: an "OK"
/// on every row took the room of the text and said no more than the missing red does.
fn errors_text(errors: usize) -> String {
    match errors {
        0 => String::new(),
        1 => "1 error".to_string(),
        k => format!("{} errors", k),
    }
}

/// Whether a pane `w` wide has its Errors column: it has the room, and `failed` rows of the record failed a check. A
/// record without a failed check has no such column, and its text has the room.
/// The widths of a protocol's columns in `free` pixels. A column that no row of the record fills is left out (the
/// MISO column of an SPI bus read on one line, the slow channel of a SENT sensor that has none). Of the others each
/// is as wide as its title and its cells need (`need`; a third of the room at most), and the columns of the data
/// (`data`; the last column where there is none) share what is left.
fn own_widths(need: &[i32], data: &[bool], filled: &[bool], free: i32) -> Vec<i32> {
    let n = need.len();
    let last = (0..n).rev().find(|&k| filled[k]);
    let any = (0..n).any(|k| filled[k] && data[k]);
    let wide = |k: usize| filled[k] && if any { data[k] } else { Some(k) == last };
    let mut w: Vec<i32> = (0..n).map(|k| if filled[k] && !wide(k) { need[k].min(free / 3) } else { 0 }).collect();
    let share = (0..n).filter(|&k| wide(k)).count() as i32;
    let left = (free - w.iter().sum::<i32>()).max(0);
    for k in (0..n).filter(|&k| wide(k)) {
        w[k] = left / share;
    }
    w
}

fn errors_column(w: i32, failed: usize) -> bool {
    w >= SMALL_W && failed > 0
}

/// Text in a cell from `x`, `w` wide: cut with dots where it is longer, left out where not even that fits.
fn cell(cv: &mut Canvas, f: &Font, at: (i32, i32), w: i32, s: &str, col: Rgb) {
    let room = w - 10;
    // a message can be as long as the record: only what a row can hold is measured
    let head: String = s.chars().take(240).collect();
    let s = startup::fit_text(f, &head, room);
    if !s.is_empty() && f.width(&s) <= room {
        cv.text_in(f, Rect::new(at.0, at.1, room, tables::ROW_H), 0, &s, col);
    }
}

impl Ui {
    /// What the decoders that are on found in the record, as the rows of the view chosen in the Events pane, in time
    /// order. A module that lists its commands itself (`Protocol::events`) has those rows in every view.
    pub(super) fn decode_rows(&self) -> Vec<MsgRow> {
        let mut v = Vec::new();
        for k in 0..DECODERS {
            if !self.m.dec_is_on(k) {
                continue;
            }
            let m = self.m.dec_view(k);
            let Ok(p) = self.decode_packets_of(&m) else { continue };
            let inverted = if crate::trig::read_inverted(&m) { " (inverted)" } else { "" };
            let who = format!("{} {}{}", self.dec_tag(k), m.dec_name(), inverted);
            match m.dec_proto.module().and_then(|x| x.events(&p, &m.dec_opts())) {
                Some(rows) => v.extend(rows.into_iter().map(|(t, text, ok)| {
                    let (hex, errors) = (String::new(), usize::from(!ok));
                    let (who, cells) = (who.clone(), Vec::new());
                    MsgRow { index: 0, dec: k, t, t1: t, who, len: 1, text, hex, errors, why: "", cols: &[], cells }
                })),
                None => v.extend(rows_of(&m, k, &who, &p, self.m.ev_view)),
            }
        }
        v.sort_by(|a, b| a.t.total_cmp(&b.t));
        for (i, row) in v.iter_mut().enumerate() {
            row.index = i + 1;
        }
        v
    }

    /// The rows the Events pane lists: all, or the ones with a failed check.
    fn events_listed(&self) -> impl Iterator<Item = &MsgRow> {
        let only = self.m.ev_errors;
        self.pn.rows.iter().filter(move |e| !only || e.errors > 0)
    }

    /// Number of rows the Events pane lists.
    pub(super) fn events_count(&self) -> usize {
        self.events_listed().count()
    }

    /// The record on the screen: a chosen row belongs to one.
    fn ev_record(&self) -> (u64, Option<usize>) {
        (self.counts.wfm_total, self.history.pos)
    }

    /// The chosen row, while its record is the one shown.
    pub(super) fn ev_selected(&self) -> Option<EvSel> {
        self.pn.ev_sel.filter(|s| s.rec == self.ev_record())
    }

    /// Whether that row is the chosen one.
    fn ev_is_selected(&self, e: &MsgRow) -> bool {
        self.ev_selected().is_some_and(|s| (s.dec, s.t0, s.t1) == (e.dec, e.t, e.t1))
    }

    /// The buttons in the head of a pane `r`: the three views and Errors only, or in a pane too small for them one
    /// button that says the view and opens the list of them, and Errors while there is room.
    fn events_head(&self, r: Rect) -> EvHead {
        let f = self.fonts();
        // in the title strip of a docked panel the buttons stand right of the title and left of its own buttons, and
        // the pane under the strip is all rows; else they have a row of their own
        let strip = self.events_strip();
        let (x0, y, h, limit) = match strip {
            Some(s) => {
                let dr = Rect::new(s.x, s.y, s.w, r.h + STRIP_H);
                let stop = self.dock_extra_rect(VKind::Pane, dr).map_or(Self::dock_close_rect(dr).x, |e| e.x);
                (s.x + 26 + f.mb.width("Events") + 16, s.y + 1, s.h - 2, stop + 8)
            }
            // a pane under the plot has its close button at the right end of the head; a docked one (tabbed) has it in
            // its title
            None => (r.x + 8, r.y + 2, HEAD - 4, r.right() - if self.dock_on() { 8 } else { 54 }),
        };
        let strip = strip.is_some();
        let seg = |v: EvView| f.sb.width(v.name()) + 24;
        let all = f.sb.width("Errors only") + 24;
        let mut x = x0;
        if x + EV_VIEWS.iter().map(|&v| seg(v)).sum::<i32>() + 10 + all <= limit {
            let views = EV_VIEWS
                .iter()
                .map(|&v| {
                    let b = Rect::new(x, y, seg(v), h);
                    x += b.w;
                    (b, v)
                })
                .collect();
            let errors = Rect::new(x + 10, y, all, h);
            return EvHead { views, cycle: false, errors: Some(errors), right: errors.right(), strip };
        }
        // room for the longest name and the mark that there is more
        let w = EV_VIEWS.iter().map(|&v| seg(v)).max().unwrap_or(0) + 14;
        let one = Rect::new(x, y, w.min((limit - x).max(44)), h);
        let e = Rect::new(one.right() + 6, y, f.sb.width("Errors") + 20, h);
        let errors = (e.right() <= limit).then_some(e);
        let right = errors.map_or(one.right(), |e| e.right());
        EvHead { views: vec![(one, self.m.ev_view)], cycle: true, errors, right, strip }
    }

    /// The title strip of the docked Events panel, when the buttons of its head live in it: a docked or floating panel
    /// that is not one tab of a group (the tabs fill the strip). Else the buttons have a row under the title.
    fn events_strip(&self) -> Option<Rect> {
        if !self.dock_on() || self.m.pane != tables::PANE_EVENTS || self.m.layout == Layout::Default {
            return None;
        }
        let r = self.dock_rect(VKind::Pane)?;
        self.dock_tab_rects(VKind::Pane, r).is_empty().then(|| Self::dock_strip(r))
    }

    /// The height of the row of buttons in the pane: none while they are in the strip.
    fn events_head_h(&self) -> i32 {
        if self.events_strip().is_some() { 0 } else { HEAD }
    }

    /// Where the rows are, and how many fit.
    fn events_rows(&self, r: Rect) -> (Rect, usize) {
        let top = self.events_head_h() + TITLES;
        let fit = ((r.h - top) / tables::ROW_H).max(0);
        (Rect::new(r.x, r.y + top, r.w, fit * tables::ROW_H), fit as usize)
    }

    /// Whether a touch at (x, y) is on a button of the head in the strip: the strip is the touch area, the buttons
    /// are drawn smaller.
    pub(super) fn events_strip_hit(&self, x: i32, y: i32) -> bool {
        let Some(s) = self.events_strip() else { return false };
        let head = self.events_head(self.pane_rect());
        s.contains(x, y)
            && (head.views.iter().any(|(b, _)| x >= b.x && x < b.right())
                || head.errors.is_some_and(|b| x >= b.x && x < b.right()))
    }

    /// The lift of a touch that began on a button of the strip.
    pub(super) fn events_strip_tap(&mut self, x: i32, y: i32) {
        let head = self.events_head(self.pane_rect());
        self.events_head_tap(&head, x, y);
    }

    /// The scroll bar beside the rows `area`, of which `fit` are on the screen.
    fn events_scroll(&self, area: Rect, fit: usize) -> scroll::Bar {
        // at the edge of the rows, over them: no room is kept for it
        let track = Rect::new(area.right() - BAR_AREA, area.y, BAR_AREA, area.h);
        scroll::Bar { track, total: self.events_count() as i32, shown: fit as i32, pos: self.pn.ev_top as i32 }
    }

    /// Whether the bar is on the screen: for a moment after the list was scrolled. A list that stands still shows
    /// none, and the right edge of its rows is for them.
    pub(super) fn events_bar_shown(&self) -> bool {
        self.pn.ev_bar_t.is_some_and(|t| self.now - t < BAR_SHOWN_S)
    }

    /// The scroll bar of the Events pane and its rows, while the pane shows them.
    pub(super) fn events_bar(&self) -> Option<(scroll::Bar, Rect)> {
        // Default's Events pane is upstream's list, without a scroll bar
        if self.pane_h() == 0 || self.m.pane != tables::PANE_EVENTS || self.m.layout == Layout::Default {
            return None;
        }
        let (rows, fit) = self.events_rows(self.pane_rect());
        Some((self.events_scroll(rows, fit), rows))
    }

    /// A tap on the Events pane: a view, Errors only, or a row. The view moves to the row and it is the chosen one;
    /// a tap on the chosen row lets go of it.
    pub(super) fn events_tap(&mut self, x: i32, y: i32) {
        let head = self.events_head(self.pane_rect());
        if self.events_head_tap(&head, x, y) {
            return;
        }
        let Some((bar, rows)) = self.events_bar() else { return };
        if !rows.contains(x, y) || (bar.needed() && self.events_bar_shown() && bar.track.contains(x, y)) {
            return;
        }
        let i = bar.top() as usize + ((y - rows.y) / tables::ROW_H) as usize;
        let row = self.events_listed().nth(i).map(|e| (e.dec, e.t, e.t1, self.ev_is_selected(e)));
        match row {
            Some((_, _, _, true)) => self.pn.ev_sel = None,
            Some((dec, t0, t1, false)) => {
                // a tap marks the row and its box. A stopped scope also goes to it; a running one is left as it is:
                // a finger that missed its row on a list this dense stopped the acquisition and moved the
                // horizontal position
                if self.m.run == RunState::Run {
                    self.toast("Stop the scope to go to an event");
                } else {
                    self.go_to_time(t0);
                }
                self.pn.ev_sel = Some(EvSel { dec, t0, t1, rec: self.ev_record() });
            }
            None => {}
        }
    }

    /// A tap on the buttons `head` of the pane: a view, the list of the views, or Errors only. Whether it was one.
    fn events_head_tap(&mut self, head: &EvHead, x: i32, y: i32) -> bool {
        // in the strip the whole height of the strip is the touch area
        let hit = |b: Rect| if head.strip { Rect::new(b.x, b.y - 1, b.w, b.h + 2) } else { b };
        if let Some(&(b, v)) = head.views.iter().find(|(b, _)| hit(*b).contains(x, y)) {
            if head.cycle {
                // the one button of a narrow pane lists the views: stepping through them blind showed two wrong
                // lists on the way to the third
                let rows = EV_VIEWS.iter().map(|&w| (w.name().to_string(), workspace::Action::EvView(w))).collect();
                self.popup("Rows", b.x, b.bottom() + 4, rows);
            } else {
                self.events_view(v);
            }
        } else if head.errors.is_some_and(|b| hit(b).contains(x, y)) {
            self.m.ev_errors = !self.m.ev_errors;
            self.events_relist();
        } else {
            return false;
        }
        true
    }

    /// The pane lists other rows (another view, or the failed ones alone): from the top, at once; the choice is part
    /// of the set-up.
    fn events_relist(&mut self) {
        self.pn.ev_top = 0;
        self.pn.rows = self.decode_rows();
        self.mark_changed();
    }

    /// The view chosen from the list of a narrow pane's button.
    pub(super) fn events_view(&mut self, v: EvView) {
        self.m.ev_view = v;
        self.events_relist();
    }

    /// The columns of the protocol the pane lists, when its rows are the frames of one: every row has the same ones.
    /// Rows of two protocols, of fields or of lines of text are one text each.
    pub(super) fn events_columns(&self) -> Option<&'static [(&'static str, Col)]> {
        let own = self.pn.rows.first()?.cols;
        (!own.is_empty() && self.pn.rows.iter().all(|e| std::ptr::eq(e.cols, own))).then_some(own)
    }

    /// The widths of the protocol's columns in `free` pixels (`own_widths`). The cells measured are those of the
    /// first rows of the record, and a column is there when a row of the whole record fills it: of the record, not
    /// of the screen, so the columns stand still while the list scrolls.
    fn events_own_widths(&self, own: &[(&'static str, Col)], f: &Font, free: i32, pad: i32) -> Vec<i32> {
        let need = |k: usize| -> i32 {
            let cells = self.pn.rows.iter().take(64).filter_map(|e| e.cells.get(k)).map(|c| f.width(c));
            cells.fold(f.width(own[k].0), i32::max) + pad
        };
        let need: Vec<i32> = (0..own.len()).map(need).collect();
        let is_data = |c: &(&str, Col)| matches!(c.1, Col::Data | Col::Rest | Col::First | Col::Second);
        let data: Vec<bool> = own.iter().map(is_data).collect();
        let filled = |k: usize| self.pn.rows.iter().any(|e| e.cells.get(k).is_some_and(|c| !c.is_empty()));
        let filled: Vec<bool> = (0..own.len()).map(filled).collect();
        own_widths(&need, &data, &filled, free)
    }

    /// The widths of the columns that depend on what is listed: the decoder names of the whole record (0 without a
    /// row) and whether a row has hex bytes. Of the record, not of the rows on the screen: the columns stand still
    /// while the list scrolls.
    fn events_widths(&self, f: &Font) -> (i32, bool) {
        let mut seen = [false; DECODERS];
        let (mut who, mut hex) = (0, false);
        for e in &self.pn.rows {
            if !seen[e.dec] {
                seen[e.dec] = true;
                who = who.max(f.width(&e.who));
            }
            hex |= !e.hex.is_empty();
        }
        (who.min(140), hex)
    }

    /// A tap on a box of a decoder's band: its row is the chosen one (a second tap lets go of it), the Events pane
    /// shows that row, and a note says its whole text, of which a box may hold only the beginning.
    pub(super) fn band_tap(&mut self, x: i32, y: i32) -> bool {
        let hit = self.band_hits.borrow().iter().find(|h| h.0.contains(x, y)).copied();
        let Some((_, dec, t0)) = hit else { return false };
        let rows = self.decode_rows();
        let Some(at) = rows.iter().position(|e| e.dec == dec && e.t <= t0 && t0 <= e.t1) else { return false };
        let e = rows[at].clone();
        if self.ev_is_selected(&e) {
            self.pn.ev_sel = None;
            return true;
        }
        self.pn.ev_sel = Some(EvSel { dec, t0: e.t, t1: e.t1, rec: self.ev_record() });
        let only = self.m.ev_errors;
        self.pn.ev_top = rows[..at].iter().filter(|r| !only || r.errors > 0).count();
        self.pn.rows = rows;
        let text: String = e.text.chars().take(60).collect();
        self.toast(&format!("{}  {}  {}", e.who, si(e.t, "s"), text));
        true
    }

    /// The buttons of the head and what the record holds, in the strip of the docked panel (`r` is the pane under it).
    pub(super) fn draw_events_strip(&self, cv: &mut Canvas, r: Rect) {
        if self.events_strip().is_some() {
            self.draw_events_head(cv, r);
        }
    }

    /// The buttons of the head, and right of them what the record holds, where there is room.
    fn draw_events_head(&self, cv: &mut Canvas, r: Rect) {
        let f = self.fonts();
        let head = self.events_head(r);
        // the row of the text beside the buttons: the head row of the pane, or the strip
        let (text_y, text_h) = match self.events_strip() {
            Some(s) => (s.y, s.h),
            None => (r.y, HEAD),
        };
        for &(b, v) in &head.views {
            let on = !head.cycle && v == self.m.ev_view;
            btn(cv, b, if on { ACCENT } else { BTN }, if on { ACCENT } else { BTN_BORDER });
            if head.cycle {
                // the view that is on, and that a tap goes on to the next
                let name = startup::fit_text(&f.sb, v.name(), b.w - 30);
                cv.text_in(&f.sb, Rect::new(b.x + 10, b.y, b.w - 30, b.h), 0, &name, TEXT);
                chevron_down(cv, b.right() - 18, b.y + b.h / 2 - 3, LABEL);
            } else {
                cv.text_in(&f.sb, b, 1, v.name(), if on { DARK_TEXT } else { TEXT });
            }
        }
        if let Some(b) = head.errors {
            let only = self.m.ev_errors;
            btn(cv, b, if only { mix(BG, RED, 90) } else { BTN }, if only { RED } else { BTN_BORDER });
            cv.text_in(&f.sb, b, 1, if head.cycle { "Errors" } else { "Errors only" }, TEXT);
        }
        // what the record holds, right of the buttons, where there is room
        let total = self.pn.rows.len();
        let failed = self.pn.rows.iter().filter(|e| e.errors > 0).count();
        let limit = match self.events_strip() {
            // left of the buttons of the strip itself
            Some(s) => s.right() - 100 - 8,
            None => r.right() - if self.dock_on() { 8 } else { 54 },
        };
        let x = head.right + 14;
        let rows_txt = self.events_count_text(total);
        let failed_txt = if failed == 1 { "1 with an error".to_string() } else { format!("{} with errors", failed) };
        if x + f.s.width(&rows_txt) <= limit {
            cv.text_in(&f.s, Rect::new(x, text_y, f.s.width(&rows_txt), text_h), 0, &rows_txt, DIM);
            let x = x + f.s.width(&rows_txt) + 14;
            if failed > 0 && x + f.s.width(&failed_txt) <= limit {
                cv.text_in(&f.s, Rect::new(x, text_y, f.s.width(&failed_txt), text_h), 0, &failed_txt, RED);
            }
        }
    }

    pub(super) fn draw_pane_events(&self, cv: &mut Canvas, r: Rect) {
        let f = self.fonts();
        // (in the strip of the docked panel the buttons are drawn with the strip)
        if self.events_strip().is_none() {
            self.draw_events_head(cv, r);
        }
        let total = self.pn.rows.len();
        let failed = self.pn.rows.iter().filter(|e| e.errors > 0).count();
        let head_h = self.events_head_h();
        let (area, fit) = self.events_rows(r);
        let n = self.events_count();
        if n == 0 || fit == 0 {
            let note = if total > 0 {
                "No row with an error"
            } else if self.decode_busy() {
                "Decoding the full reference record..."
            } else {
                "No decoder events in this record"
            };
            let note = startup::fit_text(&f.sb, note, r.w - 20);
            cv.text(&f.sb, r.x + 12, r.y + head_h + 15, &note, DIM);
            return;
        }
        let bar = self.events_scroll(area, fit);
        let top = bar.top() as usize;
        let shown: Vec<&MsgRow> = self.events_listed().skip(top).take(fit).collect();
        // the columns: number, time, decoder, length, text, hex, errors. The last one is there when a row of the
        // record failed a check (the whole record, so it does not come and go while the list scrolls). A narrow pane
        // keeps number, time, text and errors; a small one the time and the text, which is red where a check failed
        let pad = 14;
        let (wide, small) = (r.w >= NARROW_W, r.w < SMALL_W);
        let (who, hex) = self.events_widths(&f.s);
        let hex_on = wide && hex;
        let w_number = if small { 0 } else { f.s.width("99999") + pad };
        // the widest a time of three digits gets, whatever the rows on the screen say
        let w_time = f.s.width("-888.8 \u{b5}s") + pad;
        let w_who = if wide { who + pad } else { 0 };
        let w_len = if wide { f.s.width("Len").max(f.s.width("9999")) + pad } else { 0 };
        // (an error in words takes what the longest one needs, a third of the pane at most)
        let w_status = if errors_column(r.w, failed) {
            let said = self.pn.rows.iter().map(|e| f.s.width(&e.error_text())).fold(f.s.width("999 errors"), i32::max);
            (said + pad).min(r.w / 3)
        } else {
            0
        };
        let left = r.x + if small { 8 } else { 12 };
        // the bar shows over the right edge of the rows for a moment after a scroll, and no room is kept for it
        let end = area.right() - if small { 12 } else { 20 };
        // the columns of the protocol (a wide pane that lists the frames of one protocol) take the place of the
        // length, the text and the hex bytes
        let own = self.events_columns().filter(|_| wide);
        let w_len = if own.is_some() { 0 } else { w_len };
        let free = (end - w_status - left - w_number - w_time - w_who - w_len).max(0);
        let w_text = if hex_on { free * 2 / 5 } else { free };
        let mut cols: Vec<(&str, i32)> = vec![("#", w_number), ("Time", w_time), ("Source", w_who), ("Len", w_len)];
        // (the place of the first column that is drawn bold: the text, or the cells of the protocol)
        let bold = cols.len();
        match own {
            Some(own) => cols.extend(own.iter().map(|c| c.0).zip(self.events_own_widths(own, &f.sb, free, pad))),
            None => cols.extend([("Text", w_text), ("Hex", free - w_text)]),
        }
        let bold = bold..if own.is_some() { cols.len() } else { bold + 1 };
        cols.push(("Errors", w_status));
        let mut x = left;
        for &(title, w) in &cols {
            if w > 0 {
                cell(cv, &f.s, (x, r.y + head_h - 3), w, title, DIM);
            }
            x += w;
        }
        cv.hline(r.x + 8, end, area.y - 1, LINE);
        for (i, e) in shown.iter().enumerate() {
            let y = area.y + i as i32 * tables::ROW_H;
            // (the stripes run to the edge: no empty strip beside the list)
            let line = Rect::new(r.x + 4, y, area.right() - 4 - r.x - 4, tables::ROW_H);
            if self.ev_is_selected(e) {
                // the chosen row: its box in the band and its span on the plot are marked like it
                cv.fill(line, mix(BG, ACCENT, 46));
                cv.round_outline(line, 3, ACCENT);
            } else if (top + i) % 2 == 1 {
                cv.fill(line, ROW_ALT);
            }
            let status = e.error_text();
            let len = if e.len > 0 { e.len.to_string() } else { String::new() };
            let number = e.index.to_string();
            let ink = if e.errors > 0 { RED } else { TEXT };
            let mut texts = vec![(number, DIM), (si(e.t, "s"), LABEL), (e.who.clone(), DIM), (len, LABEL)];
            match own {
                Some(_) => texts.extend(e.cells.iter().map(|c| (c.clone(), ink))),
                None => texts.extend([(e.text.clone(), ink), (e.hex.clone(), LABEL)]),
            }
            texts.push((status, RED));
            let mut x = left;
            for (k, (&(_, w), (text, ink))) in cols.iter().zip(&texts).enumerate() {
                if w > 0 {
                    cell(cv, if bold.contains(&k) { &f.sb } else { &f.s }, (x, y), w, text, *ink);
                }
                x += w;
            }
        }
        if self.events_bar_shown() {
            bar.draw(cv);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn byte(t0: f64, value: u8) -> Packet {
        Packet { t0, t1: t0 + 1.0, kind: DecKind::Data, value: value as u32, value2: None, ok: true }
    }

    fn uart() -> Model {
        let mut m = Model::default();
        m.dec_proto = Proto::UART;
        m.dec_bits = 8;
        m.dec_ascii = true;
        m
    }

    /// The same characters as messages (split at the long pause), as fields and as lines (split at the line feed).
    /// The columns of a protocol share the room: one that no row fills is left out, the data takes what is left.
    #[test]
    fn a_column_no_row_fills_is_left_out() {
        // SPI read on one line: the MOSI column has the whole room, a frame of many bytes is not cut
        assert_eq!(own_widths(&[40, 40], &[true, true], &[true, false], 600), [600, 0]);
        // both lines: they share it
        assert_eq!(own_widths(&[40, 40], &[true, true], &[true, true], 600), [300, 300]);
        // CAN: identifier, length and CRC as wide as they need, the data the rest
        assert_eq!(own_widths(&[60, 30, 50, 70], &[false, false, true, false], &[true; 4], 600), [60, 30, 440, 70]);
        // a record of remote frames has no data: the last column that is filled takes the room
        let no_data = [true, true, false, true];
        assert_eq!(own_widths(&[60, 30, 50, 70], &[false, false, true, false], &no_data, 600), [60, 30, 0, 510]);
        // a column that is not the data's has a third of the room at most; no row at all: no column
        assert_eq!(own_widths(&[500, 40], &[false, true], &[true, true], 600), [200, 400]);
        assert_eq!(own_widths(&[40, 40], &[true, true], &[false, false], 600), [0, 0]);
    }

    #[test]
    fn the_rows_are_messages_fields_or_lines() {
        // "Hi\n" at one pace, a long pause, then "yo"
        let mut p: Vec<Packet> = b"Hi\n".iter().enumerate().map(|(k, &b)| byte(k as f64 * 3.0, b)).collect();
        p.extend(b"yo".iter().enumerate().map(|(k, &b)| byte(20.0 + k as f64 * 3.0, b)));
        p[1].ok = false;
        let m = uart();
        let msgs = rows_of(&m, 0, "D1 UART", &p, EvView::Messages);
        let shown: Vec<_> = msgs.iter().map(|e| (e.t, e.len, e.text.as_str(), e.hex.as_str(), e.errors)).collect();
        assert_eq!(shown, [(0.0, 3, "Hi\\n", "48 69 0A", 1), (20.0, 2, "yo", "79 6F", 0)]);
        // a message lasts from its first character to the end of its last
        assert_eq!((msgs[0].t1, msgs[1].t1), (7.0, 24.0));
        let fields = rows_of(&m, 0, "D1 UART", &p, EvView::Fields);
        assert_eq!(fields.len(), 5);
        assert_eq!((fields[0].text.as_str(), fields[0].hex.as_str(), fields[1].errors), ("H", "48", 1));
        // the line feed is shown as its value, so it has no hex beside it
        assert_eq!((fields[2].text.as_str(), fields[2].hex.as_str()), ("0A", ""));
        let mut q = p.clone();
        q.push(byte(40.0, b'\n'));
        let lines = rows_of(&m, 0, "D1 UART", &q, EvView::Lines);
        assert_eq!(lines.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["Hi\\n", "yo\\n"]);
        // a decoder that shows values has no hex column and its lines are its messages
        let mut hexm = uart();
        hexm.dec_ascii = false;
        let rows = rows_of(&hexm, 0, "D1 UART", &p, EvView::Lines);
        let shown: Vec<_> = rows.iter().map(|e| (e.text.as_str(), e.hex.as_str())).collect();
        assert_eq!(shown, [("48 69 0A", ""), ("79 6F", "")]);
    }

    /// Decode > Threshold to middle puts the threshold half way between the levels of the source's signal, and the
    /// UART decoder has the line's polarity among its rows.
    #[test]
    fn the_decode_menu_sets_the_threshold_to_the_middle() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.ch[2].on = true;
        u.m.dec_on = true;
        u.m.dec_src = 2;
        for _ in 0..4 {
            u.step(0.1);
        }
        let items = u.decode_items();
        assert!(items.contains(&Item::DecThrMid) && items.contains(&Item::UartPol), "{:?}", items);
        u.m.dec_thr = 9.0;
        u.activate(Item::DecThrMid);
        assert!((u.m.dec_thr - 1.65).abs() < 0.3, "the demo's UART line swings from 0 to 3.3 V: {}", u.m.dec_thr);
        // a decoder of another protocol has no polarity row
        u.m.dec_proto = Proto::I2C;
        assert!(!u.decode_items().contains(&Item::UartPol));
    }

    /// An SPI word 9F on MOSI at 3.3 V, with the clock and the chip select at a tenth of that level.
    fn spi_mixed_levels() -> (Model, [Vec<f32>; NCH], f64) {
        let [cs, clk, mosi, _] = oos_dsp::sim::spi_logic(&[(0x9f, 0xff)], 8, 0, 2e6, 0.0, 3.3);
        let dt = 25e-9;
        let wave = |l: &oos_dsp::sim::Logic, gain: f32| -> Vec<f32> {
            oos_dsp::sim::sample(|t| l.v(t), 0.0, dt, 2400).into_iter().map(|v| v * gain).collect()
        };
        let mut m = Model::default();
        m.dec_proto = Proto::SPI;
        (m.dec_src, m.dec_src2, m.dec_cs) = (0, 1, Some(2));
        (m.dec_bits, m.dec_mode, m.dec_thr) = (8, 0, 1.65);
        m.dec_opt = [0; crate::protocols::OPTS];
        (m, [wave(&mosi, 1.0), wave(&clk, 0.1), wave(&cs, 0.1), Vec::new()], dt)
    }

    fn words(m: &Model, ch: &[Vec<f32>; NCH], dt: f64) -> Vec<u32> {
        let p = crate::trig::decode(m, std::array::from_fn(|c| &ch[c][..]), dt).unwrap();
        p.iter().filter(|x| x.kind == DecKind::Data).map(|x| x.value).collect()
    }

    /// Lines of different levels are read with a threshold each: a clock at a tenth of the data's level gives no
    /// word at the decoder's threshold, and the word with its own. The set-up keeps a line's threshold, and `main`
    /// for a line that has none.
    #[test]
    fn a_line_has_a_threshold_of_its_own() {
        let (mut m, ch, dt) = spi_mixed_levels();
        assert!(words(&m, &ch, dt).is_empty(), "the clock never reaches 1.65 V");
        m.dec_thr_line = [Some(0.165), Some(0.165), None];
        assert_eq!(words(&m, &ch, dt), [0x9f]);
        assert_eq!((m.dec_thr_of(0), m.dec_thr_of(1), m.dec_thr_of(3)), (1.65, 0.165, 1.65));
        let text = crate::store::setup_text(&m);
        let has = |line: &str| text.lines().any(|l| l == line);
        assert!(has("dec_thr2 = 0.165") && has("dec_thr4 = main"), "{text}");
        let back = crate::store::parse_setup(&text).unwrap();
        assert_eq!(back.dec_thr_line, m.dec_thr_line);
        // the decoder keeps them when another decoder is edited and it comes back
        m.dec_select(1);
        assert_eq!(m.dec_thr_line, [None; 3]);
        m.dec_select(0);
        assert_eq!(m.dec_thr_line, [Some(0.165), Some(0.165), None]);
    }

    /// Threshold to middle sets each line's threshold to the middle of its own signal, and the sheet names the
    /// rows by their lines.
    #[test]
    fn threshold_to_middle_is_each_lines_own() {
        let (m, ch, dt) = spi_mixed_levels();
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        (u.m.dec_proto, u.m.dec_src, u.m.dec_src2, u.m.dec_cs) = (m.dec_proto, m.dec_src, m.dec_src2, m.dec_cs);
        (u.m.dec_bits, u.m.dec_mode, u.m.dec_on) = (8, 0, true);
        let (triggered, partial, pairs) = (true, false, false);
        u.last[0] = Some(Record { t0: 0.0, dt, ch: ch.clone(), triggered, partial, raw: None, la: None, pairs, peak: false });
        let items = u.decode_items();
        let lines: Vec<usize> = (1..4).filter(|&r| items.contains(&Item::DecThrLine(r))).collect();
        assert_eq!(lines, [1, 2], "a threshold for the clock and the chip select, none for a fourth line");
        let labels = [Item::DecThr, Item::DecThrLine(1), Item::DecThrLine(2)].map(|it| u.sheet_label(it));
        assert_eq!(labels, ["MOSI threshold", "Clock threshold", "CS threshold"]);
        u.dec_threshold_to_middle();
        let near = |a: f32, b: f32| (a - b).abs() < 0.05 * b;
        assert!(near(u.m.dec_thr, 1.65), "{}", u.m.dec_thr);
        let own = u.m.dec_thr_line;
        assert!(own[0].is_some_and(|t| near(t, 0.165)) && own[1].is_some_and(|t| near(t, 0.165)), "{own:?}");
        assert_eq!(u.m.dec_thr_line[2], None, "a line the protocol has not keeps none");
        assert_eq!(words(&u.m, &ch, dt), [0x9f], "and the word is read");
    }

    /// SPI: the bits of a word LSB first, a data line whose low level is a 1, and the chip select's polarity among
    /// the decoder's rows.
    #[test]
    fn spi_has_bit_order_and_data_polarity() {
        let (mut m, ch, dt) = spi_mixed_levels();
        m.dec_thr_line = [Some(0.165), Some(0.165), None];
        m.dec_opt[0] = 1;
        assert_eq!(words(&m, &ch, dt), [0xf9], "9F with its bits the other way round");
        m.dec_opt = [0, 1, 0, 0];
        assert_eq!(words(&m, &ch, dt), [0x60], "9F with every bit inverted");
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.dec_proto = Proto::SPI;
        let items = u.decode_items();
        let has = |it: Item| items.contains(&it);
        assert!(has(Item::SpiCs) && has(Item::DecOpt(0)) && has(Item::DecOpt(1)), "{items:?}");
        assert_eq!((u.item_label(Item::DecOpt(0)), u.item_label(Item::DecOpt(1))), ("Bit order", "Data polarity"));
        // the polarity of the select is not called like its line
        assert_eq!((u.item_label(Item::DecCs), u.item_label(Item::SpiCs)), ("CS", "CS polarity"));
    }

    /// SPI with a chip select and a MISO line: each frame begins with a start mark and is a message of its own, and
    /// a word is MOSI/MISO once the MISO line has a source.
    #[test]
    fn spi_frames_are_messages_and_miso_is_read() {
        // three words a frame, a frame every 50 us, mode 0
        let [cs, clk, mosi, miso] = oos_dsp::sim::spi_logic(&[(0x9f, 0xff), (0x00, 0x1f), (0x00, 0x40)], 8, 0, 2e6, 0.0, 3.3);
        let (dt, period) = (25e-9, 50e-6);
        let wave = |l: &oos_dsp::sim::Logic| -> Vec<f32> { oos_dsp::sim::sample(|t| l.v(t.rem_euclid(period)), 0.0, dt, 6400) };
        let ch: [Vec<f32>; NCH] = [wave(&mosi), wave(&clk), wave(&cs), wave(&miso)];
        let mut m = Model::default();
        m.dec_proto = Proto::SPI;
        (m.dec_src, m.dec_src2, m.dec_cs, m.dec_src4) = (0, 1, Some(2), None);
        (m.dec_bits, m.dec_mode, m.dec_thr) = (8, 0, 1.65);
        m.dec_opt = [0; crate::protocols::OPTS];
        let p = crate::trig::decode(&m, std::array::from_fn(|c| &ch[c][..]), dt).unwrap();
        let starts = p.iter().filter(|x| x.kind == DecKind::Start).count();
        assert!(starts >= 2, "a start mark for each frame the chip select begins in the record: {starts}");
        assert!(p.iter().all(|x| x.value2.is_none()), "no MISO without a source for it");
        // a message per frame: every message after the first begins with its start mark and has the three words
        let frames = oos_dsp::decode::bursts(&p);
        assert!(frames.len() >= 3, "{frames:?}");
        for f in &frames[1..frames.len() - 1] {
            let words: Vec<u32> = p[f.clone()].iter().filter(|x| x.kind == DecKind::Data).map(|x| x.value).collect();
            assert_eq!((p[f.start].kind, words), (DecKind::Start, vec![0x9f, 0x00, 0x00]));
        }
        assert_eq!(decband::message_text(&m, &p[frames[1].clone()]), "9F 00 00");
        // with the MISO line
        m.dec_src4 = Some(3);
        let p = crate::trig::decode(&m, std::array::from_fn(|c| &ch[c][..]), dt).unwrap();
        let frames = oos_dsp::decode::bursts(&p);
        assert_eq!(decband::message_text(&m, &p[frames[1].clone()]), "9F/FF 00/1F 00/40");
        // SPI names the line, and choosing SPI on a scope leaves it unset
        assert_eq!(Proto::SPI.roles()[3], Some("MISO"));
    }

    /// SPI and I2C have a Format of their own: with ASCII a message reads as text and has its bytes in hex beside
    /// it; an I2C address stays a number.
    #[test]
    fn spi_and_i2c_show_characters_on_request() {
        let mut m = Model::default();
        m.dec_proto = Proto::SPI;
        m.dec_bits = 8;
        m.dec_opt = [0; crate::protocols::OPTS];
        let p: Vec<Packet> = b"Hi".iter().enumerate().map(|(k, &b)| byte(k as f64, b)).collect();
        let hex = rows_of(&m, 0, "D1 SPI", &p, EvView::Messages);
        assert_eq!((hex[0].text.as_str(), hex[0].hex.as_str()), ("48 69", ""));
        m.dec_opt[2] = 1;
        let text = rows_of(&m, 0, "D1 SPI", &p, EvView::Messages);
        assert_eq!((text[0].text.as_str(), text[0].hex.as_str()), ("Hi", "48 69"));
        assert_eq!(packet_text(&m, &p[0]), "H");
        // I2C: the address as a number, the data as characters
        m.dec_proto = Proto::I2C;
        m.dec_opt = [0, 1, 0, 0];
        let addr = Packet { kind: DecKind::Addr { read: false }, value: 0x50, ..byte(0.0, 0) };
        assert_eq!((packet_text(&m, &addr), packet_text(&m, &byte(1.0, b'H'))), ("50 W".to_string(), "H".to_string()));
        // a byte that was not acknowledged keeps its value and its mark
        assert_eq!(packet_text(&m, &Packet { ok: false, ..byte(1.0, b'H') }), "48 N");
        // the UART's own Format is as before
        let mut u = Model::default();
        u.dec_proto = Proto::UART;
        (u.dec_bits, u.dec_ascii) = (8, false);
        assert_eq!(packet_text(&u, &byte(0.0, b'H')), "48");
    }

    /// A protocol that says where its frames begin has a row for each frame, whatever pauses a frame has inside: a
    /// LIN response that comes late stays with its header (the pause split it). And the mark an SPI frame begins
    /// with is no row of the Fields view.
    #[test]
    fn a_frame_is_one_row_and_a_mark_is_no_field() {
        let mut m = Model::default();
        m.dec_proto = Proto::LIN;
        m.dec_opt = [0; crate::protocols::OPTS];
        let at = |t: f64, kind: DecKind, value: u32| Packet { kind, value, ..byte(t, 0) };
        let id = DecKind::Id { ext: false, rtr: false };
        // break, sync, identifier; the response 30 field times later; the next frame
        let p = [
            at(0.0, DecKind::Break, 0),
            at(1.0, DecKind::Data, 0x55),
            at(2.0, id, 0x21),
            at(32.0, DecKind::Data, 0x10),
            at(33.0, DecKind::Crc, 0xce),
            at(40.0, DecKind::Break, 0),
            at(41.0, DecKind::Data, 0x55),
            at(42.0, id, 0x22),
        ];
        let rows = rows_of(&m, 0, "D1 LIN", &p, EvView::Messages);
        assert_eq!(rows.len(), 2, "{:?}", rows.iter().map(|r| r.text.as_str()).collect::<Vec<_>>());
        assert!(rows[0].text.ends_with("10 CS CE") && rows[1].text.contains("ID 22"), "{:?}", rows[0].text);
        // SPI: two frames of a word each, each with its mark
        m.dec_proto = Proto::SPI;
        m.dec_bits = 8;
        let (mark, word) = (DecKind::Start, DecKind::Data);
        let p = [at(0.0, mark, 0), at(0.1, word, 0x9f), at(5.0, mark, 0), at(5.1, word, 0x01)];
        let fields = rows_of(&m, 0, "D1 SPI", &p, EvView::Fields);
        assert_eq!(fields.iter().map(|r| r.text.as_str()).collect::<Vec<_>>(), ["9F", "01"]);
        assert_eq!(rows_of(&m, 0, "D1 SPI", &p, EvView::Messages).len(), 2);
    }

    /// A protocol with columns has them in the Events pane in place of one text: a CAN frame its identifier, the
    /// number of its data bytes, the bytes and the CRC; a LIN frame its identifier, the data of the response without
    /// the sync byte, and the checksum; an SPI frame the words of each data line. Rows that are fields have none.
    #[test]
    fn a_frame_has_the_cells_of_its_protocols_columns() {
        let mut m = Model::default();
        m.dec_opt = [0; crate::protocols::OPTS];
        m.dec_bits = 8;
        let at = |t: f64, kind: DecKind, value: u32| Packet { kind, value, ..byte(t, 0) };
        let id = |rtr: bool| DecKind::Id { ext: false, rtr };
        let titles = |rows: &[MsgRow]| rows[0].cols.iter().map(|c| c.0).collect::<Vec<_>>();
        m.dec_proto = Proto::CAN;
        let p = [
            at(0.0, DecKind::Start, 0),
            at(1.0, id(false), 0x101),
            at(2.0, DecKind::Data, 0x01),
            at(3.0, DecKind::Data, 0x03),
            at(4.0, DecKind::Crc, 0x562d),
            at(5.0, DecKind::Ack, 0),
            at(9.0, DecKind::Start, 0),
            at(10.0, id(true), 0x757),
            at(11.0, DecKind::Crc, 0x20bb),
        ];
        let rows = rows_of(&m, 0, "D1 CAN", &p, EvView::Messages);
        assert_eq!(titles(&rows), ["ID", "DLC", "Data", "CRC"]);
        assert_eq!(rows[0].cells, ["101", "2", "01 03", "562D"]);
        assert_eq!(rows[1].cells, ["757 RTR", "0", "", "20BB"]);
        assert!(rows_of(&m, 0, "D1 CAN", &p, EvView::Fields).iter().all(|r| r.cols.is_empty() && r.cells.is_empty()));
        m.dec_proto = Proto::LIN;
        let p = [
            at(0.0, DecKind::Break, 0),
            at(1.0, DecKind::Data, 0x55),
            at(2.0, id(false), 0x08),
            at(3.0, DecKind::Data, 0x67),
            at(4.0, DecKind::Data, 0x80),
            at(5.0, DecKind::Crc, 0x10),
        ];
        let rows = rows_of(&m, 0, "D1 LIN", &p, EvView::Messages);
        assert_eq!(titles(&rows), ["ID", "Data", "Checksum"]);
        assert_eq!(rows[0].cells, ["08", "67 80", "10"]);
        m.dec_proto = Proto::SPI;
        let word = |t: f64, mosi: u32, miso: u32| Packet { value2: Some(miso), ..at(t, DecKind::Data, mosi) };
        let p = [at(0.0, DecKind::Start, 0), word(0.1, 0x9f, 0xff), word(0.2, 0x00, 0xef)];
        let rows = rows_of(&m, 0, "D1 SPI", &p, EvView::Messages);
        assert_eq!(titles(&rows), ["MOSI", "MISO"]);
        assert_eq!(rows[0].cells, ["9F 00", "FF EF"]);
        m.dec_proto = Proto::I2C;
        let write = DecKind::Addr { read: false };
        let (start, stop) = (at(0.0, DecKind::Start, 0), at(3.0, DecKind::Stop, 0));
        let p = [start, at(1.0, write, 0x50), at(2.0, DecKind::Data, 0x42), stop];
        assert_eq!(rows_of(&m, 0, "D1 I2C", &p, EvView::Messages)[0].cells, ["50 W", "42 P"]);
        // MIL-STD-1553, where it is built in: the parts of the command word each in its column
        if let Some(proto) = Proto::from_key("Mil1553") {
            use oos_dsp::decode::{MIL1553_COMMAND, MIL1553_STATUS};
            m.dec_proto = proto;
            let word = |t: f64, cmd: bool, v: u32, role: Option<u32>| {
                Packet { value2: role, ..at(t, DecKind::Word { cmd }, v) }
            };
            let p = [
                word(0.0, true, 0x8842, Some(MIL1553_COMMAND)),
                word(1.0, false, 5, None),
                word(2.0, false, 6, None),
                word(3.0, true, 0x8801, Some(MIL1553_STATUS)),
                word(9.0, true, 0x8c12, Some(MIL1553_COMMAND)),
            ];
            let rows = rows_of(&m, 0, "D1 1553", &p, EvView::Messages);
            assert_eq!(
                titles(&rows),
                ["Type", "RT", "T/R", "SA/mode", "Mode code", "Status", "WC", "Data", "C/S payload"]
            );
            // and word by word: what each is, the mode code by its name, the count as words, a failed parity said
            let bad = Packet { ok: false, ..word(10.0, false, 0x0157, None) };
            let q = [p[0].clone(), p[1].clone(), p[3].clone(), p[4].clone(), bad];
            let words = rows_of(&m, 0, "D1 1553", &q, EvView::Fields);
            assert_eq!(
                titles(&words),
                ["Type", "RT", "T/R", "SA", "Mode code / words", "Data", "Status", "Status bits", "C/S payload"]
            );
            assert_eq!(words[0].cells, ["Command", "17", "R", "02", "2 words", "", "", "", "00001000010"]);
            assert_eq!(words[1].cells, ["Data", "", "", "", "", "0005", "", "", ""]);
            assert_eq!(words[2].cells, ["Status", "17", "", "", "", "", "TF", "000-000-00001", "00000000001"]);
            assert_eq!(
                words[3].cells,
                ["Command", "17", "T", "00", "Transmit last command", "", "", "", "10000010010"]
            );
            assert_eq!(words[4].cells, ["Data", "", "", "", "", "0157", "", "", ""]);
            assert_eq!(words[4].why, "Manchester or parity error");
            assert_eq!(words[0].why, "");
            assert_eq!(rows[0].cells, ["BC to RT", "17", "R", "02", "", "TF", "2", "0005 0006", "00001000010"]);
            assert_eq!(
                rows[1].cells,
                ["Mode command", "17", "T", "00", "Transmit last command", "", "", "", "10000010010"]
            );
        }
        // FlexRay, where it is built in: the start sequence every frame begins with is no part of the text of a
        // frame, and it is a field with its row
        if let Some(proto) = Proto::from_key("FlexRay") {
            m.dec_proto = proto;
            let p = [at(0.0, DecKind::Field("TSS"), 11), at(1.0, id(false), 0x2a), at(2.0, DecKind::Data, 0x11)];
            let text = |view: EvView| -> Vec<String> {
                rows_of(&m, 0, "D1 FlexRay", &p, view).iter().map(|r| r.text.clone()).collect()
            };
            assert_eq!(text(EvView::Messages), ["ID 02A 11"]);
            assert_eq!(text(EvView::Fields).first().map(String::as_str), Some("TSS B"));
        }
        // a UART has no columns: a row is its text
        m.dec_proto = Proto::UART;
        assert!(rows_of(&m, 0, "D1 UART", &[byte(0.0, b'H')], EvView::Messages)[0].cols.is_empty());
    }

    /// An I2C read is no error: the byte the controller ends it with is not acknowledged, by the rule of the bus, and
    /// the row says nothing of it; a byte that is not acknowledged anywhere else keeps its mark and counts. An
    /// address of ten bits is one field of three digits, whatever the R/W bit option says.
    #[test]
    fn an_i2c_read_is_no_error_and_a_ten_bit_address_is_one_field() {
        use oos_dsp::decode::{I2C_NAK_END, I2C_TEN_BIT};
        let mut m = Model::default();
        // the fork's rows and texts (Default keeps upstream's " N" on the last byte of a read)
        m.layout = Layout::Minimal;
        m.dec_proto = Proto::I2C;
        m.dec_bits = 8;
        m.dec_opt = [0; crate::protocols::OPTS];
        let at = |k: usize, kind: DecKind, value: u32| Packet { kind, value, ..byte(k as f64, 0) };
        let ended = Packet { value2: Some(I2C_NAK_END), ..at(3, DecKind::Data, 0x53) };
        let (start, stop) = (at(0, DecKind::Start, 0), at(4, DecKind::Stop, 0));
        let read = [start, at(1, DecKind::Addr { read: true }, 0x50), at(2, DecKind::Data, 0x4f), ended, stop];
        let rows = rows_of(&m, 0, "D1 I2C", &read, EvView::Messages);
        assert_eq!((rows.len(), rows[0].text.as_str(), rows[0].errors), (1, "50 R 4F 53 P", 0));
        // as characters, the last byte is one too
        m.dec_opt[1] = 1;
        assert_eq!(rows_of(&m, 0, "D1 I2C", &read, EvView::Messages)[0].text, "50 R OS P");
        m.dec_opt[1] = 0;
        // a write that was not acknowledged
        let mut write = read.clone();
        (write[1].kind, write[3]) = (DecKind::Addr { read: false }, Packet { ok: false, ..at(3, DecKind::Data, 0x53) });
        let rows = rows_of(&m, 0, "D1 I2C", &write, EvView::Messages);
        assert_eq!((rows[0].text.as_str(), rows[0].errors), ("50 W 4F 53 N P", 1));
        // ten bits
        let ten = Packet { value2: Some(I2C_TEN_BIT), ..at(1, DecKind::Addr { read: false }, 0x2a5) };
        assert_eq!(packet_text(&m, &ten), "2A5 W");
        assert_eq!(packet_text(&m, &Packet { value: 0x050, ..ten.clone() }), "050 W", "not the 7-bit address 50");
        m.dec_opt[0] = 1;
        assert_eq!(packet_text(&m, &ten), "2A5 W", "it has no one byte with the read/write bit in it");
    }

    /// The Events pane lists the demo's UART line as messages, switches to fields by a tap on its head, and a tap on
    /// a row needs no more than the row.
    #[test]
    fn the_events_pane_lists_messages_and_switches_by_touch() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        // the fork's pane (Default's is upstream's list)
        u.m.layout = Layout::Minimal;
        u.m.ch[2].on = true;
        u.m.dec_on = true;
        u.m.dec_src = 2;
        u.m.pane = tables::PANE_EVENTS;
        for _ in 0..12 {
            u.step(0.1);
        }
        let messages = u.pn.rows.len();
        assert!(messages > 0, "the demo's UART line decodes");
        assert!(u.pn.rows.iter().all(|e| e.who == "B1 UART" && e.len >= 1));
        assert!(u.pane_h() > EMPTY_H, "the pane has room for its rows");
        let r = u.pane_rect();
        let head = u.events_head(r);
        assert!(!head.cycle && head.views.len() == 3, "a pane under the plot has room for the three views");
        // a tap on a row chooses it, a second one lets go of it
        let (_, rows) = u.events_bar().unwrap();
        u.m.run = RunState::Stop;
        u.events_tap(rows.x + 60, rows.y + 4);
        let chosen = u.ev_selected().expect("the first row is chosen");
        assert_eq!((chosen.dec, chosen.t0), (0, u.pn.rows[0].t));
        u.events_tap(rows.x + 60, rows.y + 4);
        assert!(u.ev_selected().is_none());
        let fields = head.views[1].0;
        u.events_tap(fields.x + 4, fields.y + 4);
        assert_eq!(u.m.ev_view, EvView::Fields);
        assert!(u.pn.rows.len() > messages, "a row per character");
        assert!(u.pn.rows.iter().all(|e| e.len == 1));
        // the demo line has no errors: Errors only leaves no row, and the head stays to switch back
        let errors = head.errors.unwrap();
        u.events_tap(errors.x + 4, errors.y + 4);
        assert!(u.m.ev_errors && u.events_count() == 0);
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
    }

    /// A row without a failed check says nothing in the last column, and a record without one has no such column: the
    /// text has its room. A small pane never has it (its text is red where a check failed).
    #[test]
    fn the_errors_column_is_there_for_errors_only() {
        assert_eq!(errors_text(0), "");
        assert_eq!((errors_text(1).as_str(), errors_text(3).as_str()), ("1 error", "3 errors"));
        assert!(!errors_column(NARROW_W + 200, 0) && !errors_column(SMALL_W, 0));
        assert!(errors_column(NARROW_W + 200, 1) && errors_column(SMALL_W, 2));
        assert!(!errors_column(SMALL_W - 1, 5));
    }

    /// What opened the table of decoder events over the plot opens the Events pane, and takes it away again.
    #[test]
    fn the_events_pane_is_shown_and_taken_away() {
        for layout in [Layout::Minimal, Layout::Studio] {
            let mut u = listing(layout);
            u.set_events_pane(false);
            assert!(!u.events_pane_on() && u.m.pane == 0, "{layout:?}");
            u.set_events_pane(true);
            assert!(u.events_pane_on() && u.table.kind.is_none(), "{layout:?}");
            // another pane is not taken away by switching the events off
            u.m.pane = tables::PANE_TREND;
            u.set_events_pane(false);
            assert_eq!(u.m.pane, tables::PANE_TREND, "{layout:?}");
        }
    }

    /// A small pane has one button that goes through the views, and every text it draws stays inside it.
    #[test]
    fn a_small_pane_keeps_its_head_and_its_rows_inside() {
        let u = Ui::new(Source::Demo { t: 0.0 });
        for w in [220, 300, 420, 700] {
            let r = Rect::new(100, 100, w, 200);
            let head = u.events_head(r);
            assert_eq!(head.cycle, w < 420, "{w} px");
            let limit = r.right() - 54;
            for (b, _) in &head.views {
                assert!(b.x >= r.x && b.right() <= limit && b.h >= workspace::TOUCH_MIN, "{w} px: {b:?}");
            }
            if let Some(b) = head.errors {
                assert!(b.right() <= limit && b.h >= workspace::TOUCH_MIN, "{w} px: {b:?}");
            }
        }
    }

    /// Default's Events pane is upstream's: the decoder events a row per field in one or two columns, with no head of
    /// views and no scroll bar, and a tap on it does nothing; Minimal lists the fork's rows.
    #[test]
    fn default_has_upstreams_events_pane() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.ch[2].on = true;
        u.m.dec_on = true;
        u.m.dec_src = 2;
        u.m.pane = tables::PANE_EVENTS;
        for _ in 0..12 {
            u.step(0.1);
        }
        assert!(!u.pn.events.is_empty() && u.pn.rows.is_empty(), "upstream's rows of fields");
        assert!(u.events_bar().is_none());
        assert!(u.pane_want() > 0 && u.pane_want() % 24 == 0, "rows of 24 pixels");
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        cv.au_start();
        u.render(&mut cv);
        let texts = cv.au.take().unwrap_or_default();
        assert!(texts.iter().any(|e| matches!(e, Au::Text { s, .. } if s == "D1 UART CH3")), "who, as upstream's");
        assert!(!texts.iter().any(|e| matches!(e, Au::Text { s, .. } if s == "Errors only")), "no head of views");
        let (delay, r) = (u.m.delay, u.pane_rect());
        u.m.run = RunState::Stop;
        u.tap(r.x + 60, r.y + 10);
        assert_eq!(u.m.delay, delay, "a tap on the list does nothing");
        let u = listing(Layout::Minimal);
        assert!(!u.pn.rows.is_empty() && u.events_bar().is_some());
    }

    /// A UART decoder on the demo's line with the Events pane open, after a few records.
    fn listing(layout: Layout) -> Ui {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = layout;
        u.m.ch[2].on = true;
        u.m.dec_on = true;
        u.m.dec_src = 2;
        u.m.pane = tables::PANE_EVENTS;
        for _ in 0..12 {
            u.step(0.1);
        }
        assert!(!u.pn.rows.is_empty(), "the demo's UART line decodes");
        u
    }

    /// A tap on a row of a running scope marks the row and leaves the scope running, with its horizontal position: in
    /// Studio it stopped the acquisition and moved the view. A stopped scope goes to the row.
    #[test]
    fn a_row_tap_leaves_a_running_scope_running() {
        for layout in [Layout::Minimal, Layout::Studio] {
            let mut u = listing(layout);
            let (_, rows) = u.events_bar().unwrap();
            let delay = u.m.delay;
            assert_eq!(u.m.run, RunState::Run);
            u.events_tap(rows.x + 60, rows.y + 4);
            assert_eq!((u.m.run, u.m.delay), (RunState::Run, delay), "{layout:?}");
            assert!(u.ev_selected().is_some(), "{layout:?}: the row is marked");
            u.events_tap(rows.x + 60, rows.y + 4);
            assert!(u.ev_selected().is_none(), "{layout:?}: a second tap lets go of it");
            u.m.run = RunState::Stop;
            u.m.delay = 1.0;
            u.events_tap(rows.x + 60, rows.y + 4);
            assert!(u.m.delay != 1.0 && u.ev_selected().is_some(), "{layout:?}: a stopped scope goes to the row");
        }
    }

    /// The columns are as wide for one part of the list as for another: their widths are those of the record.
    #[test]
    fn the_columns_stand_still_while_the_list_scrolls() {
        let mut u = listing(Layout::Minimal);
        u.m.ev_view = EvView::Fields;
        u.pn.rows = u.decode_rows();
        let f = u.fonts();
        let all = u.events_widths(&f.s);
        assert!(all.0 >= f.s.width("B1 UART"));
        // rows of another decoder with a longer name, and one row with hex bytes, anywhere in the record
        let mut far = u.pn.rows[0].clone();
        (far.dec, far.who, far.hex) = (1, "D2 Modbus RTU".to_string(), "4F".to_string());
        u.pn.rows.push(far);
        let (who, hex) = u.events_widths(&f.s);
        assert!(who >= f.s.width("D2 Modbus RTU") && hex);
    }

    /// The one button of a narrow pane opens the list of the views, and a row of the list chooses it.
    #[test]
    fn a_narrow_pane_lists_its_views() {
        // (a pane under the plot: its buttons have a row of their own, where the width of the pane is the limit; in
        // the strip of the docked panel it is the strip's)
        let mut u = listing(Layout::Minimal);
        let messages = u.pn.rows.len();
        let head = u.events_head(Rect::new(100, 100, 220, 200));
        assert!(head.cycle && head.views.len() == 1);
        assert!(u.workspace.popup.is_none());
        let b = head.views[0].0;
        assert!(u.events_head_tap(&head, b.x + 4, b.y + 4));
        assert_eq!(u.m.ev_view, EvView::Messages, "the tap changes nothing by itself");
        let popup = u.workspace.popup.as_ref().expect("the list of the views");
        let names: Vec<&str> = popup.rows.iter().map(|r| r.0.as_str()).collect();
        assert_eq!(names, ["Messages", "Fields", "Lines"]);
        let row = popup.row_rect(1);
        assert!(u.workspace_popup_tap(row.x + row.w / 2, row.y + row.h / 2));
        assert_eq!(u.m.ev_view, EvView::Fields);
        assert!(u.pn.rows.len() > messages && u.pn.ev_top == 0, "a row per character, from the top");
        assert!(u.workspace.popup.is_none());
    }
}
