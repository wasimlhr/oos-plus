// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Tests of the menu dialog and the keys: the plot size and touch targets, the pickers and every option, Clear in
//! STOP, the measurement list, the quick key, the acquisition text, the channel keys, the start menu and nested menus.
//! Other tests of the screen are in `tests.rs`.

/// Centre of a rectangle (for taps).
fn mid(r: super::Rect) -> (i32, i32) {
    (r.x + r.w / 2, r.y + r.h / 2)
}
/// The row of item `it` in the open menu dialog.
fn row_of(ui: &super::Ui, it: super::Item) -> super::Row {
    ui.dlg_layout().expect("menu open").rows.into_iter().find(|r| r.it == it).expect("row")
}

/// The compact 30px footer returns space to the plot. Menus keep 40px rows.
#[test]
fn layout_plot_size_and_touch_targets() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.m.layout = crate::model::Layout::Minimal;
    ui.relayout();
    // Minimal: one measurement row under the plot
    assert_eq!((ui.plot.grid.x, ui.plot.grid.y, ui.plot.grid.w, ui.plot.grid.h), (0, 40, 1024, 496));
    ui.m.meas.clear();
    ui.relayout();
    // statistics: the results panel takes no room
    ui.m.meas = vec![(Meas::Freq, WaveId::Ch(0))];
    ui.results.on = true;
    ui.relayout();
    assert_eq!((ui.plot.grid.w, ui.plot.grid.h), (1024, 520));
    for (_, _, r) in ui.top_buttons() {
        assert!(ui.top_hit(r).h >= 40 && r.x + r.w <= ui.net_box().x, "{:?}", r);
    }
    // sticks: their boxes beside the network box, the buttons move over and stay on the bar
    (ui.stick.mounted, ui.net.wlan_stick) = (true, true);
    for (_, _, r) in ui.top_buttons() {
        assert!(r.x > ui.run_pill().x + ui.run_pill().w && r.x + r.w <= ui.status_box("USB").unwrap().x, "{:?}", r);
    }
    let (u, w) = (ui.status_box("USB").unwrap(), ui.status_box("WLAN").unwrap());
    assert!(u.x + u.w < w.x && w.x + w.w < ui.net_box().x);
    ui.tap(u.x + 5, 10);
    assert!(ui.menu_open && ui.menu_root() == MenuId::Storage);
    ui.tap(u.x + 5, 10);
    ui.tap(w.x + 5, 10);
    assert!(ui.menu_open && ui.menu_root() == MenuId::Network);
    ui.tap(w.x + 5, 10);
    (ui.stick.mounted, ui.net.wlan_stick) = (false, false);
    for n in 0..=NCH {
        for c in 0..NCH {
            ui.m.ch[c].on = c < n;
        }
        let (chs, tr, trg) = ui.cards();
        assert!(trg.x + trg.w <= W && trg.w >= 222 && tr.w >= 124, "{} on", n);
        for r in chs.iter().filter(|r| r.w > 0).chain([tr, trg, Ui::MENU_BTN].iter()) {
            assert!(r.h >= 30 && r.y + r.h <= H && r.w >= 38, "{:?}", r);
        }
    }
    // Analog off, a ref on: CH pills disappear while the reference and Time remain accessible.
    for c in 0..NCH {
        ui.m.ch[c].on = false;
    }
    ui.refs[0] = Some(RefWave {
        src: 0,
        color: 0,
        label: "DQ".into(),
        t0: 0.0,
        dt: 1e-9,
        v: vec![0.0, 1.0].into(),
        vpd: 0.2,
        offset: 0.0,
        on: true,
    });
    let (chs, tr, trg) = ui.cards();
    assert!(chs.iter().all(|r| r.w == 0), "{:?}", chs);
    assert!(tr.w >= 124 && trg.w >= 222);
    ui.tap(145, BOTTOM.y + 15);
    assert!(ui.menu_open && ui.menu == MenuId::Ref, "a stored reference has a direct source pill");
    ui.close_menu();
    assert!(!ui.m.zoom);
    ui.tap(tr.x + 10, tr.y + 10);
    assert!(ui.menu_open && ui.menu_root() == MenuId::Horizontal);
    ui.refs[0] = None;
    for c in 0..n_ch() {
        ui.m.ch[c].on = true;
    }
    for m in [
        MenuId::Main,
        MenuId::Trigger,
        MenuId::Math,
        MenuId::MeasAdd,
        MenuId::Utility,
        MenuId::Channel(0),
        MenuId::Display,
        MenuId::Acquire,
    ] {
        ui.open_menu(m);
        let d = ui.dlg_layout().unwrap();
        let fr = ui.frame();
        assert!(d.close.h >= 40 && d.r.y >= fr.top.bottom() && d.r.bottom() <= fr.bottom.y, "{:?}", m);
        assert!(d.rows.iter().all(|r| r.ctrl.h >= 40), "{:?}", m);
    }
}

