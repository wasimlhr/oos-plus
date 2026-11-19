// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! A decoder's band on the plot: its name in a pill of the source's colour, and what it decoded along the time axis.
//! How much the band says follows the time base. A field with room for its text has a box of its own. Fields too
//! narrow for that share one box per burst, which holds as much of their text as fits and "..." for the rest, so a
//! slow time base shows the messages and not an empty band. A burst too small for a box stays a mark: where the line
//! is busy is seen at every time base. At a time base that shows single bits, a protocol that reports them
//! (`Protocol::bits`) has a row of bits above its boxes, and a mark on the threshold line where each was sampled.

use super::*;
use oos_dsp::decode::{Bit, BitKind, Kind as DecKind, Packet};

/// Height of a box in a row under the LA lanes (the row is `LA_DEC` tall).
const DEC_ROW_BOX: i32 = 20;
/// Height the bit row adds to a band: its cells and the room to the boxes under them.
const DEC_BITS: i32 = 18;
/// The bit row is there when a bit is at least this wide (px): its value can be read.
const BIT_MIN: f64 = 12.0;
/// The fill of a field with an error: a dark red, so the white text on it stays readable.
const ERR_FILL: Rgb = rgb(0x70, 0x18, 0x18);
/// Room a box keeps around its text, both ends together.
const BOX_PAD: i32 = 8;
/// A field narrower than this is never asked for its text: no text fits.
const OWN_MIN: i32 = 10;
/// Fields this close (px) are one burst, whatever their width.
const RUN_GAP: i32 = 3;
/// A tag (start, stop ...) this close (px) to a burst is part of it.
const TAG_PAD: i32 = 8;

/// A decoded field as the band sees it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Span {
    /// its place in the packets
    pub i: usize,
    pub x0: i32,
    pub x1: i32,
    /// an event without a value (start, stop, acknowledge ...), drawn as a tag at `x0`
    pub tag: bool,
    pub ok: bool,
}

/// What the band draws at one place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Cell {
    /// field `i` in a box of its own
    One { i: usize, x0: i32, x1: i32 },
    /// the fields `i0..i1`, too narrow for their texts, in one box; `bad`: one of them failed its check
    Run { i0: usize, i1: usize, x0: i32, x1: i32, bad: bool },
    /// the tag of event `i`
    Tag { i: usize, x: i32 },
}

/// A burst that is still growing: the fields `i0..i1` from `x0` to `x1`.
struct Open {
    i0: usize,
    i1: usize,
    x0: i32,
    x1: i32,
    bad: bool,
}

/// The burst as a cell. A box starts right of `end`, the end of the box before it (at first the band's name).
fn close(run: &mut Option<Open>, out: &mut Vec<Cell>, end: &mut i32) {
    if let Some(r) = run.take() {
        let x0 = r.x0.max(*end + 1);
        if r.x1 >= x0 {
            out.push(Cell::Run { i0: r.i0, i1: r.i1, x0, x1: r.x1, bad: r.bad });
            *end = r.x1;
        }
    }
}

/// The cells of a band from the fields on the screen, in their order. `need(i)` is the width field `i` needs for its
/// own text; `left` is where the band's name ends.
pub(super) fn cells(spans: impl Iterator<Item = Span>, need: impl Fn(usize) -> i32, left: i32) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    let mut run: Option<Open> = None;
    let mut end = left;
    for s in spans {
        if s.tag {
            // a tag at the end of a burst is part of it; one further away ends the burst
            match run.as_mut().filter(|r| s.x0 <= r.x1 + TAG_PAD) {
                Some(r) => {
                    r.i1 = s.i + 1;
                    r.bad |= !s.ok;
                }
                None => {
                    close(&mut run, &mut out, &mut end);
                    out.push(Cell::Tag { i: s.i, x: s.x0 });
                }
            }
            continue;
        }
        let w = s.x1 - s.x0;
        if w >= OWN_MIN && need(s.i) <= w {
            close(&mut run, &mut out, &mut end);
            out.push(Cell::One { i: s.i, x0: s.x0.max(end + 1), x1: s.x1 });
            end = s.x1;
            continue;
        }
        // the next field of a burst: no further away than it is wide
        match run.as_mut().filter(|r| s.x0 <= r.x1 + RUN_GAP.max(w)) {
            Some(r) => {
                r.i1 = s.i + 1;
                r.x1 = r.x1.max(s.x1);
                r.bad |= !s.ok;
            }
            None => {
                close(&mut run, &mut out, &mut end);
                // the tags right before it (a start condition) open the burst
                let mut i0 = s.i;
                while let Some(&Cell::Tag { i, x }) = out.last() {
                    if i + 1 != i0 || x < s.x0 - TAG_PAD {
                        break;
                    }
                    i0 = i;
                    out.pop();
                }
                run = Some(Open { i0, i1: s.i + 1, x0: s.x0, x1: s.x1, bad: !s.ok });
            }
        }
    }
    close(&mut run, &mut out, &mut end);
    // a tag under a burst is said by the burst's text
    let runs: Vec<(i32, i32)> = out
        .iter()
        .filter_map(|c| match *c {
            Cell::Run { x0, x1, .. } => Some((x0, x1)),
            _ => None,
        })
        .collect();
    out.retain(|c| match *c {
        Cell::Tag { x, .. } => !runs.iter().any(|&(a, b)| x >= a - TAG_PAD && x <= b + TAG_PAD),
        _ => true,
    });
    out
}

