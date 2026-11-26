// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio: gestures of the measurements. A row of the table swiped left is removed (with an Undo bar for a few
//! seconds); a badge held and dragged off the strip unpins it, and swiped down off the screen it is removed; a tile of
//! the Add measurement panel dragged onto a channel adds it for that channel. A cell of the strip and a card have no
//! gesture of their own: a drag scrolls them, a tap opens the settings of the measurement.
use super::meascatalog::Preview;
use super::studio::{MeasKey, StudioDrag, StudioView};
use super::workspace::Action;
use super::*;

/// A swipe starts after this much horizontal movement, when it is more horizontal than vertical.
pub(super) const SWIPE_START: i32 = 16;
/// How long the Undo bar stays.
pub(super) const UNDO_S: f64 = 6.0;
/// A finger that moves more than this cancels a hold.
const HOLD_SLOP: i32 = 12;
/// Height of the Undo bar.
const BAR_H: i32 = 44;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum HoldOn {
    Badge(MeasKey),
    /// a math or reference chip of the bottom bar: a hold opens its quick actions
    Math(usize),
    Ref(usize),
    /// a channel chip of the bottom bar: its hold opens the quick actions of the channel
    Chan(usize),
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Hold {
    pub on: HoldOn,
    pub x0: i32,
    pub y0: i32,
    pub x: i32,
    pub y: i32,
    pub at: f64,
}

/// A channel, ref or math channel turned off by a swipe down, kept until the Undo bar goes.
#[derive(Clone, Copy, Debug)]
pub(super) struct SrcUndo {
    pub on: HoldOn,
    /// the math operation that was on
    pub op: Option<MathOp>,
    pub at: f64,
}

/// A removed measurement, kept until the Undo bar goes.
#[derive(Clone, Copy, Debug)]
pub(super) struct Undo {
    pub key: MeasKey,
    pub index: usize,
    pub pinned: bool,
    pub own: Option<reflook::MeasOwn>,
    pub at: f64,
}

#[derive(Default)]
pub(super) struct Md {
    pub hold: Option<Hold>,
    /// the row of the table a finger went down on: it may become a swipe
    pub cand: Option<(MeasKey, i32, i32)>,
    /// the row that is being swiped and how far (<= 0)
    pub swipe: Option<(MeasKey, i32)>,
    /// a row that springs back after a short swipe
    pub spring: Option<(MeasKey, f32)>,
    pub undo: Option<Undo>,
    /// a channel, ref or math turned off by a swipe down on its pill: kept for the Undo bar
    pub src_undo: Option<SrcUndo>,
    /// a finger went down on a badge or a bottom-bar pill: where, for the swipe down
    pub dn: Option<(HoldOn, i32, i32)>,
    /// a finger down on the detailed table: where, and how far it was scrolled
    pub hdown: Option<(i32, i32, i32)>,
    /// the badges were just hidden: a small bar with Undo, since when
    pub hide_undo: Option<f64>,
    /// the info card of a measurement (the catalogue entry) is open
    pub info: Option<super::meascatalog::Cat>,
    /// a tile that needs a channel that is off was tapped: a second tap soon after turns the channel on
    pub need_tap: Option<(super::meascatalog::Cat, f64)>,
    /// a danger button of a sheet footer was tapped once: the next tap within 3 s does it
    pub armed: Option<(Item, f64)>,
    /// the Add tiles scroll by this many pixels, and a drag that scrolled them is no tap
    pub tile_scroll: i32,
    pub tile_drag: Option<(i32, i32)>,
    pub tile_scrolled: bool,
}

impl Ui {
    // ------------------------------------------------------------------------------------------------ geometry

    /// The row of the table under a point: the measurement and the rectangle of the row.
    fn md_row_at(&self, x: i32, y: i32) -> Option<(MeasKey, Rect)> {
        let g = self.studio_panel_geo()?;
        if !g.list.contains(x, y) || !self.studio_detailed() {
            return None;
        }
        let rows = self.meas_rows();
        let listed = self.studio_listed(&rows);
        let first = self.studio_first(listed.len(), g.page);
        let (pos, r) = self.detail_layout(&g, listed.len(), first).into_iter().find(|(_, r)| r.contains(x, y))?;
        let key = *self.studio_keys().get(listed[pos])?;
        Some((key, r))
    }

    /// The Undo bar: the bottom of the list.
    pub(super) fn md_undo_bar(&self) -> Option<(Rect, Rect)> {
        if self.studio.md.undo.is_none() && self.studio.md.src_undo.is_none() {
            return None;
        }
        // with the panel it is at the foot of the list; without it, under the top bar like the hide bar
        let bar = match self.studio_panel_geo() {
            Some(g) => Rect::new(g.list.x, g.list.bottom() - BAR_H, g.list.w, BAR_H),
            None => {
                let w = 300;
                let below = if self.studio.md.hide_undo.is_some() { BAR_H + 6 } else { 0 };
                Rect::new((self.scr.w - w) / 2, self.frame().top.bottom() + 6 + below, w, BAR_H)
            }
        };
        Some((bar, Rect::new(bar.right() - 92, bar.y, 92, bar.h)))
    }

    /// The math or reference chip of the bottom bar under a point.
    pub(super) fn md_chip_at(&self, x: i32, y: i32) -> Option<HoldOn> {
        if self.m.layout != Layout::Studio || self.full_on() {
            return None;
        }
        let b = self.studio_bottom();
        if let Some(k) = (0..NMATH).find(|&k| b.math[k].w > 0 && Self::hit40(b.math[k]).contains(x, y)) {
            return Some(HoldOn::Math(k));
        }
        if let Some(c) = (0..n_ch()).find(|&c| b.ch[c].w > 0 && Self::hit40(b.ch[c]).contains(x, y)) {
            return Some(HoldOn::Chan(c));
        }
        self.shown_refs()
            .into_iter()
            .find(|&i| b.refs[i].w > 0 && Self::hit40(b.refs[i]).contains(x, y))
            .map(HoldOn::Ref)
    }

    // ------------------------------------------------------------------------------------------------ remove, undo

    /// Removes a measurement and keeps what is needed to bring it back at the same place.
    pub(super) fn md_remove(&mut self, key: MeasKey) {
        let Some(index) = self.studio_index(key) else { return };
        let pinned = self.studio_is_pinned(key);
        let own = match key {
            MeasKey::One(k, id) => self.meas_own.rows.get(&(k, id)).copied(),
            _ => None,
        };
        self.studio.md.undo = Some(Undo { key, index, pinned, own, at: self.now });
        self.studio_set_pin(key, false);
        self.remove_measurement(index);
    }

    /// Brings the removed measurement back: the same place in the list, pinned again when it was.
    pub(super) fn md_undo(&mut self) {
        let Some(u) = self.studio.md.undo.take() else { return };
        match u.key {
            MeasKey::One(k, id) => {
                let at = u.index.min(self.m.meas.len());
                self.m.meas.insert(at, (k, id));
                if let Some(o) = u.own {
                    self.meas_own.rows.insert((k, id), o);
                }
            }
            MeasKey::Pair(k, a, b) => {
                let at = u.index.saturating_sub(self.m.meas.len()).min(self.m.meas2.len());
                self.m.meas2.insert(at, (k, a, b));
            }
            MeasKey::Cat(i, a, b, c, d) => {
                let at = u.index.saturating_sub(self.m.meas.len() + self.m.meas2.len()).min(self.m.cat.len());
                self.m.cat.insert(at, (i, a, b, c, d));
            }
        }
        if u.pinned {
            self.studio_set_pin(u.key, true);
        }
        self.clear_measurement_statistics();
        self.relayout();
    }

    // ------------------------------------------------------------------------------------------------ per frame

    /// Every frame: the Undo bar times out, a short swipe springs back, a held finger picks the badge up.
    pub(super) fn md_tick(&mut self) {
        if self.studio.md.undo.is_some_and(|u| self.now - u.at > UNDO_S) {
            self.studio.md.undo = None;
        }
        if self.studio.md.src_undo.is_some_and(|u| self.now - u.at > UNDO_S) {
            self.studio.md.src_undo = None;
        }
        if self.studio.md.hide_undo.is_some_and(|t| self.now - t > UNDO_S) {
            self.studio.md.hide_undo = None;
        }
        if let Some((key, off)) = self.studio.md.spring {
            let off = off * 0.6;
            self.studio.md.spring = if off.abs() < 2.0 { None } else { Some((key, off)) };
        }
        let Some(h) = self.studio.md.hold else { return };
        if self.now - h.at < LONG_PRESS || (h.x - h.x0).abs().max((h.y - h.y0).abs()) > HOLD_SLOP {
            return;
        }
        self.studio.md.hold = None;
        self.studio.md.cand = None;
        self.studio.drag = match h.on {
            HoldOn::Badge(key) => Some(StudioDrag::BadgeDrag { key, x: h.x, y: h.y }),
            chip => {
                // the quick actions: the lift of this finger is no tap
                self.workspace.lift_pending = true;
                self.studio_chip_popup(chip, h.x, h.y);
                None
            }
        };
    }

    /// The ring that fills under a finger held on a badge or a pill.
    pub(super) fn md_ring(&self) -> Option<(i32, i32, f32)> {
        let h = self.studio.md.hold?;
        let f = ((self.now - h.at) / LONG_PRESS) as f32;
        ((h.x - h.x0).abs().max((h.y - h.y0).abs()) <= HOLD_SLOP && f > 0.2 && f < 1.0).then_some((h.x0, h.y0, f))
    }

    // ------------------------------------------------------------------------------------------------ touch

    /// A finger went down. True when a gesture of the badges, the pills or the Add measurement panel takes it.
    pub(super) fn md_down(&mut self, x: i32, y: i32) -> bool {
        self.studio.md.hold = None;
        self.studio.md.cand = None;
        self.studio.md.spring = None;
        self.studio.md.dn = None;
        // the Menu: the page scrolls with the finger
        if self.menu_open && self.menu == MenuId::Main && self.sheet_active() {
            if self.menu_geo().view.contains(x, y) {
                self.studio.drag = Some(StudioDrag::MenuScroll { y0: y, s0: self.sheet.scroll, moved: false });
                return true;
            }
            return false;
        }
        // a tile of the Add measurement panel: it follows the finger once it has moved
        if self.menu_open && self.menu == MenuId::MeasAdd && self.sheet_active() {
            let g = self.sheet_add_geo();
            if let Some(&(k, _)) =
                g.rows.iter().find(|(_, r)| r.contains(x, y) && !Self::tile_info_rect(*r).contains(x, y))
            {
                self.studio.drag = Some(StudioDrag::Tile { k, x0: x, y0: y, x, y, moved: false });
                return true;
            }
            return false;
        }
        if self.menu_open || self.studio.view == StudioView::Sim {
            return false;
        }
        // the Undo bars are buttons
        if let Some((bar, _)) = self.md_undo_bar() {
            if bar.contains(x, y) {
                return self.studio_eat(x, y);
            }
        }
        if let Some((bar, _)) = self.md_hide_bar() {
            if bar.contains(x, y) {
                return self.studio_eat(x, y);
            }
        }
        // a badge: a tap opens its popup, a hold picks it up
        if self.studio_badges_visible() && self.studio_row().contains(x, y) {
            if let Some((key, _)) = self.studio_badges().into_iter().find(|(_, r)| r.contains(x, y)) {
                self.studio.md.hold = Some(Hold { on: HoldOn::Badge(key), x0: x, y0: y, x, y, at: self.now });
                self.studio.md.dn = Some((HoldOn::Badge(key), x, y));
                return self.studio_eat(x, y);
            }
        }
        // a pill of the bottom bar (channel, ref, math): a tap selects it, a hold opens its quick actions, a swipe down
        // off the screen turns it off. The bar is clear of the panels, so this goes before their rules.
        if let Some(chip) = self.md_chip_at(x, y) {
            self.studio.md.hold = Some(Hold { on: chip, x0: x, y0: y, x, y, at: self.now });
            self.studio.md.dn = Some((chip, x, y));
            return self.studio_eat(x, y);
        }
        // the table: a sideways drag scrolls it. (The strip and the cards: a drag scrolls, a tap acts.)
        if self.studio_detailed() {
            if let Some(g) = self.studio_panel_geo().filter(|g| g.list.contains(x, y)) {
                // the columns fit, so there is no sideways scroll: a clear drag to the left swipes the row away instead
                let hmax = self.detail_hmax(g.list.w);
                if hmax > 0 {
                    self.studio.md.hdown = Some((x, y, self.studio.panel.hx.clamp(0, hmax)));
                } else if let Some((key, _)) = self.md_row_at(x, y) {
                    self.studio.md.cand = Some((key, x, y));
                }
            }
        }
        false
    }

    /// The finger moved. True when a gesture has it; the scroll of the list is left to the caller otherwise.
    pub(super) fn md_move(&mut self, x: i32, y: i32) -> bool {
        if let Some(h) = &mut self.studio.md.hold {
            h.x = x;
            h.y = y;
        }
        match self.studio.drag {
            Some(StudioDrag::Scroll { .. }) if self.studio.md.hdown.is_some() => {
                let (x0, y0, hx0) = self.studio.md.hdown.unwrap_or_default();
                if (x - x0).abs() > 8 && (x - x0).abs() > (y - y0).abs() {
                    self.studio.drag = Some(StudioDrag::HScroll { x0, hx0 });
                    self.studio.md.hdown = None;
                }
                false
            }
            Some(StudioDrag::HScroll { x0, hx0 }) => {
                let max = self.studio_panel_geo().map_or(0, |g| self.detail_hmax(g.list.w));
                self.studio.panel.hx = (hx0 - (x - x0)).clamp(0, max);
                true
            }
            Some(StudioDrag::Scroll { .. }) => {
                if let Some((key, x0, y0)) = self.studio.md.cand {
                    let (dx, dy) = (x - x0, y - y0);
                    if dx < -SWIPE_START && dx.abs() > dy.abs() {
                        self.studio.drag = Some(StudioDrag::Swipe { key, x0, y0 });
                        self.studio.md.cand = None;
                        self.studio.md.hold = None;
                        self.studio.md.swipe = Some((key, dx));
                        return true;
                    } else if dy.abs() > SWIPE_START && dy.abs() >= dx.abs() {
                        // gone vertical first: it is a scroll, and stays one
                        self.studio.md.cand = None;
                    }
                }
                false
            }
            Some(StudioDrag::Swipe { key, x0, .. }) => {
                self.studio.md.swipe = Some((key, (x - x0).min(0)));
                true
            }
            Some(StudioDrag::BadgeDrag { key, .. }) => {
                self.studio.drag = Some(StudioDrag::BadgeDrag { key, x, y });
                true
            }
            Some(StudioDrag::Tile { k, x0, y0, moved, .. }) => {
                // a mostly vertical drag scrolls the tiles; a sideways one carries the tile to a trace
                if let Some((sy0, s0)) = self.studio.md.tile_drag {
                    let max = self.tile_scroll_max();
                    self.studio.md.tile_scroll = (s0 - (y - sy0)).clamp(0, max);
                    self.studio.md.tile_scrolled = true;
                    return true;
                }
                if !moved && (y - y0).abs() > SWIPE_START && (y - y0).abs() > (x - x0).abs() {
                    self.studio.md.tile_drag = Some((y0, self.studio.md.tile_scroll));
                    return true;
                }
                let moved = moved || (x - x0).abs().max((y - y0).abs()) > SWIPE_START;
                self.studio.drag = Some(StudioDrag::Tile { k, x0, y0, x, y, moved });
                true
            }
            Some(StudioDrag::Eat { .. }) if self.studio.md.hold.is_some() => {
                // a badge or the Undo bar: a finger that leaves the hold is no hold
                if let Some(h) = self.studio.md.hold {
                    if (x - h.x0).abs().max((y - h.y0).abs()) > HOLD_SLOP {
                        self.studio.md.hold = None;
                    }
                }
                false
            }
            _ => false,
        }
    }

    /// The finger lifted at (x, y) ending `drag`. True when the lift was a gesture of this file.
    pub(super) fn md_up(&mut self, drag: StudioDrag, x: i32, y: i32) -> bool {
        self.studio.md.hold = None;
        self.studio.md.cand = None;
        self.studio.md.hdown = None;
        let dn = self.studio.md.dn.take();
        match drag {
            // a badge or a pill dragged down off the screen: removed / turned off, with the Undo bar
            StudioDrag::Eat { .. } if dn.is_some_and(|(_, x0, y0)| self.md_swiped_off(x0, y0, x, y)) => {
                self.md_swipe_down(dn.unwrap().0);
                true
            }
            StudioDrag::HScroll { .. } => true,
            StudioDrag::Swipe { key, x0, .. } => {
                let dx = (x - x0).min(0);
                self.studio.md.swipe = None;
                let w = self.studio_panel_geo().map_or(240, |g| g.list.w);
                if -dx >= (w * 2 / 5).max(80) {
                    self.md_remove(key);
                } else {
                    self.studio.md.spring = Some((key, dx as f32));
                }
                true
            }
            StudioDrag::BadgeDrag { key, .. } => {
                // off the strip: unpinned
                if !self.studio_row().contains(x, y) {
                    self.studio_set_pin(key, false);
                }
                true
            }
            StudioDrag::Tile { k, moved, .. } => {
                let scrolled = std::mem::take(&mut self.studio.md.tile_scrolled);
                self.studio.md.tile_drag = None;
                if scrolled {
                    return true;
                }
                if !moved {
                    return false;
                }
                self.md_drop_tile(k, x, y);
                true
            }
            _ => false,
        }
    }

    /// A drag that went mostly down and ended at the bottom edge of the screen (the finger cannot go further): off the
    /// screen. The pills are at the foot of the screen, so the way is short; a tap that wobbles is no swipe.
    pub(super) fn md_swiped_off(&self, x0: i32, y0: i32, x: i32, y: i32) -> bool {
        let (dx, dy) = (x - x0, y - y0);
        dy >= 12 && dy > dx.abs() && y >= self.scr.h - 12
    }

    /// The swipe down: a badge is removed, a channel, ref or math channel is turned off; the Undo bar brings it back.
    fn md_swipe_down(&mut self, on: HoldOn) {
        match on {
            HoldOn::Badge(key) => {
                self.md_remove(key);
                self.studio.md.src_undo = None;
            }
            HoldOn::Chan(c) => {
                if !self.m.ch[c].on {
                    return;
                }
                self.workspace_action(Action::Source(WaveId::Ch(c)));
                if self.m.ch[c].on {
                    return;
                }
                self.md_src_undo(on, None);
            }
            HoldOn::Ref(i) => {
                if !self.refs.get(i).and_then(Option::as_ref).is_some_and(|r| r.on) {
                    return;
                }
                self.workspace_action(Action::Source(WaveId::Ref(i)));
                if self.refs[i].as_ref().is_some_and(|r| r.on) {
                    return;
                }
                self.md_src_undo(on, None);
            }
            HoldOn::Math(k) => {
                let op = self.m.math_ch(k).op;
                if op == MathOp::Off {
                    return;
                }
                self.math_turn_off(k);
                self.md_src_undo(on, Some(op));
            }
        }
    }

    fn md_src_undo(&mut self, on: HoldOn, op: Option<MathOp>) {
        self.studio.md.undo = None;
        self.studio.md.src_undo = Some(SrcUndo { on, op, at: self.now });
    }

    /// Undo of a swipe down: the source is on again.
    pub(super) fn md_src_restore(&mut self, s: SrcUndo) {
        match s.on {
            HoldOn::Chan(c) => {
                self.activate_live_channel(c);
            }
            HoldOn::Ref(i) => {
                if let Some(r) = self.refs[i].as_mut() {
                    r.on = true;
                }
                self.select_workspace_source(WaveId::Ref(i));
            }
            HoldOn::Math(k) => {
                if let Some(op) = s.op {
                    if k == self.m.math_sel {
                        self.m.math = op;
                    } else {
                        self.m.maths[k].op = op;
                    }
                    self.settings_changed();
                }
            }
            _ => {}
        }
        self.relayout();
    }

    /// What the Undo bar says: the name of the removed source.
    fn md_src_label(&self, on: HoldOn) -> String {
        match on {
            HoldOn::Chan(c) => format!("CH{}", c + 1),
            HoldOn::Ref(i) => format!("R{}", i + 1),
            HoldOn::Math(k) => format!("M{}", k + 1),
            _ => String::new(),
        }
    }

    // ------------------------------------------------------------------------------------------------ tiles

    /// The channel a tile dropped at a point is for: its trace (the lane it is in), its badge in the lane, or its chip
    /// in the bottom bar.
    pub(super) fn md_tile_target(&self, x: i32, y: i32) -> Option<usize> {
        if let Some(&(c, _)) = self.studio.lane_badges.borrow().iter().find(|(_, r)| Self::hit40(*r).contains(x, y)) {
            return Some(c);
        }
        let b = self.studio_bottom();
        if let Some(c) = (0..n_ch()).find(|&c| b.ch[c].w > 0 && b.ch[c].contains(x, y)) {
            return Some(c);
        }
        let lane = self.plot.wins.iter().position(|a| a.contains(x, y))?;
        match self.trace_sources().get(lane)? {
            workspace::Trace::Channel(c) => Some(*c),
            _ => None,
        }
    }

    /// The rectangle that shows the channel under the finger: its lane, or its chip.
    fn md_tile_target_rect(&self, x: i32, y: i32) -> Option<(usize, Rect)> {
        let c = self.md_tile_target(x, y)?;
        if let Some(&(_, r)) =
            self.studio.lane_badges.borrow().iter().find(|(k, r)| *k == c && Self::hit40(*r).contains(x, y))
        {
            return Some((c, r));
        }
        let b = self.studio_bottom();
        if b.ch[c].w > 0 && b.ch[c].contains(x, y) {
            return Some((c, b.ch[c]));
        }
        let lane = self.plot.wins.iter().position(|a| a.contains(x, y))?;
        Some((c, self.plot.wins[lane]))
    }

    fn md_drop_tile(&mut self, k: usize, x: i32, y: i32) {
        let Some(c) = self.md_tile_target(x, y) else { return };
        let Some(cat) = self.sheet_add_entries().get(k).map(|e| e.cat) else { return };
        let id = WaveId::Ch(c);
        self.sheet_pick_source(id);
        match self.add_preview(cat) {
            Preview::Off(ch) => self.toast(&format!("Needs CH{} on", ch + 1)),
            Preview::Reason(s) => self.toast(&s),
            _ if self.add_is_on(cat) => self.toast(&format!("Already measured on CH{}", c + 1)),
            _ => self.sheet_add_toggle(cat),
        }
    }

    /// The bar that follows hiding the badges: where it is, and its Undo button.
    pub(super) fn md_hide_bar(&self) -> Option<(Rect, Rect)> {
        self.studio.md.hide_undo?;
        // not while the Display sheet is open: its switch, under the finger, is the undo. The bar shows when the sheet
        // has closed
        if self.menu_open && self.menu == MenuId::Display {
            return None;
        }
        let w = 300;
        let top = self.frame().top.bottom() + 6;
        let bar = Rect::new((self.scr.w - w) / 2, top, w, BAR_H);
        Some((bar, Rect::new(bar.right() - 92, bar.y, 92, bar.h)))
    }

    /// A tap on the Undo bar. True when it was on the bar (a tap on the button brings the row back).
    pub(super) fn md_undo_tap(&mut self, x: i32, y: i32) -> bool {
        if let Some((bar, button)) = self.md_hide_bar() {
            if bar.contains(x, y) {
                if button.contains(x, y) {
                    self.studio.hide_badges = false;
                    self.studio.md.hide_undo = None;
                    self.relayout();
                }
                return true;
            }
        }
        let Some((bar, button)) = self.md_undo_bar() else { return false };
        if !bar.contains(x, y) {
            return false;
        }
        if button.contains(x, y) {
            if let Some(s) = self.studio.md.src_undo.take() {
                self.md_src_restore(s);
            } else {
                self.md_undo();
            }
        }
        true
    }

    // ------------------------------------------------------------------------------------------------ drawing

    /// The Undo bars, and what follows a finger that drags a badge or a tile.
    pub(super) fn draw_md_overlays(&self, cv: &mut Canvas) {
        if self.m.layout != Layout::Studio {
            return;
        }
        let f = self.fonts();
        if let Some((bar, button)) = self.md_hide_bar() {
            cv.round(bar, 6, BTN_BORDER, 255);
            cv.round(bar.inset(1), 5, PANEL_BG, 255);
            cv.text_in(
                &f.s,
                Rect::new(bar.x + 10, bar.y, button.x - bar.x - 14, bar.h),
                0,
                "Measurements hidden",
                TEXT,
            );
            btn(cv, button.inset(4), BTN, ACCENT);
            cv.text_in(&f.mb, button, 1, "Undo", ACCENT);
        }
        let undo_of = self
            .studio
            .md
            .undo
            .map(|u| (self.md_key_label(u.key), u.at))
            .or_else(|| self.studio.md.src_undo.map(|s| (self.md_src_label(s.on), s.at)));
        if let (Some((bar, button)), Some((name, at))) = (self.md_undo_bar(), undo_of) {
            cv.round(bar, 6, BTN_BORDER, 255);
            cv.round(bar.inset(1), 5, PANEL_BG, 255);
            cv.set_clip(Rect::new(bar.x, bar.y, (button.x - bar.x).max(1), bar.h));
            cv.text_in(
                &f.s,
                Rect::new(bar.x + 10, bar.y, button.x - bar.x - 14, bar.h),
                0,
                &format!("Removed {name}"),
                TEXT,
            );
            cv.no_clip();
            // the time left, as a line along the bottom
            let left = (1.0 - ((self.now - at) / UNDO_S) as f32).clamp(0.0, 1.0);
            cv.fill(Rect::new(bar.x + 6, bar.bottom() - 4, ((bar.w - 12) as f32 * left) as i32, 2), ACCENT);
            btn(cv, button.inset(4), BTN, ACCENT);
            cv.text_in(&f.mb, button, 1, "Undo", ACCENT);
        }
        match self.studio.drag {
            Some(StudioDrag::BadgeDrag { key, x, y }) => {
                let off = !self.studio_row().contains(x, y);
                let label = self.md_key_label(key);
                let w = f.mb.width(&label) + 28;
                let tag = Rect::new((x - w / 2).clamp(0, self.scr.w - w), (y - 24).max(0), w, 36);
                cv.round(tag, 6, if off { RED } else { ACCENT }, 255);
                cv.round(tag.inset(1), 5, PANEL_BG, 245);
                cv.text_in(&f.mb, tag, 1, &label, TEXT);
                if off {
                    cv.text_in(&f.s, Rect::new(tag.x, tag.bottom() + 2, tag.w, 18), 1, "Unpin", RED);
                }
            }
            Some(StudioDrag::Tile { k, x, y, moved: true, .. }) => {
                if let Some((c, r)) = self.md_tile_target_rect(x, y) {
                    let col = CH_COLORS[c];
                    cv.blend(r, col, 40);
                    cv.rect(r, col);
                    cv.rect(r.inset(1), col);
                }
                let name = self.sheet_add_entries().get(k).map_or("", |e| e.name);
                let w = f.m.width(name) + 32;
                let tile = Rect::new((x - w / 2).clamp(0, self.scr.w - w), (y - 22).max(0), w, 44);
                cv.round(tile, 6, ACCENT, 255);
                cv.round(tile.inset(1), 5, PANEL_BG, 235);
                cv.text_in(&f.m, tile, 1, name, TEXT);
            }
            _ => {}
        }
    }

    /// "CH1 Vpp" for a message or a ghost.
    fn md_key_label(&self, key: MeasKey) -> String {
        match self.studio_index(key).and_then(|i| self.meas_rows().into_iter().nth(i)) {
            Some(r) => format!("{} {}", self.wave_name(r.src), r.name),
            None => match key {
                MeasKey::One(k, id) => format!("{} {}", self.wave_name(id), super::meascatalog::meas_display_name(k)),
                _ => "measurement".to_string(),
            },
        }
    }
}