/// Trigger > Type and Math > Operation: the value opens a picker, picking sets the value and returns to the menu
/// (with its Back target); Back leaves it unchanged; the knobs still step the item.
#[test]
fn pickers_open_and_set_the_value() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.m.layout = Layout::Default; // the stock dialogs: Minimal and Studio open sheets
    ui.open_menu(MenuId::Main);
    ui.activate(Item::Open(MenuId::Trigger));
    let row = row_of(&ui, Item::TrigType);
    assert!(matches!(row.ctl, Ctl::Pick));
    let (x, y) = mid(Ui::value_parts(row.ctrl).1);
    ui.tap(x, y);
    assert_eq!(ui.menu, MenuId::Pick);
    assert_eq!(ui.items(MenuId::Pick).len(), TRIG_TYPES.len());
    let k = TRIG_TYPES.iter().position(|&t| t == TrigType::Runt).unwrap();
    let (x, y) = mid(row_of(&ui, Item::PickOpt(k)).ctrl);
    ui.tap(x, y);
    assert_eq!(ui.m.trig_type, TrigType::Runt);
    assert_eq!((ui.menu, ui.parent), (MenuId::Trigger, Some(MenuId::Main)));
    assert!(ui.pick.is_none() && ui.menu_open);
    // the label opens it too; Back keeps the value
    let (x, y) = mid(row_of(&ui, Item::TrigType).label);
    ui.tap(x, y);
    assert_eq!(ui.menu, MenuId::Pick);
    let (x, y) = mid(ui.dlg_layout().unwrap().back.unwrap());
    ui.tap(x, y);
    assert_eq!((ui.menu, ui.m.trig_type), (MenuId::Trigger, TrigType::Runt));
    // Multi 2 steps the focused item, Multi 1 push opens the list
    ui.focus = 0;
    ui.now = 10.0; // a slow turn: no acceleration
    ui.key(key::MULTI2_CW);
    assert_eq!(ui.m.trig_type, TrigType::Window);
    ui.key(key::MULTI1_CW + 2);
    assert_eq!(ui.menu, MenuId::Pick);
    assert_eq!(ui.focus, TRIG_TYPES.iter().position(|&t| t == TrigType::Window).unwrap());
    ui.key(key::MULTI1_CW);
    ui.key(key::MULTI1_CW + 2);
    assert_eq!((ui.menu, ui.m.trig_type), (MenuId::Trigger, TrigType::Delay));
    // math operation
    ui.toggle_menu(MenuId::Math);
    ui.select_option(Item::MathFam, "Filter");
    let (x, y) = mid(Ui::value_parts(row_of(&ui, Item::MathOpI).ctrl).1);
    ui.tap(x, y);
    assert_eq!(ui.items(MenuId::Pick).len(), ui.options(Item::MathOpI).len());
    let fft = ui.options(Item::MathOpI).iter().position(|s| s == "FFT").unwrap();
    let (x, y) = mid(row_of(&ui, Item::PickOpt(fft)).ctrl);
    ui.tap(x, y);
    assert_eq!((ui.m.math, ui.menu), (MathOp::Fft, MenuId::Math));
    // tapping the menu's top-bar button while its picker is open closes the menu
    ui.open_picker(Item::FftWin);
    ui.toggle_menu(MenuId::Math);
    assert!(!ui.menu_open && ui.pick.is_none());
}

