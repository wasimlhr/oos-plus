// SPDX-License-Identifier: GPL-2.0-or-later
//! Batch 12: gestures of the Studio measurements. A row of the table swiped left is removed with an Undo bar; a held
//! badge dragged off the strip unpins; a tile of the Add measurement panel dragged onto a channel adds the
//! measurement for it. (The list of one measurement a row, with its drag to reorder and its row menu, is gone: the
//! first view is the strip of cells, batch 17.)
//! OOS_SHOTS=<dir> writes pictures (b12-*).
use super::measdrag::UNDO_S;
use super::studio::{MeasKey, StudioView};
use super::*;

fn studio() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.studio_detail = false;
    u.m.studio_zoom_tab = false;
    u.m.ch[1].on = true;
    u.studio.enter = true;
    for _ in 0..10 {
        u.step(0.05);
    }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    u.studio_set_view(StudioView::Docked);
    frame(&mut u);
    u
}

/// The panel with the table: the view whose rows are swiped.
fn table() -> Ui {
    let mut u = studio();
    u.studio_set_detailed(true);
    frame(&mut u);
    u
}

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    u.step(0.02);
}

fn shot(u: &mut Ui, name: &str) {
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("b12-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
}

fn touch(u: &mut Ui, phase: Phase, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase, x, y });
    u.step(0.01);
}

fn moves(u: &mut Ui, a: (i32, i32), b: (i32, i32)) {
    for k in 1..=8 {
        touch(u, Phase::Move, a.0 + (b.0 - a.0) * k / 8, a.1 + (b.1 - a.1) * k / 8);
    }
}

fn drag(u: &mut Ui, a: (i32, i32), b: (i32, i32)) {
    touch(u, Phase::Down, a.0, a.1);
    moves(u, a, b);
    touch(u, Phase::Up, b.0, b.1);
    frame(u);
}

fn tap(u: &mut Ui, x: i32, y: i32) {
    touch(u, Phase::Down, x, y);
    touch(u, Phase::Up, x, y);
    frame(u);
}

/// The middle of the left part of row `k` of the page.
fn row_pt(u: &Ui, k: usize) -> (i32, i32) {
    let g = u.studio_panel_geo().expect("the docked list");
    (g.list.x + 70, g.list.y + k as i32 * u.studio_panel_row() + u.studio_panel_row() / 2)
}

fn keys(u: &Ui) -> Vec<MeasKey> {
    u.studio_keys()
}

/// Holds a finger on `a` until the ring has run out, then moves to `b`; the finger is still down.
fn hold_to(u: &mut Ui, a: (i32, i32), b: (i32, i32)) {
    touch(u, Phase::Down, a.0, a.1);
    u.step(0.6);
    moves(u, a, b);
}

// ------------------------------------------------------------------------------------------------ swipe, undo

#[test]
fn a_row_swiped_left_is_removed_and_the_undo_bar_brings_it_back_in_place() {
    let mut u = table();
    let before = keys(&u);
    assert!(before.len() >= 4, "{before:?}");
    let pinned = u.studio_pinned();
    let victim = before[1];
    let a = row_pt(&u, 1);
    drag(&mut u, (a.0 + 100, a.1), (a.0 - 60, a.1));
    let after = keys(&u);
    assert_eq!(after.len(), before.len() - 1, "removed");
    assert!(!after.contains(&victim));
    let (bar, button) = u.md_undo_bar().expect("the Undo bar");
    assert!(button.h >= 40 && button.w >= 40, "40 px target: {button:?}");
    shot(&mut u, "undo-bar");
    // Undo: the same row at the same place, pinned as before
    tap(&mut u, button.x + button.w / 2, bar.y + bar.h / 2);
    assert_eq!(keys(&u), before, "the order is the same");
    assert_eq!(u.studio_pinned(), pinned);
    assert!(u.md_undo_bar().is_none());
}

#[test]
fn the_undo_bar_goes_after_about_four_seconds() {
    let mut u = table();
    let n = keys(&u).len();
    let a = row_pt(&u, 0);
    drag(&mut u, (a.0 + 100, a.1), (a.0 - 60, a.1));
    assert!(u.md_undo_bar().is_some());
    u.step(UNDO_S - 1.0);
    assert!(u.md_undo_bar().is_some(), "still there after 3 s");
    u.step(1.5);
    assert!(u.md_undo_bar().is_none(), "gone after 4 s");
    assert_eq!(keys(&u).len(), n - 1);
}

