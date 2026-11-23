// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: where the handles of the Studio sources stand (ADR 0077): on the zero level of each source at the right
//! edge of the plot, clear of the corner buttons, the trigger tag and the dock's grips.

use super::*;

impl Ui {
    /// The text and the width of the badge on the bottom bar pill of channel `c`, `w` wide: CHn, or the name the user gave
    /// the channel (`:CHANnel<n>:LABel:CONTent`) where the pill is wide; a narrow pill keeps the badge alone.
    pub(super) fn studio_pill_badge(&self, c: usize, w: i32) -> (String, i32) {
        let sb = &self.fonts().sb;
        if !self.m.ch_label[c].is_empty() && w >= 130 {
            let name = self.ch_name(c);
            let bw = (sb.width(&name) + 12).clamp(42, 90);
            return (name, bw);
        }
        let label = format!("CH{}", c + 1);
        let bw = if w >= 70 { 42 } else { (sb.width(&label) + 8).min(w - 4).max(8) };
        (label, bw)
    }

    /// Where the handles of the window that is being drawn sit, in the order of the sources (channels, references,
    /// math). A handle is on its source's zero level at the right edge. A zero level outside the plot pins it to that
    /// edge, clear of the time labels; a handle that would cover another one, a corner button with its word, the
    /// trigger tag or a grip of the dock moves left until it is clear, so each stays on its own level.
    pub(super) fn studio_handle_rects(&self) -> Vec<(super::workspace::Trace, Rect)> {
        let a = self.plot.area;
        let half = 12.min(a.h / 2);
        // the lane that carries the time labels keeps them readable, and the last one the Fit row above them
        let last = self.plot.cur_win + 1 >= self.plot.wins.len();
        let times = if last { self.ref_badge_pad(a).max(self.fonts().s.size + 8) } else if self.stacked() && !self.studio_lane_has_own_time() { 0 } else { self.fonts().s.size + 8 };
        let low = (a.y + a.h - times - half).max(a.y + half);
        // what the handles keep clear of: the corner buttons with the word beside them, the grips of the dock, and the
        // trigger tag with its value
        let word = self.fonts().s.width("Zoom CH4") + 10;
        let mut avoid: Vec<Rect> = Vec::new();
        if self.plot_btns_shown() {
            avoid.extend(self.corner_btns().map(|b| Rect::new(b.x - word, b.y - 2, b.w + word + 4, b.h + 4)));
        }
        avoid.extend(self.dock_handles());
        let ts = self.m.trig_src;
        if self.m.trig_ref.is_none() && self.m.ch[ts].on && self.shows(ts) && self.m.trig_line().is_none() {
            let y = self.y_of_volts(ts, self.trig_level_shown()).clamp(a.y + (a.h / 2).min(9), a.y + a.h - (a.h / 2).min(9));
            avoid.push(Rect::new(a.x + a.w - MARK_W - 70, y - 10, MARK_W + 74, 20));
        }
        let mut out: Vec<(super::workspace::Trace, Rect)> = Vec::new();
        for source in self.trace_sources() {
            let Some(zero) = self.studio_zero_y(source) else { continue };
            // a channel the user named (`:CHANnel<n>:LABel:CONTent`) has its name on the handle, so a longer one is wider
            let w = match source {
                super::workspace::Trace::Channel(c) if !self.m.ch_label[c].is_empty() => {
                    (self.fonts().sb.width(&self.ch_name(c)) + 14).max(44)
                }
                _ => 44,
            };
            let mut r = Rect::new(a.x + a.w - 3 - w, zero.clamp(a.y + half, low) - 12, w, 24);
            while r.x > a.x + 60 && (avoid.iter().any(|b| overlaps(*b, r)) || out.iter().any(|(_, b)| overlaps(*b, r))) { r.x -= r.w + 2; }
            out.push((source, r));
        }
        out
    }
}
