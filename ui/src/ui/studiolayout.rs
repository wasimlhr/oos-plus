// SPDX-License-Identifier: GPL-2.0-or-later
//! The rows of the Studio layout about its own arrangement: Pinned badges, Compact view and Reset layout on
//! the Screen tab of the Display sheet, Offline waveform analysis under the Files tab of the Storage sheet. They
//! were rows of a View menu of the top bar. The other rows of that menu had a place already: the measurement panel
//! is added from Add > Results table and closed in its strip, the expand button of that strip shows the measurements
//! alone, and SIM is an entry of the Menu (its panel opens from the SIM chip of the top bar while a SIM plays).
use super::studio::StudioView;
use super::*;

impl Ui {
    /// The rows the Display menu has in Studio alone, after its other rows.
    pub(super) fn studio_display_rows(&self) -> Vec<Item> {
        if self.m.layout != Layout::Studio {
            return Vec::new();
        }
        vec![Item::StudioBadges, Item::StudioCompact, Item::StudioReset]
    }

    /// The row the Storage menu has in Studio alone, after its other rows.
    pub(super) fn studio_storage_rows(&self) -> Vec<Item> {
        if self.m.layout != Layout::Studio {
            return Vec::new();
        }
        vec![Item::StudioOffline]
    }

    pub(super) fn studiolayout_kind(it: Item) -> Option<Kind> {
        match it {
            Item::StudioBadges | Item::StudioCompact => Some(Kind::Choice),
            Item::StudioReset | Item::StudioOffline => Some(Kind::Action),
            _ => None,
        }
    }

    pub(super) fn studiolayout_label(it: Item) -> Option<&'static str> {
        Some(match it {
            Item::StudioBadges => "Pinned badges",
            Item::StudioCompact => "Compact view",
            Item::StudioReset => "Reset layout",
            Item::StudioOffline => "Offline waveform analysis",
            _ => return None,
        })
    }

    pub(super) fn studiolayout_value(&self, it: Item) -> Option<String> {
        let on = match it {
            Item::StudioBadges => !self.studio.hide_badges,
            Item::StudioCompact => self.studio.compact,
            _ => return None,
        };
        Some(if on { "On" } else { "Off" }.into())
    }

    pub(super) fn studiolayout_options(&self, it: Item) -> Option<Vec<String>> {
        matches!(it, Item::StudioBadges | Item::StudioCompact).then(|| vec!["Off".into(), "On".into()])
    }

    pub(super) fn studiolayout_adjust(&mut self, it: Item, d: i32) -> bool {
        let flip = d.rem_euclid(2) == 1;
        match it {
            Item::StudioBadges if flip => {
                if self.studio.hide_badges {
                    self.studio.hide_badges = false;
                    self.studio.md.hide_undo = None;
                    self.relayout();
                } else {
                    // with the Undo bar, which shows when the sheet has closed (`md_hide_bar`)
                    self.studio_hide_badges();
                }
            }
            Item::StudioCompact if flip => self.studio_set_compact(!self.studio.compact),
            Item::StudioBadges | Item::StudioCompact => {}
            _ => return false,
        }
        true
    }

    /// Both actions close the sheet: what they change is the screen under it.
    pub(super) fn studiolayout_activate(&mut self, it: Item) -> bool {
        match it {
            Item::StudioReset => {
                self.close_menu();
                self.studio_reset_layout();
            }
            Item::StudioOffline => {
                self.close_menu();
                self.studio_set_view(StudioView::Offline);
            }
            _ => return false,
        }
        true
    }

    /// Compact view on or off: smaller lane gaps, badge row and measurement panel.
    pub(super) fn studio_set_compact(&mut self, on: bool) {
        self.studio.compact = on;
        self.studio.panel.scroll_top();
        self.relayout();
    }

    /// Back to the arrangement the layout starts with.
    pub(super) fn studio_reset_layout(&mut self) {
        self.studio_set_view(StudioView::Waveform);
        self.dock_reset();
        self.studio.hide_badges = false;
        // the row is back: no Undo bar for hiding it
        self.studio.md.hide_undo = None;
        self.studio.pinned = None;
        self.studio.unpinned.clear();
        self.studio.popup = None;
        self.studio.panel = Default::default();
        self.studio.compact = false;
        self.set_zoom(false);
        self.relayout();
    }
}

