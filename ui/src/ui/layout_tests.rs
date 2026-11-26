// SPDX-License-Identifier: GPL-2.0-or-later
//! Display > Layout: the stock OOS bars (Default), the compact OOS-plus bars (Minimal) and Studio.
use super::studio::{MeasKey, StudioView};
use super::*;

fn shown(layout: Layout) -> (Ui, Canvas) {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = layout;
    for _ in 0..8 {
        u.step(0.05);
    }
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    // OOS_SHOTS=<dir> writes the screens for a look
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        let name = format!("layout-{}.png", layout.name().to_ascii_lowercase());
        std::fs::write(std::path::Path::new(&dir).join(name), crate::store::png_of(&cv)).unwrap();
    }
    (u, cv)
}

#[test]
fn default_layout_has_the_stock_bars_and_minimal_the_compact_ones() {
    let (d, _) = shown(Layout::Default);
    let (m, _) = shown(Layout::Minimal);
    assert_eq!(d.frame(), Frame::of(d.scr));
    assert!(m.frame().bottom.h < d.frame().bottom.h, "the compact bar gives the plot more room");
    assert!(m.plot_rect().h > d.plot_rect().h);
    // the top bar of upstream OOS: Single beside the run pill, and the two buttons of ADR 0076
    assert!(d.single_btn().w > 0);
    let names = |u: &Ui| u.top_buttons().into_iter().map(|b| b.1).collect::<Vec<_>>();
    assert_eq!(names(&d), ["Display", "Storage"]);
    assert!(names(&m).iter().any(|n| *n == "SIM"));
    assert!(m.single_btn().w > 0);
}

#[test]
fn default_layout_cards_open_their_menus() {
    let (mut u, _) = shown(Layout::Default);
    let ch2 = u.cards().0[1];
    u.tap(ch2.x + ch2.w / 2, ch2.y + ch2.h / 2);
    assert!(u.m.ch[1].on, "a tap on an off channel card switches it on");
    assert_eq!(u.menu, MenuId::Channel(1));
    u.close_menu();
    // the cards are laid out again with CH2 on; the top half of TIME opens Horizontal
    let time = u.cards().1;
    u.tap(time.x + time.w / 2, time.y + 10);
    assert!(u.menu_open && u.menu == MenuId::Horizontal);
}

#[test]
fn layout_is_a_display_setting_kept_in_the_setup() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    assert_eq!(u.m.layout, Layout::Default, "a new setup starts with the stock bars");
    assert!(!u.m.dots, "samples are joined as lines by default");
    assert!(u.items(MenuId::Display).contains(&Item::Layout));
    for layout in LAYOUTS {
        u.m.layout = layout;
        let text = crate::store::setup_text(&u.m);
        assert!(text.contains(&format!("layout = {}", layout.name().to_ascii_lowercase())));
        assert_eq!(crate::store::parse_setup(&text).unwrap().layout, layout);
    }
    // setups from before Studio hold `default` or `minimal`; Minimal is gone and opens as Studio, as Overlay
    u.m.layout = Layout::Studio;
    let old = crate::store::setup_text(&u.m).replace("layout = studio", "layout = minimal");
    let loaded = crate::store::parse_setup(&old).unwrap();
    assert_eq!((loaded.layout, loaded.studio_overlay), (Layout::Studio, true));
    assert!(crate::store::setup_text(&loaded).contains("layout = studio"), "written back as studio");
    assert!(crate::store::parse_setup(&old.replace("layout = minimal", "layout = wide")).is_err());
}

#[test]
fn a_stored_minimal_setup_keeps_its_own_traces_line() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.studio_overlay = false;
    let text = crate::store::setup_text(&u.m).replace("layout = studio", "layout = minimal");
    assert!(!text.contains("studio_traces"), "a Stacked set-up writes no traces line");
    assert!(crate::store::parse_setup(&text).unwrap().studio_overlay, "no line: Minimal's one graticule");
    let text = format!(
        "{text}studio_traces = stacked
"
    );
    assert!(!crate::store::parse_setup(&text).unwrap().studio_overlay, "an explicit line wins");
}

#[test]
fn the_display_menu_steps_through_the_two_layouts() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    let seen: Vec<_> = (0..2)
        .map(|_| {
            u.adjust(Item::Layout, 1);
            u.m.layout
        })
        .collect();
    assert_eq!(seen, [Layout::Studio, Layout::Default]);
    assert_eq!(u.options(Item::Layout), ["Default", "Studio"]);
    u.select_option(Item::Layout, "Studio");
    assert_eq!(u.m.layout, Layout::Studio);
    assert_eq!(u.value(Item::Layout), "Studio");
    u.step(0.05);
    let mut cv = Canvas::new(W, H);
    u.render(&mut cv);
    assert!(u.stacked(), "Studio draws its sources as lanes");
}

/// Studio with CH1 and CH2 on, as the mocks show it.
fn studio() -> Ui {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.studio_detail = false;
    u.m.studio_zoom_tab = false;
    u.m.ch[1].on = true;
    u.studio.enter = true;
    for _ in 0..8 {
        u.step(0.05);
    }
    u
}

fn shot(u: &mut Ui, name: &str) {
    let mut cv = Canvas::new(W, H);
    // the maps are filled for the lanes that the first frame lays out
    u.render(&mut cv);
    for _ in 0..6 {
        u.step(0.05);
    }
    u.render(&mut cv);
    if let Some(dir) = std::env::var_os("OOS_SHOTS") {
        std::fs::write(std::path::Path::new(&dir).join(format!("studio-{name}.png")), crate::store::png_of(&cv))
            .unwrap();
    }
}

fn tap_in(u: &mut Ui, r: Rect) {
    u.tap(r.x + r.w / 2, r.y + r.h / 2);
}

