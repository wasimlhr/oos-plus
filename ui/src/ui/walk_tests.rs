// SPDX-License-Identifier: GPL-2.0-or-later
//! The functional walk: every control of the bars and of every sheet is pressed through the touch path, and what
//! shows no response is listed. `OOS_WALK=1 cargo test walk -- --ignored --nocapture` prints the list.
#![cfg(test)]
use super::sheet::RowKind;
use super::*;
use crate::gfx::Canvas;

fn fresh(layout: Layout) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    u.m.run = RunState::Stop;
    for _ in 0..4 { u.step(0.04); }
    u
}

/// What the screen and the menu say: the picture, the menu and the values of its items.
fn sig(u: &mut Ui) -> (u64, String) {
    use std::hash::{Hash, Hasher};
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    cv.px.hash(&mut h);
    let vals: Vec<String> = if u.menu_open { u.items(u.menu).into_iter().map(|it| u.value(it)).collect() } else { Vec::new() };
    (h.finish(), format!("{:?} {} {:?} {:?} {}", u.menu, u.menu_open, vals, u.toast.as_ref().map(|t| t.0.clone()), u.numpad.is_some()))
}

fn tap(u: &mut Ui, x: i32, y: i32) {
    crate::cmd::run(u, &format!("tap {x} {y}"));
    u.step(0.04);
}

fn mid(r: Rect) -> (i32, i32) { (r.x + r.w / 2, r.y + r.h / 2) }

/// The points a row is pressed at: each segment, minus / value / plus, or its middle.
fn points(u: &Ui, row: &super::sheet::SheetRowGeo) -> Vec<(i32, i32)> {
    match row.kind {
        RowKind::Choice => {
            let n = u.sheet_choice(row.it).0.len().max(1) as i32;
            (0..n).map(|k| (row.ctrl.x + row.ctrl.w * (2 * k + 1) / (2 * n), row.ctrl.y + row.ctrl.h / 2)).collect()
        }
        RowKind::Number => { let (a, b, c) = Ui::value_parts(row.ctrl); vec![mid(a), mid(b), mid(c)] }
        RowKind::Info => Vec::new(),
        _ => vec![mid(row.ctrl)],
    }
}

fn menus(u: &Ui) -> Vec<MenuId> {
    let mut v = vec![MenuId::Display, MenuId::Utility, MenuId::Storage, MenuId::Measure, MenuId::Cursor, MenuId::Horizontal, MenuId::Trigger, MenuId::Acquire, MenuId::Channel(0), MenuId::Channel(1)];
    for (_, items) in u.main_groups() {
        for it in items { if let Item::Open(id) = it { if !v.contains(&id) { v.push(id); } } }
    }
    v
}

#[test]
#[ignore]
fn walk_every_sheet_row() {
    if std::env::var("OOS_WALK").is_err() { return; }
    for layout in [Layout::Minimal, Layout::Studio] {
        let (mut rows, mut dead, mut classic) = (0, Vec::new(), Vec::new());
        let list = menus(&fresh(layout));
        // sub menus are found while walking: a Link that opens one adds it
        let mut queue: Vec<MenuId> = list.clone();
        let mut seen: Vec<MenuId> = Vec::new();
        while let Some(menu) = queue.pop() {
            if seen.contains(&menu) { continue; }
            seen.push(menu);
            let mut u = fresh(layout);
            u.open_menu(menu);
            u.step(0.04);
            if !u.sheet_active() { classic.push(format!("{menu:?}")); continue; }
            let Some(spec) = u.sheet_spec(menu) else { classic.push(format!("{menu:?} (special sheet)")); continue };
            for t in 0..spec.tabs.len() {
                let count = { let mut u = fresh(layout); u.open_menu(menu); u.step(0.04); if let Some(g) = u.sheet_geo() { if let Some(r) = g.tabs.get(t) { let (x, y) = mid(*r); tap(&mut u, x, y); } } u.sheet_geo().map_or(0, |g| g.rows.len() + g.foot.len()) };
                for i in 0..count {
                    let mut any = false;
                    let mut what = String::new();
                    let mut n_points = 1;
                    let mut p = 0;
                    while p < n_points {
                        let mut u = fresh(layout);
                        u.open_menu(menu);
                        u.step(0.04);
                        let g = u.sheet_geo().unwrap();
                        if let Some(r) = g.tabs.get(t) { let (x, y) = mid(*r); tap(&mut u, x, y); }
                        let g = u.sheet_geo().unwrap();
                        let pts = if i < g.rows.len() { what = format!("{:?} {:?}", g.rows[i].it, g.rows[i].kind); points(&u, &g.rows[i]) } else { let f = &g.foot[i - g.rows.len()]; what = format!("{:?} Foot", f.0); vec![mid(f.1)] };
                        n_points = pts.len();
                        if pts.is_empty() { any = true; break; }
                        let before = sig(&mut u);
                        tap(&mut u, pts[p].0, pts[p].1);
                        let after = sig(&mut u);
                        if before != after { any = true; }
                        if u.menu_open && u.menu != menu && u.menu != MenuId::Pick && !seen.contains(&u.menu) { queue.push(u.menu); }
                        p += 1;
                    }
                    rows += 1;
                    if !any { dead.push(format!("{menu:?} / {} / {what}", spec.tabs[t].name)); }
                }
            }
        }
        println!("== {layout:?}: {rows} rows pressed over {} menus; {} without any response", seen.len(), dead.len());
        for d in &dead { println!("   DEAD {d}"); }
        println!("   still classic dialogs: {}", classic.join(", "));
    }
}

/// Every switch of every sheet, there and back: on, the sheet closed and opened again (the switch still says on), a
/// view the switch put on the screen closed by its X (the switch then says off) and brought back by the switch, and
/// off again (the set-up is what it was). What does not hold is listed. `OOS_WALK=1 cargo test walk_on_off -- --ignored
/// --nocapture`.
#[test]
#[ignore]
fn walk_on_off_and_back() {
    use super::dock::VKind;
    if std::env::var("OOS_WALK").is_err() { return; }
    const KINDS: [VKind; 8] = [VKind::Measure, VKind::Eye, VKind::Pane, VKind::Table, VKind::Sim, VKind::Hist, VKind::Xy, VKind::View];
    for layout in [Layout::Minimal, Layout::Studio] {
        let (mut switches, mut views_back, mut found) = (0, 0, Vec::new());
        let mut queue = menus(&fresh(layout));
        queue.extend([MenuId::Network, MenuId::Remote, MenuId::Ssh, MenuId::Time, MenuId::SelfCal, MenuId::Dvm, MenuId::Zone, MenuId::Sim, MenuId::MeasStat]);
        let mut seen: Vec<MenuId> = Vec::new();
        while let Some(menu) = queue.pop() {
            if seen.contains(&menu) { continue; }
            seen.push(menu);
            let probe = fresh(layout);
            let Some(spec) = probe.sheet_spec(menu) else { continue };
            for (t, tab) in spec.tabs.iter().enumerate() {
                for &it in &tab.rows {
                    if let Item::Open(id) = it { if !seen.contains(&id) { queue.push(id); } }
                    if probe.sheet_row_kind(it) != RowKind::Toggle { continue; }
                    let what = format!("{menu:?} / {} / {}", tab.name, probe.sheet_label(it));
                    let mut u = fresh(layout);
                    let open = |u: &mut Ui| -> Option<Rect> {
                        if u.menu_open { u.close_menu(); }
                        u.open_menu(menu);
                        u.step(0.04);
                        if let Some(r) = u.sheet_geo().and_then(|g| g.tabs.get(t).copied()) { let (x, y) = mid(r); tap(u, x, y); }
                        u.sheet_geo().and_then(|g| g.rows.iter().find(|r| r.it == it).map(|r| r.ctrl))
                    };
                    let Some(ctrl) = open(&mut u) else { found.push(format!("{what}: the row is not on its tab")); continue };
                    if u.item_blocked(it).is_some() { continue; }
                    // (a frame: Studio writes its state into the set-up when it draws)
                    for _ in 0..2 { frame(&mut u); }
                    switches += 1;
                    let (start, setup) = (u.sheet_on(it), crate::store::setup_text(&u.m));
                    let views = |u: &Ui| KINDS.iter().copied().filter(|k| u.dock_shown(*k)).collect::<Vec<_>>();
                    let before = views(&u);
                    let (x, y) = mid(ctrl);
                    tap(&mut u, x, y);
                    for _ in 0..3 { frame(&mut u); }
                    if u.sheet_on(it) == start { found.push(format!("{what}: a tap does not switch it")); continue; }
                    let state = u.sheet_on(it);
                    let shown: Vec<VKind> = views(&u).into_iter().filter(|k| !before.contains(k)).collect();
                    // closed and opened again
                    match open(&mut u) {
                        None => found.push(format!("{what}: gone from its tab once it is switched")),
                        Some(_) if u.sheet_on(it) != state => found.push(format!("{what}: reads differently when the sheet is opened again")),
                        _ => {}
                    }
                    // a view it put on the screen: closed by its X, and brought back by the switch
                    if layout == Layout::Studio && state {
                        for k in shown {
                            u.close_menu();
                            for _ in 0..3 { frame(&mut u); }
                            let Some(leaf) = u.dock_leaf(k) else { found.push(format!("{what}: its view {k:?} is not on the screen with the sheet closed")); continue };
                            let (x, y) = mid(Ui::dock_close_rect(leaf));
                            tap(&mut u, x, y);
                            for _ in 0..3 { frame(&mut u); }
                            if u.dock_shown(k) { found.push(format!("{what}: the X of its view {k:?} does not close it")); continue; }
                            let Some(ctrl) = open(&mut u) else { found.push(format!("{what}: gone from its tab after the view was closed")); continue };
                            if u.sheet_on(it) { found.push(format!("{what}: still says on after the X of its view {k:?}")); }
                            else {
                                let (x, y) = mid(ctrl);
                                tap(&mut u, x, y);
                                for _ in 0..3 { frame(&mut u); }
                                if u.dock_shown(k) { views_back += 1; } else { found.push(format!("{what}: the switch does not bring the view {k:?} back")); }
                            }
                        }
                    }
                    // and back to the start
                    if let Some(ctrl) = open(&mut u) {
                        if u.sheet_on(it) != start {
                            let (x, y) = mid(ctrl);
                            tap(&mut u, x, y);
                            for _ in 0..3 { frame(&mut u); }
                        }
                        if u.sheet_on(it) != start { found.push(format!("{what}: does not switch back")); }
                        else if crate::store::setup_text(&u.m) != setup {
                            let (a, b) = (setup.clone(), crate::store::setup_text(&u.m));
                            let diff: Vec<String> = a.lines().zip(b.lines()).filter(|(x, y)| x != y).map(|(x, y)| format!("{x} -> {y}")).take(3).collect();
                            found.push(format!("{what}: on and off again leaves the set-up changed: {diff:?}"));
                        }
                    }
                }
            }
        }
        println!("== {layout:?}: {switches} switches there and back over {} sheets, {views_back} views closed by their X and brought back; {} findings", seen.len(), found.len());
        for f in &found { println!("   FOUND {f}"); }
    }
}

#[test]
#[ignore]
fn walk_the_bars() {
    if std::env::var("OOS_WALK").is_err() { return; }
    for layout in [Layout::Minimal, Layout::Studio] {
        let base = fresh(layout);
        let f = base.frame();
        let mut ys = vec![f.top.y + f.top.h / 2, f.bottom.y + f.bottom.h / 2];
        if layout == Layout::Studio { ys.push(f.bottom.y - 22); }
        for y in ys {
            let mut runs: Vec<(i32, i32, String)> = Vec::new();
            for x in (4..base.scr.w).step_by(8) {
                let mut u = fresh(layout);
                let before = sig(&mut u);
                tap(&mut u, x, y);
                let after = sig(&mut u);
                let what = if before == after { "-".to_string() } else if u.menu_open { format!("menu {}", u.menu_title(u.menu)) } else if let Some(t) = &u.toast { format!("toast {:?}", t.0) } else { "change".to_string() };
                match runs.last_mut() { Some(r) if r.2 == what => r.1 = x, _ => runs.push((x, x, what)) }
            }
            println!("== {layout:?} y={y}");
            for (a, b, w) in runs { println!("   x {a:4}..{b:4}  {w}"); }
        }
    }
}

#[test]
#[ignore]
fn walk_list_classic() {
    if std::env::var("OOS_WALK").is_err() { return; }
    let mut u = fresh(Layout::Studio);
    for menu in [MenuId::Remote] {
        u.open_menu(menu);
        let items = u.items(menu);
        println!("## {menu:?} ({} items)", items.len());
        for it in items { println!("   {:?} | {} | {:?} | {}", it, u.item_label(it), u.sheet_row_kind(it), u.value(it)); }
        u.close_menu();
    }
}

/// SIM mode behaves like a live acquisition: Run / Stop, Single, the trigger slope and level act on the simulated
/// record, and its measurements have values. Prints what it sees; asserts the parts that must hold.
#[test]
fn sim_mode_runs_stops_and_triggers_like_live() {
    for layout in [Layout::Studio, Layout::Minimal] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = layout;
        u.relayout();
        for _ in 0..8 { u.step(0.04); u.wait_sim_generation(); }
        u.select_option(Item::SimPreset, "USB 2.0 HS");
        u.activate(Item::SimPlay);
        u.wait_sim_generation();
        for _ in 0..30 { u.step(0.04); u.wait_sim_generation(); }
        assert!(u.sim_mode() && u.record_mode(), "{layout:?}: SIM plays");
        assert_eq!(u.m.run, RunState::Run, "{layout:?}: running after Play");
        assert_eq!(u.m.status, TrigStatus::Triggered, "{layout:?}: triggered on the simulated signal");
        let slot = u.m.sim_slot;
        assert_eq!(u.m.trig_ref, Some(slot), "{layout:?}: the trigger source is the simulated ref");
        // Run / Stop
        u.key(key::RUNSTOP);
        for _ in 0..4 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.m.run, RunState::Stop, "{layout:?}: Stop");
        assert!(u.sim_mode(), "{layout:?}: Stop keeps the SIM session");
        u.key(key::RUNSTOP);
        for _ in 0..10 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.m.run, RunState::Run, "{layout:?}: Run again");
        assert_eq!(u.m.status, TrigStatus::Triggered, "{layout:?}: triggered again");
        // a level outside the signal in Normal mode waits; back inside it triggers
        let level = u.m.trig_level;
        u.m.trig_mode = TrigMode::Normal;
        u.m.trig_level = 50.0;
        for _ in 0..20 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.m.status, TrigStatus::Wait, "{layout:?}: no trigger at 50 V");
        u.m.trig_level = level;
        for _ in 0..20 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.m.status, TrigStatus::Triggered, "{layout:?}: triggers at the level again");
        // the slope is a setting of the simulated trigger too
        u.open_menu(MenuId::Trigger);
        u.select_option(Item::TrigSlope, "Falling");
        u.close_menu();
        for _ in 0..20 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.value(Item::TrigSlope), "Falling");
        assert_eq!(u.m.status, TrigStatus::Triggered, "{layout:?}: triggers on the falling edge");
        // Single takes one record and stops
        u.key(key::SINGLE);
        for _ in 0..30 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.m.run, RunState::Stop, "{layout:?}: Single stops after its record");
        assert!(u.sim_mode(), "{layout:?}: still the SIM session");
        // measurements of the simulated ref have values
        let rows = u.meas_rows();
        assert!(rows.iter().any(|r| r.src == WaveId::Ref(slot)), "{layout:?}: measurements on the SIM source: {}", rows.len());
        // Live returns, and adding a signal while live keeps the live acquisition
        u.return_to_live();
        for _ in 0..10 { u.step(0.04); u.wait_sim_generation(); }
        assert!(!u.sim_mode() && !u.record_mode() && u.m.ch[0].on, "{layout:?}: Live");
        let before = u.refs.iter().filter(|r| r.is_some()).count();
        u.sim_add_next();
        u.wait_sim_generation();
        for _ in 0..6 { u.step(0.04); u.wait_sim_generation(); }
        assert_eq!(u.refs.iter().filter(|r| r.is_some()).count(), before + 1, "{layout:?}: a SIM signal as one more ref while live");
        assert!(!u.sim_mode() && u.m.ch[0].on && u.m.run == RunState::Run, "{layout:?}: the live acquisition goes on");
    }
}

/// While live, the Reference sheet loads a file from storage into the chosen slot, through its list, and the live
/// acquisition goes on. Minimal and Studio share the flow.
#[test]
fn a_file_loads_into_a_ref_while_live() {
    for layout in [Layout::Studio, Layout::Minimal] {
        let dir = std::env::temp_dir().join(format!("oos-walk-{}-{layout:?}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut csv = String::from("Time (s),Volts\n");
        for i in 0..400 { csv.push_str(&format!("{:.12e},{}\n", i as f64 * 1e-9, if (i / 50) % 2 == 0 { 0.0 } else { 1.0 })); }
        std::fs::write(dir.join("capture.csv"), csv).unwrap();
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = layout;
        u.files.dir = dir.clone();
        u.relayout();
        for _ in 0..8 { u.step(0.04); }
        u.open_menu(MenuId::Ref);
        u.ref_page.sel = 2;
        u.activate(Item::RefImport);
        assert_eq!(u.menu, MenuId::Pick, "{layout:?}: the files of storage as a list");
        assert_eq!(u.options(Item::RefImport), ["capture.csv"], "{layout:?}");
        u.activate(Item::PickOpt(0));
        for _ in 0..6 { u.step(0.04); }
        let r = u.refs[2].as_ref().unwrap_or_else(|| panic!("{layout:?}: R3 is loaded: {:?}", u.toast));
        assert!(r.on && r.v.len() == 400, "{layout:?}: shown, {} points", r.v.len());
        assert!(!u.record_mode() && u.m.ch[0].on && u.m.run == RunState::Run, "{layout:?}: the live acquisition goes on");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Two fingers are a pinch, never a long press: resting them for longer than the hold time opens no menu, and the
/// pinch that follows changes the time scale. One finger held still opens the context menu.
#[test]
fn two_fingers_never_open_the_hold_menu_and_one_finger_does() {
    for layout in [Layout::Studio, Layout::Minimal] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = layout;
        u.relayout();
        for _ in 0..8 { u.step(0.04); }
        let g = u.plot.grid;
        let (cx, cy) = (g.x + g.w / 2, g.y + g.h / 2);
        let menu = |u: &Ui| u.workspace_popup_open() || u.studio.ctx.is_some() || u.menu_open;
        // both fingers land and rest
        u.handle(Ev::Touch { id: 0, phase: Phase::Down, x: cx - 60, y: cy });
        for _ in 0..3 { u.step(0.04); }
        u.handle(Ev::Touch { id: 1, phase: Phase::Down, x: cx + 60, y: cy });
        for _ in 0..25 { u.step(0.04); }
        assert!(!menu(&u), "{layout:?}: no menu while two fingers rest");
        // then they spread: a pinch
        let before = u.m.tscale;
        for s in 1..=8 {
            u.handle(Ev::Touch { id: 0, phase: Phase::Move, x: cx - 60 - 20 * s, y: cy });
            u.handle(Ev::Touch { id: 1, phase: Phase::Move, x: cx + 60 + 20 * s, y: cy });
            u.step(0.04);
        }
        u.handle(Ev::Touch { id: 1, phase: Phase::Up, x: cx + 220, y: cy });
        u.handle(Ev::Touch { id: 0, phase: Phase::Up, x: cx - 220, y: cy });
        for _ in 0..3 { u.step(0.04); }
        assert!(!menu(&u), "{layout:?}: no menu after the pinch");
        assert!(u.m.tscale != before || u.m.zoom, "{layout:?}: the pinch zoomed the time axis ({} -> {})", before, u.m.tscale);
        // the second finger arrives late, after the menu of the first finger's hold: the menu goes away
        u.handle(Ev::Touch { id: 0, phase: Phase::Down, x: cx - 60, y: cy });
        for _ in 0..20 { u.step(0.04); }
        assert!(menu(&u), "{layout:?}: one finger held opens the context menu");
        u.handle(Ev::Touch { id: 1, phase: Phase::Down, x: cx + 60, y: cy });
        u.step(0.04);
        assert!(!menu(&u), "{layout:?}: a second finger takes the menu away");
        u.handle(Ev::Touch { id: 1, phase: Phase::Up, x: cx + 60, y: cy });
        u.handle(Ev::Touch { id: 0, phase: Phase::Up, x: cx - 60, y: cy });
        for _ in 0..3 { u.step(0.04); }
        assert!(!menu(&u), "{layout:?}: and the lift is not a tap");
    }
}

#[test]
#[ignore]
fn walk_channel_taps() {
    if std::env::var("OOS_WALK").is_err() { return; }
    for layout in [Layout::Studio, Layout::Minimal] {
        let mut u = fresh(layout);
        u.m.run = RunState::Run;
        let on = |u: &Ui| (0..4).map(|c| if u.m.ch[c].on { '1' } else { '0' }).collect::<String>();
        let y = u.frame().bottom.y + u.frame().bottom.h / 2;
        println!("== {layout:?} start on={} bottom y={y}", on(&u));
        for x in (10..520).step_by(30) {
            let mut v = fresh(layout);
            v.m.ch[1].on = true; v.relayout(); v.step(0.04);
            let b = on(&v);
            tap(&mut v, x, y);
            let menu = if v.menu_open { format!("menu {}", v.menu_title(v.menu)) } else { String::new() };
            println!("   CH1+CH2 on, tap x={x}: on {b} -> {} {menu} popup={}", on(&v), v.workspace_popup_open());
        }
    }
}

/// Studio: a tap on the pill of a channel selects the channel and nothing else, wherever on the pill; a tap on the
/// selected pill opens its sheet, and so do two quick taps on any pill; a hold has Turn off as its first row, over
/// the pill. (The coloured badge was the channel's switch and the rest opened the sheet at once: a tap meant to
/// select switched a channel off, with the math trace and the measurements that hang on it.)
#[test]
fn a_tap_on_a_channel_pill_selects_and_the_next_one_opens_its_sheet() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..8 { u.step(0.04); }
    let b = u.studio_bottom();
    let pill = b.ch[1];
    assert!(pill.w >= 110, "{pill:?}");
    // (the first frame of a layout applies its entry rules, which close what is open)
    frame(&mut u);
    let (body, badge, y) = (pill.right() - 30, pill.x + 24, pill.y + pill.h / 2);
    assert_eq!(u.selected_workspace_source(), Some(WaveId::Ch(0)));
    // the badge and the body do the same: the first tap selects, and the channel stays on
    for x in [badge, body] {
        u.select_workspace_source(WaveId::Ch(0));
        u.now += 5.0;
        tap(&mut u, x, y);
        assert!(u.m.ch[1].on && !u.menu_open, "a tap at {x} did more than select");
        assert_eq!(u.selected_workspace_source(), Some(WaveId::Ch(1)));
        // the next tap, whenever it comes, opens the sheet of the selected channel
        u.now += 5.0;
        tap(&mut u, x, y);
        assert!(u.menu_open && u.menu == MenuId::Channel(1) && u.m.ch[1].on);
        // and one more closes it
        u.now += 5.0;
        tap(&mut u, x, y);
        assert!(!u.menu_open && u.m.ch[1].on);
    }
    // a double tap ends with the sheet open, on a pill that was selected (the second tap does not close it) and on
    // one that was not
    u.now += 5.0;
    tap(&mut u, body, y);
    u.now += 0.2;
    tap(&mut u, body, y);
    assert!(u.menu_open && u.menu == MenuId::Channel(1), "a double tap on the selected pill");
    // with a channel sheet open, a tap on another pill takes the sheet to that channel
    let first = u.studio_bottom().ch[0];
    u.now += 5.0;
    tap(&mut u, first.right() - 30, y);
    assert!(u.menu_open && u.menu == MenuId::Channel(0));
    u.close_menu();
    u.select_workspace_source(WaveId::Ch(0));
    u.now += 5.0;
    tap(&mut u, body, y);
    u.now += 0.2;
    tap(&mut u, body, y);
    assert!(u.menu_open && u.menu == MenuId::Channel(1), "a double tap on a pill that was not selected");
    u.close_menu();
    // a hold: the quick actions stand on the bar over the pill, Turn off first, rows a finger high
    u.studio_chip_popup(super::measdrag::HoldOn::Chan(1), body, y);
    let (rect, rows) = { let p = u.workspace.popup.as_ref().expect("the popup"); (p.rect, (0..p.rows.len()).map(|i| (p.rows[i].0.clone(), p.row_rect(i))).collect::<Vec<_>>()) };
    let bar = u.frame().bottom.y;
    assert!(rect.bottom() <= bar && bar - rect.bottom() <= 8, "the popup stands on the bar: {rect:?} {bar}");
    assert!(rect.x <= pill.x && rect.right() >= pill.x + 40 && rect.y >= u.frame().top.bottom(), "over its pill: {rect:?} {pill:?}");
    assert_eq!(rows[0].0, "Turn off");
    assert!(rows.iter().all(|(_, r)| r.h >= 48), "{rows:?}");
    if let Ok(dir) = std::env::var("OOS_WALK_SHOTS") {
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join("pill-hold.png"), &crate::store::png_of(&cv));
    }
    let off = rows[0].1;
    tap(&mut u, off.x + off.w / 2, off.y + off.h / 2);
    assert!(!u.m.ch[1].on && !u.menu_open, "Turn off of the hold switches CH2 off");
    assert!(u.m.ch[0].on, "CH1 is not touched");
    // its pill stays as a smaller one, and a tap on that switches the channel on again without a sheet
    let small = u.studio_bottom().ch[1];
    assert!((60..=96).contains(&small.w), "a smaller pill while it is off: {}", small.w);
    tap(&mut u, small.x + small.w / 2, small.y + small.h / 2);
    assert!(u.m.ch[1].on && !u.menu_open, "the small pill switches CH2 on");
    assert!(u.studio_bottom().ch[1].w >= 110);
}

/// Sliding the waveform with one finger is a pan, not a long press: no menu while the finger moves, however slowly,
/// nor when it rests after it has moved.
#[test]
fn a_one_finger_slide_never_opens_the_hold_menu() {
    for layout in [Layout::Studio, Layout::Minimal] {
        for (name, step_px, pause_first, rest_after) in [("slow slide", 1, 0, 0), ("fast slide", 12, 0, 0), ("short pause, then slide", 4, 8, 0), ("slide, then rest", 6, 0, 30)] {
            let mut u = Ui::new(Source::Demo { t: 0.0 });
            u.m.layout = layout;
            u.relayout();
            for _ in 0..8 { u.step(0.04); }
            let g = u.plot.grid;
            let (x0, y) = (g.x + g.w / 2, g.y + g.h / 2);
            let menu = |u: &Ui| u.workspace_popup_open() || u.studio.ctx.is_some() || u.menu_open;
            let delay = u.m.delay;
            u.handle(Ev::Touch { id: 0, phase: Phase::Down, x: x0, y });
            for _ in 0..pause_first { u.step(0.04); }
            let mut x = x0;
            for _ in 0..40 {
                x += step_px;
                u.handle(Ev::Touch { id: 0, phase: Phase::Move, x, y });
                u.step(0.04);
                assert!(!menu(&u), "{layout:?} {name}: a menu opened {} px into the slide", x - x0);
            }
            for _ in 0..rest_after { u.step(0.04); assert!(!menu(&u), "{layout:?} {name}: a menu opened while resting after the slide"); }
            u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
            for _ in 0..3 { u.step(0.04); }
            assert!(!menu(&u), "{layout:?} {name}: a menu after the lift");
            if step_px >= 4 { assert!(u.m.delay != delay, "{layout:?} {name}: the slide moved the waveform"); }
        }
    }
}

/// Studio: Overlay traces puts every source on one graticule and Stacked traces gives each its lane again; the
/// plot menu marks the mode in force. The zoom bar style is a row of Horizontal > Zoom.
#[test]
fn studio_overlay_and_stacked_switch_the_lanes() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..8 { u.step(0.04); }
    assert_eq!(u.plot.wins.len(), 2, "a lane each to begin with");
    assert!(u.trace_stacked());
    u.trace_mode(false);
    for _ in 0..4 { u.step(0.04); }
    assert_eq!(u.plot.wins.len(), 1, "Overlay: one graticule");
    assert!(!u.trace_stacked());
    u.trace_mode(true);
    for _ in 0..4 { u.step(0.04); }
    assert_eq!(u.plot.wins.len(), 2, "Stacked: the lanes are back");
    // every measurement has a badge while none is pinned by hand and there is room
    assert!(u.studio.pinned.is_none());
    assert!(u.studio_badges().len() > 3, "{} badges", u.studio_badges().len());
}