#[cfg(test)]
impl Ui {
    /// Taps `it` on the sheet of `menu` as a finger does: the button of the top bar unless the sheet is open, the tab
    /// that holds the item, then its row (a switch: the half it does not stand at) or its pinned action. The
    /// rectangle that was tapped last.
    pub(super) fn tap_sheet_item(&mut self, menu: MenuId, it: Item) -> Rect {
        let mid = |r: Rect| (r.x + r.w / 2, r.y + r.h / 2);
        if !(self.menu_open && self.menu == menu) {
            let t = self.studio_top();
            let (x, y) = mid(if menu == MenuId::Storage { t.storage } else { t.display });
            self.tap(x, y);
        }
        assert!(self.menu_open && self.menu == menu && self.sheet_active(), "{menu:?} is open as a sheet");
        let spec = self.sheet_spec(menu).expect("a sheet");
        let tab = spec
            .tabs
            .iter()
            .position(|t| t.rows.contains(&it) || t.foot.contains(&it))
            .unwrap_or_else(|| panic!("{it:?} is on no tab"));
        let (x, y) = mid(self.sheet_geo().expect("its geometry").tabs[tab]);
        self.tap(x, y);
        let g = self.sheet_geo().expect("its geometry");
        let r = match g.foot.iter().find(|f| f.0 == it) {
            Some(f) => f.1,
            None => {
                let row = g.rows.iter().find(|r| r.it == it).expect("the row is on the tab");
                assert_eq!(row.kind, super::sheet::RowKind::Toggle, "{it:?}");
                Self::sheet_toggle_half(row.ctrl, self.value(it) != "On")
            }
        };
        let (x, y) = mid(r);
        self.tap(x, y);
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(l: Layout) -> Ui {
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.m.layout = l;
        u.m.ch[1].on = true;
        u.studio.enter = l == Layout::Studio;
        for _ in 0..8 {
            u.step(0.05);
        }
        u
    }

    const OWN: [Item; 4] = [Item::StudioBadges, Item::StudioCompact, Item::StudioReset, Item::StudioOffline];

    fn texts(u: &mut Ui) -> Vec<String> {
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        cv.au_start();
        u.render(&mut cv);
        cv.au
            .take()
            .unwrap()
            .into_iter()
            .filter_map(|e| match e {
                Au::Text { s, .. } => Some(s),
                _ => None,
            })
            .collect()
    }

    /// With OOS_SHOTS set, the screen as a picture in that folder.
    fn shot(u: &mut Ui, name: &str) {
        let mut cv = Canvas::new(u.scr.w, u.scr.h);
        for _ in 0..3 {
            u.step(0.05);
        }
        u.render(&mut cv);
        if let Some(dir) = std::env::var_os("OOS_SHOTS") {
            std::fs::write(std::path::Path::new(&dir).join(format!("studio-{name}.png")), crate::store::png_of(&cv))
                .unwrap();
        }
    }

    /// The top bar of Studio has no View button: Storage is the last button before the Menu, and nothing on the bar
    /// is named View.
    #[test]
    fn the_top_bar_has_no_view_button() {
        let mut u = layout(Layout::Studio);
        let t = u.studio_top();
        assert!(
            t.storage.w > 0 && t.gear.x - t.storage.right() <= 6,
            "Storage stands beside the Menu: {:?} {:?}",
            t.storage,
            t.gear
        );
        assert!(super::super::studio::TOP_LABELS.iter().all(|tier| !tier.contains(&"View")));
        assert!(!texts(&mut u).iter().any(|s| s == "View"), "no View on the screen");
        shot(&mut u, "no-view-top-bar");
    }

    /// Display > Screen has the two switches as rows and Reset layout pinned under them, in Studio alone.
    #[test]
    fn the_display_sheet_of_studio_has_the_badges_the_compact_view_and_reset_layout() {
        let mut u = layout(Layout::Studio);
        let spec = u.sheet_spec(MenuId::Display).expect("the Display sheet");
        let screen = spec.tabs.iter().find(|t| t.name == "Screen").expect("the Screen tab");
        assert!(screen.rows.ends_with(&[Item::StudioBadges, Item::StudioCompact]), "{:?}", screen.rows);
        assert_eq!(screen.foot, [Item::StudioReset]);
        assert!(spec.tabs.iter().filter(|t| t.name != "Screen").all(|t| !t
            .rows
            .iter()
            .chain(&t.foot)
            .any(|it| OWN.contains(it))));
        assert_eq!(
            (u.sheet_row_kind(Item::StudioBadges), u.sheet_row_kind(Item::StudioCompact)),
            (super::super::sheet::RowKind::Toggle, super::super::sheet::RowKind::Toggle)
        );
        // drawn with their names, the action whole on its button
        let button = u.tap_sheet_item(MenuId::Display, Item::StudioBadges);
        assert!(button.w >= 40 && button.h >= 36, "a half of the switch is a finger wide: {button:?}");
        u.tap_sheet_item(MenuId::Display, Item::StudioBadges);
        let seen = texts(&mut u);
        for name in ["Pinned badges", "Compact view", "Reset layout"] {
            assert!(seen.iter().any(|s| s == name), "{name} in {seen:?}");
        }
        shot(&mut u, "no-view-display-screen");
        let reset = u.sheet_geo().unwrap().foot[0].1;
        assert!(u.fonts().mb.width("Reset layout") <= reset.w - 8 && reset.h >= 40, "{reset:?}");
        // the other layouts have none of the rows, in the menu or on a sheet
        for l in [Layout::Default, Layout::Minimal] {
            let d = layout(l);
            for menu in [MenuId::Display, MenuId::Storage] {
                assert!(!d.items(menu).iter().any(|it| OWN.contains(it)), "{l:?} {menu:?}");
                if let Some(spec) = d.sheet_spec(menu) {
                    assert!(
                        !spec.tabs.iter().any(|t| t.rows.iter().chain(&t.foot).any(|it| OWN.contains(it))),
                        "{l:?} {menu:?}"
                    );
                }
            }
        }
    }

    /// Pinned badges hides the row of badges and shows it again; the sheet stays open under the finger.
    #[test]
    fn pinned_badges_hides_the_row_and_shows_it_again() {
        let mut u = layout(Layout::Studio);
        assert!(u.studio_badges_visible() && u.value(Item::StudioBadges) == "On");
        u.tap_sheet_item(MenuId::Display, Item::StudioBadges);
        assert!(u.studio.hide_badges && !u.studio_badges_visible() && u.value(Item::StudioBadges) == "Off");
        assert!(u.menu_open && u.menu == MenuId::Display, "the sheet stays");
        u.tap_sheet_item(MenuId::Display, Item::StudioBadges);
        assert!(!u.studio.hide_badges && u.studio.md.hide_undo.is_none() && u.value(Item::StudioBadges) == "On");
        u.close_menu();
        assert!(u.studio_badges_visible() && u.md_hide_bar().is_none(), "shown again: nothing to undo");
    }

    /// Compact view is the switch under it.
    #[test]
    fn compact_view_is_a_switch_of_the_display_sheet() {
        let mut u = layout(Layout::Studio);
        let row = u.studio_row_px();
        u.tap_sheet_item(MenuId::Display, Item::StudioCompact);
        assert!(u.studio.compact && u.value(Item::StudioCompact) == "On" && u.menu_open);
        assert!(u.studio_row_px() < row, "a smaller badge row");
        u.tap_sheet_item(MenuId::Display, Item::StudioCompact);
        assert!(!u.studio.compact && u.studio_row_px() == row);
    }

    /// Reset layout: the waveform alone, the badges shown, the pins and the size as the layout starts with them. The
    /// sheet closes, so the screen it changed is seen.
    #[test]
    fn reset_layout_brings_back_what_the_layout_starts_with() {
        let mut u = layout(Layout::Studio);
        u.studio_set_view(StudioView::Docked);
        u.studio.hide_badges = true;
        u.studio.pinned = Some(Vec::new());
        u.studio_set_compact(true);
        u.tap_sheet_item(MenuId::Display, Item::StudioReset);
        assert_eq!(
            (u.studio.view, u.studio.hide_badges, u.studio.pinned.is_none(), u.studio.compact),
            (StudioView::Waveform, false, true, false)
        );
        assert!(!u.menu_open);
    }

    /// Storage > Files has the way into the view that analyses the loaded files, in Studio alone; Live leads back.
    #[test]
    fn offline_waveform_analysis_is_an_action_of_the_storage_sheet() {
        let mut u = layout(Layout::Studio);
        let spec = u.sheet_spec(MenuId::Storage).expect("the Storage sheet");
        let files = spec.tabs.iter().find(|t| t.name == "Files").expect("the Files tab");
        assert_eq!(files.foot, [Item::StudioOffline]);
        u.open_menu(MenuId::Storage);
        let tab = u.sheet_geo().unwrap().tabs[1];
        u.tap(tab.x + tab.w / 2, tab.y + tab.h / 2);
        shot(&mut u, "no-view-storage-files");
        assert!(
            spec.tabs.iter().all(|t| !t.rows.contains(&Item::StudioOffline))
                && spec.tabs.iter().filter(|t| t.name != "Files").all(|t| t.foot.is_empty())
        );
        let button = u.tap_sheet_item(MenuId::Storage, Item::StudioOffline);
        assert!(u.fonts().mb.width("Offline waveform analysis") <= button.w - 8 && button.h >= 40, "{button:?}");
        assert!(
            u.studio.view == StudioView::Offline && u.record_mode() && !u.menu_open,
            "the view is up, the sheet is closed"
        );
        assert!(u.studio_offline_empty().is_some(), "no file yet: the card that offers Load file");
        let live = u.studio_top().badge;
        u.tap(live.x + live.w / 2, live.y + live.h / 2);
        assert!(u.studio.view == StudioView::Waveform && !u.record_mode());
    }

    /// What the View menu chose besides: the measurement panel docks from Add, its expand button puts it in a tab
    /// in front of the waveform, the same button leads back, and its close button leaves the waveform alone.
    #[test]
    fn the_measurement_panel_is_added_expanded_and_closed_without_a_menu_of_views() {
        use super::super::dock::VKind;
        let mut u = layout(Layout::Studio);
        let press = |u: &mut Ui, r: Rect| {
            let (x, y) = (r.x + r.w / 2, r.y + r.h / 2);
            u.handle(Ev::Touch { id: 0, phase: Phase::Down, x, y });
            u.handle(Ev::Touch { id: 0, phase: Phase::Up, x, y });
            for _ in 0..3 {
                u.step(0.05);
            }
        };
        u.studio_apply_panel(super::super::sheet::PanelKind::Measurements);
        for _ in 0..3 {
            u.step(0.05);
        }
        assert_eq!(u.studio.view, StudioView::Docked);
        let strip = u.dock_leaf(VKind::Measure).expect("the docked panel");
        let expand = u.dock_extra_rect(VKind::Measure, strip).expect("the expand button");
        press(&mut u, expand);
        assert!(u.dock_tabs_root() && u.studio.view == StudioView::Docked);
        let strip = u.dock_leaf(VKind::Measure).expect("the measurements fill the area");
        assert_eq!(strip, u.dock_region());
        let back = u.dock_extra_rect(VKind::Measure, strip).expect("the way back");
        press(&mut u, back);
        assert!(!u.dock_full_state() && u.studio.view == StudioView::Docked);
        assert!(u.dock_leaf(VKind::Wave).is_some(), "the waveform is beside the panel again");
        let strip = u.dock_leaf(VKind::Measure).unwrap();
        press(&mut u, Ui::dock_close_rect(strip));
        assert_eq!(u.studio.view, StudioView::Waveform);
    }
}