#[test]
fn a_short_swipe_springs_back_and_removes_nothing() {
    let mut u = table();
    let before = keys(&u);
    let a = row_pt(&u, 2);
    touch(&mut u, Phase::Down, a.0 + 100, a.1);
    moves(&mut u, (a.0 + 100, a.1), (a.0 + 50, a.1));
    let (_, off) = u.studio.md.swipe.expect("the row follows the finger");
    assert!(off <= -40, "{off}");
    shot(&mut u, "mid-swipe");
    touch(&mut u, Phase::Up, a.0 + 50, a.1);
    assert_eq!(keys(&u), before);
    assert!(u.md_undo_bar().is_none());
    assert!(u.studio.md.spring.is_some(), "springs back");
    for _ in 0..30 {
        frame(&mut u);
    }
    assert!(u.studio.md.spring.is_none() && u.studio.md.swipe.is_none());
}

#[test]
fn a_tap_on_a_row_opens_its_settings_and_does_not_swipe() {
    let mut u = studio();
    let before = keys(&u);
    let a = row_pt(&u, 0);
    // a few pixels of wobble stay a tap
    touch(&mut u, Phase::Down, a.0, a.1);
    touch(&mut u, Phase::Move, a.0 - 6, a.1 + 2);
    touch(&mut u, Phase::Up, a.0 - 6, a.1 + 2);
    frame(&mut u);
    assert_eq!(keys(&u), before);
    assert!(u.meas_own.dialog.is_some() || u.menu_open, "the settings opened");
    assert!(u.studio.md.swipe.is_none());
}

#[test]
fn a_vertical_drag_scrolls_the_list_and_never_swipes() {
    let mut u = table();
    // a list longer than its page
    for k in [
        Meas::Vmax,
        Meas::Vmin,
        Meas::Vamp,
        Meas::Vrms,
        Meas::Vavg,
        Meas::Period,
        Meas::Area,
        Meas::Rise,
        Meas::Fall,
        Meas::PWidth,
        Meas::NWidth,
    ] {
        for c in 0..2 {
            let _ = u.meas_add(k, WaveId::Ch(c));
        }
    }
    frame(&mut u);
    let g = u.studio_panel_geo().unwrap();
    let rows = u.meas_rows();
    assert!(u.studio_listed(&rows).len() > g.page, "more rows than a page");
    let before = keys(&u);
    let first = u.studio.panel.first;
    let a = row_pt(&u, 3);
    // mostly vertical, a little sideways
    touch(&mut u, Phase::Down, a.0, a.1);
    moves(&mut u, a, (a.0 - 30, a.1 - 120));
    assert!(u.studio.md.swipe.is_none(), "no swipe");
    touch(&mut u, Phase::Up, a.0 - 30, a.1 - 120);
    frame(&mut u);
    assert_eq!(keys(&u), before, "nothing removed");
    assert!(u.studio.panel.first > first, "scrolled: {} -> {}", first, u.studio.panel.first);
}

// ------------------------------------------------------------------------------------------------ pin, unpin

#[test]
fn a_held_row_dropped_on_the_badge_strip_is_pinned() {
    let mut u = studio();
    let key = keys(&u)[4];
    // a measurement is pinned from the moment it is added: this one is taken off first
    assert!(u.studio_is_pinned(key), "pinned by default");
    u.studio_set_pin(key, false);
    assert!(!u.studio_is_pinned(key), "not pinned now");
    // batch 22: with the panel open there is no strip under the lanes (the panel lists them), so a row is pinned by its pin
    assert_eq!(u.studio_row_h(), 0, "no row while the panel is open");
    u.studio_set_pin(key, true);
    assert!(u.studio_is_pinned(key), "pinned");
    u.studio_set_view(StudioView::Waveform);
    frame(&mut u);
    assert!(u.studio_row_h() > 0, "the row is back for a closed panel");
}

#[test]
fn a_held_badge_dragged_off_the_strip_is_unpinned() {
    let mut u = studio();
    u.studio_set_view(StudioView::Waveform); // batch 22: the row is for a closed panel
    frame(&mut u);
    let (key, r) = u.studio_badges().into_iter().next().expect("a badge");
    assert!(u.studio_is_pinned(key));
    let a = (r.x + r.w / 2, r.y + r.h / 2);
    let list = (300, 150); // the plot
    hold_to(&mut u, a, list);
    assert!(matches!(u.studio.drag, Some(super::studio::StudioDrag::BadgeDrag { .. })));
    touch(&mut u, Phase::Up, list.0, list.1);
    frame(&mut u);
    assert!(!u.studio_is_pinned(key), "unpinned");
    // a tap on a badge still opens its popup
    let (key, r) = u.studio_badges().into_iter().next().unwrap();
    tap(&mut u, r.x + r.w / 2, r.y + r.h / 2);
    assert_eq!(u.studio.popup, Some(key));
}

