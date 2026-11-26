// SPDX-License-Identifier: GPL-2.0-or-later
//! Batch 29: input and limits. The value of a number row opens the numeric pad (Minimal and Studio: oos-plus's on every
//! number row; Default: upstream's on the rows with a unit); every stepper has its limits in the range table
//! (`ranges.rs`) and returns where it started after N presses up and N down; the horizontal delay steps on a grid;
//! SCPI, the stepper and the pad limit a trigger level the same way.
//! OOS_SHOTS=<dir> writes pictures (b29-*).
use super::numpad::NumKey::*;
use super::ranges::{Range, Step};
use super::*;

fn frame(u: &mut Ui) {
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    u.step(0.02);
}

fn shot(u: &mut Ui, name: &str) {
    for _ in 0..4 {
        frame(u);
    }
    let mut cv = Canvas::new(u.scr.w, u.scr.h);
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(std::path::Path::new(&dir).join(format!("b29-{name}.png")), crate::store::png_of(&cv)).unwrap();
    }
}

fn touch(u: &mut Ui, phase: Phase, x: i32, y: i32) {
    u.handle(Ev::Touch { id: 0, phase, x, y });
    u.step(0.01);
}

fn tap_at(u: &mut Ui, x: i32, y: i32) {
    touch(u, Phase::Down, x, y);
    touch(u, Phase::Up, x, y);
    frame(u);
}

fn tap_rect(u: &mut Ui, r: Rect) {
    tap_at(u, r.x + r.w / 2, r.y + r.h / 2);
}

/// A scope with a second channel and a reference in R1, as a bench has it.
fn fresh(layout: Layout) -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    u.m.ch[1].on = true;
    u.refs[0] = Some(RefWave {
        src: 0,
        color: 0,
        label: "R1".into(),
        t0: 0.0,
        dt: 1e-9,
        v: vec![0.0f32; 128].into(),
        vpd: 0.5,
        offset: 0.0,
        on: true,
    });
    u.ref_page.sel = 0;
    for _ in 0..4 {
        u.step(0.05);
    }
    u
}

/// The value of an item as a number: exact where the table has a getter, else read back from what is shown.
fn number(u: &Ui, it: Item) -> Option<f64> {
    u.range_get(it).or_else(|| u.numpad_value(it).map(|v| v.0))
}

/// What a press can be seen by: the text, and the number.
fn seen(u: &Ui, it: Item) -> (String, Option<f64>) {
    (u.value(it), number(u, it))
}

fn same(a: &(String, Option<f64>), b: &(String, Option<f64>), tol: f64) -> bool {
    a.0 == b.0
        && match (a.1, b.1) {
            (Some(x), Some(y)) => (x - y).abs() <= tol,
            _ => true,
        }
}

fn range_of(u: &Ui, it: Item) -> Range {
    u.item_range(it).unwrap_or_else(|| panic!("{it:?} has a range"))
}

fn tol_of(r: &Range) -> f64 {
    match r.step {
        Step::Add(s) => s * 1e-3,
        _ => 0.0,
    }
}

#[test]
fn the_range_table_covers_the_steppers_and_every_entry_is_well_formed() {
    let u = fresh(Layout::Default);
    let items = u.range_items();
    eprintln!("range table: {} items", items.len());
    assert!(items.len() >= 50, "{}", items.len());
    // the items of the function check
    for it in [
        Item::TrigLevel,
        Item::TrigLevel2,
        Item::SpecRef,
        Item::DecThr,
        Item::BusClkThr,
        Item::BusDataThr,
        Item::MathOffset,
        Item::RefSel,
        Item::SimSlot,
        Item::SimPj,
        Item::SpecRbw,
        Item::HDelay,
        Item::ChOffset(0),
    ] {
        assert!(items.contains(&it), "{it:?} is in the table");
    }
    for it in items {
        let r = range_of(&u, it);
        assert!(r.lo.is_finite() && r.hi.is_finite() && r.lo < r.hi, "{it:?}: {r:?}");
        if let Step::Add(s) = r.step {
            assert!(s > 0.0 && s < r.hi - r.lo, "{it:?}: step {s}");
        }
    }
}

