// SPDX-License-Identifier: GPL-2.0-or-later

//! The types of a search: Edge, Pulse, Slope, Timeout and Runt, found by the software trigger (`oos_dsp::trigger`).

use super::addrail::SearchSpec;
use super::search_dense_tests::{dense, of_samples};
use super::*;

/// 20 ms at 1 us a sample on CH2: a 1 kHz square of 3 V with edges of 10 us, and in it one pulse of 50 us (5.9 ms),
/// one runt that reaches 1.2 V (8.9 ms), one rising edge of 100 us (12.25 ms) and two missing pulses (16 ms, 17 ms).
fn mixed() -> Ui {
    let trap = |t: f64, start: f64, end: f64, ramp: f64, top: f64| {
        top * (((t - start) / ramp).clamp(0.0, 1.0) - ((t - end) / 10.0).clamp(0.0, 1.0)).max(0.0)
    };
    let data: Vec<f32> = (0..20_000)
        .map(|i| {
            let t = i as f64;
            let square: f64 = (0..20)
                .filter(|k| !matches!(k, 16 | 17))
                .map(|k| {
                    trap(
                        t,
                        k as f64 * 1000.0 + 250.0,
                        k as f64 * 1000.0 + 750.0,
                        if k == 12 { 100.0 } else { 10.0 },
                        3.0,
                    )
                })
                .sum();
            (square + trap(t, 5900.0, 5950.0, 10.0, 3.0) + trap(t, 8900.0, 9000.0, 10.0, 1.2)) as f32
        })
        .collect();
    let mut u = of_samples(data, 1e-6, (0.0, 0));
    u.studio_search_add(Some(WaveId::Ch(1)));
    u
}

/// The number of events with the selected search changed by `f`.
fn count(u: &mut Ui, f: impl FnOnce(&mut SearchSpec)) -> usize {
    let mut sp = u.studio.search().unwrap();
    f(&mut sp);
    u.studio.search_put(sp);
    u.studio_search_events().len()
}

#[test]
fn every_type_finds_its_events() {
    let mut u = mixed();
    assert_eq!(u.options(Item::SrchType), vec!["Edge", "Pulse", "Slope", "Timeout", "Runt", "Window", "Bus"]);
    // 18 pulses and the narrow one, two edges each; the runt does not reach the level
    assert_eq!(u.studio_search_events().len(), 38);
    u.select_option(Item::SrchType, "Pulse");
    assert_eq!(u.studio.search().unwrap().kind, TrigType::Pulse);
    assert_eq!(count(&mut u, |s| (s.slope, s.when, s.t1) = (0, WhenKind::Lt, 100e-6)), 1, "the pulse of 50 us");
    let at = u.studio_search_events()[0];
    assert_eq!(count(&mut u, |s| s.when = WhenKind::Gt), 18);
    assert_eq!(count(&mut u, |s| (s.when, s.t1, s.t2) = (WhenKind::In, 20e-6, 100e-6)), 1);
    assert_eq!(count(&mut u, |s| s.when = WhenKind::Out), 18);
    // the mark is on the closing edge: 50 us after the edge search's event of its rising edge
    u.select_option(Item::SrchType, "Edge");
    count(&mut u, |s| s.slope = 2);
    let ev = u.studio_search_events();
    let i = ev.iter().position(|&t| (t - at).abs() < 1e-9).expect("the closing edge is an edge");
    assert!((ev[i] - ev[i - 1] - 50e-6).abs() < 2e-6, "{}", ev[i] - ev[i - 1]);
    // Slope: the levels go to 10 % and 90 % of the signal, one rising edge takes longer than 50 us
    u.select_option(Item::SrchType, "Slope");
    let sp = u.studio.search().unwrap();
    assert!((sp.level - 2.7).abs() < 0.01 && (sp.level2 - 0.3).abs() < 0.01, "{sp:?}");
    assert_eq!(count(&mut u, |s| (s.slope, s.when, s.t1) = (0, WhenKind::Gt, 50e-6)), 1, "the edge of 100 us");
    assert_eq!(count(&mut u, |s| s.when = WhenKind::Lt), 18);
    assert_eq!(count(&mut u, |s| s.slope = 1), 19, "every falling edge is fast");
    // Timeout: the two missing pulses leave 2.5 ms without an edge
    u.select_option(Item::SrchType, "Timeout");
    assert_eq!(count(&mut u, |s| (s.slope, s.t1) = (2, 1e-3)), 1);
    assert_eq!(count(&mut u, |s| s.t1 = 5e-3), 0);
    // Runt: the levels go to 25 % and 75 %, any width
    u.select_option(Item::SrchType, "Runt");
    let sp = u.studio.search().unwrap();
    assert!((sp.level - 2.25).abs() < 0.01 && (sp.level2 - 0.75).abs() < 0.01, "{sp:?}");
    assert_eq!(count(&mut u, |s| (s.slope, s.when) = (0, WhenKind::Off)), 1);
    assert_eq!(count(&mut u, |s| s.slope = 1), 0, "no negative runt");
    assert_eq!(u.studio_search_count(), "0 events");
    u.table.kind = Some(tables::TableKind::Search);
    assert_eq!(count(&mut u, |s| s.slope = 0), 1);
    assert_eq!(u.table_rows(tables::TableKind::Search)[0].text, "Runt", "the list names the type");
}

