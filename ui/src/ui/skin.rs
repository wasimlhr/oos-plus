// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! Skins: the same screen in another look, picked under Display > Skin and kept in the setup (`skin = Plus`).
//!
//! The screen is drawn with the OOS colours as always; a skin then recolours the finished frame in one pass. The
//! drawing code stays as it is, so a skin needs no change anywhere else: this file is the whole of it.
//!
//! The pass has two parts. Exact colours with a meaning of their own (the accent, the cursors) map one to one.
//! Every neutral grey maps along a ramp through the skin's greys, so text edges, dimmed grids and the blends
//! between two greys follow the new palette too. Coloured pixels (traces, status colours) stay as they are.
//!
//! OOS ignores setup keys it does not know, so a setup with a skin still loads there.

use super::*;

// oos-plus: the skin is a setting of the model (`oos_model::model::Skin`); its palette is the screen's
pub use crate::model::{Skin, SKINS};

/// (OOS colour, skin colour). Greys go on the ramp; the others only map when they match exactly.
fn palette(skin: Skin) -> &'static [(Rgb, Rgb)] {
    match skin {
        Skin::Oos => &[],
        Skin::Plus => &PLUS,
    }
}

/// The Plus palette, one row per OOS colour (ui.rs). Edit the right column to change the look.
const PLUS: [(Rgb, Rgb); 20] = [
    (BG, rgb(0x0b, 0x12, 0x20)),
    (ROW_ALT, rgb(0x0d, 0x15, 0x24)),
    (DLG_BG, rgb(0x0e, 0x16, 0x26)),
    (BAR, rgb(0x11, 0x1a, 0x2b)),
    (PANEL_BG, rgb(0x12, 0x1c, 0x30)),
    (BTN, rgb(0x17, 0x23, 0x3a)),
    (GRID_LINE, rgb(0x16, 0x23, 0x3a)),
    (CARD_BORDER, rgb(0x24, 0x33, 0x52)),
    (BTN_BORDER, rgb(0x2a, 0x3b, 0x5c)),
    (GRID_AXIS, rgb(0x2c, 0x44, 0x70)),
    (LINE, rgb(0x2f, 0x42, 0x66)),
    (GRID_FRAME, rgb(0x46, 0x62, 0x8f)),
    (GRID_DOT, rgb(0x4a, 0x6a, 0x9a)),
    (DIM, rgb(0x7d, 0x8d, 0xb0)),
    (AXIS_TEXT, rgb(0x93, 0xa4, 0xc4)),
    (LABEL, rgb(0xb8, 0xc7, 0xe0)),
    (TEXT, rgb(0xe8, 0xf1, 0xff)),
    // not on the ramp: a meaning of their own
    (CURSOR, rgb(0x9f, 0xd8, 0xff)),
    (ACCENT, rgb(0x4f, 0xd1, 0xff)),
    // pure white (a few icons) stays the lightest grey of the skin
    (rgb(0xff, 0xff, 0xff), rgb(0xf4, 0xf8, 0xff)),
];

/// Colours that keep their own mapping and stay off the grey ramp.
fn off_ramp(c: Rgb) -> bool {
    c == ACCENT || c == CURSOR
}

/// The grey level of a neutral pixel (r, g and b within 3), None for a coloured one.
fn grey(c: Rgb) -> Option<u8> {
    let (r, g, b) = ((c >> 16) & 255, (c >> 8) & 255, c & 255);
    (r.abs_diff(g) <= 3 && g.abs_diff(b) <= 3 && r.abs_diff(b) <= 3).then_some(g as u8)
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let ch = |s: u32| {
        let (x, y) = (((a >> s) & 255) as f32, ((b >> s) & 255) as f32);
        ((x + (y - x) * t).round() as u32) << s
    };
    ch(16) | ch(8) | ch(0)
}

/// Grey level -> skin colour: piecewise linear through the palette's greys, flat beyond the ends.
fn ramp(pal: &[(Rgb, Rgb)]) -> [Rgb; 256] {
    let mut stops: Vec<(u8, Rgb)> = pal.iter().filter(|(f, _)| !off_ramp(*f)).filter_map(|&(f, t)| grey(f).map(|g| (g, t))).collect();
    stops.sort_by_key(|s| s.0);
    stops.dedup_by_key(|s| s.0);
    std::array::from_fn(|v| {
        let v = v as u8;
        match stops.iter().position(|s| s.0 >= v) {
            None => stops.last().map_or(rgb(v as u32, v as u32, v as u32), |s| s.1),
            Some(0) => stops[0].1,
            Some(i) => {
                let (a, b) = (stops[i - 1], stops[i]);
                lerp(a.1, b.1, (v - a.0) as f32 / (b.0 - a.0) as f32)
            }
        }
    })
}

