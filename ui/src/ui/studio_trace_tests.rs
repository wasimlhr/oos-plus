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
    for _ in 0..12 { u.step(0.04); }
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = MathOp::Sub;
    u.m.ch[0].offset = 1.0;
    u.m.ch[1].offset = -1.5;
    if let Some(r) = u.refs[0].as_mut() { r.offset = -2.0; }
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    u.toast = None;
    u
}

fn picture(u: &mut Ui, name: &str) -> Canvas {
    for _ in 0..4 { u.step(0.04); }
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        std::fs::write(std::path::Path::new(&dir).join(format!("traces-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
    cv
}

/// The handles on the screen after a picture, by source.
fn handles(u: &Ui) -> Vec<(Trace, Rect)> {
    let mut v: Vec<(Trace, Rect)> = u.studio.lane_badges.borrow().iter().map(|&(c, r)| (Trace::Channel(c), r)).collect();
    v.extend(u.studio.handles.borrow().iter().copied());
    v
}

/// The zero level of a source in the lane `lane`, in divisions above the middle: where its handle belongs.
fn zero_divs(u: &Ui, source: Trace) -> f32 {
    match source {
        Trace::Channel(c) => u.m.ch[c].offset / u.m.ch[c].volts_per_div(),
        Trace::Reference(i) => { let r = u.refs[i].as_ref().unwrap(); r.offset / r.vpd }
        Trace::Math(k) => u.m.math_ch(k).offset,
    }
}

fn overlaps(a: Rect, b: Rect) -> bool { a.x < b.right() && b.x < a.right() && a.y < b.bottom() && b.y < a.bottom() }

/// Overlay is one graticule over the whole plot; Stacked is one lane per source, and the lanes fill the same plot.
#[test]
fn overlay_is_one_graticule_of_the_full_height_and_stacked_a_lane_per_source() {
    let mut u = bench();
    let sources = u.trace_sources();
    assert_eq!(sources, vec![Trace::Channel(0), Trace::Channel(1), Trace::Reference(0), Trace::Math(0)]);
    u.trace_mode(true);
    picture(&mut u, "stacked");
    assert_eq!(u.wins.len(), sources.len(), "a lane per source");
    let (top, bottom) = (u.wins[0].y, u.wins.last().unwrap().bottom());
    for pair in u.wins.windows(2) { assert!(pair[0].bottom() <= pair[1].y && pair[0].h == pair[1].h, "{pair:?}"); }
    u.trace_mode(false);
    picture(&mut u, "overlay");
    assert_eq!(u.wins.len(), 1, "one graticule");
    let g = u.wins[0];
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
            let lane = if stacked { u.wins[i] } else { u.wins[0] };
            let r = hs.iter().find(|(s, _)| s == source).unwrap_or_else(|| panic!("stacked {stacked}: {source:?} has a handle")).1;
            let zero = lane.y as f32 + lane.h as f32 / 2.0 - zero_divs(&u, *source) * lane.h as f32 / VDIV;
            // on the zero level, or pinned to the edge of the lane the level is beyond
            let centre = (r.y + r.h / 2) as f32;
            if (centre - zero).abs() > 1.5 {
                // (the last lane keeps the time labels and the Fit row of a reference free: 52 px and half a handle)
                let pinned = if zero < centre { r.y <= lane.y + 1 } else { r.bottom() >= lane.bottom() - 66 };
                assert!(pinned, "stacked {stacked}: {source:?} handle at {centre}, zero level at {zero}, lane {lane:?}");
            }
            assert!(r.y >= lane.y && r.bottom() <= lane.bottom(), "stacked {stacked}: {source:?} {r:?} in its lane {lane:?}");
            assert!(r.x >= lane.x && r.x < lane.x + 200, "stacked {stacked}: {source:?} at the left edge, {r:?}");
            assert!(r.w >= 40 && r.h >= 24);
        }
        for (i, a) in hs.iter().enumerate() {
            for b in &hs[i + 1..] { assert!(!overlaps(a.1, b.1), "stacked {stacked}: {a:?} covers {b:?}"); }
        }
    }
}

/// Sources with the same zero level in one graticule: their handles stand side by side on that level, in the order
/// of the sources, instead of on top of each other.
#[test]
fn handles_on_the_same_level_stand_side_by_side() {
    let mut u = bench();
    for c in 0..2 { u.m.ch[c].offset = 0.0; }
    if let Some(r) = u.refs[0].as_mut() { r.offset = 0.0; }
    u.trace_mode(false);
    picture(&mut u, "handles-same-level");
    let hs = handles(&u);
    let of = |s: Trace| hs.iter().find(|(k, _)| *k == s).unwrap().1;
    let (a, b, r) = (of(Trace::Channel(0)), of(Trace::Channel(1)), of(Trace::Reference(0)));
    assert!(a.y == b.y && b.y == r.y, "one level: {a:?} {b:?} {r:?}");
    assert!(a.x < b.x && b.x < r.x && a.right() <= b.x && b.right() <= r.x, "side by side in source order: {a:?} {b:?} {r:?}");
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
        let plot_bottom = u.wins.last().unwrap().bottom();
        for (source, r) in handles(&u) {
            let lane = if stacked { u.wins[u.trace_sources().iter().position(|s| *s == source).unwrap()] } else { u.wins[0] };
            assert!(r.y >= lane.y && r.bottom() <= lane.bottom(), "stacked {stacked}: {source:?} {r:?} in {lane:?}");
            if lane.bottom() == plot_bottom { assert!(r.bottom() <= plot_bottom - u.fonts().s.size - 8, "stacked {stacked}: {source:?} {r:?} over the time labels"); }
        }
    }
}