/// Every option of every item with a picker or segmented buttons can be selected and reads back as chosen.
#[test]
fn every_option_selects() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    let items = [
        Item::TrigType,
        Item::MathOpI,
        Item::MathFam,
        Item::FftWin,
        Item::VidSync,
        Item::AcqDepth,
        Item::Persist,
        Item::AcqAvg,
        Item::DecBaud,
        Item::QuickAct,
        Item::AcqMode,
        Item::TrigWhen,
        Item::Pat(1),
        Item::DecCs,
        Item::DecProto,
        Item::GridStyle,
        Item::ChProbe(0),
        Item::TrigMode,
        Item::StoreDest,
        Item::TrigCoup,
    ];
    for it in items {
        let opts = ui.options(it);
        assert!(!opts.is_empty(), "{:?}", it);
        for o in opts.iter().rev().chain(opts.iter()) {
            ui.select_option(it, o);
            assert_eq!(&ui.value(it), o, "{:?}", it);
        }
    }
    // long lists get a picker, short ones segmented buttons
    for it in [Item::TrigType, Item::FftWin, Item::AcqDepth, Item::Persist, Item::AcqAvg, Item::DecBaud, Item::QuickAct]
    {
        assert!(matches!(ui.ctl(it, 300), Ctl::Pick), "{:?}", it);
    }
    assert!(matches!(ui.ctl(Item::AcqMode, 300), Ctl::Seg(..)));
}

/// Clear in STOP: maps, record, measurements, statistics and mask counts are gone and stay gone; the next
/// acquisition (SINGLE) draws again.
#[test]
fn clear_empties_the_display_in_stop() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    let lit = |ui: &Ui| ui.ph.map.iter().map(|m| m.iter().filter(|&&v| v > 0).count()).sum::<usize>();
    for _ in 0..5 {
        ui.step(0.033);
    }
    ui.create_mask();
    ui.step(0.033);
    ui.key(key::RUNSTOP);
    ui.step(0.033);
    assert!(lit(&ui) > 500 && ui.mask_sw.pass > 0 && !ui.results.rows.is_empty());
    ui.key(key::CLEAR);
    let mut cv = Canvas::new(PANEL_W, PANEL_H);
    for _ in 0..5 {
        ui.step(0.033);
        ui.render(&mut cv);
    }
    assert_eq!(lit(&ui), 0);
    assert!(ui.last[0].is_none() && ui.results.rows.is_empty() && (ui.mask_sw.pass, ui.mask_sw.fail) == (0, 0));
    assert!(ui.meas(0, Meas::Freq).is_nan());
    assert_eq!(ui.toast.as_ref().unwrap().0, "Display cleared");
    assert_eq!(ui.m.run, RunState::Stop);
    // no trace pixel in the plot (below the mask pill)
    let a = ui.plot.area;
    let yellow = (a.y + 60..a.y + a.h - 20)
        .flat_map(|y| (a.x + 60..a.x + a.w - 60).map(move |x| (x, y)))
        .filter(|&(x, y)| cv.px[(y * PANEL_W + x) as usize] == CH_COLORS[0])
        .count();
    assert_eq!(yellow, 0);
    // Display > Clear does the same; SINGLE acquires and draws again
    ui.key(key::SINGLE);
    ui.step(0.033);
    assert!(lit(&ui) > 500 && ui.last[0].is_some());
    ui.activate(Item::ClearDisp);
    ui.step(0.033);
    assert_eq!(lit(&ui), 0);
}

