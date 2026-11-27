// SPDX-License-Identifier: GPL-2.0-or-later

//! The picture a monitor on the HDMI port gets (Display > HDMI = Waveform or Layout), in the states of the panel the
//! layouts can be in. With `OOS_SHOTS=<dir>` every scene is written as `hdmi-<scene>-<mode>.png`.

use super::dock::VKind;
use super::studio::StudioView;
use super::*;

const W: i32 = 1024;
const H: i32 = 600;
const MW: i32 = 1920;
const MH: i32 = 1080;

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    u.step(0.02);
}

fn start(layout: Layout) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    u.m.ch[1].on = true;
    u.studio.enter = layout == Layout::Studio;
    u.m.meas = [Meas::Vpp, Meas::Freq, Meas::Vrms, Meas::Vmax, Meas::Vmin, Meas::Rise]
        .into_iter()
        .flat_map(|k| [(k, WaveId::Ch(0)), (k, WaveId::Ch(1))])
        .collect();
    for _ in 0..10 {
        u.step(0.05);
    }
    frame(&mut u);
    u
}

/// The scenes: a name and how the panel gets there.
fn scenes() -> Vec<(&'static str, Ui)> {
    let mut out: Vec<(&'static str, Ui)> = Vec::new();
    let mut add = |name: &'static str, layout: Layout, f: &dyn Fn(&mut Ui)| {
        let mut u = start(layout);
        f(&mut u);
        for _ in 0..8 {
            frame(&mut u);
        }
        out.push((name, u));
    };
    add("default", Layout::Default, &|_| {});
    add("default-decode", Layout::Default, &|u| {
        u.m.dec_proto = Proto::UART;
        u.m.dec_on = true;
    });
    add("minimal", Layout::Minimal, &|_| {});
    add("studio-wave", Layout::Studio, &|_| {});
    add("studio-panel-strip", Layout::Studio, &|u| u.studio_set_view(StudioView::Docked));
    add("studio-panel-table", Layout::Studio, &|u| {
        u.studio_set_view(StudioView::Docked);
        u.m.studio_detail = true;
    });
    add("studio-panel-cards", Layout::Studio, &|u| {
        u.studio_set_view(StudioView::Docked);
        u.studio.panel.cards = true;
    });
    add("studio-tabs-meas", Layout::Studio, &|u| {
        u.studio_set_view(StudioView::Docked);
        frame(u);
        u.dock_maximise(VKind::Measure);
    });
    add("studio-tabs-wave", Layout::Studio, &|u| {
        u.studio_set_view(StudioView::Docked);
        frame(u);
        u.dock_maximise(VKind::Wave);
    });
    add("studio-hist", Layout::Studio, &|u| u.m.hist = Hist::Vertical);
    add("studio-spectrum", Layout::Studio, &|u| u.m.view = AView::Spectrum);
    add("studio-decode", Layout::Studio, &|u| {
        u.m.dec_proto = Proto::UART;
        u.m.dec_on = true;
    });
    add("studio-floating", Layout::Studio, &|u| u.studio_set_view(StudioView::Floating));
    // the arrangement seen on the owner's unit: waveform, eye and histogram in tabs, the measurements floating
    add("studio-tabs-float", Layout::Studio, &|u| {
        u.studio_apply_panel(super::sheet::PanelKind::Eye);
        u.m.hist = Hist::Vertical;
        frame(u);
        u.dock_maximise(VKind::Wave);
        frame(u);
        u.studio_set_view(StudioView::Floating);
        frame(u);
        eprintln!("dock: {}", u.dock_text());
    });
    out
}

fn save(cv: &Canvas, name: &str) {
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("hdmi-{name}.png")), crate::store::png_of(cv)).unwrap();
    }
}

#[test]
fn the_monitor_gets_a_picture_in_every_scene() {
    for (name, mut u) in scenes() {
        let mut panel = Canvas::new(W, H);
        u.render(&mut panel);
        save(&panel, &format!("{name}-panel"));
        for (mode, tag) in [(HdmiMode::Waveform, "waveform"), (HdmiMode::Layout, "layout")] {
            u.m.hdmi = mode;
            let mut hc = Canvas::new(MW, MH);
            if mode == HdmiMode::Layout {
                u.render_view(&mut hc);
            } else {
                u.render_waveform(&mut hc);
            }
            save(&hc, &format!("{name}-{tag}"));
            // the panel is as it was after the monitor's pass
            let mut again = Canvas::new(W, H);
            u.render(&mut again);
            assert_eq!((u.scr.w, u.scr.h), (W, H), "{name} {tag}: the panel's size is back");
        }
    }
}

/// The desktop window draws at the size it has: after `set_screen` the picture is laid out and drawn at that size, and
/// the touch targets of that layout answer (a tap on the Menu button opens the menu), at a few shapes of window.
#[test]
fn a_ui_laid_out_for_a_window_answers_at_its_size() {
    for (w, h) in [(1280, 720), (1500, 900), (1920, 1080), (1728, 1080)] {
        let mut u = start(Layout::Studio);
        u.set_screen(Screen { w, h });
        for _ in 0..4 { u.step(0.03); let mut c = Canvas::new(w, h); u.render(&mut c); }
        let mut c = Canvas::new(w, h);
        u.render(&mut c);
        assert_eq!((u.scr.w, u.scr.h), (w, h));
        // the bars span the picture: ink along the whole top edge, and in the last columns
        let top = |x: i32| (0..40).any(|y| c.px[(y * w + x) as usize] != c.px[(50 * w + x) as usize]);
        assert!(top(w - 30), "{w}x{h}: the top bar reaches the right edge");
        // the menu button of the layout opens the menu
        let menu = u.menu_btn();
        assert!(menu.y > h * 3 / 4 && menu.bottom() <= h, "{w}x{h}: the Menu button is in the bottom bar: {menu:?}");
        u.tap(menu.x + menu.w / 2, menu.y + menu.h / 2);
        assert!(u.menu_open, "{w}x{h}: a tap on the Menu button opens the menu");
        if let Some(d) = std::env::var_os("OOS_SHOTS") {
            let mut c = Canvas::new(w, h);
            u.step(0.03);
            u.render(&mut c);
            std::fs::write(std::path::Path::new(&d).join(format!("window-{w}x{h}.png")), crate::store::png_of(&c)).unwrap();
        }
    }
}