/// The links of the Studio bar: LAN as a small pill, and USB beside it while a stick is mounted, as the Default bar
/// has a USB pill. They take their room from the read-out and overlap nothing; a tap on LAN opens Network, one on
/// USB opens Storage.
#[test]
fn studio_top_bar_has_a_lan_pill_and_a_usb_pill_while_a_stick_is_in() {
    let mut u = studio();
    let t = u.studio_top();
    assert!(t.lan.w > 0 && t.lan.w < 70 && t.usb.w == 0 && t.wlan.w == 0, "{:?}", (t.lan, t.usb, t.wlan));
    let acq = t.acq.w;
    u.stick.mounted = true;
    let t = u.studio_top();
    assert!(t.usb.w > 0 && t.usb.right() <= t.lan.x && t.acq.w < acq, "{:?}", (t.usb, t.lan, t.acq));
    let all = [t.run, t.acq, t.usb, t.lan, t.add, t.cursors, t.display, t.storage, t.gear];
    for (i, a) in all.iter().enumerate() {
        assert!(a.x >= 0 && a.right() <= W && a.y >= 0 && a.bottom() <= TOP.h, "chip {i} inside the bar");
        for b in &all[i + 1..] {
            assert!(!overlaps(*a, *b), "chips {i} do not overlap: {a:?} {b:?}");
        }
    }
    shot(&mut u, "01-top-links");
    tap_in(&mut u, t.usb);
    assert!(u.menu_open && u.menu == MenuId::Storage);
    u.close_menu();
    tap_in(&mut u, t.lan);
    assert!(u.menu_open && u.menu == MenuId::Network);
    u.close_menu();
}

#[test]
fn studio_top_bar_has_run_single_state_acquisition_and_the_menu_chips() {
    let mut u = studio();
    shot(&mut u, "01-top");
    let t = u.studio_top();
    let all = [t.run, t.acq, t.add, t.storage, t.gear];
    assert_eq!(t.single.w, 0, "Single is the key of the front panel");
    for (i, a) in all.iter().enumerate() {
        assert!(a.x >= 0 && a.right() <= W && a.y >= 0 && a.bottom() <= TOP.h, "chip {i} inside the bar");
        for b in &all[i + 1..] {
            assert!(!overlaps(*a, *b), "chips do not overlap");
        }
    }
    assert_eq!(u.single_btn(), t.single);
    assert_eq!(u.run_pill(), t.run);
    // Run and Stop toggle the acquisition, Single arms one
    let run = u.m.run;
    tap_in(&mut u, t.run);
    assert_ne!(u.m.run, run);
    u.studio_single();
    assert_eq!(u.m.trig_mode, TrigMode::Single);
    // the acquisition chip opens Acquire, the gear the Main menu, Cursors its page
    tap_in(&mut u, t.acq);
    assert_eq!(u.menu, MenuId::Acquire);
    u.close_menu();
    tap_in(&mut u, t.gear);
    assert_eq!(u.menu, MenuId::Main);
    u.close_menu();
    // Cursors: the first tap puts the cursors on (no menu), the next opens their settings (batch 8)
    let t = u.studio_top();
    tap_in(&mut u, t.cursors);
    assert_eq!(u.m.cursor, CursorMode::Track);
    assert!(!u.menu_open);
    tap_in(&mut u, t.cursors);
    assert_eq!(u.menu, MenuId::Cursor);
    u.close_menu();
    // Add opens the grouped Add menu (Measurement first); Measure is its first row
    tap_in(&mut u, t.add);
    assert!(u.studio.add_menu && !u.menu_open);
    let first = u.studio_add_rows()[0];
    assert_eq!(first.0, super::addmenu::AddRow::Measurement);
    tap_in(&mut u, first.1);
    assert_eq!(u.menu, MenuId::MeasAdd);
    u.close_menu();
    // Load file and SIM stay in the menus, not in the bar
    assert!(!u.top_buttons().iter().any(|b| b.1 == "SIM" || b.1.starts_with("Load")));
}

#[test]
fn studio_bottom_bar_has_a_chip_per_channel_the_time_base_and_the_trigger() {
    let mut u = studio();
    shot(&mut u, "02-bottom");
    let b = u.studio_bottom();
    assert_eq!(u.studio_cards().time, b.time);
    // every channel has a chip: wide while it is on, small while it is off
    let mut all: Vec<Rect> = b.ch.iter().copied().filter(|r| r.w > 0).collect();
    assert_eq!(all.len(), 4);
    assert_eq!(all.iter().filter(|r| r.w > 96).count(), 2);
    all.extend([b.add, b.time, b.trig]);
    for (i, a) in all.iter().enumerate() {
        assert!(
            a.w > 0 && a.x >= 0 && a.right() <= W && a.y >= u.frame().bottom.y && a.bottom() <= H,
            "chip {i} inside the bar"
        );
        for o in &all[i + 1..] {
            assert!(!overlaps(*a, *o), "chips do not overlap");
        }
    }
    assert!((60..=96).contains(&b.ch[2].w) && b.ch[3].w == b.ch[2].w, "smaller chips for the channels that are off");
    // an on channel opens its page
    let b = u.studio_bottom();
    tap_in(&mut u, b.ch[0]);
    assert_eq!(u.menu, MenuId::Channel(0));
    u.close_menu();
    tap_in(&mut u, b.time);
    assert_eq!(u.menu, MenuId::Horizontal);
    u.close_menu();
    tap_in(&mut u, b.trig);
    assert_eq!(u.menu, MenuId::Trigger);
    u.close_menu();
    // all four channels on still fit
    for c in 0..n_ch() {
        u.m.ch[c].on = true;
    }
    let b = u.studio_bottom();
    assert!(b.ch[n_ch() - 1].right() < b.time.x && b.trig.right() <= W);
}