/// The Measure key opens the measurement list and closes it again. Held on the front panel it adds the next usual
/// measurement instead, at once, and its release does nothing more. The rest of the Measure page is unchanged.
#[test]
fn measure_key_lists_and_its_long_press_adds() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.m.layout = Layout::Default; // the stock top bar with its Measure button
    let rest = vec![
        Item::MeasSrc,
        Item::MeasSrcB,
        Item::MeasCopyTo,
        Item::MeasGate,
        Item::MeasRefs,
        Item::MeasStats,
        Item::MeasLabels,
        Item::StripDock,
        Item::MeasIdle,
        Item::MeasTsetup,
        Item::MeasThold,
        Item::MeasOutside,
        Item::MeasN,
        Item::MeasEdges,
        Item::Open(MenuId::MeasPair),
        Item::Open(MenuId::Dvm),
        Item::MeasClear,
    ];
    ui.key(key::MEASURE);
    assert_eq!(ui.menu, MenuId::MeasAdd);
    let settings = ui.catalog_settings_rect();
    ui.tap(settings.x + settings.w / 2, settings.y + settings.h / 2);
    assert_eq!(ui.menu, MenuId::Measure, "settings is visible in the chooser");
    assert_eq!(ui.items(MenuId::Measure), rest);
    for it in ui.items(MenuId::Measure) {
        assert_ne!(ui.item_label(it), "Add item");
    }
    ui.close_menu();
    ui.key(key::MEASURE);
    assert!(ui.menu_open && ui.menu == MenuId::MeasAdd);
    ui.key(key::MEASURE);
    assert!(!ui.menu_open);
    // a short press on the front panel is the key
    ui.panel_key(key::MEASURE);
    ui.step(0.2);
    assert!(!ui.menu_open);
    ui.panel_key(0);
    assert!(ui.menu_open && ui.menu == MenuId::MeasAdd);
    ui.close_menu();
    // held: the next usual measurement, then nothing on the release
    ui.m.meas.retain(|e| e.0 != Meas::Period);
    let n = ui.m.meas.len();
    ui.panel_key(key::MEASURE);
    ui.step(LONG_PRESS + 0.05);
    assert_eq!(ui.m.meas.len(), n + 1);
    assert!(!ui.menu_open);
    ui.panel_key(0);
    assert!(!ui.menu_open, "the release after a long press opens nothing");
    assert_eq!(ui.m.meas.len(), n + 1);
    // again: the next one that is not on yet
    ui.panel_key(key::MEASURE);
    ui.step(LONG_PRESS + 0.05);
    ui.panel_key(0);
    assert_eq!(ui.m.meas.len(), n + 2);
}

