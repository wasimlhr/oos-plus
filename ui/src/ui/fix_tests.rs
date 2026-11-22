// SPDX-License-Identifier: GPL-2.0-or-later
//! Regression tests for the compact (Minimal) chrome audit.
use super::workspace::Action;
use super::*;

#[cfg(audit_shots)]
mod audit_shots;

fn fresh() -> Ui {
    let mut u = super::testkit::demo_ui();
    u.m.layout = crate::model::Layout::Minimal; // these tests check the Minimal bars
    u.relayout();
    for _ in 0..4 {
        u.step(0.05);
    }
    u
}

#[test]
fn the_last_visible_source_cannot_be_switched_off() {
    let mut u = fresh();
    assert_eq!(u.trace_sources().len(), 1);
    u.workspace_action(Action::Source(WaveId::Ch(0)));
    assert!(u.m.ch[0].on && u.trace_sources().len() == 1, "CH1 stays on");
}

fn two_channels() -> Ui {
    let mut u = fresh();
    u.activate_live_channel(1);
    u.relayout();
    u
}

#[test]
fn overlay_keeps_a_split_and_stacked_leaves_a_consistent_density() {
    for density in [Density::Split, Density::Four] {
        let mut u = two_channels();
        u.m.density = density;
        u.apply_density();
        u.relayout();
        let layout = u.m.win_layout;
        assert_ne!(layout, WinLayout::One);
        u.trace_mode(false);
        assert_eq!(
            (u.m.density, u.m.win_layout, u.workspace.stacked),
            (density, layout, false),
            "Overlay keeps the split"
        );
        u.trace_mode(true);
        assert!(u.workspace.stacked && u.m.win_layout == WinLayout::One && u.m.density == Density::Scope);
        // choosing a split again drops Stacked, so the highlighted pill is the one in force
        u.m.density = density;
        u.apply_density();
        assert!(!u.workspace.stacked && u.m.win_layout != WinLayout::One);
    }
}

fn with_refs(u: &mut Ui, n: usize) {
    for i in 0..n {
        u.refs[i] = Some(RefWave {
            color: i,
            label: format!("source-{i}"),
            vpd: 0.1,
            ..RefWave::test(
                (0..128).map(|k| ((k as f32 + i as f32) * 0.2).sin() * 0.2).collect::<Vec<_>>(),
                1e-10,
                -2e-9,
            )
        });
    }
}

fn tap(u: &mut Ui, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
}

#[test]
fn sources_that_do_not_fit_sit_behind_a_plus_pill_that_opens_the_popup() {
    let mut u = fresh();
    assert!(u.source_overflow().is_none());
    for c in 1..4 {
        u.activate_live_channel(c);
    }
    with_refs(&mut u, NREF);
    u.relayout();
    let (hidden, pill) = u.source_overflow().expect("14 sources do not fit");
    let shown = u.source_pills();
    assert_eq!(shown.len() + hidden, 4 + NREF);
    assert!(shown.iter().all(|(_, r)| r.x + r.w <= pill.x), "pills do not overlap the +N pill");
    let (_, _, time, ..) = u.footer_rects();
    assert!(pill.x + pill.w <= time.x, "the +N pill stays left of the time base");
    tap(&mut u, pill.x + 5, pill.y + 5);
    let popup = u.workspace.popup.as_ref().expect("Sources popup");
    assert!(popup.rows.len() >= 4 + NREF, "every source is in the popup");
}

#[test]
fn context_menu_stays_inside_the_plot_and_marks_the_trace_mode_in_force() {
    let mut u = fresh();
    for (x, y) in [(5, 45), (1020, 595), (500, 250), (1000, 100)] {
        u.context_menu(x, y);
        let p = u.workspace.popup.as_ref().unwrap().rect;
        let g = u.plot.grid;
        assert!(
            p.x >= g.x && p.right() <= g.right() && p.y >= g.y && p.bottom() <= g.bottom(),
            "({x},{y}) {p:?} in {g:?}"
        );
    }
    u.context_menu(500, 250);
    let row = |u: &Ui, i: i32| {
        let r = u.workspace.popup.as_ref().unwrap().row_rect(i as usize);
        Rect::new(r.right() - 42, r.y, 24, r.h)
    };
    let ink = |u: &mut Ui, r: Rect| {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        (r.y..r.y + r.h)
            .flat_map(|y| (r.x..r.x + r.w).map(move |x| (x, y)))
            .filter(|&(x, y)| cv.px[(y * W + x) as usize] == ACCENT)
            .count()
    };
    let (overlay_row, stacked_row) = (row(&u, 0), row(&u, 1));
    assert!(ink(&mut u, overlay_row) > 0 && ink(&mut u, stacked_row) == 0, "check on Overlay");
    u.workspace.stacked = true;
    assert!(ink(&mut u, overlay_row) == 0 && ink(&mut u, stacked_row) > 0, "check on Stacked");
}

