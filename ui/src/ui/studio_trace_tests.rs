// SPDX-License-Identifier: GPL-2.0-or-later
//! Stacked and Overlay in Studio: Stacked gives every source a lane with its own scale, Overlay puts every source on
//! one graticule of the full height. In both, every source (channel, reference, math) has one handle with its name
//! on its own zero level, and what is pressed in the plot acts on the lane under the finger.
//! `OOS_SHOTS=<dir>` writes a picture of each state the tests stop at.
use super::workspace::Trace;
use super::*;

/// Studio with CH1 and CH2 at different scales and positions, R1 (a copy of CH1, moved) and M1 = CH1 - CH2.
fn bench() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.studio.enter = true;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..12 {
        u.step(0.04);
    }
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = MathOp::Sub;
    u.m.ch[0].offset = 1.0;
    u.m.ch[1].offset = -1.5;
    if let Some(r) = u.refs[0].as_mut() {
        r.offset = -2.0;
    }
    u.relayout();
    for _ in 0..12 {
        u.step(0.04);
    }
    u.toast = None;
    u
}

fn picture(u: &mut Ui, name: &str) -> Canvas {
    for _ in 0..4 {
        u.step(0.04);
    }
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        std::fs::write(std::path::Path::new(&dir).join(format!("traces-{name}.png")), crate::store::png_of(&cv))
            .unwrap();
    }
    cv
}

/// The handles on the screen after a picture, by source.
fn handles(u: &Ui) -> Vec<(Trace, Rect)> {
    let mut v: Vec<(Trace, Rect)> =
        u.studio.lane_badges.borrow().iter().map(|&(c, r)| (Trace::Channel(c), r)).collect();
    v.extend(u.studio.handles.borrow().iter().copied());
    v
}