#[test]
fn n_presses_up_then_n_down_return_to_the_start_for_every_stepper_of_the_table() {
    let n = 3;
    let count = fresh(Layout::Minimal).range_items().len();
    let mut bad = Vec::new();
    for k in 0..count {
        let it = fresh(Layout::Minimal).range_items()[k];
        let mut done = false;
        // from where it starts; a short range (a count of seven) from one of its ends
        for (pre, dir) in [(0, 1), (0, -1), (-1, 1), (1, -1)] {
            // a fresh scope for each try: a press that ends at a limit leaves nothing behind
            let mut u = fresh(Layout::Minimal);
            for _ in 0..if pre == 0 { 0 } else { 600 } {
                u.adjust(it, pre);
            }
            let tol = tol_of(&range_of(&u, it));
            // onto the grid or the ladder of the item first
            u.adjust(it, dir);
            u.adjust(it, -dir);
            let start = seen(&u, it);
            let mut moved = true;
            let mut last = start.clone();
            for _ in 0..n {
                u.adjust(it, dir);
                let now = seen(&u, it);
                if same(&now, &last, tol) {
                    moved = false;
                }
                last = now;
            }
            if !moved {
                continue;
            }
            for _ in 0..n {
                u.adjust(it, -dir);
            }
            let back = seen(&u, it);
            if !same(&back, &start, tol) {
                bad.push(format!("{it:?}: {start:?} -> {last:?} -> {back:?}"));
            }
            done = true;
            break;
        }
        if !done {
            bad.push(format!("{it:?}: moves in neither direction"));
        }
    }
    assert!(bad.is_empty(), "{} of {count}:\n{}", bad.len(), bad.join("\n"));
}

#[test]
fn two_hundred_presses_either_way_stay_inside_the_range() {
    let count = fresh(Layout::Minimal).range_items().len();
    let mut bad = Vec::new();
    for k in 0..count {
        let mut u = fresh(Layout::Minimal);
        let it = u.range_items()[k];
        let r = range_of(&u, it);
        let exact = u.range_get(it).is_some();
        // a number read back from the text is rounded to what is shown
        let slack = if exact { r.hi.abs().max(r.lo.abs()) * 1e-5 + 1e-12 } else { (r.hi - r.lo) * 0.01 };
        let mut check = |u: &Ui, what: &str| {
            if let Some(v) = number(u, it) {
                if v < r.lo - slack || v > r.hi + slack {
                    bad.push(format!("{it:?} {what}: {v} not in {}..{}", r.lo, r.hi));
                }
            }
        };
        for _ in 0..200 {
            u.adjust(it, 1);
        }
        check(&u, "after 200 up");
        for _ in 0..400 {
            u.adjust(it, -1);
        }
        check(&u, "after 400 down");
        for _ in 0..200 {
            u.adjust(it, 1);
        }
        check(&u, "after 200 up again");
        // the knob turns faster: four at a time
        for _ in 0..200 {
            u.adjust(it, 4);
        }
        check(&u, "after 200 knob turns up");
        for _ in 0..400 {
            u.adjust(it, -4);
        }
        check(&u, "after 400 knob turns down");
    }
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

#[test]
fn the_trigger_level_stops_at_the_screen_and_does_not_wrap() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        let vpd = u.trigger_vpd(false) as f64;
        for _ in 0..200 {
            u.adjust(Item::TrigLevel, 1);
        }
        assert!((u.m.trig_level as f64 - 5.0 * vpd).abs() < 1e-5, "{layout:?}: {} at {vpd} V/div", u.m.trig_level);
        for _ in 0..400 {
            u.adjust(Item::TrigLevel, -1);
        }
        assert!((u.m.trig_level as f64 + 5.0 * vpd).abs() < 1e-5, "{layout:?}: {}", u.m.trig_level);
        // other limits of the function check
        for _ in 0..200 {
            u.adjust(Item::MathOffset, 1);
        }
        assert_eq!(u.m.math_offset, 5.0);
        for _ in 0..200 {
            u.adjust(Item::SpecRef, 1);
        }
        assert_eq!(u.m.spec_ref, ranges::SPEC_REF_DBV.1);
        let dvpd = u.decoder_vpd();
        for _ in 0..200 {
            u.adjust(Item::DecThr, -1);
        }
        assert!((u.m.dec_thr + 5.0 * dvpd).abs() < 1e-4, "{layout:?}: {} at {dvpd}", u.m.dec_thr);
        for it in [Item::BusClkThr, Item::BusDataThr] {
            let hi = range_of(&u, it).hi as f32;
            for _ in 0..400 {
                u.adjust(it, 1);
            }
            assert!(number(&u, it).unwrap() <= hi as f64 + 1e-5 && hi < 40.0, "{it:?}: {hi}");
        }
        // the slots are numbers, not loops
        u.m.sim_slot = NREF - 1;
        u.adjust(Item::SimSlot, 1);
        assert_eq!(u.m.sim_slot, NREF - 1);
        u.m.sim_slot = 0;
        u.adjust(Item::SimSlot, -1);
        assert_eq!(u.m.sim_slot, 0);
    }
}

