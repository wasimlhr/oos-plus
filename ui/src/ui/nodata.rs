// SPDX-License-Identifier: GPL-2.0-or-later

//! A channel that is on but has no samples in the record on the screen (switched on while the scope is stopped)
//! says so in its lane or window, instead of showing an empty one.

use super::*;

impl Ui {
    /// The channels of the window being drawn that are on and have nothing in the record shown. Empty while the scope
    /// runs (the next record has them) and in a record session (its inputs get no lane).
    pub(super) fn no_data_channels(&self) -> Vec<usize> {
        if self.m.run == RunState::Run || self.record_mode() {
            return Vec::new();
        }
        let Some(r) = self.last[0].as_ref() else { return Vec::new() };
        (0..n_ch()).filter(|&c| self.m.ch[c].on && self.shows(c) && r.ch[c].is_empty()).collect()
    }

    /// The hint, in the window being drawn: one line per such channel, in its colour.
    pub(super) fn draw_no_data(&self, cv: &mut Canvas) {
        let list = self.no_data_channels();
        if list.is_empty() {
            return;
        }
        let (f, a) = (self.fonts(), self.plot.area);
        cv.set_clip(a);
        for (k, c) in list.into_iter().enumerate() {
            let text = format!("CH{}: no data yet, press Run or Single", c + 1);
            let r = Rect::new(a.x, a.y + a.h / 2 - 9 + k as i32 * 22, a.w, 18);
            cv.text_in(&f.m, r, 1, &text, CH_COLORS[c]);
        }
        cv.no_clip();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stopped_with_ch1_only() -> Ui {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        for c in 1..NCH {
            u.m.ch[c].on = false;
        }
        for _ in 0..10 {
            u.step(0.05);
        }
        u.m.run = RunState::Stop;
        u.step(0.05);
        u
    }

    /// A channel switched on while stopped has no samples: it says so, and stops saying so once a record has it.
    #[test]
    fn a_channel_switched_on_while_stopped_says_it_has_no_data() {
        let mut u = stopped_with_ch1_only();
        assert!(u.no_data_channels().is_empty(), "CH1 has its record");
        u.m.ch[2].on = true;
        u.relayout();
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        // (Default layout, Overlay: one window, the channel is in it)
        assert_eq!(u.no_data_channels(), [2]);
        // a record that has it takes the hint away
        u.m.run = RunState::Run;
        for _ in 0..6 {
            u.step(0.05);
        }
        u.m.run = RunState::Stop;
        u.step(0.05);
        assert!(u.no_data_channels().is_empty(), "the next record has CH3");
        // running, the next record has it: no hint
        u.m.run = RunState::Run;
        assert!(u.no_data_channels().is_empty());
    }
}