#[test]
fn the_sheet_has_the_rows_of_the_type_and_they_fit() {
    use Item::*;
    let mut u = mixed();
    let rows = |u: &Ui| u.addrail_search_items();
    assert_eq!(
        rows(&u),
        vec![
            SrchWhich,
            SrchType,
            SrchSrc,
            SrchSlope,
            SrchLevel,
            Srch50,
            SrchShow,
            SrchEvent,
            SrchFromTrig,
            SrchToTrig,
            SearchTable,
            SrchRemove
        ]
    );
    assert_eq!((u.item_label(SrchSlope), u.item_label(SrchLevel)), ("Slope", "Level"));
    u.select_option(SrchType, "Pulse");
    assert!(
        rows(&u).contains(&SrchWhen)
            && rows(&u).contains(&SrchTime)
            && !rows(&u).contains(&SrchTime2)
            && !rows(&u).contains(&SrchLevel2)
    );
    assert_eq!(
        (u.item_label(SrchSlope), u.options(SrchSlope)),
        ("Polarity", vec!["Positive".to_string(), "Negative".to_string()])
    );
    assert_eq!(u.options(SrchWhen), vec![">", "<", "Equal", "Not equal", "Inside", "Outside"]);
    u.select_option(SrchWhen, "Inside");
    assert!(rows(&u).contains(&SrchTime2));
    assert_eq!((u.item_label(SrchTime), u.item_label(SrchTime2)), ("Lower limit", "Upper limit"));
    u.open_menu(MenuId::Search);
    let mut cv = Canvas::new(W, H);
    for _ in 0..4 {
        u.step(0.05);
    }
    u.render(&mut cv);
    super::search_dense_tests::shot(&cv, "search-type-pulse");
    u.close_menu();
    // the times step on the trigger's ladder and stay in order
    let t = |u: &Ui| (u.studio.search().unwrap().t1, u.studio.search().unwrap().t2);
    assert_eq!(t(&u), (20e-6, 40e-6));
    u.adjust(SrchTime, 2);
    assert_eq!(t(&u), (100e-6, 100e-6), "the upper limit goes along");
    u.adjust(SrchTime2, 1);
    assert_eq!((t(&u), u.value(SrchTime2)), ((100e-6, 200e-6), "200 \u{b5}s".to_string()));
    u.select_option(SrchType, "Timeout");
    assert!(rows(&u).contains(&SrchTime) && !rows(&u).contains(&SrchWhen));
    assert_eq!((u.item_label(SrchTime), u.item_label(SrchSlope)), ("Timeout", "Slope"));
    u.select_option(SrchType, "Runt");
    assert_eq!(u.options(SrchWhen)[0], "Off");
    assert_eq!((u.item_label(SrchLevel), u.item_label(SrchLevel2)), ("Upper", "Lower"));
    // the lower level stops at the upper one, the upper one takes it along
    let l = |u: &Ui| (u.studio.search().unwrap().level, u.studio.search().unwrap().level2);
    u.adjust(SrchLevel2, 1000);
    assert_eq!(l(&u).0, l(&u).1);
    u.adjust(SrchLevel, -4);
    assert_eq!(l(&u).0, l(&u).1);
    // every type and every condition on the two pages of the sheet
    for kind in super::addrail::SEARCH_TYPES {
        for when in super::addrail::WHENS {
            let sp = u.studio.search().unwrap();
            u.studio.search_put(SearchSpec { kind, when, ..sp });
            let spec = u.sheet_spec(MenuId::Search).unwrap_or_else(|| panic!("{kind:?} {when:?} has no sheet"));
            assert!(spec.tabs.len() == 2 && spec.tabs.iter().all(|t| t.rows.len() <= 7), "{kind:?} {when:?}");
        }
    }
}