#[test]
fn the_picoseconds_of_the_simulator_ladders_step_one_rung_at_a_time() {
    // an absolute tolerance of a picosecond swallowed the rungs of the jitter ladders: 2 ps went to 1 ps
    for (start, up, down) in [
        (0.0, 0.5e-12, 0.0),
        (0.5e-12, 1e-12, 0.0),
        (1e-12, 2.5e-12, 0.5e-12),
        (2.5e-12, 5e-12, 1e-12),
        (5e-12, 10e-12, 2.5e-12),
    ] {
        let u = eng_ladder(start, 1, 0.5e-12, 50e-12, true);
        let d = eng_ladder(start, -1, 0.5e-12, 50e-12, true);
        assert!((u - up).abs() < 1e-18, "{start} up {u}");
        assert!((d - down).abs() < 1e-18, "{start} down {d}");
    }
    let mut u = fresh(Layout::Default);
    u.m.sim_pj = 5e-12;
    u.adjust(Item::SimPj, 1);
    u.adjust(Item::SimPj, -1);
    assert!((u.m.sim_pj - 5e-12).abs() < 1e-18, "{}", u.m.sim_pj);
}

#[test]
fn the_horizontal_delay_steps_on_a_grid_and_comes_back_exactly_at_every_time_base() {
    for ts in 0..tscales().len() {
        let mut u = fresh(Layout::Minimal);
        u.m.tscale = ts;
        u.m.tfine = 1.0;
        u.m.delay = 0.0;
        let step = u.m.time_per_div() as f64 / 25.0;
        for n in [1, 3, 127] {
            for _ in 0..n {
                u.adjust(Item::HDelay, 1);
            }
            let up = u.m.delay;
            assert!((up as f64 - n as f64 * step).abs() <= step * 1e-3, "tscale {ts}: {n} up is {up}");
            for _ in 0..n {
                u.adjust(Item::HDelay, -1);
            }
            assert_eq!(u.m.delay, 0.0, "tscale {ts}: {n} up and {n} down");
            // and the other way round
            for _ in 0..n {
                u.adjust(Item::HDelay, -1);
            }
            for _ in 0..n {
                u.adjust(Item::HDelay, 1);
            }
            assert_eq!(u.m.delay, 0.0, "tscale {ts}: {n} down and {n} up");
        }
        // from another point of the grid
        for _ in 0..40 {
            u.adjust(Item::HDelay, 1);
        }
        let from = u.m.delay;
        for _ in 0..127 {
            u.adjust(Item::HDelay, -1);
        }
        for _ in 0..127 {
            u.adjust(Item::HDelay, 1);
        }
        assert_eq!(u.m.delay, from, "tscale {ts}");
        // knob turns of four at a time and single presses meet
        u.m.delay = 0.0;
        u.adjust(Item::HDelay, 4);
        for _ in 0..4 {
            u.adjust(Item::HDelay, -1);
        }
        assert_eq!(u.m.delay, 0.0, "tscale {ts}");
    }
}