#[test]
fn a_badge_dropped_on_the_strip_stays_pinned() {
    let mut u = studio();
    let (key, r) = u.studio_badges().into_iter().next().unwrap();
    let a = (r.x + r.w / 2, r.y + r.h / 2);
    hold_to(&mut u, a, (a.0 + 20, a.1));
    touch(&mut u, Phase::Up, a.0 + 20, a.1);
    frame(&mut u);
    assert!(u.studio_is_pinned(key));
}

// ------------------------------------------------------------------------------------------------ a tile onto a channel

#[test]
fn a_tile_dragged_onto_a_channel_adds_the_measurement_for_that_channel() {
    let mut u = studio();
    u.open_meas_list();
    frame(&mut u);
    assert!(u.menu_open && u.menu == MenuId::MeasAdd);
    let g = u.sheet_add_geo();
    // an entry that is not on any channel yet
    let &(k, row) = g.rows.iter().find(|(k, _)| !u.add_is_on(u.sheet_add_entries()[*k].cat)).expect("a tile");
    let before = keys(&u);
    // the lane of CH2
    let lane = u.trace_sources().iter().position(|t| *t == workspace::Trace::Channel(1)).expect("CH2 has a lane");
    let a = u.plot.wins[lane];
    let to = (a.x + a.w / 2, a.y + a.h / 2);
    let from = (row.x + 40, row.y + row.h / 2);
    touch(&mut u, Phase::Down, from.0, from.1);
    moves(&mut u, from, to);
    assert_eq!(u.md_tile_target(to.0, to.1), Some(1));
    shot(&mut u, "tile-ghost");
    touch(&mut u, Phase::Up, to.0, to.1);
    frame(&mut u);
    let after = keys(&u);
    let _ = k;
    assert_eq!(after.len(), before.len() + 1, "one measurement added");
    let new = after.iter().find(|k| !before.contains(k)).copied().unwrap();
    let src = match new {
        MeasKey::One(_, id) | MeasKey::Pair(_, id, _) | MeasKey::Cat(_, id, _, _, _) => id,
    };
    assert_eq!(src, WaveId::Ch(1), "for CH2");
}

#[test]
fn a_tap_on_a_tile_still_adds_it() {
    let mut u = studio();
    u.open_meas_list();
    frame(&mut u);
    let g = u.sheet_add_geo();
    let &(k, row) = g.rows.iter().find(|(k, _)| !u.add_is_on(u.sheet_add_entries()[*k].cat)).unwrap();
    let _ = k;
    let n = keys(&u).len();
    tap(&mut u, row.x + 40, row.y + row.h / 2);
    assert_eq!(keys(&u).len(), n + 1);
}

// ------------------------------------------------------------------------------------------------ narrow views

#[test]
fn a_narrow_strip_has_a_short_title_and_a_narrow_footer_does_not_overlap() {
    let u = studio();
    let full = u.dock_name(super::dock::VKind::Table);
    let long = u.dock_title_fit(super::dock::VKind::Table, "Search events", 400);
    assert_eq!(long, "Search events");
    let short = u.dock_title_fit(super::dock::VKind::Table, "Search events", u.fonts().lb.width("Search") + 2);
    assert_eq!(short, "Search");
    let tiny = u.dock_title_fit(super::dock::VKind::Table, "Search events", u.fonts().lb.width("Sea") + 2);
    assert!(tiny.ends_with('\u{2026}') && u.fonts().lb.width(&tiny) <= u.fonts().lb.width("Sea") + 2, "{tiny}");
    assert_eq!(u.dock_title_fit(super::dock::VKind::Measure, "Measurements", 60), "Meas");
    let _ = full;
    // the footer of a narrow measurements panel
    let mut u = studio();
    let d = u.dock_layout().dividers.iter().find(|d| d.dir == super::dock::Dir::H).cloned().unwrap();
    let (x, y) = (d.line.x + d.line.w / 2, d.line.y + d.line.h / 2);
    drag(&mut u, (x, y), (x + 300, y));
    let g = u.studio_panel_geo().unwrap();
    assert!(g.p.w < super::studio::NARROW_PANEL_W, "narrow: {}", g.p.w);
    assert!(u.panel_bar(g.p).more.is_some(), "what does not fit is behind the dots");
    shot(&mut u, "narrow-panel");
}
