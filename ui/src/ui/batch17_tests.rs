// SPDX-License-Identifier: GPL-2.0-or-later
//! Batch 17: colour carries the source. The table names the source in every row beside its colour bar, badges have
//! no chip. And the strip view of the measurements panel: cells of one line in columns, the first of the three views.
//! OOS_SHOTS=<dir> writes pictures (b17-*).
use super::dock::{Drop, Side, VKind};
use super::studio::StudioView;
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
    u.m.run = RunState::Stop;
    frame(&mut u);
    u
}

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    u.step(0.02);
}

fn shot(u: &mut Ui, name: &str) {
    for _ in 0..6 {
        frame(u);
    }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("b17-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
}

fn two_sources(u: &mut Ui) {
    u.m.meas = vec![
        (Meas::Vpp, WaveId::Ch(0)),
        (Meas::Freq, WaveId::Ch(0)),
        (Meas::Vpp, WaveId::Ch(1)),
        (Meas::Freq, WaveId::Ch(1)),
    ];
    u.ensure_meas_own();
    for _ in 0..4 {
        frame(u);
    }
}

fn one_source(u: &mut Ui) {
    u.m.meas = vec![(Meas::Vpp, WaveId::Ch(0)), (Meas::Freq, WaveId::Ch(0)), (Meas::Vavg, WaveId::Ch(0))];
    u.ensure_meas_own();
    for _ in 0..4 {
        frame(u);
    }
}

/// Pixels of exactly this colour in `r` of a canvas.
fn count(cv: &Canvas, r: Rect, col: Rgb) -> usize {
    (r.y..r.bottom())
        .flat_map(|y| (r.x..r.right()).map(move |x| (x, y)))
        .filter(|&(x, y)| cv.px[(y * W + x) as usize] == col)
        .count()
}

#[test]
fn the_detailed_table_names_the_source_in_every_row_beside_its_colour_bar() {
    let mut u = studio();
    two_sources(&mut u);
    u.studio_set_detailed(true);
    for _ in 0..4 {
        frame(&mut u);
    }
    let g = u.studio_panel_geo().unwrap();
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    let pitch = u.studio_panel_row();
    let c0 = u.wave_color(WaveId::Ch(0));
    let name = |k: i32| Rect::new(g.list.x + 10, g.list.y + k * pitch, 24, pitch);
    assert!(count(&cv, name(0), c0) > 5, "CH1 in the first row");
    assert!(
        count(&cv, name(1), c0) > 0 || count(&cv, name(1), u.wave_color(WaveId::Ch(1))) > 5,
        "batch 22: the second row names its source too"
    );
    // with one source the name is there as well
    one_source(&mut u);
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    assert!(count(&cv, name(0), c0) > 5, "the name with one source");
    shot(&mut u, "table-one-source");
}

#[test]
fn a_badge_has_no_chip_a_colour_bar_and_the_short_name() {
    let mut u = studio();
    // the badges of the Studio style (the classic strip is the default of a new set-up)
    u.studio_set_classic(false);
    two_sources(&mut u);
    u.studio_set_view(StudioView::Waveform);
    for _ in 0..3 {
        frame(&mut u);
    }
    let badges = u.studio_badges();
    assert!(!badges.is_empty());
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    let (key, r) = badges[0];
    let col = u.wave_color(WaveId::Ch(0));
    assert!(count(&cv, Rect::new(r.x + 3, r.y + 4, 5, r.h - 8), col) > 10, "the colour bar at the edge");
    assert_eq!(count(&cv, Rect::new(r.x + 9, r.y, r.w - 9, r.h), col), 0, "no chip in the badge");
    assert_eq!(u.studio_badge_name(key, "Peak-to-Peak"), "Pk-pk");
    shot(&mut u, "badges");
}

/// The three buttons of the view switch take the touch where they are drawn: each is lit inside its own segment, and
/// a tap in the middle of a segment chooses its view. They were drawn at the left of the switch, a third narrower
/// than the segments that took the touch, so Cards answered only right of its box.
#[test]
fn the_view_switch_is_drawn_where_it_is_tapped() {
    use super::measpanel::PanelView;
    let mut u = studio();
    one_source(&mut u);
    let r = u.dock_rect(super::dock::VKind::Measure).expect("the panel is docked");
    for (k, view) in [PanelView::List, PanelView::Detailed, PanelView::Cards].into_iter().enumerate() {
        let e = u
            .dock_view_rect(super::dock::VKind::Measure, u.dock_rect(super::dock::VKind::Measure).unwrap_or(r))
            .expect("the switch");
        let cell = Ui::view_switch_cells(e)[k];
        assert!(cell.w >= 24, "{cell:?}");
        let (x, y) = (cell.x + cell.w / 2, cell.y + cell.h / 2);
        u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
        u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
        frame(&mut u);
        assert_eq!(u.panel_view(), view, "a tap in segment {k}");
        // the first segment is the strip of cells, the second the table, the third the cards
        assert_eq!(
            (u.panel_is_strip(), u.studio_detailed(), u.panel_is_cards()),
            (k == 0, k == 1, k == 2),
            "segment {k}"
        );
        // the lit segment is inside the one that was tapped
        let e =
            u.dock_view_rect(super::dock::VKind::Measure, u.dock_rect(super::dock::VKind::Measure).unwrap()).unwrap();
        let cell = Ui::view_switch_cells(e)[k];
        let mut cv = Canvas::new(W, H);
        cv.au_start();
        u.draw_view_switch(&mut cv, e);
        let lit = cv
            .au
            .take()
            .unwrap()
            .into_iter()
            .find_map(|a| match a {
                Au::Round { r, c, .. } if c == ACCENT => Some(r),
                _ => None,
            })
            .expect("one segment is lit");
        assert!(lit.x >= cell.x && lit.right() <= cell.right(), "segment {k}: lit {lit:?} in {cell:?}");
    }
}

// ------------------------------------------------------------------------------------------------ the strip view

/// The panel in its first view, the strip, with `n` measurements a channel on CH1 and CH2; under the plot when
/// `bottom`, else in its place at the side.
fn strip(n: usize, bottom: bool) -> Ui {
    let mut u = studio();
    let kinds = [
        Meas::Vpp,
        Meas::Freq,
        Meas::Vamp,
        Meas::Vrms,
        Meas::Vmax,
        Meas::Vmin,
        Meas::Vavg,
        Meas::Period,
        Meas::Rise,
        Meas::Fall,
        Meas::PWidth,
        Meas::NWidth,
        Meas::PDuty,
        Meas::NDuty,
        Meas::Vtop,
        Meas::Vbase,
    ];
    u.m.meas = (0..2).flat_map(|c| kinds.iter().take(n).map(move |&k| (k, WaveId::Ch(c)))).collect();
    u.ensure_meas_own();
    if bottom {
        u.dock_apply(VKind::Measure, Drop::Split(VKind::Wave, Side::Bottom));
    }
    for _ in 0..6 {
        frame(&mut u);
    }
    assert!(u.panel_is_strip() && u.panel_view() == super::measpanel::PanelView::List, "the strip is the view");
    u
}

/// The cells on the page: the index of each in the rows, and its rectangle.
fn cells(u: &Ui) -> Vec<(usize, Rect)> {
    let g = u.studio_panel_geo().expect("the panel");
    let listed = u.studio_listed(&u.meas_rows());
    let first = u.studio_first(listed.len(), g.page);
    u.panel_cards_layout(first, listed.len(), g.list).into_iter().map(|(pos, r)| (listed[pos], r)).collect()
}

fn touch(u: &mut Ui, phase: Phase, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase, x, y });
    u.step(0.01);
}

