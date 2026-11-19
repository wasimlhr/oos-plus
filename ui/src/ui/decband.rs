// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! A decoder's band on the plot: its name in a pill of the source's colour, and what it decoded along the time axis.
//! How much the band says follows the time base. The fields are taken message by message (`oos_dsp::decode::bursts`:
//! what the sender sent without a long pause). Where every field of a message is wide enough for its text, each has
//! a box of its own. Else the message is one box, which holds as much of its text as fits and "..." for the rest, so
//! a slow time base shows the messages and not an empty band or a row of single letters. A message shorter than its
//! text takes the room of the idle line behind it, up to the next message, and a bar in the box says how long the
//! message itself is. A message too small for a box stays a mark: where the line is busy is seen at every time base.
//! At a time base that shows single bits, a protocol that reports them (`Protocol::bits`) has a row of bits above its
//! boxes, and a mark on the threshold line where each was sampled.

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
/// A field with less room than this is never asked for its text: no text fits.
const OWN_MIN: i32 = 10;
/// Room the box of a message leaves to the message after it.
const SLOT_GAP: i32 = 2;

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
    /// it begins a message
    pub first: bool,
}

/// What the band draws at one place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Cell {
    /// field `i` in a box of its own
    One { i: usize, x0: i32, x1: i32 },
    /// the message of the fields `i0..i1`, which are not all wide enough for their texts, in one box; `bad`: one of
    /// them failed its check; `until`: where the idle line behind the message ends, which its text may use
    Run { i0: usize, i1: usize, x0: i32, x1: i32, bad: bool, until: i32 },
    /// the tag of event `i`
    Tag { i: usize, x: i32 },
}

/// The cells of a band from the fields on the screen, in their order. `need(i)` is the width field `i` needs for its
/// own text; `left` is where the band's name ends.
pub(super) fn cells(spans: &[Span], need: impl Fn(usize) -> i32, left: i32) -> Vec<Cell> {
    let mut out: Vec<Cell> = Vec::new();
    // the end of the box before: a box starts right of it (at first right of the band's name)
    let mut end = left;
    let mut k = 0;
    while k < spans.len() {
        // one message: up to the next field that begins one
        let n = spans[k + 1..].iter().position(|s| s.first).map_or(spans.len(), |d| k + 1 + d);
        let burst = &spans[k..n];
        k = n;
        let fields: Vec<&Span> = burst.iter().filter(|s| !s.tag).collect();
        let Some(head) = fields.first() else {
            // events only
            out.extend(burst.iter().map(|s| Cell::Tag { i: s.i, x: s.x0 }));
            continue;
        };
        // every field wide enough for its own text: a box each
        let own = fields.iter().enumerate().all(|(f, s)| {
            let room = s.x1 - if f == 0 { s.x0.max(end + 1) } else { s.x0 };
            room >= OWN_MIN && need(s.i) <= room
        });
        if own {
            for s in burst {
                if s.tag {
                    out.push(Cell::Tag { i: s.i, x: s.x0 });
                    continue;
                }
                out.push(Cell::One { i: s.i, x0: s.x0.max(end + 1), x1: s.x1 });
                end = s.x1;
            }
            continue;
        }
        // one box from its first field to its last; its tags are said by its text. The idle line up to the field
        // after the message is room for a text longer than the message
        let x0 = head.x0.max(end + 1);
        let x1 = fields.iter().map(|s| s.x1).max().unwrap_or(head.x1);
        if x1 >= x0 {
            let bad = burst.iter().any(|s| !s.ok);
            let until = spans[n..].iter().find(|s| !s.tag).map_or(i32::MAX / 2, |s| s.x0 - SLOT_GAP);
            out.push(Cell::Run { i0: burst[0].i, i1: burst[burst.len() - 1].i + 1, x0, x1, bad, until });
            end = x1;
        }
    }
    out
}

/// Whether the decoder shows characters (UART with Format ASCII).
pub(super) fn ascii(m: &Model) -> bool {
    m.dec_proto == Proto::UART && m.dec_ascii
}

/// One field in the text of a message. A character line reads as text: control characters as `\r`, `\n`, `\t`, any
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

/// The text of a message: its fields in order, as many as fit into `room` pixels, then "...". Characters stand side by
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