#[test]
fn studio_plot_has_lanes_with_channel_badges_the_t_marker_and_cursor_tags() {
    let mut u = studio();
    u.m.cursor = CursorMode::Time;
    u.m.cur_a = 0.62;
    u.m.cur_b = 0.80;
    shot(&mut u, "03-plot");
    // one lane per channel that is on, one under the other, inside the plot under the tag row
    assert_eq!(u.plot.wins.len(), 2);
    assert!(u.plot.wins[0].bottom() <= u.plot.wins[1].y);
    assert!(u.studio_tags_on());
    assert_eq!(
        u.plot.grid.y,
        u.frame().body.y + super::dock::STRIP_H,
        "the strip of the waveform, then the scale: no tag row"
    );
    let t = u.studio_tag_rect(u.studio_x_of_time(0.0));
    assert!(t.y > u.plot.grid.y && t.bottom() < u.plot.grid.y + 26, "T sits inside the scale, on its top edge");
    // the tag row, the lanes and the bars do not overlap
    let top = u.studio_top();
    assert!(u.plot.grid.y - super::dock::STRIP_H >= top.run.bottom());
    assert!(u.plot.grid.bottom() <= u.frame().bottom.y);
    // the T tag sits at the trigger position: 0 s at the centre with no delay
    u.m.delay = 0.0;
    assert_eq!(u.studio_x_of_time(0.0), u.plot.grid.x + u.plot.grid.w / 2);
    // Default and Minimal keep their plot exactly
    u.m.layout = Layout::Minimal;
    assert!(!u.studio_tags_on());
    u.m.layout = Layout::Studio;
    u.m.studio_detail = false;
    u.m.studio_zoom_tab = false;
    u.m.zoom = true;
    assert!(!u.studio_tags_on(), "the zoom view brings its own toolbar");
}

fn badges() -> Ui {
    let mut u = studio();
    u.m.meas.clear();
    u.m.meas2.clear();
    u.m.cat.clear();
    u.m.meas = vec![
        (Meas::Freq, WaveId::Ch(0)),
        (Meas::Vpp, WaveId::Ch(0)),
        (Meas::Vrms, WaveId::Ch(0)),
        (Meas::Rise, WaveId::Ch(0)),
        (Meas::Freq, WaveId::Ch(1)),
        (Meas::Vpp, WaveId::Ch(1)),
    ];
    u.studio.pinned = Some(vec![
        MeasKey::One(Meas::Freq, WaveId::Ch(0)),
        MeasKey::One(Meas::Vpp, WaveId::Ch(0)),
        MeasKey::One(Meas::Vpp, WaveId::Ch(1)),
    ]);
    u.relayout();
    u
}

#[test]
fn studio_pinned_badges_sit_under_the_lanes_and_the_eye_hides_them() {
    let mut u = badges();
    shot(&mut u, "04-badges");
    let row = u.studio_row();
    assert_eq!(row.bottom(), u.frame().bottom.y);
    assert!(u.plot.grid.bottom() <= row.y, "the lanes end above the badges");
    let list = u.studio_badges();
    assert_eq!(list.len(), 3);
    for (_, r) in &list {
        assert!(row.contains(r.x, r.y) && r.right() <= u.studio_list_chip().x, "badge inside the row");
    }
    // the row is paid for by the lanes: hiding the badges gives the height back (mock 02)
    let with = u.plot.grid.h;
    u.studio_hide_badges();
    assert!(u.studio.hide_badges);
    assert_eq!(u.studio_row_h(), 0);
    assert!(u.plot.grid.h > with);
    shot(&mut u, "05-badges-hidden");
}

#[test]
fn studio_badge_popup_configures_unpins_floats_and_removes() {
    let mut u = badges();
    u.studio_set_view(StudioView::Waveform); // batch 22: the row is for a closed panel
    u.step(0.05);
    let vpp = MeasKey::One(Meas::Vpp, WaveId::Ch(0));
    let open = |u: &mut Ui, key: MeasKey| {
        let r = u.studio_badges().into_iter().find(|(k, _)| *k == key).unwrap().1;
        // the first tap shows the measurement on the waveform, the next one on the same badge opens its menu
        tap_in(u, r);
        if u.studio.popup.is_none() {
            assert_eq!(u.studio.mark, Some(key));
            tap_in(u, r);
        }
        assert_eq!(u.studio.popup, Some(key));
        u.studio_popup_rect().unwrap()
    };
    // a tap outside closes it and does nothing else
    let p = open(&mut u, vpp);
    shot(&mut u, "06-badge-actions");
    u.tap(p.x + 2, p.y - 80);
    assert!(u.studio.popup.is_none() && u.m.meas.len() == 6);
    // Configure opens the measurement's own settings
    let p = open(&mut u, vpp);
    tap_in(&mut u, Ui::studio_popup_row(p, 0));
    assert_eq!(u.meas_own.dialog, Some(1));
    u.close_menu();
    // Unpin leaves the measurement in the list
    let p = open(&mut u, vpp);
    tap_in(&mut u, Ui::studio_popup_row(p, 1));
    assert!(!u.studio_is_pinned(vpp) && u.m.meas.contains(&(Meas::Vpp, WaveId::Ch(0))));
    assert_eq!(u.studio_badges().len(), 2);
    // Move to floating panel unpins and shows the panel over the lanes
    let key = MeasKey::One(Meas::Freq, WaveId::Ch(0));
    let p = open(&mut u, key);
    tap_in(&mut u, Ui::studio_popup_row(p, 2));
    assert!(!u.studio_is_pinned(key));
    assert_eq!(u.studio.view, StudioView::Floating);
    // batch 22: the floating panel lists them, so the row is gone; close it to have the row again
    u.studio_set_view(StudioView::Waveform);
    u.step(0.05);
    // Remove takes the measurement out of the list, and its pin with it
    let key = MeasKey::One(Meas::Vpp, WaveId::Ch(1));
    let p = open(&mut u, key);
    tap_in(&mut u, Ui::studio_popup_row(p, 3));
    assert!(!u.m.meas.contains(&(Meas::Vpp, WaveId::Ch(1))) && u.m.meas.len() == 5);
    assert!(u.studio_pinned().is_empty());
}