/// A wide low panel under the plot has its cells in several columns, so it shows many measurements where a row
/// each showed two; the narrow panel at the side has them in one column.
#[test]
fn the_strip_has_columns_in_a_wide_low_panel_and_one_column_at_the_side() {
    let mut u = strip(6, true);
    let g = u.studio_panel_geo().unwrap();
    assert!(g.list.w >= 900 && g.list.h <= 260, "wide and low: {:?}", g.list);
    let c = cells(&u);
    assert_eq!(c.len(), 12, "all twelve show in {:?}", g.list);
    let cols = c.iter().filter(|(_, r)| r.y == c[0].1.y).count();
    assert!(cols >= 4, "{cols} columns in {} px", g.list.w);
    for (k, (_, r)) in c.iter().enumerate() {
        assert_eq!(r.h, super::measpanel::CELL_H, "a cell is one line high");
        assert!(r.w >= 150, "a cell has the room of a source, a name and a value: {r:?}");
        assert!(
            r.x >= g.list.x && r.right() <= g.list.right() && r.y >= g.list.y && r.bottom() <= g.list.bottom(),
            "{r:?} in {:?}",
            g.list
        );
        // they run along the line, then on the next
        assert_eq!((r.x, r.y), (c[k % cols].1.x, c[0].1.y + (k / cols) as i32 * (c[cols].1.y - c[0].1.y)), "cell {k}");
        if k % cols > 0 {
            assert!(c[k - 1].1.right() < r.x, "cells beside each other do not touch");
        }
    }
    shot(&mut u, "strip-bottom");
    let mut u = strip(6, false);
    let g = u.studio_panel_geo().unwrap();
    assert!(g.list.w < 300, "the side panel: {:?}", g.list);
    let c = cells(&u);
    assert_eq!(c.len(), 12);
    for w in c.windows(2) {
        assert!(w[0].1.x == w[1].1.x && w[0].1.bottom() < w[1].1.y, "one column: {:?} {:?}", w[0].1, w[1].1);
    }
    shot(&mut u, "strip-side");
}