#[test]
fn the_trigger_gives_and_takes_every_type_of_the_search() {
    let mut u = mixed();
    u.select_option(Item::SrchType, "Runt");
    let sp = u.studio.search().unwrap();
    u.studio.search_put(SearchSpec { slope: 1, when: WhenKind::In, t1: 1e-3, t2: 2e-3, ..sp });
    u.activate(Item::SrchToTrig);
    let m = &u.m;
    assert_eq!(
        (m.trig_type, m.trig_src, m.trig_pol, m.trig_when, m.trig_t1, m.trig_t2),
        (TrigType::Runt, 1, Pol::Neg, WhenKind::In, 1e-3, 2e-3)
    );
    assert_eq!((m.trig_level, m.trig_level2), (sp.level, sp.level2));
    let mut on_first = u.m.clone();
    on_first.trig_src = 0;
    assert_eq!(on_first.trigger(), u.studio.search().unwrap().trigger(), "the same condition");
    // from the trigger: its type, or its edge where the search has no such type
    u.m.trig_type = TrigType::Pulse;
    u.m.trig_pol = Pol::Pos;
    u.m.trig_when = WhenKind::Lt;
    u.m.trig_t1 = 100e-6;
    u.m.trig_level = 1.5;
    u.activate(Item::SrchFromTrig);
    let sp = u.studio.search().unwrap();
    assert_eq!((sp.kind, sp.slope, sp.when, sp.t1, sp.level), (TrigType::Pulse, 0, WhenKind::Lt, 100e-6, 1.5));
    assert_eq!(u.studio_search_events().len(), 1);
    u.m.trig_type = TrigType::Delay;
    u.m.slope = Slope::Fall;
    u.activate(Item::SrchFromTrig);
    assert_eq!((u.studio.search().unwrap().kind, u.studio.search().unwrap().slope), (TrigType::Edge, 1));
}

#[test]
fn a_view_stores_the_type_and_its_settings() {
    let mut u = mixed();
    u.select_option(Item::SrchType, "Pulse");
    let sp = u.studio.search().unwrap();
    u.studio.search_put(SearchSpec { slope: 1, when: WhenKind::Out, t1: 30e-6, t2: 70e-6, ..sp });
    let text = u.view_ui_text();
    let word = text.split_whitespace().find(|w| w.starts_with("search=")).unwrap();
    assert!(word.contains("/Pulse/"), "{word}");
    let mut v = mixed();
    v.view_ui_apply(&text);
    assert_eq!(v.studio.searches, u.studio.searches);
    // a type this version does not know reads as the edge search the first three fields are
    v.view_ui_apply("search=2/1.5/1/Glitch/0.5/2/1e-5/2e-5");
    assert_eq!(v.studio.searches[0], Some(SearchSpec::edge(WaveId::Ch(1), 1.5, 1)));
}