#[test]
#[ignore]
fn walk_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK_SHOTS") else { return };
    let shot = |u: &mut Ui, name: &str| {
        for _ in 0..6 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    };
    let studio = || {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.m.ch[1].on = true;
        u.relayout();
        for _ in 0..12 { u.step(0.04); }
        u
    };
    // the Add menu
    let mut u = studio();
    let add = u.studio_top().add;
    tap(&mut u, add.x + add.w / 2, add.y + add.h / 2);
    shot(&mut u, "1-add-menu");
    // the plot menu, then Overlay
    let mut u = studio();
    let g = u.plot.grid;
    u.context_menu(g.x + 300, g.y + 120);
    shot(&mut u, "2-plot-menu");
    u.trace_mode(false);
    shot(&mut u, "3-overlay");
    // Display > Grid and views: the Layout row
    let mut u = studio();
    u.open_menu(MenuId::Display);
    u.step(0.04);
    let t = u.sheet_geo().unwrap().tabs[1];
    tap(&mut u, t.x + t.w / 2, t.y + t.h / 2);
    shot(&mut u, "4-display-layout");
    // a reference and a math channel beside the channels, six measurements
    let mut u = studio();
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Add;
    u.relayout();
    shot(&mut u, "5-ref-and-math-pills");
    // Horizontal > Zoom with the zoom bar row
    let mut u = studio();
    u.open_menu(MenuId::Horizontal);
    u.step(0.04);
    let t = u.sheet_geo().unwrap().tabs[1];
    tap(&mut u, t.x + t.w / 2, t.y + t.h / 2);
    shot(&mut u, "6-horizontal-zoom");
}

#[test]
#[ignore]
fn walk_table_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK_SHOTS") else { return };
    let shot = |u: &mut Ui, name: &str| {
        for _ in 0..30 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    };
    let studio = |view: super::studio::StudioView, detail: bool, stats: bool| {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.m.ch[1].on = true;
        u.m.meas.extend([(Meas::Freq, WaveId::Ch(1)), (Meas::Vpp, WaveId::Ch(1)), (Meas::Vrms, WaveId::Ch(1))]);
        u.relayout();
        for _ in 0..12 { u.step(0.04); }
        u.studio_set_view(view);
        u.m.studio_detail = detail;
        u.studio.panel.stats = stats;
        u.relayout();
        u
    };
    use super::studio::StudioView as V;
    shot(&mut studio(V::Only, true, false), "t1-only");
    shot(&mut studio(V::Only, true, true), "t2-only-stats");
    shot(&mut studio(V::Docked, true, true), "t3-docked-detail");
    shot(&mut studio(V::Docked, false, false), "t4-docked-list");
    // the cards in a panel docked above the waveform, as wide as the screen
    let mut u = studio(V::Docked, false, false);
    u.studio.panel.cards = true;
    u.dock_apply(super::dock::VKind::Measure, super::dock::Drop::Split(super::dock::VKind::Wave, super::dock::Side::Top));
    u.relayout();
    for _ in 0..200 { u.step(0.04); }
    shot(&mut u, "t5-cards-wide");
    let mut u = studio(V::Docked, false, false);
    u.studio.panel.cards = true;
    u.relayout();
    for _ in 0..100 { u.step(0.04); }
    shot(&mut u, "t6-cards-narrow");
    // the detailed table in a panel docked above the waveform
    let mut u = studio(V::Docked, true, true);
    u.dock_apply(super::dock::VKind::Measure, super::dock::Drop::Split(super::dock::VKind::Wave, super::dock::Side::Top));
    u.relayout();
    for _ in 0..100 { u.step(0.04); }
    shot(&mut u, "t7-detail-wide");
}

#[test]
#[ignore]
fn walk_lane_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK_SHOTS") else { return };
    let shot = |u: &mut Ui, name: &str| {
        for _ in 0..30 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    };
    let studio = || {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.m.ch[1].on = true;
        u.relayout();
        for _ in 0..12 { u.step(0.04); }
        u
    };
    // four sources in lanes under a docked measurements panel
    let mut u = studio();
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Sub;
    u.studio_set_view(super::studio::StudioView::Docked);
    u.m.studio_detail = true;
    u.dock_apply(super::dock::VKind::Measure, super::dock::Drop::Split(super::dock::VKind::Wave, super::dock::Side::Top));
    u.relayout();
    shot(&mut u, "L1-four-lanes-short");
    // the same lanes with the whole body
    let mut u = studio();
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Sub;
    u.relayout();
    shot(&mut u, "L2-four-lanes");
    // a tap on the Area badge, then on Peak-to-Peak
    let mut u = studio();
    u.m.meas = vec![(Meas::Area, WaveId::Ch(0)), (Meas::Vpp, WaveId::Ch(1)), (Meas::Period, WaveId::Ch(0)), (Meas::AreaCyc, WaveId::Ch(1))];
    u.relayout();
    for _ in 0..30 { u.step(0.04); }
    let badges = u.studio_badges();
    let b = badges[0].1;
    tap(&mut u, b.x + b.w / 2, b.y + b.h / 2);
    shot(&mut u, "L3-area-mark");
    let b = badges[3].1;
    tap(&mut u, b.x + b.w / 2, b.y + b.h / 2);
    shot(&mut u, "L4-cycle-area-mark");
    let b = badges[1].1;
    tap(&mut u, b.x + b.w / 2, b.y + b.h / 2);
    shot(&mut u, "L5-pkpk-mark");
}

/// A tap on a badge shows its measurement on the waveform (Area shades what the trace encloses), the next tap on
/// it opens its menu, and closing the menu takes the mark away. A tap on another badge moves the mark.
#[test]
fn a_badge_tap_marks_its_measurement_on_the_waveform() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.meas = vec![(Meas::Area, WaveId::Ch(0)), (Meas::Vpp, WaveId::Ch(0)), (Meas::PEdges, WaveId::Ch(0))];
    u.relayout();
    for _ in 0..30 { u.step(0.04); }
    let badges = u.studio_badges();
    let (area, vpp, edges) = (badges[0], badges[1], badges[2]);
    let plain = sig(&mut u).0;
    tap(&mut u, area.1.x + area.1.w / 2, area.1.y + area.1.h / 2);
    assert_eq!(u.studio.mark, Some(area.0));
    assert!(u.studio.popup.is_none(), "no menu on the first tap");
    assert_ne!(sig(&mut u).0, plain, "the area is drawn");
    tap(&mut u, vpp.1.x + vpp.1.w / 2, vpp.1.y + vpp.1.h / 2);
    assert_eq!(u.studio.mark, Some(vpp.0), "another badge takes the mark");
    tap(&mut u, vpp.1.x + vpp.1.w / 2, vpp.1.y + vpp.1.h / 2);
    assert_eq!(u.studio.popup, Some(vpp.0), "the second tap opens the menu");
    u.studio.popup = None;
    u.step(0.04);
    assert!(u.studio.mark.is_none(), "closing the menu ends the mark");
    // a count has nothing to draw: its badge opens the menu at once
    tap(&mut u, edges.1.x + edges.1.w / 2, edges.1.y + edges.1.h / 2);
    assert_eq!(u.studio.popup, Some(edges.0));
}

/// Overlay through the real path: a finger held on the plot opens its menu, a tap on "Overlay traces" puts every
/// source on one graticule, and "Stacked traces" brings the lanes back. With channels, a reference and a math
/// channel, the measurements docked above, and more than one view.
#[test]
fn overlay_from_the_plot_menu_by_touch() {
    for (name, setup) in [("plain", 0), ("ref and math", 1), ("docked panel", 2), ("three views", 3)] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.m.ch[1].on = true;
        u.relayout();
        for _ in 0..12 { u.step(0.04); }
        if setup >= 1 {
            u.activate(Item::RefSave);
            u.m.math_select(0);
            u.m.math = crate::math::MathOp::Sub;
            u.relayout();
        }
        if setup >= 2 {
            u.studio_set_view(super::studio::StudioView::Docked);
            u.dock_apply(super::dock::VKind::Measure, super::dock::Drop::Split(super::dock::VKind::Wave, super::dock::Side::Top));
            u.relayout();
        }
        if setup >= 3 { u.views_set(3); }
        for _ in 0..12 { u.step(0.04); }
        u.toast = None;
        let lanes = u.plot.wins.len();
        assert!(lanes >= 2, "{name}: lanes to begin with");
        let pick = |u: &mut Ui, row: super::studioctx::CtxRow| {
            let g = u.plot.wins[0];
            let (x, y) = (g.x + g.w / 2, g.y + g.h / 2);
            u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
            for _ in 0..20 { u.step(0.04); }
            u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
            u.step(0.04);
            let rows = u.studio_ctx_rows();
            let r = rows.iter().find(|(k, _, _)| *k == row).unwrap_or_else(|| panic!("the menu has the row: {} rows", rows.len())).2;
            tap(u, r.x + r.w / 2, r.y + r.h / 2);
            for _ in 0..6 { u.step(0.04); }
        };
        pick(&mut u, super::studioctx::CtxRow::Overlay);
        assert_eq!(u.plot.wins.len(), 1, "{name}: Overlay is one graticule");
        assert!(!u.trace_stacked(), "{name}");
        pick(&mut u, super::studioctx::CtxRow::Stacked);
        assert_eq!(u.plot.wins.len(), lanes, "{name}: Stacked brings the lanes back");
    }
}

#[test]
#[ignore]
fn walk_overlay_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK_SHOTS") else { return };
    let shot = |u: &mut Ui, name: &str| {
        for _ in 0..30 { u.step(0.04); u.wait_sim_generation(); }
        u.toast = None;
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    };
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Sub;
    u.relayout();
    u.trace_mode(false);
    shot(&mut u, "O1-overlay-ch-ref-math");
    // SIM playing two signals, overlaid
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    u.select_option(Item::SimPreset, "USB 2.0 HS");
    u.sim_add_next();
    u.wait_sim_generation();
    u.select_option(Item::SimPreset, "PCIe 1.0");
    u.sim_add_next();
    u.wait_sim_generation();
    shot(&mut u, "O2-live-two-refs-stacked");
    u.trace_mode(false);
    shot(&mut u, "O3-live-two-refs-overlay");
    u.activate(Item::SimPlayRefs);
    shot(&mut u, "O4-sim-overlay");
}

#[test]
fn overlay_stays_with_a_reference_and_a_math_channel() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Sub;
    u.relayout();
    u.trace_mode(false);
    assert!(!u.trace_stacked());
    for i in 0..40 {
        u.step(0.04);
        assert!(!u.trace_stacked(), "frame {i}: overlay flag {} workspace.stacked {}", u.studio.overlay, u.workspace_stacked_flag());
        assert_eq!(u.plot.wins.len(), 1, "frame {i}: one graticule");
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        assert_eq!(u.plot.wins.len(), 1, "frame {i}: one graticule after the picture (overlay {} workspace.stacked {} win_layout {:?} view {:?})", u.studio.overlay, u.workspace_stacked_flag(), u.m.win_layout, u.m.view);
        assert!(!u.trace_stacked(), "frame {i}: after the picture");
    }
    // and with a wait for the SIM worker in between, as the picture tests do
    u.wait_sim_generation();
    u.toast = None;
    u.step(0.04);
    assert_eq!(u.plot.wins.len(), 1);
}

/// A finger held on the plot, then a tap on a row of its menu.
fn pick_ctx(u: &mut Ui, row: super::studioctx::CtxRow) {
    let g = u.plot.wins[0];
    let (x, y) = (g.x + g.w / 3, g.y + g.h / 2);
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    for _ in 0..20 { u.step(0.04); }
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
    u.step(0.04);
    let rows = u.studio_ctx_rows();
    let r = rows.iter().find(|(k, _, _)| *k == row).unwrap_or_else(|| panic!("the menu has the row: {} rows", rows.len())).2;
    tap(u, r.x + r.w / 2, r.y + r.h / 2);
    for _ in 0..6 { u.step(0.04); }
}

/// Fit all lanes from the plot menu: every shown source gets a scale that holds its signal and is centred in its
/// lane; the time base and the trigger source stay. Pictures with `OOS_WALK_SHOTS`.
#[test]
fn fit_all_lanes_scales_every_source_to_its_lane() {
    let shot = |u: &mut Ui, name: &str| {
        let Ok(dir) = std::env::var("OOS_WALK_SHOTS") else { return };
        for _ in 0..20 { u.step(0.04); }
        u.toast = None;
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    };
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..12 { u.step(0.04); u.render(&mut Canvas::new(1024, 600)); }
    u.activate(Item::RefSave);
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Sub;
    // badly set: CH1 far too coarse and off centre, CH2 clipped, the reference tiny
    u.m.ch[0].vscale = VSCALES.len() - 2;
    u.m.ch[0].offset = 20.0;
    u.m.ch[1].vscale = 2;
    u.refs[0].as_mut().unwrap().vpd = 10.0;
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    shot(&mut u, "F1-before-fit");
    let (tscale, trig) = (u.m.tscale, u.m.trig_src);
    pick_ctx(&mut u, super::studioctx::CtxRow::FitLanes);
    for _ in 0..12 { u.step(0.04); }
    shot(&mut u, "F2-after-fit");
    assert_eq!((u.m.tscale, u.m.trig_src), (tscale, trig), "the time base and the trigger stay");
    let rec = u.last[0].as_ref().unwrap().clone();
    for c in 0..2 {
        let (lo, hi) = rec.ch[c].iter().fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
        let ch = &u.m.ch[c];
        let divs = (hi - lo) / ch.volts_per_div();
        assert!((2.0..=6.01).contains(&divs), "CH{}: {divs} divisions of 8", c + 1);
        assert!(((hi + lo) / 2.0 + ch.offset).abs() <= ch.volts_per_div() * 0.6, "CH{} is centred", c + 1);
    }
    let r = u.refs[0].as_ref().unwrap();
    let (lo, hi) = r.v.iter().fold((f32::MAX, f32::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    assert!((2.0..=6.01).contains(&((hi - lo) / r.vpd)), "R1: {} divisions", (hi - lo) / r.vpd);
    // Overlay is kept with the setup and comes back after a restart
    pick_ctx(&mut u, super::studioctx::CtxRow::Overlay);
    assert!(u.m.studio_overlay && u.plot.wins.len() == 1);
    shot(&mut u, "F3-overlay-after-fit");
    let text = crate::store::setup_text(&u.m);
    assert!(text.contains("studio_traces = overlay"), "the setup has the trace mode");
}

/// One frame as the device runs it: the picture is drawn after every step.
fn frame(u: &mut Ui) { u.step(0.033); u.render(&mut Canvas::new(1024, 600)); }

/// A tap as a finger makes it: it lands, wanders a few pixels, and lifts a little off where it landed.
fn finger_tap(u: &mut Ui, x: i32, y: i32, wander: i32) {
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    frame(u);
    for (dx, dy) in [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0)] {
        u.handle(Ev::Touch { id: 0, phase: Phase::Move, x: x + dx * wander, y: y + dy * wander });
        frame(u);
    }
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x: x - wander, y });
    for _ in 0..3 { frame(u); }
}

/// A hold as a finger makes it: it rests for `frames`, never quite still, then lifts.
fn finger_hold(u: &mut Ui, x: i32, y: i32, wander: i32, frames: usize) {
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    for k in 0..frames {
        let (dx, dy) = [(1, 0), (0, 1), (-1, 0), (0, -1)][k % 4];
        u.handle(Ev::Touch { id: 0, phase: Phase::Move, x: x + dx * wander, y: y + dy * wander });
        frame(u);
    }
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
    for _ in 0..3 { frame(u); }
}

/// Real fingers: the long-press menu of the plot and the zoom tab must work when the finger wanders by a few pixels.
#[test]
fn a_wandering_finger_still_taps_and_holds() {
    for wander in [0, 2, 4, 6] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.m.ch[1].on = true;
        u.relayout();
        for _ in 0..12 { frame(&mut u); }
        // the long-press menu
        let g = u.plot.wins[1];
        finger_hold(&mut u, g.x + g.w / 3, g.y + g.h / 2, wander, 24);
        assert!(u.studio.ctx.is_some(), "wander {wander}: a held finger opens the plot menu");
        let rows = u.studio_ctx_rows();
        let r = rows.iter().find(|(k, _, _)| *k == super::studioctx::CtxRow::Overlay).unwrap().2;
        finger_tap(&mut u, r.x + r.w / 2, r.y + r.h / 2, wander.min(3));
        assert_eq!(u.plot.wins.len(), 1, "wander {wander}: a tap on Overlay traces overlays");
        // a button of the zoom row
        u.set_zoom(true);
        for _ in 0..6 { frame(&mut u); }
        let t = u.studio_zoom_bar().expect("the zoom row").boxr;
        finger_tap(&mut u, t.x + t.w / 2, t.y + t.h / 2, wander);
        assert!(u.m.zoom && u.zoom_box.arm, "wander {wander}: a tap on Box arms the zoom box");
    }
}