#[test]
fn studio_pins_survive_other_measurements_coming_and_going() {
    let mut u = badges();
    let freq = MeasKey::One(Meas::Freq, WaveId::Ch(1));
    u.studio_set_pin(freq, true);
    assert_eq!(u.studio_pinned().len(), 4);
    u.remove_measurement(0);
    assert_eq!(u.studio_pinned().len(), 3, "the pin of the removed one is gone, the others stay");
    assert!(u.studio_is_pinned(freq));
    // every measurement is pinned from the start, and one that is added is pinned too
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.layout = Layout::Studio;
    u.m.studio_detail = false;
    u.m.studio_zoom_tab = false;
    assert_eq!(u.studio_pinned().len(), u.studio_keys().len());
    u.studio_set_pin(MeasKey::One(Meas::Vpp, WaveId::Ch(0)), false);
    u.m.meas.push((Meas::Vmax, WaveId::Ch(0)));
    assert!(u.studio_is_pinned(MeasKey::One(Meas::Vmax, WaveId::Ch(0))), "the new one is pinned");
    assert!(!u.studio_is_pinned(MeasKey::One(Meas::Vpp, WaveId::Ch(0))), "the one taken off stays off");
}

/// Twelve measurements on CH1 and CH2, three of them pinned, as in the mocks.
fn twelve() -> Ui {
    let mut u = studio();
    u.m.meas = [Meas::Freq, Meas::Vpp, Meas::Vrms, Meas::Rise, Meas::PDuty, Meas::Vavg]
        .into_iter()
        .map(|k| (k, WaveId::Ch(0)))
        .chain(
            [Meas::Freq, Meas::Vpp, Meas::Vrms, Meas::Rise, Meas::Fall, Meas::Vavg]
                .into_iter()
                .map(|k| (k, WaveId::Ch(1))),
        )
        .collect();
    u.m.meas2.clear();
    u.m.cat.clear();
    u.results.rows.clear();
    u.studio.pinned = None;
    u.relayout();
    u
}

#[test]
fn studio_docked_panel_lists_and_hands_back_the_width() {
    let mut u = twelve();
    let full = u.plot.grid.w;
    u.studio_set_view(StudioView::Docked);
    for _ in 0..4 {
        u.step(0.05);
    }
    shot(&mut u, "08-docked");
    let g = u.studio_panel_geo().expect("docked panel");
    assert_eq!(u.plot.grid.w, full - 274, "the lanes give their width to the panel");
    assert!(g.p.x >= u.plot.grid.right() && g.p.right() <= W && g.p.bottom() <= u.frame().bottom.y && g.p.y >= TOP.h);
    assert!(g.list.bottom() <= g.p.bottom(), "no pager and no footer: the list has the room");
    assert!(g.page >= 5, "about six rows to a page, like the mock");
    // the badges sit under the lanes only, the panel keeps its column
    assert_eq!(u.studio_row().right(), u.plot.grid.right());
    // the strip of cells has the twelve on one page (the list of a row each had six and scrolled), so there is
    // nothing to scroll; there is no pager (the strip that scrolls: batch 17)
    assert!(g.page >= 12, "{} cells on a page", g.page);
    assert_eq!(u.panel_scroll_max(12, &g), 0);
    assert_eq!(u.studio_pinned().len(), u.studio_keys().len(), "every measurement is pinned until it is taken off");
    // a tap on a cell opens the settings of the measurement
    tap_in(&mut u, Rect::new(g.list.x + 80, g.list.y + 5, 40, 30));
    assert_eq!(u.meas_own.dialog, Some(0));
    u.close_menu();
    // + Add measurement opens the chooser
    tap_in(&mut u, g.add);
    assert_eq!(u.menu, MenuId::MeasAdd);
    u.close_menu();
    // the strip's X goes back to the lanes, its maximize button puts the panel in a tab in front of them
    let strip = |u: &mut Ui, r: Rect| {
        u.touch_ev(Phase::Down, r.x + r.w / 2, r.y + 20);
        u.touch_ev(Phase::Up, r.x + r.w / 2, r.y + 20);
    };
    let leaf = u.dock_leaf(super::dock::VKind::Measure).unwrap();
    strip(&mut u, Ui::dock_close_rect(leaf));
    assert_eq!(u.studio.view, StudioView::Waveform);
    assert_eq!(u.plot.grid.w, full);
    u.studio_set_view(StudioView::Docked);
    let leaf = u.dock_leaf(super::dock::VKind::Measure).unwrap();
    strip(&mut u, Rect::new(leaf.right() - 96, leaf.y, 48, 40));
    assert!(
        u.dock_tabs_root() && !u.dock_tabbed_out(super::dock::VKind::Measure) && u.studio.view == StudioView::Docked
    );
}