/// A cell is one line: the source, the name of the measurement and the value, in that order, each whole in the cell.
#[test]
fn a_cell_of_the_strip_is_one_line_with_the_source_the_name_and_the_value() {
    for bottom in [true, false] {
        let u = strip(6, bottom);
        let g = u.studio_panel_geo().unwrap();
        let rows = u.meas_rows();
        let keys = u.studio_keys();
        let listed = u.studio_listed(&rows);
        let mut cv = Canvas::new(W, H);
        cv.au_start();
        u.draw_panel_strip(&mut cv, &g, &rows, &keys, &listed, 0);
        let texts = cv.au.take().unwrap();
        for (i, r) in cells(&u) {
            let row = &rows[i];
            // (ink and clip are left, top, right, bottom)
            let inside =
                |ink: &(i32, i32, i32, i32)| ink.0 >= r.x && ink.2 <= r.right() && ink.1 >= r.y && ink.3 <= r.bottom();
            let find = |want: &str| {
                texts.iter().find_map(|e| match e {
                    Au::Text { s, ink: Some(ink), clip, .. } if s == want && inside(ink) => Some((*ink, *clip)),
                    _ => None,
                })
            };
            let (src, _) = find(&u.wave_name(row.src))
                .unwrap_or_else(|| panic!("bottom {bottom}: the source of {} in {r:?}", row.name));
            let (value, _) =
                find(&row.text()).unwrap_or_else(|| panic!("bottom {bottom}: the value of {} in {r:?}", row.name));
            // the long name where it fits, else the short one of the badges
            let (name, clip) = find(&row.name)
                .or_else(|| find(&u.studio_badge_name(keys[i], &row.name)))
                .unwrap_or_else(|| panic!("bottom {bottom}: the name of {} in {r:?}", row.name));
            assert!(
                src.2 < name.0 && name.2 < value.0,
                "bottom {bottom}: source, name, value along the line: {src:?} {name:?} {value:?}"
            );
            assert!(name.2 <= clip.2, "bottom {bottom}: the name {} is whole: {name:?} in {clip:?}", row.name);
        }
    }
}

