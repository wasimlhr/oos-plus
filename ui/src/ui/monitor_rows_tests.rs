// SPDX-License-Identifier: GPL-2.0-or-later

//! What the HDMI monitor shows follows what Display says (found by `funccheck_tests`), and its passes leave the panel's
//! touch targets alone.

use super::*;

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    u.step(0.02);
}

fn studio(hdmi: HdmiMode) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.studio.enter = true;
    u.m.ch[1].on = true;
    u.m.hdmi = hdmi;
    for _ in 0..10 {
        u.step(0.05);
    }
    u.m.run = RunState::Stop;
    frame(&mut u);
    u
}

/// HDMI lines acts in Waveform as in Layout, so its row is offered in both, and not in Mirror.
#[test]
fn hdmi_lines_is_offered_when_the_monitor_draws_its_own_picture() {
    for (mode, offered) in [(HdmiMode::Mirror, false), (HdmiMode::Waveform, true), (HdmiMode::Layout, true)] {
        let u = studio(mode);
        assert_eq!(u.items(MenuId::Display).contains(&Item::HdmiLines), offered, "{mode:?}");
    }
}

/// A menu that is open on the panel is not drawn over the view the monitor shows in its place.
#[test]
fn the_panels_menu_is_not_on_another_view_of_the_monitor() {
    let mut u = studio(HdmiMode::Layout);
    u.views_set(2);
    u.m.hdmi_view = 2;
    frame(&mut u);
    let mut a = Canvas::new(1920, 1080);
    u.render_view(&mut a);
    u.open_menu(MenuId::Display);
    frame(&mut u);
    assert!(u.menu_open);
    let mut b = Canvas::new(1920, 1080);
    u.render_view(&mut b);
    assert!(a.px == b.px, "the monitor's other view changed with the panel's menu");
    assert!(u.menu_open, "and the panel's menu is still open afterwards");
}

/// After the monitor's pass the panel's touch targets are the panel's again, not rectangles of the monitor's size.
#[test]
fn a_monitor_pass_leaves_the_panels_touch_targets() {
    for mode in [HdmiMode::Waveform, HdmiMode::Layout] {
        let mut u = studio(mode);
        let count = |u: &Ui| (u.studio.handles.borrow().len(), u.studio.label_rects.borrow().len(), u.band_hits.borrow().len(), u.studio.lane_badges.borrow().len());
        let before = count(&u);
        assert!(before.0 + before.1 + before.3 > 0, "the panel has touch targets to keep: {before:?}");
        let mut cv = Canvas::new(1920, 1080);
        if mode == HdmiMode::Layout {
            u.render_view(&mut cv);
        } else {
            u.render_waveform(&mut cv);
        }
        assert_eq!(before, count(&u), "{mode:?}");
        assert!(u.studio.handles.borrow().iter().all(|(_, r)| r.right() <= u.scr.w && r.bottom() <= u.scr.h), "{mode:?}: the targets are on the panel");
    }
}