/// The desktop window: a right click opens the plot menu (`context_menu`), a left click on a row picks it.
#[test]
fn a_right_click_menu_row_acts_on_a_left_click() {
    for with_panel in [false, true] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.m.ch[1].on = true;
        if with_panel { u.workspace.desktop_panel = Some(false); }
        u.relayout();
        for _ in 0..12 { frame(&mut u); }
        let g = u.plot.wins[1];
        u.context_menu(g.x + g.w / 3, g.y + g.h / 2);
        for _ in 0..4 { frame(&mut u); }
        assert!(u.studio.ctx.is_some(), "panel {with_panel}: the right click opens the plot menu");
        let rows = u.studio_ctx_rows();
        let r = rows.iter().find(|(k, _, _)| *k == super::studioctx::CtxRow::Overlay).unwrap().2;
        finger_tap(&mut u, r.x + r.w / 2, r.y + r.h / 2, 0);
        assert_eq!(u.plot.wins.len(), 1, "panel {with_panel}: the left click on Overlay traces overlays");
        assert!(u.studio.ctx.is_none(), "panel {with_panel}: the menu closes");
        // Stacked again, then Fit all lanes
        let g = u.plot.wins[0];
        u.context_menu(g.x + g.w / 3, g.y + g.h / 2);
        for _ in 0..4 { frame(&mut u); }
        let rows = u.studio_ctx_rows();
        let r = rows.iter().find(|(k, _, _)| *k == super::studioctx::CtxRow::Stacked).unwrap().2;
        finger_tap(&mut u, r.x + r.w / 2, r.y + r.h / 2, 0);
        assert_eq!(u.plot.wins.len(), 2, "panel {with_panel}: Stacked traces");
    }
}

/// A channel that reaches the end of the ADC range says so in the plot and over SCPI; one that does not is quiet.
#[test]
fn a_clipping_channel_warns_on_screen_and_over_scpi() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..20 { frame(&mut u); }
    assert!(u.clipping_channels().is_empty(), "the demo signals fit their scales");
    let quiet = sig(&mut u).0;
    // CH1 at 200 mV/div: its 2.5 V square runs off the converter
    u.m.ch[0].vscale = VSCALES.iter().position(|&v| v >= 0.2).unwrap();
    u.m.ch[0].offset = 0.0;
    for _ in 0..20 { frame(&mut u); }
    assert_eq!(u.clipping_channels(), vec![0], "CH1 clips, CH2 does not");
    assert_ne!(sig(&mut u).0, quiet, "the plot says so");
    let mut scpi = crate::scpi::Scpi::open(0);
    let cv = Canvas::new(1024, 600);
    let ask = |scpi: &mut crate::scpi::Scpi, u: &mut Ui, q: &str| String::from_utf8(scpi.exec_line(u, &cv, q)).unwrap().trim().to_string();
    if let Ok(dir) = std::env::var("OOS_WALK_SHOTS") {
        let mut pic = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut pic);
        let _ = crate::store::write(&std::path::Path::new(&dir).join("clipping.png"), &crate::store::png_of(&pic));
    }
    assert_eq!(ask(&mut scpi, &mut u, ":CHANnel1:CLIPping?"), "1");
    assert_eq!(ask(&mut scpi, &mut u, ":CHANnel2:CLIPping?"), "0");
    assert_eq!(ask(&mut scpi, &mut u, ":CHAN3:CLIP?"), "0", "a channel that is off does not clip");
    // back on a scale that holds it
    u.m.ch[0].vscale = VSCALES.iter().position(|&v| v >= 1.0).unwrap();
    u.m.ch[0].offset = -1.5;
    for _ in 0..20 { frame(&mut u); }
    assert!(u.clipping_channels().is_empty());
    assert_eq!(ask(&mut scpi, &mut u, ":CHANnel1:CLIPping?"), "0");
}

/// A vertical drag in the lane of a math channel moves it, as it moves a channel in its lane.
#[test]
fn a_math_lane_drags_up_and_down() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.ch[1].on = true;
    u.relayout();
    for _ in 0..12 { frame(&mut u); }
    u.m.math_select(0);
    u.m.math = crate::math::MathOp::Sub;
    u.relayout();
    for _ in 0..12 { frame(&mut u); }
    let lane = *u.plot.wins.last().unwrap();
    let before = u.m.math_ch(0).offset;
    let (x, y) = (lane.x + lane.w / 2, lane.y + lane.h / 2);
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    for s in 1..=8 { u.handle(Ev::Touch { id: 0, phase: Phase::Move, x, y: y + 5 * s }); frame(&mut u); }
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y: y + 40 });
    for _ in 0..3 { frame(&mut u); }
    let after = u.m.math_ch(0).offset;
    assert!(after < before - 0.5, "the math channel moved down: {before} -> {after} divisions");
    assert_eq!(u.m.ch[0].offset, -1.5, "CH1 did not move");
}

/// Audit: every measurement of the catalog, one at a time, on the demo signals (all four channels on). Prints what each
/// row reads. `OOS_MEAS_AUDIT=1 cargo test meas_audit_table -- --ignored --nocapture`.
#[test]
#[ignore]
fn meas_audit_table() {
    for (_, def) in crate::measures::all() {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        u.relayout();
        for c in 0..4 { u.m.ch[c].on = true; }
        u.m.meas.clear();
        u.m.meas2.clear();
        u.m.cat.clear();
        u.sources.a = WaveId::Ch(0);
        u.sources.b = WaveId::Ch(1);
        u.cat.c = WaveId::Ch(2);
        u.cat.d = WaveId::Ch(3);
        for _ in 0..6 { u.step(0.04); }
        u.catalog_add(def);
        let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
        let mut rows;
        loop {
            u.step(0.04);
            rows = u.meas_rows();
            if rows.iter().all(|r| !r.computing) || std::time::Instant::now() > end { break; }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let kind = match def.kind { crate::measures::Kind::Standard(_) => "std", crate::measures::Kind::Pair(_) => "pair", crate::measures::Kind::Module => "mod" };
        match rows.first() {
            Some(r) => println!("AUDIT|{}|{}|{}|{:?}|{}|{:e}|{}|{}|{}", def.id, kind, def.name, def.roles, r.text(), r.v, def.unit, if r.computing { "COMPUTING" } else { "" }, r.reason),
            None => println!("AUDIT|{}|{}|{}|{:?}|NO ROW||{}||", def.id, kind, def.name, def.roles, def.unit),
        }
    }
}

/// A pulse train whose every figure is known: 1 kHz, 0 to 1 V, high for 300 µs, edges of 10 µs, ten cycles in 10 ms.
fn known_pulses(delay: f64) -> Vec<f32> {
    (0..10_000).map(|i| {
        let t = (i as f64 * 1e-6 - 200e-6 - delay).rem_euclid(1e-3);
        (if t < 10e-6 { t / 10e-6 } else if t < 300e-6 { 1.0 } else if t < 310e-6 { 1.0 - (t - 300e-6) / 10e-6 } else { 0.0 }) as f32
    }).collect()
}

/// One catalog measurement on the known pulse train in R1 (the same 100 µs later in R2) over the record: its value.
fn known_value(def: &'static crate::measures::Def) -> (f64, String) {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.run = RunState::Stop;
    u.m.ch.iter_mut().for_each(|c| c.on = false);
    u.m.meas.clear();
    u.m.meas2.clear();
    u.m.cat.clear();
    (u.m.meas_full_record, u.m.meas_out, u.m.meas_n) = (true, oos_dsp::measure::Outside::Above, 6);
    for (r, delay) in [(0, 0.0), (1, 100e-6)] {
        u.refs[r] = Some(RefWave { src: 0, color: r, label: String::new(), t0: -5e-3, dt: 1e-6, v: known_pulses(delay).into(), vpd: 0.2, offset: 0.0, on: true });
    }
    u.sources.a = WaveId::Ref(0);
    u.sources.b = WaveId::Ref(1);
    for _ in 0..3 { u.step(0.04); }
    u.catalog_add(def);
    let end = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        u.step(0.04);
        let rows = u.meas_rows();
        if rows.iter().all(|r| !r.computing) || std::time::Instant::now() > end {
            return rows.first().map_or((f64::NAN, "NO ROW".into()), |r| (r.v, r.text()));
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}

/// The standard and the pair measurements read what the pulse train is, each within 2 % (edge times within a sample).
#[test]
fn the_measurements_read_a_known_pulse_train() {
    let rms = ((290.0 + 20.0 / 3.0) / 1000.0f64).sqrt();
    let want: &[(&str, f64)] = &[
        ("Amplitude", 1.0), ("Peak-to-Peak", 1.0), ("Maximum", 1.0), ("Minimum", 0.0), ("Top", 1.0), ("Base", 0.0),
        ("Upper Level", 0.9), ("Middle Level", 0.5), ("Lower Level", 0.1), ("Positive Overshoot", 0.0), ("Negative Overshoot", 0.0),
        ("Mean", 0.3), ("RMS", rms), ("AC RMS", (rms * rms - 0.09).sqrt()), ("Area", 3e-3), ("Standard Deviation", (rms * rms - 0.09).sqrt()),
        ("Frequency", 1e3), ("Period", 1e-3), ("Positive Pulse Width", 300e-6), ("Negative Pulse Width", 700e-6),
        ("Positive Duty Cycle", 30.0), ("Negative Duty Cycle", 70.0), ("Rise Time", 8e-6), ("Fall Time", 8e-6),
        ("Rising Slew Rate", 1e5), ("Falling Slew Rate", -1e5), ("Rising Edges", 10.0), ("Falling Edges", 10.0),
        ("Positive Pulses", 10.0), ("Negative Pulses", 9.0), ("Cycle Mean", 0.3), ("Cycle RMS", rms), ("Cycle Area", 300e-6),
        ("Cycle Amplitude", 1.0), ("Cycle Top", 1.0), ("Cycle Base", 0.0), ("Cycle Peak-to-Peak", 1.0), ("Cycle Maximum", 1.0),
        ("Cycle Minimum", 0.0), ("High Time", 292e-6), ("Low Time", 692e-6),
        // at or above the upper level of 0.9 V: from 9 us into the rise to 1 us into the fall, as the high time
        ("Time Outside Level", 292e-6),
        ("Delay rise-rise", 100e-6), ("Delay fall-fall", 100e-6), ("Phase rise-rise", 36.0), ("Phase fall-fall", 36.0),
        ("Delay rise-fall", 400e-6), ("Delay fall-rise", -200e-6), ("Phase rise-fall", 144.0), ("Phase fall-rise", 288.0),
        ("Median", 0.0), ("Mode", 0.0), ("Duration N-Periods", 6e-3), ("Time of Maximum", -4.79e-3), ("Time of Minimum", -5e-3),
    ];
    let mut bad = Vec::new();
    let mut seen = 0;
    for (_, def) in crate::measures::all() {
        if matches!(def.kind, crate::measures::Kind::Module) { continue; }
        let (v, text) = known_value(def);
        let expect = want.iter().find(|w| w.0 == def.name).map(|w| w.1);
        println!("KNOWN|{}|{}|{}|{:e}|{}", def.id, def.name, text, v, expect.map_or("-".to_string(), |e| format!("{e:e}")));
        if let Some(e) = expect {
            seen += 1;
            let tol = if def.unit == "s" { 2e-6 } else { e.abs() * 0.02 + 0.012 };
            if !(v - e).abs().le(&tol) { bad.push(format!("{}: {} (want {:e})", def.name, text, e)); }
        }
    }
    assert_eq!(seen, want.len(), "a name of the table is not in the catalog");
    assert!(bad.is_empty(), "{bad:#?}");
}

/// Every SIM preset generated into a reference while the scope runs live: the acquisition goes on, the reference is
/// drawn beside the live channel, both can be measured, and the eye diagram takes the reference as its source.
#[test]
fn every_sim_preset_sits_in_a_ref_beside_the_live_channels() {
    // the lanes: a bus preset is a few rounds of traffic in several references, with a test of its own
    // (`sim_protocol.rs`)
    let presets: Vec<String> = oos_dsp::serial::names().into_iter().map(|s| s.to_string()).collect();
    assert!(presets.len() >= 3, "{presets:?}");
    for (n, name) in presets.iter().enumerate() {
        let mut u = fresh_live(Layout::Studio);
        u.select_option(Item::SimPreset, name);
        u.sim_add_next();
        let end = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while u.refs[0].is_none() && std::time::Instant::now() < end { u.step(0.04); u.wait_sim_generation(); }
        let r = u.refs[0].as_ref().unwrap_or_else(|| panic!("{name}: R1 is filled: {:?}", u.toast));
        let (len, dt) = (r.v.len(), r.dt);
        assert!(r.on && len > 100 && r.v.iter().all(|x| x.is_finite()), "{name}: shown, {len} points");
        // as long as the live screen (2 ms here), or as long as the budget of a reference allows (4 M samples in a test)
        assert!(len as f64 * dt >= 2e-3 * 0.99 || len >= 4_000_000, "{name}: {len} points of {dt:e} s are {:e} s", len as f64 * dt);
        let (lo, hi) = r.v.iter().fold((f32::MAX, f32::MIN), |(a, b), &x| (a.min(x), b.max(x)));
        assert!(hi - lo > 1e-3, "{name}: the waveform has a swing, {lo}..{hi}");
        for _ in 0..6 { u.step(0.04); u.wait_sim_generation(); }
        assert!(!u.sim_mode() && !u.record_mode(), "{name}: still live");
        assert!(u.m.run == RunState::Run && u.m.ch[0].on && u.m.trig_ref.is_none(), "{name}: the live trigger and channel stay");
        // the live channel and the reference are both measured
        u.m.meas = vec![(Meas::Freq, WaveId::Ch(0)), (Meas::Vpp, WaveId::Ref(0))];
        u.m.meas2.clear();
        u.m.cat.clear();
        // a long reference is measured in the background: wait for the value, not for a number of frames
        let end = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let mut rows = u.meas_rows();
        while rows.iter().any(|r| r.computing || !r.v.is_finite()) && std::time::Instant::now() < end {
            u.step(0.04);
            std::thread::sleep(std::time::Duration::from_millis(5));
            rows = u.meas_rows();
        }
        assert!((rows[0].v - 1e3).abs() < 20.0, "{name}: CH1 is still measured live, {}", rows[0].text());
        assert!((rows[1].v - (hi - lo) as f64).abs() < 0.05 * (hi - lo) as f64 + 1e-3, "{name}: R1 peak-to-peak {} of {}", rows[1].text(), hi - lo);
        let drawn = u.trace_sources();
        assert!(drawn.iter().any(|t| matches!(t, super::workspace::Trace::Channel(0))) && drawn.iter().any(|t| matches!(t, super::workspace::Trace::Reference(0))), "{name}: both are drawn");
        // the eye diagram of the reference
        u.m.eye_src = WaveId::Ref(0);
        u.m.view = AView::Eye;
        let end = std::time::Instant::now() + std::time::Duration::from_secs(40);
        while u.eye_readings().iter().all(|(_, v)| !v.is_finite()) && std::time::Instant::now() < end {
            u.step(0.04);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let eye = u.eye_readings();
        let found = eye.iter().filter(|(_, v)| v.is_finite()).count();
        println!("SIMREF|{n}|{name}|{len} pts dt {dt:e}|pp {}|eye figures {found}/{}|status {:?}", rows[1].text(), eye.len(), u.azr.page.status);
        if std::env::var("OOS_WALK").is_ok() && n < 4 {
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            u.render(&mut cv);
            let _ = crate::store::write(&std::path::Path::new(&std::env::var("OOS_WALK").unwrap()).join(format!("sim-eye-{n}.png")), &crate::store::png_of(&cv));
        }
    }
}

fn fresh_live(layout: Layout) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    u.relayout();
    for _ in 0..8 { u.step(0.04); u.wait_sim_generation(); }
    u
}

#[test]
#[ignore]
fn meas_defs_dump() {
    for (m, def) in crate::measures::all() {
        if matches!(def.kind, crate::measures::Kind::Module) { println!("DEF|{m}|{}|{}|{}|{:?}|{}", def.id, def.name, def.unit, def.roles, def.blurb); }
    }
}

/// Picture of the histogram view in Studio, in overlay and in stacked lanes (`OOS_WALK=dir`).
#[test]
#[ignore]
fn walk_hist_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK") else { return };
    for (name, on) in [("hist-1", vec![0]), ("hist-3", vec![0, 1, 2])] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        for c in 0..4 { u.m.ch[c].on = on.contains(&c); }
        u.m.hist = Hist::Vertical;
        u.relayout();
        for _ in 0..20 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    }
}

/// The histogram's mean and sigma are those of the samples, whatever the height of the lane its bars are read from.
#[test]
fn the_histogram_header_reads_the_samples_in_any_lane_height() {
    for on in [vec![0], vec![0, 1, 2]] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = Layout::Studio;
        for c in 0..4 { u.m.ch[c].on = on.contains(&c); }
        u.m.hist = Hist::Vertical;
        u.relayout();
        for _ in 0..20 { u.step(0.04); }
        let (mean, sd) = (u.meas_number(WaveId::Ch(0), Meas::Vavg), u.meas_number(WaveId::Ch(0), Meas::StdDev));
        assert!((mean - 1.25).abs() < 0.02 && (sd - 1.25).abs() < 0.03, "{} channels: mean {mean} sd {sd}", on.len());
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
    }
}

