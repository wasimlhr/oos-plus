// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What the decoders found, as rows: the Events pane under the plot and the event table over it. The rows are the
//! messages of the record (`oos_dsp::decode::bursts`), its single fields, or the lines of a character stream; the
//! pane's head chooses, and can leave only the rows with a failed check. A message has its time, its decoder, the
//! number of its fields, its text, for a character line the bytes in hex, and whether its checks passed. A tap on a
//! row moves the view to it.

use super::*;
use oos_dsp::decode::{Kind as DecKind, Packet};

/// Height of the pane's head (the view switch; as high as the close button) and of its column titles.
const HEAD: i32 = 40;
const TITLES: i32 = 20;
/// Height of a pane with its head and one line of text.
const EMPTY_H: i32 = HEAD + 24;
/// Below this width the pane has no columns for the decoder, the length and the hex bytes.
const NARROW_W: i32 = 560;

/// How the decoded fields are listed.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EvView {
    /// one row per message
    #[default]
    Messages,
    /// one row per field
    Fields,
    /// one row per line of a character stream (UART shown as ASCII); messages for every other decoder
    Lines,
}

impl EvView {
    pub(super) const ALL: [EvView; 3] = [EvView::Messages, EvView::Fields, EvView::Lines];

    pub(super) fn name(self) -> &'static str {
        match self {
            EvView::Messages => "Messages",
            EvView::Fields => "Fields",
            EvView::Lines => "Lines",
        }
    }
}

