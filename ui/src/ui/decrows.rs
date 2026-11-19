// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What the decoders found, as rows: the Events pane under the plot and the event table over it. The rows are the
//! messages of the record (`oos_dsp::decode::bursts`), its single fields, or the lines of a character stream; the
//! pane's head chooses, and can leave only the rows with a failed check. A message has its time, its decoder, the
//! number of its fields, its text, for a character line the bytes in hex, and whether its checks passed. The pane
//! fits its size: a narrow one keeps number, time, text and status, a small one time and text, and its head has one
//! button that goes through the views. A tap on a row moves the view to it and chooses it: the row, its box in the
//! band and its time span on the plot are marked, and a tap on a box of the band chooses its row.

use super::*;
use oos_dsp::decode::{Kind as DecKind, Packet};

/// Height of the pane's head (the view switch; as high as the close button) and of its column titles.
const HEAD: i32 = 40;
const TITLES: i32 = 20;
/// Height of a pane with its head and one line of text.
const EMPTY_H: i32 = HEAD + 24;
/// Below this width the pane has no columns for the decoder, the length and the hex bytes.
const NARROW_W: i32 = 560;
/// Below this width it lists the time and the text alone.
const SMALL_W: i32 = 360;

/// One row of decoder results.
#[derive(Clone, Debug, PartialEq)]
pub struct MsgRow {
    /// its decoder (0 = D1)
    pub dec: usize,
    /// start of its first field and end of its last one (s from the trigger)
    pub t: f64,
    pub t1: f64,
    /// its decoder, as the band on the plot names it ("D1 UART")
    pub who: String,
    /// fields with a value in it
    pub len: usize,
    pub text: String,
    /// the bytes in hex, where the text shows characters; else empty
    pub hex: String,
    /// fields that failed their check
    pub errors: usize,
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
        return p
            .iter()
            .map(|x| {
                let tag = packet_tag(m, x);
                let text = tag.map_or_else(|| packet_text(m, x), str::to_string);
                // beside a character its value; a byte without a character is shown as its value already
                let character = ascii && tag.is_none() && x.kind == DecKind::Data && text != hex(x);
                let hex = if character { hex(x) } else { String::new() };
                let (len, errors) = (usize::from(tag.is_none()), usize::from(!x.ok));
                MsgRow { dec, t: x.t0, t1: x.t1, who: who.to_string(), len, text, hex, errors }
            })
            .collect();
    }
    let groups = if view == EvView::Lines && ascii { lines(p) } else { oos_dsp::decode::bursts(p) };
    groups
        .into_iter()
        .map(|g| {
            let pk = &p[g];
            let values = || pk.iter().filter(|x| packet_tag(m, x).is_none());
            MsgRow {
                dec,
                t: pk[0].t0,
                t1: pk.iter().map(|x| x.t1).fold(pk[0].t1, f64::max),
                who: who.to_string(),
                len: values().count(),
                text: decband::message_text(m, pk),
                hex: if ascii { values().map(hex).collect::<Vec<_>>().join(" ") } else { String::new() },
                errors: pk.iter().filter(|x| !x.ok).count(),
            }
        })
        .collect()
}