#[test]
fn studio_list_scrolls_with_the_finger() {
    let mut u = twelve();
    // more cells than the strip has on a page
    for k in [
        Meas::Vmax,
        Meas::Vmin,
        Meas::Vamp,
        Meas::Period,
        Meas::PWidth,
        Meas::NWidth,
        Meas::NDuty,
        Meas::Vtop,
        Meas::Vbase,
    ] {
        for c in 0..2 {
            u.m.meas.push((k, WaveId::Ch(c)));
        }
    }
    u.relayout();
    u.studio_set_view(StudioView::Docked);
    let g = u.studio_panel_geo().unwrap();
    assert!(u.m.meas.len() >= g.page + 6, "{} cells, {} on a page", u.m.meas.len(), g.page);
    let unit = u.studio_panel_row();
    let (x, y) = (g.list.x + 60, g.list.y + g.list.h - 10);
    u.touch_ev(Phase::Down, x, y);
    u.touch_ev(Phase::Move, x, y - 70);
    u.touch_ev(Phase::Move, x, y - 130);
    u.touch_ev(Phase::Up, x, y - 130);
    assert_eq!(u.studio.panel.first, (130 / unit) as usize, "the list follows an upward finger by rows");
    assert!(u.meas_own.dialog.is_none(), "a scroll is not a tap");
    // a drag the other way brings it back
    u.touch_ev(Phase::Down, x, y - 100);
    u.touch_ev(Phase::Move, x, y + 40);
    u.touch_ev(Phase::Up, x, y + 10);
    assert_eq!(u.studio.panel.first, 0);
    // a tap through the touch path still reaches the row
    u.touch_ev(Phase::Down, g.list.x + 60, g.list.y + 5);
    u.touch_ev(Phase::Up, g.list.x + 60, g.list.y + 5);
    assert_eq!(u.meas_own.dialog, Some(0));
}

#[test]
fn studio_floating_panel_lists_the_measurements_moves_docks_and_opens_the_table() {
    // Batch 10: the floating panel is a dock view: its title strip moves it, the X closes it, the strip's maximize
    // opens the table and a drag to an edge docks it (batch10_tests has the drops)
    let mut u = twelve();
    u.studio_set_view(StudioView::Floating);
    shot(&mut u, "09-floating");
    let r = u.dock_float_rect(super::dock::VKind::Measure).expect("floating panel");
    let p = u.studio_panel_rect().unwrap();
    assert!(r.x >= 0 && r.right() <= W && r.y >= TOP.h && r.bottom() <= u.frame().bottom.y);
    assert!(p.y == r.y + super::dock::STRIP_H && p.h == r.h - super::dock::STRIP_H, "the panel draws under its strip");
    assert_eq!(u.plot.grid.w, W, "a floating panel takes no room from the lanes");
    // it follows a finger on its title
    let (x, y) = (r.x + 100, r.y + 12);
    u.touch_ev(Phase::Down, x, y);
    u.touch_ev(Phase::Move, x - 200, y + 60);
    u.touch_ev(Phase::Move, x - 200, y + 60);
    u.touch_ev(Phase::Up, x - 200, y + 60);
    let moved = u.dock_float_rect(super::dock::VKind::Measure).unwrap();
    assert_eq!((moved.x, moved.y), (r.x - 200, r.y + 60));
    assert_eq!(u.studio.view, StudioView::Floating, "dragging the title is not a tap");
    // a touch on the lanes outside it is the plot's, not the panel's
    let delay = u.m.delay;
    let lane = u.plot.wins[0];
    let (lx, ly) = (lane.x + 120, lane.y + 40);
    u.touch_ev(Phase::Down, lx, ly);
    u.touch_ev(Phase::Move, lx + 50, ly);
    u.touch_ev(Phase::Up, lx + 50, ly);
    assert_ne!(u.m.delay, delay, "the lanes still pan");
    // the strip's maximize button: the floating panel is a tab in front of the lanes, and floats again after
    let r = u.dock_float_rect(super::dock::VKind::Measure).unwrap();
    let max = Rect::new(r.right() - 96, r.y, 48, 40);
    u.touch_ev(Phase::Down, max.x + 24, max.y + 20);
    u.touch_ev(Phase::Up, max.x + 24, max.y + 20);
    assert!(u.dock_tabs_root() && u.studio.view == StudioView::Docked);
    assert!(u.dock_exit_full());
    assert_eq!(u.studio.view, StudioView::Floating);
    let r = u.dock_float_rect(super::dock::VKind::Measure).unwrap();
    let close = Ui::dock_close_rect(r);
    u.touch_ev(Phase::Down, close.x + 24, close.y + 20);
    u.touch_ev(Phase::Up, close.x + 24, close.y + 20);
    assert_eq!(u.studio.view, StudioView::Waveform);
}

