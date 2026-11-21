// SPDX-License-Identifier: GPL-2.0-or-later
//! The Studio measurements panel with content first: the table is the default view, the chrome sits in the title bar
//! (or one slim toolbar when it is narrow), the list scrolls by swipe, the pin is a thumbtack.
//! OOS_SHOTS=<dir> writes pictures (b32-*).
use super::dock::{Drop, Side, VKind};
use super::measpanel::PanelView;
use super::studio::StudioView;
use super::*;

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    u.step(0.02);
}

fn studio_with(n: usize) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.studio.enter = true;
    for _ in 0..10 { u.step(0.05); }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    u.studio_set_view(StudioView::Docked);
    frame(&mut u);
    let all = [(Meas::Vpp, WaveId::Ch(0)), (Meas::Freq, WaveId::Ch(0)), (Meas::Vamp, WaveId::Ch(1)), (Meas::Freq, WaveId::Ch(1)), (Meas::Rise, WaveId::Ch(0)), (Meas::Vrms, WaveId::Ch(0)),
        (Meas::Vmax, WaveId::Ch(0)), (Meas::Vmin, WaveId::Ch(0)), (Meas::Fall, WaveId::Ch(0)), (Meas::Period, WaveId::Ch(0)), (Meas::Vmax, WaveId::Ch(1)), (Meas::Vmin, WaveId::Ch(1))];
    u.m.meas = all.iter().take(n).copied().collect();
    u.ensure_meas_own();
    for _ in 0..6 { frame(&mut u); }
    u
}

fn studio_bottom(n: usize) -> Ui {
    let mut u = studio_with(n);
    u.dock_apply(VKind::Measure, Drop::Split(VKind::Wave, Side::Bottom));
    for _ in 0..6 { frame(&mut u); }
    u
}


// ---------------------------------------------------------------------------------------------------- helpers

fn shot(u: &mut Ui, name: &str) {
    for _ in 0..6 { frame(u); }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("b32-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
}

fn touch(u: &mut Ui, phase: Phase, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase, x, y });
    u.step(0.01);
}

fn tap_at(u: &mut Ui, x: i32, y: i32) {
    touch(u, Phase::Down, x, y);
    touch(u, Phase::Up, x, y);
    frame(u);
}

fn tap(u: &mut Ui, r: Rect) { tap_at(u, r.x + r.w / 2, r.y + r.h / 2); }

fn drag(u: &mut Ui, a: (i32, i32), b: (i32, i32)) {
    touch(u, Phase::Down, a.0, a.1);
    for k in 1..=8 { touch(u, Phase::Move, a.0 + (b.0 - a.0) * k / 8, a.1 + (b.1 - a.1) * k / 8); }
    touch(u, Phase::Up, b.0, b.1);
}

/// The rows of the table that are whole in the list.
fn full_rows(u: &Ui) -> usize {
    let g = u.studio_panel_geo().unwrap();
    let n = u.studio_listed(&u.meas_rows()).len();
    u.detail_layout(&g, n, u.studio_first(n, g.page)).iter().filter(|(_, r)| r.y >= g.list.y && r.bottom() <= g.list.bottom()).count()
}

fn with_dock(text: &str, n: usize) -> Ui {
    let mut u = studio_with(n);
    u.dock_restore(text);
    u.dock_changed();
    for _ in 0..6 { frame(&mut u); }
    u
}

fn item_rect(u: &Ui, it: super::measpanel::BarItem) -> Rect {
    u.panel_bar(u.studio_panel_geo().unwrap().p).slots.iter().find(|s| s.item == it).expect("a control of the bar").r
}

// ---------------------------------------------------------------------------------------------------- tests

#[test]
fn the_table_is_the_first_and_default_view_and_the_switch_reads_table_list_cards() {
    let mut u = studio_with(6);
    assert_eq!(u.panel_view(), PanelView::Detailed);
    assert!(u.studio_detailed());
    let r = u.dock_rect(VKind::Measure).and_then(|r| u.dock_view_rect(VKind::Measure, r)).expect("the view switch");
    let seg = |k: i32| (r.x + (2 * k + 1) * r.w / 6, r.y + r.h / 2);
    let (x, y) = seg(1);
    tap_at(&mut u, x, y);
    assert_eq!(u.panel_view(), PanelView::List);
    let (x, y) = seg(2);
    tap_at(&mut u, x, y);
    assert_eq!(u.panel_view(), PanelView::Cards);
    let (x, y) = seg(0);
    tap_at(&mut u, x, y);
    assert_eq!(u.panel_view(), PanelView::Detailed);
}