/// QUICK key and the Quick button run the action set in System > Quick key; Default keeps the setting.
#[test]
fn quick_key_dispatch() {
    use super::*;
    let dir = crate::ui::testkit::Scratch::new("ui-quick");
    let mut ui = crate::ui::testkit::demo_ui();
    ui.files.dir = dir.clone();
    ui.step(0.033);
    // default: screenshot as before
    ui.key(key::QUICK);
    assert_eq!(ui.asks.shot.take(), Some(String::new()));
    ui.select_option(Item::QuickAct, "Save setup");
    assert_eq!(ui.m.quick, Quick::SaveSetup);
    ui.key(key::QUICK);
    assert!(dir.join("setup1.txt").exists());
    ui.m.quick = Quick::SaveCsv;
    ui.key(key::QUICK);
    assert!(dir.join("wave-001.csv").exists());
    ui.m.quick = Quick::SavePng;
    ui.key(key::QUICK);
    assert!(ui.asks.shot.take().unwrap().ends_with("shot-001.png"));
    ui.m.quick = Quick::RunStop;
    ui.key(key::QUICK);
    assert_eq!(ui.m.run, RunState::Stop);
    ui.m.quick = Quick::Clear;
    ui.key(key::QUICK);
    assert!(ui.cleared && ui.last[0].is_none());
    ui.m.quick = Quick::Single;
    ui.key(key::QUICK);
    assert_eq!((ui.m.trig_mode, ui.m.run), (TrigMode::Single, RunState::Run));
    ui.m.quick = Quick::Autoset;
    ui.key(key::QUICK);
    assert_eq!(ui.toast.as_ref().unwrap().0, "Auto set-up");
    // the start menu entry does the same and closes the menu
    ui.m.quick = Quick::RunStop;
    let (x, y) = mid(ui.menu_btn());
    ui.tap(x, y);
    let run = ui.m.run;
    let (x, y) = mid(row_of(&ui, Item::QuickRun).ctrl);
    ui.tap(x, y);
    assert_ne!(ui.m.run, run);
    assert!(!ui.menu_open);
    ui.key(key::DEFAULT);
    assert_eq!(ui.m.quick, Quick::RunStop);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The top bar's acquisition text stays clear of the buttons in every text size, with the longest values.
#[test]
fn the_acquisition_text_fits_before_the_buttons() {
    use super::*;
    for ts in TEXT_SIZES {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.text_size = ts;
        ui.relayout();
        let f = ui.fonts();
        let r = ui.run_pill();
        let room = ui.top_buttons().first().map_or(ui.top_right(), |b| b.2.x) - 12;
        for (sr, depth) in [(1.25e9, 125e6), (625e6, 62.5e6), (3.125e6, 1.25e6)] {
            let (bold, main, mode) = Ui::top_acq(f, r.x + r.w + 14, room, sr, depth, "Average 1024".into());
            let w = if bold { f.mb.width(&main) } else { f.m.width(&main) } + mode.as_ref().map_or(0, |m| f.m.width(m));
            assert!(r.x + r.w + 14 + w <= room, "{:?} {} {}", ts, main, w);
        }
    }
}

/// With several channels on, the key of a channel that is not selected only selects it; the next press opens its
/// menu, the one after that switches it off. Another channel's menu closes, other menus stay.
#[test]
fn a_channel_key_first_selects_when_several_are_on() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    // the stock cards; Minimal and Studio cards have their own taps
    ui.m.layout = Layout::Default;
    ui.relayout();
    ui.step(0.033);
    assert!(ui.m.ch[0].on && ui.m.n_on() == 1 && ui.m.active == 0);
    ui.key(key::CH1 + 1);
    assert!(ui.m.ch[1].on && ui.m.active == 1 && ui.menu_open && ui.menu == MenuId::Channel(1));
    ui.key(key::CH1);
    assert!(ui.m.ch[0].on && ui.m.active == 0 && !ui.menu_open, "select only, CH2's menu closes");
    ui.key(key::CH1);
    assert!(ui.menu_open && ui.menu == MenuId::Channel(0), "selected: the menu opens");
    ui.key(key::CH1);
    assert!(!ui.m.ch[0].on && ui.m.active == 1, "menu open: off");
    // one channel on: its key opens the menu at once, as before
    ui.key(key::CH1 + 1);
    assert!(ui.menu_open && ui.menu == MenuId::Channel(1));
    // a menu that is not a channel's stays while the key selects
    ui.key(key::CH1);
    assert!(ui.m.ch[0].on && ui.m.active == 0);
    ui.open_menu(MenuId::Display);
    ui.key(key::CH1 + 1);
    assert!(ui.m.active == 1 && ui.menu_open && ui.menu == MenuId::Display);
    // a tap on a channel's card does the same: select first, then the menu
    ui.close_menu();
    let card = |ui: &Ui, c: usize| mid(ui.cards().0[c]);
    let (x, y) = card(&ui, 0);
    ui.tap(x, y);
    assert!(ui.m.active == 0 && !ui.menu_open, "tap: select only");
    let (x, y) = card(&ui, 0);
    ui.tap(x, y);
    assert!(ui.menu_open && ui.menu == MenuId::Channel(0), "tap again: the menu");
    let (x, y) = card(&ui, 1);
    ui.tap(x, y);
    assert!(ui.m.active == 1 && !ui.menu_open, "CH1's menu closes when CH2 is selected");
}

/// A channel switched off with its menu's On, not with its key, gives the vertical knobs to a channel still on
/// (#523): CH1 on, CH2 on with its key, CH2 off in its menu, menu closed; V/div then turns CH1's scale.
#[test]
fn a_channel_switched_off_in_its_menu_gives_the_knobs_to_one_still_on() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.step(0.033);
    assert!(ui.m.ch[0].on && ui.m.n_on() == 1 && ui.m.active == 0);
    ui.key(key::CH1 + 1);
    assert!(ui.m.ch[1].on && ui.m.active == 1 && ui.menu == MenuId::Channel(1));
    ui.adjust(Item::ChOn(1), 1);
    assert!(!ui.m.ch[1].on, "the menu's On switched CH2 off");
    assert_eq!(ui.m.active, 0, "CH1, the channel still on, is selected");
    ui.close_menu();
    let (ch1, ch2) = (ui.m.ch[0].vscale, ui.m.ch[1].vscale);
    ui.knob_turn(key::VSCALE_CW, 1);
    assert_ne!(ui.m.ch[0].vscale, ch1, "V/div turns CH1");
    assert_eq!(ui.m.ch[1].vscale, ch2, "CH2, off, stays");
}