/// A session comes back whole: the settings, the record on the screen, the reference slots with their samples and
/// their look. What was on the bench before the load is gone.
#[test]
fn a_session_saves_and_loads_the_whole_bench() {
    let dir = std::env::temp_dir().join(format!("oos-session-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut u = fresh_live(Layout::Studio);
    u.files.dir = dir.clone();
    u.m.ch[1].on = true;
    u.m.ch[0].vscale = 5;
    u.m.meas = vec![(Meas::Freq, WaveId::Ch(0)), (Meas::Vpp, WaveId::Ref(2))];
    for _ in 0..10 { u.step(0.04); }
    u.refs[2] = Some(RefWave { src: 1, color: 2, label: "golden".into(), t0: -1e-3, dt: 1e-6, v: known_pulses(0.0).into(), vpd: 0.5, offset: 0.25, on: true });
    u.m.run = RunState::Stop;
    for _ in 0..4 { u.step(0.04); }
    let record = u.last[0].clone().expect("a record on the screen");
    let (setup, tscale) = (u.setup_text_full(), u.m.tscale);
    let settle = |u: &mut Ui| {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let mut rows = u.meas_rows();
        while rows.iter().any(|r| r.computing) && std::time::Instant::now() < end { u.step(0.04); rows = u.meas_rows(); }
        rows.iter().map(|r| r.text()).collect::<Vec<_>>()
    };
    let readings = settle(&mut u);
    assert!(readings.iter().all(|t| t != "----"), "the bench that is saved measures something: {readings:?}");
    let file = u.session_save("").unwrap();
    assert!(file.file_name().unwrap().to_str().unwrap().starts_with("session-") && file.extension().unwrap() == "oss");

    // another bench: other settings, another reference, a slot the session does not have
    let mut v = fresh_live(Layout::Minimal);
    v.files.dir = dir.clone();
    v.m.tscale = 8;
    v.m.ch[1].on = false;
    v.refs[0] = Some(RefWave { src: 0, color: 0, label: "left over".into(), t0: 0.0, dt: 1e-6, v: vec![0.5; 100].into(), vpd: 1.0, offset: 0.0, on: true });
    v.session_load("").unwrap();
    for _ in 0..4 { v.step(0.04); }
    assert_eq!(v.m.run, RunState::Stop, "a loaded session is stopped");
    assert_eq!((v.m.tscale, v.m.ch[1].on, v.m.layout), (tscale, true, Layout::Studio), "the settings");
    assert_eq!(v.setup_text_full(), setup, "the whole setup text");
    assert!(v.refs[0].is_none(), "a reference that is not in the session is gone");
    let r = v.refs[2].as_ref().expect("R3 of the session");
    assert_eq!((r.v.len(), r.dt, r.t0, r.vpd, r.offset, r.label.as_str(), r.on), (10_000, 1e-6, -1e-3, 0.5, 0.25, "golden", true), "the reference and its look");
    assert_eq!(&r.v[..], &known_pulses(0.0)[..], "its samples");
    let got = v.last[0].as_ref().expect("the record");
    assert_eq!((got.dt, got.t0), (record.dt, record.t0));
    for c in 0..2 { assert_eq!(got.ch[c], record.ch[c], "the samples of CH{}", c + 1); }
    // measured again from what was loaded: the same readings as on the bench that was saved
    assert_eq!(settle(&mut v), readings);

    // over SCPI names, and what is refused
    let named = dir.join("bench.oss");
    v.session_save(named.to_str().unwrap()).unwrap();
    assert!(named.is_file());
    let bytes = std::fs::read(&named).unwrap();
    assert!(crate::store::parse_session(&bytes[..bytes.len() - 7], NREF).is_err(), "a cut file");
    assert!(crate::store::parse_session(&bytes[..40], NREF).is_err(), "a file cut in its setup");
    assert!(crate::store::parse_session(b"OOSW\x01\x00\x00\x00\x00\x00\x00\x00", NREF).is_err(), "another kind of file");
    let mut newer = bytes.clone();
    newer[4] = 2;
    assert!(crate::store::parse_session(&newer, NREF).err().is_some_and(|e| e.contains("version 2")));
    std::fs::write(&named, &bytes[..bytes.len() / 2]).unwrap();
    let before = v.setup_text_full();
    assert!(v.session_load(named.to_str().unwrap()).is_err());
    assert_eq!(v.setup_text_full(), before, "a file that does not load changes nothing");
    assert!(v.refs[2].is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

/// A callout: held on a lane, "Add note here", typed; it sits on the signal at that time, follows the view, is part of
/// the setup text, is edited by a tap on its text and removed by emptying it.
#[test]
fn a_note_is_added_from_the_hold_menu_and_stays_on_its_signal() {
    let mut u = fresh(Layout::Studio);
    let lane = u.source_area(WaveId::Ch(0)).unwrap();
    // the helper holds a finger a third into the plot, opens the hold menu there and taps the row
    let x = u.plot.wins[0].x + u.plot.wins[0].w / 3;
    pick_ctx(&mut u, super::studioctx::CtxRow::NoteHere(WaveId::Ch(0)));
    assert!(u.wlan.osk.is_some() && u.callouts.len() == 1, "a note and the keyboard for it");
    for c in "runt".chars() { u.osk_type(c); }
    u.osk_ok();
    u.step(0.04);
    assert_eq!(u.callouts[0].text, "runt");
    let (tdiv, delay) = u.view();
    let want = delay as f64 + ((x - lane.x) as f64 / lane.w as f64 - 0.5) * HDIV as f64 * tdiv as f64;
    assert!((u.callouts[0].t - want).abs() < tdiv as f64 * 0.02, "at the time that was held: {} for {}", u.callouts[0].t, want);
    let ((px, py), r) = u.callout_place(0).expect("on the screen");
    assert!((px - x).abs() <= 2 && lane.contains(px, py) && lane.contains(r.x, r.y), "on its lane at the held column: {px} {py} {r:?}");
    // the view moves: the note moves with the signal, and leaves with it
    u.m.delay += tdiv;
    u.settings_changed();
    for _ in 0..3 { u.step(0.04); }
    let ((moved, _), _) = u.callout_place(0).expect("still in view");
    assert!((x - moved - lane.w / HDIV as i32).abs() <= 3, "one division to the left: {x} -> {moved}");
    u.m.delay += tdiv * 20.0;
    u.settings_changed();
    for _ in 0..3 { u.step(0.04); }
    assert!(u.callout_place(0).is_none(), "out of view, not drawn");
    u.m.delay -= tdiv * 21.0;
    u.settings_changed();
    for _ in 0..3 { u.step(0.04); }
    // part of the setup, and so of a session
    let setup = u.setup_text_full();
    assert!(setup.contains("callout1 = CH1,") && setup.trim_end().ends_with(",runt"), "{}", setup.lines().last().unwrap_or(""));
    let mut v = fresh(Layout::Studio);
    v.apply_setup_text(&setup).unwrap();
    assert_eq!(v.callouts, u.callouts);
    // a tap on the text edits it; an empty text removes the note
    let (_, r) = u.callout_place(0).unwrap();
    finger_tap(&mut u, r.x + r.w / 2, r.y + r.h / 2, 0);
    assert!(u.wlan.osk.as_ref().is_some_and(|o| o.text == "runt"), "the keyboard on the note's text");
    if let Some(o) = u.wlan.osk.as_mut() { o.text.clear(); }
    u.osk_ok();
    u.step(0.04);
    assert!(u.callouts.is_empty(), "emptied: removed");
    if let Ok(dir) = std::env::var("OOS_WALK") {
        let mut cv = Canvas::new(v.scr.w, v.scr.h);
        for _ in 0..3 { v.step(0.04); }
        v.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join("callout.png"), &crate::store::png_of(&cv));
    }
}

/// A note is added by touch outside Studio too: a finger held on the plot of Minimal or of the stock layout opens the
/// plot menu, and "Add note here" puts the note at the time that was held and opens the keyboard for its text.
#[test]
fn a_note_is_added_from_the_plot_menu_of_minimal_and_the_stock_layout() {
    for layout in [Layout::Minimal, Layout::Default] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = layout;
        u.relayout();
        for _ in 0..12 { frame(&mut u); }
        u.m.run = RunState::Stop;
        for _ in 0..4 { frame(&mut u); }
        let lane = u.source_area(WaveId::Ch(0)).unwrap();
        let (x, y) = (lane.x + lane.w / 3, lane.y + lane.h / 2);
        finger_hold(&mut u, x, y, 2, 24);
        let popup = u.workspace.popup.as_ref().unwrap_or_else(|| panic!("{layout:?}: a held finger opens the plot menu"));
        assert_eq!(popup.title, "Plot", "{layout:?}");
        let i = (0..popup.rows.len()).find(|&i| matches!(popup.rows[i].1, super::workspace::Action::Note(WaveId::Ch(0), _))).unwrap_or_else(|| panic!("{layout:?}: the menu has the note row"));
        assert_eq!(popup.rows[i].0, "Add note here");
        let r = popup.row_rect(i);
        finger_tap(&mut u, r.x + r.w / 2, r.y + r.h / 2, 2);
        assert!(u.wlan.osk.is_some() && u.callouts.len() == 1, "{layout:?}: a note and the keyboard for it");
        for c in "glitch".chars() { u.osk_type(c); }
        u.osk_ok();
        frame(&mut u);
        assert_eq!(u.callouts[0].text, "glitch", "{layout:?}");
        assert_eq!(u.callouts[0].src, WaveId::Ch(0), "{layout:?}");
        let (tdiv, delay) = u.view();
        let want = delay as f64 + ((x - lane.x) as f64 / lane.w as f64 - 0.5) * HDIV as f64 * tdiv as f64;
        assert!((u.callouts[0].t - want).abs() < tdiv as f64 * 0.02, "{layout:?}: at the time that was held: {} for {}", u.callouts[0].t, want);
        let ((px, _), _) = u.callout_place(0).unwrap_or_else(|| panic!("{layout:?}: the note is on the screen"));
        assert!((px - x).abs() <= 2, "{layout:?}: at the held column: {px} for {x}");
        // a tap on its text edits it, as in Studio
        let (_, r) = u.callout_place(0).unwrap();
        finger_tap(&mut u, r.x + r.w / 2, r.y + r.h / 2, 0);
        assert!(u.wlan.osk.as_ref().is_some_and(|o| o.text == "glitch"), "{layout:?}: the keyboard on the note's text");
        if let Ok(dir) = std::env::var("OOS_WALK") {
            u.wlan.osk = None;
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            for _ in 0..3 { u.step(0.04); }
            u.render(&mut cv);
            let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("note-{layout:?}.png")), &crate::store::png_of(&cv));
        }
    }
}

/// Pictures of the SIM sheet in Studio and in Minimal (`OOS_WALK=dir`).
#[test]
#[ignore]
fn walk_sim_sheet_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK") else { return };
    let shot = |u: &mut Ui, name: String| {
        for _ in 0..3 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join(name), &crate::store::png_of(&cv));
    };
    for (layout, tag) in [(Layout::Studio, "s"), (Layout::Minimal, "m")] {
        let mut u = fresh_live(layout);
        u.open_menu(MenuId::Sim);
        shot(&mut u, format!("simsheet-{tag}.png"));
        // every tab: live with an empty slot, with the preset added to the slot, and in SIM mode
        for state in ["live", "ref", "sim"] {
            let mut u = fresh_live(layout);
            u.open_menu(MenuId::Sim);
            u.step(0.04);
            if state != "live" {
                let g = u.sheet_geo().unwrap();
                let (x, y) = mid(g.foot[if state == "ref" { 0 } else { 1 }].1);
                tap(&mut u, x, y);
                for _ in 0..8 { u.step(0.04); u.wait_sim_generation(); }
            }
            for t in 0..u.sheet_geo().map_or(0, |g| g.tabs.len()) {
                let (x, y) = mid(u.sheet_geo().unwrap().tabs[t]);
                tap(&mut u, x, y);
                shot(&mut u, format!("simsheet-{tag}-{state}{t}.png"));
            }
        }
    }
}

/// The SIM sheet by touch, in Minimal and in Studio: Run is under every tab, the knobs of the Advanced page are rows
/// of its tabs, the switch of the slot shows and hides the reference with one tap, and no row moves under the finger
/// when SIM mode starts.
#[test]
fn the_sim_sheet_has_run_under_every_tab_and_a_show_switch_that_works() {
    fn settle(u: &mut Ui, done: impl Fn(&Ui) -> bool) {
        let end = std::time::Instant::now() + std::time::Duration::from_secs(120);
        while !done(u) && std::time::Instant::now() < end { u.step(0.04); u.wait_sim_generation(); }
        assert!(done(u), "the SIM generation came to its end: {:?}", u.toast.as_ref().map(|t| t.0.clone()));
    }
    fn row(u: &Ui, it: Item) -> (RowKind, (i32, i32)) {
        let g = u.sheet_geo().unwrap();
        let r = g.rows.iter().find(|r| r.it == it).unwrap_or_else(|| panic!("{it:?} is a row of this tab"));
        (r.kind, mid(r.ctrl))
    }
    fn foot(u: &Ui, it: Item) -> (i32, i32) {
        mid(u.sheet_geo().unwrap().foot.iter().find(|f| f.0 == it).unwrap_or_else(|| panic!("{it:?} is under this tab")).1)
    }
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh_live(layout);
        u.open_menu(MenuId::Sim);
        u.step(0.04);
        assert!(u.sheet_active(), "{layout:?}: SIM is a sheet");
        let spec = u.sheet_spec(MenuId::Sim).unwrap();
        assert_eq!(spec.tabs.iter().map(|t| t.name).collect::<Vec<_>>(), ["Signal", "Levels", "Jitter", "Spread"]);
        for (i, t) in spec.tabs.iter().enumerate() {
            assert!(t.rows.len() >= 4, "{layout:?} {}: {} rows", t.name, t.rows.len());
            assert!(t.foot.contains(&Item::SimPlay), "{layout:?} {}: Run is under every tab", t.name);
            assert_eq!(t.foot.contains(&Item::SimAddRef), i == 0, "{layout:?} {}: Add is beside the slot", t.name);
            assert_eq!(t.foot.contains(&Item::SimDefaults), i != 0, "{layout:?} {}: Defaults is under the knobs", t.name);
        }
        let rows: Vec<Item> = spec.tabs.iter().flat_map(|t| t.rows.iter().copied()).collect();
        assert!(!rows.contains(&Item::Open(MenuId::SimAdvanced)), "{layout:?}: no second page");
        for it in u.items(MenuId::SimAdvanced) {
            assert!(it == Item::SimDefaults || rows.contains(&it), "{layout:?}: {it:?} of the Advanced page is a row");
        }
        // an empty slot: the switch is off, and a tap says why
        let (kind, (sx, sy)) = row(&u, Item::SimShow);
        assert_eq!(kind, RowKind::Toggle);
        assert!(!u.sheet_on(Item::SimShow));
        tap(&mut u, sx, sy);
        assert!(u.refs[0].is_none() && u.toast.as_ref().is_some_and(|t| t.0.contains("empty")), "{layout:?}: {:?}", u.toast.as_ref().map(|t| t.0.clone()));
        // Add to R1: the reference is there and shown, and so is the switch
        let (x, y) = foot(&u, Item::SimAddRef);
        tap(&mut u, x, y);
        settle(&mut u, |u| u.refs[0].is_some());
        assert!(u.refs[0].as_ref().unwrap().on && u.sheet_on(Item::SimShow), "{layout:?}: shown, and the switch says so");
        // one tap hides it, one more shows it
        tap(&mut u, sx, sy);
        assert!(!u.refs[0].as_ref().unwrap().on && !u.sheet_on(Item::SimShow), "{layout:?}: hidden by one tap");
        tap(&mut u, sx, sy);
        assert!(u.refs[0].as_ref().unwrap().on && u.sheet_on(Item::SimShow), "{layout:?}: shown by the next");
        // live with a reference shown: the last row of Signal plays the shown references
        let before: Vec<(Item, Rect)> = u.sheet_geo().unwrap().rows.iter().map(|r| (r.it, r.row)).collect();
        assert_eq!(before.last().unwrap().0, Item::SimPlayRefs, "{layout:?}: {before:?}");
        assert_eq!(u.sheet_label(Item::SimPlayRefs), "Play reference R1");
        // Run from under the Jitter tab, with that reference shown: SIM mode starts on the signal of the knobs, not on
        // the reference, and the sheet stays
        let (x, y) = mid(u.sheet_geo().unwrap().tabs[2]);
        tap(&mut u, x, y);
        assert_eq!(row(&u, Item::SimDcd).0, RowKind::Number, "{layout:?}: duty-cycle distortion is a row of Jitter");
        let (x, y) = foot(&u, Item::SimPlay);
        tap(&mut u, x, y);
        settle(&mut u, |u| u.sim_mode() && !u.sim_generating());
        assert!(u.session_is_sim && u.sim_refs.is_empty(), "{layout:?}: the signal of the knobs plays");
        assert!(u.menu_open && u.menu == MenuId::Sim && u.sheet_active(), "{layout:?}: the sheet stays");
        // a knob changes the signal that plays: High one step up, and the record reaches higher
        let slot = u.sim_slot();
        let top = |u: &Ui| u.refs[slot].as_ref().map_or(f32::MIN, |r| r.v.iter().fold(f32::MIN, |a, &x| a.max(x)));
        let was = top(&u);
        let (x, y) = mid(u.sheet_geo().unwrap().tabs[1]);
        tap(&mut u, x, y);
        let g = u.sheet_geo().unwrap();
        let plus = Ui::value_parts(g.rows.iter().find(|r| r.it == Item::SimHigh).expect("High is a row of Levels").ctrl).2;
        let (x, y) = mid(plus);
        tap(&mut u, x, y);
        settle(&mut u, |u| !u.sim_generating() && top(u) > was + 0.03);
        assert!(u.sim_mode() && u.sheet_active(), "{layout:?}: still playing, the sheet still open");
        // the rows of Signal are where they were; Back to live took the place of Play references
        let (x, y) = mid(u.sheet_geo().unwrap().tabs[0]);
        tap(&mut u, x, y);
        let after: Vec<(Item, Rect)> = u.sheet_geo().unwrap().rows.iter().map(|r| (r.it, r.row)).collect();
        let n = before.len();
        assert_eq!(after.len(), n, "{layout:?}: {after:?}");
        assert_eq!(after[..n - 1], before[..n - 1], "{layout:?}: no row moved");
        assert_eq!(*after.last().unwrap(), (Item::SimLive, before[n - 1].1), "{layout:?}: the way back is the last row");
        let (_, (x, y)) = row(&u, Item::SimLive);
        tap(&mut u, x, y);
        for _ in 0..4 { u.step(0.04); }
        assert!(!u.sim_mode() && !u.record_mode(), "{layout:?}: live again");
        // the shown reference has its own play: the record in R1 as the acquisition, nothing generated
        assert!(u.refs[slot].as_ref().is_some_and(|r| r.on), "{layout:?}: the reference is still shown");
        if !u.menu_open { u.open_menu(MenuId::Sim); u.step(0.04); }
        let (kind, (x, y)) = row(&u, Item::SimPlayRefs);
        assert_eq!(kind, RowKind::Link);
        let held = u.refs[slot].as_ref().unwrap().v.clone();
        tap(&mut u, x, y);
        for _ in 0..4 { u.step(0.04); }
        assert!(u.sim_mode() && !u.session_is_sim && u.sim_refs == vec![slot], "{layout:?}: the reference plays: {:?}", u.sim_refs);
        assert!(std::sync::Arc::ptr_eq(&u.refs[slot].as_ref().unwrap().v, &held), "{layout:?}: and is not overwritten");
        u.return_to_live();
        assert!(!u.sim_mode() && !u.record_mode());
    }
}

/// The measurement strip of Minimal names every measurement whole at every text size: where the long name has no room
/// (several sources, the larger sizes) it takes the short one, in the band, the paged band and the column.
#[test]
fn the_measurement_strip_takes_the_short_name_where_the_long_one_is_cut() {
    use crate::gfx::Au;
    for ts in [TextSize::Normal, TextSize::Large, TextSize::Larger] {
        for (dock, channels) in [(StripDock::Below, 1), (StripDock::Below, 4), (StripDock::Right, 4)] {
            let mut u = Ui::new(Source::Demo { t: 0.0 });
            u.m.layout = Layout::Minimal;
            u.m.text_size = ts;
            u.m.strip_dock = dock;
            if channels == 4 {
                for c in 0..4 { u.m.ch[c].on = true; }
                u.m.meas.extend([(Meas::Vpp, WaveId::Ch(1)), (Meas::Freq, WaveId::Ch(1)), (Meas::Vrms, WaveId::Ch(1)), (Meas::Vpp, WaveId::Ch(2)), (Meas::Period, WaveId::Ch(2)), (Meas::Vmax, WaveId::Ch(3)), (Meas::Vmin, WaveId::Ch(3))]);
            }
            u.relayout();
            for _ in 0..12 { u.step(0.04); }
            u.toast = None;
            let what = format!("{ts:?} {dock:?} {channels} ch");
            let strip = u.strip_rect().unwrap_or_else(|| panic!("{what}: a strip"));
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            cv.au_start();
            u.render(&mut cv);
            let au = cv.au.take().unwrap();
            // every text of the strip that is the long or the short name of a measurement, and whether its ink is whole
            let (rows, keys) = (u.meas_rows(), u.studio_keys());
            let names: Vec<(String, String)> = rows.iter().zip(&keys).map(|(r, &k)| (r.name.clone(), u.studio_badge_name(k, &r.name))).collect();
            let (mut seen, mut cut) = (0, Vec::new());
            for e in &au {
                let Au::Text { s, ink: Some(ink), clip, .. } = e else { continue };
                if !strip.contains((ink.0 + ink.2) / 2, (ink.1 + ink.3) / 2) { continue; }
                // the column names its lines "CH2 Frequency" on a page of many
                let name = s.rsplit_once("CH").map_or(s.as_str(), |(_, rest)| rest.split_once(' ').map_or(s.as_str(), |(_, n)| n));
                if !names.iter().any(|(long, short)| name == long || name == short) { continue; }
                seen += 1;
                if ink.0 < clip.0 - 1 || ink.2 > clip.2 + 1 { cut.push(format!("'{s}' ink {}..{} clip {}..{}", ink.0, ink.2, clip.0, clip.2)); }
            }
            assert!(seen >= rows.len().min(6), "{what}: {seen} names of {} measurements are drawn", rows.len());
            assert!(cut.is_empty(), "{what}: names cut by their cell: {cut:#?}");
        }
    }
    // the long name stays where it has room
    let u = fresh(Layout::Minimal);
    let key = super::studio::MeasKey::One(Meas::PDuty, WaveId::Ch(0));
    assert_eq!(u.strip_name(Some(key), "Positive Duty Cycle", 400), "Positive Duty Cycle");
    assert_eq!(u.strip_name(Some(key), "Positive Duty Cycle", 40), "+Duty");
}

/// The docked event table: its title and count are the title of the view, its columns fit their content, and a
/// divider between two columns follows a finger.
#[test]
fn the_docked_event_table_has_its_count_in_the_title_and_columns_that_can_be_dragged() {
    let mut u = fresh_live(Layout::Studio);
    // the marks of a search (the events of the decoders are the Events pane, which holds its columns itself)
    u.search_on = true;
    for _ in 0..12 { u.step(0.04); }
    u.table_toggle(super::tables::TableKind::Search);
    u.relayout();
    for _ in 0..12 { u.step(0.04); }
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    let title = u.table_title();
    assert!(title.starts_with("Search ") && title.contains(" \u{b7} ") && !title.ends_with(" 0"), "{title}");
    // (the rows as the list shows them: the number of the event in the middle column)
    let (r, rows) = (u.dock_content(super::dock::VKind::Table).expect("the table is a view"), u.table_shown().expect("rows").1);
    // (the names of the columns are a line of their own; the title is in the strip of the view)
    assert!(u.head_h(r) < 30, "no title row inside the docked table");
    let before = u.table_col_x(&rows, r.w);
    assert!(before[0] < before[1] && before[1] < before[2] && before[2] < r.w - 30, "{before:?}");
    // a finger on the second divider, moved 60 pixels to the right
    let (x, y) = (r.x + before[2] - 8, r.y + 40);
    assert_eq!(u.table_col_grip(x, y), Some(2));
    u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
    for k in 1..=6 { u.handle(Ev::Touch { id: 0, phase: Phase::Move, x: x + k * 10, y }); u.step(0.04); }
    u.handle(Ev::Touch { id: 0, phase: Phase::Up, x: x + 60, y });
    u.step(0.04);
    let after = u.table_col_x(&rows, r.w);
    // (the fitted widths were those of every row, the fixed ones are those of the rows on the screen: a few pixels)
    assert!((after[1] - before[1]).abs() <= 6 && after[2] == before[2] + 60, "the text column moved with the finger, the middle one stayed: {before:?} {after:?}");
    assert_eq!(u.table.top, 0, "dragging a divider does not scroll the rows");
    if let Ok(dir) = std::env::var("OOS_WALK") {
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(&dir).join("table.png"), &crate::store::png_of(&cv));
    }
    // the dragged columns are part of the setup (and so of a session): recalled, they are where they were put
    let [b, c] = u.table_cols.expect("dragged");
    let setup = u.setup_text_full();
    assert!(setup.contains(&format!("tcols={b}/{c}")), "{}", setup.lines().find(|l| l.starts_with("studio_state")).unwrap_or("no studio_state"));
    let mut v = fresh_live(Layout::Studio);
    assert_eq!(v.table_cols, None, "a fresh table fits its columns to the content");
    v.apply_setup_text(&setup).unwrap();
    assert_eq!(v.table_cols, Some([b, c]));
    // a setup from before a divider was dragged brings the fitted columns back
    u.table_cols = None;
    let plain = u.setup_text_full();
    assert!(!plain.contains("tcols="));
    v.apply_setup_text(&plain).unwrap();
    assert_eq!(v.table_cols, None);
}