#[test]
fn the_default_view_survives_the_saved_setup() {
    assert!(Model::default().studio_detail);
    let mut l = Model::default();
    l.studio_detail = false;
    let text = crate::store::setup_text(&l);
    assert!(text.contains("studio_meas_view"), "a chosen List is written down");
    assert!(!crate::store::parse_setup(&text).expect("it reads back").studio_detail);
    let d = crate::store::setup_text(&Model::default());
    assert!(crate::store::parse_setup(&d).expect("it reads back").studio_detail);
}

#[test]
fn rows_visible_at_the_bottom_and_right_dock_sizes() {
    // the sizes of the owner's screenshots: bottom 1024 x 260, right 270 wide
    let mut u = studio_bottom(12);
    let r = u.dock_rect(VKind::Measure).unwrap();
    let g = u.studio_panel_geo().unwrap();
    assert_eq!(u.panel_view(), PanelView::Detailed);
    assert!(r.h <= 262 && r.w >= 1000, "{r:?}");
    assert!(full_rows(&u) >= 5, "bottom dock {r:?} shows {}", full_rows(&u));
    assert_eq!((g.up.w, g.down.w), (0, 0), "no pager");
    assert!(super::measview::DETAIL_ROW <= 28 && super::measview::COL_HEAD <= 24);
    eprintln!("ROWS bottom {r:?} {}", full_rows(&u));
    shot(&mut u, "bottom-dock");

    let mut u = studio_with(12);
    let r = u.dock_rect(VKind::Measure).unwrap();
    assert!(r.w <= 272);
    assert!(full_rows(&u) >= 8, "right dock {r:?} shows {}", full_rows(&u));
    eprintln!("ROWS right {r:?} {}", full_rows(&u));
    shot(&mut u, "right-dock");
}

#[test]
fn the_panel_at_990_by_230_shows_at_least_five_rows() {
    let mut u = studio_with(12);
    u.dock_apply(VKind::Measure, Drop::Float(Rect::new(17, 330, 990, 230)));
    for _ in 0..6 { frame(&mut u); }
    let r = u.dock_rect(VKind::Measure).unwrap();
    assert_eq!((r.w, r.h), (990, 230), "{r:?}");
    assert!(full_rows(&u) >= 5, "{} rows", full_rows(&u));
    eprintln!("ROWS 990x230 {}", full_rows(&u));
    shot(&mut u, "bottom-dock-990x230");
}

#[test]
fn a_narrow_dock_keeps_name_and_value_and_puts_the_rest_of_the_controls_behind_the_dots() {
    // about 230 px wide
    let mut u = with_dock("H0.780(W,M)", 12);
    let r = u.dock_rect(VKind::Measure).unwrap();
    assert!(r.w < 260, "{r:?}");
    let g = u.studio_panel_geo().unwrap();
    let (cols, _) = Ui::detail_cols(g.list.w);
    let names: Vec<usize> = cols.iter().map(|c| c.0).collect();
    assert!(names.contains(&1) && names.contains(&2), "{names:?}");
    let bar = u.panel_bar(g.p);
    assert!(bar.more.is_some() && !bar.hidden.is_empty(), "controls overflow into the dots");
    assert!(!bar.wide);
    assert!(full_rows(&u) >= 8, "{}", full_rows(&u));
    eprintln!("ROWS narrow {r:?} {} columns {names:?}", full_rows(&u));
    let more = bar.more.unwrap().r;
    tap(&mut u, more);
    assert_eq!(u.studio.panel.drop, Some(super::measpanel::PanelDrop::More));
    shot(&mut u, "narrow-dock");
    let p = u.panel_drop_rect().unwrap();
    let first = bar.hidden[0];
    tap(&mut u, Ui::panel_drop_row(p, 0));
    // the gate is in the list with its two choices: the first is the full record, taken with that one tap
    if first == super::measpanel::BarItem::Gate { assert!(u.m.meas_full_record && u.studio.panel.drop.is_none()); }
}