/// A setting whose options are just Off and On is one switch (#55, ADR 0075): a tap anywhere on it switches, both
/// ways; a choice of other options stays a row of buttons.
#[test]
fn an_on_off_setting_is_a_switch_a_tap_switches() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.step(0.033);
    ui.open_menu(MenuId::Channel(0));
    let row = |ui: &Ui, it: Item| {
        let d = ui.dlg_layout().expect("the channel menu is open");
        let r = d.rows.iter().find(|r| r.it == it).expect("the row is in the menu");
        (r.ctrl, matches!(r.ctl, Ctl::Switch(_)), matches!(r.ctl, Ctl::Switch(true)))
    };
    let (c, switch, on) = row(&ui, Item::ChInvert(0));
    assert!(switch && !on && !ui.m.ch[0].invert, "Invert is a switch, off");
    // the left end, where the On button of the pair was not
    ui.tap(c.x + 4, c.y + c.h / 2);
    assert!(ui.m.ch[0].invert && row(&ui, Item::ChInvert(0)).2, "a tap switched it on");
    ui.tap(c.x + c.w - 4, c.y + c.h / 2);
    assert!(!ui.m.ch[0].invert, "a tap switched it off again");
    assert!(!row(&ui, Item::ChCoupling(0)).1, "coupling (DC, AC, GND) stays a row of buttons");
}

/// A channel key held on the front panel: AC/DC, nothing else; a short press does what the key does, when it is
/// let go.
#[test]
fn a_long_press_of_a_channel_key_switches_ac_dc() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.step(0.033);
    assert!(ui.m.ch[0].on && ui.m.ch[0].coupling == Coupling::Dc && !ui.menu_open);
    // short: the menu opens when the key is let go
    ui.panel_key(key::CH1);
    assert!(!ui.menu_open, "not yet");
    ui.step(0.1);
    ui.panel_key(0);
    assert!(ui.menu_open && ui.menu == MenuId::Channel(0));
    // long: AC while still held, the menu stays, the channel on
    ui.panel_key(key::CH1);
    ui.step(LONG_PRESS);
    assert!(ui.m.ch[0].on && ui.m.ch[0].coupling == Coupling::Ac);
    ui.step(0.5);
    ui.panel_key(0);
    assert!(ui.m.ch[0].on && ui.m.ch[0].coupling == Coupling::Ac && ui.menu_open, "letting go does nothing more");
    ui.panel_key(key::CH1);
    ui.step(1.0);
    ui.panel_key(0);
    assert_eq!(ui.m.ch[0].coupling, Coupling::Dc, "and back");
    // a short press with the menu open: off
    ui.panel_key(key::CH1);
    ui.panel_key(0);
    assert!(!ui.m.ch[0].on);
    // scripts and the remote: every press at once
    ui.key(key::CH1);
    assert!(ui.m.ch[0].on);
    // Minimal, a finger held on the channel's chip: its quick actions (no silent AC/DC), and lifting it opens no menu
    // (Default switches AC/DC there, as upstream OOS: batch4_tests.rs)
    ui.close_menu();
    ui.m.layout = Layout::Minimal;
    ui.step(0.033);
    let (x, y) = mid(ui.cards().0[0]);
    ui.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    ui.step(LONG_PRESS + 0.05);
    assert_eq!(ui.m.ch[0].coupling, Coupling::Dc, "a hold never flips the coupling");
    assert!(ui.workspace.popup.is_some(), "the quick actions are open");
    ui.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
    assert!(!ui.menu_open, "no tap after a long touch");
    ui.dismiss_context();
    // a short one is a tap
    ui.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    ui.step(0.1);
    ui.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
    assert!(ui.menu_open && ui.menu == MenuId::Channel(0) && ui.m.ch[0].coupling == Coupling::Dc);
}

