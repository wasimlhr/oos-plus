// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The Windows page and the three arrangements of the traces, the same in every layout.
//!
//! Traces: **Overlay** puts every source on one graticule, **Stacked** gives each its lane in one plot, **Windows**
//! splits the plot into four separate waveform views, numbered 1 to 4, each source standing in the window its number
//! row names. The page lists the numbers only while Windows is the arrangement. Full screen and Strip are the page's
//! other display rows; Lower is the pane under the plot (Results, Trend, Events, Spectrum, ...), which the
//! arrangement does not touch. Every other plot and table is a view that takes the whole plot (the view button of the
//! plot, `AView`), whichever arrangement is chosen. The external display's choice (mirror, waveform, layout) is on
//! Display > Screen; the density of its plot is Full screen here.

use super::*;

/// How the traces share the plot (the Traces row of Display and of the Windows page, the plot's menu).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Arrangement {
    Overlay,
    Stacked,
    Windows,
}

impl Arrangement {
    pub(super) const ALL: [Arrangement; 3] = [Arrangement::Overlay, Arrangement::Stacked, Arrangement::Windows];

    pub(super) fn label(self) -> &'static str {
        match self {
            Arrangement::Overlay => "Overlay",
            Arrangement::Stacked => "Stacked",
            Arrangement::Windows => "Windows",
        }
    }
}

/// The external display (ADR 0062).
#[derive(Default)]
pub struct Hdmi {
    /// An external display is attached. Only then does "waveform on the external display" keep the panel bars in Full.
    present: bool,
    /// the monitor's size while one is driven (main.rs), for `:SYSTem:STATe?`
    pub out: Option<(u32, u32)>,
}

/// Full density on the panel (ADR 0062).
#[derive(Default)]
pub(super) struct Full {
    /// the plot covers the top bar and the cards. Stays off while the external display shows the waveform, so the
    /// panel keeps the controls, unless the expand button asked for it (`asked`).
    pub on: bool,
    /// Full came from a tap on the expand button: the panel enlarges its plot even while the external display shows
    /// the waveform. Full chosen on the Windows page, a display plugged in, or leaving Full clears it.
    pub asked: bool,
}

const CH_NAMES: [&str; 4] = ["CH1", "CH2", "CH3", "CH4"];
const REF_NAMES: [&str; NREF] = ["R1", "R2", "R3", "R4", "R5", "R6", "R7", "R8", "R9", "R10"];
const MATH_NAMES: [&str; 8] = ["M1", "M2", "M3", "M4", "M5", "M6", "M7", "M8"];

impl Ui {
    /// The Windows page, the same in every layout: Traces, Full screen, Strip and the lower pane; then, while the
    /// arrangement is Windows, the window number of every channel, math channel, reference and the LA that is on.
    pub(super) fn win_items(&self) -> Vec<Item> {
        let mut v = vec![Item::Traces, Item::FullScreen, Item::StripPos, Item::PaneSel];
        if self.arrangement() == Arrangement::Windows {
            v.extend((0..n_ch()).filter(|&c| self.m.ch[c].on).map(Item::WinCh));
            v.extend((0..NREF).filter(|&i| self.refs[i].as_ref().is_some_and(|r| r.on)).map(Item::WinRef));
            v.extend((0..NMATH).filter(|&k| self.m.math_shown(k)).map(Item::WinMath));
            if self.m.la_on() {
                v.push(Item::WinLa);
            }
        }
        v
    }

    /// The arrangement in force: four windows, else lanes, else one graticule.
    pub(super) fn arrangement(&self) -> Arrangement {
        if self.m.win_layout == WinLayout::Quad {
            Arrangement::Windows
        } else if self.trace_stacked() {
            Arrangement::Stacked
        } else {
            Arrangement::Overlay
        }
    }

    /// Chooses the arrangement. Windows spreads the sources over the four windows in channel order; leaving it puts
    /// everything on one plot again. The lower pane and Full screen are left as they are.
    pub(super) fn set_arrangement(&mut self, a: Arrangement) {
        match a {
            Arrangement::Windows => {
                if self.arrangement() == a {
                    return;
                }
                self.workspace.popup = None;
                self.workspace.stacked = false;
                if self.m.layout == Layout::Studio {
                    self.studio.overlay = true;
                    self.m.studio_overlay = true;
                }
                self.m.win_layout = WinLayout::Quad;
                self.spread_wins();
                if self.m.zoom {
                    self.toast("Windows show while zoom is off");
                }
                self.relayout();
                self.dirty_map = true;
            }
            Arrangement::Overlay | Arrangement::Stacked => {
                self.m.win_layout = WinLayout::One;
                self.choose_trace_mode(a == Arrangement::Stacked);
            }
        }
    }

    /// Full screen on or off: the plot alone with the strip, or the bars and cards back. Full takes the lower pane
    /// away and leaving it brings it back (`split_pane` keeps it in between, also across a restart). `asked`: the
    /// expand button of the plot did it, so the panel enlarges its plot even while an external display shows the
    /// waveform.
    pub(super) fn set_full(&mut self, on: bool, asked: bool) {
        if on != (self.m.density == Density::Full) {
            if on {
                self.m.split_pane = self.m.pane;
                self.m.pane = 0;
            } else {
                let kept = std::mem::take(&mut self.m.split_pane);
                // a pane chosen meanwhile stays
                if kept != 0 && self.m.pane == 0 {
                    self.m.pane = kept;
                }
            }
            self.m.density = if on { Density::Full } else { Density::Scope };
        }
        self.full.asked = on && asked;
        self.sync_full();
    }

