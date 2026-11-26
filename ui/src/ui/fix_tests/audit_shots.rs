// SPDX-License-Identifier: GPL-2.0-or-later
use super::super::workspace::Action;
use super::super::*;
use std::io::Write;

fn save(ui: &mut Ui, name: &str) {
    for _ in 0..3 {
        ui.step(0.05);
    }
    let mut cv = Canvas::new(W, H);
    ui.render(&mut cv);
    let dir = std::env::var("SHOTS").unwrap_or("/tmp".into());
    let mut f = std::fs::File::create(format!("{dir}/{name}.png")).unwrap();
    f.write_all(&crate::store::png_of(&cv)).unwrap();
}
fn fresh() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.relayout();
    for _ in 0..4 {
        u.step(0.05);
    }
    u
}
fn refs(u: &mut Ui, n: usize) {
    for i in 0..n {
        u.refs[i] = Some(RefWave {
            src: 0,
            color: i,
            label: format!("source-{i}"),
            t0: -2e-9,
            dt: 1e-10,
            v: ((0..128).map(|k| ((k as f32 + i as f32) * 0.2).sin() * 0.2).collect::<Vec<_>>()).into(),
            vpd: 0.1,
            offset: 0.0,
            on: true,
        });
    }
}
fn chs(u: &mut Ui, n: usize) {
    for c in 0..n {
        if !u.m.ch[c].on {
            u.activate_live_channel(c);
        }
    }
    u.relayout();
}
fn tap(u: &mut Ui, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
}

#[test]
fn audit_a() {
    let mut u = fresh();
    save(&mut u, "01_default");
    let mut u = fresh();
    u.open_menu(MenuId::Measure);
    save(&mut u, "02_menu_measure");
    let mut u = fresh();
    u.open_menu(MenuId::Display);
    save(&mut u, "03_menu_display");
    let mut u = fresh();
    u.open_menu(MenuId::Main);
    save(&mut u, "04_menu_main");
    for (n, d) in [
        ("under", StripDock::Below),
        ("left", StripDock::Left),
        ("right", StripDock::Right),
        ("above", StripDock::Above),
    ] {
        let mut u = fresh();
        u.m.strip_dock = d;
        u.stats_on = false;
        u.relayout();
        save(&mut u, &format!("05_strip_{n}"));
    }
    let mut u = fresh();
    u.m.meas.clear();
    u.relayout();
    save(&mut u, "05_strip_hidden_nomeas");
    for (n, d) in [
        ("under", StripDock::Below),
        ("left", StripDock::Left),
        ("right", StripDock::Right),
        ("above", StripDock::Above),
    ] {
        let mut u = fresh();
        u.workspace_action(Action::MeasureDock(d));
        save(&mut u, &format!("05_mdock_{n}"));
        chs(&mut u, 3);
        save(&mut u, &format!("05_mdock_{n}_3ch"));
    }
    for (n, d) in [("left", StripDock::Left), ("right", StripDock::Right), ("above", StripDock::Above)] {
        let mut u = fresh();
        chs(&mut u, 4);
        u.m.strip_dock = d;
        u.stats_on = false;
        u.relayout();
        save(&mut u, &format!("05_strip_{n}_4ch"));
    }
    let mut u = fresh();
    u.toggle_expand();
    u.relayout();
    save(&mut u, "06_full_on");
    u.toggle_expand();
    u.relayout();
    save(&mut u, "06_full_restored");
    // tap the full button
    let mut u = fresh();
    let (_, hit) = u.full_btn();
    tap(&mut u, hit.x + 2, hit.y + 2);
    save(&mut u, "06_full_tapped");
    let (_, hit) = u.full_btn();
    tap(&mut u, hit.x + 2, hit.y + 2);
    save(&mut u, "06_full_tapped_back");
}