/// Remote services and SSH settings are sheets in Minimal and Studio, System > Connectivity says where the scope is
/// reached and leads to its three pages, and a link to another page stands after the rows of its own page.
#[test]
fn remote_services_are_sheets_and_connectivity_leads_to_them() {
    fn row(u: &Ui, it: Item) -> (RowKind, (i32, i32)) {
        let g = u.sheet_geo().unwrap();
        let r = g.rows.iter().find(|r| r.it == it).unwrap_or_else(|| panic!("{it:?} is a row of this tab"));
        (r.kind, mid(r.ctrl))
    }
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        u.open_menu(MenuId::Utility);
        u.step(0.04);
        let spec = u.sheet_spec(MenuId::Utility).unwrap();
        assert_eq!(spec.tabs[0].name, "Connectivity");
        assert_eq!(spec.tabs[0].rows, [Item::NetEth, Item::NetWlan, Item::NetWeb, Item::TimeNow, Item::Open(MenuId::Network), Item::Open(MenuId::Remote), Item::Open(MenuId::Time)], "{layout:?}");
        let g = u.sheet_geo().unwrap();
        assert!(g.rows[..4].iter().all(|r| r.kind == RowKind::Info) && g.rows[4..].iter().all(|r| r.kind == RowKind::Link), "{layout:?}: four lines of state, three pages");
        // Remote services opens from here, as a sheet with Back
        let (_, (x, y)) = row(&u, Item::Open(MenuId::Remote));
        tap(&mut u, x, y);
        assert!(u.menu == MenuId::Remote && u.sheet_active() && u.sheet_chrome().1.is_some(), "{layout:?}: Remote services is a sheet with Back");
        let spec = u.sheet_spec(MenuId::Remote).unwrap();
        assert_eq!(spec.tabs.iter().map(|t| t.name).collect::<Vec<_>>(), ["Services", "Access"]);
        let on_sheet: Vec<Item> = spec.tabs.iter().flat_map(|t| t.rows.iter().copied()).collect();
        for it in u.items(MenuId::Remote) { assert!(on_sheet.contains(&it), "{layout:?}: {it:?} is on the sheet"); }
        // a switch acts on one tap
        let (kind, (x, y)) = row(&u, Item::RemVxi);
        assert!(kind == RowKind::Toggle && u.sys.remote_vxi11);
        tap(&mut u, x - 40, y);
        assert!(!u.sys.remote_vxi11, "{layout:?}: VXI-11 off");
        tap(&mut u, x + 40, y);
        assert!(u.sys.remote_vxi11, "{layout:?}: and on again");
        // the addresses say whose they are
        assert_eq!(u.sheet_label(Item::RemScpiInfo), "SCPI port");
        assert_eq!(u.sheet_label(Item::NetWeb), "Web address");
        // SSH settings: a sheet too, its two key actions pinned
        let (kind, (x, y)) = row(&u, Item::Open(MenuId::Ssh));
        assert_eq!(kind, RowKind::Link);
        tap(&mut u, x, y);
        assert!(u.menu == MenuId::Ssh && u.sheet_active(), "{layout:?}: SSH settings is a sheet");
        let spec = u.sheet_spec(MenuId::Ssh).unwrap();
        assert_eq!(spec.tabs[0].foot, [Item::SshImport, Item::SshRemove]);
        assert_eq!(row(&u, Item::SshState).0, RowKind::Info);
        assert_eq!(row(&u, Item::SshPassword).0, RowKind::Toggle);
        // Back, and Back again
        let b = u.sheet_chrome().1.expect("Back");
        tap(&mut u, b.x + b.w / 2, b.y + b.h / 2);
        assert_eq!(u.menu, MenuId::Remote, "{layout:?}");
        let b = u.sheet_chrome().1.expect("Back");
        tap(&mut u, b.x + b.w / 2, b.y + b.h / 2);
        assert_eq!(u.menu, MenuId::Utility, "{layout:?}");
        // every sheet: the links to other pages come last (the lines of the logic analyser keep theirs in place)
        let mut list = menus(&u);
        list.extend([MenuId::Network, MenuId::Remote, MenuId::Ssh, MenuId::Time, MenuId::SelfCal, MenuId::Dvm, MenuId::Zone, MenuId::Sim]);
        for menu in list {
            let mut u = fresh(layout);
            u.open_menu(menu);
            let Some(spec) = u.sheet_spec(menu) else { continue };
            if matches!(menu, MenuId::La | MenuId::Main) { continue; }
            for t in &spec.tabs {
                let first_link = t.rows.iter().position(|it| matches!(it, Item::Open(_))).unwrap_or(t.rows.len());
                assert!(t.rows[first_link..].iter().all(|it| matches!(it, Item::Open(_))), "{layout:?} {menu:?} {}: {:?}", t.name, t.rows);
            }
        }
    }
}

/// Pictures of the sheets a second look went over (`OOS_WALK=dir`): Remote services, SSH settings, and the tabs where
/// the links to other pages moved to the end.
#[test]
#[ignore]
fn walk_second_look_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK") else { return };
    for (layout, tag) in [(Layout::Studio, "s"), (Layout::Minimal, "m")] {
        for (menu, name) in [(MenuId::Remote, "remote"), (MenuId::Ssh, "ssh"), (MenuId::Measure, "measure"), (MenuId::Display, "display"), (MenuId::Network, "network"), (MenuId::Utility, "system")] {
            let mut u = fresh_live(layout);
            u.open_menu(menu);
            u.step(0.04);
            let tabs = u.sheet_geo().map_or(0, |g| g.tabs.len());
            for t in 0..tabs.max(1) {
                if tabs > 0 { let (x, y) = mid(u.sheet_geo().unwrap().tabs[t]); tap(&mut u, x, y); }
                for _ in 0..3 { u.step(0.04); }
                let mut cv = Canvas::new(u.scr.w, u.scr.h);
                u.render(&mut cv);
                let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("look-{tag}-{name}{t}.png")), &crate::store::png_of(&cv));
            }
        }
    }
}

/// The page of a channel pair has its two sources under every tab, and no sheet opens a tab that holds nothing
/// (Position while the cursors are off, Scale while math has no function); the rows of Math stay where they are when a
/// function is chosen.
#[test]
fn the_pair_sources_are_under_every_tab_and_no_empty_tab_opens() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        u.m.ch[1].on = true;
        u.open_menu(MenuId::MeasPair);
        u.step(0.04);
        let n = u.sheet_spec(MenuId::MeasPair).expect("a sheet").tabs.len();
        assert_eq!(n, 4, "{layout:?}");
        for t in 0..n {
            let (x, y) = mid(u.sheet_geo().unwrap().tabs[t]);
            tap(&mut u, x, y);
            let g = u.sheet_geo().unwrap();
            assert_eq!(g.foot.iter().map(|f| f.0).collect::<Vec<_>>(), [Item::MeasSrc, Item::MeasSrcB], "{layout:?} tab {t}: A and B");
        }
        // B opens its list from under the last tab, and the source chosen there is the one of the pair
        let (x, y) = mid(u.sheet_geo().unwrap().foot[1].1);
        tap(&mut u, x, y);
        assert!(u.menu == MenuId::Pick && u.pick.is_some_and(|p| p.it == Item::MeasSrcB), "{layout:?}: the list of source B");
        u.close_menu();
        // a tab that holds nothing is never the one shown, in any sheet, as the scope starts
        let mut list = menus(&u);
        list.extend([MenuId::Network, MenuId::Remote, MenuId::Ssh, MenuId::Time, MenuId::SelfCal, MenuId::Dvm, MenuId::Zone, MenuId::Sim, MenuId::MeasPair, MenuId::MeasStat]);
        for menu in list {
            let mut u = fresh(layout);
            u.open_menu(menu);
            let Some(spec) = u.sheet_spec(menu) else { continue };
            if menu == MenuId::Main { continue; }
            for t in 0..spec.tabs.len() {
                if let Some(r) = u.sheet_geo().unwrap().tabs.get(t) { let (x, y) = mid(*r); tap(&mut u, x, y); }
                let g = u.sheet_geo().unwrap();
                assert!(!g.rows.is_empty() || !g.foot.is_empty(), "{layout:?} {menu:?}: a tap on the tab {} shows an empty page", spec.tabs[t].name);
            }
        }
    }
    // Studio, the cursors off: Position holds nothing. It keeps its place in the strip, and a tap on it does not open it
    let mut u = fresh(Layout::Studio);
    u.open_menu(MenuId::Cursor);
    u.step(0.04);
    assert_eq!(u.m.cursor, CursorMode::Off);
    let spec = u.sheet_spec(MenuId::Cursor).unwrap();
    assert_eq!(spec.tabs.iter().map(|t| (t.name, t.holds())).collect::<Vec<_>>(), [("Cursors", true), ("Position", false)]);
    let g = u.sheet_geo().unwrap();
    let shown: Vec<Item> = g.rows.iter().map(|r| r.it).collect();
    let (x, y) = mid(g.tabs[1]);
    tap(&mut u, x, y);
    assert_eq!(u.sheet_geo().unwrap().rows.iter().map(|r| r.it).collect::<Vec<_>>(), shown, "the page of Cursors stays");
    // with a cursor type it opens
    u.m.cursor = CursorMode::Time;
    assert!(u.sheet_spec(MenuId::Cursor).unwrap().tabs[1].holds());
    tap(&mut u, x, y);
    assert_ne!(u.sheet_geo().unwrap().rows.iter().map(|r| r.it).collect::<Vec<_>>(), shown, "Position is open");
    // and when the cursors go off while it is open, the sheet shows the page that holds something
    u.m.cursor = CursorMode::Off;
    assert_eq!(u.sheet_geo().unwrap().rows.iter().map(|r| r.it).collect::<Vec<_>>(), shown, "back on Cursors");
    // math without a function: Scale holds nothing
    let mut u = fresh(Layout::Studio);
    u.open_menu(MenuId::Math);
    assert_eq!(u.m.math, MathOp::Off);
    assert_eq!(u.sheet_spec(MenuId::Math).unwrap().tabs.iter().map(|t| (t.name, t.holds())).collect::<Vec<_>>(), [("Function", true), ("Scale", false)]);
    // a function chosen: the three rows stay where they were, and Turn off is pinned under them
    u.step(0.04);
    let before: Vec<(Item, Rect)> = u.sheet_geo().unwrap().rows.iter().map(|r| (r.it, r.row)).collect();
    assert_eq!(before.iter().map(|r| r.0).collect::<Vec<_>>(), [Item::MathSel, Item::MathFam, Item::MathOpI]);
    u.select_option(Item::MathOpI, "A+B");
    u.step(0.04);
    assert!(u.m.math != MathOp::Off, "a function is on");
    let g = u.sheet_geo().unwrap();
    let after: Vec<(Item, Rect)> = g.rows.iter().map(|r| (r.it, r.row)).collect();
    assert_eq!(after[..3], before[..], "no row moved");
    assert_eq!(g.foot.iter().map(|f| f.0).collect::<Vec<_>>(), [Item::MathOff], "Turn off is at the foot");
    let (x, y) = mid(g.foot[0].1);
    tap(&mut u, x, y);
    assert_eq!(u.m.math, MathOp::Off, "and a tap on it turns math off");
}

/// Pictures of every tab of the channel, trigger and measure sheets (`OOS_WALK=dir`), with the signals of the demo.
#[test]
#[ignore]
fn walk_channel_trigger_measure_pictures() {
    let Ok(dir) = std::env::var("OOS_WALK") else { return };
    for (layout, tag) in [(Layout::Studio, "s"), (Layout::Minimal, "m")] {
        for (menu, name) in [(MenuId::Channel(0), "ch"), (MenuId::Trigger, "trig"), (MenuId::Measure, "meas"), (MenuId::MeasPair, "pair"), (MenuId::MeasStat, "stat"), (MenuId::Dvm, "dvm"), (MenuId::Horizontal, "hor"), (MenuId::Cursor, "cursor")] {
            let mut u = fresh_live(layout);
            u.m.ch[1].on = true;
            u.open_menu(menu);
            u.step(0.04);
            let tabs = u.sheet_geo().map_or(0, |g| g.tabs.len());
            for t in 0..tabs.max(1) {
                if tabs > 0 { let (x, y) = mid(u.sheet_geo().unwrap().tabs[t]); tap(&mut u, x, y); }
                for _ in 0..3 { u.step(0.04); }
                let mut cv = Canvas::new(u.scr.w, u.scr.h);
                u.render(&mut cv);
                let _ = crate::store::write(&std::path::Path::new(&dir).join(format!("ctm-{tag}-{name}{t}.png")), &crate::store::png_of(&cv));
            }
        }
    }
}

/// Four decoders run side by side. Each of D1 to D4 is chosen and switched on by touch, with its own protocol and its
/// own channel; the sheet marks the decoders that are on while another one is edited, the switch says whose it is, and
/// the plot names a bus for each of the four.
#[test]
fn four_decoders_of_four_protocols_run_side_by_side() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        for c in 0..4 { u.m.ch[c].on = true; }
        u.open_menu(MenuId::Decode);
        u.step(0.04);
        let ctrl = |u: &Ui, it: Item| u.sheet_geo().unwrap().rows.iter().find(|r| r.it == it).map(|r| r.ctrl).unwrap_or_else(|| panic!("{layout:?}: {it:?}"));
        // the place of decoder k in the row of the four, and the colour of its upper edge
        let chip = |u: &Ui, k: usize| { let c = ctrl(u, Item::DecSel); let w = (c.w - 12) / 4; Rect::new(c.x + k as i32 * (w + 4), c.y, w, c.h) };
        let edge = |u: &mut Ui, k: usize| {
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            u.render(&mut cv);
            let r = chip(u, k);
            cv.px[(r.y * cv.w + r.x + r.w / 2) as usize]
        };
        assert_eq!(u.sheet_label(Item::DecSel), "Decoders", "{layout:?}");
        assert_eq!(edge(&mut u, 2), edge(&mut u, 3), "{layout:?}: none is on, none is marked");
        let protos = [Proto::UART, Proto::I2C, Proto::SPI, Proto::CAN];
        for k in 0..4 {
            let (x, y) = mid(chip(&u, k));
            tap(&mut u, x, y);
            assert_eq!(u.m.dec_sel, k, "{layout:?}: a tap on D{}", k + 1);
            assert_eq!(u.sheet_label(Item::DecOn), format!("Decoder B{}", k + 1), "{layout:?}");
            assert!(!u.m.dec_on, "{layout:?}: D{} starts off", k + 1);
            let (x, y) = mid(ctrl(&u, Item::DecOn));
            tap(&mut u, x, y);
            assert!(u.m.dec_on, "{layout:?}: the switch of D{}", k + 1);
            u.m.dec_proto = protos[k];
            u.m.dec_src = k;
            u.step(0.04);
        }
        assert!((0..4).all(|k| u.m.dec_is_on(k)), "{layout:?}: all four are on");
        assert_eq!((0..4).map(|k| (u.m.dec_slot(k).proto, u.m.dec_slot(k).src)).collect::<Vec<_>>(),
            protos.iter().copied().zip(0..4).collect::<Vec<_>>(), "{layout:?}: each kept its protocol and its channel");
        // D4 is the one edited: D1 to D3 are marked as on. D2 switched off loses its mark, the others keep theirs
        assert_eq!(edge(&mut u, 0), edge(&mut u, 2), "{layout:?}");
        let marked = edge(&mut u, 1);
        let (x, y) = mid(chip(&u, 1));
        tap(&mut u, x, y);
        let (x, y) = mid(ctrl(&u, Item::DecOn));
        tap(&mut u, x - 40, y);
        let (x, y) = mid(chip(&u, 3));
        tap(&mut u, x, y);
        assert!(!u.m.dec_is_on(1) && u.m.dec_is_on(0) && u.m.dec_is_on(2) && u.m.dec_is_on(3), "{layout:?}");
        assert_ne!(edge(&mut u, 1), marked, "{layout:?}: D2 is off and not marked");
        assert_eq!(edge(&mut u, 0), marked, "{layout:?}: D1 is still marked");
        // the plot names a bus for each decoder that is on
        let (x, y) = mid(chip(&u, 1));
        tap(&mut u, x, y);
        let (x, y) = mid(ctrl(&u, Item::DecOn));
        tap(&mut u, x, y);
        u.close_menu();
        for _ in 0..30 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        cv.au_start();
        u.render(&mut cv);
        let texts: Vec<String> = cv.au.take().unwrap().into_iter().filter_map(|e| match e { Au::Text { s, .. } => Some(s), _ => None }).collect();
        for (k, name) in ["UART", "I2C", "SPI", "CAN"].iter().enumerate() {
            // the bus first, its number and protocol; Studio writes its input after them ("B1 UART CH1"), in the
            // other layout the colour of the name says the source
            let want = if layout == Layout::Studio { format!("B{} {} CH{}", k + 1, name, k + 1) } else { format!("B{} {}", k + 1, name) };
            assert!(texts.iter().any(|t| *t == want), "{layout:?}: no bus named {want:?} on the plot");
        }
    }
}

/// Display > Screen in Minimal and Studio has no row that does nothing there: Bottom bar chooses between the bars of
/// the Default layout and stays in the menu of the Default layout.
#[test]
fn the_display_sheet_has_no_bottom_bar() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        u.open_menu(MenuId::Display);
        u.step(0.04);
        let spec = u.sheet_spec(MenuId::Display).expect("a sheet");
        let rows: Vec<Item> = spec.tabs.iter().flat_map(|t| t.rows.iter().chain(&t.foot).copied()).collect();
        assert!(!rows.contains(&Item::BottomBar), "{layout:?}: {rows:?}");
        let screen = spec.tabs.iter().find(|t| t.name == "Screen").expect("the Screen tab");
        assert_eq!(screen.rows, [Item::Hdmi, Item::HdmiView, Item::Backlight, Item::TextSize], "{layout:?}");
        // what Bottom bar chooses is the same either way: the bars are the compact ones, and nothing moves
        u.close_menu();
        let places = |u: &Ui| (u.m.compact(), u.plot_rect(), u.frame().bottom, u.studio_bottom().ch, u.source_pills());
        let cards = places(&u);
        u.adjust(Item::BottomBar, 1);
        for _ in 0..4 { u.step(0.04); }
        assert!(u.m.bar_compact && cards.0, "{layout:?}");
        assert_eq!(places(&u), cards, "{layout:?}: Bottom bar moves something");
    }
    let u = fresh(Layout::Default);
    let items = u.items(MenuId::Display);
    assert!(items.contains(&Item::BottomBar), "the Default layout keeps it");
}

/// A channel pill of Studio's bottom bar is as wide as its badge, the widest scale and its chevron need: with one or
/// two channels on it was 170 px, with empty room before the chevron. The scale is drawn whole, the pills of all
/// channels are equally wide whatever their scales, and many sources still share the bar. The pill of a channel that
/// is off is a button to switch it on: never under 60 px (it was 44), and with room to spare it says "Off".
#[test]
fn a_channel_pill_is_as_wide_as_its_scale_needs() {
    // Studio's bar; Minimal has the small pills of its own footer
    for layout in [Layout::Studio] {
        for ts in [TextSize::Normal, TextSize::Large, TextSize::Larger] {
            let mut u = fresh(layout);
            u.m.text_size = ts;
            u.relayout();
            for c in 0..4 { u.m.ch[c].on = c < 2; }
            u.m.ch[0].vscale = 0;
            u.step(0.04);
            let what = format!("{layout:?} {ts:?}");
            let b = u.studio_bottom();
            let f = u.fonts();
            let scales: Vec<String> = (0..2).map(|c| format!("{}/div", si_short(u.m.ch[c].volts_per_div() as f64, u.m.ch[c].unit.name()))).collect();
            assert_ne!(scales[0], scales[1], "{what}: two scales");
            assert_eq!(b.ch[0].w, b.ch[1].w, "{what}: the pills are equally wide");
            // every pill of the bar is taller than a measurement badge of the row above it, and the bar ends at the screen
            let badge = u.studio_badges().first().map(|b| b.1.h).expect("a badge");
            let bar = u.frame().bottom;
            let chips: Vec<Rect> = b.ch.iter().copied().filter(|r| r.w > 0).chain([b.add, b.time, b.trig]).chain(b.la).chain(b.gen).collect();
            assert!(chips.iter().all(|r| r.h == 40 && r.h > badge && r.y >= bar.y + 3 && r.bottom() <= bar.bottom() - 3), "{what}: a badge of {badge} px, {chips:?}");
            assert_eq!(bar.bottom(), u.scr.h, "{what}");
            // the text begins 58 px in; what is left after the widest scale is the chevron's room and no more. The
            // largest text needs all of the 170 px a pill ever had
            let widest = f.mb.width("500 mV/div");
            let need = (58 + widest + 28).min(170);
            assert!(b.ch[0].w <= need && b.ch[0].w >= need - 8, "{what}: {} px for a scale of {widest} px", b.ch[0].w);
            if ts == TextSize::Normal { assert!(b.ch[0].w == need && need <= 160, "{what}: {} px", b.ch[0].w); }
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            cv.au_start();
            u.render(&mut cv);
            let au = cv.au.take().unwrap();
            for (c, scale) in scales.iter().enumerate() {
                let r = b.ch[c];
                let whole = au.iter().any(|e| matches!(e, Au::Text { s, ink: Some(ink), clip, .. }
                    if s == scale && r.contains(ink.0, (ink.1 + ink.3) / 2) && ink.0 >= clip.0 && ink.2 <= clip.2 && ink.2 <= r.right() - 16));
                assert!(whole, "{what}: the scale {scale} of CH{} is cut or missing", c + 1);
            }
            // the two channels that are off: a pill each, equally wide and inside the bar
            let off: Vec<Rect> = (2..4).map(|c| b.ch[c]).collect();
            assert!(off.iter().all(|r| r.w >= 60 && r.w == off[0].w) && b.add.right() <= b.time.x, "{what}: {off:?}");
            // one channel on: the three others have the room for their badge and "Off", and a tap switches one on
            u.m.ch[1].on = false;
            u.step(0.04);
            let b = u.studio_bottom();
            if ts == TextSize::Normal {
                assert!((1..4).all(|c| b.ch[c].w == 96), "{what}: {:?}", &b.ch[1..4]);
                let mut cv = Canvas::new(u.scr.w, u.scr.h);
                cv.au_start();
                u.render(&mut cv);
                let au = cv.au.take().unwrap();
                for c in 1..4 {
                    let says = au.iter().any(|e| matches!(e, Au::Text { s, ink: Some(ink), .. } if s == "Off" && b.ch[c].contains(ink.0, (ink.1 + ink.3) / 2) && ink.2 <= b.ch[c].right()));
                    assert!(says, "{what}: the pill of CH{} does not say Off", c + 1);
                }
            }
            let (x, y) = mid(b.ch[2]);
            tap(&mut u, x, y);
            assert!(u.m.ch[2].on, "{what}: a tap on the pill of CH3 switches it on");
            // every channel on: the pills share the bar as before and stay inside it
            for c in 0..4 { u.m.ch[c].on = true; }
            u.step(0.04);
            let b = u.studio_bottom();
            assert!((0..4).all(|c| b.ch[c].w > 0 && b.ch[c].w <= b.ch[0].w) && b.add.right() <= b.time.x, "{what}: four channels");
        }
    }
}