/// A tap on a cell opens the settings of its measurement, and changes nothing else.
#[test]
fn a_tap_on_a_cell_of_the_strip_opens_the_settings_of_its_measurement() {
    for bottom in [true, false] {
        let mut u = strip(6, bottom);
        let (keys, pinned) = (u.studio_keys(), u.studio_pinned());
        let (i, r) = cells(&u)[7];
        let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
        touch(&mut u, Phase::Down, x, y);
        touch(&mut u, Phase::Up, x, y);
        frame(&mut u);
        assert_eq!(u.meas_own.dialog, Some(i as u16), "bottom {bottom}: the settings of the measurement of the cell");
        assert!(u.menu_open && u.menu == MenuId::MeasRow, "bottom {bottom}: its settings are open");
        assert_eq!((u.studio_keys(), u.studio_pinned()), (keys, pinned), "nothing removed, nothing pinned");
        // Remove is under the settings
        assert!(u.sheet_geo().unwrap().foot.iter().any(|f| f.0 == Item::MeasRemove(i as u16)));
    }
}

/// The right end of a cell is its pin: a tap there pins the measurement as a badge or takes the badge away, and
/// opens nothing.
#[test]
fn the_right_end_of_a_cell_of_the_strip_is_its_pin() {
    let mut u = strip(6, false);
    let (i, r) = cells(&u)[2];
    let key = u.studio_keys()[i];
    let was = u.studio_is_pinned(key);
    let pin = Ui::panel_cell_pin(r);
    assert!(pin.w >= 24 && pin.right() == r.right() && pin.h == r.h, "{pin:?} {r:?}");
    for want in [!was, was] {
        // (the cell of that measurement, wherever it stands now)
        let r = cells(&u).into_iter().find(|&(j, _)| u.studio_keys().get(j) == Some(&key)).expect("its cell").1;
        let pin = Ui::panel_cell_pin(r);
        let (x, y) = (pin.x + pin.w / 2, pin.y + pin.h / 2);
        touch(&mut u, Phase::Down, x, y);
        touch(&mut u, Phase::Up, x, y);
        frame(&mut u);
        assert_eq!(u.studio_is_pinned(key), want);
        assert!(!(u.menu_open && u.menu == MenuId::MeasRow), "the pin opens no settings");
    }
}

/// More cells than the panel holds: the strip scrolls with the finger, by whole lines, and a drag is no tap.
#[test]
fn the_strip_scrolls_when_there_are_more_cells_than_fit() {
    let mut u = strip(16, false);
    let g = u.studio_panel_geo().unwrap();
    let total = u.studio_listed(&u.meas_rows()).len();
    assert!(total > g.page, "{total} cells, {} on a page", g.page);
    assert!(u.panel_scroll_max(total, &g) > 0, "there is something to scroll to");
    let before = cells(&u);
    assert_eq!(before.len(), g.page, "a full page");
    let (x, y) = (g.list.x + 60, g.list.bottom() - 10);
    let step = before[1].1.y - before[0].1.y;
    touch(&mut u, Phase::Down, x, y);
    for k in 1..=6 {
        touch(&mut u, Phase::Move, x, y - k * step / 2);
    }
    touch(&mut u, Phase::Up, x, y - 3 * step);
    frame(&mut u);
    assert!(u.meas_own.dialog.is_none() && !u.menu_open, "a drag opens no settings");
    let first = u.studio.panel.first;
    assert!(first > 0 && first % u.panel_cols() == 0, "scrolled by whole lines: {first}");
    assert_ne!(cells(&u)[0].0, before[0].0, "other cells are on the page");
    shot(&mut u, "strip-scrolled");
    // to the end, and no further
    u.panel_scroll_set(i32::MAX / 2, total, &g);
    assert_eq!(u.panel_scroll_px(), u.panel_scroll_max(total, &g));
    assert_eq!(
        cells(&u).last().map(|c| c.0),
        u.studio_listed(&u.meas_rows()).last().copied(),
        "the last cell shows at the end"
    );
    // and back
    touch(&mut u, Phase::Down, x, g.list.y + 10);
    for k in 1..=8 {
        touch(&mut u, Phase::Move, x, g.list.y + 10 + k * 200);
    }
    touch(&mut u, Phase::Up, x, g.list.y + 10 + 1600);
    frame(&mut u);
    assert_eq!(u.studio.panel.first, 0, "back at the top");
}