#[test]
fn audit_b() {
    for n in [2usize, 3, 4] {
        let mut u = fresh();
        chs(&mut u, n);
        save(&mut u, &format!("07_overlay_{n}ch"));
        u.trace_mode(true);
        save(&mut u, &format!("07_stacked_{n}ch"));
        let mut u = fresh();
        chs(&mut u, n);
        u.trace_mode(true);
        u.set_zoom(true);
        save(&mut u, &format!("08_stacked_zoom_{n}ch"));
    }
    let mut u = fresh();
    chs(&mut u, 2);
    u.set_zoom(true);
    save(&mut u, "08_overlay_zoom_2ch");
    let mut u = fresh();
    chs(&mut u, 2);
    u.set_arrangement(super::super::windows::Arrangement::Windows);
    u.relayout();
    save(&mut u, "09_split2");
    let mut u = fresh();
    chs(&mut u, 4);
    u.set_arrangement(super::super::windows::Arrangement::Windows);
    u.relayout();
    save(&mut u, "09_split4");
    let mut u = fresh();
    chs(&mut u, 2);
    u.set_arrangement(super::super::windows::Arrangement::Windows);
    u.relayout();
    u.set_zoom(true);
    save(&mut u, "09_split2_zoom");
    let mut u = fresh();
    chs(&mut u, 2);
    u.set_arrangement(super::super::windows::Arrangement::Windows);
    u.relayout();
    u.relayout();
    save(&mut u, "09_split_restored");
}

#[test]
fn audit_c() {
    for (n, a) in [("right", 0), ("left", 1), ("below", 2), ("float", 3)] {
        let mut u = fresh();
        u.m.view = AView::Eye;
        u.relayout();
        save(&mut u, &format!("10_eye_default_{n}"));
        match a {
            0 => u.workspace_action(Action::EyeRight),
            1 => u.workspace_action(Action::EyeLeft),
            2 => u.workspace_action(Action::EyeBelow),
            _ => u.workspace_action(Action::EyeFloat),
        }
        save(&mut u, &format!("10_eye_{n}"));
    }
    let mut u = fresh();
    u.workspace_action(Action::MeasureFloat);
    save(&mut u, "11_meas_float");
    u.workspace_action(Action::MeasureDock(StripDock::Right));
    save(&mut u, "11_meas_dock_right");
    let mut u = fresh();
    u.context_menu(500, 250);
    save(&mut u, "12_context_plot");
    let mut u = fresh();
    chs(&mut u, 4);
    refs(&mut u, 3);
    u.context_menu(500, 250);
    save(&mut u, "12_context_plot_4ch_3ref");
    let mut u = fresh();
    tap(&mut u, 80, BOTTOM.y + 10);
    save(&mut u, "13_sources_popup");
    let mut u = fresh();
    chs(&mut u, 4);
    refs(&mut u, 3);
    u.relayout();
    save(&mut u, "14_refs3_4ch");
    let mut u = fresh();
    refs(&mut u, 2);
    u.relayout();
    save(&mut u, "14_refs2");
    tap(&mut u, 60, BOTTOM.y + 10);
    save(&mut u, "14_refs2_sources");
    let mut u = fresh();
    u.m.cursor = CursorMode::Time;
    u.relayout();
    save(&mut u, "15_cursor_time");
    let mut u = fresh();
    u.m.cursor = CursorMode::Xy;
    u.relayout();
    save(&mut u, "15_cursor_both");
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::MeasureFloat);
    save(&mut u, "16_eye_meas");
    // eye close restore
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::EyeRight);
    u.workspace_action(Action::EyeClose);
    save(&mut u, "17_eye_closed");
}