#[test]
fn a_session_saved_in_the_offline_view_comes_back_in_it() {
    let dir = std::env::temp_dir().join(format!("oos-walk-offline-session-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut u = fresh_live(Layout::Minimal);
    u.files.dir = dir.clone();
    u.set_offline(true);
    // a capture long enough for the zoom window to open, loaded as a file is
    let n = 20_000usize;
    let header = format!(r#"{{"sampleType":"float32","nPoints":{n},"xincr":1e-6,"xzero":-0.01,"channel":"CH1"}}"#);
    let mut file = b"PWWF0001".to_vec();
    file.extend_from_slice(&(header.len() as u32).to_le_bytes());
    file.extend_from_slice(header.as_bytes());
    for i in 0..n {
        file.extend_from_slice(&(if (i / 500) % 2 == 0 { 0.0f32 } else { 1.0 }).to_le_bytes());
    }
    u.load_waveform_auto(&file, "cap.pwwf").unwrap();
    for _ in 0..4 { u.step(0.04); }
    assert!(u.offline.on && u.offline.slot == 0 && u.m.zoom, "the capture fills the screen, zoom window open");
    // the user moves the zoom window: that view is what the session keeps
    u.m.zdelay += 0.002;
    for _ in 0..2 { u.step(0.04); }
    let view = (u.m.tscale, u.m.zoom, u.m.zscale);
    let (tfine, delay, zdelay) = (u.m.tfine, u.m.delay, u.m.zdelay);
    let named = dir.join("offline.oss");
    u.session_save(named.to_str().unwrap()).unwrap();
    let bytes = std::fs::read(&named).unwrap();
    assert_eq!(crate::store::parse_session(&bytes, NREF).unwrap().offline_ref, Some(0));
    // the same file without its end is a session too: that is what a build from before the view was kept reads
    assert_eq!(crate::store::parse_session(&bytes[..bytes.len() - 12], NREF).unwrap().offline_ref, None);

    // another start, live, with its own channels on
    let mut v = fresh_live(Layout::Minimal);
    v.files.dir = dir.clone();
    assert!(!v.offline.on);
    v.session_load(named.to_str().unwrap()).unwrap();
    for _ in 0..4 { v.step(0.04); }
    assert!(v.offline.on && v.offline.slot == 0 && v.record_mode(), "the offline view, on the slot that was shown");
    assert!(v.m.ch.iter().all(|c| !c.on), "the analog channels stay off");
    assert_eq!((v.m.tscale, v.m.zoom, v.m.zscale), view, "the time base and the zoom as saved");
    assert!((v.m.tfine - tfine).abs() < 1e-6 && (v.m.delay - delay).abs() < 1e-9 && (v.m.zdelay - zdelay).abs() < 1e-7, "{} {} {}", v.m.tfine, v.m.delay, v.m.zdelay);
    assert_eq!(v.refs[0].as_ref().unwrap().v.len(), n);

    // a session saved with the live channels says nothing of a view and loads as before
    let mut live = fresh_live(Layout::Minimal);
    live.files.dir = dir.clone();
    live.refs[1] = Some(RefWave { src: 0, color: 1, label: "kept".into(), t0: 0.0, dt: 1e-6, v: vec![0.25; 200].into(), vpd: 1.0, offset: 0.0, on: true });
    let plain = dir.join("live.oss");
    live.session_save(plain.to_str().unwrap()).unwrap();
    let plain_bytes = std::fs::read(&plain).unwrap();
    assert_eq!(crate::store::parse_session(&plain_bytes, NREF).unwrap().offline_ref, None);
    v.session_load(plain.to_str().unwrap()).unwrap();
    assert!(!v.offline.on, "a live session leaves the offline view");
    // a view that names a slot the file does not hold is not a view
    let mut odd = plain_bytes.clone();
    odd.extend_from_slice(&crate::store::session_view(7));
    assert_eq!(crate::store::parse_session(&odd, NREF).unwrap().offline_ref, None);
    let mut held = plain_bytes;
    held.extend_from_slice(&crate::store::session_view(1));
    assert_eq!(crate::store::parse_session(&held, NREF).unwrap().offline_ref, Some(1));
    let _ = std::fs::remove_dir_all(&dir);
}

/// HDMI Layout beside Studio: the monitor's pass is 1920 wide, where badges that fill two lines on the panel fill
/// one. What that pass counts does not stay behind: the panel's badge row and its plot are the same after it, frame
/// after frame. The count of the monitor's pass stayed, and the next step laid the panel out with one line: the
/// plot changed its height from frame to frame and its traces were cleared each time.
#[test]
fn the_monitor_s_pass_leaves_the_panel_s_badge_row_and_plot_as_they_are() {
    let mut u = fresh(Layout::Studio);
    u.m.ch[1].on = false;
    u.m.hdmi = crate::model::HdmiMode::Layout;
    let lines = |u: &Ui| {
        let mut ys: Vec<i32> = u.studio_badges().iter().map(|b| b.1.y).collect();
        ys.sort();
        ys.dedup();
        ys.len()
    };
    // measurements until the panel's row has two lines
    for k in oos_dsp::measure::ALL {
        if lines(&u) >= 2 { break; }
        if !u.m.meas.iter().any(|m| m.0 == k) { u.m.meas.push((k, WaveId::Ch(0))); }
        for _ in 0..2 { frame(&mut u); }
    }
    assert_eq!(lines(&u), 2, "{} measurements", u.m.meas.len());
    let (row, plot) = (u.studio_row(), u.plot_rect());
    let mut monitor = Canvas::new(1920, 1080);
    for round in 0..4 {
        u.render_view(&mut monitor);
        assert_eq!((u.studio_row(), u.plot_rect()), (row, plot), "after the monitor's pass, round {round}");
        u.step(0.033);
        assert_eq!((u.studio_row(), u.plot_rect()), (row, plot), "after the step behind it, round {round}");
        u.render(&mut Canvas::new(1024, 600));
        assert_eq!((u.studio_row(), u.plot_rect(), lines(&u)), (row, plot, 2), "after the panel's pass, round {round}");
    }
}

/// Studio's badge row takes a second line before a measurement goes behind "+N", in both badge styles. The row grows
/// upwards and the plot gives the room; the buttons at its right stay where they are; a badge of the upper line takes a
/// tap. With more than two lines hold, "+N" is back.
#[test]
fn the_badge_row_wraps_onto_a_second_line() {
    for classic in [false, true] {
        let what = if classic { "classic" } else { "badges" };
        let mut u = fresh(Layout::Studio);
        u.m.ch[1].on = false;
        u.studio_set_classic(classic);
        for _ in 0..2 { frame(&mut u); }
        let (one, chip) = (u.studio_row(), u.studio_list_chip());
        assert!(u.studio_badges_more().is_none() && !u.studio_badges().is_empty(), "{what}: the measurements of the start fit one line");
        let lines = |u: &Ui| { let mut ys: Vec<i32> = u.studio_badges().iter().map(|b| b.1.y).collect(); ys.sort(); ys.dedup(); ys.len() };
        assert_eq!(lines(&u), 1, "{what}");
        for k in oos_dsp::measure::ALL { if u.m.meas.len() >= 14 { break; } if !u.m.meas.iter().any(|m| m.0 == k) { u.m.meas.push((k, WaveId::Ch(0))); } }
        for _ in 0..2 { frame(&mut u); }
        let (row, badges) = (u.studio_row(), u.studio_badges());
        assert_eq!((badges.len(), u.studio_badges_more().map(|m| m.1)), (14, None), "{what}: fourteen measurements, none behind +N");
        assert_eq!(lines(&u), 2, "{what}");
        assert!(row.h > one.h && row.bottom() == one.bottom() && u.plot_rect().bottom() <= row.y, "{what}: {one:?} became {row:?}");
        let limit = u.studio_row_limit();
        for (i, (_, r)) in badges.iter().enumerate() {
            // the chip at the end of the row is on one line: the badges of that line end before it, the others
            // run to the end of the row
            let (at, end) = (u.studio_list_chip(), row.right() - 4);
            let beside = r.y < at.bottom() && at.y < r.bottom();
            assert!(r.y >= row.y && r.bottom() <= row.bottom() && r.x >= row.x && r.right() <= if beside { limit } else { end }, "{what}: badge {i} at {r:?} in {row:?}");
            for (_, o) in &badges[i + 1..] { assert!(r.right() <= o.x || o.right() <= r.x || r.bottom() <= o.y || o.bottom() <= r.y, "{what}: {r:?} over {o:?}"); }
        }
        if !classic { assert_eq!(u.studio_list_chip(), chip, "{what}: the button at the right stays on the line above the bar"); }
        // the upper line is live
        let (key, r) = *badges.iter().min_by_key(|b| b.1.y).unwrap();
        let (x, y) = mid(r);
        tap(&mut u, x, y);
        assert!(u.studio.mark == Some(key) || u.studio.popup == Some(key), "{what}: a tap on a badge of the upper line");
        // more than the lines hold: the rest is behind +N, and the row is no higher
        for k in oos_dsp::measure::ALL { if !u.m.meas.iter().any(|m| m.0 == k) { u.m.meas.push((k, WaveId::Ch(0))); } }
        for _ in 0..2 { frame(&mut u); }
        let (n, more) = (u.studio_badges().len(), u.studio_badges_more().expect("+N"));
        assert_eq!(n + more.1, u.m.meas.len(), "{what}: every measurement is shown or counted");
        assert!(n >= 10, "{what}: {n} badges on the two lines");
        assert!(u.studio_row().h <= row.h + 24 && u.studio_row().bottom() == one.bottom(), "{what}: {:?}", u.studio_row());
        // "+N" is said by the chip at the end of the row
        assert!(more.0 == u.studio_list_chip() && more.0.y >= u.studio_row().y && more.0.right() <= u.studio_row().right(), "{what}: +N at {:?}", more.0);
        // back to a few: one line again
        u.m.meas.truncate(4);
        for _ in 0..2 { frame(&mut u); }
        assert_eq!((lines(&u), u.studio_row()), (1, one), "{what}: four measurements");
    }
}

/// Rows and footers where they are used, in Minimal and Studio: Clear beside Persistence and without a second tap,
/// Sweep with the trigger's type wherever the tab has the room (and a sheet for every trigger type), a way to a new
/// measurement at the foot of the Measure sheet, Clear for the one reference slot beside Store, and Clear all with
/// the files.
#[test]
fn clear_sweep_and_the_footers_are_where_they_are_used() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let u = fresh(layout);
        let tab = |menu: MenuId, name: &str| {
            let spec = u.sheet_spec(menu).unwrap_or_else(|| panic!("{layout:?}: {menu:?} has a sheet"));
            spec.tabs.into_iter().find(|t| t.name == name).unwrap_or_else(|| panic!("{layout:?}: {menu:?} has the tab {name}"))
        };
        let t = tab(MenuId::Display, "Waveform");
        assert!(t.rows.contains(&Item::Persist) && t.foot == [Item::ClearDisp], "{layout:?}: {:?}", t.foot);
        assert!(tab(MenuId::Display, "Screen").foot.is_empty(), "{layout:?}");
        assert_eq!(tab(MenuId::Measure, "Settings").foot, [Item::Open(MenuId::MeasAdd), Item::MeasClear], "{layout:?}");
        assert_eq!(tab(MenuId::Ref, "Reference").foot, [Item::RefSave, Item::RefClearOne], "{layout:?}");
        assert_eq!(tab(MenuId::Ref, "Scale and files").foot, [Item::RefClear], "{layout:?}");
        // every trigger type keeps its sheet; Sweep is on the first tab unless that tab is full without it
        for t in crate::model::TRIG_TYPES {
            let mut u = fresh(layout);
            u.m.trig_type = t;
            u.step(0.04);
            let spec = u.sheet_spec(MenuId::Trigger).unwrap_or_else(|| panic!("{layout:?}: no sheet for the {t:?} trigger"));
            let (first, second) = (&spec.tabs[0].rows, &spec.tabs[1].rows);
            assert!(first.contains(&Item::TrigMode) || (first.len() == 7 && second.contains(&Item::TrigMode)), "{layout:?} {t:?}: {first:?}");
        }
        assert!(tab(MenuId::Trigger, "Type and source").rows.contains(&Item::TrigMode), "{layout:?}: the edge trigger");
        // by touch. Clear acts at the first tap; Delete of a zone and Clear of a slot wait for a second one
        let foot = |u: &Ui, it: Item| u.sheet_geo().unwrap().foot.iter().find(|f| f.0 == it).map(|f| f.1).unwrap_or_else(|| panic!("{layout:?}: {it:?}"));
        let mut u = fresh(layout);
        u.open_menu(MenuId::Display);
        u.step(0.04);
        let (x, y) = mid(foot(&u, Item::ClearDisp));
        tap(&mut u, x, y);
        assert!(u.studio.md.armed.is_none(), "{layout:?}: Clear does not ask twice");
        u.close_menu();
        // two references: Clear R1 leaves R2
        let wave = |label: &str| RefWave { src: 0, color: 0, label: label.into(), t0: 0.0, dt: 1e-6, v: (0..100).map(|i| i as f32).collect::<Vec<_>>().into(), vpd: 1.0, offset: 0.0, on: true };
        (u.refs[0], u.refs[1]) = (Some(wave("one")), Some(wave("two")));
        u.ref_page.sel = 0;
        assert!(u.refs[0].is_some() && u.refs[1].is_some(), "{layout:?}: two references stored");
        u.ref_page.sel = 0;
        u.open_menu(MenuId::Ref);
        u.step(0.04);
        assert_eq!(u.sheet_label(Item::RefClearOne), "Clear R1");
        let (x, y) = mid(foot(&u, Item::RefClearOne));
        tap(&mut u, x, y);
        assert!(u.refs[0].is_some(), "{layout:?}: the first tap arms it");
        tap(&mut u, x, y);
        assert!(u.refs[0].is_none() && u.refs[1].is_some(), "{layout:?}: R1 is cleared, R2 is kept");
        u.close_menu();
        // the foot of the Measure sheet leads to the list of measurements
        u.open_menu(MenuId::Measure);
        u.step(0.04);
        assert_eq!(u.sheet_label(Item::Open(MenuId::MeasAdd)), "Add measurement");
        let (x, y) = mid(foot(&u, Item::Open(MenuId::MeasAdd)));
        tap(&mut u, x, y);
        assert!(u.menu_open && u.menu == MenuId::MeasAdd, "{layout:?}: {:?}", u.menu);
    }
    // Studio's trigger pill says the sweep, and the slope once
    let mut u = fresh(Layout::Studio);
    for (mode, word) in [(TrigMode::Auto, "Auto"), (TrigMode::Normal, "Normal"), (TrigMode::Single, "Single")] {
        u.m.trig_mode = mode;
        u.step(0.04);
        let r = u.studio_bottom().trig;
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        cv.au_start();
        u.render(&mut cv);
        let said = cv.au.take().unwrap().into_iter().any(|e| matches!(e, Au::Text { s, ink: Some(ink), .. } if s == word && r.contains(ink.0, (ink.1 + ink.3) / 2) && ink.2 <= r.right() - 16));
        assert!(said, "the trigger pill says {word}");
    }
}

/// What was behind a long press or several taps has a place of its own: Overlay / Stacked is a row of the Display
/// sheet, the cursors have a button in the top bar of Minimal and of Studio, and the badge of a channel pill is its
/// switch at every width at which the pill has a body beside it.
#[test]
fn traces_cursors_and_the_pill_switch_are_in_reach() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        u.m.ch[1].on = true;
        u.open_menu(MenuId::Display);
        u.step(0.04);
        let (x, y) = mid(u.sheet_geo().unwrap().tabs[1]);
        tap(&mut u, x, y);
        let row = u.sheet_geo().unwrap().rows.iter().find(|r| r.it == Item::Traces).map(|r| r.ctrl).unwrap_or_else(|| panic!("{layout:?}: no Traces row"));
        let was = u.trace_stacked();
        assert_eq!(u.value(Item::Traces), if was { "Stacked" } else { "Overlay" }, "{layout:?}");
        // the other half of the row
        let (x, y) = (if was { row.x + row.w / 4 } else { row.x + row.w * 3 / 4 }, row.y + row.h / 2);
        tap(&mut u, x, y);
        assert_eq!(u.trace_stacked(), !was, "{layout:?}: a tap on the other choice");
        assert_eq!(u.value(Item::Traces), if was { "Overlay" } else { "Stacked" }, "{layout:?}");
        u.close_menu();
        // the cursors: off at the start, on after the button
        assert_eq!(u.m.cursor, CursorMode::Off, "{layout:?}");
        let button = if layout == Layout::Studio { u.studio_top().cursors }
            else { u.top_buttons().into_iter().find(|b| b.0 == TopBtn::Menu(MenuId::Cursor)).map(|b| b.2).unwrap_or_else(|| panic!("{layout:?}: no Cursor button")) };
        let top = u.frame().top;
        assert!(button.w >= 40 && button.y >= top.y && button.bottom() <= top.bottom(), "{layout:?}: {button:?}");
        let (x, y) = mid(button);
        tap(&mut u, x, y);
        assert!(u.m.cursor != CursorMode::Off, "{layout:?}: one tap puts the cursors on");
        if layout == Layout::Studio {
            assert!(!u.menu_open, "Studio: the cursors alone at the first tap");
            tap(&mut u, x, y);
        }
        assert!(u.menu_open && u.menu == MenuId::Cursor, "{layout:?}: their sheet");
        // and off again from that sheet
        u.select_option(Item::CurMode, "Off");
        assert_eq!(u.m.cursor, CursorMode::Off, "{layout:?}");
    }
    // four channels and two references on: the pills are under 110 px, and a tap on one selects like on a wide one
    let mut u = fresh(Layout::Studio);
    for c in 0..4 { u.m.ch[c].on = true; }
    for i in 0..2 {
        u.refs[i] = Some(RefWave { src: 0, color: 0, label: format!("r{i}"), t0: 0.0, dt: 1e-6, v: (0..100).map(|i| i as f32).collect::<Vec<_>>().into(), vpd: 1.0, offset: 0.0, on: true });
    }
    u.relayout();
    u.step(0.04);
    let pill = u.studio_bottom().ch[2];
    assert!((70..110).contains(&pill.w), "{pill:?}");
    tap(&mut u, pill.x + 24, pill.y + pill.h / 2);
    assert!(u.m.ch[2].on && !u.menu_open && u.m.active == 2, "a tap on a pill of {} px selects CH3", pill.w);
    tap(&mut u, pill.right() - 12, pill.y + pill.h / 2);
    assert!(u.m.ch[2].on && u.menu_open && u.menu == MenuId::Channel(2), "the next tap opens its sheet");
}

/// Studio's hold menu of the plot: rows of 40 px in two columns, on the screen wherever it is opened, without the rows
/// the bars have one tap away (Run, Single, the trigger sheet, the sources). A row acts at a tap.
#[test]
fn the_hold_menu_of_the_plot_has_rows_a_finger_high() {
    let mut u = fresh(Layout::Studio);
    u.m.ch[1].on = true;
    u.set_zoom(true);
    for _ in 0..3 { u.step(0.04); }
    let g = u.plot.wins[0];
    for at in [(g.x + g.w / 3, g.y + g.h / 2), (4, 60), (u.scr.w - 4, 60), (4, u.scr.h - 60), (u.scr.w - 4, u.scr.h - 60)] {
        u.studio_ctx_open(at.0, at.1);
        let p = u.studio_ctx_rect().expect("the menu");
        assert!(p.x >= 0 && p.y >= 0 && p.right() <= u.scr.w && p.bottom() <= u.scr.h, "opened at {at:?}: {p:?}");
        let rows = u.studio_ctx_rows();
        for (i, (_, label, r)) in rows.iter().enumerate() {
            assert!(r.h >= 40 && r.w >= 200, "{label}: {r:?}");
            assert!(r.x >= p.x && r.right() <= p.right() && r.y >= p.y && r.bottom() <= p.bottom(), "{label}: {r:?} in {p:?}");
            for (_, other, o) in &rows[i + 1..] { assert!(r.right() <= o.x || o.right() <= r.x || r.bottom() <= o.y || o.bottom() <= r.y, "{label} over {other}"); }
        }
        let labels: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
        for gone in ["Run", "Stop", "Single", "Trigger settings", "Sources"] { assert!(!labels.contains(&gone), "{gone} is on the bars: {labels:?}"); }
        for kept in ["Overlay traces", "Stacked traces", "Fit all lanes", "Fit record", "Zoom here", "Zoom box", "Reset zoom", "Close zoom", "Cursor here", "Autoset"] {
            assert!(labels.contains(&kept), "{kept}: {labels:?}");
        }
        u.studio.ctx = None;
    }
    // by touch: a hold opens it, and Cursor here puts the cursors on
    assert_eq!(u.m.cursor, CursorMode::Off);
    pick_ctx(&mut u, super::studioctx::CtxRow::CursorHere);
    assert!(u.m.cursor != CursorMode::Off && u.studio.ctx.is_none(), "Cursor here");
}

/// Thin sheets are one page (DVM and counter, the logic analyser, the double pulse), the Add list has rows a finger
/// high, the chip that adds a source has one name, and the eye and the list button of the badge row take a touch of
/// 40 px.
#[test]
fn thin_sheets_are_one_page_and_the_add_controls_are_a_finger_high() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let u = fresh(layout);
        for menu in [MenuId::Dvm, MenuId::La, MenuId::Dpt] {
            let spec = u.sheet_spec(menu).unwrap_or_else(|| panic!("{layout:?}: {menu:?} has a sheet"));
            assert_eq!(spec.tabs.len(), 1, "{layout:?} {menu:?}: {:?}", spec.tabs.iter().map(|t| (t.name, t.rows.len())).collect::<Vec<_>>());
            assert!(spec.tabs[0].rows.len() <= 7 && spec.tabs[0].foot.len() <= 2, "{layout:?} {menu:?}");
        }
        // the counter's rows are on the page of the DVM, with its Clear
        let dvm = &u.sheet_spec(MenuId::Dvm).unwrap().tabs[0];
        assert!(dvm.rows.len() == 7 && dvm.foot.len() == 1, "{layout:?}: {:?} {:?}", dvm.rows, dvm.foot);
    }
    let mut u = fresh(Layout::Studio);
    // the chip of the bottom bar: one name and one width, whatever is off
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    for on in [1, 4] {
        for c in 0..4 { u.m.ch[c].on = c < on; }
        u.step(0.04);
        let chip = u.studio_bottom().add;
        assert_eq!(chip.w, 62, "{on} channels on");
        cv.au_start();
        u.render(&mut cv);
        let said = cv.au.take().unwrap().into_iter().any(|e| matches!(e, Au::Text { s, ink: Some(ink), .. } if s == "+ Source" && chip.contains(ink.0, (ink.1 + ink.3) / 2) && ink.2 <= chip.right()));
        assert!(said, "{on} channels on: the chip says + Source");
    }
    // the Add list of the top bar
    let (x, y) = mid(u.studio_top().add);
    tap(&mut u, x, y);
    assert!(u.studio.add_menu);
    let rows = u.studio_add_rows();
    assert!(rows.len() >= 10 && rows.iter().all(|r| r.1.h >= 40 && r.1.bottom() <= u.scr.h), "{:?}", rows.iter().map(|r| r.1).collect::<Vec<_>>());
    u.studio.add_menu = false;
    // the Meas button of the badge row: a touch 4 px over its 34 px still opens the table
    let chip = u.studio_list_chip();
    assert!(chip.h < 40 && u.studio.view != super::studio::StudioView::Docked);
    tap(&mut u, chip.x + chip.w / 2, chip.y - 2);
    assert_eq!(u.studio.view, super::studio::StudioView::Docked, "the button takes a touch of 40 px");
}

