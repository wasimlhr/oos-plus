// SPDX-License-Identifier: GPL-2.0-or-later

//! The searches of Studio with zoom: ticks in the overview, marks in the zoomed part, and previous / next move the
//! zoom window.

use super::addrail::{SearchSpec, SEARCH_COLS};
use super::render_marks::{SEARCH_LANE_H, SEARCH_STRIP_H};
use super::search_dense_tests::{of_samples, px, shot};
use super::*;

/// 20 ms at 1 us a sample on CH2, a 1 kHz square with a pulse of 50 us at 12.9 ms; 1 ms/div around the middle of the
/// record, zoomed to 100 us/div. Search 1: every edge; Search 2: pulses under 100 us (one); Search 3: rising edges.
fn zoomed() -> Ui {
    let trap = |t: f64, start: f64, end: f64| 3.0 * (((t - start) / 10.0).clamp(0.0, 1.0) - ((t - end) / 10.0).clamp(0.0, 1.0)).max(0.0);
    let data: Vec<f32> = (0..20_000)
        .map(|i| {
            let t = i as f64;
            ((0..20).map(|k| trap(t, k as f64 * 1000.0 + 250.0, k as f64 * 1000.0 + 750.0)).sum::<f64>() + trap(t, 12_900.0, 12_950.0)) as f32
        })
        .collect();
    let mut u = of_samples(data, 1e-6, (0.0, 0));
    u.m.tscale = tscales().iter().position(|&t| (t - 1e-3).abs() < 1e-9).unwrap();
    u.m.delay = 0.0;
    u.set_zoom(true);
    u.m.zscale = tscales().iter().position(|&t| (t - 100e-6).abs() < 1e-9).unwrap();
    u.m.zdelay = 0.0;
    for (kind, slope) in [(TrigType::Edge, 2), (TrigType::Pulse, 0), (TrigType::Edge, 0)] {
        u.studio_search_add(Some(WaveId::Ch(1)));
        let sp = u.studio.search().unwrap();
        u.studio.search_put(SearchSpec { kind, slope, when: WhenKind::Lt, t1: 100e-6, ..sp });
    }
    u
}

fn frame(u: &mut Ui) -> Canvas {
    let mut cv = Canvas::new(W, H);
    for _ in 0..4 { u.step(0.05); }
    u.render(&mut cv);
    cv
}

#[test]
fn the_overview_has_a_line_of_ticks_for_each_search_and_the_zoomed_part_its_marks() {
    let mut u = zoomed();
    assert_eq!([0, 1, 2].map(|i| u.studio_search_events_of(i).len()), [42, 1, 21]);
    let cv = frame(&mut u);
    shot(&cv, "search-zoom");
    let (o, z) = u.split_zoom(u.plot.grid);
    // the overview shows 10 ms of the record: ten of the 1 ms periods, the narrow pulse among them
    // (a note at the top left of the plot covers the first ticks of the lower lines)
    for (slot, want) in [(0, 18..=24), (1, 1..=1), (2, 8..=12)] {
        let y = o.y + 1 + slot * (SEARCH_STRIP_H + 1) + 1;
        let ticks = (o.x..o.right()).filter(|&x| px(&cv, x, y) == SEARCH_COLS[slot as usize]).count();
        assert!(want.contains(&ticks), "search {}: {ticks} ticks in the overview", slot + 1);
    }
    // the tick of the pulse is at the time of its event in the main view
    let x = (o.x..o.right()).find(|&x| px(&cv, x, o.y + 1 + SEARCH_STRIP_H + 2) == SEARCH_COLS[1]).unwrap();
    let at = u.studio_search_events_of(1)[0];
    assert!(at > 2e-3 && at < 4e-3, "{at}");
    let want = o.x + ((at / (1e-3 * HDIV as f64) + 0.5) * o.w as f64) as i32;
    assert!((x - want).abs() <= 2, "{x} for {want}");
    // the zoomed part (1 ms around the trigger) has the marks of the searches that have an event there
    let mark = |slot: usize| (z.x..z.right()).filter(|&x| (0..SEARCH_LANE_H).any(|dy| px(&cv, x, u.plot.area.y + slot as i32 * SEARCH_LANE_H + dy) == SEARCH_COLS[slot])).count();
    assert!(mark(0) > 0 && mark(2) > 0, "{} {}", mark(0), mark(2));
    assert_eq!(mark(1), 0, "the pulse is not in the zoomed part");
}

#[test]
fn previous_and_next_move_the_zoom_window_and_take_the_main_view_along_when_they_must() {
    let mut u = zoomed();
    // Search 2 has one event, in the main view: the window goes there, the main view stays
    u.studio_search_select(1);
    let at = u.studio_search_events()[0];
    u.studio_search_go(1);
    assert!((u.m.zdelay as f64 - at).abs() < 1e-7 && u.m.delay == 0.0, "zoom {} main {} for {at}", u.m.zdelay, u.m.delay);
    let cv = frame(&mut u);
    let (o, z) = u.split_zoom(u.plot.grid);
    let (xa, xb) = u.zoom_window_x(o);
    let tick = (o.x..o.right()).find(|&x| px(&cv, x, o.y + 1 + SEARCH_STRIP_H + 2) == SEARCH_COLS[1]).unwrap();
    assert!(xa < tick && tick < xb, "the window {xa}..{xb} is on the tick {tick}");
    // and the mark of the event is in the middle of the zoomed part, with the dashed line of the selected search
    let mark = (z.x..z.right()).find(|&x| (0..SEARCH_LANE_H).any(|dy| px(&cv, x, u.plot.area.y + SEARCH_LANE_H + dy) == SEARCH_COLS[1])).unwrap();
    assert!((mark - (z.x + z.w / 2)).abs() <= 6, "{mark} in {z:?}");
    // Search 1: its last event is 9.75 ms after the trigger, beyond the 5 ms the main view reaches
    u.studio_search_select(0);
    let ev = u.studio_search_events();
    u.studio_search_go(-1);
    let last = *ev.last().unwrap();
    assert_eq!(u.studio.search_at[0], ev.len() - 1);
    assert!(last > 5e-3 && (u.m.zdelay as f64 - last).abs() < 1e-7, "zoom {} for {last}", u.m.zdelay);
    assert!((u.m.delay as f64 - last).abs() < 1e-7, "the main view went along: {}", u.m.delay);
    // the next one is the first again, at the other end of the record
    u.studio_search_go(1);
    assert!((u.m.zdelay as f64 - ev[0]).abs() < 1e-7 && u.studio.search_at[0] == 0);
}