/// The whole text of a message, as its box would hold it with all the room.
pub(super) fn message_text(m: &Model, pk: &[Packet]) -> String {
    pk.iter().map(|p| run_piece(m, p)).collect::<Vec<_>>().join(if ascii(m) { "" } else { " " })
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
    /// The samples of a decoder source in the record on the screen.
    fn dec_wave(&self, source: usize) -> Option<&[f32]> {
        match decode_wave(source)? {
            WaveId::Ch(c) => self.last[0].as_ref().map(|r| &r.ch[c][..]),
            WaveId::Ref(i) => self.refs.get(i)?.as_ref().map(|r| &r.v[..]),
        }
    }

    /// The middle of the signal on the source of the decoder `m` is the view of: half way between the lowest and
    /// the highest sample of the record on the screen (of A - B for a pair). `None` on LA lines, without a record
    /// and for a line that does not move.
    pub(crate) fn dec_signal_middle(&self, m: &Model) -> Option<f32> {
        if m.dec_on_la() {
            return None;
        }
        let a = self.dec_wave(m.dec_src)?;
        let b = if m.dec_proto.diff_ok() { m.dec_diff.and_then(|b| self.dec_wave(b)) } else { None };
        let (mut lo, mut hi) = (f32::INFINITY, f32::NEG_INFINITY);
        for (k, &x) in a.iter().enumerate() {
            let v = x - b.and_then(|b| b.get(k)).copied().unwrap_or(0.0);
            if v.is_finite() {
                lo = lo.min(v);
                hi = hi.max(v);
            }
        }
        (hi > lo).then(|| (lo + hi) / 2.0)
    }

    /// The thresholds of the further lines of the decoder `m` is the view of, when the source's is `thr`: for each
    /// line the middle of its own signal, where that is not the source's (more than a tenth of the line's swing
    /// away); `None` for a line that can share `thr`, has no source or does not move.
    pub(crate) fn dec_line_middles(&self, m: &Model, thr: f32) -> [Option<f32>; 3] {
        let r = m.dec_proto.roles();
        let src = [r[1].map(|_| m.dec_src2), r[2].and(m.dec_cs), r[3].and(m.dec_src4)];
        std::array::from_fn(|k| {
            let w = self.dec_wave(src[k]?)?;
            let (lo, hi) = w.iter().filter(|x| x.is_finite()).fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &x| (lo.min(x), hi.max(x)));
            let mid = (lo + hi) / 2.0;
            (hi > lo && (mid - thr).abs() > 0.1 * (hi - lo)).then_some(mid)
        })
    }

    /// Decode > Threshold to middle: the thresholds of the decoder the menu edits, once. The source's goes to the
    /// middle of its signal, and each further line gets one of its own where its signal has another middle.
    pub(super) fn dec_threshold_to_middle(&mut self) {
        match self.dec_signal_middle(&self.m) {
            Some(v) => {
                let lines = if self.m.dec_on_la() { [None; 3] } else { self.dec_line_middles(&self.m, v) };
                (self.m.dec_thr, self.m.dec_thr_line) = (v, lines);
                self.settings_changed();
                let roles = self.m.dec_proto.roles();
                let own: Vec<String> = (0..3).filter_map(|k| lines[k].map(|t| format!("{} {}", roles[k + 1].unwrap_or("line"), si(t as f64, "V")))).collect();
                self.toast(&if own.is_empty() { format!("Threshold {}", si(v as f64, "V")) } else { format!("Threshold {}, {}", si(v as f64, "V"), own.join(", ")) });
            }
            None => self.toast("No signal on the decoder's source"),
        }
    }

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
        // the name in the source's colour, which says the source. Studio writes the source too, and the number only
        // where several decoders are on ("UART CH1", "D2 SPI CH2"); a tap on the name opens the Decode menu of this
        // decoder there (`studio.dec_labels`)
        let studio = self.m.layout == Layout::Studio;
        let name = if studio {
            let several = (0..DECODERS).filter(|&d| self.m.dec_is_on(d)).count() > 1;
            let number = if several { format!("D{} ", k + 1) } else { String::new() };
            let source = if m.dec_on_la() { String::new() } else { format!(" {}", decode_source_name(m.dec_src)) };
            format!("{}{}{}", number, m.dec_proto.name(), source)
        } else {
            format!("D{} {}", k + 1, m.dec_proto.name())
        };
        // in a plot it stands right of the channel markers at the left edge (Studio: of the lane badges), which are
        // drawn after the band and would cover it
        let px = a.x + if row.is_some() { 4 } else if studio { 50 } else { MARK_W + 2 };
        let (py, ph) = if row.is_some() { (y, h) } else { (y + 2, h - 4) };
        let pill = Rect::new(px, py, f.sb.width(&name) + 12, ph);
        if studio {
            self.studio.dec_labels.borrow_mut().push((k, pill));
        }
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
        // the row chosen in the Events pane: its time span across the plot, lightly, and below its boxes marked
        let chosen = self.ev_selected().filter(|s| s.dec == k);
        if let Some(s) = chosen.filter(|_| row.is_none()) {
            let (x0, x1) = (xt(s.t0).max(a.x), xt(s.t1).min(right));
            if x1 >= x0 {
                cv.blend(Rect::new(x0, a.y, (x1 - x0).max(2), a.h), ACCENT, 26);
                cv.vline(x0, a.y, a.y + a.h - 1, DIM);
                cv.vline(x1.max(x0 + 1), a.y, a.y + a.h - 1, DIM);
            }
        }
        // a box a finger can find: a finger's height around it
        let touch = |x0: i32, x1: i32| Rect::new(x0.max(left), y - 8, (x1 - x0.max(left)).max(12), h + 16);
        let mark = |cv: &mut Canvas, x0: i32, x1: i32| {
            cv.round_outline(Rect::new(x0 - 2, y - 2, x1 - x0 + 5, h + 4), 4, ACCENT);
        };
        // the messages of the record, and of each field on the screen whether it begins one
        let groups = oos_dsp::decode::bursts(&packets);
        let mut g = 0;
        let spans: Vec<Span> = packets
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                while g + 1 < groups.len() && groups[g].end <= i {
                    g += 1;
                }
                let (x0, x1) = (xt(p.t0), xt(p.t1));
                let first = groups[g].start == i;
                let seen = x1 >= left && x0 <= right;
                seen.then(|| Span { i, x0, x1, tag: packet_tag(m, p).is_some(), ok: p.ok, first })
            })
            .collect();
        let mut tag_end = left;
        for c in cells(&spans, |i| f.sb.width(&packet_text(m, &packets[i])) + BOX_PAD, left) {
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
                    self.band_hits.borrow_mut().push((touch(x0, x1), k, p.t0));
                    if !field_box(cv, x0, x1, y, h, edge, if p.ok { scale(edge, 70) } else { ERR_FILL }) {
                        continue;
                    }
                    if chosen.is_some_and(|s| p.t0 >= s.t0 && p.t1 <= s.t1) {
                        mark(cv, x0, x1);
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
                Cell::Run { i0, i1, x0, x1, bad, until } => {
                    // the text is what the message begins with, from the left end of its box, which is on the
                    // screen. A message shorter than its text takes the idle line behind it, as far as that goes
                    let reach = if x1 >= right { x1 } else { x1.max(until.min(right)) };
                    let room = reach.min(right) - x0 - BOX_PAD - 2;
                    let txt = run_text(m, &packets[i0..i1], &f.sb, room);
                    let need = x0 + f.sb.width(&txt) + BOX_PAD + 2;
                    let wide = if txt.is_empty() { x1 } else { x1.max(need.min(reach)) };
                    let edge = if bad { RED } else { col };
                    self.band_hits.borrow_mut().push((touch(x0, wide), k, packets[i0].t0));
                    if !field_box(cv, x0, wide, y, h, edge, scale(col, 70)) {
                        continue;
                    }
                    if chosen.is_some_and(|s| packets[i0].t0 <= s.t1 && packets[i1 - 1].t1 >= s.t0) {
                        mark(cv, x0, wide);
                    }
                    if wide > x1 {
                        // the bar is the message itself
                        cv.fill(Rect::new(x0 + 1, y + h - 4, (x1 - x0).max(2), 3), edge);
                    }
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
        Span { i, x0, x1, tag: false, ok: true, first: false }
    }

    fn byte(value: u32) -> Packet {
        Packet { t0: 0.0, t1: 1.0, kind: DecKind::Data, value, value2: None, ok: true }
    }

    /// No message behind it: the idle line is its to the end.
    const OPEN: i32 = i32::MAX / 2;

    /// Fields wide enough for their text keep a box each. A message whose fields are not is one box, and the next
    /// message another.
    #[test]
    fn a_message_without_room_for_its_fields_is_one_box() {
        let wide = [span(0, 100, 140), span(1, 140, 180)];
        let boxes = cells(&wide, |_| 20, 50);
        assert_eq!(boxes, [Cell::One { i: 0, x0: 100, x1: 140 }, Cell::One { i: 1, x0: 141, x1: 180 }]);
        // ten fields of 2 px from 100; four more from 200 are the next message
        let at = |k: usize, x: i32| Span { first: k == 10, ..span(k, x + 2 * k as i32, x + 2 + 2 * k as i32) };
        let narrow: Vec<Span> = (0..10).map(|k| at(k, 100)).chain((10..14).map(|k| at(k, 180))).collect();
        let first = Cell::Run { i0: 0, i1: 10, x0: 100, x1: 120, bad: false, until: 198 };
        let second = Cell::Run { i0: 10, i1: 14, x0: 200, x1: 208, bad: false, until: OPEN };
        assert_eq!(cells(&narrow, |_| 20, 50), [first, second]);
        // one message with a pause inside that is no longer than usual stays one box
        let paced: Vec<Span> = (0..10).map(|k| span(k, 100 + 6 * k as i32, 102 + 6 * k as i32)).collect();
        assert_eq!(cells(&paced, |_| 20, 50), [Cell::Run { i0: 0, i1: 10, x0: 100, x1: 156, bad: false, until: OPEN }]);
        // a field under the band's name starts right of it
        assert_eq!(cells(&[span(0, 20, 90)], |_| 30, 50), [Cell::One { i: 0, x0: 51, x1: 90 }]);
    }

    /// Fields narrower than their texts with idle line between them are their message in one box, not a row of
    /// boxes that took the idle line (seen on a unit: single letters where the message had been readable). The idle
    /// line behind a message is room for its text, up to the next message.
    #[test]
    fn narrow_fields_with_idle_between_them_stay_one_message() {
        let sparse = [span(0, 100, 102), span(1, 130, 132), span(2, 160, 162)];
        assert_eq!(cells(&sparse, |_| 12, 50), [Cell::Run { i0: 0, i1: 3, x0: 100, x1: 162, bad: false, until: OPEN }]);
        // three messages of one field: each has the line up to the next
        let apart = sparse.map(|s| Span { first: true, ..s });
        let want = [
            Cell::Run { i0: 0, i1: 1, x0: 100, x1: 102, bad: false, until: 128 },
            Cell::Run { i0: 1, i1: 2, x0: 130, x1: 132, bad: false, until: 158 },
            Cell::Run { i0: 2, i1: 3, x0: 160, x1: 162, bad: false, until: OPEN },
        ];
        assert_eq!(cells(&apart, |_| 12, 50), want);
    }

    /// A message in one box says that one of its fields failed, and takes its tags into its text; a tag on its own
    /// stays a tag.
    #[test]
    fn a_message_keeps_its_errors_and_its_tags() {
        let tag = |i, x, first| Span { i, x0: x, x1: x, tag: true, ok: true, first };
        let failed = Span { ok: false, ..span(2, 104, 107) };
        let spans = [tag(0, 100, true), span(1, 101, 104), failed, tag(3, 108, false), tag(4, 300, true)];
        let burst = Cell::Run { i0: 0, i1: 4, x0: 101, x1: 107, bad: true, until: OPEN };
        assert_eq!(cells(&spans, |_| 20, 50), [burst, Cell::Tag { i: 4, x: 300 }]);
    }

    /// A decoder whose source channel is off has no band; it stays on and its band is back with the channel.
    #[test]
    fn the_band_goes_with_its_source_channel_and_comes_back() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.ch[2].on = true;
        u.m.dec_on = true;
        u.m.dec_src = 2;
        let named = |u: &mut Ui| {
            for _ in 0..4 {
                u.step(0.1);
            }
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            cv.au_start();
            u.render(&mut cv);
            let texts = cv.au.take().unwrap();
            texts.into_iter().any(|e| matches!(e, Au::Text { s, .. } if s == "D1 UART" || s == "source channel off"))
        };
        assert!(named(&mut u), "the band of a decoder on a channel that is on");
        u.m.ch[2].on = false;
        assert!(!named(&mut u), "no band, and no note of one, for a source that is off");
        assert!(u.m.dec_on && u.m.dec_src == 2, "the decoder is as it was set");
        u.m.ch[2].on = true;
        assert!(named(&mut u));
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