#[test]
fn the_delay_stays_within_fifty_screens_in_minimal_and_studio() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        let hi = range_of(&u, Item::HDelay).hi;
        for _ in 0..40_000 {
            u.adjust(Item::HDelay, 1);
        }
        assert!(u.m.delay as f64 <= hi * (1.0 + 1e-6), "{layout:?}: {}", u.m.delay);
        assert!(u.m.delay as f64 >= hi * (1.0 - 1e-6), "{layout:?}: reached the limit {hi}, at {}", u.m.delay);
    }
}

// ------------------------------------------------------------------------------------------------ the pad from a dialog

/// The control of a row of the open menu: of the classic dialog, or of the sheet (its tabs tried in turn).
fn ctrl_of(u: &mut Ui, it: Item) -> Rect {
    if u.sheet_active() {
        let tabs = u.sheet_geo().expect("a sheet").tabs.len().max(1);
        for i in 0..tabs {
            let g = u.sheet_geo().expect("a sheet");
            if let Some(t) = g.tabs.get(i) {
                tap_rect(u, *t);
            }
            let g = u.sheet_geo().expect("a sheet");
            if let Some(row) = g.rows.iter().find(|r| r.it == it) {
                return row.ctrl;
            }
        }
        panic!("{it:?} is a row of the sheet");
    }
    let rows = u.dlg_layout().expect("a dialog").rows;
    rows.into_iter().find(|r| r.it == it).unwrap_or_else(|| panic!("{it:?} is a row")).ctrl
}

/// A tap on the value (the middle) of a row of the open menu.
fn tap_value(u: &mut Ui, it: Item) {
    let mid = Ui::value_parts(ctrl_of(u, it)).1;
    tap_rect(u, mid);
}

/// Closes Default's pad (upstream's picker) with Cancel.
fn cancel_row_pad(u: &mut Ui) {
    let k = u.row_pad_keys().iter().position(|k| k == "Cancel").expect("a Cancel key");
    u.activate(Item::PickOpt(k));
}

fn press(u: &mut Ui, keys: &[super::numpad::NumKey]) {
    for &k in keys {
        u.numpad_key(k);
    }
    frame(u);
}

/// Default: upstream's pad (issue #22) from the classic dialog; the value lands where SCPI puts it.
#[test]
fn the_value_of_a_number_row_opens_upstreams_pad_in_default() {
    let mut u = fresh(Layout::Default);
    u.open_menu(MenuId::Trigger);
    frame(&mut u);
    let (minus, mid, plus) = Ui::value_parts(ctrl_of(&mut u, Item::TrigLevel));
    let before = u.m.trig_level;
    tap_rect(&mut u, plus);
    assert!(u.m.trig_level > before && u.pick.is_none(), "+ steps");
    tap_rect(&mut u, minus);
    assert!((u.m.trig_level - before).abs() < 1e-6, "- steps");
    tap_rect(&mut u, mid);
    assert_eq!(u.row_pad.target, Some(Item::TrigLevel));
    assert!(u.pick.is_some_and(|p| p.it == Item::NumEntry) && u.numpad.is_none(), "upstream's pad, not oos-plus's");
    let keys = u.row_pad_keys();
    for k in ["0", ".", "7", "V"] {
        let i = keys.iter().position(|x| x == k).unwrap_or_else(|| panic!("no key {k} in {keys:?}"));
        u.activate(Item::PickOpt(i));
    }
    assert!(u.pick.is_none() && u.row_pad.target.is_none() && u.menu == MenuId::Trigger);
    assert!((u.m.trig_level - 0.7).abs() < 1e-6, "{}", u.m.trig_level);
    // the horizontal delay: in seconds
    u.open_menu(MenuId::Horizontal);
    frame(&mut u);
    tap_value(&mut u, Item::HDelay);
    assert_eq!(u.row_pad.target, Some(Item::HDelay));
    assert!(u.row_pad_keys().iter().any(|k| k == "\u{b5}s"));
    cancel_row_pad(&mut u);
    assert!(u.pick.is_none() && u.row_pad.target.is_none());
}