#[test]
fn expanding_the_measurements_puts_them_in_a_tab_in_front_of_the_waveform() {
    use super::dock::{Extra, VKind};
    use super::measpanel::{BarItem, PanelView};
    // a finger down and up on a button or a tab of a title strip
    let press = |u: &mut Ui, r: Rect| {
        u.touch_ev(Phase::Down, r.x + r.w / 2, r.y + r.h / 2);
        u.touch_ev(Phase::Up, r.x + r.w / 2, r.y + r.h / 2);
    };
    let mut u = twelve();
    u.studio_set_view(StudioView::Docked);
    for _ in 0..6 {
        u.step(0.05);
    }
    let (wave0, meas0) = (u.dock_leaf(VKind::Wave).unwrap(), u.dock_leaf(VKind::Measure).unwrap());
    // the button left of the X is the one every view has
    assert_eq!(u.dock_extra_kind(VKind::Measure), Some(Extra::Maximise));
    let expand = u.dock_extra_rect(VKind::Measure, meas0).unwrap();
    press(&mut u, expand);
    for _ in 0..6 {
        u.step(0.05);
    }
    // one group of tabs fills the area: the waveform and the measurements, the measurements in front
    let lay = u.dock_layout();
    assert_eq!(lay.tabs.len(), 1);
    assert_eq!(lay.tabs[0].kinds, vec![VKind::Wave, VKind::Measure]);
    assert_eq!(lay.tabs[0].kinds[lay.tabs[0].active], VKind::Measure);
    assert!(u.dock_tabs_root() && u.studio.view == StudioView::Docked);
    let r = u.dock_leaf(VKind::Measure).unwrap();
    assert_eq!(r, u.dock_region());
    let tabs = u.dock_tab_rects(VKind::Measure, r);
    assert_eq!(
        tabs.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![VKind::Wave, VKind::Measure],
        "Waveform | Measurements"
    );
    assert_eq!(u.dock_extra_kind(VKind::Measure), Some(Extra::RestoreTabs));
    // it is the panel, with its bar on a line under the tabs: the strip of cells in six columns
    let g = u.studio_panel_geo().expect("the panel, not a view of its own");
    assert_eq!(g.p, u.dock_content(VKind::Measure).unwrap());
    let bar = u.panel_bar(g.p);
    assert!(!bar.wide && bar.hidden.is_empty(), "the bar is whole, on its line under the tabs");
    assert!(bar.slots.iter().any(|s| s.item == BarItem::Stats) && bar.slots.iter().any(|s| s.item == BarItem::Source));
    assert!(u.panel_is_strip());
    assert_eq!(u.panel_cols(), 6);
    shot(&mut u, "10-expanded-strip");
    // the table has all its columns: the statistics are there
    u.panel_set_view(PanelView::Detailed);
    let g = u.studio_panel_geo().unwrap();
    assert_eq!(Ui::detail_cols(g.list.w).0.len(), super::measview::DETAIL_COLS.len());
    shot(&mut u, "12-expanded-table");
    // the cards are in as many columns as fit, and the list scrolls by a line of them
    u.panel_set_view(PanelView::Cards);
    let g = u.studio_panel_geo().unwrap();
    let cols = Ui::panel_card_cols(g.list.w) as usize;
    assert!(cols > 2, "{cols} columns of cards in {} px", g.list.w);
    assert_eq!(u.panel_cols(), cols);
    shot(&mut u, "12-expanded-cards");
    // a tap on the Waveform tab shows the waveform
    let tab = tabs.iter().find(|(k, _)| *k == VKind::Wave).unwrap().1;
    press(&mut u, tab);
    for _ in 0..6 {
        u.step(0.05);
    }
    assert_eq!(u.dock_leaf(VKind::Wave), Some(u.dock_region()));
    assert!(
        u.dock_tabbed_out(VKind::Measure) && !u.dock_tabbed_out(VKind::Wave) && u.dock_leaf(VKind::Measure).is_none()
    );
    assert!(u.plot.grid.w > wave0.w, "the lanes have the whole width: {:?}", u.plot.grid);
    shot(&mut u, "12-expanded-waveform-tab");
    // the restore button (it is on the waveform's strip now) brings the docked arrangement back
    let r = u.dock_leaf(VKind::Wave).unwrap();
    assert_eq!(u.dock_extra_kind(VKind::Wave), Some(Extra::RestoreTabs));
    let restore = u.dock_extra_rect(VKind::Wave, r).unwrap();
    press(&mut u, restore);
    for _ in 0..6 {
        u.step(0.05);
    }
    assert!(!u.dock_full_state() && u.studio.view == StudioView::Docked);
    assert_eq!((u.dock_leaf(VKind::Wave), u.dock_leaf(VKind::Measure)), (Some(wave0), Some(meas0)));
    // the list button of the badge row (it carries the count; the Meas tab is gone) brings a closed panel back
    u.studio_set_view(StudioView::Waveform);
    assert!(u.studio_tab().is_none());
    let chip = u.studio_list_chip();
    tap_in(&mut u, chip);
    assert_eq!(u.studio.view, StudioView::Docked);
}