/// The types on the 1 Mpts record: each is searched once, like the edges.
#[test]
fn the_types_search_a_megapoint_once() {
    let mut u = dense();
    u.studio_search_add(Some(WaveId::Ch(1)));
    // (a bus search reads the packets of a decoder, which this record has none of: `search_bus_tests`)
    for kind in super::addrail::SEARCH_TYPES.into_iter().filter(|&k| k != TrigType::Bus) {
        u.studio_search_set_kind(kind);
        let sp = u.studio.search().unwrap();
        u.studio.search_put(SearchSpec { when: WhenKind::Gt, t1: 1e-9, ..sp });
        let t = std::time::Instant::now();
        let n = u.studio_search_events().len();
        let first = t.elapsed();
        let t = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(u.studio_search_events().len(), n);
        }
        let again = t.elapsed() / 100;
        eprintln!("{kind:?}: {n} events in {first:?}, asked again in {again:?}");
        assert!(again < first / 20 || again < std::time::Duration::from_micros(50), "{kind:?}: {again:?} of {first:?}");
        // (a clean square has no runt)
        assert_eq!(
            n,
            match kind {
                TrigType::Edge | TrigType::Window => 64_000,
                TrigType::Runt => 0,
                _ => 32_000,
            },
            "{kind:?}"
        );
    }
}

#[test]
fn a_window_search_marks_where_the_signal_comes_between_the_levels_leaves_them_or_stayed() {
    let mut u = mixed();
    u.select_option(Item::SrchType, "Window");
    let sp = u.studio.search().unwrap();
    assert!((sp.level - 2.25).abs() < 0.01 && (sp.level2 - 0.75).abs() < 0.01, "25 % and 75 %: {sp:?}");
    assert_eq!(
        (u.item_label(Item::SrchSlope), u.options(Item::SrchSlope)),
        ("Position", vec!["Enter".to_string(), "Exit".to_string(), "Inside".to_string()])
    );
    // every edge of the 19 pulses comes between the levels and leaves them; the runt comes in and goes back
    assert_eq!(u.value(Item::SrchSlope), "Enter");
    assert_eq!(u.studio_search_events().len(), 39);
    assert!(!u.addrail_search_items().contains(&Item::SrchWhen), "a time only for Inside");
    u.select_option(Item::SrchSlope, "Exit");
    assert_eq!(u.studio_search_events().len(), 39);
    // between the levels for longer than 30 us: the slow edge (50 us) and the runt (its whole width)
    u.select_option(Item::SrchSlope, "Inside");
    assert!(u.addrail_search_items().contains(&Item::SrchWhen) && u.addrail_search_items().contains(&Item::SrchTime));
    assert_eq!(count(&mut u, |s| (s.when, s.t1) = (WhenKind::Gt, 30e-6)), 2);
    assert_eq!(count(&mut u, |s| s.when = WhenKind::Lt), 37);
    // the trigger takes it and gives it back
    u.activate(Item::SrchToTrig);
    assert_eq!((u.m.trig_type, u.m.win_mode, u.m.trig_when), (TrigType::Window, WinMode::Inside, WhenKind::Lt));
    u.m.win_mode = WinMode::Exit;
    u.activate(Item::SrchFromTrig);
    assert_eq!((u.studio.search().unwrap().kind, u.value(Item::SrchSlope)), (TrigType::Window, "Exit".to_string()));
    let spec = u.sheet_spec(MenuId::Search).unwrap();
    assert!(spec.tabs.iter().all(|t| t.rows.len() <= 7));
}