#[test]
fn the_value_of_a_number_row_opens_the_pad_in_minimal_and_studio_and_enter_applies() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        u.open_menu(MenuId::Trigger);
        frame(&mut u);
        assert!(u.numpad.is_none());
        let (minus, mid, plus) = Ui::value_parts(ctrl_of(&mut u, Item::TrigLevel));
        // the buttons still step
        let before = u.m.trig_level;
        tap_rect(&mut u, plus);
        assert!(u.m.trig_level > before && u.numpad.is_none(), "{layout:?}: + steps");
        tap_rect(&mut u, minus);
        assert!((u.m.trig_level - before).abs() < 1e-6, "{layout:?}: - steps");
        // the value opens the pad, for this row, in volts
        tap_rect(&mut u, mid);
        let pad = u.numpad.clone().unwrap_or_else(|| panic!("{layout:?}: the pad opens"));
        assert_eq!((pad.it, pad.unit.as_str()), (Item::TrigLevel, "V"));
        let (lo, hi) = u.numpad_limits(Item::TrigLevel).unwrap();
        assert!(lo < 0.0 && hi > 0.0, "{lo} {hi}");
        press(&mut u, &[Digit(0), Dot, Digit(7), Enter]);
        assert!(u.numpad.is_none());
        assert!((u.m.trig_level - 0.7).abs() < 1e-6, "{layout:?}: {}", u.m.trig_level);
        // a number beyond the limit is limited, and says so
        tap_rect(&mut u, mid);
        press(&mut u, &[Digit(9), Digit(9), Enter]);
        assert!((u.m.trig_level as f64 - hi).abs() < 1e-5, "{layout:?}: {}", u.m.trig_level);
        // Cancel changes nothing
        let kept = u.m.trig_level;
        tap_rect(&mut u, mid);
        press(&mut u, &[Digit(1), Cancel]);
        assert!(u.numpad.is_none());
        assert_eq!(u.m.trig_level, kept);
    }
}

#[test]
fn other_number_rows_take_the_pad_too_with_their_own_unit() {
    for layout in [Layout::Minimal, Layout::Studio] {
        let mut u = fresh(layout);
        // Intensity: a whole number, in percent, through the stepper's own path
        u.open_menu(MenuId::Display);
        frame(&mut u);
        tap_value(&mut u, Item::Intensity);
        assert_eq!(u.numpad.as_ref().map(|p| p.it), Some(Item::Intensity), "{layout:?}");
        press(&mut u, &[Digit(7), Digit(0), Enter]);
        assert_eq!(u.m.intensity, 70, "{layout:?}");
        // the horizontal delay: seconds, through the table
        u.open_menu(MenuId::Horizontal);
        frame(&mut u);
        tap_value(&mut u, Item::HDelay);
        let pad = u.numpad.clone().expect("the pad opens on Delay");
        assert_eq!((pad.it, pad.unit.as_str()), (Item::HDelay, "s"));
        press(&mut u, &[Digit(2), Prefix(-6)]);
        assert!((u.m.delay - 2e-6).abs() < 1e-12, "{layout:?}: {}", u.m.delay);
    }
}