/// Every front-panel key function is in the start menu, and every menu is reachable by its key and by touch.
#[test]
fn start_menu_and_keys_reach_everything() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    for _ in 0..3 {
        ui.step(0.033);
    }
    // the menu keys
    for (k, m) in [
        (key::HMENU, MenuId::Acquire),
        (key::ANALYSE, MenuId::Decode),
        (key::TMENU, MenuId::Trigger),
        (key::MEASURE, MenuId::MeasAdd),
        (key::CURSOR, MenuId::Cursor),
        (key::MATH, MenuId::Math),
        (key::REF, MenuId::Ref),
    ] {
        ui.close_menu();
        ui.key(k);
        assert!(ui.menu_open && ui.menu == m, "key {:#x}", k);
    }
    assert!(ui.items(MenuId::Acquire).contains(&Item::Open(MenuId::Horizontal)));
    ui.close_menu();
    // the Menu button opens the start menu with all entries
    let (x, y) = mid(ui.menu_btn());
    ui.tap(x, y);
    assert_eq!(ui.menu, MenuId::Main);
    let all = ui.items(MenuId::Main);
    for it in [
        Item::RunStop,
        Item::SingleAct,
        Item::AutosetAct,
        Item::ClearDisp,
        Item::DefaultAct,
        Item::SlopeAct,
        Item::ForceAct,
        Item::NavHist,
        Item::QuickRun,
        Item::Shot,
        Item::LockAct,
        Item::PowerAct,
    ] {
        assert!(all.contains(&it), "{:?}", it);
    }
    for m in [
        MenuId::Acquire,
        MenuId::Trigger,
        MenuId::Horizontal,
        MenuId::Measure,
        MenuId::Cursor,
        MenuId::Math,
        MenuId::Ref,
        MenuId::Decode,
        MenuId::Display,
        MenuId::Storage,
        MenuId::Mask,
        MenuId::Utility,
    ] {
        assert!(all.contains(&Item::Open(m)), "{:?}", m);
        ui.open_menu(MenuId::Main);
        let (x, y) = mid(row_of(&ui, Item::Open(m)).ctrl);
        ui.tap(x, y);
        // Measure is the measurement page, as its key and the top bar open it (with Back to the start menu)
        let page = if m == MenuId::Measure { MenuId::MeasAdd } else { m };
        assert_eq!((ui.menu, ui.parent), (page, Some(MenuId::Main)));
        if m == MenuId::Measure {
            let (x, y) = mid(ui.dlg_layout().unwrap().back.expect("Back to the start menu"));
            ui.tap(x, y);
            assert_eq!(ui.menu, MenuId::Main);
        }
    }
    assert_eq!(ui.m.cursor, CursorMode::Time, "Cursor entry turns the cursors on like the key");
    // slope steps and keeps the menu, history entries appear with the history
    ui.open_menu(MenuId::Main);
    let s = ui.m.slope;
    let (x, y) = mid(row_of(&ui, Item::SlopeAct).ctrl);
    ui.tap(x, y);
    assert!(ui.m.slope != s && ui.menu == MenuId::Main);
    assert!(!ui.items(MenuId::Main).contains(&Item::NavBack));
    let (x, y) = mid(row_of(&ui, Item::NavHist).ctrl);
    ui.tap(x, y);
    assert!(ui.history.pos.is_some() && ui.items(MenuId::Main).contains(&Item::NavBack));
    let i = ui.history.pos.unwrap();
    let (x, y) = mid(row_of(&ui, Item::NavBack).ctrl);
    ui.tap(x, y);
    assert_eq!(ui.history.pos, Some(i - 1));
    // the panel keys as the unit sends them: `<` (0x06) steps back to older records, `>` (0x08) forward
    ui.panel_key(0x06);
    assert_eq!(ui.history.pos, Some(i - 2));
    ui.panel_key(0x08);
    assert_eq!(ui.history.pos, Some(i - 1));
    // power: the dialog, without the MCU frame of a key press
    let (x, y) = mid(row_of(&ui, Item::PowerAct).ctrl);
    ui.tap(x, y);
    assert!(ui.power.open && ui.asks.panel.is_empty() && !ui.menu_open);
    ui.tap(5, 300);
    // touch lock from the start menu: touches do nothing, except the Menu button, which unlocks
    ui.open_menu(MenuId::Main);
    let (x, y) = mid(row_of(&ui, Item::LockAct).ctrl);
    ui.tap(x, y);
    assert!(ui.m.touch_lock && !ui.menu_open);
    let touch = |ui: &mut Ui, x: i32, y: i32| {
        ui.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
        ui.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
    };
    let (tr, _) = (ui.cards().1, ());
    touch(&mut ui, tr.x + 10, tr.y + 10);
    assert!(!ui.menu_open && ui.m.touch_lock);
    let (x, y) = mid(ui.menu_btn());
    touch(&mut ui, x, y);
    assert!(!ui.m.touch_lock && !ui.menu_open);
    touch(&mut ui, x, y);
    assert_eq!(ui.menu, MenuId::Main);
}

