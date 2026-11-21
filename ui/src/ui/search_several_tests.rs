// SPDX-License-Identifier: GPL-2.0-or-later

//! Several searches at once in Studio: Search 1 to 3, each with its number, its colour and its line of marks.

use super::addrail::{SearchSpec, SEARCH_COLS, SEARCH_SLOTS};
use super::render_marks::SEARCH_LANE_H;
use super::search_dense_tests::{dense, px, shot};
use super::*;

/// The 1 Mpts record of a 40 MHz square with three searches on CH2: every edge, the falling and the rising ones.
fn three() -> Ui {
    let mut u = dense();
    for slope in [2, 1, 0] {
        u.studio_search_add(Some(WaveId::Ch(1)));
        let sp = u.studio.search().unwrap();
        u.studio.search_put(SearchSpec { slope, ..sp });
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
fn three_searches_keep_their_numbers_and_a_fourth_is_refused() {
    let mut u = three();
    assert_eq!(u.studio.search_list().map(|(i, s)| (i, s.slope)).collect::<Vec<_>>(), vec![(0, 2), (1, 1), (2, 0)]);
    assert_eq!(u.studio.search_sel, 2, "the one added last is the selected one");
    let counts: Vec<usize> = (0..SEARCH_SLOTS).map(|i| u.studio_search_events_of(i).len()).collect();
    assert_eq!(counts, vec![64_000, 32_000, 32_000]);
    u.studio_search_add(Some(WaveId::Ch(1)));
    assert_eq!(u.toast.as_ref().map(|t| t.0.as_str()), Some("Three searches are the most"));
    assert_eq!(u.studio.search_list().count(), 3);
    // Search 2 goes: 1 and 3 stay what they were, and the place of 2 is the next one taken
    u.studio_search_select(1);
    u.studio_search_remove();
    assert_eq!(u.studio.search_list().map(|(i, s)| (i, s.slope)).collect::<Vec<_>>(), vec![(0, 2), (2, 0)]);
    assert_eq!(u.studio.search_sel, 0);
    u.studio_search_add(Some(WaveId::Ch(1)));
    assert_eq!(u.studio.search_sel, 1);
    // the last one removed closes what was open for the search
    u.open_menu(MenuId::Search);
    for _ in 0..3 { assert!(u.menu_open); u.studio_search_remove(); }
    assert!(u.studio.search().is_none() && !u.menu_open);
}

#[test]
fn each_search_has_its_line_of_marks_in_its_colour() {
    let mut u = three();
    let cv = frame(&mut u);
    shot(&cv, "three-searches-dense");
    let a = u.area;
    for slot in 0..SEARCH_SLOTS {
        let y = a.y + slot as i32 * SEARCH_LANE_H + 2;
        let n = (a.x..a.right()).filter(|&x| px(&cv, x, y) == SEARCH_COLS[slot]).count();
        // (the lower lines pass under the note and the buttons at the top of the plot)
        assert!(n >= a.w as usize * 6 / 10, "search {}: {n} of {} columns", slot + 1, a.w);
    }
    // marks that stand alone: 50 ns on the screen, a rising edge every 25 ns
    u.m.tscale = tscales().iter().position(|&t| (t - 5e-9).abs() < 1e-15).unwrap();
    u.m.delay = 0.0;
    u.studio_search_select(1);
    let cv = frame(&mut u);
    shot(&cv, "three-searches-sparse");
    let marks = |slot: usize| (a.x..a.right()).filter(|&x| px(&cv, x, a.y + slot as i32 * SEARCH_LANE_H + 1) == SEARCH_COLS[slot]).count();
    assert!(marks(0) > marks(1) && marks(1) > 0 && marks(2) > 0, "{} {} {}", marks(0), marks(1), marks(2));
    // only the selected search draws lines down the plot: none of the others' colours under the marks
    let under = |col: Rgb| {
        let dim = mix(BG, col, 90);
        (a.y + 40..a.bottom()).flat_map(|y| (a.x..a.right()).map(move |x| (x, y))).filter(|&(x, y)| px(&cv, x, y) == dim).count()
    };
    assert!(under(SEARCH_COLS[1]) > 50, "the lines of Search 2");
    assert_eq!((under(SEARCH_COLS[0]), under(SEARCH_COLS[2])), (0, 0));
}

#[test]
fn the_count_previous_next_and_the_list_are_of_the_selected_search() {
    let mut u = three();
    u.studio_search_select(0);
    assert!(u.studio_search_chip().unwrap().3.ends_with("64000 events"));
    u.studio_search_go(1);
    let first = u.m.delay;
    // the row of the sheet goes to the next search that is there
    u.open_menu(MenuId::Search);
    assert_eq!(u.options(Item::SrchWhich), vec!["1", "2", "3"]);
    u.adjust(Item::SrchWhich, 1);
    assert_eq!((u.studio.search_sel, u.value(Item::SrchWhich)), (1, "2".to_string()));
    assert_eq!(u.value(Item::SrchSlope), "Falling");
    let chip = u.studio_search_chip().unwrap().3;
    assert!(chip.ends_with("32000 events"), "{chip}");
    u.studio_search_go(1);
    u.studio_search_go(1);
    assert_eq!(u.studio.search_at, [1, 2, 0], "each search keeps the event it stands on");
    let falling = u.studio_search_events_of(1);
    assert!((u.m.delay as f64 - falling[2]).abs() < 1e-9 && u.m.delay != first, "the view is on an event of Search 2");
    u.close_menu();
    u.table = Some(tables::TableKind::Search);
    assert_eq!(u.table_title(), "Search 2 \u{b7} CH2 \u{b7} 32000");
    assert_eq!(u.table_bar().unwrap().0.total, 32_000);
    u.studio_search_select(0);
    assert_eq!(u.table_title(), "Search 1 \u{b7} CH2 \u{b7} 64000");
}

#[test]
fn a_view_stores_every_search_and_search_1_under_the_key_it_had() {
    let mut u = three();
    u.studio_search_select(1);
    let text = u.view_ui_text();
    let words: Vec<&str> = text.split_whitespace().filter(|w| w.starts_with("search")).collect();
    assert_eq!(words.len(), 4, "{words:?}");
    assert!(words[0].starts_with("search=2/") && words[0].ends_with("/2"), "{words:?}");
    assert!(words[1].starts_with("search2=") && words[2].starts_with("search3=") && words[3] == "searchsel=2", "{words:?}");
    let mut v = dense();
    v.view_ui_apply(&text);
    assert_eq!(v.studio.searches, u.studio.searches);
    assert_eq!(v.studio.search_sel, 1);
    // a view stored while there was one search
    v.view_ui_apply("search=2/1.5/1");
    assert_eq!(v.studio.searches, [Some(SearchSpec { src: WaveId::Ch(1), level: 1.5, slope: 1 }), None, None]);
    // the selected search of a stored view that is not there any more
    v.view_ui_apply("searchsel=3 search2=2/1.5/0");
    assert_eq!(v.studio.search_sel, 1);
}

#[test]
fn a_search_of_a_channel_that_is_off_counts_no_events_and_names_no_part() {
    // a stopped part of a record, and Search 2 on CH1, which is off and has no samples
    let mut u = super::search_dense_tests::dense_of(375_000, (1.25e9, 1_000_000));
    u.studio_search_add(Some(WaveId::Ch(1)));
    u.studio_search_add(Some(WaveId::Ch(0)));
    assert_eq!(u.studio.search_sel, 1);
    let chip = u.studio_search_chip().unwrap().3;
    assert!(chip.ends_with("0 events"), "{chip}");
    u.table = Some(tables::TableKind::Search);
    assert_eq!(u.table_title(), "Search 2 \u{b7} CH1 \u{b7} 0");
}

/// Three searches of 1 Mpts are searched once for a record; a frame takes their events from the cache.
#[test]
fn a_frame_searches_nothing() {
    let mut u = three();
    u.studio.search_cache.borrow_mut().iter_mut().for_each(|c| *c = None);
    let t0 = std::time::Instant::now();
    let first: Vec<_> = (0..SEARCH_SLOTS).map(|i| u.studio_search_events_of(i)).collect();
    let searching = t0.elapsed();
    let mut cv = frame(&mut u);
    let t0 = std::time::Instant::now();
    for _ in 0..20 { u.render(&mut cv); }
    let per_frame = t0.elapsed() / 20;
    let t0 = std::time::Instant::now();
    for _ in 0..20 { u.draw_search(&mut cv); }
    let marks = t0.elapsed() / 20;
    println!("three searches of 1 Mpts: searched in {searching:?}; a frame {per_frame:?}, of which the marks {marks:?}");
    for (i, ev) in first.iter().enumerate() {
        assert!(std::sync::Arc::ptr_eq(ev, &u.studio_search_events_of(i)), "search {} was searched again", i + 1);
    }
}