/// A tap on the handle of a reference or of a math channel opens its sheet, as a channel's handle opens the channel.
#[test]
fn a_tap_on_a_handle_opens_the_sheet_of_its_source() {
    let mut u = bench();
    u.trace_mode(false);
    picture(&mut u, "handle-tap");
    for (source, menu) in [(Trace::Reference(0), MenuId::Ref), (Trace::Math(0), MenuId::Math), (Trace::Channel(1), MenuId::Channel(1))] {
        let r = handles(&u).into_iter().find(|(s, _)| *s == source).unwrap().1;
        u.tap(r.x + r.w / 2, r.y + r.h / 2);
        for _ in 0..2 { u.step(0.04); }
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
            let lane = if stacked { u.wins[c] } else { u.wins[0] };
            for at in [lane.h / 5, lane.h / 2, lane.h * 3 / 4] {
                let (x, y) = (lane.x + lane.w / 2, lane.y + at);
                // in Overlay the menu is for the selected channel: the one the level is set on
                if !stacked { u.select_workspace_source(WaveId::Ch(c)); }
                u.studio_ctx_open(x, y);
                let rows = u.studio_ctx_rows();
                let (_, _, r) = rows.iter().find(|(_, label, _)| label == "Set level here").unwrap_or_else(|| panic!("stacked {stacked}: the row is there"));
                u.tap(r.x + r.w / 2, r.y + r.h / 2);
                for _ in 0..2 { u.step(0.04); }
                let ch = &u.m.ch[u.m.trig_src];
                let want = (lane.y as f32 + lane.h as f32 / 2.0 - y as f32) / (lane.h as f32 / VDIV) * ch.volts_per_div() - ch.offset;
                let pixel = ch.volts_per_div() * VDIV / lane.h as f32;
                assert!((u.m.trig_level - want).abs() <= 1.5 * pixel, "stacked {stacked} CH{} at y {y}: level {} V, the point is at {want} V", u.m.trig_src + 1, u.m.trig_level);
            }
        }
    }
}

/// The View menu and the popup of Add source are menus like the others: read and pressed by name over SCPI.
#[test]
fn the_view_menu_and_add_source_are_read_and_pressed_by_name() {
    let mut u = bench();
    let cmd = |u: &mut Ui, h: &str, arg: &str| -> String {
        let q = h.ends_with('?');
        let nodes: Vec<&str> = h.trim_end_matches('?').trim_start_matches(':').split(':').collect();
        let reply = u.scpi_ext_command(&nodes, q, arg).expect("a header of the layout commands").expect("no error");
        reply.map(|b| String::from_utf8(b).unwrap()).unwrap_or_default()
    };
    let view = u.studio_top().view;
    u.tap(view.x + view.w / 2, view.y + view.h / 2);
    u.step(0.04);
    let menu = cmd(&mut u, ":UI:MENU?", "");
    assert!(menu.starts_with("\"View\",\"Waveform focus\",\"Docked measurements\"") && menu.contains("\"Reset layout\""), "{menu}");
    cmd(&mut u, ":UI:MENU:SELect", "\"Docked measurements\"");
    for _ in 0..3 { u.step(0.04); }
    assert_eq!(u.studio.view, super::studio::StudioView::Docked);
    cmd(&mut u, ":UI:MENU:CLOSe", "");
    assert!(!u.studio.view_menu);
    let add = u.studio_bottom().add;
    u.tap(add.x + add.w / 2, add.y + add.h / 2);
    u.step(0.04);
    let menu = cmd(&mut u, ":UI:MENU?", "");
    assert!(menu.starts_with("\"Add source\",\"CH3\",\"CH4\"") && menu.ends_with("\"Add math\",\"Add ref\",\"Add bus\""), "{menu}");
    cmd(&mut u, ":UI:MENU:SELect", "\"CH3\"");
    for _ in 0..3 { u.step(0.04); }
    assert!(u.m.ch[2].on, "CH3 is on");
    assert_eq!(cmd(&mut u, ":UI:MENU?", ""), "\"\"");
}