/// The owner on his unit: with the measurements in front of a group of tabs the strip held the panel's controls
/// (source, gate, search, pin, statistics, add) beside the view switch, restore and X, and the names of the tabs
/// went into the list behind the arrow. The tabs have the strip, the controls a line of their own under it.
#[test]
fn the_tabs_keep_the_strip_and_the_controls_of_the_panel_have_a_line_under_it() {
    use super::dock::VKind;
    use super::measpanel::{BarItem, PanelDrop, PanelView};
    let press = |u: &mut Ui, r: Rect| {
        u.touch_ev(Phase::Down, r.x + r.w / 2, r.y + r.h / 2);
        u.touch_ev(Phase::Up, r.x + r.w / 2, r.y + r.h / 2);
        for _ in 0..2 {
            u.step(0.05);
        }
    };
    let mut u = twelve();
    u.studio_set_view(StudioView::Docked);
    for _ in 0..6 {
        u.step(0.05);
    }
    // the narrow panel beside the waveform, not in a group: its toolbar, as before
    let narrow = u.studio_panel_geo().unwrap();
    assert!(!u.panel_bar_wide(narrow.p) && u.dock_tab_group(VKind::Measure).is_none());
    u.dock_maximise(VKind::Measure);
    for _ in 0..6 {
        u.step(0.05);
    }
    let r = u.dock_leaf(VKind::Measure).unwrap();
    assert_eq!((r, r.w), (u.dock_region(), W), "the group fills the screen's width");
    // both tabs by name, no arrow
    let tabs = u.dock_tab_rects(VKind::Measure, r);
    assert_eq!(tabs.iter().map(|(k, _)| *k).collect::<Vec<_>>(), vec![VKind::Wave, VKind::Measure]);
    assert!(u.dock_tab_more(VKind::Measure, r).is_none(), "no list of tabs behind an arrow");
    let f = u.fonts();
    for (k, t) in &tabs {
        assert!(t.w >= f.mb.width(&u.dock_name(*k)) + 20, "{k:?} has its whole name: {t:?}");
    }
    // the view switch, restore and X stay in the strip, right of the tabs
    let sw = u.dock_view_rect(VKind::Measure, r).unwrap();
    assert!(tabs.iter().all(|(_, t)| t.right() <= sw.x));
    // the controls: all six on one line of the content, between the strip and the list, none behind "..."
    let strip = Ui::dock_strip(r);
    let g = u.studio_panel_geo().unwrap();
    let bar = u.panel_bar(g.p);
    assert!(!bar.wide && bar.more.is_none() && bar.hidden.is_empty() && bar.slots.len() == 6);
    for s in &bar.slots {
        assert!(
            s.r.y >= strip.bottom() && s.r.bottom() <= g.list.y,
            "{:?} {:?} under the strip {strip:?}, over the list {:?}",
            s.item,
            s.r,
            g.list
        );
        assert!(s.r.h >= 36 && s.hit.h >= 40 && s.r.w >= 34, "{:?}: {:?} touch {:?}", s.item, s.r, s.hit);
        // the same rectangle is drawn and tapped
        assert_eq!(u.panel_bar_hit(g.p, s.r.x + s.r.w / 2, s.r.y + s.r.h / 2), Some(Some(s.item)));
        assert_eq!(u.panel_bar_hit(g.p, s.r.x + 1, s.r.y + 1), Some(Some(s.item)));
        assert_eq!(u.panel_bar_hit(g.p, s.r.right() - 2, s.r.bottom() - 2), Some(Some(s.item)));
    }
    shot(&mut u, "12-expanded-controls-line");
    // a finger on each control where it is drawn does what the control says
    let at = |it: BarItem| bar.slots.iter().find(|s| s.item == it).unwrap().r;
    press(&mut u, at(BarItem::Pinned));
    assert!(u.studio.panel.pinned_only);
    press(&mut u, at(BarItem::Pinned));
    assert!(!u.studio.panel.pinned_only);
    assert_eq!(u.studio.panel.stats_mode, 0);
    press(&mut u, at(BarItem::Stats));
    assert_eq!(u.studio.panel.stats_mode, 1);
    press(&mut u, at(BarItem::Source));
    assert_eq!(u.studio.panel.drop, Some(PanelDrop::Source));
    u.studio.panel.drop = None;
    press(&mut u, at(BarItem::Gate));
    assert_eq!(u.studio.panel.drop, Some(PanelDrop::Gate));
    u.studio.panel.drop = None;
    press(&mut u, at(BarItem::Add));
    assert_eq!(u.menu, MenuId::MeasAdd);
    u.close_menu();
    // the view switch in the strip chooses the view
    let cells = Ui::view_switch_cells(sw);
    press(&mut u, Rect::new(cells[1].x, r.y, cells[1].w, 26));
    assert_eq!(u.panel_view(), PanelView::Detailed);
    // the search: its field is on the controls' line, and the tabs stay in the strip
    let search = at(BarItem::Search);
    press(&mut u, search);
    assert!(u.studio.panel.searching);
    let (field, _) = u.panel_bar(g.p).field.expect("the search field");
    assert!(field.y >= strip.bottom());
    assert_eq!(u.dock_tab_rects(VKind::Measure, r).len(), 2);
}

#[test]
fn the_cards_of_a_wide_panel_scroll_by_the_columns_they_are_drawn_in() {
    use super::dock::VKind;
    let mut u = studio();
    for c in 2..4 {
        u.m.ch[c].on = true;
    }
    let kinds = [
        Meas::Freq,
        Meas::Vpp,
        Meas::Vrms,
        Meas::Rise,
        Meas::PDuty,
        Meas::Vavg,
        Meas::Fall,
        Meas::Vmax,
        Meas::Vmin,
        Meas::Period,
        Meas::NDuty,
        Meas::Vamp,
    ];
    u.m.meas = (0..4).flat_map(|c| kinds.into_iter().map(move |k| (k, WaveId::Ch(c)))).collect();
    u.m.meas2.clear();
    u.m.cat.clear();
    u.results.rows.clear();
    u.studio_set_view(StudioView::Docked);
    u.panel_set_view(super::measpanel::PanelView::Cards);
    for _ in 0..6 {
        u.step(0.05);
    }
    u.dock_maximise(VKind::Measure);
    for _ in 0..6 {
        u.step(0.05);
    }
    let g = u.studio_panel_geo().unwrap();
    let cols = u.panel_cell_geo(g.list.w).0 as usize;
    let total = {
        let rows = u.meas_rows();
        u.studio_listed(&rows).len()
    };
    assert!(cols > 2 && total > g.page, "{cols} columns, {total} cards, {} a page", g.page);
    // one step of the wheel: the list moves by whole lines of cards, so a card stays in its column
    assert!(u.panel_wheel(g.list.x + 40, g.list.y + 40, -1));
    let first = u.studio.panel.first;
    assert!(first > 0 && first % cols == 0, "first {first} of {cols} columns");
    let cards = u.panel_cards_layout(u.studio_first(total, g.page), total, g.list);
    assert_eq!(cards[0].0 % cols, 0, "the card at the left is the first of its line");
    // the end of the list is reached, and the last card is on it
    u.panel_scroll_set(i32::MAX / 2, total, &g);
    let cards = u.panel_cards_layout(u.studio_first(total, g.page), total, g.list);
    assert_eq!(cards.last().unwrap().0, total - 1);
    shot(&mut u, "12-expanded-cards-scrolled");
}