#[test]
fn the_wide_bar_sits_in_the_title_strip_and_search_replaces_the_title() {
    let mut u = studio_bottom(8);
    let g = u.studio_panel_geo().unwrap();
    let bar = u.panel_bar(g.p);
    assert!(bar.wide && bar.more.is_none() && bar.slots.len() == 6, "all six controls fit the strip");
    assert!(bar.slots.iter().all(|s| s.r.bottom() <= g.p.y), "inside the title strip");
    assert_eq!(g.list.y, g.p.y + super::measview::COL_HEAD, "no row of chrome before the table head");
    let s = item_rect(&u, super::measpanel::BarItem::Search);
    tap(&mut u, s);
    assert!(u.studio.panel.searching && u.wlan.osk.is_some());
    assert!(u.panel_bar(g.p).field.is_some());
    u.wlan.osk = None;
    u.panel_set_query("freq");
    assert!(u.panel_caption().is_some());
    shot(&mut u, "search-open");
    let close = u.panel_bar(g.p).field.unwrap().1;
    tap(&mut u, close);
    assert!(!u.studio.panel.searching && u.studio.panel.query.is_empty());
    assert!(u.panel_caption().is_none(), "no caption while nothing filters");
}

#[test]
fn a_swipe_scrolls_the_list_and_a_flick_keeps_it_moving() {
    let mut u = studio_bottom(12);
    let g = u.studio_panel_geo().unwrap();
    let n = u.studio_listed(&u.meas_rows()).len();
    assert!(n > full_rows(&u));
    assert_eq!(u.panel_scroll_px(), 0);
    let (x, y) = (g.list.x + 60, g.list.y + g.list.h - 30);
    // a slow drag (the finger rests before it lifts): the list follows by pixels
    touch(&mut u, Phase::Down, x, y);
    for k in 1..=6 { touch(&mut u, Phase::Move, x, y - 7 * k); }
    u.step(0.4);
    touch(&mut u, Phase::Up, x, y - 42);
    assert!(u.panel_scroll_px() > 0 && u.panel_scroll_px() <= 42, "{}", u.panel_scroll_px());
    assert_eq!(u.studio.panel.vel, 0.0, "a resting finger does not fling");
    // a quick flick coasts on after the lift
    let at = u.panel_scroll_px();
    drag(&mut u, (x, y), (x, y - 40));
    let lifted = u.panel_scroll_px();
    assert!(lifted > at);
    for _ in 0..10 { u.step(0.016); }
    assert!(u.panel_scroll_px() > lifted, "it coasts");
    for _ in 0..300 { u.step(0.016); }
    assert_eq!(u.studio.panel.vel, 0.0, "and it comes to rest");
    // the other way brings it back to the top, and the top stops it
    for _ in 0..6 { drag(&mut u, (x, y - 90), (x, y + 10)); for _ in 0..300 { u.step(0.016); } }
    assert_eq!(u.panel_scroll_px(), 0);
    // the wheel
    u.handle(Ev::Wheel { x, y, steps: -2, ctrl: false, shift: false, alt: false });
    assert!(u.panel_scroll_px() > 0, "the wheel scrolls");
    shot(&mut u, "scrolled");
}

#[test]
fn a_swipe_left_removes_a_row_and_a_vertical_drag_only_scrolls() {
    let mut u = studio_bottom(12);
    let g = u.studio_panel_geo().unwrap();
    let before = u.m.meas.len();
    let r = u.detail_layout(&g, before, 0)[1].1;
    let (x, y) = (r.x + r.w - 80, r.y + r.h / 2);
    // mostly vertical with a little drift to the left: a scroll
    drag(&mut u, (x, y), (x - 12, y - 60));
    frame(&mut u);
    assert_eq!(u.m.meas.len(), before, "a scroll removes nothing");
    assert!(u.panel_scroll_px() > 0);
    let mut u = studio_bottom(12);
    let g = u.studio_panel_geo().unwrap();
    let r = u.detail_layout(&g, before, 0)[1].1;
    let (x, y) = (r.x + r.w - 20, r.y + r.h / 2);
    // a clear swipe to the left across most of the row: removed, with the Undo bar
    drag(&mut u, (x, y), (x - 480, y + 3));
    frame(&mut u);
    assert_eq!(u.m.meas.len(), before - 1);
    assert!(u.studio.md.undo.is_some());
    assert_eq!(u.panel_scroll_px(), 0, "a swipe does not scroll");
    // a short swipe springs back
    let g = u.studio_panel_geo().unwrap();
    let n = u.m.meas.len();
    let r = u.detail_layout(&g, n, 0)[0].1;
    drag(&mut u, (r.right() - 20, r.y + 10), (r.right() - 60, r.y + 10));
    for _ in 0..30 { frame(&mut u); }
    assert_eq!(u.m.meas.len(), n);
}