/// Whether the decoder shows characters (UART with Format ASCII).
fn ascii(m: &Model) -> bool {
    m.dec_proto == Proto::UART && m.dec_ascii
}

/// One field in the text of a burst. A character line reads as text: control characters as `\r`, `\n`, `\t`, any
/// other byte without a character as `<1B>`.
fn run_piece(m: &Model, p: &Packet) -> String {
    if let Some(tag) = packet_tag(m, p) {
        return tag.to_string();
    }
    if ascii(m) && p.kind == DecKind::Data {
        return match p.value {
            0x0d => "\\r".into(),
            0x0a => "\\n".into(),
            0x09 => "\\t".into(),
            v @ 0x20..=0x7e => (v as u8 as char).to_string(),
            v => format!("<{:02X}>", v),
        };
    }
    packet_text(m, p)
}

/// The text of a burst: its fields in order, as many as fit into `room` pixels, then "...". Characters stand side by
/// side, other values with a space between them. Empty when not even the first field fits.
pub(super) fn run_text(m: &Model, pk: &[Packet], f: &Font, room: i32) -> String {
    let dots = f.width("...");
    let glue = if ascii(m) { "" } else { " " };
    let mut s = String::new();
    let mut w = 0;
    for (k, p) in pk.iter().enumerate() {
        let piece = run_piece(m, p);
        let pw = f.width(&piece) + if s.is_empty() { 0 } else { f.width(glue) };
        // what is left out is said by the dots, which need their room
        let rest = if k + 1 == pk.len() { 0 } else { dots };
        if w + pw + rest > room {
            return if s.is_empty() { s } else { s + "..." };
        }
        if !s.is_empty() {
            s.push_str(glue);
        }
        s.push_str(&piece);
        w += pw;
    }
    s
}

/// The hex value beside a character, for a box with room for both.
fn second_text(m: &Model, p: &Packet) -> Option<String> {
    (ascii(m) && p.kind == DecKind::Data && (0x20..0x7f).contains(&p.value)).then(|| format!("{:02X}", p.value))
}

/// A field's box from `x0` to `x1` with pointed ends, or a plain mark where there is no room for them. False when
/// nothing can be written into it.
fn field_box(cv: &mut Canvas, x0: i32, x1: i32, y: i32, h: i32, edge: Rgb, fill: Rgb) -> bool {
    if x1 < x0 {
        return false;
    }
    if x1 - x0 < 6 {
        cv.fill(Rect::new(x0, y + 2, (x1 - x0).max(2), h - 4), edge);
        return false;
    }
    cv.fill(Rect::new(x0 + 3, y, x1 - x0 - 6, h), fill);
    cv.line(x0, y + h / 2, x0 + 3, y, edge);
    cv.line(x0, y + h / 2, x0 + 3, y + h - 1, edge);
    cv.line(x1, y + h / 2, x1 - 3, y, edge);
    cv.line(x1, y + h / 2, x1 - 3, y + h - 1, edge);
    cv.hline(x0 + 3, x1 - 3, y, edge);
    cv.hline(x0 + 3, x1 - 3, y + h - 1, edge);
    true
}

/// What a bit's cell says: in full, and where there is no room for that.
fn bit_label(b: &Bit) -> (String, &'static str) {
    let v = match b.level {
        Some(true) => "1",
        Some(false) => "0",
        None => "?",
    };
    match b.kind {
        BitKind::Start => ("Start".into(), "S"),
        BitKind::Data => (v.into(), v),
        BitKind::Parity => (format!("Par {}", v), "P"),
        // a stop bit is 1; what a failed one was read as is said
        BitKind::Stop if b.ok => ("Stop".into(), "T"),
        BitKind::Stop => (format!("Stop {}", v), "T"),
    }
}