/// The zero level of a source in the lane `lane`, in divisions above the middle: where its handle belongs.
fn zero_divs(u: &Ui, source: Trace) -> f32 {
    match source {
        Trace::Channel(c) => u.m.ch[c].offset / u.m.ch[c].volts_per_div(),
        Trace::Reference(i) => {
            let r = u.refs[i].as_ref().unwrap();
            r.offset / r.vpd
        }
        Trace::Math(k) => u.m.math_ch(k).offset,
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom()
}

/// Overlay is one graticule over the whole plot; Stacked is one lane per source, and the lanes fill the same plot.
#[test]
fn overlay_is_one_graticule_of_the_full_height_and_stacked_a_lane_per_source() {
    let mut u = bench();
    let sources = u.trace_sources();
    assert_eq!(sources, vec![Trace::Channel(0), Trace::Channel(1), Trace::Reference(0), Trace::Math(0)]);
    u.trace_mode(true);
    picture(&mut u, "stacked");
    assert_eq!(u.plot.wins.len(), sources.len(), "a lane per source");
    let (top, bottom) = (u.plot.wins[0].y, u.plot.wins.last().unwrap().bottom());
    for pair in u.plot.wins.windows(2) {
        assert!(pair[0].bottom() <= pair[1].y && pair[0].h == pair[1].h, "{pair:?}");
    }
    u.trace_mode(false);
    picture(&mut u, "overlay");
    assert_eq!(u.plot.wins.len(), 1, "one graticule");
    let g = u.plot.wins[0];
    assert_eq!(g.y, top, "from the top of the plot");
    assert!((g.bottom() - bottom).abs() <= sources.len() as i32, "to its bottom: {} against {bottom}", g.bottom());
}

/// Every source has a handle with its name, on its own zero level, at the left edge of its lane (Stacked) or of
/// the one graticule (Overlay). No handle covers another.
#[test]
fn every_source_has_a_handle_on_its_zero_level_in_both_trace_modes() {
    for stacked in [true, false] {
        let mut u = bench();
        u.trace_mode(stacked);
        picture(&mut u, if stacked { "handles-stacked" } else { "handles-overlay" });
        let sources = u.trace_sources();
        let hs = handles(&u);
        assert_eq!(hs.len(), sources.len(), "stacked {stacked}: a handle per source, {hs:?}");
        for (i, source) in sources.iter().enumerate() {
            let lane = if stacked { u.plot.wins[i] } else { u.plot.wins[0] };
            let r = hs
                .iter()
                .find(|(s, _)| s == source)
                .unwrap_or_else(|| panic!("stacked {stacked}: {source:?} has a handle"))
                .1;
            let zero = lane.y as f32 + lane.h as f32 / 2.0 - zero_divs(&u, *source) * lane.h as f32 / VDIV;
            // on the zero level, or pinned to the edge of the lane the level is beyond
            let centre = (r.y + r.h / 2) as f32;
            if (centre - zero).abs() > 1.5 {
                // (the last lane keeps the time labels and the Fit row of a reference free: 52 px and half a handle)
                let pinned = if zero < centre { r.y <= lane.y + 1 } else { r.bottom() >= lane.bottom() - 66 };
                assert!(
                    pinned,
                    "stacked {stacked}: {source:?} handle at {centre}, zero level at {zero}, lane {lane:?}"
                );
            }
            assert!(
                r.y >= lane.y && r.bottom() <= lane.bottom(),
                "stacked {stacked}: {source:?} {r:?} in its lane {lane:?}"
            );
            assert!(
                r.right() <= lane.right() && r.x > lane.x + lane.w / 2,
                "stacked {stacked}: {source:?} at the right edge, {r:?}"
            );
            assert!(r.w >= 40 && r.h >= 24);
        }
        for (i, a) in hs.iter().enumerate() {
            for b in &hs[i + 1..] {
                assert!(!overlaps(a.1, b.1), "stacked {stacked}: {a:?} covers {b:?}");
            }
        }
    }
}

/// Sources with the same zero level in one graticule: their handles stand side by side on that level, in the order
/// of the sources from the right edge in, instead of on top of each other.
#[test]
fn handles_on_the_same_level_stand_side_by_side() {
    let mut u = bench();
    for c in 0..2 {
        u.m.ch[c].offset = 0.0;
    }
    if let Some(r) = u.refs[0].as_mut() {
        r.offset = 0.0;
    }
    u.trace_mode(false);
    picture(&mut u, "handles-same-level");
    let hs = handles(&u);
    let of = |s: Trace| hs.iter().find(|(k, _)| *k == s).unwrap().1;
    let (a, b, r) = (of(Trace::Channel(0)), of(Trace::Channel(1)), of(Trace::Reference(0)));
    assert!(a.y == b.y && b.y == r.y, "one level: {a:?} {b:?} {r:?}");
    assert!(
        a.x > b.x && b.x > r.x && b.right() <= a.x && r.right() <= b.x,
        "side by side in source order, from the edge in: {a:?} {b:?} {r:?}"
    );
}

/// A zero level far outside the plot pins the handle inside it, above the time labels.
#[test]
fn a_handle_stays_in_the_plot_and_clear_of_the_time_labels() {
    for stacked in [true, false] {
        let mut u = bench();
        u.m.ch[0].offset = -40.0;
        u.m.ch[1].offset = 40.0;
        u.trace_mode(stacked);
        picture(&mut u, if stacked { "handles-pinned-stacked" } else { "handles-pinned-overlay" });
        let plot_bottom = u.plot.wins.last().unwrap().bottom();
        for (source, r) in handles(&u) {
            let lane = if stacked {
                u.plot.wins[u.trace_sources().iter().position(|s| *s == source).unwrap()]
            } else {
                u.plot.wins[0]
            };
            assert!(r.y >= lane.y && r.bottom() <= lane.bottom(), "stacked {stacked}: {source:?} {r:?} in {lane:?}");
            if lane.bottom() == plot_bottom {
                assert!(
                    r.bottom() <= plot_bottom - u.fonts().s.size - 8,
                    "stacked {stacked}: {source:?} {r:?} over the time labels"
                );
            }
        }
    }
}

/// A channel the user named (`:CHANnel<n>:LABel:CONTent`) shows its name on its handle, and the handle grows to hold it.
#[test]
fn a_named_channel_has_its_name_on_its_handle() {
    let mut u = bench();
    picture(&mut u, "handle-unnamed-channel");
    let plain = handles(&u).iter().find(|(s, _)| *s == Trace::Channel(0)).map(|(_, r)| *r).unwrap_or_default();
    assert_eq!(plain.w, 44, "an unnamed channel keeps the narrow handle");
    u.m.ch_label[0] = "FR_TXRX".into();
    picture(&mut u, "handle-named-channel");
    let named = handles(&u).iter().find(|(s, _)| *s == Trace::Channel(0)).map(|(_, r)| *r).unwrap();
    assert!(named.w > 44, "the handle holds \"FR_TXRX\": {named:?}");
    assert!(named.right() <= u.plot.area.right(), "and stays inside the plot: {named:?}");
    assert_eq!(u.ch_name(0), "FR_TXRX");
}

/// A math trace takes its name the same way, and a name too long for a narrow pill is cut with a dot behind it, not
/// replaced by CHn; a scale label moves clear of a handle.
#[test]
fn a_named_math_trace_and_a_narrow_pill_keep_their_names() {
    let mut u = bench();
    u.m.math_label[0] = "Vdiff_FRBP".into();
    picture(&mut u, "handle-named-math");
    let math = handles(&u).iter().find(|(s, _)| *s == Trace::Math(0)).map(|(_, r)| *r).expect("math handle");
    assert!(math.w > 44, "the handle holds \"Vdiff_FRBP\": {math:?}");
    assert_eq!(u.math_name(0), "Vdiff_FRBP");
    assert_eq!(u.math_name(1), "M2");
    // a pill 100 px wide has half its width for the name; a 60 px one has nearly all
    u.m.ch_label[0] = "FR_TXRX_LONG".into();
    let (text, bw) = u.pill_name(&u.ch_name(0), 100);
    assert!(text.ends_with('.') && text.starts_with("FR_"), "cut with a dot: {text}");
    assert!(bw <= 50, "inside half the pill: {bw}");
    let (text, bw) = u.pill_name("Vcc", 60);
    assert_eq!(text, "Vcc");
    assert!(bw <= 56, "{bw}");
    // an unnamed one stays as it was
    assert_eq!(u.studio_pill_badge(1, 100).0, "CH2");
    assert_eq!(u.studio_pill_badge(1, 100).1, 42);
}

/// A tap on the handle of a reference or of a math channel opens its sheet, as a channel's handle opens the channel.
#[test]
fn a_tap_on_a_handle_opens_the_sheet_of_its_source() {
    let mut u = bench();
    u.trace_mode(false);
    picture(&mut u, "handle-tap");
    for (source, menu) in
        [(Trace::Reference(0), MenuId::Ref), (Trace::Math(0), MenuId::Math), (Trace::Channel(1), MenuId::Channel(1))]
    {
        let r = handles(&u).into_iter().find(|(s, _)| *s == source).unwrap().1;
        u.tap(r.x + r.w / 2, r.y + r.h / 2);
        for _ in 0..2 {
            u.step(0.04);
        }
        assert!(u.menu_open && u.menu == menu, "{source:?} opens {menu:?}, not {:?}", u.menu);
        u.close_menu();
        picture(&mut u, "handle-tap");
    }
}

/// The lanes and the handles are on the list of the screen's controls (`:UI:ELEMents?`), by the name of their source.
#[test]
fn the_handles_are_listed_as_elements_with_the_lanes() {
    for stacked in [true, false] {
        let mut u = bench();
        u.trace_mode(stacked);
        picture(&mut u, "elements");
        let els = u.ui_elements();
        let lanes: Vec<&super::scpi_layout::Element> = els.iter().filter(|e| e.kind == "LANE").collect();
        let names: Vec<&str> = els.iter().filter(|e| e.kind == "HANDLE").map(|e| e.label.as_str()).collect();
        assert_eq!(names, ["CH1", "CH2", "R1", "M1"], "stacked {stacked}");
        if stacked {
            assert_eq!(lanes.iter().map(|e| e.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH2", "R1", "M1"]);
        } else {
            assert_eq!(lanes.len(), 1);
            assert_eq!(lanes[0].label, "All sources");
        }
    }
}

/// Set level here puts the trigger level under the finger, on the scale of the lane's channel, in both modes.
#[test]
fn set_level_here_is_the_level_under_the_finger() {
    for stacked in [true, false] {
        let mut u = bench();
        u.trace_mode(stacked);
        picture(&mut u, "level");
        for c in 0..2 {
            let lane = if stacked { u.plot.wins[c] } else { u.plot.wins[0] };
            for at in [lane.h / 5, lane.h / 2, lane.h * 3 / 4] {
                let (x, y) = (lane.x + lane.w / 2, lane.y + at);
                // in Overlay the menu is for the selected channel: the one the level is set on
                if !stacked {
                    u.select_workspace_source(WaveId::Ch(c));
                }
                u.studio_ctx_open(x, y);
                let rows = u.studio_ctx_rows();
                let (_, _, r) = rows
                    .iter()
                    .find(|(_, label, _)| label == "Set level here")
                    .unwrap_or_else(|| panic!("stacked {stacked}: the row is there"));
                u.tap(r.x + r.w / 2, r.y + r.h / 2);
                for _ in 0..2 {
                    u.step(0.04);
                }
                let ch = &u.m.ch[u.m.trig_src];
                let want = (lane.y as f32 + lane.h as f32 / 2.0 - y as f32) / (lane.h as f32 / VDIV)
                    * ch.volts_per_div()
                    - ch.offset;
                let pixel = ch.volts_per_div() * VDIV / lane.h as f32;
                assert!(
                    (u.m.trig_level - want).abs() <= 1.5 * pixel,
                    "stacked {stacked} CH{} at y {y}: level {} V, the point is at {want} V",
                    u.m.trig_src + 1,
                    u.m.trig_level
                );
            }
        }
    }
}

/// Studio's rows of the Display sheet and the popup of Add source are read and pressed by name over SCPI, like the
/// rows of any menu.
#[test]
fn the_studio_rows_of_display_and_add_source_are_read_and_pressed_by_name() {
    let mut u = bench();
    let cmd = |u: &mut Ui, h: &str, arg: &str| -> String {
        let q = h.ends_with('?');
        let nodes: Vec<&str> = h.trim_end_matches('?').trim_start_matches(':').split(':').collect();
        let reply = u.scpi_ext_command(&nodes, q, arg).expect("a header of the layout commands").expect("no error");
        reply.map(|b| String::from_utf8(b).unwrap()).unwrap_or_default()
    };
    let display = u.studio_top().display;
    u.tap(display.x + display.w / 2, display.y + display.h / 2);
    u.step(0.04);
    let menu = cmd(&mut u, ":UI:MENU?", "");
    assert!(
        menu.starts_with("\"Display\"")
            && menu.contains("\"Pinned badges = On\"")
            && menu.contains("\"Compact view = Off\"")
            && menu.contains("\"Reset layout\""),
        "{menu}"
    );
    cmd(&mut u, ":UI:MENU:SELect", "\"Pinned badges\"");
    assert!(u.studio.hide_badges && u.menu_open, "a switch leaves the sheet open");
    assert!(cmd(&mut u, ":UI:MENU?", "").contains("\"Pinned badges = Off\""));
    u.studio_set_view(super::studio::StudioView::Docked);
    cmd(&mut u, ":UI:MENU:SELect", "\"Reset layout\"");
    for _ in 0..3 {
        u.step(0.04);
    }
    assert!(u.studio.view == super::studio::StudioView::Waveform && !u.studio.hide_badges && !u.menu_open);
    cmd(&mut u, ":UI:MENU:CLOSe", "");
    let add = u.studio_bottom().add;
    u.tap(add.x + add.w / 2, add.y + add.h / 2);
    u.step(0.04);
    let menu = cmd(&mut u, ":UI:MENU?", "");
    assert!(
        menu.starts_with("\"Add source\",\"CH3\",\"CH4\"") && menu.ends_with("\"Add math\",\"Add ref\",\"Add bus\""),
        "{menu}"
    );
    cmd(&mut u, ":UI:MENU:SELect", "\"CH3\"");
    for _ in 0..3 {
        u.step(0.04);
    }
    assert!(u.m.ch[2].on, "CH3 is on");
    assert_eq!(cmd(&mut u, ":UI:MENU?", ""), "\"\"");
}

// ------------------------------------------------------------------------------------------------ the zoom row

fn zoomed(layout: Layout) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    u.studio.enter = layout == Layout::Studio;
    u.relayout();
    for _ in 0..12 {
        u.step(0.04);
    }
    u.m.run = RunState::Stop;
    u.set_zoom(true);
    for _ in 0..6 {
        u.step(0.04);
    }
    u.toast = None;
    u
}

/// Studio has the zoom row of the Default layout, between the overview and the zoomed lanes: the same text and the
/// same buttons, pixel for pixel, and each button does what it does there. The window in the overview is the same
/// slim frame. Studio's own pull-down tab, toolbar and zoom menu are gone.
#[test]
fn studio_has_the_zoom_row_and_the_window_of_the_default_layout() {
    let (mut d, mut s) = (zoomed(Layout::Default), zoomed(Layout::Studio));
    let (cd, cs) = (picture(&mut d, "zoom-default"), picture(&mut s, "zoom-studio"));
    let (rd, rs) = (d.zoom_readout(), s.zoom_readout());
    assert_eq!((rd.w, rd.h), (rs.w, rs.h), "the row has the size of the Default row: {rd:?} {rs:?}");
    let (od, os) = (d.split_zoom(d.plot.grid).0, s.split_zoom(s.plot.grid).0);
    assert_eq!(rs.y, os.bottom(), "under the overview");
    assert_eq!(rs.bottom(), s.plot.wins[0].y, "over the zoomed lanes");
    // the row: the same pixels (the text, H-, H+, V-, V+, Box, Reset, Close)
    let px = |cv: &Canvas, x: i32, y: i32| cv.px[(y * cv.w + x) as usize];
    // (but for the frame of the Waveform view, a pixel column at each end)
    let differ: Vec<(i32, i32)> = (0..rd.h)
        .flat_map(|y| (2..rd.w - 2).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&cd, rd.x + x, rd.y + y) != px(&cs, rs.x + x, rs.y + y))
        .collect();
    assert!(
        differ.is_empty(),
        "the zoom row of Studio differs from the Default row in {} pixels, from {:?} to {:?}",
        differ.len(),
        differ.first(),
        differ.last()
    );
    // the window in the overview: a frame one pixel wide in the accent colour at the same place, not a shaded box
    let (wd, ws) = (d.zoom_window_x(od), s.zoom_window_x(os));
    assert_eq!((wd.0 - od.x, wd.1 - od.x), (ws.0 - os.x, ws.1 - os.x), "the same window");
    let inside = px(&cs, (ws.0 + ws.1) / 2, os.y + 3);
    let outside_frame = px(&cs, ws.0 + 1, os.y);
    assert_ne!(inside, outside_frame, "the window is a frame: its top line stands out from its inside");
    assert_eq!(px(&cs, ws.0, os.y + 2), px(&cd, wd.0, od.y + 2), "the frame has the colour of the Default one");
    // every button, by the names Studio gives them
    let bar = s.studio_zoom_bar().expect("the row");
    let c = d.zoom_controls();
    assert_eq!(
        [bar.minus.w, bar.plus.w, bar.vminus.w, bar.vplus.w, bar.boxr.w],
        [c[0].0.w, c[1].0.w, c[2].0.w, c[3].0.w, c[4].0.w]
    );
    let mid = |r: Rect| (r.x + r.w / 2, r.y + r.h / 2);
    let z0 = s.m.zscale;
    let (x, y) = mid(bar.plus);
    s.tap(x, y);
    assert!(s.m.zscale < z0, "H+ zooms in");
    let (x, y) = mid(bar.minus);
    s.tap(x, y);
    assert_eq!(s.m.zscale, z0, "H- zooms out again");
    let (x, y) = mid(bar.vplus);
    s.tap(x, y);
    assert_eq!(s.zoom_view.vfactor, 2.0, "V+ doubles the display scale");
    let (x, y) = mid(bar.vminus);
    s.tap(x, y);
    assert_eq!(s.zoom_view.vfactor, 1.0, "V- halves it");
    let (x, y) = mid(bar.boxr);
    s.tap(x, y);
    assert!(s.zoom_box.arm, "Box arms the zoom box");
    let (x, y) = mid(bar.plus);
    s.tap(x, y);
    let (x, y) = mid(bar.fit);
    s.tap(x, y);
    assert_eq!(s.m.zscale, s.m.tscale, "Reset: the window is the whole screen");
    let (x, y) = mid(bar.close);
    s.tap(x, y);
    assert!(!s.m.zoom && s.studio_zoom_bar().is_none(), "Close: zoom off, no row");
    // the Horizontal menu of Studio has no row of its own for the place of the zoom controls
    s.set_zoom(true);
    assert_eq!(s.items(MenuId::Horizontal), d.items(MenuId::Horizontal), "the Horizontal rows are those of Default");
}

/// The buttons of the zoom row and the one button of the badge row ("Meas" and the count) are on the list of the
/// screen's controls. The Meas button opens the table of the measurements; it has no menu.
#[test]
fn the_zoom_row_and_the_meas_button_are_listed_and_pressed_by_name() {
    let mut u = zoomed(Layout::Studio);
    picture(&mut u, "zoom-elements");
    let els = u.ui_elements();
    let zoom: Vec<&str> = els.iter().filter(|e| e.kind == "ZOOM").map(|e| e.label.as_str()).collect();
    assert_eq!(zoom, ["H-", "H+", "V-", "V+", "Box", "Reset", "Close"]);
    let close = els.iter().find(|e| e.kind == "ZOOM" && e.label == "Close").unwrap().rect;
    u.tap(close.x + close.w / 2, close.y + close.h / 2);
    assert!(!u.m.zoom && !u.ui_elements().iter().any(|e| e.kind == "ZOOM"), "no zoom, no row");
    let cmd = |u: &mut Ui, h: &str, arg: &str| -> String {
        let q = h.ends_with('?');
        let nodes: Vec<&str> = h.trim_end_matches('?').trim_start_matches(':').split(':').collect();
        let reply = u.scpi_ext_command(&nodes, q, arg).expect("a header of the layout commands").expect("no error");
        reply.map(|b| String::from_utf8(b).unwrap()).unwrap_or_default()
    };
    let btn = u
        .ui_elements()
        .into_iter()
        .find(|e| e.kind == "BADGE" && e.label == "Meas")
        .expect("the button of the badge row")
        .rect;
    assert_eq!(btn, u.studio_list_chip());
    assert_ne!(u.studio.view, super::studio::StudioView::Docked);
    u.tap(btn.x + btn.w / 2, btn.y + btn.h / 2);
    for _ in 0..3 {
        u.step(0.04);
    }
    assert_eq!(u.studio.view, super::studio::StudioView::Docked, "the Meas button opens the table of the measurements");
    assert_eq!(cmd(&mut u, ":UI:MENU?", ""), "\"\"", "and no menu");
    // a hidden row has no badges and no button on the list
    let mut u = zoomed(Layout::Studio);
    assert!(u.ui_elements().iter().any(|e| e.kind == "BADGE"));
    u.studio_hide_badges();
    assert!(!u.ui_elements().iter().any(|e| e.kind == "BADGE"), "hidden badges are not controls on the screen");
}

/// The plot's menu has Center trigger while zoom is on, between Reset zoom and Close zoom: it puts the trigger in the
/// middle of the zoom window. It is read and pressed by name, and the menu still fits the screen.
#[test]
fn the_plot_menu_centres_the_trigger_in_the_zoom_window() {
    let mut u = zoomed(Layout::Studio);
    let cmd = |u: &mut Ui, h: &str, arg: &str| -> String {
        let q = h.ends_with('?');
        let nodes: Vec<&str> = h.trim_end_matches('?').trim_start_matches(':').split(':').collect();
        let reply = u.scpi_ext_command(&nodes, q, arg).expect("a header of the layout commands").expect("no error");
        reply.map(|b| String::from_utf8(b).unwrap()).unwrap_or_default()
    };
    u.m.zdelay = u.view().0 * 2.0;
    u.clamp_zoom();
    u.relayout();
    assert!(u.m.zdelay != 0.0, "the zoom window starts off the trigger");
    let lane = u.plot.wins[0];
    let at = format!("{},{}", lane.x + lane.w / 3, lane.y + lane.h / 2);
    cmd(&mut u, ":UI:CONText", &at);
    u.step(0.04);
    picture(&mut u, "plot-menu-center-trigger");
    let menu = cmd(&mut u, ":UI:MENU?", "");
    assert!(menu.contains("\"Reset zoom\",\"Close zoom\""), "{menu}");
    let (p, rows) = (u.studio_ctx_rect().expect("the menu"), u.studio_ctx_rows());
    assert!(p.y >= 0 && p.bottom() <= u.scr.h, "the menu is on the screen: {p:?}");
    assert!(
        rows.iter().all(|(_, _, r)| r.h >= 24 && r.bottom() <= p.bottom()),
        "every row is inside it and can be pressed"
    );
    // the zoom row's Center puts the window on the trigger (the plot's menu has no row for it)
    cmd(&mut u, ":UI:MENU:CLOSe", "");
    u.studio_center_trigger();
    u.relayout();
    for _ in 0..3 {
        u.step(0.04);
    }
    assert_eq!(u.m.zdelay, 0.0, "the zoom window is on the trigger");
    let lane = u.plot.wins[0];
    assert!(
        (u.studio_x_of_time(0.0) - (lane.x + lane.w / 2)).abs() <= 1,
        "T at {} of a lane from {} to {}",
        u.studio_x_of_time(0.0),
        lane.x,
        lane.right()
    );
    assert!(u.studio.ctx.is_none(), "the menu is closed");
    // without zoom there is nothing to centre: no row
    u.set_zoom(false);
    for _ in 0..3 {
        u.step(0.04);
    }
    let lane = u.plot.wins[0];
    let at = format!("{},{}", lane.x + lane.w / 3, lane.y + lane.h / 2);
    cmd(&mut u, ":UI:CONText", &at);
    u.step(0.04);
    assert!(!cmd(&mut u, ":UI:MENU?", "").contains("Center trigger"));
}
