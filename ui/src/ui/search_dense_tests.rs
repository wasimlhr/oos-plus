// SPDX-License-Identifier: GPL-2.0-or-later

//! Search on a record with tens of thousands of events (a 40 MHz square in 1 Mpts at 20 us/div, unit 2026-11-20).

use super::*;
use std::sync::Arc;

/// Studio, stopped, CH2 alone: 1 Mpts at 1.25 GSa/s of a 40 MHz square between 0.56 V and 2.52 V, 20 us/div.
pub(super) fn dense() -> Ui {
    dense_of(1_000_000, (0.0, 0))
}

/// The same with `points` in the frame, of an acquisition `acq` (sample rate, points of the record).
pub(super) fn dense_of(points: usize, acq: (f64, usize)) -> Ui {
    let dt = 0.8e-9;
    let data: Vec<f32> = (0..points)
        .map(|i| {
            let phase = (i as f64 * dt * 40e6).fract();
            // edges of about 1.7 ns
            let up = (phase / 0.07).min(1.0);
            let down = ((phase - 0.5) / 0.07).clamp(0.0, 1.0);
            (0.56 + 1.96 * (up - down)) as f32
        })
        .collect();
    of_samples(data, dt, acq)
}

/// Studio, stopped, CH2 alone with `data` at the sample period `dt`.
pub(super) fn of_samples(data: Vec<f32>, dt: f64, acq: (f64, usize)) -> Ui {
    let points = data.len();
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.studio_detail = false;
    u.m.studio_zoom_tab = false;
    u.studio.enter = true;
    u.m.ch[0].on = false;
    u.m.ch[1].on = true;
    u.m.ch[1].vscale = VSCALES.iter().position(|&v| (v - 0.5).abs() < 1e-6).unwrap();
    u.m.trig_src = 1;
    u.m.trig_level = 1.6;
    u.m.tscale = tscales().iter().position(|&t| (t - 20e-6).abs() < 1e-12).unwrap();
    u.m.delay = -250e-6;
    u.m.run = RunState::Stop;
    let frame = Arc::new(crate::board::Frame {
        seq: 3,
        dt,
        pre: points / 2,
        ch: [Vec::new(), data, Vec::new(), Vec::new()],
        roll: false,
        scan: false,
        partial: false,
        envelope: false,
        acq,
        stamp: 0,
        basic: Default::default(),
        la: None,
        fixed_ti: None,
        hw_avg: false,
    });
    u.last[0] = Some(crate::wave::to_record(&frame, &u.m, 3600, 20e-6, -250e-6));
    // no measurement whose answer comes from its worker between two pictures
    u.m.meas.clear();
    u.m.meas2.clear();
    u.m.cat.clear();
    for _ in 0..4 {
        u.step(0.05);
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while u.measurement_busy() {
        assert!(std::time::Instant::now() < deadline, "the measurement worker did not answer");
        std::thread::sleep(std::time::Duration::from_millis(2));
        u.step(0.05);
    }
    u
}

pub(super) fn px(cv: &Canvas, x: i32, y: i32) -> u32 {
    cv.px[(y * W + x) as usize]
}

pub(super) fn shot(cv: &Canvas, name: &str) {
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("{name}.png")), crate::store::png_of(cv)).unwrap();
    }
}

/// The plot before the search and with it.
fn with_search(u: &mut Ui) -> (Vec<u32>, Canvas) {
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    let before = cv.px.clone();
    u.studio_search_add(Some(WaveId::Ch(1)));
    for _ in 0..4 {
        u.step(0.05);
    }
    u.render(&mut cv);
    (before, cv)
}

#[test]
fn events_too_close_to_tell_apart_are_a_strip_at_the_top_and_leave_the_plot_alone() {
    let mut u = dense();
    let (before, cv) = with_search(&mut u);
    assert_eq!(u.studio_search_events().len(), 64_000, "every edge of the record");
    shot(&cv, "dense-search");
    let a = u.plot.area;
    // (in the colour of Search 1, which is no channel's: two searches of one channel are told apart)
    let col = super::addrail::SEARCH_COLS[0];
    // the strip: every column of the plot holds events
    let strip = (a.x..a.right()).filter(|&x| px(&cv, x, a.y + 2) == col).count();
    // (but the ones under the buttons at the top right)
    assert!(strip >= a.w as usize * 9 / 10, "{strip} of {} columns", a.w);
    // under it nothing is drawn over the trace and the grid: a dashed line in every column made bands
    let under: Vec<(i32, i32)> = (a.y + super::render_marks::SEARCH_STRIP_H + 1..a.bottom())
        .flat_map(|y| (a.x..a.right()).map(move |x| (x, y)))
        .filter(|&(x, y)| px(&cv, x, y) != before[(y * W + x) as usize])
        .collect();
    assert!(
        under.is_empty(),
        "{} pixels of the plot changed by the marks, from {:?} to {:?}",
        under.len(),
        under.first(),
        under.last()
    );
}

