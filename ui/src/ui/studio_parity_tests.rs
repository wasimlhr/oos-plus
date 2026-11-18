// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio offers what Default offers: every row of a menu of the Default layout is a row of the Studio sheet of that
//! menu (or the menu opens as the same dialog), and what the Default bars open, the Studio bars open too.
//! `cargo test parity_table -- --ignored --nocapture` prints the table of the audit.
use super::*;

fn default_ui() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    for _ in 0..8 { u.step(0.05); }
    u
}

fn studio_ui() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.studio.enter = true;
    for _ in 0..8 { u.step(0.05); }
    u
}

/// A state a menu is audited in: its name and what sets it up (the same on both layouts).
type State = (&'static str, fn(&mut Ui));

/// Every menu a user can open, with the states that change its rows.
fn menus() -> Vec<(MenuId, Vec<State>)> {
    use MenuId as M;
    let plain: Vec<State> = vec![("", |_| {})];
    let mut v: Vec<(MenuId, Vec<State>)> = vec![
        (M::Acquire, vec![("", |_| {}), ("Average", |u| u.m.acq = AcqMode::Average), ("Segments", |u| u.m.segments = 10)]),
        (M::Horizontal, vec![("", |_| {}), ("Zoom on", |u| u.m.zoom = true)]),
        (M::Trigger, vec![
            ("Edge", |_| {}),
            ("Pulse", |u| u.m.trig_type = TrigType::Pulse),
            ("Slope", |u| u.m.trig_type = TrigType::Slope),
            ("Video", |u| u.m.trig_type = TrigType::Video),
            ("Pattern", |u| u.m.trig_type = TrigType::Pattern),
            ("Runt", |u| u.m.trig_type = TrigType::Runt),
            ("Window", |u| u.m.trig_type = TrigType::Window),
            ("Timeout", |u| u.m.trig_type = TrigType::Timeout),
            ("Nth edge", |u| u.m.trig_type = TrigType::NthEdge),
            ("Delay", |u| u.m.trig_type = TrigType::Delay),
            ("Duration", |u| u.m.trig_type = TrigType::Duration),
            ("Setup/hold", |u| u.m.trig_type = TrigType::SetupHold),
            ("Bus", |u| u.m.trig_type = TrigType::Bus),
        ]),
        (M::Channel(0), plain.clone()),
        (M::ChMore(0), plain.clone()),
        (M::Measure, plain.clone()),
        (M::MeasPair, plain.clone()),
        (M::MeasStat, plain.clone()),
        (M::MeasLevels, plain.clone()),
        (M::MeasFall, plain.clone()),
        (M::Dvm, plain.clone()),
        (M::Cursor, vec![
            ("Off", |u| u.m.cursor = CursorMode::Off),
            ("Time", |u| u.m.cursor = CursorMode::Time),
            ("Track", |u| u.m.cursor = CursorMode::Track),
            ("XY", |u| u.m.cursor = CursorMode::Xy),
        ]),
        (M::Display, plain.clone()),
        (M::Windows, plain.clone()),
        (M::Storage, plain.clone()),
        (M::Utility, plain.clone()),
        (M::Network, plain.clone()),
        (M::Remote, plain.clone()),
        (M::Ssh, plain.clone()),
        (M::Time, plain.clone()),
        (M::Features, plain.clone()),
        (M::Decoders(0), plain.clone()),
        (M::SelfCal, plain.clone()),
        (M::Math, vec![
            ("Off", |_| {}),
            ("A + B", |u| u.m.math = MathOp::Add),
            ("FFT", |u| u.m.math = MathOp::Fft),
            ("Expression", |u| u.m.math = MathOp::Expr),
        ]),
        (M::Ref, plain.clone()),
        (M::Decode, plain.clone()),
        (M::Bus, plain.clone()),
        (M::Mask, plain.clone()),
        (M::Zone, plain.clone()),
        (M::Sim, plain.clone()),
        (M::SimAdvanced, plain.clone()),
        (M::Spectrum, plain.clone()),
        (M::Eye, plain.clone()),
        (M::Power, vec![("", |_| {}), ("Ripple", |u| u.m.pw_mode = PowerMode::Ripple), ("Rail", |u| u.m.pw_mode = PowerMode::Rail)]),
        (M::Bode, plain.clone()),
        (M::BwTest, plain.clone()),
        (M::Identify, plain.clone()),
        (M::Magnetic, plain.clone()),
        (M::Inverter, plain.clone()),
        (M::Dpt, plain.clone()),
        (M::Afg, plain.clone()),
        (M::ExtGen, plain.clone()),
        (M::La, vec![("", |_| {}), ("LA on", |u| u.m.la.on = true)]),
        (M::LaLines, plain.clone()),
        (M::LaPat, plain.clone()),
        (M::Logger, plain.clone()),
    ];
    v.retain(|(m, _)| !default_ui().menu_left_out(*m));
    v
}

/// Where a row of the Default menu is in Studio.
#[derive(Debug, Clone, PartialEq)]
enum Place {
    /// the menu opens as the same dialog in Studio: every row is there
    SameDialog,
    /// a row of this tab of the sheet
    Tab(&'static str),
    /// a pinned action under this tab
    Foot(&'static str),
    /// on the sheet of another menu that this sheet links to or merges
    Elsewhere(&'static str),
    Missing,
}

/// The rows of `menu` in the Default layout, each with its place in Studio.
fn audit(menu: MenuId, set: fn(&mut Ui)) -> Vec<(Item, String, Place)> {
    let (mut d, mut s) = (default_ui(), studio_ui());
    set(&mut d);
    set(&mut s);
    d.open_menu(menu);
    s.open_menu(menu);
    let rows = d.items(menu);
    let spec = s.sheet_spec(menu);
    rows.into_iter().map(|it| {
        let label = d.item_label(it).to_string();
        let place = match &spec {
            None => if s.items(menu).contains(&it) { Place::SameDialog } else { Place::Missing },
            Some(spec) => spec.tabs.iter().find_map(|t| {
                if t.rows.contains(&it) { Some(Place::Tab(t.name)) } else if t.foot.contains(&it) { Some(Place::Foot(t.name)) } else { None }
            }).unwrap_or_else(|| elsewhere(&s, menu, it)),
        };
        (it, label, place)
    }).collect()
}

/// A row the sheet of `menu` leaves out because Studio has it in another place.
fn elsewhere(s: &Ui, menu: MenuId, it: Item) -> Place {
    use Item::*;
    use MenuId as M;
    match (menu, it) {
        // the sheet of a channel holds the rows of both pages, so it has no link to the second
        (M::Channel(_), Open(M::ChMore(_))) => Place::Elsewhere("the More tab of the same sheet"),
        // the sheet has the levels on a tab of its own
        (M::Measure, Open(M::MeasLevels)) if s.sheet_spec(M::Measure).is_some_and(|sp| sp.tabs.iter().any(|t| t.name == "Levels" && t.holds())) => Place::Elsewhere("the Levels tab of the same sheet"),
        // where the strip docks: Studio has no strip, its measurements are badges and the table (View menu)
        (M::Measure, StripDock) => Place::Elsewhere("View menu: badges, docked or floating table"),
        // the skin and the choice of the bottom bar belong to the Default bars
        (M::Display, Skin | BottomBar) => Place::Elsewhere("Default only: Studio has its own bars"),
        // the SIM sheet holds the rows of the Advanced page on its own tabs
        (M::Sim, Open(M::SimAdvanced)) => Place::Elsewhere("the Levels, Jitter and Spread tabs of the same sheet"),
        // the ten Show switches are one: the switch of the selected slot
        (M::Ref, RefShow(_)) => Place::Elsewhere("Shown: the switch of the selected slot"),
        // Zones are opened from the Menu (Trigger group) in Studio
        (M::Trigger, Open(M::Zone)) => Place::Elsewhere("Menu > Trigger > Zones"),
        // the second source of the tracking cursors is Second source
        (M::Cursor, CurSrcB) if s.sheet_source_has(M::Cursor, CurSecond) => Place::Elsewhere("Second source"),
        // the formula is shown on the row that edits it
        (M::Math, MathExprI) if s.sheet_source_has(M::Math, MathExprEdit) => Place::Elsewhere("on the Edit formula row"),
        // offered while the selected slot is taken; into an empty slot Store does the same
        (M::Ref, RefSaveNext) => Place::Elsewhere("offered while the selected slot is taken"),
        _ => Place::Missing,
    }
}

fn menu_name(u: &Ui, m: MenuId) -> String {
    match m {
        MenuId::ChMore(_) => "Channel > More".into(),
        MenuId::Channel(_) => "Channel".into(),
        m => u.menu_title(m),
    }
}

#[test]
#[ignore]
fn parity_table() {
    let d = default_ui();
    println!("| Default menu | Row | In Studio |");
    println!("|---|---|---|");
    for (menu, states) in menus() {
        let mut seen: Vec<Item> = Vec::new();
        for (state, set) in states {
            for (it, label, place) in audit(menu, set) {
                if seen.contains(&it) { continue; }
                seen.push(it);
                let place = match place {
                    Place::SameDialog => "yes, same dialog".to_string(),
                    Place::Tab(t) => format!("yes, tab {t}"),
                    Place::Foot(t) => format!("yes, pinned under {t}"),
                    Place::Elsewhere(w) => format!("yes: {w}"),
                    Place::Missing => "**no**".to_string(),
                };
                let name = if state.is_empty() || matches!(place.as_str(), "yes, same dialog") { menu_name(&d, menu) } else { format!("{} ({state})", menu_name(&d, menu)) };
                println!("| {name} | {label} [{it:?}] | {place} |");
            }
        }
    }
}

/// Every row of every Default menu is in Studio: on the sheet, in the same dialog, or in the place `elsewhere` names.
#[test]
fn every_row_of_a_default_menu_is_in_studio() {
    let d = default_ui();
    let mut missing = Vec::new();
    for (menu, states) in menus() {
        for (state, set) in states {
            for (it, label, place) in audit(menu, set) {
                if place == Place::Missing { missing.push(format!("{} {state}: {label} ({it:?})", menu_name(&d, menu))); }
            }
        }
    }
    missing.dedup();
    assert!(missing.is_empty(), "rows of the Default menus that Studio has not:\n{}", missing.join("\n"));
}

impl Ui {
    /// The Studio sheet of `menu` has the row `it` (in the state the menu is in).
    fn sheet_source_has(&self, menu: MenuId, it: Item) -> bool {
        self.sheet_spec(menu).is_some_and(|sp| sp.tabs.iter().any(|t| t.rows.contains(&it) || t.foot.contains(&it)))
    }
}

fn tap_in(u: &mut Ui, r: Rect) {
    u.tap(r.x + r.w / 2, r.y + r.h / 2);
    for _ in 0..2 { u.step(0.05); }
}

/// OOS_SHOTS=<dir> writes a picture of each state the tests stop at.
fn shot(u: &mut Ui, name: &str) {
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    for _ in 0..4 { u.step(0.05); }
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        std::fs::write(std::path::Path::new(&dir).join(format!("parity-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
}

/// What the Default header opens, the Studio header opens: the read-out is a button that opens Acquire, and Acquire
/// leads on to Horizontal.
#[test]
fn the_read_out_of_the_studio_bar_is_a_button_that_opens_acquire() {
    let mut u = studio_ui();
    let t = u.studio_top();
    let (chip, caption) = u.studio_acq_chip(&t);
    assert!(chip.w >= 120 && chip.h >= 30 && chip.x >= t.single.right() && chip.right() <= t.acq.right(), "{chip:?} in {:?}", t.acq);
    assert!(u.fonts().mb.width(&caption) + 30 <= chip.w, "the text and its chevron fit the button: {caption:?} in {chip:?}");
    // the mode follows the rate and the depth, as in the Default bar
    assert!(caption.ends_with("\u{b7} Normal"), "{caption}");
    shot(&mut u, "top-live");
    tap_in(&mut u, chip);
    assert!(u.menu_open && u.menu == MenuId::Acquire && u.sheet_active());
    let spec = u.sheet_spec(MenuId::Acquire).unwrap();
    assert!(spec.tabs[0].rows.contains(&Item::AcqDepth) && spec.tabs[0].rows.contains(&Item::AcqMode), "Memory and Mode: {:?}", spec.tabs[0].rows);
    shot(&mut u, "acquire");
    // the way on to Horizontal is pinned under the rows
    let g = u.sheet_geo().unwrap();
    let foot = g.foot.iter().find(|(it, _)| *it == Item::Open(MenuId::Horizontal)).expect("Horizontal under Acquire").1;
    tap_in(&mut u, foot);
    assert!(u.menu_open && u.menu == MenuId::Horizontal);
    shot(&mut u, "horizontal");
    u.close_menu();
    // and the time pill opens it directly
    let b = u.studio_bottom();
    tap_in(&mut u, b.time);
    assert!(u.menu_open && u.menu == MenuId::Horizontal);
}

/// The Acquire sheet keeps its place with averaging and segments on (it fell back to the dialog with more rows than
/// a tab holds), and says where aliasing starts.
#[test]
fn the_acquire_sheet_holds_every_row_in_every_state() {
    let states: [(&str, fn(&mut Ui)); 3] = [("normal", |_| {}), ("average", |u| u.m.acq = AcqMode::Average), ("segments", |u| u.m.segments = 10)];
    for (name, set) in states {
        let mut u = studio_ui();
        set(&mut u);
        u.open_menu(MenuId::Acquire);
        assert!(u.sheet_active(), "{name}: a sheet");
        for (it, label, place) in audit(MenuId::Acquire, set) {
            assert!(matches!(place, Place::Tab(_) | Place::Foot(_)), "{name}: {label} ({it:?}) is {place:?}");
        }
    }
    let mut u = studio_ui();
    if u.alias_above().is_some() {
        assert!(u.sheet_source_has(MenuId::Acquire, Item::AcqAlias), "the aliasing line");
    }
    u.m.acq = AcqMode::Average;
    u.open_menu(MenuId::Acquire);
    shot(&mut u, "acquire-average");
}

/// The trigger pill names the type and the sweep, as the trigger card of the Default layout does; the time pill the
/// delay or the zoom scale, as the TIME card does.
#[test]
fn the_pills_say_what_the_default_cards_say() {
    for ts in [TextSize::Normal, TextSize::Large, TextSize::Larger] {
        for &t in &crate::model::TRIG_TYPES {
            let mut u = studio_ui();
            u.m.text_size = ts;
            u.m.trig_type = t;
            u.relayout();
            for _ in 0..3 { u.step(0.05); }
            let mut cv = Canvas::new(u.scr.w, u.scr.h);
            u.render(&mut cv);
            if u.studio_pill_two_lines() {
                assert_eq!(u.studio_trig_detail(), format!("{} \u{b7} Auto", t.name()));
                let room = u.studio_bottom().trig.w - 9 - 20;
                assert!(u.fonts().s.width(&u.studio_trig_detail()) <= room, "{ts:?} {t:?}: the second line fits");
            }
            if t == TrigType::Edge || t == TrigType::SetupHold {
                shot(&mut u, &format!("pills-{}-{}", ts.name().to_lowercase(), t.name().to_lowercase().replace('/', "-")));
            }
        }
    }
    let mut u = studio_ui();
    assert!(u.studio_pill_two_lines(), "the normal text size has two lines");
    u.m.trig_mode = TrigMode::Normal;
    assert_eq!(u.studio_trig_detail(), "Edge \u{b7} Normal");
    assert_eq!(u.studio_time_detail(), "delay 0 s");
    u.set_zoom(true);
    assert!(u.studio_time_detail().starts_with("zoom ") && u.studio_time_detail().ends_with("/div"), "{}", u.studio_time_detail());
    // a pill of one line has the type in front: whole, or cut short so the level stays whole
    let f = &u.fonts().sb;
    assert_eq!(Ui::studio_cut(f, "EDGE", 200), "EDGE");
    let room = f.width("SETUP/HOLD") - 4;
    let cut = Ui::studio_cut(f, "SETUP/HOLD", room);
    assert!(cut.ends_with("...") && cut.starts_with("SE") && f.width(&cut) <= room, "{cut}");
    assert_eq!(Ui::studio_cut(f, "SETUP/HOLD", 6), "");
}

/// SIM is an entry of the Menu, not a button of the Studio bar.
#[test]
fn the_studio_bar_has_no_sim_button() {
    let mut u = studio_ui();
    let top: Vec<String> = u.ui_elements().into_iter().filter(|e| e.kind == "TOP").map(|e| e.label).collect();
    assert!(!top.iter().any(|l| l == "SIM"), "{top:?}");
    assert!(u.items(MenuId::Main).contains(&Item::Open(MenuId::Sim)), "SIM is in the Menu");
    let t = u.studio_top();
    tap_in(&mut u, t.gear);
    shot(&mut u, "menu");
}

/// Measure in Studio: Add > Measurement and the Measure key open the quick Add panel; the button beside its close
/// button opens the Measure page of every layout, whose own button leads back; the Menu opens the page.
#[test]
fn the_quick_add_panel_expands_to_the_measure_page_and_back() {
    let mut u = studio_ui();
    u.key(key::MEASURE);
    assert!(u.menu_open && u.menu == MenuId::MeasAdd && u.sheet_active(), "the quick Add panel");
    assert!(u.catalog_bar().is_none());
    shot(&mut u, "measure-quick");
    let (expand, close) = (u.sheet_add_expand(), u.sheet_chrome().2);
    assert!(expand.w >= 44 && expand.h >= 44 && expand.right() <= close.x && expand.right() + 8 >= close.x, "{expand:?} beside {close:?}");
    tap_in(&mut u, expand);
    assert!(u.menu_open && u.menu == MenuId::MeasAdd && !u.sheet_active(), "the Measure page");
    assert!(u.catalog_bar().is_some(), "its list scrolls");
    shot(&mut u, "measure-page");
    // a tile adds its measurement
    let n = u.measurement_count();
    u.catalog_tap_name("Period").unwrap();
    assert_eq!(u.measurement_count(), n + 1);
    // Settings: the page nothing else in Studio leads to
    let settings = u.catalog_settings_rect();
    tap_in(&mut u, settings);
    assert!(u.menu_open && u.menu == MenuId::Measure && u.sheet_active(), "Measure settings, as a sheet");
    shot(&mut u, "measure-settings");
    // Back returns to the page, not to the quick panel
    let back = u.sheet_chrome().1.expect("Back");
    tap_in(&mut u, back);
    assert!(u.menu == MenuId::MeasAdd && !u.sheet_active(), "back on the page");
    // a source is picked on the page, and the page is still there
    u.open_picker(Item::MeasSrcB);
    u.activate(Item::PickOpt(1));
    assert!(u.menu_open && u.menu == MenuId::MeasAdd && !u.sheet_active(), "the page after a pick");
    // the button beside the close button leads back to the quick panel
    let quick = u.catalog_quick_rect().expect("the way back");
    tap_in(&mut u, quick);
    assert!(u.menu_open && u.menu == MenuId::MeasAdd && u.sheet_active(), "the quick Add panel again");
    // closed from the page, Measure opens as the quick panel the next time
    let expand = u.sheet_add_expand();
    tap_in(&mut u, expand);
    u.close_menu();
    u.key(key::MEASURE);
    assert!(u.sheet_active(), "quick again after a close");
    u.close_menu();
    // the Menu opens the page
    u.open_menu(MenuId::Main);
    u.activate(Item::Open(MenuId::Measure));
    assert!(u.menu_open && u.menu == MenuId::MeasAdd && !u.sheet_active(), "Menu > Measure is the page");
    // the Default layout has no such button
    let mut d = default_ui();
    d.key(key::MEASURE);
    assert!(d.catalog_quick_rect().is_none());
}
