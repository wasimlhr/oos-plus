// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: the Add measurement panel as a grid of tiles (a picture and a name each, nothing computed while browsing),
//! and the info card of a measurement: the picture, what it is, how it is computed, its sources and its unit.
use super::meascatalog::{cat_info, Cat};
use super::*;

/// Height of a tile, and the two columns of the grid.
pub(super) const TILE_H: i32 = 84;
pub(super) const TILE_COLS: i32 = 2;
/// Height of the heading of a group of tiles.
const HEAD_H: i32 = 26;
const CARD_W: i32 = 520;
const CARD_H: i32 = 460;

impl Ui {
    /// The "i" of a tile: a 40 px target in its top right corner.
    pub(super) fn tile_info_rect(tile: Rect) -> Rect {
        Rect::new(tile.right() - 40, tile.y, 40, 40)
    }

    /// How far the grid can scroll, for a grid of `total` px.
    pub(super) fn tile_scroll_max_of(&self, total: i32, list_h: i32) -> i32 {
        (total - list_h).max(0)
    }
    pub(super) fn tile_scroll_max(&self) -> i32 {
        let g = self.sheet_add_geo();
        self.tile_scroll_max_of(self.add_layout(g.list).2, g.list.h)
    }

    /// The tiles of the list, in the grid of `list` (not scrolled), with the headings of the groups of the Measure
    /// page between them (not while the list is a search), and the height of it all.
    pub(super) fn add_layout(&self, list: Rect) -> (Vec<Rect>, Vec<(Rect, &'static str)>, i32) {
        let entries = self.sheet_add_entries();
        let headed = self.sheet.search.trim().is_empty();
        let w = (list.w - 4) / TILE_COLS;
        let (mut tiles, mut heads) = (Vec::new(), Vec::new());
        let (mut y, mut col, mut last) = (0, 0, None);
        for e in &entries {
            if headed && last != Some(e.section) {
                if col != 0 {
                    y += TILE_H;
                    col = 0;
                }
                heads.push((Rect::new(list.x, list.y + y, list.w, HEAD_H), e.section));
                y += HEAD_H;
                last = Some(e.section);
            }
            tiles.push(Rect::new(list.x + col * (w + 4), list.y + y, w, TILE_H - 4));
            col += 1;
            if col == TILE_COLS {
                col = 0;
                y += TILE_H;
            }
        }
        if col != 0 {
            y += TILE_H;
        }
        (tiles, heads, y)
    }

    /// One tile: a picture over its name, a mark when it is added, the "i".
    pub(super) fn draw_add_tile(&self, cv: &mut Canvas, tile: Rect, cat: Cat) {
        let f = self.fonts();
        let info = cat_info(cat);
        let added = self.add_is_on(cat);
        let need = self.add_preview(cat);
        let off = matches!(need, super::meascatalog::Preview::Off(_) | super::meascatalog::Preview::Reason(_));
        cv.round(tile, 6, if added { ACCENT } else { BTN_BORDER }, 255);
        cv.round(tile.inset(1), 5, if added { mix(BTN, ACCENT, 28) } else { BTN }, 255);
        let col = if off {
            DIM
        } else if added {
            ACCENT
        } else {
            TEXT
        };
        // an added tile has its value at the left: the figure moves over and the value ends before it
        let pic = if added {
            Rect::new(tile.x + tile.w / 2 - 8, tile.y + 6, 60, 36)
        } else {
            Rect::new(tile.x + tile.w / 2 - 36, tile.y + 6, 72, 36)
        };
        super::caticons::draw_cat_icon(cv, f, pic, cat, col, if off { DIM } else { ACCENT });
        let lines = super::meascatalog::wrap_lines(&f.s, info.name, tile.w - 10);
        for (i, l) in lines.iter().take(2).enumerate() {
            cv.text_in(&f.s, Rect::new(tile.x + 4, tile.y + 42 + i as i32 * 15, tile.w - 8, 16), 1, l, col);
        }
        if added {
            let (x, y) = (tile.x + 10, tile.y + 12);
            for d in [0, 1] {
                cv.line(x, y + d, x + 4, y + 5 + d, ACCENT);
                cv.line(x + 4, y + 5 + d, x + 11, y - 5 + d, ACCENT);
            }
            // its value now, as the list has it
            let rows = self.meas_rows();
            if let Some(row) = self.studio_index(self.add_key(cat)).and_then(|i| rows.get(i)) {
                if row.v.is_finite() {
                    let (text, room) = (row.text(), pic.x - 3 - (tile.x + 26));
                    let font = if f.sb.width(&text) <= room { &f.sb } else { &f.s };
                    let outer = cv.save_clip();
                    cv.clip_within(Rect::new(tile.x + 26, tile.y + 2, room.max(1), 22));
                    cv.text(font, tile.x + 26, tile.y + 17, &text, ACCENT);
                    cv.restore_clip(outer);
                }
            }
        }
        // the "i": a circle with a stem
        let i = Self::tile_info_rect(tile);
        let (cx, cy) = (i.x + i.w / 2 + 4, i.y + 16);
        cv.circle(cx, cy, 8, if off { DIM } else { LABEL });
        cv.circle(cx, cy, 7, BTN);
        cv.fill(Rect::new(cx - 1, cy - 1, 2, 6), TEXT);
        cv.fill(Rect::new(cx - 1, cy - 5, 2, 2), TEXT);
    }

    // ------------------------------------------------------------------------------------------------ the info card

    pub(super) fn info_card_rect(&self) -> Rect {
        let top = self.frame().top.bottom() + 10;
        Rect::new((self.scr.w - CARD_W) / 2, top, CARD_W, CARD_H)
    }

    pub(super) fn info_card_close(card: Rect) -> Rect {
        Rect::new(card.right() - 48, card.y + 4, 44, 44)
    }
    pub(super) fn info_card_action(card: Rect) -> Rect {
        Rect::new(card.right() - 150, card.bottom() - 54, 138, 44)
    }

    /// The card's three parts, wrapped in the largest body font whose text ends above the action button (the owner:
    /// the text can be bigger), with the line step and where the last line ends.
    pub(super) fn info_card_text(
        f: &crate::font::Fonts,
        cat: Cat,
        card: Rect,
    ) -> (&Font, i32, Vec<(&'static str, Vec<String>)>) {
        let info = cat_info(cat);
        let texts =
            [("What it is", info.blurb.to_string()), ("How it is computed", info.how), ("Settings", info.settings)];
        let bottom = Self::info_card_action(card).y - 6;
        let mut out = (&f.s, f.s.size + 4, Vec::new());
        for body in [&f.m, &f.s] {
            let step = body.size + 5;
            let parts: Vec<_> =
                texts.iter().map(|(h, t)| (*h, super::meascatalog::wrap_lines(body, t, card.w - 32))).collect();
            let h: i32 = parts.iter().map(|(_, l)| 16 + step * (l.len() as i32 + 1)).sum();
            out = (body, step, parts);
            if card.y + 108 + h <= bottom {
                break;
            }
        }
        out
    }

    pub(super) fn draw_info_card(&self, cv: &mut Canvas) {
        let Some(cat) = self.studio.md.info else { return };
        let f = self.fonts();
        let info = cat_info(cat);
        let card = self.info_card_rect();
        cv.blend(Rect::new(0, 0, self.scr.w, self.scr.h), BG, 140);
        cv.round(card, 10, BTN_BORDER, 255);
        cv.round(card.inset(1), 9, PANEL_BG, 255);
        let pic = Rect::new(card.x + 16, card.y + 16, 110, 72);
        cv.round(pic, 6, BTN_BORDER, 255);
        cv.round(pic.inset(1), 5, BG, 255);
        super::caticons::draw_cat_icon(cv, f, pic.inset(6), cat, TEXT, ACCENT);
        cv.text(&f.lb, pic.right() + 16, card.y + 38, info.name, TEXT);
        cv.text(
            &f.m,
            pic.right() + 16,
            card.y + 64,
            &format!("Unit: {}", if info.unit.is_empty() { "none" } else { info.unit }),
            DIM,
        );
        cv.text(&f.m, pic.right() + 16, card.y + 86, &info.sources, DIM);
        let close = Self::info_card_close(card);
        btn(cv, close.inset(4), BTN, BTN_BORDER);
        cv.line(close.x + 15, close.y + 15, close.right() - 15, close.bottom() - 15, TEXT);
        cv.line(close.x + 15, close.bottom() - 15, close.right() - 15, close.y + 15, TEXT);
        let (body, step, parts) = Self::info_card_text(f, cat, card);
        let mut y = card.y + 108;
        for (head, lines) in parts {
            y += 10;
            cv.text(&f.lb, card.x + 16, y, head, TEXT);
            y += 6;
            for l in lines {
                y += step;
                cv.text(body, card.x + 16, y, &l, TEXT);
            }
            y += step;
        }
        let a = Self::info_card_action(card);
        let added = self.add_is_on(cat);
        btn(cv, a, BTN, if added { RED } else { ACCENT });
        cv.text_in(&f.mb, a, 1, if added { "Remove" } else { "Add" }, if added { RED } else { ACCENT });
    }

    /// A tap while the card is open: its buttons, or anywhere else closes it. Always taken.
    pub(super) fn info_card_tap(&mut self, x: i32, y: i32) -> bool {
        let Some(cat) = self.studio.md.info else { return false };
        let card = self.info_card_rect();
        if !card.contains(x, y) || Self::info_card_close(card).contains(x, y) {
            self.studio.md.info = None;
            return true;
        }
        if Self::info_card_action(card).contains(x, y) {
            self.sheet_add_toggle(cat);
            if !self.add_is_on(cat) {
                self.studio.md.info = None;
            }
        }
        true
    }
}
