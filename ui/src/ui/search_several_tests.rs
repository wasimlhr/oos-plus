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
    assert!(u.studio_search_chip().unwrap().3.contains("\u{b7} 64000"));
    u.studio_search_go(1);
    let first = u.m.delay;
    // the row of the sheet goes to the next search that is there
    u.open_menu(MenuId::Search);
    assert_eq!(u.options(Item::SrchWhich), vec!["1", "2", "3"]);
    u.adjust(Item::SrchWhich, 1);
    assert_eq!((u.studio.search_sel, u.value(Item::SrchWhich)), (1, "2".to_string()));
    assert_eq!(u.value(Item::SrchSlope), "Falling");
    let chip = u.studio_search_chip().unwrap().3;
    assert!(chip.contains("\u{b7} 32000"), "{chip}");
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
    assert_eq!(v.studio.searches, [Some(SearchSpec::edge(WaveId::Ch(1), 1.5, 1)), None, None]);
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
    assert!(chip.contains("\u{b7} 0"), "{chip}");
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

#[test]
fn every_search_has_its_badge_in_the_top_bar_and_a_tap_selects_it_and_opens_its_sheet() {
    let mut u = three();
    u.studio_search_select(1);
    u.studio_search_set_kind(TrigType::Pulse);
    let cv = frame(&mut u);
    shot(&cv, "search-badges");
    let badges = u.studio_search_badges();
    let (prev, label, next, text) = u.studio_search_chip().unwrap();
    assert_eq!(badges.iter().map(|b| b.0).collect::<Vec<_>>(), vec![0, 1, 2]);
    // the selected one names its type, the others their number and count; each is a finger wide and they are apart
    assert_eq!(badges[0].2, "1 \u{b7} 64000");
    assert!(badges[1].2.starts_with("2 Pulse \u{b7} 3"), "{}", badges[1].2);
    assert_eq!((badges[1].1, badges[1].2.clone()), (label, text));
    assert_eq!(badges[2].2, "3 \u{b7} 32000");
    assert_eq!(badges[0].1.w + badges[1].1.w + badges[2].1.w + 4, next.x - prev.right() - 4, "they fill the room between the arrows");
    let f = u.fonts();
    for (i, (slot, r, text)) in badges.iter().enumerate() {
        assert!(r.w >= 40 && r.h >= 40 && f.mb.width(text) + 8 <= r.w, "badge {}: {r:?} for {text:?}", slot + 1);
        assert!(r.x >= if i == 0 { prev.right() } else { badges[i - 1].1.right() } && r.right() <= next.x, "badge {}: {r:?}", slot + 1);
        // in its colour: the border, and the text
        let n = (r.x..r.right()).flat_map(|x| (r.y..r.bottom()).map(move |y| (x, y))).filter(|&(x, y)| px(&cv, x, y) == SEARCH_COLS[*slot]).count();
        assert!(n > 20, "badge {}: {n} pixels of its colour", slot + 1);
    }
    // the read-out of the acquisition keeps its room beside them
    assert!(u.studio_top().acq.right() <= prev.x && u.studio_top().acq.w >= 40);
    // a tap on another badge selects its search and opens its sheet
    let mid = |r: Rect| (r.x + r.w / 2, r.y + r.h / 2);
    let (x, y) = mid(badges[2].1);
    u.tap(x, y);
    assert_eq!(u.studio.search_sel, 2);
    assert!(u.menu_open && u.menu == MenuId::Search);
    assert_eq!((u.value(Item::SrchWhich), u.value(Item::SrchType)), ("3".to_string(), "Edge".to_string()));
    // with the sheet open another badge changes the search the sheet is of
    let (x, y) = mid(u.studio_search_badges()[1].1);
    u.tap(x, y);
    assert!(u.studio.search_sel == 1 && u.menu_open && u.menu == MenuId::Search);
    assert_eq!(u.value(Item::SrchType), "Pulse");
    // the badge of the selected search closes it again, and opens it
    let (x, y) = mid(u.studio_search_badges()[1].1);
    u.tap(x, y);
    assert!(u.studio.search_sel == 1 && !u.menu_open);
    u.tap(x, y);
    assert!(u.menu_open && u.menu == MenuId::Search);
    u.close_menu();
    // previous and next walk the selected search
    let (x, y) = mid(u.studio_search_chip().unwrap().2);
    u.tap(x, y);
    assert_eq!(u.studio.search_at, [0, 1, 0]);
    // one search: one badge between the arrows, as there was one label
    u.studio_search_remove();
    u.studio_search_remove();
    let badges = u.studio_search_badges();
    assert_eq!(badges.len(), 1);
    assert!(badges[0].2.starts_with(&format!("{} Edge \u{b7} ", badges[0].0 + 1)), "{}", badges[0].2);
}

#[test]
fn a_badge_of_a_part_of_the_record_keeps_saying_so_beside_two_others() {
    let mut u = super::search_dense_tests::dense_of(375_000, (1.25e9, 1_000_000));
    for _ in 0..3 { u.studio_search_add(Some(WaveId::Ch(1))); }
    let (_, label, _, text) = u.studio_search_chip().unwrap();
    assert!(text.ends_with("24000 in 300 \u{b5}s"), "{text}");
    assert!(u.fonts().mb.width(&text) + 8 <= label.w, "{text:?} in {label:?}");
}