#[test]
fn audit_d() {
    // split then Overlay tap
    let mut u = fresh();
    chs(&mut u, 2);
    u.set_arrangement(super::super::windows::Arrangement::Windows);
    u.relayout();
    let (ov, _, _, _, _, _, _) = u.footer_rects();
    tap(&mut u, ov.x + 5, ov.y + 5);
    println!("AUDIT split->overlay: density={:?} win_layout={:?}", u.m.density, u.m.win_layout);
    save(&mut u, "20_split_then_overlay");
    // 4 split then stacked
    let mut u = fresh();
    chs(&mut u, 4);
    u.set_arrangement(super::super::windows::Arrangement::Windows);
    u.relayout();
    let (_, st, _, _, _, _, _) = u.footer_rects();
    tap(&mut u, st.x + 5, st.y + 5);
    println!(
        "AUDIT four->stacked: density={:?} win_layout={:?} stacked={}",
        u.m.density, u.m.win_layout, u.workspace.stacked
    );
    save(&mut u, "20_four_then_stacked");
    // Four -> back to Scope while stacked
    u.relayout();
    save(&mut u, "20_four_stacked_scope");
    // last channel off via Sources popup
    let mut u = fresh();
    tap(&mut u, 80, BOTTOM.y + 10);
    // the Minimal bar moved; without the popup the tap below lands where it was and the picture shows that
    let p = u.workspace.popup.as_ref().map(|p| p.rect).unwrap_or(Rect::new(60, BOTTOM.y, 1, 1));
    tap(&mut u, p.x + 20, p.y + 4 + 10);
    println!("AUDIT ch1 off: on={} sources={:?}", u.m.ch[0].on, u.trace_sources().len());
    save(&mut u, "21_last_ch_off");
    // stacked + full + restore
    let mut u = fresh();
    chs(&mut u, 3);
    u.trace_mode(true);
    u.toggle_expand();
    u.relayout();
    save(&mut u, "22_stacked_full");
    u.toggle_expand();
    u.relayout();
    save(&mut u, "22_stacked_full_restored");
    // eye side + full
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::EyeRight);
    u.toggle_expand();
    u.relayout();
    save(&mut u, "23_eye_side_full");
    // eye side + zoom
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::EyeRight);
    u.set_zoom(true);
    u.relayout();
    save(&mut u, "23_eye_side_zoom");
    // eye below + measurements strip + cursors
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::EyeBelow);
    u.m.cursor = CursorMode::Xy;
    u.relayout();
    save(&mut u, "23_eye_below_cursors");
    // eye side + strip right
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::EyeRight);
    u.m.strip_dock = StripDock::Right;
    u.relayout();
    save(&mut u, "23_eye_side_strip_right");
    // context menu on eye header and measurements header
    let mut u = fresh();
    u.m.view = AView::Eye;
    u.relayout();
    u.workspace_action(Action::EyeRight);
    let r = u.eye_pane_rect();
    u.context_menu(r.x + 100, r.y + 10);
    save(&mut u, "24_ctx_eye");
    let mut u = fresh();
    u.workspace_action(Action::MeasureFloat);
    let (h, _) = u.results_rects().unwrap();
    u.context_menu(h.x + 40, h.y + 10);
    save(&mut u, "24_ctx_meas");
    // context menu near bottom/right corner
    let mut u = fresh();
    u.context_menu(1000, 520);
    save(&mut u, "24_ctx_corner");
    // 4ch + 3 refs stacked
    let mut u = fresh();
    chs(&mut u, 4);
    refs(&mut u, 3);
    u.trace_mode(true);
    save(&mut u, "25_stacked_4ch_3ref");
    // many refs
    let mut u = fresh();
    chs(&mut u, 4);
    refs(&mut u, NREF);
    u.relayout();
    save(&mut u, "25_all_refs");
    // desktop panel buttons
    let mut u = fresh();
    u.desktop_panel = Some(false);
    u.relayout();
    save(&mut u, "26_desktop_panel");
    let mut u = fresh();
    u.desktop_panel = Some(false);
    chs(&mut u, 4);
    refs(&mut u, 3);
    u.relayout();
    save(&mut u, "26_desktop_panel_4ch_3ref");
    // touch lock
    let mut u = fresh();
    u.m.touch_lock = true;
    u.relayout();
    save(&mut u, "27_touch_lock");
    // measurement menu dock with stats on
    let mut u = fresh();
    u.m.strip_dock = StripDock::Left;
    u.stats_on = true;
    u.relayout();
    println!("AUDIT stats_on + dock Left: results_panel={}", u.results_panel());
    // zoom + float meas
    let mut u = fresh();
    u.set_zoom(true);
    u.workspace_action(Action::MeasureFloat);
    save(&mut u, "28_zoom_meas_float");
}