/// Where a band's bit row is drawn.
struct BitRow<'a> {
    /// time (as the packets have it) to pixel
    xt: &'a dyn Fn(f64) -> i32,
    /// from the end of the band's name to the right edge of the plot
    left: i32,
    right: i32,
    /// top of the cells
    y: i32,
    /// the source's colour
    col: Rgb,
    /// the threshold line on the source trace, where the sample marks go
    thr_y: Option<i32>,
}

impl Ui {
    /// Whether the band of the decoder `m` is the view of has its bit row: the protocol reports the bits of its
    /// fields, and at this time base one is wide enough to be read.
    pub(super) fn dec_bit_row(&self, m: &Model) -> bool {
        let Some(p) = m.dec_proto.module().filter(|p| p.bit_row()) else { return false };
        let (tdiv, _) = self.view();
        let bit = p.bit_time_sh(&m.dec_opts(), &crate::protocols::Shared::of(m));
        tdiv > 0.0 && bit / tdiv as f64 * (self.area.w as f64 / HDIV as f64) >= BIT_MIN
    }

    /// Height the band of that decoder takes of the plot.
    pub(super) fn dec_band_h(&self, m: &Model) -> i32 {
        DEC_BAND + if self.dec_bit_row(m) { DEC_BITS } else { 0 }
    }

    /// The bit row of a band: the bits of the fields on the screen in cells, and a mark on the threshold line where
    /// each bit was sampled.
    fn draw_bit_row(&self, cv: &mut Canvas, m: &Model, packets: &[Packet], at: &BitRow) {
        let Some(module) = m.dec_proto.module() else { return };
        let f = self.fonts();
        let (opt, sh) = (m.dec_opts(), crate::protocols::Shared::of(m));
        let xt = at.xt;
        let bh = DEC_BITS - 2;
        for p in packets {
            if xt(p.t1) < at.left || xt(p.t0) > at.right {
                continue;
            }
            let Some(bits) = module.bits(p, &opt, &sh) else { continue };
            for b in &bits {
                if let Some(ty) = at.thr_y {
                    let xm = xt((b.t0 + b.t1) / 2.0);
                    if xm >= at.left && xm <= at.right {
                        cv.vline(xm, ty - 4, ty + 4, ACCENT);
                    }
                }
                // right of the band's name
                let (x0, x1) = (xt(b.t0).max(at.left), xt(b.t1));
                if x1 - x0 < 4 {
                    continue;
                }
                let cell = Rect::new(x0, at.y, x1 - x0, bh);
                // start, parity and stop stand apart from the data; a failed bit is red
                let framing = b.kind != BitKind::Data;
                if !b.ok {
                    cv.fill(cell, ERR_FILL);
                } else if framing {
                    cv.fill(cell, scale(at.col, 40));
                }
                let edge = if b.ok { LINE } else { RED };
                cv.hline(x0, x1, at.y, edge);
                cv.hline(x0, x1, at.y + bh - 1, edge);
                cv.vline(x0, at.y, at.y + bh - 1, edge);
                cv.vline(x1, at.y, at.y + bh - 1, edge);
                let (long, short) = bit_label(b);
                let room = x1 - x0 - 4;
                let txt = if f.s.width(&long) <= room { long.as_str() } else { short };
                if f.s.width(txt) <= room {
                    cv.text_in(&f.s, cell, 1, txt, if framing && b.ok { DIM } else { TEXT });
                }
            }
        }
    }