/// Recolours a frame in place. Neighbouring pixels repeat a lot, so the last answer is kept.
fn recolour(px: &mut [u32], pal: &[(Rgb, Rgb)], lut: &[Rgb; 256]) {
    let (mut last_in, mut last_out) = (u32::MAX, 0);
    for p in px.iter_mut() {
        if *p != last_in {
            last_in = *p;
            last_out = match pal.iter().find(|(f, _)| *f == *p && off_ramp(*f)) {
                Some(&(_, t)) => t,
                None => grey(*p).map_or(*p, |g| lut[g as usize]),
            };
        }
        *p = last_out;
    }
}

impl Ui {
    /// The skin's pass over the finished frame (the last step of `render`).
    pub(super) fn skin_pass(&self, cv: &mut Canvas) {
        let pal = palette(self.m.skin);
        if pal.is_empty() {
            return;
        }
        recolour(&mut cv.px, pal, &ramp(pal));
    }

    pub(super) fn skin_kind(it: Item) -> Option<Kind> {
        (it == Item::Skin).then_some(Kind::Choice)
    }

    pub(super) fn skin_label(it: Item) -> Option<&'static str> {
        (it == Item::Skin).then_some("Skin")
    }

    pub(super) fn skin_value(&self, it: Item) -> Option<String> {
        (it == Item::Skin).then(|| self.m.skin.name().into())
    }

    pub(super) fn skin_options(&self, it: Item) -> Option<Vec<String>> {
        (it == Item::Skin).then(|| SKINS.iter().map(|s| s.name().into()).collect())
    }

    pub(super) fn skin_adjust(&mut self, it: Item, d: i32) -> bool {
        if it != Item::Skin {
            return false;
        }
        let i = SKINS.iter().position(|&s| s == self.m.skin).unwrap_or(0) as i32;
        self.m.skin = SKINS[(i + d).rem_euclid(SKINS.len() as i32) as usize];
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_oos_colour_maps_once() {
        let mut from: Vec<Rgb> = PLUS.iter().map(|p| p.0).collect();
        from.sort();
        from.dedup();
        assert_eq!(from.len(), PLUS.len(), "a colour twice in the palette");
    }

    #[test]
    fn palette_greys_land_exactly_and_colours_stay() {
        let lut = ramp(&PLUS);
        let mut px: Vec<u32> = PLUS.iter().map(|p| p.0).collect();
        let traces = [rgb(0xf5, 0xd5, 0x2b), GREEN, RED, AMBER];
        px.extend(traces);
        recolour(&mut px, &PLUS, &lut);
        for (i, &(_, to)) in PLUS.iter().enumerate() {
            assert_eq!(px[i], to, "row {}", i);
        }
        assert_eq!(&px[PLUS.len()..], &traces);
    }

    #[test]
    fn a_text_edge_between_two_greys_lands_between_their_skin_colours() {
        let lut = ramp(&PLUS);
        let mid = lerp(BG, TEXT, 0.5);
        let mut px = [mid];
        recolour(&mut px, &PLUS, &lut);
        let b = |c: u32| c & 255;
        assert!(b(px[0]) > b(PLUS[0].1) && b(px[0]) < b(rgb(0xe8, 0xf1, 0xff)), "{:06x}", px[0]);
    }

    #[test]
    fn the_skin_is_in_the_display_menu_and_the_setup() {
        let mut ui = Ui::new(crate::wave::Source::Demo { t: 0.0 });
        assert!(ui.items(MenuId::Display).contains(&Item::Skin));
        assert_eq!(ui.value(Item::Skin), "OOS");
        ui.select_option(Item::Skin, "OOS Plus");
        assert_eq!(ui.m.skin, Skin::Plus);
        let text = crate::store::setup_text(&ui.m);
        assert!(text.contains("skin = Plus"), "{}", text);
        assert_eq!(crate::store::parse_setup(&text).unwrap().skin, Skin::Plus);
        // a setup without the key (OOS, or older): the OOS look
        assert_eq!(crate::store::parse_setup("").unwrap().skin, Skin::Oos);
    }
}