/// Minimal and Studio: every number row of a menu, classic dialog or sheet, opens oos-plus's pad. Default: the rows
/// with a unit open upstream's pad, the other number rows open nothing (upstream's issue #22).
#[test]
fn every_number_row_of_the_classic_menus_opens_the_pad() {
    let menus = [
        MenuId::Channel(0),
        MenuId::ChMore(0),
        MenuId::Horizontal,
        MenuId::Trigger,
        MenuId::Acquire,
        MenuId::MeasLevels,
        MenuId::MeasFall,
        MenuId::Dvm,
        MenuId::Cursor,
        MenuId::Display,
        MenuId::Utility,
        MenuId::Math,
        MenuId::Ref,
        MenuId::Decode,
        MenuId::Bus,
        MenuId::Storage,
        MenuId::Mask,
        MenuId::Zone,
        MenuId::Spectrum,
        MenuId::Eye,
        MenuId::Power,
        MenuId::Magnetic,
        MenuId::Inverter,
        MenuId::Dpt,
        MenuId::Bode,
        MenuId::Sim,
        MenuId::SimAdvanced,
        MenuId::Afg,
        MenuId::BwTest,
        MenuId::Logger,
    ];
    let mut rows = 0;
    let (mut up_rows, mut up_plain) = (0, 0);
    let mut missed = Vec::new();
    for layout in [Layout::Default, Layout::Minimal, Layout::Studio] {
        for &menu in &menus {
            let mut u = fresh(layout);
            u.open_menu(menu);
            frame(&mut u);
            if u.sheet_active() {
                let tabs = u.sheet_geo().map_or(1, |g| g.tabs.len().max(1));
                for i in 0..tabs {
                    if let Some(t) = u.sheet_geo().and_then(|g| g.tabs.get(i).copied()) {
                        tap_rect(&mut u, t);
                    }
                    let Some(g) = u.sheet_geo() else { continue };
                    for row in g.rows.iter().filter(|r| r.kind == super::sheet::RowKind::Number) {
                        let mid = Ui::value_parts(row.ctrl).1;
                        let numeric = u.numpad_value(row.it).is_some() && u.item_blocked(row.it).is_none();
                        if !numeric || !g.r.contains(mid.x + mid.w / 2, mid.y + mid.h / 2) {
                            continue;
                        }
                        u.numpad = None;
                        tap_rect(&mut u, mid);
                        rows += 1;
                        if u.numpad.as_ref().map(|p| p.it) != Some(row.it) {
                            missed.push(format!("{layout:?} {menu:?} {:?}", row.it));
                        }
                        u.numpad = None;
                    }
                }
                continue;
            }
            let Some(d) = u.dlg_layout() else { continue };
            for row in d.rows.iter().filter(|r| matches!(r.ctl, Ctl::Value)) {
                let mid = Ui::value_parts(row.ctrl).1;
                let numeric = u.numpad_value(row.it).is_some();
                if !numeric || !d.r.contains(mid.x + mid.w / 2, mid.y + mid.h / 2) {
                    continue;
                }
                // from a clean state each time: a row opened before may have moved the dialog
                u.numpad = None;
                tap_rect(&mut u, mid);
                if layout == Layout::Default {
                    let want = u.row_pad_unit(row.it).map(|_| row.it);
                    if want.is_some() {
                        up_rows += 1
                    } else {
                        up_plain += 1
                    }
                    if u.row_pad.target != want || u.numpad.is_some() {
                        missed.push(format!("{layout:?} {menu:?} {:?}: upstream's pad {want:?}", row.it));
                    }
                    if want.is_some() {
                        cancel_row_pad(&mut u);
                    }
                } else {
                    rows += 1;
                    if u.numpad.as_ref().map(|p| p.it) != Some(row.it) {
                        missed.push(format!("{layout:?} {menu:?} {:?}", row.it));
                    }
                }
                u.numpad = None;
                u.menu = menu;
                u.menu_open = true;
            }
        }
    }
    eprintln!("number rows that opened the pad: {rows}; Default: {up_rows} with upstream's pad, {up_plain} without");
    assert!(rows >= 60, "only {rows} number rows found");
    assert!(up_rows >= 5 && up_plain >= 30, "Default: {up_rows} rows with a unit, {up_plain} without");
    assert!(missed.is_empty(), "{} rows did not do what their layout does: {missed:?}", missed.len());
}

#[test]
fn the_value_of_a_text_row_does_not_open_the_pad() {
    let mut u = fresh(Layout::Minimal);
    u.open_menu(MenuId::Trigger);
    frame(&mut u);
    // the source is a picker: its middle opens the list, not the pad
    tap_value(&mut u, Item::TrigSrc);
    assert!(u.numpad.is_none());
}

// ------------------------------------------------------------------------------------------------ SCPI and the pad