    /// One decoder's band at height y; row = its place under the lanes.
    pub(super) fn draw_decoder(&self, cv: &mut Canvas, m: &Model, k: usize, y: i32, row: Option<Rect>) {
        let f = self.fonts();
        let col = if m.dec_on_la() { la::line_color(m.dec_la[0].unwrap_or(0)) }
            else { decode_wave(m.dec_src).map_or(DIM, |source| self.wave_color(source)) };
        // rows under the lanes span all windows, like the lanes
        let a = if row.is_some() { self.lane_area() } else { self.area };
        let h = if row.is_some() { DEC_ROW_BOX } else { DEC_BOX };
        cv.set_clip(row.unwrap_or(a));
        // the bit row stands above the boxes
        let bit_row = row.is_none() && self.dec_bit_row(m);
        if row.is_none() {
            let top = if bit_row { y - DEC_BITS } else { y };
            cv.blend(Rect::new(a.x, top - 3, a.w, y + h + 3 - (top - 3)), BG, 200);
        }
        // the name in the source's colour, which says the source. In a plot it stands right of the channel markers
        // at the left edge (Studio: of the lane badges), which are drawn after the band and would cover it
        let name = format!("D{} {}", k + 1, m.dec_proto.name());
        let px = a.x + if row.is_some() { 4 } else if self.m.layout == Layout::Studio { 50 } else { MARK_W + 2 };
        let pill = Rect::new(px, y + 2, f.sb.width(&name) + 12, h - 4);
        cv.round(pill, 5, col, 255);
        cv.text_in(&f.sb, pill, 1, &name, DARK_TEXT);
        let left = pill.x + pill.w + 6;
        let right = a.x + a.w;
        let packets = match self.decode_packets_of(m) {
            Ok(p) => p,
            Err(e) => {
                cv.text_in(&f.s, Rect::new(left + 2, y, f.s.width(e), h), 0, e, DIM);
                cv.no_clip();
                return;
            }
        };
        if packets.is_empty() {
            // an empty band says so: too few samples a bit, a wrong baud rate and a wrong threshold all look like it
            let note = "Nothing decoded";
            cv.text_in(&f.s, Rect::new(left + 2, y, f.s.width(note), h), 0, note, DIM);
        }
        let (tdiv, delay) = self.view();
        let ppx = a.w as f64 / HDIV as f64;
        // kept near the screen: a field that ends far outside it must not overflow the sums below
        let xt = |t: f64| {
            let x = a.x as f64 + a.w as f64 / 2.0 + (t - delay as f64) / tdiv as f64 * ppx;
            (x.round() as i32).clamp(a.x - 64, right + 64)
        };
        let spans = packets.iter().enumerate().filter_map(|(i, p)| {
            let (x0, x1) = (xt(p.t0), xt(p.t1));
            (x1 >= left && x0 <= right).then(|| Span { i, x0, x1, tag: packet_tag(m, p).is_some(), ok: p.ok })
        });
        let mut tag_end = left;
        for c in cells(spans, |i| f.sb.width(&packet_text(m, &packets[i])) + BOX_PAD, left) {
            match c {
                Cell::Tag { i, x } => {
                    let p = &packets[i];
                    let Some(tag) = packet_tag(m, p) else { continue };
                    let tw = f.s.width(tag);
                    let r = Rect::new(x - tw / 2 - 3, y + 3, tw + 6, h - 6);
                    // one tag at a place: the next one that would cover it, or the name, is left out
                    if r.x < tag_end {
                        continue;
                    }
                    let event = matches!(p.kind, DecKind::Field(l) if l.starts_with('!'));
                    let bad = p.kind == DecKind::Stop || ((p.kind == DecKind::Ack || event) && !p.ok);
                    cv.round(r, 3, if bad { mix(BG, RED, 140) } else { mix(BG, GREEN, 140) }, 255);
                    cv.text_in(&f.s, r, 1, tag, TEXT);
                    tag_end = r.x + r.w + 1;
                }
                Cell::One { i, x0, x1 } => {
                    let p = &packets[i];
                    let head = matches!(
                        p.kind,
                        DecKind::Addr { .. } | DecKind::Id { .. } | DecKind::Word { cmd: true } | DecKind::Field(_)
                    );
                    let edge = if head { ACCENT } else { col };
                    if !field_box(cv, x0, x1, y, h, edge, if p.ok { scale(edge, 70) } else { ERR_FILL }) {
                        continue;
                    }
                    let txt = packet_text(m, p);
                    let (w, tw) = (x1 - x0, f.sb.width(&txt));
                    if tw + BOX_PAD > w {
                        continue;
                    }
                    // a character with its hex value where both fit
                    match second_text(m, p).filter(|s| tw + 6 + f.s.width(s) + BOX_PAD <= w) {
                        Some(s) => {
                            let sw = f.s.width(&s);
                            let tx = x0 + (w - tw - 6 - sw) / 2;
                            cv.text_in(&f.sb, Rect::new(tx, y, tw, h), 0, &txt, TEXT);
                            cv.text_in(&f.s, Rect::new(tx + tw + 6, y, sw, h), 0, &s, LABEL);
                        }
                        None => cv.text_in(&f.sb, Rect::new(x0, y, w, h), 1, &txt, TEXT),
                    }
                }
                Cell::Run { i0, i1, x0, x1, bad } => {
                    if !field_box(cv, x0, x1, y, h, if bad { RED } else { col }, scale(col, 70)) {
                        continue;
                    }
                    // from the left end of the box, which is on the screen: the text is what the burst begins with
                    let room = x1.min(right) - x0 - BOX_PAD - 2;
                    let txt = run_text(m, &packets[i0..i1], &f.sb, room);
                    if !txt.is_empty() {
                        cv.text_in(&f.sb, Rect::new(x0 + 5, y, room, h), 0, &txt, TEXT);
                    }
                }
            }
        }
        if bit_row {
            let source = if m.dec_on_la() { None } else { decode_wave(m.dec_src) };
            let thr_y = source.map(|source| self.y_of_wave(source, m.dec_thr));
            self.draw_bit_row(cv, m, &packets, &BitRow { xt: &xt, left, right, y: y - DEC_BITS, col, thr_y });
        }
        // threshold line on the source trace (LA lines have no analog level)
        if !m.dec_on_la() && m.dec_proto != Proto::CPHY {
            let Some(source) = decode_wave(m.dec_src) else { cv.no_clip(); return; };
            let ty = self.y_of_wave(source, m.dec_thr);
            cv.dashed_hline(a.x + 26, a.x + a.w - 26, ty, 2, 6, scale(col, 160));
            if m.dec_proto.module().is_some_and(|p| p.key() == "Arinc429") {
                // the LO level of ARINC 429
                let ty = self.y_of_wave(source, -m.dec_thr);
                cv.dashed_hline(a.x + 26, a.x + a.w - 26, ty, 2, 6, scale(col, 160));
            }
        }
        cv.no_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(i: usize, x0: i32, x1: i32) -> Span {
        Span { i, x0, x1, tag: false, ok: true }
    }

    fn byte(value: u32) -> Packet {
        Packet { t0: 0.0, t1: 1.0, kind: DecKind::Data, value, value2: None, ok: true }
    }

    /// Fields with room for their text keep a box each; narrow ones that touch are one burst, and a gap wider than
    /// they are starts the next burst.
    #[test]
    fn narrow_fields_share_a_box_per_burst() {
        let wide = [span(0, 100, 140), span(1, 140, 180)];
        let boxes = cells(wide.into_iter(), |_| 20, 50);
        assert_eq!(boxes, [Cell::One { i: 0, x0: 100, x1: 140 }, Cell::One { i: 1, x0: 141, x1: 180 }]);
        // ten fields of 2 px from 100, four more from 200
        let at = |k: usize, x: i32| span(k, x + 2 * k as i32, x + 2 + 2 * k as i32);
        let narrow = (0..10).map(|k| at(k, 100)).chain((10..14).map(|k| at(k, 180)));
        let first = Cell::Run { i0: 0, i1: 10, x0: 100, x1: 120, bad: false };
        assert_eq!(cells(narrow, |_| 20, 50), [first, Cell::Run { i0: 10, i1: 14, x0: 200, x1: 208, bad: false }]);
        // a field under the band's name starts right of it
        let under = cells([span(0, 20, 90)].into_iter(), |_| 200, 50);
        assert_eq!(under, [Cell::Run { i0: 0, i1: 1, x0: 51, x1: 90, bad: false }]);
    }

    /// A burst says that one of its fields failed, and takes the tags at its ends into its text.
    #[test]
    fn a_burst_keeps_its_errors_and_its_tags() {
        let tag = |i, x| Span { i, x0: x, x1: x, tag: true, ok: true };
        let failed = Span { ok: false, ..span(2, 104, 107) };
        let spans = [tag(0, 100), span(1, 101, 104), failed, tag(3, 108), tag(4, 300)];
        let burst = Cell::Run { i0: 0, i1: 4, x0: 101, x1: 107, bad: true };
        assert_eq!(cells(spans.into_iter(), |_| 20, 50), [burst, Cell::Tag { i: 4, x: 300 }]);
    }

    /// A character line reads as text with its control characters spelled out; a hex line has spaces; what does not
    /// fit is said by dots, and a box too small for its first field stays empty.
    #[test]
    fn the_text_of_a_burst_is_cut_with_dots() {
        let ui = Ui::new(Source::Demo { t: 0.0 });
        let f = &ui.fonts().sb;
        let mut m = Model::default();
        m.dec_proto = Proto::UART;
        m.dec_bits = 8;
        m.dec_ascii = true;
        let pk: Vec<Packet> = b"Hi\r\n\x1b".iter().map(|&b| byte(b as u32)).collect();
        assert_eq!(run_text(&m, &pk, f, 1000), "Hi\\r\\n<1B>");
        let cut = run_text(&m, &pk, f, f.width("Hi\\r..."));
        assert_eq!(cut, "Hi\\r...");
        assert_eq!(run_text(&m, &pk, f, 2), "");
        m.dec_ascii = false;
        assert_eq!(run_text(&m, &pk[..2], f, 1000), "48 69");
    }
}