/// One row of decoder results.
#[derive(Clone, Debug, PartialEq)]
pub struct MsgRow {
    /// start of its first field (s from the trigger)
    pub t: f64,
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
fn rows_of(m: &Model, who: &str, p: &[Packet], view: EvView) -> Vec<MsgRow> {
    let ascii = decband::ascii(m);
    let hex = |x: &Packet| format!("{:02X}", x.value);
    if view == EvView::Fields {
        return p
            .iter()
            .map(|x| {
                let tag = packet_tag(m, x);
                let text = tag.map_or_else(|| packet_text(m, x), str::to_string);
                // beside a character its value; a byte without a character is shown as its value already
                let hex = if ascii && tag.is_none() && x.kind == DecKind::Data && text != hex(x) { hex(x) } else { String::new() };
                MsgRow { t: x.t0, who: who.to_string(), len: usize::from(tag.is_none()), text, hex, errors: usize::from(!x.ok) }
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
                t: pk[0].t0,
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
                Some(rows) => v.extend(rows.into_iter().map(|(t, text, ok)| MsgRow { t, who: who.clone(), len: 1, text, hex: String::new(), errors: usize::from(!ok) })),
                None => v.extend(rows_of(&m, &who, &p, self.pn.ev_view)),
            }
        }
        v.sort_by(|a, b| a.t.total_cmp(&b.t));
        v
    }

    /// The rows the Events pane lists: all, or the ones with a failed check.
    fn events_listed(&self) -> impl Iterator<Item = &MsgRow> {
        let only = self.pn.ev_errors;
        self.pn.rows.iter().filter(move |e| !only || e.errors > 0)
    }

    /// Number of rows the Events pane lists.
    pub(super) fn events_count(&self) -> usize {
        self.events_listed().count()
    }

    /// The buttons in the head of the Events pane: the three views, and Errors only.
    fn events_head(&self, r: Rect) -> ([Rect; 3], Rect) {
        let f = self.fonts();
        let mut x = r.x + 8;
        let segs = EvView::ALL.map(|v| {
            let b = Rect::new(x, r.y + 2, f.sb.width(v.name()) + 24, HEAD - 4);
            x += b.w;
            b
        });
        (segs, Rect::new(x + 10, r.y + 2, f.sb.width("Errors only") + 24, HEAD - 4))
    }

    /// Where the rows are, and how many fit.
    fn events_rows(r: Rect) -> (Rect, usize) {
        let fit = ((r.h - HEAD - TITLES) / tables::ROW_H).max(0);
        (Rect::new(r.x, r.y + HEAD + TITLES, r.w, fit * tables::ROW_H), fit as usize)
    }

    /// The scroll bar of the Events pane and its rows, while the pane shows them.
    pub(super) fn events_bar(&self) -> Option<(scroll::Bar, Rect)> {
        if self.pane_h() == 0 || self.m.pane != tables::PANE_EVENTS {
            return None;
        }
        let (rows, fit) = Self::events_rows(self.pane_rect());
        let track = Rect::new(rows.x + rows.w - scroll::BAR_W - 2, rows.y, scroll::BAR_W, rows.h);
        Some((scroll::Bar { track, total: self.events_count() as i32, shown: fit as i32, pos: self.pn.ev_top as i32 }, rows))
    }

    /// A tap on the Events pane: a view, Errors only, or a row (the view moves to it).
    pub(super) fn events_tap(&mut self, x: i32, y: i32) {
        let r = self.pane_rect();
        let (segs, errors) = self.events_head(r);
        if let Some(k) = segs.iter().position(|s| s.contains(x, y)) {
            self.pn.ev_view = EvView::ALL[k];
        } else if errors.contains(x, y) {
            self.pn.ev_errors = !self.pn.ev_errors;
        } else {
            let Some((bar, rows)) = self.events_bar() else { return };
            if !rows.contains(x, y) || (bar.needed() && bar.track.contains(x, y)) {
                return;
            }
            let i = bar.top() as usize + ((y - rows.y) / tables::ROW_H) as usize;
            let t = self.events_listed().nth(i).map(|e| e.t);
            if let Some(t) = t {
                self.go_to_time(t);
            }
            return;
        }
        // another list: from its top, at once
        self.pn.ev_top = 0;
        self.pn.rows = self.decode_rows();
    }

    pub(super) fn draw_pane_events(&self, cv: &mut Canvas, r: Rect) {
        let f = self.fonts();
        let (segs, errors) = self.events_head(r);
        for (b, v) in segs.into_iter().zip(EvView::ALL) {
            let on = v == self.pn.ev_view;
            btn(cv, b, if on { ACCENT } else { BTN }, if on { ACCENT } else { BTN_BORDER });
            cv.text_in(&f.sb, b, 1, v.name(), if on { DARK_TEXT } else { TEXT });
        }
        let only = self.pn.ev_errors;
        btn(cv, errors, if only { mix(BG, RED, 90) } else { BTN }, if only { RED } else { BTN_BORDER });
        cv.text_in(&f.sb, errors, 1, "Errors only", TEXT);
        // what the record holds, right of the buttons and left of the close button
        let total = self.pn.rows.len();
        let failed = self.pn.rows.iter().filter(|e| e.errors > 0).count();
        let x = errors.right() + 14;
        let room = r.right() - 54 - x;
        let rows_txt = format!("{} rows", total);
        let failed_txt = if failed == 1 { "1 with an error".to_string() } else { format!("{} with errors", failed) };
        if f.s.width(&rows_txt) <= room {
            cv.text_in(&f.s, Rect::new(x, r.y, f.s.width(&rows_txt), HEAD), 0, &rows_txt, DIM);
            let x = x + f.s.width(&rows_txt) + 14;
            if failed > 0 && x + f.s.width(&failed_txt) <= r.right() - 54 {
                cv.text_in(&f.s, Rect::new(x, r.y, f.s.width(&failed_txt), HEAD), 0, &failed_txt, RED);
            }
        }
        let (area, fit) = Self::events_rows(r);
        let n = self.events_count();
        if n == 0 || fit == 0 {
            let note = if total == 0 { "No decoder events in this record" } else { "No row with an error" };
            cv.text(&f.sb, r.x + 12, r.y + HEAD + 17, note, DIM);
            return;
        }
        let bar = scroll::Bar { track: Rect::new(area.x + area.w - scroll::BAR_W - 2, area.y, scroll::BAR_W, area.h), total: n as i32, shown: fit as i32, pos: self.pn.ev_top as i32 };
        let top = bar.top() as usize;
        let shown: Vec<&MsgRow> = self.events_listed().skip(top).take(fit).collect();
        // the columns: number, time, decoder, length, text, hex, status. A narrow pane keeps number, time, text, status
        let pad = 14;
        let wide = r.w >= NARROW_W;
        let hex_on = wide && shown.iter().any(|e| !e.hex.is_empty());
        let w_who = if wide { shown.iter().map(|e| f.s.width(&e.who)).max().unwrap_or(0).min(140) + pad } else { 0 };
        let w_len = if wide { f.s.width("Len").max(f.s.width("9999")) + pad } else { 0 };
        let w_status = f.s.width("999 errors") + pad;
        let widths = [f.s.width("9999") + pad, f.s.width("-888.888 ms") + pad, w_who, w_len];
        let end = area.right() - if bar.needed() { scroll::BAR_W + 4 } else { 8 };
        let free = (end - w_status - (r.x + 12) - widths.iter().sum::<i32>()).max(0);
        let w_text = if hex_on { free * 2 / 5 } else { free };
        let cols = [("#", widths[0]), ("Time", widths[1]), ("Source", w_who), ("Len", w_len), ("Text", w_text), ("Hex", free - w_text), ("Status", w_status)];
        let mut x = r.x + 12;
        for (title, w) in cols {
            if w > 0 {
                cell(cv, &f.s, (x, r.y + HEAD - 3), w, title, DIM);
            }
            x += w;
        }
        cv.hline(r.x + 8, end, area.y - 1, LINE);
        for (i, e) in shown.iter().enumerate() {
            let y = area.y + i as i32 * tables::ROW_H;
            if (top + i) % 2 == 1 {
                cv.fill(Rect::new(r.x + 4, y, end - r.x - 4, tables::ROW_H), ROW_ALT);
            }
            let status = match e.errors {
                0 => "OK".to_string(),
                1 => "1 error".to_string(),
                k => format!("{} errors", k),
            };
            let len = if e.len > 0 { e.len.to_string() } else { String::new() };
            let texts = [(top + i + 1).to_string(), si(e.t, "s"), e.who.clone(), len, e.text.clone(), e.hex.clone(), status];
            let inks = [DIM, LABEL, DIM, LABEL, if e.errors > 0 { RED } else { TEXT }, LABEL, if e.errors > 0 { RED } else { GREEN }];
            let mut x = r.x + 12;
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
        let msgs = rows_of(&m, "D1 UART", &p, EvView::Messages);
        assert_eq!(msgs.iter().map(|e| (e.t, e.len, e.text.as_str(), e.hex.as_str(), e.errors)).collect::<Vec<_>>(),
            [(0.0, 3, "Hi\\n", "48 69 0A", 1), (20.0, 2, "yo", "79 6F", 0)]);
        let fields = rows_of(&m, "D1 UART", &p, EvView::Fields);
        assert_eq!(fields.len(), 5);
        assert_eq!((fields[0].text.as_str(), fields[0].hex.as_str(), fields[1].errors), ("H", "48", 1));
        // the line feed is shown as its value, so it has no hex beside it
        assert_eq!((fields[2].text.as_str(), fields[2].hex.as_str()), ("0A", ""));
        let mut q = p.clone();
        q.push(byte(40.0, b'\n'));
        let lines = rows_of(&m, "D1 UART", &q, EvView::Lines);
        assert_eq!(lines.iter().map(|e| e.text.as_str()).collect::<Vec<_>>(), ["Hi\\n", "yo\\n"]);
        // a decoder that shows values has no hex column and its lines are its messages
        let mut hexm = uart();
        hexm.dec_ascii = false;
        let rows = rows_of(&hexm, "D1 UART", &p, EvView::Lines);
        assert_eq!(rows.iter().map(|e| (e.text.as_str(), e.hex.as_str())).collect::<Vec<_>>(), [("48 69 0A", ""), ("79 6F", "")]);
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
        let (segs, errors) = u.events_head(r);
        u.events_tap(segs[1].x + 4, segs[1].y + 4);
        assert_eq!(u.pn.ev_view, EvView::Fields);
        assert!(u.pn.rows.len() > messages, "a row per character");
        assert!(u.pn.rows.iter().all(|e| e.len == 1));
        // the demo line has no errors: Errors only leaves no row, and the head stays to switch back
        u.events_tap(errors.x + 4, errors.y + 4);
        assert!(u.pn.ev_errors && u.events_count() == 0);
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
    }
}