#[test]
fn nested_menu_back_restores_each_parent_and_its_focus_after_a_picker() {
    use super::*;
    let mut ui = crate::ui::testkit::demo_ui();
    ui.m.layout = Layout::Default; // the stock dialogs: Minimal and Studio open sheets
    let back = |ui: &mut Ui| {
        let (x, y) = mid(ui.dlg_layout().unwrap().back.expect("nested menu has Back"));
        ui.tap(x, y);
    };
    ui.open_menu(MenuId::MeasAdd);
    ui.focus = 2;
    ui.activate(Item::Open(MenuId::Measure));
    ui.focus = 5;
    ui.open_picker(Item::MeasSrc);
    back(&mut ui);
    assert_eq!((ui.menu, ui.focus), (MenuId::Measure, 5));
    ui.focus = 7;
    ui.activate(Item::Open(MenuId::MeasPair));
    back(&mut ui);
    assert_eq!((ui.menu, ui.focus), (MenuId::Measure, 7));
    back(&mut ui);
    assert_eq!((ui.menu, ui.focus), (MenuId::MeasAdd, 2));
    assert!(ui.parent.is_none() && ui.menu_history.is_empty());
    ui.activate(Item::Open(MenuId::Measure));
    ui.close_menu();
    ui.open_menu(MenuId::Trigger);
    assert!(ui.parent.is_none() && ui.menu_history.is_empty());
    assert!(ui.dlg_layout().unwrap().back.is_none(), "a direct toolbar action starts a fresh route");
}

/// + on a cursor raises its value: a time cursor moves right, a voltage cursor up (its place counts from the top).
#[test]
fn plus_raises_every_cursor() {
    use super::{CursorMode, Item};
    for (mode, items) in [
        (CursorMode::Time, [Item::CurA, Item::CurB]),
        (CursorMode::Volt, [Item::CurA, Item::CurB]),
        (CursorMode::Xy, [Item::CurC, Item::CurD]),
    ] {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.m.cursor = mode;
        (ui.m.cur_a, ui.m.cur_b, ui.m.cur_c, ui.m.cur_d) = (0.3, 0.7, 0.3, 0.7);
        for it in items {
            let value = |ui: &super::Ui| {
                let place = match it {
                    Item::CurA => ui.m.cur_a,
                    Item::CurB => ui.m.cur_b,
                    Item::CurC => ui.m.cur_c,
                    _ => ui.m.cur_d,
                };
                if mode == CursorMode::Time {
                    ui.cursor_time_at(place)
                } else {
                    ui.cursor_volt_at(place)
                }
            };
            let before = value(&ui);
            ui.adjust(it, 1);
            let after = value(&ui);
            assert!(after > before, "{mode:?} {it:?}: + took {before} to {after}");
            ui.adjust(it, -1);
            assert!((value(&ui) - before).abs() < 1e-9, "{mode:?} {it:?}: - undoes +");
        }
    }
}
