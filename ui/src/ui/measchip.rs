// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the measurement chip of the Studio waveform, an arrow at the edge of the plot that slides out to
//! "Meas . N" on a tap and in again by itself, so it covers as little of the plot as it can.

use super::studio::{chip, StudioView};
use super::*;

/// Seconds the measurement chip stays out after a tap on its arrow.
const MEAS_OUT_S: f64 = 3.0;

impl Ui {
    /// How far the measurement chip has slid out, 0 (just its arrow) to 1 (its count too): out in a fifth of a second
    /// after a tap on the arrow, and back after `MEAS_OUT_S`.
    pub(super) fn studio_meas_slide(&self) -> f64 {
        let Some(t0) = self.studio.meas_out else { return 0.0 };
        let dt = self.now - t0;
        if dt < 0.0 || dt > MEAS_OUT_S + 0.2 {
            0.0
        } else if dt < 0.2 {
            dt / 0.2
        } else if dt > MEAS_OUT_S {
            1.0 - (dt - MEAS_OUT_S) / 0.2
        } else {
            1.0
        }
    }

    /// The chip and its arrow: the arrow points left while the chip is closed, right once it is out.
    pub(super) fn draw_studio_meas_chip(&self, cv: &mut Canvas) {
        let Some(r) = self.studio_meas_chip() else { return };
        let f = self.fonts();
        chip(cv, r, BTN, BTN_BORDER);
        let out = self.studio_meas_slide() > 0.9;
        if out {
            let text = format!("Meas \u{b7} {}", self.measurement_count());
            cv.text_in(&f.sb, Rect::new(r.x + 8, r.y, r.w - 30, r.h), 0, &text, TEXT);
        }
        chevron(cv, r.right() - 12, r.y + r.h / 2, if out { 1 } else { -1 }, TEXT);
    }

    /// A tap on the chip: the arrow slides it out; out, the count opens the list and the arrow takes the chip in.
    pub(super) fn studio_meas_chip_tap(&mut self, x: i32, y: i32) -> bool {
        let Some(r) = self.studio_meas_chip().filter(|r| Self::hit40(*r).contains(x, y)) else { return false };
        if self.studio_meas_slide() > 0.9 {
            self.studio.meas_out = None;
            if x < r.right() - 30 {
                self.studio_set_view(StudioView::Docked);
            }
        } else {
            self.studio.meas_out = Some(self.now);
        }
        true
    }
}