#[test]
fn a_floating_measurement_card_never_covers_the_zoom_bar_or_the_corner_buttons() {
    let mut u = fresh();
    u.set_zoom(true);
    for place in [None, Some((W - 220, TOP.h + 32)), Some((600, 60)), Some((W - 40, 140))] {
        u.workspace_action(Action::MeasureFloat);
        if place.is_some() {
            u.workspace.measure_float = place;
        }
        let (head, body) = u.results_rects().unwrap();
        let card = Rect::new(head.x, head.y, head.w, head.h + body.h);
        let hits = |r: Rect| card.x < r.right() && r.x < card.right() && card.y < r.bottom() && r.y < card.bottom();
        assert!(!u.corner_btns().into_iter().any(hits), "{place:?} covers a corner button: {card:?}");
        assert!(!hits(u.zoom_readout()), "{place:?} covers the zoom bar: {card:?}");
    }
}

#[test]
fn a_docked_measurement_card_reflows_the_plot_instead_of_covering_it() {
    for dock in StripDock::ALL.into_iter().filter(|d| *d != StripDock::Hidden) {
        let mut u = fresh();
        u.workspace_action(Action::MeasureDock(dock));
        u.relayout();
        assert_eq!(u.m.strip_dock, dock, "one dock state");
        let (head, body) = u.results_rects().unwrap();
        let card = Rect::new(head.x, head.y, head.w, head.h + body.h);
        let g = u.plot.grid;
        let apart = card.x >= g.right() || g.x >= card.right() || card.y >= g.bottom() || g.y >= card.bottom();
        assert!(apart, "{dock:?}: card {card:?} overlaps the plot {g:?}");
        let hits = |r: Rect| card.x < r.right() && r.x < card.right() && card.y < r.bottom() && r.y < card.bottom();
        assert!(!u.corner_btns().into_iter().any(hits), "{dock:?} covers a corner button");
        assert!(
            card.y >= TOP.h && card.bottom() <= BOTTOM.y + 1 && card.x >= 0 && card.right() <= W,
            "{dock:?} {card:?} on screen"
        );
    }
    // Statistics respects the Dock choice of the Measure menu
    let mut u = fresh();
    u.results.on = true;
    u.m.strip_dock = StripDock::Below;
    let before = u.plot.grid;
    u.adjust(Item::StripDock, 1);
    u.relayout();
    assert!(u.results_docked().is_some());
    assert_ne!(u.plot.grid, before);
    // Default layout keeps the stock overlay
    let mut u = fresh();
    u.m.layout = Layout::Default;
    u.workspace_action(Action::MeasureDock(StripDock::Left));
    assert!(u.results_docked().is_none());
}

#[test]
fn reference_badges_are_fanned_out_and_the_legend_is_cut_with_a_count() {
    let mut taken = Vec::new();
    for _ in 0..6 {
        let spot = badge_spot(&taken, 280, 42, 480);
        assert!(taken.iter().all(|t: &i32| (t - spot).abs() >= 36), "{spot} overlaps {taken:?}");
        taken.push(spot);
    }
    let mut u = fresh();
    with_refs(&mut u, NREF);
    u.relayout();
    let note = u.live_ref_note();
    for width in [140, 300, 480, 800] {
        let cut = u.fit_ref_note(&note, width);
        assert!(u.fonts().s.width(&cut) <= width || cut.ends_with("+10"), "{cut:?} in {width}");
        if cut != note {
            assert!(cut.contains('+'), "{cut:?}");
        }
    }
    assert_eq!(u.fit_ref_note(&note, 5000), note);
    // the note and the Fit button sit above the axis labels, not on them
    let fit = u.live_ref_fit_rect().unwrap();
    assert!(fit.bottom() <= u.plot.grid.bottom() - 20, "{fit:?} over the axis labels of {:?}", u.plot.grid);
}

