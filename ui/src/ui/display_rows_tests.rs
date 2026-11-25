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
    let area = u.plot.area;
    u.context_menu(area.x + area.w / 2, area.y + area.h / 2);
    frame(&mut u);
    let (_, _, r) = u.studio_ctx_rows().into_iter().find(|(_, l, _)| l == "Autoset").expect("an Autoset row");
    let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
    for _ in 0..5 {
        frame(&mut u);
    }
}
