// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: the classic badge strip under the lanes (Measure > Badge style). One row per source: a chip of the
//! source's colour at the left, then name and value pairs in columns ("Pk-pk 976 mV   Ampl 976 mV ..."), "----"
//! where there is no value. A column is as wide as its own widest name and widest value, and its values begin at
//! one place, so the rows read as a table and as many columns fit as the names allow. A tap on a pair opens the
//! same popup as a Studio badge.
use super::studio::MeasKey;
use super::*;

/// Height of a row of the strip.
pub(super) const CLASSIC_ROW: i32 = 24;
/// Rows of sources the strip shows at most: a scope of four channels with a measurement on each has a line for each.
/// (It was three: the measurements of the fourth channel were all behind "+N".)
const MAX_GROUPS: usize = 4;
/// Lines of the strip at most: a line per source. Fewer than three sources wrap onto the lines up to three: the
/// fourth line is only for a fourth source.
const MAX_LINES: i32 = 3;
/// Width of the source chip.
const CHIP_W: i32 = 46;

pub(super) struct ClassicLayout {
    /// the chip of each source row
    pub chips: Vec<(WaveId, Rect)>,
    pub pairs: Vec<(MeasKey, Rect)>,
    /// where the value of each pair begins (the values of a column line up)
    pub values: Vec<i32>,
    /// pinned measurements that have no place
    pub overflow: usize,
}

impl Ui {
    pub(super) fn studio_classic(&self) -> bool {
        self.m.layout == Layout::Studio && self.m.studio_classic
    }

    /// The pinned measurements by source, in the order the sources first appear (all of them: the strip shows the first four).
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

    /// The pairs of each source that is shown, with the width of the name and of the value. A value counts at least
    /// as wide as "-00.0 mV" and in steps of 8 px, so a value that changes moves no column.
    fn classic_cells(&self) -> Vec<(WaveId, Vec<(MeasKey, i32, i32)>)> {
        let f = self.fonts();
        let rows = self.meas_rows();
        let floor = f.sb.width("-00.0 mV");
        self.classic_groups().into_iter().take(MAX_GROUPS).map(|(src, keys)| (src, keys.into_iter().filter_map(|key| {
            let r = self.studio_index(key).and_then(|i| rows.get(i)).filter(|r| !self.meas_row_hidden(r))?;
            Some((key, f.s.width(&self.studio_badge_name(key, &r.name)), (f.sb.width(&r.text()).max(floor) + 7) / 8 * 8))
        }).collect())).collect()
    }

    /// The columns of the strip in `avail` px, each as (width of its names, width of its values): as many as fit,
    /// every column as wide as its own widest name and widest value. The strip had every column as wide as the widest
    /// pair of all, with room for "0.000 MHz" in each: six columns where seven fit.
    fn classic_columns(cells: &[(WaveId, Vec<(MeasKey, i32, i32)>)], avail: i32) -> Vec<(i32, i32)> {
        let longest = cells.iter().map(|(_, c)| c.len()).max().unwrap_or(0).max(1);
        // column k holds the pairs k, k + cols, ... of every source
        let widths = |cols: usize| -> Vec<(i32, i32)> {
            (0..cols).map(|k| cells.iter().flat_map(|(_, c)| c.iter().skip(k).step_by(cols)).fold((0, 0), |(n, v), c| (n.max(c.1), v.max(c.2)))).collect()
        };
        (1..=longest).rev().map(widths).find(|w| w.iter().map(|&(n, v)| Self::classic_col_w(n, v)).sum::<i32>() <= avail).unwrap_or_else(|| widths(1))
    }

    /// The width of a column: 4 to the name, 6 between the name and the value, 10 to the next column.
    fn classic_col_w(name: i32, value: i32) -> i32 {
        4 + name + 6 + value + 10
    }

    /// How many sources need a second line: their pairs do not fit one. `room`: from the first chip to the chip at
    /// the end of the row. The columns are those of `classic_layout`.
    pub(super) fn classic_wrap_now(&self, room: i32) -> i32 {
        let cells = self.classic_cells();
        let n = cells.len() as i32;
        let cols = Self::classic_columns(&cells, room - CHIP_W - 8).len();
        let long = cells.iter().filter(|(_, c)| c.len() > cols).count() as i32;
        long.min((MAX_LINES - n).max(0))
    }

    pub(super) fn classic_layout(&self) -> ClassicLayout {
        let row = self.studio_row();
        let groups = self.classic_groups();
        let cells = self.classic_cells();
        let x0 = row.x + 4;
        let columns = Self::classic_columns(&cells, self.studio_row_limit() - (x0 + CHIP_W + 8));
        let cols = columns.len();
        // where each column starts, from the first
        let starts: Vec<i32> = columns.iter().scan(0, |x, &(n, v)| { let at = *x; *x += Self::classic_col_w(n, v); Some(at) }).collect();
        let mut l = ClassicLayout { chips: Vec::new(), pairs: Vec::new(), values: Vec::new(), overflow: 0 };
        // the lines beyond one per source go to the sources whose pairs do not fit their line, in order
        let mut spare = self.classic_spare(cells.len() as i32);
        let mut line = 0;
        for (src, pairs) in &cells {
            let lines = if pairs.len() > cols && spare > 0 { spare -= 1; 2 } else { 1 };
            let y = row.y + 3 + line * CLASSIC_ROW;
            l.chips.push((*src, Rect::new(x0, y, CHIP_W, CLASSIC_ROW - 3)));
            for (c, &(key, _, _)) in pairs.iter().enumerate() {
                if c >= cols * lines { l.overflow += 1; continue; }
                let (k, cy) = (c % cols, (c / cols) as i32 * CLASSIC_ROW);
                let cell = Rect::new(x0 + CHIP_W + 8 + starts[k], y + cy, Self::classic_col_w(columns[k].0, columns[k].1) - 4, CLASSIC_ROW - 3);
                l.pairs.push((key, cell));
                // the values of a column begin at one place, after its widest name
                l.values.push(cell.x + 4 + columns[k].0 + 6);
            }
            line += lines as i32;
        }
        // the sources beyond the rows of the strip
        l.overflow += groups.iter().skip(MAX_GROUPS).map(|(_, keys)| keys.len()).sum::<usize>();
        l
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
        for ((key, r), value_x) in l.pairs.iter().zip(&l.values) {
            let Some(m) = self.studio_index(*key).and_then(|i| rows.get(i)) else { continue };
            if self.studio.popup == Some(*key) { cv.round_outline(*r, 4, ACCENT); }
            let cy = r.y + (r.h + 1) / 2 + 4;
            cv.set_clip(Rect::new(r.x, r.y, r.w, r.h + 3));
            cv.text(&f.s, r.x + 4, cy, &self.studio_badge_name(*key, &m.name), DIM);
            cv.text(&f.sb, *value_x, cy, &m.text(), TEXT);
            cv.no_clip();
        }
    }
}
