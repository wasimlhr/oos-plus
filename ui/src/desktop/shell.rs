// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The visible native buffer is the only source of truth for mouse coordinates.

use super::panel;
use crate::ui::Screen;

/// The screen the picture of a window of `size` pixels is laid out for: its own size up to Full HD (a bigger window
/// gets a picture of its shape within 1920 x 1080, which the window scales up), the panel for a window smaller than
/// that. So a window drawn at the size it has is sharp, and the layout has the room of the window.
pub fn window_screen(size: (usize, usize)) -> Screen {
    let (w, h) = (i32::try_from(size.0).unwrap_or(0), i32::try_from(size.1).unwrap_or(0));
    if w < 1 || h < 1 {
        return Screen::PANEL;
    }
    Screen::for_monitor(w, h).unwrap_or(Screen::PANEL)
}

#[derive(Clone, Copy)]
pub struct Geometry {
    pub panel_open: bool,
    /// the size of the picture the window shows (`Ui::scr`)
    pub screen: Screen,
}

impl Geometry {
    pub fn width(self) -> i32 {
        self.screen.w + if self.panel_open { panel::WIDTH } else { 0 }
    }

    /// Inverse of minifb's AspectRatioStretch, excluding letterbox bars.
    pub fn pointer(self, size: (usize, usize), x: f32, y: f32) -> Option<(i32, i32)> {
        let (ww, wh) = (size.0.max(1) as f32, size.1.max(1) as f32);
        let (width, height) = (self.width() as f32, self.screen.h as f32);
        let scale = (ww / width).min(wh / height);
        let (ox, oy) = ((ww - width * scale) * 0.5, (wh - height * scale) * 0.5);
        let (sx, sy) = ((x - ox) / scale, (y - oy) / scale);
        (sx >= 0.0 && sy >= 0.0 && sx < width && sy < height).then_some((sx as i32, sy as i32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui;

    #[test]
    fn compact_canvas_has_no_invisible_sidebar_or_shifted_targets() {
        let compact = Geometry { panel_open: false, screen: Screen::PANEL };
        assert_eq!(compact.width(), ui::W);
        for point in [(0, 0), (511, 299), (1023, 599)] {
            assert_eq!(compact.pointer((1024, 600), point.0 as f32, point.1 as f32), Some(point));
            assert_eq!(compact.pointer((2048, 1200), point.0 as f32 * 2.0, point.1 as f32 * 2.0), Some(point));
        }
        assert_eq!(compact.pointer((1024, 600), 1024.0, 300.0), None);
        assert_eq!(compact.pointer((1024, 600), -1.0, 300.0), None);
    }

    #[test]
    fn a_window_draws_at_the_size_it_has_and_the_mouse_follows_it() {
        // a window of the panel's size or smaller keeps the panel
        assert_eq!(window_screen((1024, 600)), Screen::PANEL);
        assert_eq!(window_screen((800, 500)), Screen::PANEL);
        assert_eq!(window_screen((0, 0)), Screen::PANEL);
        // a larger one is laid out at its own size, up to Full HD, in its own shape
        assert_eq!(window_screen((1500, 900)), Screen { w: 1500, h: 900 });
        assert_eq!(window_screen((1920, 1080)), Screen { w: 1920, h: 1080 });
        assert_eq!(window_screen((3840, 2160)), Screen { w: 1920, h: 1080 });
        assert_eq!(window_screen((2560, 1600)), Screen { w: 1728, h: 1080 });
        // the mouse in a window of that size reaches every pixel of the picture, one to one
        let screen = window_screen((1500, 900));
        let geometry = Geometry { panel_open: false, screen };
        assert_eq!(geometry.pointer((1500, 900), 1499.0, 899.0), Some((1499, 899)));
        assert_eq!(geometry.pointer((1500, 900), 750.0, 450.0), Some((750, 450)));
        // a window scaled up from Full HD maps back to the picture's pixels
        let big = Geometry { panel_open: false, screen: window_screen((3840, 2160)) };
        assert_eq!(big.pointer((3840, 2160), 3839.0, 2159.0), Some((1919, 1079)));
        assert_eq!(big.pointer((3840, 2160), 1920.0, 1080.0), Some((960, 540)));
    }

    #[test]
    fn expanded_panel_and_letterbox_mapping_match_the_rendered_buffer() {
        let expanded = Geometry { panel_open: true, screen: Screen::PANEL };
        assert_eq!(expanded.width(), 1256);
        let button = panel::controls().into_iter().find(|c| c.command == "key single").unwrap().rect;
        let point = (button.x + button.w / 2, button.y + button.h / 2);
        assert_eq!(expanded.pointer((1256, 600), point.0 as f32, point.1 as f32), Some(point));
        // At half-size in a taller window, only the actual centred image is clickable.
        let y_offset = 100.0;
        assert_eq!(expanded.pointer((628, 500), point.0 as f32 * 0.5, point.1 as f32 * 0.5 + y_offset), Some(point));
        assert_eq!(expanded.pointer((628, 500), 300.0, y_offset - 1.0), None);
        assert_eq!(expanded.pointer((628, 500), 300.0, y_offset + 300.0), None);
        assert_eq!(expanded.pointer((1256, 600), 1023.5, 300.0), Some((1023, 300)));
        assert_eq!(expanded.pointer((1256, 600), 1024.0, 300.0), Some((1024, 300)));
    }
}
