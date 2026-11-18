// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: the classic badge strip under the lanes (Measure > Badge style). One row per source: a chip of the
//! source's colour at the left, then name and value pairs in even columns ("Pk-pk 976 mV   Ampl 976 mV ..."), "----"
//! where there is no value. A tap on a pair opens the same popup as a Studio badge.
use super::studio::{chip, MeasKey, OVERFLOW_W};
use super::*;

/// Height of a row of the strip.
pub(super) const CLASSIC_ROW: i32 = 24;
/// Rows of sources the strip shows at most.
const MAX_GROUPS: usize = 3;
/// Lines of the strip at most: a line per source, and with fewer sources the lines a source wraps onto.
const MAX_LINES: i32 = 3;
/// Width of the source chip.
const CHIP_W: i32 = 46;

pub(super) struct ClassicLayout {
    /// the chip of each source row
    pub chips: Vec<(WaveId, Rect)>,
    pub pairs: Vec<(MeasKey, Rect)>,
    /// pinned measurements that have no place
    pub overflow: usize,
}

impl Ui {
    pub(super) fn studio_classic(&self) -> bool {
        self.m.layout == Layout::Studio && self.m.studio_classic
    }

    /// The pinned measurements by source, in the order the sources first appear (all of them: the strip shows three rows).
    fn classic_groups(&self) -> Vec<(WaveId, Vec<MeasKey>)> {
        let mut out: Vec<(WaveId, Vec<MeasKey>)> = Vec::new();
        for key in self.studio_pinned() {
            let src = Self::studio_key_source(key);
            match out.iter_mut().find(|g| g.0 == src) {
                Some(g) => g.1.push(key),
                None => out.push((src, vec![key])),
            }
        }
        out
    }

    /// Height of the strip: a line per source, and the lines the sources wrap onto.
    pub(super) fn classic_row_px(&self) -> i32 {
        let n = self.classic_groups().len().min(MAX_GROUPS) as i32;
        (n + self.classic_spare(n)) * CLASSIC_ROW + 6
    }

    /// The lines of the strip beyond one per source (`Ui::studio_badge_wrap_now` counts them at the start of a frame).
    fn classic_spare(&self, groups: i32) -> i32 {
        self.studio.badge_wrap.clamp(0, (MAX_LINES - groups).max(0))
    }

    /// How many sources need a second line: their pairs do not fit one, with every value as wide as `value`. `names`:
    /// the badges that are shown and the widths of their names; `room`: from the first chip to the buttons at the
    /// right.
    pub(super) fn classic_wrap_now(&self, names: &[(MeasKey, i32)], value: i32, room: i32) -> i32 {
        let groups = self.classic_groups();
        let n = groups.len().min(MAX_GROUPS) as i32;
        let col_w = names.iter().map(|(_, w)| w + 8 + value + 14).max().unwrap_or(1).max(1);
        let cols = ((room - CHIP_W - 8) / col_w).max(1) as usize;
        let shown = |keys: &Vec<MeasKey>| keys.iter().filter(|k| names.iter().any(|(n, _)| n == *k)).count();
        let long = groups.iter().take(MAX_GROUPS).filter(|(_, keys)| shown(keys) > cols).count() as i32;
        long.min((MAX_LINES - n).max(0))
    }

    pub(super) fn classic_layout(&self) -> ClassicLayout {
        let f = self.fonts();
        let row = self.studio_row();
        let rows = self.meas_rows();
        let groups = self.classic_groups();
        let x0 = row.x + 4;
        let limit = self.studio_row_limit();
        let cell = |key: MeasKey| -> Option<(String, String)> {
            let r = self.studio_index(key).and_then(|i| rows.get(i)).filter(|r| !self.meas_row_hidden(r))?;
            Some((self.studio_badge_name(key, &r.name), r.text()))
        };
        // every column is as wide as the widest pair, so the columns line up from row to row
        let mut col_w = 0;
        for (_, keys) in groups.iter().take(MAX_GROUPS) {
            for &key in keys {
                if let Some((n, v)) = cell(key) { col_w = col_w.max(f.s.width(&n) + 8 + f.sb.width(&v).max(f.sb.width("0.000 MHz")) + 14); }
            }
        }
        let place = |avail: i32| -> ClassicLayout {
            let cols = (avail / col_w.max(1)).max(1) as usize;
            let mut l = ClassicLayout { chips: Vec::new(), pairs: Vec::new(), overflow: 0 };
            // the lines beyond one per source go to the sources whose pairs do not fit their line, in order
            let mut spare = self.classic_spare(groups.len().min(MAX_GROUPS) as i32);
            let mut line = 0;
            for (g, (src, keys)) in groups.iter().enumerate() {
                if g >= MAX_GROUPS { l.overflow += keys.len(); continue; }
                let shown = keys.iter().filter(|k| cell(**k).is_some()).count();
                let lines = if shown > cols && spare > 0 { spare -= 1; 2 } else { 1 };
                let y = row.y + 3 + line * CLASSIC_ROW;
                l.chips.push((*src, Rect::new(x0, y, CHIP_W, CLASSIC_ROW - 3)));
                let mut c = 0;
                for &key in keys {
                    if cell(key).is_none() { continue; }
                    if c >= cols * lines { l.overflow += 1; continue; }
                    let (cx, cy) = ((c % cols) as i32 * col_w, (c / cols) as i32 * CLASSIC_ROW);
                    l.pairs.push((key, Rect::new(x0 + CHIP_W + 8 + cx, y + cy, col_w - 4, CLASSIC_ROW - 3)));
                    c += 1;
                }
                line += lines as i32;
            }
            l
        };
        let avail = limit - (x0 + CHIP_W + 8);
        let l = place(avail);
        if l.overflow > 0 { place(avail - OVERFLOW_W - 6) } else { l }
    }

    /// The "+N" chip of the classic strip: at the right end of the first row.
    pub(super) fn classic_more(&self) -> Option<(Rect, usize)> {
        let n = self.classic_layout().overflow;
        let row = self.studio_row();
        (n > 0).then(|| (Rect::new(self.studio_row_limit() - OVERFLOW_W, row.y + 3, OVERFLOW_W, CLASSIC_ROW - 3), n))
    }

    pub(super) fn draw_classic_badges(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let rows = self.meas_rows();
        let l = self.classic_layout();
        let row = self.studio_row();
        for (g, (src, r)) in l.chips.iter().enumerate() {
            if g > 0 { cv.hline(row.x + 6, self.studio_row_limit(), r.y - 2, LINE); }
            cv.round(*r, 4, self.wave_color(*src), 255);
            cv.text_in(&f.sb, *r, 1, &self.wave_name(*src), DARK_TEXT);
        }
        for (key, r) in &l.pairs {
            let Some(m) = self.studio_index(*key).and_then(|i| rows.get(i)) else { continue };
            if self.studio.popup == Some(*key) { cv.round_outline(*r, 4, ACCENT); }
            let cy = r.y + (r.h + 1) / 2 + 4;
            cv.set_clip(Rect::new(r.x, r.y, r.w, r.h + 3));
            let x = cv.text(&f.s, r.x + 4, cy, &self.studio_badge_name(*key, &m.name), DIM) + 8;
            cv.text(&f.sb, x, cy, &m.text(), TEXT);
            cv.no_clip();
        }
        if let Some((r, n)) = self.classic_more() {
            chip(cv, r, BTN, ACCENT);
            cv.text_in(&f.sb, r, 1, &format!("+{n}"), ACCENT);
        }
    }
}