#[test]
fn eye_tab_in_a_side_pane_clears_the_charts_and_cursor_tags_clear_the_corner_buttons() {
    let hit = |a: Rect, b: Rect| a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom();
    for place in [workspace::Action::EyeRight, workspace::Action::EyeLeft, workspace::Action::EyeBelow] {
        let mut u = fresh();
        u.m.view = AView::Eye;
        u.relayout();
        u.workspace_action(place);
        let page = u.eye_page_rect();
        let l = eyepage::layout(page, false);
        if l.charts[0].w == 0 {
            continue;
        }
        let tab = u.eye_tab();
        for (k, c) in l.charts.iter().enumerate() {
            if page.w < 700 {
                assert!(!hit(tab, *c), "tab {tab:?} over chart {k} {c:?}");
            }
        }
    }
    let u = fresh();
    for y in (u.plot.area.y..u.plot.area.bottom()).step_by(3) {
        let x = u.right_tag_x(u.plot.area, y, 24);
        let tag = Rect::new(x, y, 24, 14);
        assert!(!u.corner_btns().into_iter().any(|r| hit(tag, r)), "tag at y {y} covers a button");
    }
}

/// Centre of the row `label` of the open workspace popup (it sits inside the plot, so tests ask it).
pub(super) fn row_xy(u: &Ui, label: &str) -> (i32, i32) {
    let p = u.workspace.popup.as_ref().expect("a popup is open");
    let i = p.rows.iter().position(|(l, _)| l.starts_with(label)).expect("row in popup");
    let r = p.row_rect(i);
    (r.x + 30, r.y + r.h / 2)
}

/// The compact bottom bar has the pills of Studio on one line: a source that is on says its scale beside its tag, a
/// decoder that is on has a pill that opens its menu, a channel that is off is a dim pill that switches it on, and the
/// plus after them lists the sources. Overlay and Stacked have no button there (they are Display > Traces).
#[test]
fn the_compact_bar_has_a_pill_for_each_source_and_decoder() {
    use super::workspace::FootPill;
    let mut u = two_channels();
    u.m.dec_on = true;
    u.relayout();
    let pills = u.footer_pills();
    let kinds: Vec<FootPill> = pills.iter().map(|p| p.0).collect();
    let want = [FootPill::Wave(WaveId::Ch(0)), FootPill::Wave(WaveId::Ch(1)), FootPill::Dec(0), FootPill::Off(2), FootPill::Off(3)];
    assert_eq!(kinds, want);
    let (overlay, stacked, time, trigger, ..) = u.footer_rects();
    assert_eq!((overlay.w, stacked.w), (0, 0), "no Overlay and Stacked on the bar");
    let plus = u.footer_plus();
    let mut all: Vec<Rect> = pills.iter().map(|p| p.1).chain([plus, time, trigger, Ui::MENU_BTN]).collect();
    all.sort_by_key(|r| r.x);
    assert!(all.iter().all(|r| r.w >= 38 && r.h >= 30) && all.windows(2).all(|w| w[0].right() <= w[1].x), "{all:?}");
    // what the pills say
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    cv.au_start();
    u.render(&mut cv);
    let texts: Vec<String> = cv.au.take().unwrap().into_iter().filter_map(|e| match e { Au::Text { s, .. } => Some(s), _ => None }).collect();
    let scale = format!("{}/div", si_short(u.m.ch[0].volts_per_div() as f64, "V"));
    for t in ["CH1", scale.as_str(), "B1", "UART", "CH3", "+"] {
        assert!(texts.iter().any(|s| s == t), "{t:?} is on the bar: {texts:?}");
    }
    assert!(!texts.iter().any(|s| s == "Overlay" || s == "Stacked" || s == "Sources"));
    // the pill of the decoder opens its menu, and closes it again
    let dec = pills[2].1;
    tap(&mut u, dec.x + dec.w / 2, dec.y + dec.h / 2);
    assert!(u.menu_open && u.menu_root() == MenuId::Decode && u.m.dec_sel == 0);
    tap(&mut u, dec.x + dec.w / 2, dec.y + dec.h / 2);
    assert!(!u.menu_open);
    // the pill of a channel that is off switches it on, and it has a pill with its scale then
    let off = pills[3].1;
    tap(&mut u, off.x + off.w / 2, off.y + off.h / 2);
    assert!(u.m.ch[2].on && u.footer_pills().iter().any(|p| p.0 == FootPill::Wave(WaveId::Ch(2))));
}