/// The measurement panel of Studio is made for a finger: its toolbar buttons are 36 px high with a touch of 40, the
/// rows of its table are 36 px in a tall panel, and the three segments of its view switch are 32 px wide. With the badges hidden, the
/// chip of the bottom bar that brings them back says "Badges", and a tap on it does.
#[test]
fn the_measurement_panel_and_the_badge_buttons_are_a_finger_high() {
    use super::dock::VKind;
    use super::measpanel::PanelView;
    use super::studio::StudioView;
    let mut u = fresh(Layout::Studio);
    u.studio_set_view(StudioView::Docked);
    for _ in 0..3 { frame(&mut u); }
    let panel = u.studio_panel_rect().expect("the docked panel");
    assert!(!Ui::panel_bar_wide(panel), "a narrow panel has its toolbar under the title: {panel:?}");
    let bar = u.panel_bar(panel);
    assert!(bar.slots.len() >= 4, "{} buttons", bar.slots.len());
    for slot in bar.slots.iter().chain(bar.more.iter()) {
        assert!(slot.r.h >= 36 && slot.hit.h >= 40 && slot.r.w >= 34, "{:?}: {:?} touch {:?}", slot.item, slot.r, slot.hit);
        assert!(slot.r.x >= panel.x && slot.r.right() <= panel.right(), "{:?} inside the panel", slot.item);
    }
    // the switch between table, list and cards: three segments of 32 px, each of which picks its view
    let strip = u.dock_rect(VKind::Measure).expect("the panel's place");
    let sw = u.dock_view_rect(VKind::Measure, strip).expect("the view switch");
    assert!(sw.w / 3 >= 32 && sw.h >= 26, "{sw:?}");
    for (k, view) in [PanelView::Detailed, PanelView::List, PanelView::Cards].into_iter().enumerate() {
        tap(&mut u, sw.x + (2 * k as i32 + 1) * sw.w / 6, sw.y + sw.h / 2);
        for _ in 0..2 { frame(&mut u); }
        assert_eq!(u.panel_view(), view, "segment {k}");
    }
    // the table in this tall panel has rows of 36 px (a low panel under the plot keeps 28, to show five of them)
    u.panel_set_view(PanelView::Detailed);
    for _ in 0..2 { frame(&mut u); }
    assert!(u.studio_panel_rect().is_some_and(|p| p.h >= 380) && u.detail_row_h() == 36 && u.studio_panel_row() == 36, "{:?}", u.studio_panel_rect());
    // the badges hidden: the bottom bar says what its chip brings back
    u.studio_set_view(StudioView::Waveform);
    for _ in 0..2 { frame(&mut u); }
    u.studio_hide_badges();
    for _ in 0..2 { frame(&mut u); }
    assert!(u.studio.hide_badges && u.studio_badges_hidden());
    let chip = u.studio_bottom().show.expect("the chip of the bottom bar");
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    cv.au_start();
    u.render(&mut cv);
    let said = cv.au.take().unwrap().into_iter().any(|e| matches!(e, Au::Text { s, ink: Some(ink), .. } if s == "Badges" && chip.contains(ink.0, (ink.1 + ink.3) / 2) && ink.2 <= chip.right()));
    assert!(said, "the chip says Badges: {chip:?}");
    let (x, y) = mid(chip);
    tap(&mut u, x, y);
    for _ in 0..2 { frame(&mut u); }
    assert!(!u.studio.hide_badges, "a tap brings the badges back");
}

/// A new set-up shows its measurements in the classic strip, a line per source with names and values in columns. The
/// other style is what a set-up file says; a file that says nothing has the classic strip, and one that says
/// `classic`, as files of before do, has it too.
#[test]
fn the_classic_strip_is_the_badge_style_of_a_new_setup() {
    assert!(Model::default().studio_classic);
    let mut u = fresh(Layout::Studio);
    for _ in 0..2 { frame(&mut u); }
    assert!(u.studio_classic() && u.value(Item::BadgeStyle) == "Classic");
    let text = crate::store::setup_text(&u.m);
    assert!(!text.contains("studio_badges"), "the default is not written");
    assert!(crate::store::parse_setup(&text).unwrap().studio_classic);
    assert!(crate::store::parse_setup("studio_badges = classic").unwrap().studio_classic);
    // the other style is written and read back
    u.select_option(Item::BadgeStyle, "Studio");
    assert!(!u.studio_classic());
    let text = crate::store::setup_text(&u.m);
    assert!(text.lines().any(|l| l.trim() == "studio_badges = studio"), "the Studio badges are written");
    assert!(!crate::store::parse_setup(&text).unwrap().studio_classic);
    // a reset is a new set-up
    u.select_option(Item::BadgeStyle, "Classic");
    assert!(u.studio_classic());
}

/// The View menu arranges the screen and ends with Reset layout. It had an "Add panel" row under it: a longer way to
/// five of the ten rows that the Add button of the top bar opens. Each of the five is a row of the Add menu, two taps
/// from the plot.
#[test]
fn the_view_menu_ends_with_reset_layout_and_adding_is_the_button_of_the_top_bar() {
    use super::addmenu::{AddRow, ADD_MENU};
    use super::studio::VIEW_RESET;
    let mut u = fresh(Layout::Studio);
    let (x, y) = mid(u.studio_top().view);
    tap(&mut u, x, y);
    assert!(u.studio.view_menu);
    let (p, last) = (u.studio_view_rect(), u.studio_view_row(VIEW_RESET));
    assert!(p.bottom() >= last.bottom() && p.bottom() - last.bottom() <= 12, "no empty row under Reset layout: {p:?} {last:?}");
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    cv.au_start();
    u.render(&mut cv);
    let texts: Vec<String> = cv.au.take().unwrap().into_iter().filter_map(|e| match e { Au::Text { s, .. } => Some(s), _ => None }).collect();
    assert!(texts.iter().any(|s| s == "Reset layout") && !texts.iter().any(|s| s == "Add panel"), "{texts:?}");
    // a tap under the menu closes it and opens nothing
    tap(&mut u, p.x + p.w / 2, p.bottom() + 20);
    assert!(!u.studio.view_menu && !u.menu_open);
    // what the row led to: one tap on Add, one on a row
    for want in [AddRow::Cursors, AddRow::Measurement, AddRow::Search, AddRow::ResultsTable, AddRow::Plot] {
        assert!(ADD_MENU.contains(&Some(want)), "{want:?}");
    }
    let (x, y) = mid(u.studio_top().add);
    tap(&mut u, x, y);
    assert!(u.studio.add_menu);
    let (_, row) = u.studio_add_rows().into_iter().find(|(r, _)| *r == AddRow::Plot).expect("Plot");
    let (x, y) = mid(row);
    tap(&mut u, x, y);
    assert!(u.menu_open && u.menu == MenuId::AddPanel);
    assert_eq!(u.sheet.add.kind, Some(super::addrail::AddKind::Plot));
}

/// Menu > Measure opens the Measure page in Studio; Back leaves it, and the next Measure is the quick Add panel again
/// (the page stayed chosen after Back, and the key opened the page where the panel is promised).
#[test]
fn back_from_the_measure_page_makes_the_next_measure_the_quick_panel() {
    let mut u = fresh(Layout::Studio);
    u.open_menu(MenuId::Main);
    u.activate(Item::Open(MenuId::Measure));
    assert!(u.menu == MenuId::MeasAdd && u.studio.meas_page, "the Menu opens the page itself");
    u.menu_back();
    assert!(u.menu == MenuId::Main && !u.studio.meas_page);
    u.open_submenu(MenuId::MeasAdd);
    assert!(u.sheet_active(), "the quick Add panel");
}

/// A bit rate from a fast time base, kept at a slow one, is no clock of the slow record: its bit is far below a
/// sample. Recovered with it, the clock had hundreds of millions of bits between two edges and the eye page took
/// 3.7 GB on a unit (2026-11-18, the program was killed). The clock is refused, and a trend has a bound of its own.
#[test]
fn a_bit_rate_above_the_sample_rate_is_no_clock_of_the_record() {
    use oos_dsp::eye;
    // 1 kSa/s, an edge every 100 samples: 5 bit/s as it stands
    let (dt, n) = (1e-3, 20_000);
    let v: Vec<f32> = (0..n).map(|i| if (i / 100) % 2 == 0 { 0.0 } else { 1.0 }).collect();
    let fitted = eye::recover_clock(&v, dt, 0.5, 0.1, None).expect("a clock from the edges");
    assert!((fitted.period / 0.1 - 1.0).abs() < 0.01, "{}", fitted.period);
    assert!(eye::recover_clock(&v, dt, 0.5, 0.1, Some(10.0)).is_some(), "a rate the record can hold");
    // 2 Mbit/s on that record: 2000 bits a sample, 40 million between the first edge and the last
    assert!(eye::recover_clock(&v, dt, 0.5, 0.1, Some(2e6)).is_none());
    assert!(eye::recover_clock_pll(&v, dt, 0.5, 0.1, Some(2e6), 0.0, true).is_none());
    // the same from the signal alone: spikes that only just pass the comparator rise and fall within a small part of
    // a sample, the shortest spacings are taken for the bit, and the record would hold half a billion of them
    let spikes: Vec<f32> = (0..n).map(|i| if i % 50 == 25 { 2e-5 } else { -1.0 }).collect();
    assert!(eye::recover_clock(&spikes, dt, 0.0, 1e-5, None).is_none_or(|clock| clock.period >= 0.5 * dt));
    let edges = oos_dsp::edges::edges(&spikes, 0.0, 1e-5);
    let times: Vec<f64> = edges.iter().map(|edge| edge.t * dt).collect();
    assert!(eye::clock_from_edges(times, None).is_some_and(|clock| clock.period < dt / 1000.0), "the fit the bound is for");
    // a clock from edge times alone, with a bit of 1 ns and edges a second apart: no trend, not a billion values
    let clock = eye::clock_from_edges(vec![0.0, 1.0, 2.0, 3.0], Some(1e9)).expect("a clock");
    let tie = eye::tie(&clock.edges, &clock);
    assert!(eye::tie_trend(&clock.edges, &tie, &clock).is_empty());
}

/// On the page: a rate that was set stays and the page says it needs a faster time base; a rate found from a record
/// is found again from the record of the slower time base.
#[test]
fn the_eye_page_takes_a_slow_time_base_after_a_fast_one() {
    for from_record in [false, true] {
        let mut u = fresh(Layout::Studio);
        u.m.tscale = tscales().iter().position(|&t| (t as f64 - 5e-2).abs() < 1e-9).unwrap();
        (u.m.eye_src, u.m.eye_from_record, u.m.eye_rate) = (WaveId::Ch(0), from_record, 2e9);
        u.m.view = AView::Eye;
        let started = u.azr.page.status.clone();
        for _ in 0..12 { u.step(0.05); frame(&mut u); }
        eprintln!("from_record {from_record}: rate {} status {:?} (was {started:?})", u.m.eye_rate, u.azr.page.status);
        if from_record {
            assert!(u.m.eye_rate < 2e9, "found again from this record: {}", u.m.eye_rate);
        } else {
            assert_eq!(u.m.eye_rate, 2e9, "a set rate is the user's");
            assert!(u.azr.eye_res.is_none() && u.azr.eye.is_none(), "no eye of a clock the record cannot hold");
            assert!(u.azr.page.status[0].contains("needs a faster time base") && u.azr.page.img.is_empty(), "said in the plot: {:?}", u.azr.page.status);
        }
    }
}

/// The button that brings the hidden badges back does not cost a channel pill its scale: with four channels, the LA
/// and the generator in the bar each pill had 29 px for it and "500 mV" was cut to "500m" (seen on a unit,
/// 2026-11-18). The button is then its eye alone and the chip that adds a source its plus alone, both still a
/// touch wide; with fewer sources they keep their names.
#[test]
fn the_badges_button_does_not_cost_a_channel_pill_its_scale() {
    let mut u = fresh(Layout::Studio);
    for c in 0..n_ch() { (u.m.ch[c].on, u.m.ch[c].vscale) = (true, crate::model::VSCALES.iter().position(|v| (v - 0.5).abs() < 1e-6).unwrap()); }
    frame(&mut u);
    let before = u.studio_bottom();
    assert!(before.la.is_some() && before.gen.is_some() && before.show.is_none(), "the full bar of a unit with LA and generator");
    u.studio_hide_badges();
    frame(&mut u);
    let b = u.studio_bottom();
    let room = |r: Rect| r.w - 62;
    let scale = u.fonts().s.width("500 mV");
    for c in 0..n_ch() {
        assert!(room(b.ch[c]) >= scale, "CH{}: {} px for a scale of {scale}", c + 1, room(b.ch[c]));
    }
    let show = b.show.expect("the button that brings the badges back");
    assert!(show.w >= 40 && show.h >= 36 && b.add.w >= 40, "still a touch wide: {show:?} {:?}", b.add);
    assert!(show.right() <= b.time.x && b.add.right() <= show.x, "in the bar, in their order");
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("walk-badges-hidden-full-bar.png"), crate::store::png_of(&cv)).unwrap();
    }
    tap(&mut u, show.x + show.w / 2, show.y + show.h / 2);
    assert!(u.studio_badges_visible(), "the eye alone brings the row back");
    // two channels: the room is there, and both keep their names
    for c in 2..n_ch() { u.m.ch[c].on = false; }
    u.studio_hide_badges();
    frame(&mut u);
    let b = u.studio_bottom();
    assert!(b.show.is_some_and(|r| r.w >= 80) && b.add.w >= 62, "{:?} {:?}", b.show, b.add);
}

/// The classic strip reads as a table and ends with one small chip. A column is as wide as its own widest name and
/// value, and its values begin at one place; the chip at the end is as small as a pair, says how many measurements
/// there are (or "+N" for those without a place) and opens the table. Before, every column was as wide as the
/// widest pair of all, and a button "Meas . N" over the whole height of the strip stood beside a "+N" chip: six
/// pairs a line where seven fit (seen on a unit, 2026-11-18, with these eighteen measurements).
#[test]
fn the_classic_strip_is_a_table_and_ends_with_one_small_chip() {
    use oos_dsp::measure::Meas::*;
    let mut u = fresh(Layout::Studio);
    u.studio_set_classic(true);
    u.m.ch[1].on = true;
    u.m.meas = [Vamp, Vmax, Vmin, Vpp, Vtop, Vbase, Vupper, Vmid, Vlower, Overshoot, Vavg, Area, StdDev].into_iter().map(|m| (m, WaveId::Ch(0)))
        .chain([Median, Mode, StdDev, VrmsAc].into_iter().map(|m| (m, WaveId::Ch(1)))).collect();
    u.studio.pinned = None;
    for _ in 0..3 { frame(&mut u); }
    let (row, chip, l) = (u.studio_row(), u.studio_list_chip(), u.classic_layout());
    eprintln!("row {row:?} chip {chip:?} pairs {} overflow {} lines {:?}", l.pairs.len(), l.overflow, l.chips.iter().map(|c| c.1.y).collect::<Vec<_>>());
    assert!(chip.w <= 60 && chip.h <= 24 && chip.right() <= row.right() && chip.y >= row.y && chip.bottom() <= row.bottom(), "{chip:?} in {row:?}");
    // the pairs: in the row, before the chip, none over another
    for (i, (_, r)) in l.pairs.iter().enumerate() {
        let beside = r.y < chip.bottom() && chip.y < r.bottom();
        assert!(r.x >= row.x && r.right() <= if beside { chip.x } else { row.right() } && r.y >= row.y && r.bottom() <= row.bottom(), "pair {i} at {r:?}");
        for (_, o) in &l.pairs[i + 1..] { assert!(r.right() <= o.x || o.right() <= r.x || r.bottom() <= o.y || o.bottom() <= r.y, "{r:?} over {o:?}"); }
    }
    // a table: the pairs of a column begin at one place, and so do their values
    let mut columns: std::collections::BTreeMap<i32, Vec<i32>> = Default::default();
    for ((_, r), value) in l.pairs.iter().zip(&l.values) { columns.entry(r.x).or_default().push(*value); }
    assert!(columns.values().all(|values| values.iter().all(|v| *v == values[0])), "{columns:?}");
    assert!(columns.len() >= 7, "{} columns on a line", columns.len());
    assert_eq!(l.pairs.len() + l.overflow, 17, "every measurement has a place or is counted");
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("walk-classic-strip.png"), crate::store::png_of(&cv)).unwrap();
    }
    // the chip opens the table
    let (x, y) = mid(chip);
    tap(&mut u, x, y);
    assert_eq!(u.studio.view, super::studio::StudioView::Docked);
}

/// Four channels with measurements have a line of the strip each; a fifth source (a reference) is behind "+N" with
/// what has no place on the lines, and the chip counts them all. (The strip had three lines: the fourth channel's
/// measurements were nowhere on it.)
#[test]
fn the_classic_strip_has_a_line_for_each_of_four_channels() {
    use oos_dsp::measure::Meas::*;
    let mut u = fresh(Layout::Studio);
    u.studio_set_classic(true);
    for c in 0..n_ch() { u.m.ch[c].on = true; }
    for r in 0..3 {
        u.refs[r] = Some(RefWave { src: 0, color: r, label: String::new(), t0: -5e-3, dt: 1e-6, v: known_pulses(0.0).into(), vpd: 0.2, offset: 0.0, on: true });
    }
    let on = |ms: &[oos_dsp::measure::Meas], id: WaveId| ms.iter().map(|m| (*m, id)).collect::<Vec<_>>();
    u.m.meas = [on(&[Vamp, Vpp, Vbase, Vlower, Preshoot, Overshoot, Vmax, Vmin, Vtop, Vmid, Vavg, Area], WaveId::Ch(0)), on(&[Median, Mode, StdDev, VrmsAc], WaveId::Ch(1)),
        on(&[Overshoot, Median, Mode, Area, StdDev, Vrms, Freq, Period], WaveId::Ch(2)), on(&[Vpp, Freq, Rise, Fall], WaveId::Ch(3)), on(&[Vpp, Freq, Vavg, Vrms], WaveId::Ref(0))].concat();
    u.studio.pinned = None;
    for _ in 0..4 { u.step(0.03); frame(&mut u); }
    let (l, row, chip) = (u.classic_layout(), u.studio_row(), u.studio_list_chip());
    assert_eq!(l.chips.iter().map(|c| c.0).collect::<Vec<_>>(), (0..4).map(WaveId::Ch).collect::<Vec<_>>());
    assert_eq!(l.pairs.len() + l.overflow, u.m.meas.len(), "every measurement has a place or is counted");
    assert!(l.overflow >= 4 && u.studio_badges_more().map(|m| m.1) == Some(l.overflow), "the reference's four and what did not fit: {}", l.overflow);
    assert!(l.pairs.iter().all(|(_, r)| r.y >= row.y && r.bottom() <= row.bottom() && r.right() <= row.right() && (r.right() <= chip.x || r.bottom() <= chip.y || chip.bottom() <= r.y)), "in the row, none under the chip");
    // the chip is the last thing of a line that has the room: no line gives it a pair while another has room
    let on_its_line = l.pairs.iter().filter(|(_, r)| r.y == chip.y).count();
    assert!(l.chips.iter().any(|(_, c)| c.y == chip.y) && on_its_line < l.pairs.iter().filter(|(_, r)| r.y == l.chips[0].1.y).count(), "the chip is on a line with room: {chip:?}");
    // the strip stands still: the places are those of the names and the units, whatever the values read. A record
    // that changes every value (and "----" where there was one) moves no pair
    let before = l.pairs.clone();
    u.m.ch[0].vscale = (u.m.ch[0].vscale + 3).min(crate::model::VSCALES.len() - 1);
    for _ in 0..6 { u.step(0.05); frame(&mut u); }
    assert_eq!(u.classic_layout().pairs, before, "the pairs stay where they are when the values change");
    u.m.ch[0].vscale -= 3;
    for _ in 0..3 { u.step(0.05); frame(&mut u); }
    assert!(u.plot_rect().bottom() <= row.y && u.plot_rect().h >= 250, "the plot keeps most of the screen: {:?}", u.plot_rect());
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("walk-many-sources.png"), crate::store::png_of(&cv)).unwrap();
    }
}

/// Choosing Stacked or Overlay in Studio fits every signal to the height it gets: a channel that was kept apart from
/// the others by its offset (three divisions up, at a scale too coarse for it) is in the middle of its lane and
/// fills it, and in Overlay the signals lie on one another in the whole plot. A mode that a set-up brings changes no
/// channel.
#[test]
fn choosing_stacked_or_overlay_fits_every_signal_to_its_height() {
    let mut u = fresh(Layout::Studio);
    u.trace_mode(false);
    for _ in 0..6 { u.step(0.03); frame(&mut u); }
    // the signal of the demo on CH1, parked high and small as in a plot shared with three others
    let (lo, hi) = u.auto_levels(0);
    assert!(hi - lo > 0.5, "the demo has a signal on CH1: {lo} {hi}");
    (u.m.ch[0].vscale, u.m.ch[0].offset) = (VSCALES.iter().position(|v| *v == 5.0).unwrap(), 12.0);
    for _ in 0..6 { u.step(0.03); frame(&mut u); }
    let parked = (u.m.ch[0].vscale, u.m.ch[0].offset);
    // a set-up or a view sets the mode: nothing is fitted
    u.trace_mode(true);
    assert_eq!((u.m.ch[0].vscale, u.m.ch[0].offset), parked);
    u.trace_mode(false);
    for stacked in [true, false] {
        (u.m.ch[0].vscale, u.m.ch[0].offset) = parked;
        for _ in 0..6 { u.step(0.03); frame(&mut u); }
        u.choose_trace_mode(stacked);
        let ch = &u.m.ch[0];
        let (vdiv, span) = (ch.volts_per_div(), hi - lo);
        assert!(span <= 6.0 * vdiv && span >= 2.0 * vdiv, "stacked {stacked}: {span} V at {vdiv} V/div fills its height");
        assert!((ch.offset + (hi + lo) / 2.0).abs() < 0.2 * vdiv, "stacked {stacked}: in the middle, offset {}", ch.offset);
        assert!(u.toast_text().contains(if stacked { "Stacked" } else { "Overlay" }), "{}", u.toast_text());
        if let Some(dir) = std::env::var_os("OOS_SHOTS") {
            for _ in 0..6 { u.step(0.03); frame(&mut u); }
            let mut cv = Canvas::new(W, H);
            u.render(&mut cv);
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(std::path::Path::new(&dir).join(format!("walk-fit-{}.png", if stacked { "stacked" } else { "overlay" })), crate::store::png_of(&cv)).unwrap();
        }
    }
    // the same mode again: nothing to fit, nothing said
    let now = (u.m.ch[0].vscale, u.m.ch[0].offset);
    u.choose_trace_mode(false);
    assert_eq!((u.m.ch[0].vscale, u.m.ch[0].offset), now);
}

