// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the Info tab of the channel sheet (Minimal and Studio): what channel `c` is set to and what the
//! acquisition gives it, as seven lines of status, read from the model and the acquisition that are already there
//! (no write to the scope): the scale and offset, the input (coupling, probe, invert),
//! the bandwidth limit with the bandwidth it leaves, the sample rate and memory depth in force, the deskew
//! and the name.

use super::*;

/// The lines of the tab, in the order they are shown.
pub(super) const CH_INFO_LINES: u8 = 7;

impl Ui {
    pub(super) fn chinfo_text(it: Item) -> bool {
        matches!(it, Item::ChInfo(..))
    }

    pub(super) fn chinfo_label(it: Item) -> Option<&'static str> {
        let Item::ChInfo(_, k) = it else { return None };
        Some(match k {
            0 => "Vertical",
            1 => "Input",
            2 => "Bandwidth",
            3 => "Sample rate",
            4 => "Memory",
            5 => "Deskew",
            _ => "Name",
        })
    }

    /// The bandwidth channel `c` has: the limit that is set, and where it leaves less than that, the bandwidth in force
    /// (the rated bandwidth of the model caps every limit; Auto follows the sample rate,
    /// about a quarter of it: assumed, not measured on a unit, docs/protocols/afe-settings.md).
    pub(super) fn ch_bandwidth_text(&self, c: usize, srate: f64) -> String {
        let rated = oos_hal::model::platform().caps.rated_bw_mhz as f64;
        let (name, set) = BWS[self.m.ch[c].bw.min(BWS.len() - 1)];
        let mhz = |v: f64| {
            if v >= 1000.0 {
                format!("{} GHz", (v / 100.0).round() / 10.0)
            } else {
                format!("{} MHz", v.round())
            }
        };
        match self.m.ch[c].bw {
            BW_AUTO => format!("Auto, ~{}", mhz((srate / 4e6).min(rated))),
            BW_FULL => format!("Full, {}", mhz(rated)),
            3 => "GND, input grounded".into(),
            _ if f64::from(set) > rated => format!("{}, {}", name, mhz(rated)),
            _ => name.to_string(),
        }
    }

    pub(super) fn chinfo_value(&self, it: Item) -> Option<String> {
        let Item::ChInfo(c, k) = it else { return None };
        let m = &self.m;
        let (sr, depth) = self.live_rate_depth();
        Some(match k {
            0 => format!(
                "{}/div, {}",
                si_short(m.ch[c].volts_per_div() as f64, m.unit(c)),
                si(m.ch[c].offset as f64, m.unit(c))
            ),
            1 => format!(
                "{}, {}x{}",
                m.ch[c].coupling.name(),
                m.ch[c].probe,
                if m.ch[c].invert { ", inverted" } else { "" }
            ),
            2 => self.ch_bandwidth_text(c, sr),
            3 => si_digits(sr, "Sa/s", 3),
            4 => si_digits(depth as f64, "pts", 3),
            5 => si(m.ch[c].skew as f64, "s"),
            _ => self.ch_name(c),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(u: &Ui, c: usize, k: u8) -> String {
        u.value(Item::ChInfo(c, k))
    }

    // Break it catches: a line of the Info tab that does not follow the setting it reports.
    #[test]
    fn the_lines_follow_the_settings_of_their_channel() {
        let mut u = crate::ui::testkit::demo_ui();
        u.m.ch[1].coupling = Coupling::Ac;
        u.m.ch[1].probe = 10.0;
        u.m.ch[1].invert = true;
        u.m.ch[1].skew = 2e-9;
        u.m.ch_label[1] = "Clock".into();
        assert_eq!(line(&u, 1, 1), "AC, 10x, inverted");
        assert_eq!(line(&u, 1, 5), u.value(Item::ChSkew(1)));
        assert_eq!(line(&u, 1, 6), "Clock");
        assert_eq!(line(&u, 0, 6), "CH1");
        assert!(line(&u, 0, 0).contains("/div"), "{}", line(&u, 0, 0));
        assert_eq!(line(&u, 0, 1), "DC, 1x");
    }

    // Break it catches: a bandwidth line that gives the limit alone where the front end leaves less.
    #[test]
    fn bandwidth_names_the_limit_and_what_it_leaves() {
        let mut u = crate::ui::testkit::demo_ui();
        let rated = oos_hal::model::platform().caps.rated_bw_mhz;
        u.m.ch[0].bw = BW_FULL;
        assert_eq!(u.ch_bandwidth_text(0, 1e9), format!("Full, {} MHz", rated));
        u.m.ch[0].bw = BW_AUTO;
        assert_eq!(u.ch_bandwidth_text(0, 100e6), "Auto, ~25 MHz");
        u.m.ch[0].bw = BW_20M;
        assert_eq!(u.ch_bandwidth_text(0, 1e9), "20 MHz");
        u.m.ch[0].bw = 30;
        assert_eq!(u.ch_bandwidth_text(0, 1e9), format!("2 GHz, {} MHz", rated));
        u.m.ch[0].bw = 3;
        assert!(u.ch_bandwidth_text(0, 1e9).starts_with("GND"));
    }

    // Break it catches: the sample rate and the memory of the tab differing from the top bar's.
    #[test]
    fn rate_and_memory_are_the_ones_in_force() {
        let u = crate::ui::testkit::demo_ui();
        let (sr, depth) = u.live_rate_depth();
        assert_eq!(line(&u, 0, 3), si_digits(sr, "Sa/s", 3));
        assert_eq!(line(&u, 2, 4), si_digits(depth as f64, "pts", 3));
    }

    // Break it catches: the tab missing from the channel sheet, or its lines not all on it.
    #[test]
    fn the_channel_sheet_has_the_tab_with_all_lines() {
        let mut u = crate::ui::testkit::demo_ui();
        u.m.layout = Layout::Studio;
        u.open_menu(MenuId::Channel(2));
        let spec = u.sheet_spec(MenuId::Channel(2)).expect("sheet");
        assert_eq!(spec.tabs.iter().map(|t| t.name).collect::<Vec<_>>(), ["Vertical", "More", "Info"]);
        let want: Vec<Item> = (0..CH_INFO_LINES).map(|k| Item::ChInfo(2, k)).collect();
        assert_eq!(spec.tabs[2].rows, want);
    }
}