#[test]
fn the_thumbtack_toggles_the_pin_and_is_filled_when_pinned() {
    let mut u = studio_with(6);
    let g = u.studio_panel_geo().unwrap();
    let n = u.studio_listed(&u.meas_rows()).len();
    let row = u.detail_layout(&g, n, 0)[2].1;
    let key = u.studio_keys()[u.studio_listed(&u.meas_rows())[2]];
    let was = u.studio_is_pinned(key);
    // the table has no thumbtack (it took the end of every name, in a panel 320 px wide): where it was, a tap opens
    // the settings of the row as anywhere else on it, and the pin stays
    let (cols, _) = Ui::detail_cols(g.list.w);
    let name = Ui::detail_cell(row, cols[1].1, cols[1].2, 0);
    tap_at(&mut u, name.right() - 10, row.y + row.h / 2);
    assert_eq!(u.studio_is_pinned(key), was);
    assert!(u.meas_cfg.is_some(), "the tap opened no settings");
    u.close_menu();
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    cv.au_start();
    u.render(&mut cv);
    // the cards have it: a tap on the tack of a card pins and unpins
    u.m.studio_detail = false;
    u.studio.panel.cards = true;
    u.relayout();
    frame(&mut u);
    let g = u.studio_panel_geo().unwrap();
    let (pos, card) = u.panel_cards_layout(0, n, g.list)[2];
    let key = u.studio_keys()[u.studio_listed(&u.meas_rows())[pos]];
    let was = u.studio_is_pinned(key);
    let pin = Ui::panel_card_pin(card);
    tap(&mut u, pin);
    assert_eq!(u.studio_is_pinned(key), !was);
    tap(&mut u, pin);
    assert_eq!(u.studio_is_pinned(key), was);
    u.m.studio_detail = true;
    u.studio.panel.cards = false;
    u.relayout();
    frame(&mut u);
    // the picture: a filled tack covers more pixels than an outline one, and both stay in their 12 x 17 box
    let ink = |filled: bool| {
        let mut cv = Canvas::new(40, 40);
        let bg = cv.px[0];
        super::studio::icon_pin(&mut cv, 10, 20, filled, 0xFFFFFF);
        let mut k = 0;
        for y in 0..40 { for x in 0..40 {
            if cv.px[(y * 40 + x) as usize] != bg { k += 1; assert!((10..22).contains(&x) && (11..29).contains(&y), "({x},{y}) is outside the glyph box"); }
        } }
        k
    };
    assert!(ink(true) > ink(false) + 20, "filled {} outline {}", ink(true), ink(false));
    // the control in the bar lights up and filters
    let r = item_rect(&u, super::measpanel::BarItem::Pinned);
    tap(&mut u, r);
    assert!(u.studio.panel.pinned_only);
}

#[test]
fn default_and_minimal_layouts_do_not_see_the_new_panel() {
    for layout in [Layout::Default, Layout::Minimal] {
        let mut a = Ui::new(Source::Demo { t: 0.0 });
        a.m.layout = layout;
        for _ in 0..8 { frame(&mut a); }
        assert!(!a.studio_detailed() && a.studio_panel_geo().is_none(), "{layout:?}");
        let mut cva = Canvas::new(W, H);
        a.render(&mut cva);
        // the Studio settings change nothing there
        a.m.studio_detail = !a.m.studio_detail;
        a.studio.panel.searching = true;
        a.studio.panel.pinned_only = true;
        let mut cvb = Canvas::new(W, H);
        a.render(&mut cvb);
        assert!(cva.px == cvb.px, "{layout:?} draws the same");
    }
}

#[test]
fn zoom_on_with_the_panel_at_the_bottom_shows_six_rows_and_the_zoom_row() {
    let mut u = studio_bottom(12);
    u.m.studio_zoom_tab = false;
    u.m.zoom = true;
    for _ in 0..8 { frame(&mut u); }
    let row = u.zoom_readout();
    assert_eq!(row.h, 32, "the zoom row has the height of the Default row");
    let bar = u.studio_zoom_bar().expect("the zoom bar");
    for r in [bar.label, bar.plus, bar.minus, bar.fit, bar.close] { assert!(r.y >= row.y && r.bottom() <= row.bottom(), "{r:?} in {row:?}"); }
    assert!(full_rows(&u) >= 6, "{} rows", full_rows(&u));
    // the split stays resizable: the divider under the overview moves the zoom area
    let (hit, handle) = u.ov_divider().expect("the overview divider");
    let before = u.studio_ov_h(u.grid);
    let (x, y) = (handle.x + handle.w / 2, hit.y + hit.h / 2);
    drag(&mut u, (x, y), (x, y + 24));
    frame(&mut u);
    assert!(u.studio_ov_h(u.grid) > before, "dragging the divider changes the overview height");
    let (hit, handle) = u.ov_divider().unwrap();
    let (x, y) = (handle.x + handle.w / 2, hit.y + hit.h / 2);
    drag(&mut u, (x, y), (x, y - 24));
    shot(&mut u, "zoom-plus-meas");
}