#[test]
fn scpi_takes_a_trigger_level_as_it_is_and_the_steppers_of_minimal_stop_at_the_screen() {
    let mut u = fresh(Layout::Minimal);
    let hi = range_of(&u, Item::TrigLevel).hi;
    let mut scpi = crate::scpi::Scpi::open(0);
    let cv = Canvas::new(W, H);
    let mut line = |u: &mut Ui, c: &str| String::from_utf8(scpi.exec_line(u, &cv, c)).unwrap();
    // upstream's setters take the number they are sent, in every layout
    for header in [":TRIGger:EDGE:LEVel", ":TRIGger:LEVel"] {
        line(&mut u, &format!("{header} 99"));
        assert!((u.m.trig_level - 99.0).abs() < 1e-5, "{header}: {}", u.m.trig_level);
        line(&mut u, &format!("{header} -99"));
        assert!((u.m.trig_level + 99.0).abs() < 1e-5, "{header}: {}", u.m.trig_level);
        line(&mut u, &format!("{header} 0.25"));
        assert!((u.m.trig_level - 0.25).abs() < 1e-6, "{header}");
    }
    let q = line(&mut u, ":TRIGger:EDGE:LEVel?");
    assert!(q.trim().parse::<f64>().is_ok_and(|v| (v - 0.25).abs() < 1e-6), "{q}");
    // the stepper and the pad of Minimal stop at the screen
    for _ in 0..300 {
        u.adjust(Item::TrigLevel, 1);
    }
    assert!((u.m.trig_level as f64 - hi).abs() < 1e-5, "{}", u.m.trig_level);
    assert_eq!(u.numpad_apply(Item::TrigLevel, 1e6), hi as f32 as f64);
    // the lower level of a runt trigger
    u.set_trig_type(TrigType::Runt);
    line(&mut u, ":TRIGger:RUNT:LOWer -50");
    assert!((u.m.trig_level2 + 50.0).abs() < 1e-5, "{}", u.m.trig_level2);
}

// Break it catches: the Default layout stopped the levels, offsets and the delay at five divisions, which upstream's
// steppers do not, and SCPI answered 5 for `:TRIGger:EDGE:LEVel 100`.
#[test]
fn the_steppers_of_default_add_their_step_without_a_limit_as_upstreams() {
    let mut u = fresh(Layout::Default);
    let (vpd, l0) = (u.trigger_vpd(false), u.m.trig_level);
    for _ in 0..200 {
        u.adjust(Item::TrigLevel, 1);
    }
    assert!((u.m.trig_level - l0 - 200.0 * vpd / 25.0).abs() < 1e-3, "{} at {vpd} V/div", u.m.trig_level);
    let ovpd = u.m.ch[0].volts_per_div();
    let o0 = u.m.ch[0].offset;
    for _ in 0..10 {
        u.adjust(Item::ChOffset(0), 1);
    }
    assert!((u.m.ch[0].offset - o0 - 10.0 * ovpd / 25.0).abs() < 1e-4, "{}", u.m.ch[0].offset);
    let tdiv = u.m.time_per_div();
    let (d0, m0) = (u.m.delay, u.m.math_offset);
    for _ in 0..30 {
        u.adjust(Item::HDelay, 1);
    }
    assert!((u.m.delay - d0 - 30.0 * tdiv / 25.0).abs() < 1e-3 * tdiv, "{}", u.m.delay);
    for _ in 0..200 {
        u.adjust(Item::MathOffset, 1);
    }
    assert!((u.m.math_offset - m0 - 20.0).abs() < 1e-3, "{}", u.m.math_offset);
    // upstream's own limits stay: the deskew and the bus thresholds
    u.adjust(Item::ChSkew(0), 100_000);
    assert_eq!(u.m.ch[0].skew, SKEW_MAX);
    for _ in 0..100_000 {
        u.adjust(Item::BusClkThr, 1);
    }
    assert!((u.m.bus.clk_thr - 40.0).abs() < 1e-3, "{}", u.m.bus.clk_thr);
}

// ------------------------------------------------------------------------------------------------ pictures

#[test]
fn pictures_of_the_pad_from_a_dialog_and_a_level_at_its_limit() {
    let mut u = fresh(Layout::Minimal);
    u.open_menu(MenuId::Trigger);
    frame(&mut u);
    tap_value(&mut u, Item::TrigLevel);
    assert!(u.numpad.is_some());
    shot(&mut u, "minimal-numpad-from-dialog");
    u.numpad = None;
    for _ in 0..200 {
        u.adjust(Item::TrigLevel, 1);
    }
    frame(&mut u);
    shot(&mut u, "trigger-level-at-limit");
    let mut d = fresh(Layout::Default);
    d.open_menu(MenuId::Horizontal);
    frame(&mut d);
    tap_value(&mut d, Item::HDelay);
    assert_eq!(d.row_pad.target, Some(Item::HDelay), "upstream's pad");
    shot(&mut d, "default-numpad-delay");
}