#[test]
fn events_far_enough_apart_keep_their_mark_and_line() {
    let mut u = dense();
    u.m.tscale = tscales().iter().position(|&t| (t - 5e-9).abs() < 1e-15).unwrap();
    u.m.delay = 0.0;
    for _ in 0..4 {
        u.step(0.05);
    }
    let (before, cv) = with_search(&mut u);
    shot(&cv, "sparse-search");
    let a = u.plot.area;
    // 50 ns on the screen: an edge every 12.5 ns, a quarter of the width apart
    let lines: Vec<i32> = (a.x..a.right())
        .filter(|&x| (a.y + 20..a.bottom()).filter(|&y| px(&cv, x, y) != before[(y * W + x) as usize]).count() > 20)
        .collect();
    assert!((3..=5).contains(&lines.len()), "{lines:?}");
    assert!(lines.windows(2).all(|w| w[1] - w[0] > 200), "{lines:?}");
}

#[test]
fn the_search_sheet_opens_the_list_of_events_and_the_list_names_the_search() {
    let mut u = dense();
    u.studio_search_add(Some(WaveId::Ch(1)));
    u.open_menu(MenuId::Search);
    let spec = u.sheet_spec(MenuId::Search).unwrap();
    assert_eq!(spec.tabs[0].foot, vec![Item::SearchTable, Item::SrchRemove]);
    assert!(spec.tabs[0].rows.len() <= 7);
    u.activate(Item::SearchTable);
    assert_eq!(u.table.kind, Some(tables::TableKind::Search));
    assert!(!u.menu_open, "the sheet makes room for the list");
    for _ in 0..4 {
        u.step(0.05);
    }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    shot(&cv, "dense-search-list");
    assert_eq!(u.table_title(), "Search 1 \u{b7} CH2 \u{b7} 64000");
    // edges 12.5 ns apart, 400 us before the trigger: the times of two rows differ
    let (_, rows) = u.table_shown().unwrap();
    let digits = tables::table_time_digits(&rows);
    let times: Vec<String> = rows.iter().map(|e| si_digits(e.t, "s", digits)).collect();
    assert!(times.len() >= 4 && times.windows(2).all(|w| w[0] != w[1]), "{times:?}");
    // (the source is in the title; the column has the number of the event)
    assert_eq!(rows[0].who, "1");
    let (bar, _) = u.table_bar().unwrap();
    assert_eq!(bar.total, 64_000);
    // the trigger is on another channel: the list still names the search's source
    u.m.trig_src = 0;
    assert_eq!(u.table_title(), "Search 1 \u{b7} CH2 \u{b7} 64000");
    // the last rows are reached
    u.table.top = 63_990;
    u.render(&mut cv);
    u.activate(Item::SearchTable);
    assert_eq!(u.table.kind, None, "the same button closes it");
}

#[test]
fn the_count_says_when_the_search_is_of_a_part_of_the_record_or_ended_at_its_limit() {
    let text = |u: &Ui| u.studio_search_chip().unwrap().3;
    // the whole record of the acquisition: the count alone
    let mut u = dense_of(1_000_000, (1.25e9, 1_000_000));
    u.studio_search_add(Some(WaveId::Ch(1)));
    assert_eq!(u.studio_search_part(), None);
    assert!(text(&u).contains("\u{b7} 64000"), "{}", text(&u));
    // stopped, the scope read 300 us of the 800 us again: the count names the part
    let mut u = dense_of(375_000, (1.25e9, 1_000_000));
    u.studio_search_add(Some(WaveId::Ch(1)));
    assert!(text(&u).ends_with("24000 in 300 \u{b5}s"), "{}", text(&u));
    u.table.kind = Some(tables::TableKind::Search);
    assert_eq!(u.table_title(), "Search 1 \u{b7} CH2 \u{b7} 24000 in 300 \u{b5}s of 800 \u{b5}s");
    // more events than the search keeps
    let mut u = dense_of(2_000_000, (0.0, 0));
    u.studio_search_add(Some(WaveId::Ch(1)));
    assert_eq!(u.studio_search_events().len(), super::addrail::SEARCH_MAX);
    assert!(text(&u).contains("\u{b7} 100000+"), "{}", text(&u));
    u.table.kind = Some(tables::TableKind::Search);
    assert_eq!(u.table_title(), "Search 1 \u{b7} CH2 \u{b7} 100000+");
}