#[test]
fn the_comparison_picture_of_rows_visible_before_and_after() {
    // before (the old panel at the same sizes, measured at HEAD 087d51f): table view bottom 1, right 8; list view bottom 1, right 5
    let u = studio_with(12);
    let right = full_rows(&u);
    let bottom = full_rows(&studio_bottom(12));
    let mut cv = Canvas::new(520, 260);
    cv.fill(Rect::new(0, 0, 520, 260), BG);
    let f = u.fonts();
    cv.text(&f.lb, 14, 26, "Measurement rows visible (whole rows)", TEXT);
    let bars = [("Bottom dock 1024x260", 1, bottom as i32), ("Right dock 270 wide", 8, right as i32)];
    for (i, (name, before, after)) in bars.iter().enumerate() {
        let y = 50 + i as i32 * 100;
        cv.text(&f.m, 14, y + 12, name, TEXT);
        cv.fill(Rect::new(14, y + 22, before * 28, 20), DIM);
        cv.text(&f.m, 14 + before * 28 + 8, y + 38, &format!("before {before}"), TEXT);
        cv.fill(Rect::new(14, y + 50, after * 28, 20), ACCENT);
        cv.text(&f.m, 14 + after * 28 + 8, y + 66, &format!("after {after}"), TEXT);
    }
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("b32-before-after.png"), crate::store::png_of(&cv)).unwrap();
    }
    assert!(bottom >= 5 && right >= 8);
}

// ---------------------------------------------------------------------------------------------------- follow-ups (b32b)