/// Height the Events pane wants for `rows` rows.
pub(super) fn want_h(rows: usize) -> i32 {
    if rows == 0 { EMPTY_H } else { HEAD + TITLES + rows as i32 * tables::ROW_H }
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
            let who = format!("D{} {}", k + 1, m.dec_proto.name());
            match m.dec_proto.module().and_then(|x| x.events(&p, &m.dec_opts())) {
                Some(rows) => v.extend(rows.into_iter().map(|(t, text, ok)| {
                    let (hex, errors) = (String::new(), usize::from(!ok));
                    MsgRow { dec: k, t, t1: t, who: who.clone(), len: 1, text, hex, errors }
                })),
                None => v.extend(rows_of(&m, k, &who, &p, self.m.ev_view)),
            }
        }
        v.sort_by(|a, b| a.t.total_cmp(&b.t));
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
        (self.wfm_total, self.hist_pos)
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
    /// button that goes through the views, and Errors while there is room.
    fn events_head(&self, r: Rect) -> EvHead {
        let f = self.fonts();
        let h = HEAD - 4;
        // a pane under the plot has its close button at the right end of the head; a docked one has it in its title
        let limit = r.right() - if self.dock_on() { 8 } else { 54 };
        let seg = |v: EvView| f.sb.width(v.name()) + 24;
        let all = f.sb.width("Errors only") + 24;
        let mut x = r.x + 8;
        if x + EV_VIEWS.iter().map(|&v| seg(v)).sum::<i32>() + 10 + all <= limit {
            let views = EV_VIEWS
                .iter()
                .map(|&v| {
                    let b = Rect::new(x, r.y + 2, seg(v), h);
                    x += b.w;
                    (b, v)
                })
                .collect();
            let errors = Rect::new(x + 10, r.y + 2, all, h);
            return EvHead { views, cycle: false, errors: Some(errors), right: errors.right() };
        }
        // room for the longest name and the mark that there is more
        let w = EV_VIEWS.iter().map(|&v| seg(v)).max().unwrap_or(0) + 14;
        let one = Rect::new(x, r.y + 2, w.min((limit - x).max(44)), h);
        let e = Rect::new(one.right() + 6, r.y + 2, f.sb.width("Errors") + 20, h);
        let errors = (e.right() <= limit).then_some(e);
        let right = errors.map_or(one.right(), |e| e.right());
        EvHead { views: vec![(one, self.m.ev_view)], cycle: true, errors, right }
    }

    /// Where the rows are, and how many fit.
    fn events_rows(r: Rect) -> (Rect, usize) {
        let fit = ((r.h - HEAD - TITLES) / tables::ROW_H).max(0);
        (Rect::new(r.x, r.y + HEAD + TITLES, r.w, fit * tables::ROW_H), fit as usize)
    }

    /// The scroll bar beside the rows `area`, of which `fit` are on the screen.
    fn events_scroll(&self, area: Rect, fit: usize) -> scroll::Bar {
        let track = Rect::new(area.x + area.w - scroll::BAR_W - 2, area.y, scroll::BAR_W, area.h);
        scroll::Bar { track, total: self.events_count() as i32, shown: fit as i32, pos: self.pn.ev_top as i32 }
    }

    /// The scroll bar of the Events pane and its rows, while the pane shows them.
    pub(super) fn events_bar(&self) -> Option<(scroll::Bar, Rect)> {
        if self.pane_h() == 0 || self.m.pane != tables::PANE_EVENTS {
            return None;
        }
        let (rows, fit) = Self::events_rows(self.pane_rect());
        Some((self.events_scroll(rows, fit), rows))
    }

    /// A tap on the Events pane: a view, Errors only, or a row. The view moves to the row and it is the chosen one;
    /// a tap on the chosen row lets go of it.
    pub(super) fn events_tap(&mut self, x: i32, y: i32) {
        let head = self.events_head(self.pane_rect());
        if let Some(&(_, v)) = head.views.iter().find(|(b, _)| b.contains(x, y)) {
            let at = EV_VIEWS.iter().position(|&w| w == v).unwrap_or(0);
            self.m.ev_view = if head.cycle { EV_VIEWS[(at + 1) % EV_VIEWS.len()] } else { v };
        } else if head.errors.is_some_and(|b| b.contains(x, y)) {
            self.m.ev_errors = !self.m.ev_errors;
        } else {
            let Some((bar, rows)) = self.events_bar() else { return };
            if !rows.contains(x, y) || (bar.needed() && bar.track.contains(x, y)) {
                return;
            }
            let i = bar.top() as usize + ((y - rows.y) / tables::ROW_H) as usize;
            let row = self.events_listed().nth(i).map(|e| (e.dec, e.t, e.t1, self.ev_is_selected(e)));
            match row {
                Some((_, _, _, true)) => self.pn.ev_sel = None,
                Some((dec, t0, t1, false)) => {
                    // the jump may stop the scope: the row is of the record that is shown after it
                    self.go_to_time(t0);
                    self.pn.ev_sel = Some(EvSel { dec, t0, t1, rec: self.ev_record() });
                }
                None => {}
            }
            return;
        }
        // another list: from its top, at once; the choice is part of the set-up
        self.pn.ev_top = 0;
        self.pn.rows = self.decode_rows();
        self.mark_changed();
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

    pub(super) fn draw_pane_events(&self, cv: &mut Canvas, r: Rect) {
        let f = self.fonts();
        let head = self.events_head(r);
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
        let limit = r.right() - if self.dock_on() { 8 } else { 54 };
        let x = head.right + 14;
        let rows_txt = if total == 1 { "1 row".to_string() } else { format!("{} rows", total) };
        let failed_txt = if failed == 1 { "1 with an error".to_string() } else { format!("{} with errors", failed) };
        if x + f.s.width(&rows_txt) <= limit {
            cv.text_in(&f.s, Rect::new(x, r.y, f.s.width(&rows_txt), HEAD), 0, &rows_txt, DIM);
            let x = x + f.s.width(&rows_txt) + 14;
            if failed > 0 && x + f.s.width(&failed_txt) <= limit {
                cv.text_in(&f.s, Rect::new(x, r.y, f.s.width(&failed_txt), HEAD), 0, &failed_txt, RED);
            }
        }
        let (area, fit) = Self::events_rows(r);
        let n = self.events_count();
        if n == 0 || fit == 0 {
            let note = if total == 0 { "No decoder events in this record" } else { "No row with an error" };
            let note = startup::fit_text(&f.sb, note, r.w - 20);
            cv.text(&f.sb, r.x + 12, r.y + HEAD + 17, &note, DIM);
            return;
        }
        let bar = self.events_scroll(area, fit);
        let top = bar.top() as usize;
        let shown: Vec<&MsgRow> = self.events_listed().skip(top).take(fit).collect();
        // the columns: number, time, decoder, length, text, hex, status. A narrow pane keeps number, time, text and
        // status; a small one the time and the text, which is red where a check failed
        let pad = 14;
        let (wide, small) = (r.w >= NARROW_W, r.w < SMALL_W);
        let hex_on = wide && shown.iter().any(|e| !e.hex.is_empty());
        let w_number = if small { 0 } else { f.s.width("9999") + pad };
        let w_time = shown.iter().map(|e| f.s.width(&si(e.t, "s"))).max().unwrap_or(0) + pad;
        let w_who = if wide { shown.iter().map(|e| f.s.width(&e.who)).max().unwrap_or(0).min(140) + pad } else { 0 };
        let w_len = if wide { f.s.width("Len").max(f.s.width("9999")) + pad } else { 0 };
        let w_status = if small { 0 } else { f.s.width("999 errors") + pad };
        let left = r.x + if small { 8 } else { 12 };
        // the bar is drawn narrow in the middle of its touch area: a small pane lets the text come close to it
        let end = area.right() - if !bar.needed() { 8 } else if small { 28 } else { scroll::BAR_W + 4 };
        let free = (end - w_status - left - w_number - w_time - w_who - w_len).max(0);
        let w_text = if hex_on { free * 2 / 5 } else { free };
        let cols = [
            ("#", w_number),
            ("Time", w_time),
            ("Source", w_who),
            ("Len", w_len),
            ("Text", w_text),
            ("Hex", free - w_text),
            ("Status", w_status),
        ];
        let mut x = left;
        for (title, w) in cols {
            if w > 0 {
                cell(cv, &f.s, (x, r.y + HEAD - 3), w, title, DIM);
            }
            x += w;
        }
        cv.hline(r.x + 8, end, area.y - 1, LINE);
        for (i, e) in shown.iter().enumerate() {
            let y = area.y + i as i32 * tables::ROW_H;
            let line = Rect::new(r.x + 4, y, end - r.x - 4, tables::ROW_H);
            if self.ev_is_selected(e) {
                // the chosen row: its box in the band and its span on the plot are marked like it
                cv.fill(line, mix(BG, ACCENT, 46));
                cv.round_outline(line, 3, ACCENT);
            } else if (top + i) % 2 == 1 {
                cv.fill(line, ROW_ALT);
            }
            let status = match e.errors {
                0 => "OK".to_string(),
                1 => "1 error".to_string(),
                k => format!("{} errors", k),
            };
            let len = if e.len > 0 { e.len.to_string() } else { String::new() };
            let number = (top + i + 1).to_string();
            let texts = [number, si(e.t, "s"), e.who.clone(), len, e.text.clone(), e.hex.clone(), status];
            let (ink, mark) = if e.errors > 0 { (RED, RED) } else { (TEXT, GREEN) };
            let inks = [DIM, LABEL, DIM, LABEL, ink, LABEL, mark];
            let mut x = left;
            for (k, (_, w)) in cols.iter().enumerate() {
                if *w > 0 {
                    cell(cv, if k == 4 { &f.sb } else { &f.s }, (x, y), *w, &texts[k], inks[k]);
                }
                x += *w;
            }
        }
        bar.draw(cv);
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
        u.last[0] = Some(Record { t0: 0.0, dt, ch: ch.clone(), triggered, partial, raw: None, la: None, pairs });
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
        assert_eq!((u.item_label(Item::DecOpt(0)), u.item_label(Item::DecOpt(1))), ("Bit order", "Data"));
    }

    /// The Events pane lists the demo's UART line as messages, switches to fields by a tap on its head, and a tap on
    /// a row needs no more than the row.
    #[test]
    fn the_events_pane_lists_messages_and_switches_by_touch() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.ch[2].on = true;
        u.m.dec_on = true;
        u.m.dec_src = 2;
        u.m.pane = tables::PANE_EVENTS;
        for _ in 0..12 {
            u.step(0.1);
        }
        let messages = u.pn.rows.len();
        assert!(messages > 0, "the demo's UART line decodes");
        assert!(u.pn.rows.iter().all(|e| e.who == "D1 UART" && e.len >= 1));
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
                assert!(b.x >= r.x && b.right() <= limit && b.h >= 36, "{w} px: {b:?}");
            }
            if let Some(b) = head.errors {
                assert!(b.right() <= limit, "{w} px: {b:?}");
            }
        }
    }
}