    /// Expand button of the plot: Full screen on or off.
    pub(super) fn toggle_expand(&mut self) {
        self.set_full(self.m.density != Density::Full, true);
    }

    /// Panel bars hide for Full while the external display copies the panel, or when the expand button asked for it.
    pub(crate) fn sync_full(&mut self) {
        if self.m.density != Density::Full {
            self.full.asked = false;
        }
        // waveform on an external display keeps the panel's bars unless the button asked; with no display attached
        // Full enlarges the panel
        let external = self.m.hdmi != HdmiMode::Mirror && self.hdmi.present;
        self.full.on = self.m.density == Density::Full && (!external || self.full.asked);
    }

    /// Called when an external display comes or goes, so Full follows it.
    pub(crate) fn set_hdmi_present(&mut self, present: bool) {
        if self.hdmi.present != present {
            self.hdmi.present = present;
            // a display plugged in takes the waveform; the panel keeps its bars until the button is tapped again
            if present {
                self.full.asked = false;
            }
            self.sync_full();
        }
    }

    pub(super) fn win_kind(it: Item) -> Option<Kind> {
        match it {
            Item::FullScreen | Item::StripPos | Item::WinCh(_) | Item::WinRef(_) | Item::WinMath(_) | Item::WinLa => Some(Kind::Choice),
            _ => None,
        }
    }