fn shotb(u: &mut Ui, name: &str) {
    for _ in 0..6 { frame(u); }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("b32b-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
}

fn zoomed(tab: bool) -> Ui {
    let mut u = studio_bottom(8);
    u.m.studio_zoom_tab = tab;
    u.set_zoom(true);
    for _ in 0..8 { frame(&mut u); }
    u
}

#[test]
fn the_top_bar_has_one_add_button_and_its_menu_starts_with_measurement() {
    let mut u = studio_with(6);
    let t = u.studio_top();
    assert_eq!(t.measure.w, 0, "no Meas button");
    assert!(t.add.w >= 40 && t.add.h >= 24);
    assert!(u.studio_acq_caption().len() > 0);
    tap(&mut u, t.add);
    assert!(u.studio.add_menu);
    let rows = u.studio_add_rows();
    assert_eq!(rows[0].0, super::addmenu::AddRow::Measurement, "Measurement is first");
    assert!(rows.iter().all(|(_, r)| r.h >= 24 && r.right() <= W && r.bottom() <= H));
    let have: Vec<_> = rows.iter().map(|r| r.0).collect();
    for want in [super::addmenu::AddRow::ResultsTable, super::addmenu::AddRow::Plot, super::addmenu::AddRow::Cursors, super::addmenu::AddRow::Search,
        super::addmenu::AddRow::Math, super::addmenu::AddRow::Reference, super::addmenu::AddRow::Bus, super::addmenu::AddRow::Zoom, super::addmenu::AddRow::ZoomBox] {
        assert!(have.contains(&want), "{want:?}");
    }
    shotb(&mut u, "add-menu");
    // Measurement opens the Add measurements sheet
    let r = rows[0].1;
    tap(&mut u, r);
    assert!(!u.studio.add_menu && u.menu_open && u.menu == MenuId::MeasAdd);
    u.close_menu();
    // Results table opens the page of the tables, and Add there shows the panel
    u.studio_set_view(StudioView::Waveform);
    frame(&mut u);
    tap(&mut u, t.add);
    let r = u.studio_add_rows().into_iter().find(|(k, _)| *k == super::addmenu::AddRow::ResultsTable).unwrap().1;
    tap(&mut u, r);
    assert!(u.menu_open && u.menu == MenuId::AddPanel && !u.studio_panel_shown());
    let add = u.rail_geo().foot[0].1;
    tap(&mut u, add);
    assert!(u.studio_panel_shown());
    u.close_menu();
    // a tap outside closes it
    tap(&mut u, t.add);
    assert!(u.studio.add_menu);
    tap_at(&mut u, 500, 300);
    assert!(!u.studio.add_menu);
}

#[test]
fn the_meas_tab_is_gone_and_hiding_the_strip_is_reversible_in_one_tap() {
    let mut u = studio_with(6);
    u.studio_set_view(StudioView::Waveform);
    for _ in 0..4 { frame(&mut u); }
    assert!(u.studio_tab().is_none());
    shotb(&mut u, "strip");
    // with the strip hidden (View > Show pinned badges) the bottom bar has a chip that brings it back
    u.studio_set_pin_all_for_test();
    for _ in 0..4 { frame(&mut u); }
    if u.studio_badges_visible() {
        u.studio_hide_badges();
        assert!(u.studio.hide_badges);
        let show = u.studio_bottom().show.expect("a restore chip in the bottom bar");
        shotb(&mut u, "strip-hidden");
        tap(&mut u, show);
        assert!(!u.studio.hide_badges, "one tap brings the strip back");
        // the Add menu adds things: showing the strip is the chip above and View > Show pinned badges
        let t = u.studio_top();
        tap(&mut u, t.add);
        assert_eq!(u.studio_add_rows().len(), 17, "measurement, results table, plot, cursors, search, math, reference, bus, zoom, zoom box");
        u.studio.add_menu = false;
    }
    // without any pinned measurement the restore chip is there all the same
    u.studio.pinned = Some(Vec::new());
    u.studio.hide_badges = true;
    for _ in 0..4 { frame(&mut u); }
    assert!(u.studio_bottom().show.is_some());
}

#[test]
fn the_plot_context_menu_is_a_clamped_grouped_popover_with_the_lane_and_the_mode() {
    let mut u = studio_with(4);
    u.m.ch[1].on = true;
    for _ in 0..4 { frame(&mut u); }
    // near the bottom right corner: kept on the screen. Two columns of rows a finger high (it was one of 28 px rows)
    u.context_menu(W - 10, H - 80);
    assert!(u.studio.ctx.is_some() && u.workspace.popup.is_none(), "the old list popup is not used");
    let p = u.studio_ctx_rect().unwrap();
    assert!(p.w <= 500 && p.x >= 0 && p.right() <= W && p.y >= 0 && p.bottom() <= H, "{p:?}");
    let rows = u.studio_ctx_rows();
    assert!(rows.iter().all(|(_, _, r)| r.h >= 40 && r.x >= p.x && r.right() <= p.right() && r.bottom() <= p.bottom()));
    let labels: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
    // (Run, Single, Trigger settings and Sources are on the bars and not repeated here)
    for want in ["Overlay traces", "Stacked traces", "Fit record", "Zoom here", "Zoom box", "Autoset", "Cursor here"] {
        assert!(labels.iter().any(|l| l == &want), "{want} in {labels:?}");
    }
    assert!(labels.iter().any(|l| l.starts_with("Trigger on ")) && labels.iter().any(|l| l.starts_with("Measurement on ")), "{labels:?}");
    shotb(&mut u, "context-menu-corner");
    // a tap outside closes it
    tap_at(&mut u, 100, 100);
    assert!(u.studio.ctx.is_none());
    // at the press point, with a row that works: Stacked
    u.context_menu(300, 200);
    let p = u.studio_ctx_rect().unwrap();
    assert!(p.x >= 300 && p.y >= 4);
    shotb(&mut u, "context-menu");
    let stacked = u.studio_ctx_rows().into_iter().find(|r| r.0 == super::studioctx::CtxRow::Stacked).unwrap().2;
    tap(&mut u, stacked);
    assert!(u.workspace.stacked && u.studio.ctx.is_none());
    // Default and Minimal keep the old popup
    let mut d = Ui::new(Source::Demo { t: 0.0 });
    d.m.layout = Layout::Default;
    for _ in 0..4 { frame(&mut d); }
    d.context_menu(300, 200);
    assert!(d.studio.ctx.is_none());
}

#[test]
fn a_zoom_box_can_be_armed_from_the_bar_the_menu_and_the_add_menu_and_zooms_in() {
    // from the bar (title bar variant), zoom already on: the box zooms further
    let mut u = zoomed(false);
    let bar = u.studio_zoom_bar().unwrap();
    tap(&mut u, bar.boxr);
    assert!(u.zoom_arm, "Box arms the drawing");
    let z = u.split_zoom(u.grid).1;
    let (x0, y0) = (z.x + 200, z.y + 30);
    touch(&mut u, Phase::Down, x0, y0);
    touch(&mut u, Phase::Move, x0 + 100, y0 + 50);
    touch(&mut u, Phase::Move, x0 + 220, y0 + 90);
    assert_eq!(u.zoom_path.len(), 2, "the rubber band is being drawn");
    shotb(&mut u, "zoom-box-drawing");
    let zs = u.m.zscale;
    touch(&mut u, Phase::Up, x0 + 220, y0 + 90);
    assert!(u.m.zscale < zs || u.zoom_vfactor > 1.0, "the box zoomed");
    shotb(&mut u, "zoom-box-result");

    // with zoom OFF: the Add menu arms it, and the box turns zoom on with that window
    let mut u = studio_bottom(6);
    assert!(!u.m.zoom);
    let t = u.studio_top();
    tap(&mut u, t.add);
    let r = u.studio_add_rows().into_iter().find(|(k, _)| *k == super::addmenu::AddRow::ZoomBox).unwrap().1;
    tap(&mut u, r);
    assert!(u.zoom_arm);
    let a = u.area;
    let (x0, y0) = (a.x + a.w / 2 - 100, a.y + a.h / 3);
    drag(&mut u, (x0, y0), (x0 + 120, y0 + 80));
    assert!(u.m.zoom, "the box turned zoom on");
    assert!(u.zoom_arm, "and it stays armed for the next box");
    // a tiny drag cancels
    let a = u.area;
    let (x0, y0) = (a.x + 300, a.y + 50);
    drag(&mut u, (x0, y0), (x0 + 4, y0 + 3));
    assert!(!u.zoom_arm);

    // from the long-press menu
    let mut u = studio_bottom(6);
    u.context_menu(300, 150);
    let r = u.studio_ctx_rows().into_iter().find(|r| r.0 == super::studioctx::CtxRow::ZoomBox).unwrap().2;
    tap(&mut u, r);
    assert!(u.zoom_arm);
}

#[test]
fn the_new_presentation_state_is_per_view() {
    let mut u = studio_bottom(12);
    u.views_set(2);
    // view 1: List view, scrolled, a search, pinned only, the strip hidden
    u.panel_set_view(PanelView::List);
    u.studio.panel.first = 3;
    u.studio.panel.sub = 7;
    u.studio.panel.query = "freq".into();
    u.studio.panel.searching = true;
    u.studio.panel.pinned_only = true;
    u.studio.hide_badges = true;
    u.set_zoom(true);
    u.sync_view_state();
    // view 2 starts as a copy; make it differ: the table, nothing scrolled, no filter, strip shown
    u.view_go(1);
    u.panel_set_view(PanelView::Detailed);
    u.studio.panel.scroll_top();
    u.studio.panel.query.clear();
    u.studio.panel.searching = false;
    u.studio.panel.pinned_only = false;
    u.studio.hide_badges = false;
    u.view_go(0);
    assert_eq!(u.panel_view(), PanelView::List);
    assert_eq!((u.studio.panel.first, u.studio.panel.sub), (3, 7));
    assert_eq!(u.studio.panel.query, "freq");
    assert!(u.studio.panel.searching && u.studio.panel.pinned_only && u.studio.hide_badges);
    u.view_go(1);
    assert_eq!(u.panel_view(), PanelView::Detailed);
    assert_eq!((u.studio.panel.first, u.studio.panel.sub), (0, 0));
    assert!(u.studio.panel.query.is_empty() && !u.studio.panel.searching && !u.studio.panel.pinned_only && !u.studio.hide_badges);
    // Cards are per view too
    u.panel_set_view(PanelView::Cards);
    u.view_go(0);
    assert_eq!(u.panel_view(), PanelView::List);
    u.view_go(1);
    assert_eq!(u.panel_view(), PanelView::Cards);
}

/// The ... list of a narrow panel has the statistics and the gate with their choices, the one in use marked, and one tap
/// takes one. They were one row each that named the choice in use: a tap on the statistics went to the next one and
/// closed the list, which had to be opened again to go on (reported from a unit, 2026-11-18).
#[test]
fn the_dots_list_the_choices_of_the_statistics_and_one_tap_takes_one() {
    use super::measpanel::{BarItem, MoreAct};
    let mut u = with_dock("H0.780(W,M)", 12);
    let g = u.studio_panel_geo().unwrap();
    let bar = u.panel_bar(g.p);
    assert!(bar.hidden.contains(&BarItem::Stats) && bar.hidden.contains(&BarItem::Gate), "{:?}", bar.hidden);
    let more = bar.more.unwrap().r;
    for (mode, name) in [(2u8, "Min Max"), (1, "Mean + \u{3c3}"), (0, "Statistics off"), (2, "Min Max")] {
        tap(&mut u, more);
        assert_eq!(u.studio.panel.drop, Some(super::measpanel::PanelDrop::More));
        let rows = u.panel_more_rows();
        assert_eq!(rows.iter().filter(|r| matches!(r.2, MoreAct::Stats(_)) && r.1).count(), 1, "one choice is the one in use: {rows:?}");
        assert_eq!(rows.iter().filter(|r| matches!(r.2, MoreAct::Gate(_)) && r.1).count(), 1, "{rows:?}");
        let i = rows.iter().position(|r| r.0 == name).expect(name);
        let p = u.panel_drop_rect().unwrap();
        assert!(p.y >= 0 && p.bottom() <= H && p.x >= 0 && p.right() <= W, "{p:?}");
        assert!(Ui::panel_drop_row(p, rows.len() - 1).bottom() <= p.bottom());
        if mode == 2 && u.studio.panel.stats_mode == 0 { shot(&mut u, "dots-list"); }
        tap(&mut u, Ui::panel_drop_row(p, i));
        assert_eq!(u.studio.panel.stats_mode, mode, "{name}");
        assert_eq!(u.studio.panel.stats, mode != 0);
        assert!(u.studio.panel.drop.is_none(), "the list stayed");
    }
    for (full, name) in [(false, "Gate: Screen"), (true, "Gate: Full record")] {
        tap(&mut u, more);
        let rows = u.panel_more_rows();
        let i = rows.iter().position(|r| r.0 == name).expect(name);
        let p = u.panel_drop_rect().unwrap();
        tap(&mut u, Ui::panel_drop_row(p, i));
        assert_eq!(u.m.meas_full_record, full, "{name}");
    }
}

/// The statistics of a card fit the card: with the words where the card has the room, as "a .. b" and "a ± b" where it
/// has not, and the form follows the width and the unit, never the value. Two cards beside each other in a side panel
/// are 148 px wide, and "min 145 mV  max 212 mV" ended at "max" (reported from a unit, 2026-11-18).
#[test]
fn the_statistics_of_a_card_fit_the_card() {
    let u = studio_with(6);
    let f = &u.fonts().s;
    assert_eq!(Ui::card_stats_line(f, true, "V", "145 mV", "212 mV", 300), "min 145 mV  max 212 mV");
    assert_eq!(Ui::card_stats_line(f, false, "V", "187.6 mV", "1.2 mV", 300), "\u{b5} 187.6 mV  \u{3c3} 1.2 mV");
    let room = 148 - 16;
    for unit in ["V", "A", "Hz", "s", "%", "Vs", "W"] {
        for range in [true, false] {
            let mut forms = Vec::new();
            for (a, b) in [("1", "2"), ("145 m", "212 m"), ("-9.89 m", "-9.80 m"), ("-99.9 m", "-99.9 m"), ("7.26 \u{b5}", "7.54 \u{b5}")] {
                let (a, b) = (format!("{a}{unit}"), format!("{b}{unit}"));
                let line = Ui::card_stats_line(f, range, unit, &a, &b, room);
                assert!(f.width(&line) <= room, "{line:?} is {} wide in {room}", f.width(&line));
                assert!(line.contains(&b), "{line:?} has no {b}");
                forms.push((line.starts_with("min") || line.starts_with('\u{b5}'), line.contains(".."), line.contains('\u{b1}')));
            }
            assert!(forms.windows(2).all(|w| w[0] == w[1]), "the form changes with the value: {unit} {forms:?}");
        }
    }
}

#[test]
fn the_trend_page_reaches_every_measurement() {
    let mut u = studio_with(3);
    u.open_menu(MenuId::AddPanel);
    u.sheet.add.kind = Some(super::addrail::AddKind::Plot);
    u.sheet.add.plot = 4;
    frame(&mut u);
    let g = u.rail_geo();
    let (_, more) = *g.meas.last().expect("a More chip");
    tap(&mut u, more);
    assert!(u.menu_open && u.menu == MenuId::Pick, "{:?}", u.menu);
    assert_eq!(u.m.pane, super::tables::PANE_TREND);
    shotb(&mut u, "trend-more");
}