#[test]
fn a_stored_measurements_only_view_loads_as_the_docked_panel() {
    use super::dock::VKind;
    // the word of the view that is gone, in a set-up file and in a stored view
    assert_eq!(StudioView::from_token("only"), StudioView::Docked);
    let u = studio();
    let text = crate::store::setup_text(&u.m);
    assert!(text.contains(
        "studio_view = waveform
"
    ));
    let old = text.replace(
        "studio_view = waveform
",
        "studio_view = only
",
    );
    let m = crate::store::parse_setup(&old).expect("a set-up with the old word loads");
    assert_eq!(m.studio_view, "docked");
    let mut v = Ui::new(Source::Demo { t: 0.0 });
    v.m = m;
    v.restore_view_state();
    for _ in 0..10 {
        v.step(0.05);
    }
    assert_eq!(v.studio.view, StudioView::Docked);
    assert!(
        v.dock_leaf(VKind::Wave).is_some() && v.dock_leaf(VKind::Measure).is_some(),
        "the panel beside the waveform"
    );
    assert!(!crate::store::setup_text(&v.m).contains("only\n"), "the old word is not written again");
}

/// Studio zoomed in on the trigger, five times.
fn zoomed() -> Ui {
    let mut u = twelve();
    u.m.run = RunState::Stop;
    u.set_zoom(true);
    u.m.zscale = u.m.tscale.saturating_sub(3);
    u.m.zdelay = 0.0;
    u.relayout();
    u
}

#[test]
fn studio_zoom_overview_window_drags_and_a_lane_badge_scales_vertically() {
    let mut u = zoomed();
    for _ in 0..4 {
        u.step(0.05);
    }
    shot(&mut u, "15-zoom-navigator");
    // the overview window moves with a finger on it (the existing gesture, restyled)
    let (overview, _) = u.split_zoom(u.plot.grid);
    let before = u.m.zdelay;
    let (x, y) = (overview.x + overview.w / 2, overview.y + 45);
    u.touch_ev(Phase::Down, x, y);
    u.touch_ev(Phase::Move, x + 80, y);
    u.touch_ev(Phase::Up, x + 80, y);
    assert!(u.m.zdelay > before, "the window follows the finger to later time");
    // a pan in the detail lanes moves the time too
    let lane = u.plot.wins[0];
    let before = u.m.zdelay;
    u.touch_ev(Phase::Down, lane.x + 300, lane.y + 60);
    u.touch_ev(Phase::Move, lane.x + 360, lane.y + 60);
    shot(&mut u, "16-zoom-pan");
    u.touch_ev(Phase::Up, lane.x + 360, lane.y + 60);
    assert!(u.m.zdelay < before, "the content follows the finger");
    // a channel badge opens the display scale of its lane
    u.render(&mut Canvas::new(W, H));
    let (c, badge) = u.studio.lane_badges.borrow()[1];
    assert_eq!(c, 1);
    tap_in(&mut u, badge);
    assert_eq!(u.studio.yzoom, Some(1));
    let (_, minus, plus, reset) = u.studio_yzoom_parts().unwrap();
    tap_in(&mut u, plus);
    tap_in(&mut u, plus);
    assert_eq!(u.zoom_view.vfactor, 4.0);
    shot(&mut u, "17-zoom-vertical");
    tap_in(&mut u, minus);
    assert_eq!(u.zoom_view.vfactor, 2.0);
    tap_in(&mut u, reset);
    assert_eq!(u.zoom_view.vfactor, 1.0);
    u.tap(300, 120);
    assert!(u.studio.yzoom.is_none());
    // outside the zoom a badge opens the channel menu
    u.set_zoom(false);
    u.render(&mut Canvas::new(W, H));
    let badge = u.studio.lane_badges.borrow()[0].1;
    tap_in(&mut u, badge);
    assert_eq!(u.menu, MenuId::Channel(0));
}

// Break it catches (seen on a unit, 2026-11-17): Display > Bottom bar = Compact was left on, so Layout = Default kept the
// compact top and bottom bars and the screen did not look like OOS. Picking Default gives the stock bars, also when
// Default is already the layout; Bottom bar can still turn the compact one on afterwards.
#[test]
fn picking_default_brings_back_the_stock_bars() {
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.m.bar_compact = true;
    assert!(u.m.compact());
    u.select_option(Item::Layout, "Default");
    assert!(!u.m.compact(), "Default picked again: the stock bars");
    u.select_option(Item::Layout, "Studio");
    u.m.bar_compact = true;
    u.select_option(Item::Layout, "Default");
    assert!(u.m.layout == Layout::Default && !u.m.compact(), "from Studio: the stock bars");
    u.select_option(Item::BottomBar, "Compact");
    assert!(u.m.compact(), "Bottom bar still chooses the compact one");
}

// Break it catches (the owner, from the unit): the Default key in Studio took the screen back to the Default layout.
// The key resets the set-up and keeps the layout the person works in; `*RST` of a script still starts in Default.
#[test]
fn the_default_key_keeps_the_layout() {
    for layout in [Layout::Studio, Layout::Default] {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.select_option(Item::Layout, lname(layout));
        u.m.ch[1].on = true;
        u.key(key::DEFAULT);
        assert!(u.m.layout == layout, "{layout:?}: Default leaves the layout");
        assert!(!u.m.ch[1].on, "{layout:?}: the rest is reset");
    }
    let mut u = Ui::new(Source::Demo { t: 0.0 });
    u.select_option(Item::Layout, "Studio");
    u.default_setup();
    assert!(u.m.layout == Layout::Default, "a reset by command is a new set-up in the Default layout");
}

fn lname(l: Layout) -> &'static str {
    match l {
        Layout::Default => "Default",
        Layout::Minimal => "Minimal",
        Layout::Studio => "Studio",
    }
}