    pub(super) fn win_label(it: Item) -> Option<&'static str> {
        Some(match it {
            Item::Open(MenuId::Windows) => "Windows",
            Item::FullScreen => "Full screen",
            Item::StripPos => "Strip",
            Item::WinCh(c) => CH_NAMES.get(c).copied().unwrap_or("CH"),
            Item::WinRef(i) => REF_NAMES.get(i).copied().unwrap_or("R"),
            Item::WinMath(k) => MATH_NAMES.get(k).copied().unwrap_or("M"),
            Item::WinLa => "LA",
            _ => return None,
        })
    }

    pub(super) fn win_value(&self, it: Item) -> Option<String> {
        let n = WinLayout::Quad.count();
        Some(match it {
            Item::FullScreen => if self.m.density == Density::Full { "On" } else { "Off" }.to_string(),
            Item::StripPos => self.m.strip_dock.label().to_string(),
            Item::WinCh(c) => (self.m.ch_win[c].min(n - 1) + 1).to_string(),
            Item::WinRef(i) => (self.m.ref_win[i].min(n - 1) + 1).to_string(),
            Item::WinMath(k) => (self.m.math_win[k].min(n - 1) + 1).to_string(),
            Item::WinLa => (self.m.la_win.min(n - 1) + 1).to_string(),
            _ => return None,
        })
    }

    pub(super) fn win_options(&self, it: Item) -> Option<Vec<String>> {
        Some(match it {
            Item::FullScreen => vec!["Off".into(), "On".into()],
            Item::StripPos => StripDock::ALL.iter().map(|p| p.label().to_string()).collect(),
            Item::WinCh(_) | Item::WinRef(_) | Item::WinMath(_) | Item::WinLa => {
                (1..=WinLayout::Quad.count()).map(|i| i.to_string()).collect()
            }
            _ => return None,
        })
    }

    pub(super) fn win_adjust(&mut self, it: Item, d: i32) -> bool {
        let n = WinLayout::Quad.count();
        match it {
            // the menu row asks for the panel itself as the expand button does (`full.asked`): with an external display
            // it hid nothing on the panel, the row said On and the bars stayed
            Item::FullScreen => self.set_full(self.m.density != Density::Full, true),
            Item::StripPos => {
                let all = StripDock::ALL;
                let i = all.iter().position(|&p| p == self.m.strip_dock).unwrap_or(0);
                self.m.strip_dock = all[cycle(i, all.len(), d)];
                // the statistics card follows the dock the strip uses
                self.workspace.measure_docked = true;
                self.workspace.measure_float = None;
                self.relayout();
            }
            Item::WinCh(c) => self.m.ch_win[c] = cycle(self.m.ch_win[c].min(n - 1), n, d),
            Item::WinRef(i) => self.m.ref_win[i] = cycle(self.m.ref_win[i].min(n - 1), n, d),
            Item::WinMath(k) => self.m.math_win[k] = cycle(self.m.math_win[k].min(n - 1), n, d),
            Item::WinLa => self.m.la_win = cycle(self.m.la_win.min(n - 1), n, d),
            _ => return false,
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ui() -> Ui {
        let mut u = crate::ui::testkit::demo_ui();
        for c in 0..n_ch() {
            u.m.ch[c].on = true;
        }
        u
    }

    fn in_layout(layout: Layout) -> Ui {
        let mut u = ui();
        u.m.layout = layout;
        u
    }

    fn render(u: &mut Ui) -> Canvas {
        let mut cv = Canvas::new(PANEL_W, PANEL_H);
        u.render(&mut cv);
        cv
    }

    /// The three arrangements, one row, the same page in every layout.
    #[test]
    fn the_page_is_the_same_in_every_layout_and_lists_the_window_numbers_only_for_windows() {
        for layout in [Layout::Default, Layout::Minimal, Layout::Studio] {
            let mut u = in_layout(layout);
            u.open_menu(MenuId::Windows);
            let rows = u.items(MenuId::Windows);
            assert_eq!(rows, [Item::Traces, Item::FullScreen, Item::StripPos, Item::PaneSel], "{layout:?}");
            assert_eq!(u.options(Item::Traces), ["Overlay", "Stacked", "Windows"], "{layout:?}");
            assert_eq!(u.options(Item::FullScreen), ["Off", "On"]);
            assert_eq!(u.options(Item::StripPos), ["Below plot", "Left", "Right", "Above plot", "Hidden"]);
            assert_eq!((Ui::label(Item::FullScreen), Ui::label(Item::StripPos)), ("Full screen", "Strip"));
            // the number rows come with Windows and go with it
            u.select_option(Item::Traces, "Windows");
            let v = u.win_items();
            assert!(v.contains(&Item::WinCh(0)) && v.contains(&Item::WinCh(3)), "{layout:?} {v:?}");
            assert_eq!(u.options(Item::WinCh(0)), ["1", "2", "3", "4"]);
            u.select_option(Item::Traces, "Stacked");
            assert_eq!(u.win_items(), [Item::Traces, Item::FullScreen, Item::StripPos, Item::PaneSel], "{layout:?}");
            // every row of the page takes every one of its options and reads it back
            u.select_option(Item::Traces, "Windows");
            for it in u.win_items() {
                let opts = u.options(it);
                for o in opts.iter().rev().chain(opts.iter()) {
                    u.select_option(it, o);
                    assert_eq!(&u.value(it), o, "{layout:?} {it:?} {o}");
                }
            }
        }
    }

    /// The Traces row is on the page of every layout and on the Display sheet of the compact ones.
    #[test]
    fn traces_is_a_row_of_the_page_and_of_the_display_sheet() {
        for layout in [Layout::Default, Layout::Minimal, Layout::Studio] {
            let mut u = in_layout(layout);
            assert!(u.items(MenuId::Windows).contains(&Item::Traces), "{layout:?}");
            if layout != Layout::Default {
                let spec = u.sheet_spec(MenuId::Display).expect("the Display sheet");
                assert!(spec.tabs.iter().any(|t| t.rows.contains(&Item::Traces)), "{layout:?}");
            }
            for a in ["Windows", "Stacked", "Overlay"] {
                u.select_option(Item::Traces, a);
                assert_eq!(u.value(Item::Traces), a, "{layout:?}");
            }
        }
    }

    #[test]
    fn windows_make_four_views_each_source_in_the_one_its_number_names() {
        for layout in [Layout::Default, Layout::Minimal, Layout::Studio] {
            let mut u = in_layout(layout);
            u.select_option(Item::Traces, "Windows");
            assert_eq!((u.m.win_layout, u.arrangement()), (WinLayout::Quad, Arrangement::Windows), "{layout:?}");
            // the sources spread over the windows in channel order
            assert_eq!(&u.m.ch_win[..n_ch()], &(0..n_ch()).map(|c| c % 4).collect::<Vec<_>>()[..], "{layout:?}");
            render(&mut u);
            assert_eq!(u.plot.wins.len(), 4, "{layout:?}");
            let g = u.plot.grid;
            for (i, a) in u.plot.wins.iter().enumerate() {
                assert_eq!((a.w, a.h), (u.plot.wins[0].w, u.plot.wins[0].h), "{layout:?}");
                assert!(a.x >= g.x && a.y >= g.y && a.x + a.w <= g.x + g.w && a.y + a.h <= g.y + g.h, "{layout:?} {a:?}");
                for b in &u.plot.wins[i + 1..] {
                    assert!(!overlaps(*a, *b), "{layout:?}");
                }
            }
            for c in 0..n_ch() {
                assert_eq!(u.ch_area(c), u.plot.wins[c], "{layout:?}: CH{} in window {}", c + 1, c + 1);
            }
            // CH1 moved to window 4 by its number
            u.select_option(Item::WinCh(0), "4");
            render(&mut u);
            assert_eq!(u.ch_area(0), u.plot.wins[3], "{layout:?}");
            let y = u.y_of_volts(0, 0.0);
            let w = u.plot.wins[3];
            assert!(y >= w.y && y < w.y + w.h, "{layout:?}");
            // Stacked and Overlay put everything on one plot again, Windows does not outlive them
            for a in ["Stacked", "Overlay"] {
                u.select_option(Item::Traces, a);
                render(&mut u);
                assert_ne!(u.m.win_layout, WinLayout::Quad, "{layout:?} {a}");
                assert_eq!(u.value(Item::Traces), a, "{layout:?}");
                if a == "Overlay" {
                    assert_eq!(u.plot.wins.len(), 1, "{layout:?}");
                    assert!((0..n_ch()).all(|c| u.ch_area(c) == u.plot.area), "{layout:?}");
                } else {
                    assert_eq!(u.plot.wins.len(), n_ch(), "{layout:?}: a lane each");
                }
            }
        }
    }

    // Break it catches: an empty window beside the one that holds every source. On a unit, 2026-11-20: Overlay with
    // two windows and only CH2 on drew a second, empty lane under it
    #[test]
    fn windows_with_every_source_in_one_are_that_window_alone() {
        let mut u = crate::ui::testkit::demo_ui();
        for c in 0..n_ch() {
            u.m.ch[c].on = c == 1;
        }
        u.m.win_layout = WinLayout::Quad;
        u.m.ch_win = [1; NCH];
        render(&mut u);
        assert_eq!(u.plot.wins.len(), 1, "one source, one window");
        assert_eq!(u.ch_area(1), u.plot.wins[0]);
        u.m.ch[0].on = true;
        u.m.ch_win[0] = 0;
        render(&mut u);
        assert_eq!(u.plot.wins.len(), 4, "a second window in use: the four views");
    }

    #[test]
    fn zoom_and_views_keep_one_window() {
        let mut u = ui();
        u.set_arrangement(Arrangement::Windows);
        u.m.zoom = true;
        render(&mut u);
        assert_eq!(u.plot.wins.len(), 1);
        u.m.zoom = false;
        render(&mut u);
        assert_eq!(u.plot.wins.len(), 4);
    }

    // Break it catches: something taken away after Add. The Windows (and Stacked) arrangement stood in the way of the
    // lower pane: a pane chosen, or Add > Plot > Spectrum / Add > Table > decoder events, turned the windows into one
    // plot, and Stacked closed the pane.
    #[test]
    fn a_lower_pane_and_the_arrangement_do_not_remove_each_other() {
        for layout in [Layout::Default, Layout::Minimal, Layout::Studio] {
            for a in [Arrangement::Windows, Arrangement::Stacked, Arrangement::Overlay] {
                let mut u = in_layout(layout);
                u.set_arrangement(a);
                for pane in ["Results", "Spectrum", "Trend", "Events"] {
                    u.select_option(Item::PaneSel, pane);
                    render(&mut u);
                    assert_eq!(u.arrangement(), a, "{layout:?} {a:?} {pane}");
                    assert_ne!(u.m.pane, 0, "{layout:?} {a:?} {pane}");
                }
                // and the other way round: the pane stays through every arrangement
                for b in Arrangement::ALL {
                    u.set_arrangement(b);
                    render(&mut u);
                    assert_ne!(u.m.pane, 0, "{layout:?} {a:?} -> {b:?}");
                    assert_eq!(u.arrangement(), b);
                }
            }
        }
    }

    #[test]
    fn add_plot_and_table_leave_the_windows_alone() {
        use super::super::sheet::PanelKind;
        let mut u = in_layout(Layout::Studio);
        u.set_arrangement(Arrangement::Windows);
        render(&mut u);
        for k in [PanelKind::Spectrum, PanelKind::Trend, PanelKind::Measurements, PanelKind::Histogram] {
            u.studio_apply_panel(k);
            render(&mut u);
            assert_eq!(u.arrangement(), Arrangement::Windows, "{k:?}");
            assert_eq!(u.plot.wins.len(), 4, "{k:?}: the four windows stay");
        }
    }

    #[test]
    fn full_screen_takes_the_lower_pane_away_and_gives_it_back() {
        let mut u = ui();
        u.select_option(Item::PaneSel, "Trend");
        let pane = u.m.pane;
        u.select_option(Item::FullScreen, "On");
        assert_eq!((u.m.density, u.m.pane, u.m.split_pane), (Density::Full, 0, pane));
        u.select_option(Item::FullScreen, "Off");
        assert_eq!((u.m.density, u.m.pane, u.m.split_pane), (Density::Scope, pane, 0));
        // a pane chosen while Full is kept
        u.select_option(Item::FullScreen, "On");
        u.select_option(Item::PaneSel, "Results");
        assert_eq!((u.m.density, u.m.pane), (Density::Scope, tables::PANE_RESULTS));
        // the pane's own close button forgets it
        u.m.pane = 0;
        u.select_option(Item::FullScreen, "On");
        u.select_option(Item::FullScreen, "Off");
        assert_eq!(u.m.pane, 0);
    }

    #[test]
    fn full_screen_button_enlarges_and_goes_back() {
        let mut u = ui();
        // mirror: the panel is the picture, so Full gives it the bars
        u.select_option(Item::Hdmi, "Mirror");
        render(&mut u);
        let normal = u.plot.grid;
        let (_, hit) = u.full_btn();
        u.tap(hit.x + hit.w / 2, hit.y + hit.h / 2);
        assert_eq!(u.m.density, Density::Full);
        assert!(u.full.on);
        render(&mut u);
        assert!(u.plot.grid.y < normal.y && u.plot.grid.h > normal.h, "{:?} {:?}", u.plot.grid, normal);
        // the cards are gone: a tap where CH1's card was opens nothing
        let (chs, _, _) = u.cards();
        u.tap(chs[0].x + 5, chs[0].y + 5);
        assert!(!u.menu_open);
        let (_, hit) = u.full_btn();
        u.tap(hit.x + hit.w / 2, hit.y + hit.h / 2);
        assert_eq!(u.m.density, Density::Scope);
        assert!(!u.full.on);
        render(&mut u);
        assert_eq!(u.plot.grid, normal);
        // the touch lock needs the Menu button: no full screen while it is on
        u.m.density = Density::Full;
        u.sync_full();
        u.m.touch_lock = true;
        render(&mut u);
        assert_eq!(u.plot.grid, normal);
    }

    #[test]
    fn full_screen_enlarges_without_an_external_display_and_goes_back() {
        // default "waveform on the external display", but no display attached (desktop, or a scope without a monitor)
        let mut u = ui();
        assert_eq!(u.m.hdmi, HdmiMode::Waveform);
        let press = |u: &mut Ui| {
            render(u);
            let (_, hit) = u.full_btn();
            let (x, y) = (hit.x + hit.w / 2, hit.y + hit.h / 2);
            u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
            u.step(0.08);
            u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
            render(u);
        };
        render(&mut u);
        let normal = u.plot.grid;
        press(&mut u);
        assert!(u.full.on && u.plot.grid.h > normal.h, "Full must enlarge the plot: {:?} {:?}", u.plot.grid, normal);
        press(&mut u);
        assert_eq!((u.m.density, u.full.on, u.plot.grid), (Density::Scope, false, normal));
        // a display attached while Full: the panel keeps its bars, the external display shows the waveform
        press(&mut u);
        u.set_hdmi_present(true);
        render(&mut u);
        assert!(!u.full.on && u.plot.grid == normal);
        u.set_hdmi_present(false);
        render(&mut u);
        assert!(u.full.on && u.plot.grid.h > normal.h);
    }

    #[test]
    fn the_expand_button_enlarges_the_panel_while_the_external_display_shows_the_waveform() {
        // forum report: with HDMI on Waveform or Layout the button did nothing on the panel
        for mode in ["Waveform", "Layout"] {
            let mut u = ui();
            u.set_hdmi_present(true);
            u.select_option(Item::Hdmi, mode);
            render(&mut u);
            let normal = u.plot.grid;
            let tap = |u: &mut Ui| {
                let (_, hit) = u.full_btn();
                u.tap(hit.x + hit.w / 2, hit.y + hit.h / 2);
                render(u);
            };
            tap(&mut u);
            assert!(
                u.full.on && u.plot.grid.h > normal.h,
                "{mode}: the tap must enlarge the panel: {:?} {:?}",
                u.plot.grid,
                normal
            );
            tap(&mut u);
            assert_eq!((u.m.density, u.full.on, u.plot.grid), (Density::Scope, false, normal), "{mode}");
            // Full screen chosen on the page hides the panel's bars too, as the button does
            u.select_option(Item::FullScreen, "On");
            render(&mut u);
            assert!(u.m.density == Density::Full && u.full.on && u.plot.grid.h > normal.h, "{mode}");
            // and a display plugged in after a tap takes the waveform back
            u.select_option(Item::FullScreen, "Off");
            tap(&mut u);
            assert!(u.full.on, "{mode}");
            u.set_hdmi_present(false);
            u.set_hdmi_present(true);
            render(&mut u);
            assert!(!u.full.on && u.plot.grid == normal, "{mode}");
        }
    }

    #[test]
    fn a_loaded_reference_gets_a_window_row_and_draws_there() {
        let mut u = ui();
        u.refs[0] = Some(RefWave::test(vec![0.0f32; 400], 1e-9, 0.0));
        u.select_option(Item::Traces, "Windows");
        render(&mut u);
        assert!(u.win_items().contains(&Item::WinRef(0)), "{:?}", u.win_items());
        assert!(!u.win_items().contains(&Item::WinRef(1)), "only loaded references are listed");
        u.select_option(Item::WinRef(0), "3");
        assert_eq!(u.value(Item::WinRef(0)), "3");
        render(&mut u);
        assert_eq!(u.source_area(WaveId::Ref(0)), Some(u.plot.wins[2]));
    }

    #[test]
    fn the_corner_buttons_stay_in_place_and_windows_opens_its_menu() {
        let mut u = ui();
        u.select_option(Item::Hdmi, "Mirror");
        render(&mut u);
        let at = u.corner_btns();
        let (home, _) = u.full_btn();
        let (off, delay, level) = (u.m.ch[0].offset, u.m.delay, u.m.trig_level);
        // a drag that starts on a corner button moves neither the button nor the trace, and is no tap
        for r in at {
            let (x0, y0) = (r.x + r.w / 2, r.y + r.h / 2);
            u.touch_ev(Phase::Down, x0, y0);
            u.touch_ev(Phase::Move, x0 - 140, y0 + 70);
            u.touch_ev(Phase::Up, x0 - 140, y0 + 70);
            render(&mut u);
            assert_eq!(u.corner_btns(), at);
            assert_eq!((u.m.ch[0].offset, u.m.delay, u.m.trig_level), (off, delay, level));
            assert_eq!(u.m.density, Density::Scope);
            assert!(!u.m.zoom && !u.menu_open);
        }
        // a tap on the expand button still switches, from the same place
        let (tx, ty) = (home.x + home.w / 2, home.y + home.h / 2);
        u.touch_ev(Phase::Down, tx, ty);
        u.touch_ev(Phase::Up, tx + 4, ty - 3);
        assert_eq!(u.m.density, Density::Full);
        u.touch_ev(Phase::Down, tx, ty);
        u.touch_ev(Phase::Up, tx, ty);
        assert_eq!(u.m.density, Density::Scope);
        render(&mut u);
        assert_eq!(u.corner_btns(), at);
        // the button under the magnifier opens the Windows menu, and a second tap closes it
        let (w, hit) = u.win_btn();
        assert!(hit.contains(w.x + w.w / 2, w.y + w.h / 2));
        assert!(!hit.contains(u.zoom_btn().0.x + 4, u.zoom_btn().0.bottom() - 1));
        u.tap(w.x + w.w / 2, w.y + w.h / 2);
        assert!(u.menu_open && u.menu_root() == MenuId::Windows);
        render(&mut u);
        let (w, _) = u.win_btn();
        u.tap(w.x + w.w / 2, w.y + w.h / 2);
        assert!(!u.menu_open);
    }

    #[test]
    fn waveform_keeps_the_panel_bars_and_full_screen_changes_the_plot() {
        let mut u = ui();
        // the external display is a row of Display > Screen only
        assert!(!u.win_items().contains(&Item::Hdmi));
        assert_eq!(u.value(Item::Hdmi), "Waveform");
        u.m.meas = vec![(Meas::Vpp, WaveId::Ch(0))];
        u.results.on = true;
        render(&mut u);
        let normal = u.plot.grid;
        u.set_hdmi_present(true);
        u.select_option(Item::FullScreen, "On");
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        assert_eq!(u.m.density, Density::Full);
        assert!(u.full.on && u.plot.grid.y == 0, "the row hides the panel's bars as the button does");
        u.select_option(Item::Hdmi, "Mirror");
        u.render(&mut cv);
        assert!(u.full.on && u.plot.grid.y == 0, "mirror shows Full on the panel");
        u.select_option(Item::Hdmi, "Waveform");
        u.render(&mut cv);
        assert!(u.full.on && u.plot.grid.y == 0, "waveform keeps what the row asked for");
        // the monitor's waveform area: in Full without the statistics card, in Scope with it
        let (mw, mh) = (1920, 1080);
        let mut full_pic = Canvas::new(mw, mh);
        u.render_waveform(&mut full_pic);
        u.select_option(Item::FullScreen, "Off");
        u.render(&mut cv);
        let mut scope_pic = Canvas::new(mw, mh);
        u.render_waveform(&mut scope_pic);
        let head = u.with_waveform_pass(mw, mh, |u| {
            u.render(&mut Canvas::new(mw, mh));
            u.results_rects().unwrap().0
        });
        assert!(head.right() > W, "the card is placed for the monitor: {:?}", head);
        let at = (head.y * mw + head.x + 8) as usize;
        assert_ne!(full_pic.px[at], scope_pic.px[at], "Full leaves the statistics card off the external plot");
        assert!(u.plot.grid == normal && u.scr == Screen::PANEL, "the panel's layout is as it was");
        u.select_option(Item::Traces, "Windows");
        u.render(&mut cv);
        assert_eq!(u.plot.wins.len(), 4);
        assert!(u.plot.wins.iter().all(|w| w.y >= TOP.h && w.y + w.h <= u.frame().bottom.y));
        assert!(!u.full.on);
        u.select_option(Item::PaneSel, "Results");
        u.render(&mut cv);
        assert!(u.pane_h() > 0 && u.plot.wins.len() == 4 && !u.full.on, "the pane stands under the four windows");
        let pane = u.pane_rect();
        assert!(pane.y >= TOP.h && pane.y + pane.h <= u.frame().bottom.y);
    }

    /// Studio with so many measurement badges that their row takes two lines on the panel and one on the monitor:
    /// the monitor's waveform pass counts the lines for its own width, and the panel's next step still has its own
    /// (with HDMI Layout the row and the plot changed height every frame and the panel's maps were cleared: seen on a
    /// DHO924S, 2026-11-19, found on the decoder branch).
    #[test]
    fn the_waveform_pass_leaves_the_panel_s_badge_row_and_plot_as_they_are() {
        let mut u = ui();
        u.m.layout = Layout::Studio;
        u.m.hdmi = HdmiMode::Waveform;
        u.set_hdmi_present(true);
        // Measure > Badge style = Studio: a row of badges that wraps onto a second line
        u.m.studio_classic = false;
        u.m.meas = [Meas::Vpp, Meas::Vmax, Meas::Vmin, Meas::Freq, Meas::Period]
            .into_iter()
            .flat_map(|k| [(k, WaveId::Ch(0)), (k, WaveId::Ch(1))])
            .collect();
        let mut cv = render(&mut u);
        u.step(1.0 / 30.0);
        u.render(&mut cv);
        let wrap = u.studio.badge_wrap;
        assert!(wrap > 0, "the row wraps on the panel");
        let wide = u.with_waveform_pass(1920, 1080, |u| {
            u.render(&mut Canvas::new(1920, 1080));
            u.studio.badge_wrap
        });
        assert!(wide < wrap, "and less on the monitor: {} {}", wide, wrap);
        let (row, plot) = (u.studio_row(), u.plot_rect());
        let mut hc = Canvas::new(1920, 1080);
        for round in 0..4 {
            u.render_waveform(&mut hc);
            assert_eq!((u.studio_row(), u.plot_rect()), (row, plot), "after the monitor's pass, round {}", round);
            u.step(1.0 / 30.0);
            u.render(&mut cv);
            assert_eq!((u.studio_row(), u.plot_rect()), (row, plot), "after the panel's frame, round {}", round);
        }
    }

    #[test]
    fn a_drag_in_the_second_window_moves_a_channel_of_that_window() {
        let mut u = ui();
        u.m.win_layout = WinLayout::Quad;
        u.spread_wins();
        u.m.active = 0;
        render(&mut u);
        let b = u.plot.wins[1];
        // CH2's marker at the left edge of the second window
        let y = u.y_of_volts(1, 0.0);
        let x = b.x + 10;
        let before = u.m.ch[1].offset;
        u.touch_ev(Phase::Down, x, y);
        u.touch_ev(Phase::Move, x, y - 40);
        u.touch_ev(Phase::Move, x, y - 60);
        u.touch_ev(Phase::Up, x, y - 60);
        assert!(u.m.ch[1].offset > before);
        assert_eq!(u.m.active, 1);
    }

    #[test]
    fn setup_keeps_the_windows_and_reads_the_older_densities() {
        let mut u = ui();
        u.set_arrangement(Arrangement::Windows);
        u.m.ch_win = std::array::from_fn(|c| (3 - c) % 4);
        u.m.math_win[2] = 3;
        u.m.split_pane = 2;
        let t = crate::store::setup_text(&u.m);
        let mut back = Model::default();
        for l in t.lines().filter(|l| !l.starts_with('#')) {
            let (k, v) = l.split_once(" = ").unwrap();
            crate::store::set(&mut back, k, v).unwrap();
        }
        assert_eq!(
            (back.win_layout, back.density, back.split_pane, back.ch_win, back.math_win),
            (u.m.win_layout, u.m.density, u.m.split_pane, u.m.ch_win, u.m.math_win)
        );
        // a setup written with two windows (Cols, Rows) or the densities Split and Four reads as the four windows
        // and Scope; the lower pane it had stays
        for old in ["windows = Cols\n", "windows = Rows\ndensity = Split\n", "windows = Quad\ndensity = Four\n"] {
            let m = crate::store::parse_setup(old).unwrap();
            assert_eq!((m.win_layout, m.density), (WinLayout::Quad, Density::Scope), "{old:?}");
        }
        let pane = crate::store::parse_setup("pane = 2\ndensity = Split\n").unwrap();
        assert_eq!((pane.density, pane.pane), (Density::Scope, 2));
        let old = crate::store::parse_setup("pane = 2\n").unwrap();
        assert_eq!((old.density, old.pane, old.split_pane), (Density::Scope, 2, 2));
    }

    #[test]
    fn one_strip_setting_with_a_hidden_position() {
        let mut u = in_layout(Layout::Minimal);
        u.m.meas = vec![(Meas::Vpp, WaveId::Ch(0)), (Meas::Freq, WaveId::Ch(0))];
        u.m.cursor = CursorMode::Off;
        u.relayout();
        assert_eq!((u.m.strip_dock, u.value(Item::StripPos)), (StripDock::Below, "Below plot".to_string()));
        let shown = u.plot.grid;
        assert!(u.strip_rows() > 0 && u.strip_rect().is_some());
        // the Windows row, the Measure row and the card's menu write the same field
        u.select_option(Item::StripPos, "Left");
        assert_eq!((u.m.strip_dock, u.value(Item::StripDock)), (StripDock::Left, "Left".to_string()));
        u.select_option(Item::StripDock, "Right");
        assert_eq!(u.value(Item::StripPos), "Right");
        u.workspace_action(workspace::Action::MeasureDock(StripDock::Above));
        assert_eq!(u.value(Item::StripPos), "Above plot");
        // Hidden: no rows, no strip, the plot takes the room
        u.select_option(Item::StripPos, "Hidden");
        u.relayout();
        assert_eq!((u.m.strip_dock, u.strip_rows(), u.strip_rect()), (StripDock::Hidden, 0, None));
        assert_eq!(u.value(Item::StripDock), "Hidden");
        assert!(u.plot.grid.h > shown.h && u.plot.grid.x == 0 && u.plot.grid.w == W, "{:?} vs {:?}", u.plot.grid, shown);
        assert!(!u.paged_strip());
        let mut cv = Canvas::new(W, H);
        u.render(&mut cv);
        // the page cycles round from the last position
        u.adjust(Item::StripPos, 1);
        assert_eq!(u.m.strip_dock, StripDock::Below);
    }

    #[test]
    fn the_strip_and_the_windows_round_trip_in_the_setup_under_oos_next_keys() {
        let mut m = Model::default();
        for p in StripDock::ALL {
            m.strip_dock = p;
            let t = crate::store::setup_text(&m);
            assert!(t.lines().any(|l| l == format!("strip = {:?}", p)), "{:?}", p);
            assert!(!t.lines().any(|l| l.starts_with("strip_dock")));
            assert_eq!(crate::store::parse_setup(&t).unwrap().strip_dock, p);
        }
        // an oos-next file: the names are the same
        let n = crate::store::parse_setup("windows = Quad\nstrip = Hidden\n").unwrap();
        assert_eq!((n.win_layout, n.density, n.strip_dock), (WinLayout::Quad, Density::Scope, StripDock::Hidden));
        // an OOS-plus setup from before: its own key, and Under for Below
        let o = crate::store::parse_setup("density = Split\nwindows = Cols\nstrip_dock = Under\n").unwrap();
        assert_eq!((o.density, o.win_layout, o.strip_dock), (Density::Scope, WinLayout::Quad, StripDock::Below));
        let o = crate::store::parse_setup("density = Full\nstrip_dock = Right\n").unwrap();
        assert_eq!((o.density, o.strip_dock), (Density::Full, StripDock::Right));
        assert!(crate::store::parse_setup("strip = Under").is_err());
    }

    /// The Windows page and every strip position as pictures, when OOS_SHOT_DIR names a folder.
    #[test]
    fn windows_page_and_strip_pictures() {
        let Ok(dir) = std::env::var("OOS_SHOT_DIR") else { return };
        let dir = std::path::PathBuf::from(dir);
        let mut u = in_layout(Layout::Minimal);
        u.m.meas = vec![(Meas::Vpp, WaveId::Ch(0)), (Meas::Freq, WaveId::Ch(0)), (Meas::Vrms, WaveId::Ch(1))];
        u.m.cursor = CursorMode::Time;
        let shot = |u: &mut Ui, name: &str| {
            let mut cv = Canvas::new(W, H);
            u.render(&mut cv);
            let _ = std::fs::write(dir.join(name), crate::store::png_of(&cv));
        };
        u.open_menu(MenuId::Windows);
        shot(&mut u, "windows-page.png");
        u.select_option(Item::Traces, "Windows");
        shot(&mut u, "windows-page-windows.png");
        u.select_option(Item::Traces, "Stacked");
        shot(&mut u, "windows-page-stacked.png");
        u.select_option(Item::Traces, "Overlay");
        u.close_menu();
        for (p, name) in [(StripDock::Below, "strip-below"), (StripDock::Left, "strip-left"), (StripDock::Right, "strip-right"), (StripDock::Above, "strip-above"), (StripDock::Hidden, "strip-hidden")] {
            u.m.strip_dock = p;
            u.relayout();
            shot(&mut u, &format!("{name}.png"));
        }
    }

    #[test]
    fn auto_set_up_places_the_channels_per_window() {
        use crate::wave::{place, Stats};
        let mut m = Model { win_layout: WinLayout::Quad, ch_win: [0, 1, 1, 0], ..Default::default() };
        let s = Stats { vmin: -1.0, vmax: 1.0, ..Stats::default() };
        place(&mut m, &[(0, s, 1e-3), (1, s, 1e-3), (2, s, 1e-3)], true);
        // CH1 alone in window 1: its 2 Vpp fill about 80 % of 8 div, centred
        assert!(
            2.0 / m.ch[0].volts_per_div() >= 3.9 && m.ch[0].offset.abs() < 1e-6,
            "{} {}",
            m.ch[0].volts_per_div(),
            m.ch[0].offset
        );
        // CH2 and CH3 share window 2: half the height each, one above the other
        assert!(
            2.0 / m.ch[1].volts_per_div() <= 3.2 && m.ch[1].offset > 0.0 && m.ch[2].offset < 0.0,
            "{} {} {}",
            m.ch[1].volts_per_div(),
            m.ch[1].offset,
            m.ch[2].offset
        );
        // one window: three bands
        m.win_layout = WinLayout::One;
        place(&mut m, &[(0, s, 1e-3), (1, s, 1e-3), (2, s, 1e-3)], true);
        assert!(2.0 / m.ch[0].volts_per_div() <= 2.2 && m.ch[0].offset > 0.0);
    }

    #[test]
    fn the_la_lanes_stand_in_their_window() {
        let mut u = ui();
        u.m.la.on = true;
        if !u.m.la_on() {
            return; // a build for a board without LA
        }
        u.m.win_layout = WinLayout::Quad;
        u.spread_wins();
        // four channels and the LA: the LA goes round to window 1, which it shares with CH1
        assert_eq!(u.m.la_win, 0);
        render(&mut u);
        let (band, pitch) = u.la_band(u.m.la.lines().len());
        let w = u.plot.wins[0];
        // at the bottom of the window, half of it or what 16 lanes of the smallest pitch need
        let n = u.m.la.lines().len() as i32;
        assert!(
            band.y >= w.y && band.y + band.h == w.y + w.h - 22 && band.x == w.x && band.w == w.w,
            "{:?} {:?}",
            band,
            w
        );
        assert!(band.h <= (w.h / 2).max(n * 9 + 18), "{:?} {:?}", band, w);
        assert!(pitch >= 9);
        // the plot keeps its height: no strip under it
        assert_eq!(u.la_strip_h(), 0);
        // alone in a window the lanes take all of it
        u.m.ch[3].on = false;
        u.m.la_win = 3;
        render(&mut u);
        let (band, _) = u.la_band(u.m.la.lines().len());
        let w = u.plot.wins[3];
        assert!(band.h > w.h * 3 / 4 && band.y >= w.y, "{:?} {:?}", band, w);
    }

    /// The plot's menu has the third arrangement too, and its mark follows the one in force.
    #[test]
    fn the_plot_menu_and_scpi_choose_the_same_three_arrangements() {
        let mut u = in_layout(Layout::Minimal);
        for (action, want) in [
            (workspace::Action::Windows, Arrangement::Windows),
            (workspace::Action::Stacked, Arrangement::Stacked),
            (workspace::Action::Overlay, Arrangement::Overlay),
        ] {
            u.workspace_action(action);
            assert_eq!(u.arrangement(), want);
        }
    }
}
