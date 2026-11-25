// SPDX-License-Identifier: GPL-2.0-or-later

//! A row of the Display menu or of the plot's menu does what its label says (found by `funccheck_tests`).

use super::*;

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    u.step(0.05);
}

fn studio() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.studio.enter = true;
    for _ in 0..10 {
        u.step(0.05);
    }
    u.m.run = RunState::Stop;
    frame(&mut u);
    u
}

/// Display > Format: YT after XY in Studio (the row set XY again whatever it showed).
#[test]
fn format_switches_between_yt_and_xy_in_studio() {
    let mut u = studio();
    assert_eq!(u.value(Item::XyMode), "YT");
    u.select_option(Item::XyMode, "XY");
    assert!(u.xy && u.value(Item::XyMode) == "XY");
    u.select_option(Item::XyMode, "YT");
    assert!(!u.xy && u.value(Item::XyMode) == "YT", "YT takes XY off");
    u.adjust(Item::XyMode, 1);
    u.adjust(Item::XyMode, 1);
    assert!(!u.xy, "two steps are back where they began");
}

/// The plot's menu offers Maximise view only when another view shares the screen with the waveform.
#[test]
fn maximise_is_offered_only_with_a_second_view() {
    let mut u = studio();
    let area = u.plot.area;
    u.context_menu(area.x + area.w / 2, area.y + area.h / 2);
    frame(&mut u);
    let labels = |u: &Ui| u.studio_ctx_rows().into_iter().map(|(_, l, _)| l).collect::<Vec<_>>();
    assert!(!labels(&u).iter().any(|l| l == "Maximise view"), "the waveform alone: {:?}", labels(&u));
    u.studio.ctx = None;
    u.xy = true;
    frame(&mut u);
    u.context_menu(area.x + area.w / 2, area.y + area.h / 2);
    frame(&mut u);
    assert!(labels(&u).iter().any(|l| l == "Maximise view"), "with XY beside it: {:?}", labels(&u));
}

/// Autoset with the zoom on must not overflow an integer in the trace drawing (it did in debug builds).
#[test]
fn autoset_with_zoom_on_draws() {
    let mut u = studio();
    u.m.zoom = true;
    u.workspace_action(super::workspace::Action::Autoset);
    for _ in 0..5 {
        frame(&mut u);
    }
}

/// Display is one list in the same order in every layout, in three tabs of at most seven rows.
#[test]
fn display_is_three_tabs_in_one_order() {
    for hdmi in [HdmiMode::Mirror, HdmiMode::Waveform, HdmiMode::Layout] {
        for arrangement in [windows::Arrangement::Overlay, windows::Arrangement::Stacked, windows::Arrangement::Windows] {
            let mut u = studio();
            u.m.hdmi = hdmi;
            u.set_arrangement(arrangement);
            u.open_menu(MenuId::Display);
            u.step(0.04);
            let spec = u.sheet_spec(MenuId::Display).expect("Display is a sheet");
            let names: Vec<_> = spec.tabs.iter().map(|t| t.name).collect();
            assert_eq!(names, ["Traces", "Look", "Screen"], "{hdmi:?} {arrangement:?}");
            assert_eq!(spec.tabs[0].rows, [Item::Layout, Item::Traces, Item::Views, Item::XyMode, Item::ZInput, Item::Histogram], "{hdmi:?} {arrangement:?}");
            assert_eq!(spec.tabs[0].foot, [Item::Open(MenuId::Windows)]);
            assert_eq!(spec.tabs[1].rows, [Item::Persist, Item::Intensity, Item::ColorGrade, Item::Dots, Item::GridStyle, Item::GridBright, Item::GridFrame]);
            assert_eq!(spec.tabs[1].foot, [Item::ClearDisp]);
            for t in &spec.tabs {
                assert!(t.rows.len() <= 7, "{}: {} rows", t.name, t.rows.len());
            }
            // what :UI:MENU? lists is what the sheet has
            let listed = u.script_labels().into_iter().map(|(it, _)| it).collect::<Vec<_>>();
            let drawn: Vec<Item> = spec.tabs.iter().flat_map(|t| t.rows.iter().chain(&t.foot).copied()).collect();
            assert!(listed.iter().all(|it| drawn.contains(it)), "{hdmi:?} {arrangement:?}: {listed:?} vs {drawn:?}");
        }
    }
}

/// The Default layout's dialog shows the same rows in the same order as the sheet (plus the Bottom bar row it has).
#[test]
fn the_default_dialog_has_the_rows_of_the_sheet_in_its_order() {
    let mut a = Ui::new(Source::Demo { t: 0.0 });
    a.m.layout = Layout::Default;
    let dialog: Vec<Item> = a.items(MenuId::Display).into_iter().filter(|it| *it != Item::BottomBar).collect();
    let mut s = studio();
    s.m.hdmi = a.m.hdmi;
    let studio_rows: Vec<Item> = s.items(MenuId::Display).into_iter().filter(|it| !matches!(it, Item::StudioBadges | Item::StudioCompact | Item::StudioReset | Item::BottomBar)).collect();
    assert_eq!(dialog, studio_rows);
}

/// "Cursor here" puts the first cursor at the press, on the lane that was pressed.
#[test]
fn cursor_here_is_where_the_press_was() {
    let mut u = studio();
    let w = u.plot.wins[0];
    let x = w.x + w.w / 4;
    u.context_menu(x, w.y + w.h / 2);
    frame(&mut u);
    let (_, _, r) = u.studio_ctx_rows().into_iter().find(|(_, l, _)| l == "Cursor here").expect("a Cursor here row");
    let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x: cx, y: cy });
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x: cx, y: cy });
    frame(&mut u);
    assert_eq!(u.m.cursor, CursorMode::Track);
    let want = (x - w.x) as f32 / w.w as f32;
    assert!((u.m.cur_a - want).abs() < 0.01, "cursor A at {} of the window, the press was at {want}", u.m.cur_a);
    assert!(u.m.cur_b > u.m.cur_a && u.m.cur_b <= 1.0);
}

/// The plot's menu has the same names for the same things in both layouts.
#[test]
fn the_plot_menu_names_the_arrangements_alike_in_every_layout() {
    let mut d = Ui::new(Source::Demo { t: 0.0 });
    d.m.layout = Layout::Default;
    for _ in 0..10 {
        d.step(0.05);
    }
    frame(&mut d);
    let g = d.plot.grid;
    d.context_menu(g.x + g.w / 2, g.y + g.h / 2);
    let default: Vec<String> = d.workspace.popup.as_ref().expect("Default's popup").rows.iter().map(|r| r.0.clone()).collect();
    let mut s = studio();
    let w = s.plot.wins[0];
    s.context_menu(w.x + w.w / 2, w.y + w.h / 2);
    frame(&mut s);
    let studio_rows: Vec<String> = s.studio_ctx_rows().into_iter().map(|(_, l, _)| l).collect();
    for name in ["Overlay", "Stacked", "Windows", "Fit record"] {
        assert!(default.iter().any(|l| l == name), "Default: {default:?}");
        assert!(studio_rows.iter().any(|l| l == name), "Studio: {studio_rows:?}");
    }
    for gone in ["Overlay traces", "Stacked traces", "Four windows", "Autoset", "Single acquisition", "Run / Stop", "Sources", "Trigger settings", "Float measurements", "Center trigger"] {
        assert!(!default.iter().any(|l| l == gone) && !studio_rows.iter().any(|l| l == gone), "{gone} is not in the plot's menu");
    }
}