/// A math trace has its badge at the left of its lane in Studio, as a channel has: "M1" in the colour of the trace.
#[test]
fn a_math_lane_has_its_badge() {
    let mut u = fresh(Layout::Studio);
    u.m.ch[1].on = true;
    u.m.math_hidden[0] = false;
    u.m.math = crate::math::MathOp::Add;
    assert!(u.m.math_shown(0), "M1 = CH1 + CH2 is on");
    u.trace_mode(true);
    for _ in 0..6 { u.step(0.03); frame(&mut u); }
    let mut cv = Canvas::new(W, H);
    cv.au_start();
    u.render(&mut cv);
    let texts: Vec<String> = cv.au.take().unwrap().into_iter().filter_map(|e| match e { Au::Text { s, .. } => Some(s), _ => None }).collect();
    assert!(texts.iter().any(|s| s == "M1"), "the badge of the math lane: {:?}", texts.iter().filter(|s| s.starts_with('M')).collect::<Vec<_>>());
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("walk-math-badge.png"), crate::store::png_of(&cv)).unwrap();
    }
}

/// Hide trace takes the picture of a channel away and nothing else: the channel stays on, its math trace and its
/// measurements go on, the set-up and SCPI hold the switch. Switching the channel off was the only way before, and
/// that took the math trace with it.
#[test]
fn a_hidden_channel_stays_on_for_math_and_measurements() {
    let mut u = fresh(Layout::Studio);
    u.m.ch[1].on = true;
    // the demo has its signal on CH1: M1 = CH1 + CH1
    u.m.math = crate::math::MathOp::Add;
    u.m.math_b = u.m.math_a;
    u.trace_mode(true);
    for _ in 0..6 { u.step(0.03); frame(&mut u); }
    let value = |u: &Ui| u.meas_rows().iter().find(|r| r.src == WaveId::Ch(0)).map(|r| r.v);
    let math = |u: &mut Ui| { for _ in 0..40 { if u.math_record(0).is_ok_and(|w| w.is_some()) { return true; } u.step(0.03); frame(u); } false };
    assert!(u.trace_sources().contains(&super::workspace::Trace::Channel(0)), "CH1 has its lane");
    assert!(math(&mut u), "M1 = CH1 + CH1 is computed: {:?}", u.math_record(0).err());
    let before = value(&u).expect("a measurement on CH1");
    assert!(before.is_finite());
    // by the menu of the channel
    assert_eq!(u.value(Item::ChHide(0)), "Off");
    u.activate(Item::ChHide(0));
    for _ in 0..6 { u.step(0.03); frame(&mut u); }
    assert!(u.m.ch[0].on && u.m.ch_hidden[0] && !u.m.ch_drawn(0));
    assert!(!u.trace_sources().contains(&super::workspace::Trace::Channel(0)), "no lane for the hidden channel");
    assert!(u.trace_sources().contains(&super::workspace::Trace::Math(0)) && math(&mut u), "its math trace goes on");
    assert!(value(&u).is_some_and(|v| v.is_finite()), "its measurement goes on");
    // the set-up holds it and brings it back
    let text = u.setup_text_full();
    assert!(text.lines().any(|l| l == "ch_hidden = 1"), "{:?}", text.lines().filter(|l| l.starts_with("ch")).take(3).collect::<Vec<_>>());
    let mut other = fresh(Layout::Studio);
    other.apply_setup_text(&text).unwrap();
    assert!(other.m.ch_hidden[0] && !other.m.ch_hidden[1]);
    // the pill's menu shows it again
    u.workspace_action(super::workspace::Action::ChHide(0));
    for _ in 0..3 { u.step(0.03); frame(&mut u); }
    assert!(!u.m.ch_hidden[0] && u.trace_sources().contains(&super::workspace::Trace::Channel(0)));
}

/// The name of a decoder beside its band is a badge in Studio, and a tap on it opens the Decode menu of that
/// decoder; a second tap closes it.
#[test]
fn a_tap_on_the_name_of_a_decoder_opens_its_decode_menu() {
    let mut u = fresh(Layout::Studio);
    u.m.dec_on = true;
    // the band is there while the channel of the decoder's line is on
    let line = u.m.dec_src;
    u.m.ch[line].on = true;
    for _ in 0..6 { u.step(0.03); frame(&mut u); }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    let labels = u.studio.dec_labels.borrow().clone();
    let &(k, r) = labels.first().expect("the decoder has its name in the plot");
    assert!(k == 0 && r.w >= 40 && r.h >= 18 && u.plot_rect().x <= r.x, "{r:?}");
    let (x, y) = mid(r);
    tap(&mut u, x, y);
    assert!(u.menu_open && u.menu == MenuId::Decode && u.m.dec_sel == 0, "the Decode menu of decoder 1");
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("walk-decode-menu.png"), crate::store::png_of(&cv)).unwrap();
    }
}

/// A decoder that is on has a pill in the bottom bar with its protocol and its lines; a tap on it opens the Decode
/// sheet of that decoder, a second tap closes it, and a decoder that is off has none.
#[test]
fn a_decoder_has_a_pill_in_the_bottom_bar_that_opens_its_sheet() {
    let mut u = fresh(Layout::Studio);
    for _ in 0..3 { u.step(0.03); frame(&mut u); }
    assert!(u.studio_bottom().dec.iter().all(|r| r.w == 0), "no decoder is on");
    // decoder 2: SPI
    u.select_option(Item::DecSel, "B2");
    u.m.dec_on = true;
    u.select_option(Item::DecProto, "SPI");
    u.select_option(Item::DecSel, "B1");
    for _ in 0..3 { u.step(0.03); frame(&mut u); }
    let b = u.studio_bottom();
    assert!(b.dec[0].w == 0 && b.dec[1].w >= 40 && b.dec[1].h >= 36, "{:?}", b.dec);
    assert!(b.dec[1].right() <= b.add.x && b.add.right() <= b.time.x, "the pill is among the sources: {:?}", b.dec[1]);
    let m = u.m.dec_view(1);
    assert_eq!(super::decband::dec_lines_text(&m).split(' ').count(), 3, "SPI has three lines");
    let mut cv = Canvas::new(W, H);
    cv.au_start();
    u.render(&mut cv);
    let texts: Vec<String> = cv.au.take().unwrap().into_iter().filter_map(|e| match e { Au::Text { s, .. } => Some(s), _ => None }).collect();
    assert!(texts.iter().any(|t| t == "B2") && texts.iter().any(|t| t == "SPI"), "the pill says which decoder and what it reads");
    let (x, y) = mid(b.dec[1]);
    tap(&mut u, x, y);
    assert!(u.menu_open && u.menu == MenuId::Decode && u.m.dec_sel == 1, "the Decode sheet of decoder 2");
    tap(&mut u, x, y);
    assert!(!u.menu_open, "a second tap closes it");
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        tap(&mut u, x, y);
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join("walk-decoder-pill.png"), crate::store::png_of(&cv)).unwrap();
    }
}

/// The Decode sheet: what the protocol needs is on the first page with its lines (the baud rate of a UART; SPI has
/// its four lines there, and its mode heads Settings), the B line of a differential pair is a setting, the lines
/// are named by their role, and the protocol is chosen from its list, without steps.
#[test]
fn the_decode_sheet_has_the_protocols_rows_on_its_first_page() {
    let mut u = fresh(Layout::Studio);
    u.m.dec_on = true;
    for name in ["UART", "SPI"] {
        u.select_option(Item::DecProto, name);
        u.open_menu(MenuId::Decode);
        for _ in 0..3 { u.step(0.03); frame(&mut u); }
        let spec = u.sheet_spec(MenuId::Decode).expect("Decode is a sheet");
        let first = &spec.tabs[0].rows;
        eprintln!("{name}: first page {:?} | settings {:?}", first.iter().map(|it| u.sheet_label(*it)).collect::<Vec<_>>(), spec.tabs[1].rows.iter().map(|it| u.sheet_label(*it)).collect::<Vec<_>>());
        assert!(first.len() <= 7 && !first.contains(&Item::DecDiff), "{name}: {first:?}");
        if name == "UART" {
            assert!(first.contains(&Item::DecBaud), "its own setting is on the first page: {first:?}");
        } else {
            let lines = [Item::DecSrc, Item::DecSrc2, Item::DecCs, Item::DecSrc4];
            assert!(lines.iter().all(|it| first.contains(it)), "the four lines stand together: {first:?}");
            assert_eq!(spec.tabs[1].rows.first(), Some(&Item::DecMode), "the mode heads Settings");
            assert!(spec.tabs[1].rows.len() <= 7, "{:?}", spec.tabs[1].rows);
        }
        assert!(Ui::sheet_pick_plain(Item::DecProto) && u.sheet_row_kind(Item::DecProto) == RowKind::Pick);
        if name == "SPI" { assert_ne!(u.sheet_label(Item::DecSrc2), u.sheet_label(Item::DecSrc), "the lines have their roles"); }
        if let Some(dir) = std::env::var_os("OOS_SHOTS") {
            let mut cv = Canvas::new(W, H);
            u.render(&mut cv);
            let _ = std::fs::create_dir_all(&dir);
            std::fs::write(std::path::Path::new(&dir).join(format!("walk-decode-{}.png", name.to_lowercase())), crate::store::png_of(&cv)).unwrap();
        }
        u.close_menu();
    }
}

/// The push of the horizontal SCALE knob switches the fine time base, and the time pill says so. It opened the zoom: a
/// time base left in fine steps then looked like a broken knob, with no way back from the panel (reported from a unit,
/// 2026-11-18). The steps come back at the step beside the fine value, and a push closes a zoom that is open.
#[test]
fn a_push_of_the_time_knob_switches_the_fine_steps() {
    let mut u = fresh(Layout::Studio);
    let step = u.m.tscale;
    assert!(!u.m.fine_t && !u.m.zoom);
    assert!(u.studio_time_detail().starts_with("delay"));
    u.key(key::HSCALE_CW + 2);
    assert!(u.m.fine_t && !u.m.zoom, "the push opened the zoom");
    assert!(u.studio_time_detail().starts_with("fine"), "{}", u.studio_time_detail());
    // a turn in fine steps: a hundredth, not a step
    let coarse = u.m.time_per_div();
    u.key(key::HSCALE_CW);
    u.key(key::HSCALE_CW);
    let fine = u.m.time_per_div();
    assert!(fine < coarse && fine > coarse * 0.9, "{coarse} {fine}");
    // the push back: the time base stays where it is
    u.key(key::HSCALE_CW + 2);
    assert!(!u.m.fine_t);
    assert_eq!(u.m.time_per_div(), fine);
    assert!(u.studio_time_detail().starts_with("delay"));
    // counter-clockwise is slower: the step above the fine value, not the one after it
    u.key(key::HSCALE_CW + 1);
    assert_eq!((u.m.tscale, u.m.tfine), (step, 1.0));
    // and clockwise from a fine value: the step below it
    u.key(key::HSCALE_CW + 2);
    u.key(key::HSCALE_CW);
    u.key(key::HSCALE_CW + 2);
    u.key(key::HSCALE_CW);
    assert_eq!((u.m.tscale, u.m.tfine), (step - 1, 1.0));
    // the zoom has its own command; while it is open the push closes it and leaves the steps alone
    crate::cmd::run(&mut u, "zoom");
    assert!(u.m.zoom);
    u.key(key::HSCALE_CW + 2);
    assert!(!u.m.zoom && !u.m.fine_t);
    assert!(crate::cmd::remote_ok("zoom") && !crate::cmd::remote_ok("zoom on"));
}

/// Add > Results table opens the page of the three tables (measurements, search events, decoder events), and each is
/// put on the screen and taken off there. The row docked the table of the measurements without a word, and no row of
/// the menu led to the events of a decoder or of a search (asked from a unit, 2026-11-18).
#[test]
fn add_results_table_offers_the_three_tables() {
    use super::addmenu::AddRow;
    use super::addrail::{AddKind, RailAct};
    use super::tables::TableKind;
    let shots = std::env::var("OOS_WALK_SHOTS").ok();
    let shot = |u: &mut Ui, name: &str| {
        let Some(dir) = &shots else { return };
        for _ in 0..20 { u.step(0.04); }
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        u.render(&mut cv);
        let _ = crate::store::write(&std::path::Path::new(dir).join(format!("{name}.png")), &crate::store::png_of(&cv));
    };
    let open = |u: &mut Ui| {
        if u.menu_open { u.close_menu(); }
        frame(u);
        let (x, y) = mid(u.studio_top().add);
        tap(u, x, y);
        let (_, row) = u.studio_add_rows().into_iter().find(|(r, _)| *r == AddRow::ResultsTable).expect("the row");
        let (x, y) = mid(row);
        tap(u, x, y);
        frame(u);
    };
    let shown = |u: &Ui, i: usize| match i { 0 => u.studio_panel_shown(), 1 => u.table.kind == Some(TableKind::Search), _ => u.events_pane_on() };
    let mut u = fresh(Layout::Studio);
    open(&mut u);
    assert!(u.menu_open && u.menu == MenuId::AddPanel);
    assert_eq!(u.sheet.add.kind, Some(AddKind::Table));
    assert!(!u.studio_panel_shown(), "the row docked the table of the measurements");
    assert_eq!(u.sheet.add.table, 0, "nothing is on: the measurements");
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    cv.au_start();
    u.render(&mut cv);
    let texts: Vec<String> = cv.au.take().unwrap().into_iter().filter_map(|e| match e { Au::Text { s, .. } => Some(s), _ => None }).collect();
    for want in ["Measurements", "Search events", "Decoder events"] {
        assert!(texts.iter().any(|s| s == want), "{want}: {texts:?}");
    }
    shot(&mut u, "tables-0-page");
    // the events of a decoder need one: the page says so and Add shows nothing
    let ty = u.rail_geo().types[2].1;
    tap(&mut u, mid(ty).0, mid(ty).1);
    let g = u.rail_geo();
    assert_eq!(g.foot.iter().map(|f| f.0).collect::<Vec<_>>(), [RailAct::Add]);
    tap(&mut u, mid(g.foot[0].1).0, mid(g.foot[0].1).1);
    assert!(!shown(&u, 2));
    u.adjust(Item::DecOn, 1);
    // every table: chosen, added, named as on the screen, removed
    for i in 0..3 {
        frame(&mut u);
        let ty = u.rail_geo().types[i].1;
        tap(&mut u, mid(ty).0, mid(ty).1);
        let g = u.rail_geo();
        assert_eq!(g.foot.iter().map(|f| f.0).collect::<Vec<_>>(), [RailAct::Add], "table {i}");
        tap(&mut u, mid(g.foot[0].1).0, mid(g.foot[0].1).1);
        frame(&mut u);
        assert!(shown(&u, i), "table {i} is not on the screen");
        assert!(u.menu_open && u.menu == MenuId::AddPanel, "the page stays for the next table");
        shot(&mut u, &format!("tables-{}-on", i + 1));
        let g = u.rail_geo();
        assert_eq!(g.foot.iter().map(|f| f.0).collect::<Vec<_>>(), [RailAct::Remove], "table {i}");
        tap(&mut u, mid(g.foot[0].1).0, mid(g.foot[0].1).1);
        frame(&mut u);
        assert!(!shown(&u, i), "table {i} stayed");
    }
    // with a decoder on and no table, the page opens on its events; the row of the menu says On for any of the three
    open(&mut u);
    assert_eq!(u.sheet.add.table, 2);
    let g = u.rail_geo();
    tap(&mut u, mid(g.foot[0].1).0, mid(g.foot[0].1).1);
    assert!(shown(&u, 2));
    u.close_menu();
    frame(&mut u);
    let (x, y) = mid(u.studio_top().add);
    tap(&mut u, x, y);
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    cv.au_start();
    u.render(&mut cv);
    let (_, row) = u.studio_add_rows().into_iter().find(|(r, _)| *r == AddRow::ResultsTable).unwrap();
    let on = cv.au.take().unwrap().into_iter().any(|e| matches!(e, Au::Text { ref s, bound: Some(b), .. } if s == "On" && b.y == row.y));
    assert!(on, "the row does not say On with the decoder events on the screen");
}

/// With AC trigger coupling the level counts from the signal's mean. Changing the coupling in the menu keeps the level
/// at the same place of the signal, and the push of the level knob (50 %) gives the middle counted from that mean. The
/// level stayed as set with DC coupling, where the signal without its DC part never reached it: the trigger stopped
/// (a forum report on 0.1.13, and seen on a unit, 2026-11-18).
#[test]
fn the_trigger_level_follows_the_signals_mean_with_ac_coupling() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.ch[2].on = true;
    u.m.trig_src = 2;
    u.relayout();
    for _ in 0..8 { u.step(0.1); }
    let s = u.measured.stats[2];
    assert!(s.valid, "the demo's line on CH3");
    let (mid, mean) = (crate::wave::mid_level(&s), s.vavg);
    assert!(mean > 1.0 && (mid - mean).abs() > 0.05, "a signal with a mean far from 0 V that is not its middle: mid {mid} mean {mean}");
    // DC: the middle in volts
    u.key(key::TLEVEL_CW + 2);
    assert!((u.m.trig_level - mid).abs() < 0.05, "{} {mid}", u.m.trig_level);
    assert_eq!(u.trig_level_base(), 0.0);
    // to AC in the menu: the level moves by the mean and stays on the same place of the signal
    let at = u.m.trig_level;
    u.adjust(Item::TrigCoup, 1);
    assert_eq!(u.m.trig_coup, TrigCoup::Ac);
    assert!((u.trig_level_base() - mean).abs() < 0.2);
    assert!((u.m.trig_level + u.trig_level_base() - at).abs() < 1e-4, "{} + {} against {at}", u.m.trig_level, u.trig_level_base());
    // the push of the knob with AC coupling: the middle, counted from the mean
    u.m.trig_level = 9.0;
    u.key(key::TLEVEL_CW + 2);
    let s = u.measured.stats[2];
    assert!((u.m.trig_level - (crate::wave::mid_level(&s) - s.vavg)).abs() < 0.05, "{}", u.m.trig_level);
    // on through LF reject and HF reject back to DC: in volts again
    let place = u.m.trig_level + u.trig_level_base();
    u.adjust(Item::TrigCoup, 1);
    assert_eq!(u.m.trig_coup, TrigCoup::LfReject);
    assert!((u.m.trig_level + u.trig_level_base() - place).abs() < 1e-4);
    u.adjust(Item::TrigCoup, 1);
    assert_eq!((u.m.trig_coup, u.trig_level_base()), (TrigCoup::HfReject, 0.0));
    assert!((u.m.trig_level - place).abs() < 1e-4, "HF reject counts in volts: {} {place}", u.m.trig_level);
}

/// Minimal and Studio, cursors on: the multipurpose knobs move them (the first A, the second B), with no menu open
/// and with the Cursor menu open; a turn had opened the Menu or stepped its rows (reported from a unit, 2026-11-18).
/// Without cursors a turn opens the Menu as before. Default keeps the knobs' job, as upstream OOS (batch4_tests.rs).
#[test]
fn the_multipurpose_knobs_move_the_cursors() {
    const A_CW: u8 = 0xba;
    const B_CW: u8 = 0xaa;
    for layout in [Layout::Studio, Layout::Minimal] {
        let mut u = fresh(layout);
        // no cursors: the knob opens the Menu
        u.key(A_CW);
        assert!(u.menu_open && u.menu == MenuId::Main, "{layout:?}");
        u.close_menu();
        // the Cursor key puts them on and opens their menu: the knobs move the cursors there, the menu stays
        u.key(key::CURSOR);
        assert!(u.m.cursor != CursorMode::Off && u.menu_open && u.menu == MenuId::Cursor, "{layout:?}");
        let (a, b) = (u.m.cur_a, u.m.cur_b);
        u.now += 1.0;
        u.key(A_CW);
        assert!(u.m.cur_a > a && u.m.cur_b == b, "{layout:?}: knob 1 moves cursor A: {} {}", u.m.cur_a, a);
        u.now += 1.0;
        u.key(B_CW + 1);
        assert!(u.m.cur_b < b, "{layout:?}: knob 2 moves cursor B back");
        assert!(u.menu_open && u.menu == MenuId::Cursor, "{layout:?}: the menu stays");
        // and with no menu open
        u.close_menu();
        let a = u.m.cur_a;
        u.now += 1.0;
        u.key(A_CW + 1);
        assert!(u.m.cur_a < a && !u.menu_open, "{layout:?}: no menu opens, the cursor moves");
        // a push opens the Cursor menu, the next closes it
        u.key(A_CW + 2);
        assert!(u.menu_open && u.menu == MenuId::Cursor, "{layout:?}");
        u.key(B_CW + 2);
        assert!(!u.menu_open, "{layout:?}");
        // another menu keeps the knobs for its rows
        u.open_menu(MenuId::Trigger);
        let a = u.m.cur_a;
        u.now += 1.0;
        u.key(A_CW);
        assert_eq!(u.m.cur_a, a, "{layout:?}: the Trigger menu has the knob");
        u.close_menu();
        // time and level cursors together: a push gives the knobs to the other pair
        u.m.cursor = CursorMode::Xy;
        let (a, c) = (u.m.cur_a, u.m.cur_c);
        u.key(A_CW + 2);
        assert!(!u.menu_open, "{layout:?}: the push changes the pair, no menu");
        u.now += 1.0;
        u.key(A_CW);
        assert!(u.m.cur_c != c && u.m.cur_a == a, "{layout:?}: knob 1 moves the first level cursor");
        u.key(A_CW + 2);
        u.now += 1.0;
        u.key(A_CW);
        assert!(u.m.cur_a != a, "{layout:?}: and the time cursor again");
    }
}