#[test]
fn equal_and_not_equal_are_within_a_twentieth_of_the_time_or_not() {
    let mut u = mixed();
    u.select_option(Item::SrchType, "Pulse");
    count(&mut u, |s| (s.slope, s.t1, s.t2) = (0, 50e-6, 80e-6));
    u.select_option(Item::SrchWhen, "Equal");
    let sp = u.studio.search().unwrap();
    assert_eq!((sp.cond(), sp.eq, u.value(Item::SrchWhen)), (5, true, "Equal".to_string()));
    assert_eq!(u.studio_search_events().len(), 1, "the pulse of 50 us");
    // one time, named Time: the upper limit is not asked for
    assert!(!u.addrail_search_items().contains(&Item::SrchTime2));
    assert_eq!(u.item_label(Item::SrchTime), "Time");
    assert_eq!(count(&mut u, |s| s.t1 = 53e-6), 0, "6 % off");
    assert_eq!(count(&mut u, |s| s.t1 = 52e-6), 1, "4 % off");
    // (the pulse behind the slow edge is 9 % narrower at the level)
    assert_eq!(count(&mut u, |s| s.t1 = 500e-6), 17);
    u.select_option(Item::SrchWhen, "Not equal");
    assert_eq!((u.studio.search().unwrap().cond(), u.studio_search_events().len()), (6, 2));
    // the row steps through them between the conditions of one limit and those of two
    u.adjust(Item::SrchWhen, 1);
    assert_eq!(u.value(Item::SrchWhen), "Inside");
    assert!(u.addrail_search_items().contains(&Item::SrchTime2) && !u.studio.search().unwrap().eq);
    u.adjust(Item::SrchWhen, -2);
    assert_eq!(u.value(Item::SrchWhen), "Equal");
    // the trigger has no Equal: it gets the two limits
    count(&mut u, |s| s.t1 = 100e-6);
    u.activate(Item::SrchToTrig);
    assert_eq!(u.m.trig_when, WhenKind::In);
    assert!(
        (u.m.trig_t1 - 95e-6).abs() < 1e-9 && (u.m.trig_t2 - 105e-6).abs() < 1e-9,
        "{} {}",
        u.m.trig_t1,
        u.m.trig_t2
    );
    // a view keeps it
    let text = u.view_ui_text();
    let mut v = mixed();
    v.view_ui_apply(&text);
    assert_eq!(v.studio.searches, u.studio.searches);
}

#[test]
fn a_search_whose_marks_are_hidden_keeps_its_events_its_badge_and_its_place() {
    use super::addrail::SEARCH_COLS;
    use super::search_dense_tests::px;
    let mut u = mixed();
    u.m.tscale = tscales().iter().position(|&t| (t - 1e-3).abs() < 1e-9).unwrap();
    u.m.delay = 0.0;
    let mut cv = Canvas::new(W, H);
    let amber = |u: &mut Ui, cv: &mut Canvas| {
        for _ in 0..4 {
            u.step(0.05);
        }
        u.render(cv);
        let a = u.plot.area;
        (a.x..a.right())
            .flat_map(|x| (a.y..a.bottom()).map(move |y| (x, y)))
            .filter(|&(x, y)| px(cv, x, y) == SEARCH_COLS[0])
            .count()
    };
    // (the trigger's marker is amber too: the marks are what a search adds to it)
    let shown = amber(&mut u, &mut cv);
    assert_eq!(u.options(Item::SrchShow), vec!["Shown", "Hidden"]);
    u.select_option(Item::SrchShow, "Hidden");
    assert!(u.studio.search().unwrap().hidden);
    let hidden = amber(&mut u, &mut cv);
    assert!(hidden + 50 < shown, "no mark and no line in the plot: {hidden} of {shown}");
    // the search is there: its events, its badge, previous and next
    assert_eq!(u.studio_search_events().len(), 38);
    assert_eq!(u.studio_search_badges().len(), 1);
    u.studio_search_go(1);
    assert_eq!(u.studio.search_at[0], 1);
    // a second search keeps its own line under the hidden one's
    u.studio_search_add(Some(WaveId::Ch(1)));
    assert!(!u.studio.search().unwrap().hidden && u.studio.searches[0].unwrap().hidden);
    // a view keeps it, and the copy from the trigger does not show the marks again
    let text = u.view_ui_text();
    assert!(text.split_whitespace().any(|w| w == "searchhide=1"), "{text}");
    let mut v = mixed();
    v.view_ui_apply(&text);
    assert_eq!(v.studio.searches, u.studio.searches);
    u.studio_search_select(0);
    u.activate(Item::SrchFromTrig);
    assert!(u.studio.search().unwrap().hidden);
    // (the view has moved to an event by now: the marks are counted again)
    let hidden = amber(&mut u, &mut cv);
    u.select_option(Item::SrchShow, "Shown");
    assert!(amber(&mut u, &mut cv) > hidden + 20);
}
