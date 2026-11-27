// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The two bars around the plot: the top bar (run state, acquisition, menu buttons, network, USB) with its toast, and
//! the bottom row of cards (Menu, the channels, LA, generator, time base, trigger), plus the front-panel LEDs that follow
//! the same state. A new top bar entry needs a decision record first (ADR 0007); a new card goes into `Ui::card_layout`
//! and `Ui::draw_bottom_cards`.

use super::*;

/// What `Ui::draw_aux_card` shows: the card's name and colour, on and selected, its value `big` and the line under it.
struct AuxCard<'a> {
    name: &'a str,
    col: Rgb,
    on: bool,
    sel: bool,
    big: &'a str,
    sub: &'a str,
}

impl Ui {
    /// Run state pill at the left of the top bar (wider for the larger text sizes); taller, with the waveform rate under
    /// the state, while the hardware runs (capture_rate.rs), and wide enough for that line.
    pub(super) fn run_pill(&self) -> Rect {
        if self.m.layout == Layout::Studio { return self.studio_top().run; }
        let f = self.fonts();
        let w = 92.max(30 + f.mb.width("TRIG'D") + 12);
        match self.capture_caption() {
            Some(c) => Rect::new(8, 3, w.max(f.s.width(c) + 12), 34),
            None => Rect::new(8, 6, w, 28),
        }
    }

    /// Network box at the right of the top bar: room for an address, two lines of text (taller at the larger text
    /// sizes).
    pub(super) fn net_box(&self) -> Rect {
        let k = self.m.text_size.index() as i32;
        let (l1, addr) = self.lan_box_lines();
        let w = self.link_width(l1, addr);
        let top = self.frame().top;
        Rect::new(top.right() - METRICS.edge - w, top.y + 5 - 2 * k, w, 30 + 4 * k)
    }

    /// The status boxes that are there now, with where they stand: from the right edge of the bar to the left, in the
    /// order of `STATUS_BOXES` (ADR 0063), 6 px apart.
    pub(super) fn status_boxes(&self) -> Vec<(&'static StatusBox, Rect)> {
        let n = self.net_box();
        let mut x = n.right() + 6;
        let mut out = Vec::new();
        for b in STATUS_BOXES.iter().filter(|b| (b.shown)(self)) {
            let w = (b.width)(self);
            x -= 6 + w;
            out.push((b, if b.chip { Rect::new(x, 5, w, 30) } else { Rect::new(x, n.y, w, n.h) }));
        }
        out
    }

    /// Where the status box `label` stands, while it is there.
    #[cfg(test)]
    pub(super) fn status_box(&self, label: &str) -> Option<Rect> {
        self.status_boxes().into_iter().find(|(b, _)| b.label == label).map(|(_, r)| r)
    }
    /// Start menu button left of the channel cards (all menus and key functions by touch).
    /// The fork's compact bottom bar on the panel; `menu_btn` follows the frame on other screens.
    pub(super) const MENU_BTN: Rect = Rect::new(4, H - BOTTOM_H + 3, 46, 30);
    pub(super) const SINGLE_BTN: Rect = Rect::new(104, 5, 54, 30);
    /// Run state pill at the panel's normal text size (`run_pill` grows with the text).
    #[cfg(test)]
    pub(super) const RUN_PILL: Rect = Rect::new(8, 6, 92, 28);
    pub(super) fn menu_btn(&self) -> Rect {
        self.frame().menu_btn
    }

    /// Single button right of the run state pill (moves over when the pill grows with the text size).
    pub(super) fn single_btn(&self) -> Rect {
        if self.m.layout == Layout::Studio { return self.studio_top().single; }
        let p = self.run_pill();
        Rect::new(p.right() + 4, Self::SINGLE_BTN.y, Self::SINGLE_BTN.w, Self::SINGLE_BTN.h)
    }

    /// Top-bar buttons, right-aligned before the network box: what is used most, and the menus without a front-panel key
    /// (Display, Storage). Everything else is in the start menu: System first in its group, SIM under Analysis; Load file
    /// is a row of the Reference menu.
    pub(super) fn top_buttons(&self) -> Vec<(TopBtn, &'static str, Rect)> {
        if self.m.layout == Layout::Studio { return self.studio_top_buttons(); }
        let f = self.fonts();
        // The bar holds what has no key of the front panel: Display and Storage (ADR 0076). Measure and Cursor are keys
        // and entries of the start menu; System and SIM are in the start menu, Load file a row of the Reference menu.
        let mut all = vec![(TopBtn::Menu(MenuId::Display), "Display"), (TopBtn::Menu(MenuId::Storage), "Storage")];
        if self.m.compact() {
            // oos-plus, Minimal: File loading, simulation and System are direct actions beside the usual tools, and
            // Cursor has its button: without it the cursors were four taps away, through the Menu
            all = vec![
                (TopBtn::Load, "Load file"),
                (TopBtn::Menu(MenuId::Sim), "SIM"),
                (TopBtn::Menu(MenuId::Cursor), "Cursor"),
                (TopBtn::Menu(MenuId::Display), "Display"),
                (TopBtn::Menu(MenuId::Storage), "Storage"),
                (TopBtn::System, "System"),
            ];
        }
        all.retain(|(b, _)| match b {
            TopBtn::Menu(m) => !self.menu_left_out(*m),
            _ => true,
        });
        if self.record_mode() {
            all.insert(0, (TopBtn::Live, "Live"));
        }
        let required = self.single_btn().right() + 8 + f.s.width(&self.acq_caption().0) + 12;
        let fits = |items: &[(TopBtn, &'static str)], padding: i32| {
            let width = items.iter().map(|(_, label)| (f.mb.width(label) + padding).max(40)).sum::<i32>()
                + items.len().saturating_sub(1) as i32 * 6;
            self.top_right() - 8 - width >= required
        };
        let mut padding = 28;
        if !fits(&all, padding) {
            if let Some((_, label)) = all.iter_mut().find(|(b, _)| *b == TopBtn::Load) {
                *label = "Load";
            }
        }
        if !fits(&all, padding) {
            padding = 20;
        }
        for (button, short) in [
            (TopBtn::Menu(MenuId::Storage), "Store"),
            (TopBtn::Menu(MenuId::Display), "Disp"),
            (TopBtn::Menu(MenuId::Measure), "Meas"),
            (TopBtn::System, "Sys"),
            (TopBtn::Menu(MenuId::Cursor), "Cur"),
        ] {
            if fits(&all, padding) {
                break;
            }
            if let Some((_, label)) = all.iter_mut().find(|(b, _)| *b == button) {
                *label = short;
            }
        }
        let mut x = self.top_right() - 8;
        let mut out = Vec::new();
        for &(b, l) in all.iter().rev() {
            let w = (f.mb.width(l) + padding).max(40);
            x -= w;
            out.push((b, l, Rect::new(x, 5, w, 30)));
            x -= 6;
        }
        out.reverse();
        out
    }

    pub(super) fn top_acq_caption(&self, width: i32) -> (String, bool) {
        let (caption, live) = self.acq_caption();
        if !live && self.fonts().s.width(&caption) > width {
            if let Some(r) = self.refs.get(self.active_reference()).and_then(Option::as_ref) {
                return (
                    format!(
                        "{} · {}",
                        if self.session_is_sim { "SIM" } else { "REF" },
                        si_digits(r.v.len() as f64, "pts", 3)
                    ),
                    false,
                );
            }
        }
        (caption, live)
    }

    /// Where the buttons of the top bar end: the leftmost of the status boxes.
    pub(super) fn top_right(&self) -> i32 {
        self.status_boxes().last().map_or(self.net_box().x, |(_, r)| r.x)
    }

    /// The status boxes: drawn with their dot and text, the border in the accent while their page is open.
    fn draw_status_boxes(&self, cv: &mut Canvas) {
        for (b, r) in self.status_boxes() {
            let open = self.menu_open && self.menu_root() == b.page;
            cv.round(r, 5, if open { ACCENT } else { BTN_BORDER }, 255);
            cv.round(r.inset(1), 4, BG, 255);
            (b.draw)(self, cv, r, b.label);
        }
    }

    /// A small box of the top bar: a dot (green: in use) and a short name.
    fn draw_chip(&self, cv: &mut Canvas, r: Rect, name: &str, used: bool) {
        cv.circle(r.x + 9, r.y + 15, 3, if used { GREEN } else { DIM });
        cv.text(&self.fonts().sb, r.x + 16, r.y + 20, name, TEXT);
    }

    /// A box of the top bar with a state dot and two lines; one line, its name beside the green dot, while it has an
    /// address that is not shown.
    fn draw_link(&self, cv: &mut Canvas, r: Rect, l1: &str, addr: Option<&str>) {
        let f = self.fonts();
        let line = self.link_line(addr);
        if line.is_empty() {
            let y = r.y + r.h / 2;
            cv.circle(r.x + 11, y, 3, GREEN);
            cv.text(&f.sb, r.x + 20, y + f.sb.size * 36 / 100, l1, TEXT);
            return;
        }
        // first line under the top edge, the second on the bottom edge
        let (cap, y1) = (f.s.size * 72 / 100, r.y + 5 + f.s.size * 72 / 100);
        cv.circle(r.x + 11, y1 - cap / 2 + 1, 3, if addr.is_some() { GREEN } else { DIM });
        cv.text(&f.s, r.x + 20, y1, l1, DIM);
        cv.text(&f.sb, r.x + 20, r.y + r.h - 4, line, TEXT);
    }

    /// The second line of a link box: the address only where System > Network > Show address is on, since
    /// pictures and photos of the screen are passed on; else none while the link has one (the green dot says it), a
    /// dash where it has none.
    pub(super) fn link_line<'a>(&self, addr: Option<&'a str>) -> &'a str {
        match addr {
            Some(a) if self.sys.show_addr => a,
            Some(_) => "",
            None => "-",
        }
    }

    /// The width of a link box: as wide as its lines, so it is narrow without an address shown and as wide as an
    /// address only while one is (ADR 0063).
    pub(super) fn link_width(&self, l1: &str, addr: Option<&str>) -> i32 {
        let f = self.fonts();
        let line = self.link_line(addr);
        if line.is_empty() {
            return 20 + f.sb.width(l1) + 8;
        }
        20 + f.s.width(l1).max(f.sb.width(line)) + 8
    }

    /// The two lines of the LAN box: how the address came, and the address; an address without a cable is only the
    /// fallback waiting for one.
    pub(super) fn lan_box_lines(&self) -> (&'static str, Option<&str>) {
        let lan = self.addr_of("eth0").filter(|_| self.net.eth_link);
        let l1 = if lan.is_none() {
            "LAN"
        } else if self.net.fixed {
            "LAN \u{b7} local"
        } else {
            "LAN \u{b7} DHCP"
        };
        (l1, lan)
    }

    /// The two lines of the WLAN box.
    pub(super) fn wlan_box_lines(&self) -> (&'static str, Option<&str>) {
        ("WLAN", self.addr_of(oos_hal::wlan::IFACE))
    }

    /// The address of an interface, if it has one.
    pub(super) fn addr_of(&self, name: &str) -> Option<&str> {
        self.net.ifs.iter().find(|i| i.0 == name).map(|i| i.1.as_str())
    }

    /// Touch area of a top-bar button: the full bar height.
    pub(super) fn top_hit(&self, r: Rect) -> Rect {
        let top = self.frame().top;
        Rect::new(r.x - 2, top.y, r.w + 4, top.h)
    }

    /// The acquisition mode as the top bar names it.
    fn acq_name(&self) -> String {
        match self.m.acq {
            AcqMode::Normal => "Normal".to_string(),
            AcqMode::Peak => "Peak".to_string(),
            AcqMode::Average if self.src.is_hw() && board().avg_hw(&self.m) => {
                format!("Average {} HW", self.m.avg_count)
            }
            AcqMode::Average => format!("Average {}", self.m.avg_count),
            AcqMode::HiRes => "High res".to_string(),
        }
    }

    /// The acquisition text of the top bar from `x` on, before the buttons at `room`: sample rate and depth in the bold
    /// font, else in the regular one, else without the spaces; the mode only when it fits too. Larger text with a long
    /// depth ran under the buttons. (bold, rate and depth, mode)
    pub(super) fn top_acq(
        f: &Fonts,
        x: i32,
        room: i32,
        sr: f64,
        depth: f64,
        mode: String,
    ) -> (bool, String, Option<String>) {
        let (rate, pts) = (si_digits(sr, "Sa/s", 3), si_digits(depth, "pts", 3));
        let full = format!("{} \u{b7} {}", rate, pts);
        let (bold, main) = if x + f.mb.width(&full) <= room {
            (true, full)
        } else if x + f.m.width(&full) <= room {
            (false, full)
        } else {
            // tighter, then the rate alone (large text with the view buttons, a USB stick, a WLAN stick)
            let tight = format!("{}\u{b7}{}", rate.replace(' ', ""), pts.replace(' ', ""));
            let alone = rate.replace(' ', "");
            let main = [tight, alone].into_iter().find(|t| x + f.m.width(t) <= room).unwrap_or_default();
            (false, main)
        };
        let end = x + if bold { f.mb.width(&main) } else { f.m.width(&main) };
        let mode = format!(" \u{b7} {}", mode);
        let mode = (end + f.m.width(&mode) <= room).then_some(mode);
        (bold, main, mode)
    }

    pub(super) fn draw_top(&self, cv: &mut Canvas) {
        if self.m.layout == Layout::Studio {
            return self.draw_studio_top(cv);
        }
        let f = self.fonts();
        let top = self.frame().top;
        cv.fill(top, BAR);
        cv.hline(top.x, top.right() - 1, top.bottom() - 1, LINE);
        // run / trigger state; offline mode reads the record (playing or held) instead of a trigger
        let (txt, col) = self.run_caption();
        let r = self.run_pill();
        cv.round(r, 6, mix(BAR, col, 80), 255);
        cv.round(r.inset(1), 5, mix(BAR, col, 36), 255);
        // with the waveform rate the state takes the upper part and the rate the line under it
        let capture = self.capture_caption();
        let h = if capture.is_some() { 18 } else { r.h };
        cv.circle(r.x + 16, r.y + h / 2, 4, col);
        cv.text_in(&f.mb, Rect::new(r.x + 30, r.y, r.w - 30, h), 0, txt, col);
        if let Some(c) = capture {
            cv.text_in(&f.s, Rect::new(r.x, r.y + h, r.w, r.h - h), 1, c, TEXT);
        }
        let single = self.single_btn();
        if single.w > 0 {
            btn(cv, single, BTN, BTN_BORDER);
            cv.text_in(&f.s, single, 1, "Single", TEXT);
        }
        // acquisition: sample rate and depth of the hardware when it runs, else of the model; mode.
        // Offline mode shows the reference's own rate and length, with no acquire mode.
        let buttons = self.top_buttons();
        let room = self.view_box().map_or(buttons.first().map_or(self.top_right(), |b| b.2.x), |v| v.x) - 12;
        let x0 = single.right() + 8;
        let (caption, live_acq) = self.top_acq_caption((room - x0).max(1));
        let acq_open =
            self.menu_open && self.menu_root() == if self.record_mode() { MenuId::Ref } else { MenuId::Acquire };
        let tcol = if acq_open { ACCENT } else { TEXT };
        cv.set_clip(Rect::new(x0, top.y, (room - x0).max(1), top.h));
        if live_acq {
            let (sr, depth) = self.live_rate_depth();
            let (bold, main, mode) = Self::top_acq(f, x0, room, sr, depth as f64, self.acq_name());
            let x = cv.text(if bold { &f.mb } else { &f.m }, x0, 25, &main, tcol);
            if let Some(m) = mode {
                cv.text(&f.m, x, 25, &m, tcol);
            }
        } else {
            cv.text(&f.s, x0, 25, &caption, tcol);
        }
        cv.no_clip();
        self.draw_view_box(cv);
        // buttons (the open menu in orange)
        let root = self.menu_root();
        for (b, l, r) in buttons {
            let hl = match b {
                TopBtn::Live => true,
                TopBtn::Menu(m) => self.menu_open && root == m,
                TopBtn::System => self.menu_open && root == MenuId::Utility,
                TopBtn::Load => false,
            };
            btn(cv, r, BTN, if hl { ACCENT } else { BTN_BORDER });
            cv.text_in(&f.mb, r, 1, l, if hl { ACCENT } else { TEXT });
        }
        self.draw_status_boxes(cv);
    }

    /// Messages as an amber pill at the top centre of the plot; next to or under an open menu dialog that would
    /// cover it (it never hides dialog controls).
    /// A message that reports something done: it gets a tick, not the warning triangle.
    /// A message that only informs (nothing to clear, which view, a channel turned on): a neutral pill with an "i".
    pub(crate) fn toast_is_info(s: &str) -> bool {
        s == NOTHING_TO_CLEAR || s == "Views off" || s.ends_with(" turned on") || s.starts_with("Generating ") || (s.starts_with("View ") && s.contains(" of "))
            // the SCPI recorder says what it wrote: nothing went wrong
            || s == "SCPI recorder on" || s.starts_with("Recorded ") || s.starts_with("Running ")
    }

    pub(crate) fn toast_is_success(s: &str) -> bool {
        matches!(s, "Eye cleared" | "Counter cleared" | "Limit cleared" | "Counts reset" | "Mask cleared") || s.starts_with("Copied ") || (s.starts_with("Zone ") && s.ends_with(" deleted")) || s.ends_with(" added") || s.contains(" added to R") || s.ends_with(" removed") || s.contains(" \u{b7} ") || s.starts_with("Loaded ") || s.starts_with("Saved ") || s.starts_with("Ran ") || s.contains(" loaded from ") || s.starts_with("Restored ") || s.contains(" saved to ") || s.starts_with("Returned to live") || s.starts_with("Fit ")
    }

    pub(super) fn draw_toast(&self, cv: &mut Canvas) {
        let Some((s, _)) = &self.toast else { return };
        let f = self.fonts();
        let tw = f.mb.width(s) + 44;
        let body = self.frame().body;
        let mut r = Rect::new(body.x + body.w / 2 - tw / 2, body.y + 4, tw, 28);
        if self.eye_open() {
            let pane = self.eye_pane_rect();
            if overlaps(pane, r) {
                let wave = self.plot.grid;
                let room = pane.x - wave.x - 12;
                if room >= tw {
                    r.x = wave.x + (room - tw) / 2;
                }
            }
        }
        let dlg = self.menu_rect().filter(|d| overlaps(*d, r));
        if let Some(d) = dlg {
            let (left, right) = (d.x - 6, body.right() - 6 - d.right());
            if right >= tw + 16 {
                r.x = d.x + d.w + (right - tw) / 2;
            } else if left >= tw + 16 {
                r.x = 6 + (left - tw) / 2;
            } else {
                r.y = d.y + d.h + 6;
                // never over the bottom bar: above the dialog when there is room, else where it was
                if r.y + r.h > self.frame().bottom.y { r.y = if d.y - r.h - 6 >= body.y { d.y - r.h - 6 } else { body.y + 4 }; }
            }
        }
        if Self::toast_is_info(s) {
            // information, not a warning: a neutral pill with an "i"
            cv.round(r, 6, mix(BAR, TEXT, 80), 255);
            cv.round(r.inset(1), 5, mix(BAR, TEXT, 20), 255);
            let (cx, cy) = (r.x + 18, r.y + 14);
            cv.circle(cx, cy, 8, TEXT);
            cv.circle(cx, cy, 7, mix(BAR, TEXT, 20));
            cv.pixel(cx, cy - 4, TEXT, 255);
            cv.vline(cx, cy - 1, cy + 4, TEXT);
            cv.text(&f.mb, r.x + 32, r.y + 19, s, TEXT);
            return;
        }
        cv.round(r, 6, mix(BAR, AMBER, 110), 255);
        cv.round(r.inset(1), 5, mix(BAR, AMBER, 34), 255);
        if Self::toast_is_success(s) {
            // done, not a warning: a tick in green
            cv.round(r, 6, mix(BAR, GREEN, 110), 255);
            cv.round(r.inset(1), 5, mix(BAR, GREEN, 34), 255);
            for d in [0, 1] { cv.line(r.x + 11, r.y + 15 + d, r.x + 16, r.y + 20 + d, GREEN); cv.line(r.x + 16, r.y + 20 + d, r.x + 25, r.y + 8 + d, GREEN); }
            cv.text(&f.mb, r.x + 32, r.y + 19, s, GREEN);
            return;
        }
        cv.triangle([(r.x + 11, r.y + 20), (r.x + 25, r.y + 20), (r.x + 18, r.y + 7)], AMBER);
        cv.vline(r.x + 18, r.y + 11, r.y + 15, DARK_TEXT);
        cv.pixel(r.x + 18, r.y + 18, DARK_TEXT, 255);
        cv.text(&f.mb, r.x + 32, r.y + 19, s, AMBER);
    }

    /// The menu a sub menu or picker belongs to (button and card highlights, dialog position).
    pub(super) fn menu_root(&self) -> MenuId {
        let m = match (self.menu, self.pick) {
            (MenuId::Pick, Some(p)) => p.menu,
            (m, _) => m,
        };
        if matches!(m, MenuId::MeasAdd | MenuId::MeasSelected) {
            MenuId::Measure
        } else if m == MenuId::SimAdvanced {
            MenuId::Sim
        } else {
            m
        }
    }

    /// Bottom cards after the Menu button: channels (wide when on, up to 213 px), TIME (at least 130 px), trigger
    /// (at least 222 px).
    pub(super) fn cards(&self) -> ([Rect; NCH], Rect, Rect) {
        let c = self.card_layout();
        (c.ch, c.time, c.trig)
    }

    /// The cards of the logic analyser and the generator, where the model has them.
    pub(super) fn aux_cards(&self) -> (Option<Rect>, Option<Rect>) {
        let c = self.card_layout();
        (c.la, c.gen)
    }

    /// All bottom cards. LA and generator sit behind the channels, wide like them when they are on and the channels keep
    /// room for their text, else narrow.
    pub(super) fn card_layout(&self) -> Cards {
        if self.m.layout == Layout::Studio {
            self.studio_cards()
        } else if self.m.compact() {
            self.compact_cards()
        } else if self.record_mode() {
            // a stored record or the simulator on the screen (Offline, Take over, SIM): no analog channel, LA or
            // generator cards, the time base and trigger stay (as the compact bar has it, upstream PR #8)
            let c = self.classic_card_layout();
            Cards { ch: c.ch.map(|r| Rect::new(r.x, r.y, 0, r.h)), la: None, gen: None, ..c }
        } else {
            self.classic_card_layout()
        }
    }

    /// The stock OOS bottom row (Display > Layout > Default).
    fn classic_card_layout(&self) -> Cards {
        let row = self.frame().cards;
        let (y, h) = (row.y, row.h);
        // 2-channel models: no cards for CH3/CH4 (zero width, never drawn or hit)
        let n = n_ch();
        let n_on = (0..n).filter(|&c| self.m.ch[c].on).count() as i32;
        let n_off = n as i32 - n_on;
        let caps = &oos_hal::model::platform().caps;
        let aux = [(caps.la, self.m.la.on), (caps.has_gen() && !self.m.gen_hidden, self.m.afg_on)];
        let ref_card = !self.record_mode() && !self.shown_refs().is_empty();
        let start = row.x;
        let room = |wide: bool| {
            let a: i32 = aux.iter().filter(|a| a.0).map(|a| if a.1 && wide { AUX_WIDE } else { AUX_NARROW } + 7).sum::<i32>()
                + if ref_card { AUX_NARROW + 7 } else { 0 };
            row.right() - start - n_off * (56 + 7) - n_on * 7 - a - (130 + 7) - 222
        };
        let wide = n_on == 0 || room(true) / n_on.max(1) >= CH_MIN;
        let won = if n_on > 0 { (room(wide) / n_on).min(213) } else { 0 };
        let mut x = start;
        let mut ch = [Rect::new(0, 0, 0, 0); NCH];
        for (c, r) in ch.iter_mut().enumerate() {
            if c >= n {
                *r = Rect::new(x, y, 0, h);
                continue;
            }
            let w = if self.m.ch[c].on { won } else { 56 };
            *r = Rect::new(x, y, w, h);
            x += w + 7;
        }
        let mut next = |(has, on): (bool, bool)| {
            has.then(|| {
                let w = if on && wide { AUX_WIDE } else { AUX_NARROW };
                let r = Rect::new(x, y, w, h);
                x += w + 7;
                r
            })
        };
        let (la, gen) = (next(aux[0]), next(aux[1]));
        let refs = ref_card.then(|| { let r = Rect::new(x, y, AUX_NARROW, h); x += AUX_NARROW + 7; r });
        let rest = row.right() - x - 7;
        let tw = ((rest as f32 * 0.42) as i32).min(rest - 222).max(130);
        let tw = tw.min(TIME_CARD_MAX);
        let trw = (rest - tw).min(TRIG_CARD_MAX);
        // a wide screen: TIME and trigger keep their largest size and move to the right edge, the room stays between
        // the channels and them (as in the top bar)
        let x = row.right() - trw - 7 - tw;
        Cards { ch, la, gen, refs, time: Rect::new(x, y, tw, h), trig: Rect::new(x + tw + 7, y, trw, h) }
    }

    /// The big digits of a card while `s` fits `room`, else the next size: nothing is cut off with every card on.
    pub(super) fn card_font(f: &'static Fonts, s: &str, room: i32) -> &'static Font {
        fit_font(&[&f.l, &f.m, &Fonts::get(0).m], room, s)
    }

    /// The line under V/div of a channel card: coupling, probe, bandwidth, invert, offset.
    pub(super) fn card_sub(&self, c: usize) -> String {
        let ch = &self.m.ch[c];
        let mut sub = vec![ch.coupling.name().to_string(), format!("{}\u{d7}", ch.probe)];
        if ch.bw != 0 && ch.bw != BW_FULL {
            sub.push(format!("BW {}", BWS[ch.bw].0));
        }
        if ch.invert {
            sub.push("Inv".into());
        }
        sub.push(si_short(ch.offset as f64, ch.unit.name()));
        sub.join(" \u{b7} ")
    }

    /// Font of that line, one for all channel cards: the normal size when a line would run out of its card at the
    /// text size.
    pub(super) fn card_sub_font(&self) -> &'static Font {
        let (f, chs) = (self.fonts(), self.cards().0);
        let all_fit = (0..NCH)
            .filter(|&c| chs[c].w > 0 && self.m.ch[c].on)
            .all(|c| f.s.width(&self.card_sub(c)) <= chs[c].w - 22);
        if all_fit {
            &f.s
        } else {
            &Fonts::get(0).s
        }
    }

    /// Trigger card buttons, one over the other at the right edge (touch): source, sweep; the rest of the card, and with
    /// it the level, opens the menu.
    pub(super) fn trig_parts(r: Rect) -> (Rect, Rect) {
        let (w, h) = (66, (r.h - 12 - 4) / 2);
        let x = r.x + r.w - w - 6;
        (Rect::new(x, r.y + 6, w, h), Rect::new(x, r.y + 6 + h + 4, w, h))
    }

    /// The refs that are shown over the live channels, in slot order.
    pub(super) fn shown_refs(&self) -> Vec<usize> {
        (0..NREF).filter(|&i| self.refs[i].as_ref().is_some_and(|r| r.on)).collect()
    }

    /// REF card of the Default bottom row: the active ref; a tap makes the next shown ref the active source.
    pub(super) fn draw_ref_card(&self, cv: &mut Canvas, r: Rect) {
        let f = self.fonts();
        let shown = self.shown_refs();
        let active = shown.iter().copied().find(|&i| self.selected_workspace_source() == Some(WaveId::Ref(i)));
        let col = active.map_or(LABEL, |i| self.wave_color(WaveId::Ref(i)));
        cv.round(r, 7, if active.is_some() { col } else { CARD_BORDER }, 255);
        cv.round(r.inset(2), 5, BAR, 255);
        cv.set_clip(r.inset(3));
        cv.text_in(&f.sb, Rect::new(r.x, r.y + 8, r.w, 18), 1, "REF", if active.is_some() { col } else { LABEL });
        let name = active.map_or_else(|| format!("{} on", shown.len()), |i| format!("R{}", i + 1));
        cv.text_in(&f.mb, Rect::new(r.x, r.y + 30, r.w, 24), 1, &name, TEXT);
        cv.no_clip();
    }

    /// Makes the next shown ref the active source (cursors, measurements and the cards follow it).
    pub(super) fn cycle_active_ref(&mut self) {
        let shown = self.shown_refs();
        let Some(&first) = shown.first() else { return };
        let next = match self.selected_workspace_source() {
            Some(WaveId::Ref(i)) => shown.iter().copied().find(|&k| k > i).unwrap_or(first),
            _ => first,
        };
        self.select_workspace_source(WaveId::Ref(next));
        self.toast(&format!("Active source R{}", next + 1));
    }

    /// A card like the channels': on, wide with its values; on, narrow with a full dot; off, with a ring.
    fn draw_aux_card(&self, cv: &mut Canvas, r: Rect, card: AuxCard) {
        let AuxCard { name, col, on, sel, big, sub } = card;
        let f = self.fonts();
        cv.round(r, 7, if sel || on { col } else { CARD_BORDER }, 255);
        cv.round(r.inset(2), 5, BAR, 255);
        cv.set_clip(r.inset(3));
        if on && r.w > AUX_NARROW {
            cv.fill(Rect::new(r.x + 6, r.y + 10, 3, r.h - 20), col);
            let (yn, yb, ys) = self.lay().card;
            cv.text(&f.sb, r.x + 16, r.y + yn, name, col);
            cv.circle(r.x + r.w - 16, r.y + 14, 5, col);
            cv.text(fit_font(&[&f.mb, &Fonts::get(0).mb], r.w - 22, big), r.x + 16, r.y + yb, big, TEXT);
            cv.text(fit_font(&[&f.s, &Fonts::get(0).s], r.w - 22, sub), r.x + 16, r.y + ys, sub, DIM);
        } else {
            cv.text_in(&f.sb, Rect::new(r.x, r.y + 8, r.w, 18), 1, name, if on { col } else { LABEL });
            let (cx, cy) = (r.x + r.w / 2, r.y + 45);
            cv.circle(cx, cy, 7, col);
            if !on {
                cv.circle(cx, cy, 6, BAR);
            }
        }
        cv.no_clip();
    }

    pub(super) fn draw_bottom(&self, cv: &mut Canvas) {
        if self.m.layout == Layout::Studio { self.draw_studio_bottom(cv) } else if self.m.compact() { self.draw_workspace_footer(cv) } else { self.draw_classic_bottom(cv) }
    }

    /// The stock OOS cards: Menu, channels, LA, generator, TIME and trigger (Display > Layout > Default).
    fn draw_classic_bottom(&self, cv: &mut Canvas) {
        let f = self.fonts();
        let (yn, yb, ys) = self.lay().card;
        // a line that would run out of a narrow card takes the normal size
        let small = |s: &str, room: i32| fit_font(&[&f.s, &Fonts::get(0).s], room, s);
        cv.fill(self.frame().bottom, BG);
        let root = if self.menu_open { Some(self.menu_root()) } else { None };
        self.draw_menu_button(cv, f, root);
        let (chs, tr, trg) = self.cards();
        let sub_font = self.card_sub_font();
        for (c, &r) in chs.iter().enumerate() {
            if r.w == 0 {
                continue;
            }
            let ch = &self.m.ch[c];
            let col = CH_COLORS[c];
            let sel = root == Some(MenuId::Channel(c));
            let border = if ch.on && (self.m.active == c || sel) { col } else { CARD_BORDER };
            cv.round(r, 7, border, 255);
            cv.round(r.inset(2), 5, BAR, 255);
            // narrow cards: the text stays inside
            cv.set_clip(r.inset(3));
            let name = self.ch_name(c);
            if ch.on {
                cv.fill(Rect::new(r.x + 6, r.y + 10, 3, r.h - 20), col);
                cv.text(&f.sb, r.x + 16, r.y + yn, &name, col);
                cv.circle(r.x + r.w - 16, r.y + 14, 5, col);
                let scale = format!("{}/div", si_short(ch.volts_per_div() as f64, ch.unit.name()));
                cv.text(Self::card_font(f, &scale, r.w - 24), r.x + 16, r.y + yb, &scale, TEXT);
                cv.text(sub_font, r.x + 16, r.y + ys, &self.card_sub(c), DIM);
            } else {
                cv.text_in(&f.sb, Rect::new(r.x, r.y + 8, r.w, 18), 1, &name, LABEL);
                let (cx, cy) = (r.x + r.w / 2, r.y + 45);
                cv.circle(cx, cy, 7, col);
                cv.circle(cx, cy, 6, BAR);
            }
            cv.no_clip();
        }
        // logic analyser and generator, in the manner of the channels
        let (la, gen) = self.aux_cards();
        if let Some(r) = la {
            let sel = matches!(root, Some(MenuId::La | MenuId::LaLines | MenuId::LaPat));
            let l = &self.m.la;
            let sub = format!("{} \u{b7} {}", si_short(l.thr(0) as f64, "V"), si_short(l.thr(1) as f64, "V"));
            // grey without a probe: nothing to switch on
            let col = if l.on || self.la_probe_ok() { la::line_color(0) } else { CARD_BORDER };
            let big = format!("{} lines", l.lines().len());
            self.draw_aux_card(cv, r, AuxCard { name: "LA", col, on: l.on, sel, big: &big, sub: &sub });
        }
        if let Some(r) = gen {
            let (big, sub) = self.afg_card_text();
            let sel = root == Some(MenuId::Afg);
            self.draw_aux_card(
                cv,
                r,
                AuxCard { name: "GEN", col: GEN_COLOR, on: self.m.afg_on, sel, big: &big, sub: &sub },
            );
        }
        if let Some(r) = self.card_layout().refs { self.draw_ref_card(cv, r); }
        // time base (sample rate and depth are in the top bar)
        let r = tr;
        let sel = root == Some(MenuId::Horizontal);
        cv.round(r, 7, if sel { ACCENT } else { CARD_BORDER }, 255);
        cv.round(r.inset(2), 5, BAR, 255);
        cv.text(&f.sb, r.x + 12, r.y + yn, "TIME", LABEL);
        let tdiv = format!("{}/div", si_short(self.m.time_per_div() as f64, "s"));
        cv.text(Self::card_font(f, &tdiv, r.w - 20), r.x + 12, r.y + yb, &tdiv, TEXT);
        if self.m.zoom {
            cv.text(
                &f.sb,
                r.x + 12,
                r.y + ys,
                &format!("zoom {}/div", si_short(tscales()[self.m.zscale] as f64, "s")),
                ACCENT,
            );
        } else {
            let t = format!("delay {}", si_short(self.m.delay as f64, "s"));
            cv.text(small(&t, r.w - 18), r.x + 12, r.y + ys, &t, DIM);
        }
        self.draw_trigger_card(cv, f, trg, root, yn);
    }

    /// The start menu button: three bars and "Menu"; amber "Locked" while touch is locked (a tap unlocks).
    fn draw_menu_button(&self, cv: &mut Canvas, f: &'static Fonts, root: Option<MenuId>) {
        let r = self.menu_btn();
        let (hl, lock) = (root == Some(MenuId::Main), self.m.touch_lock);
        cv.round(
            r,
            7,
            if hl {
                ACCENT
            } else if lock {
                AMBER
            } else {
                CARD_BORDER
            },
            255,
        );
        cv.round(r.inset(2), 5, BAR, 255);
        let col = if hl {
            ACCENT
        } else if lock {
            AMBER
        } else {
            TEXT
        };
        for k in 0..3 {
            cv.fill(Rect::new(r.x + r.w / 2 - 11, r.y + 16 + k * 7, 22, 3), col);
        }
        cv.text_in(&f.sb, Rect::new(r.x, r.y + 42, r.w, 16), 1, if lock { "Locked" } else { "Menu" }, col);
    }

    /// The trigger card: type, slope / polarity / standard and level; source and sweep buttons.
    fn draw_trigger_card(&self, cv: &mut Canvas, f: &'static Fonts, r: Rect, root: Option<MenuId>, yn: i32) {
        let sel = root == Some(MenuId::Trigger);
        cv.round(r, 7, if sel { ACCENT } else { CARD_BORDER }, 255);
        cv.round(r.inset(2), 5, BAR, 255);
        // type and slope on the first line, the level under them: it has the width up to the buttons
        let tx = cv.text(&f.sb, r.x + 12, r.y + yn, &self.m.trig_type.name().to_uppercase(), LABEL) + 8;
        if matches!(
            self.m.trig_type,
            TrigType::Edge | TrigType::Slope | TrigType::Timeout | TrigType::NthEdge | TrigType::Delay
        ) {
            draw_slope(cv, tx, r.y + 6, self.m.slope, ACCENT);
        } else {
            let tag = match self.m.trig_type {
                TrigType::Pulse | TrigType::Runt => {
                    if self.m.trig_pol == Pol::Pos {
                        "+"
                    } else {
                        "-"
                    }
                }
                TrigType::Video => {
                    if self.m.video_std == VideoStd::Pal {
                        "P"
                    } else {
                        "N"
                    }
                }
                _ => "",
            };
            cv.text(&f.sb, tx, r.y + yn, tag, ACCENT);
        }
        // an LA line has the pod's threshold, not a level of its own
        let lv = match self.m.trig_line() {
            Some(n) => si(self.m.la.thr(n as usize / 8) as f64, "V"),
            None => si(self.trig_level_shown() as f64, self.m.unit(self.m.trig_src)),
        };
        let (sb, mb) = Self::trig_parts(r);
        // the big digits while they fit
        cv.text(Self::card_font(f, &lv, sb.x - r.x - 18), r.x + 12, r.y + 50, &lv, TEXT);
        let mode = match self.m.trig_mode {
            TrigMode::Auto => "Auto",
            TrigMode::Normal => "Normal",
            TrigMode::Single => "Single",
        };
        let ts = self.m.trig_src;
        let (src, scol) = match self.m.trig_line() {
            Some(n) => (format!("D{}", n), la::line_color(n)),
            None => (format!("CH{}", ts + 1), CH_COLORS[ts]),
        };
        for (b, val, col) in [(sb, src, scol), (mb, mode.to_string(), TEXT)] {
            btn(cv, b, BTN, BTN_BORDER);
            cv.text_in(&f.sb, b, 1, &val, col);
        }
    }

    /// Front-panel LED mask (19 LEDs, stock order; `Inputs::set_leds` maps it to
    /// the MCU's hardware bits):
    /// 0-3 CH1-CH4, 4 RUN green, 5 STOP red, 6 SINGLE red, 7 SINGLE orange, 8 LA, 9 G1, 10 G2, 11 TOUCH,
    /// 12 A up/down, 13 B left/right, 14 SLOPE1, 15 SLOPE2, 16 A left/right, 17 B up/down, 18 INTENSITY.
    pub fn leds(&self) -> u32 {
        let mut m = 0u32;
        for c in 0..NCH {
            if self.m.ch[c].on {
                m |= 1 << c;
            }
        }
        if self.m.trig_mode == TrigMode::Single && self.m.run == RunState::Run {
            m |= (1 << 6) | (1 << 7); // the stock system always drives both SINGLE LEDs together
        } else if self.m.run == RunState::Run {
            m |= 1 << 4;
        } else {
            m |= 1 << 5;
        }
        if self.m.la.on {
            m |= 1 << 8;
        }
        if self.m.afg_on {
            m |= 1 << 9; // G1 (GI); G2 has no LED on the DHO900
        }
        if self.m.touch_lock {
            m |= 1 << 11;
        }
        match self.m.slope {
            Slope::Rise => m |= 1 << 14,
            Slope::Fall => m |= 1 << 15,
            Slope::Either => m |= (1 << 14) | (1 << 15),
        }
        m
    }
}

pub(super) fn draw_slope(cv: &mut Canvas, x: i32, y: i32, s: Slope, col: Rgb) {
    draw_slope1(cv, x, y, s, col);
    draw_slope1(cv, x + 1, y, s, col);
}

fn draw_slope1(cv: &mut Canvas, x: i32, y: i32, s: Slope, col: Rgb) {
    // 14 x 16 icon: rising / falling edge
    let (x0, x1, x2, x3) = (x, x + 5, x + 9, x + 14);
    let (ylo, yhi) = (y + 14, y + 2);
    match s {
        Slope::Rise => {
            cv.hline(x0, x1, ylo, col);
            cv.line(x1, ylo, x2, yhi, col);
            cv.hline(x2, x3, yhi, col);
        }
        Slope::Fall => {
            cv.hline(x0, x1, yhi, col);
            cv.line(x1, yhi, x2, ylo, col);
            cv.hline(x2, x3, ylo, col);
        }
        Slope::Either => {
            cv.line(x0, ylo, x0 + 6, yhi, col);
            cv.line(x0 + 7, yhi, x3, ylo, col);
        }
    }
}

/// A status box of the top bar (ADR 0063): something plugged in or connected, with its state, and its page a tap opens.
pub(super) struct StatusBox {
    /// its name, which a small box shows
    pub label: &'static str,
    /// whether it is there now
    pub shown: fn(&Ui) -> bool,
    /// its width
    pub width: fn(&Ui) -> i32,
    /// a small box of one line (30 px high) or, else, a box of two lines as high as the LAN box
    pub chip: bool,
    /// its dot and its text, inside the border; it is given the box's label
    pub draw: fn(&Ui, &mut Canvas, Rect, &'static str),
    /// the page a tap opens, and whose open menu lights its border
    pub page: MenuId,
}

/// The status boxes, from the right edge of the top bar to the left: LAN always, WLAN, Bluetooth and USB while their
/// stick is there. At most five (ADR 0063).
pub(super) const STATUS_BOXES: [StatusBox; 4] = [
    StatusBox {
        label: "LAN",
        shown: |_| true,
        width: |u| u.net_box().w,
        chip: false,
        draw: |u, cv, r, _| {
            let (l1, lan) = u.lan_box_lines();
            u.draw_link(cv, r, l1, lan)
        },
        page: MenuId::Network,
    },
    StatusBox {
        label: "WLAN",
        shown: |u| u.net.wlan_stick,
        // as wide as its lines, as LAN: room for an address while one is shown (issue #23)
        width: |u| {
            let (l1, addr) = u.wlan_box_lines();
            u.link_width(l1, addr)
        },
        chip: false,
        draw: |u, cv, r, _| {
            let (l1, addr) = u.wlan_box_lines();
            u.draw_link(cv, r, l1, addr)
        },
        page: MenuId::Network,
    },
    // green while a mouse or keyboard is connected through the stick
    StatusBox {
        label: "BT",
        shown: |u| u.bt.stick,
        width: |_| 46,
        chip: true,
        draw: |u, cv, r, label| u.draw_chip(cv, r, label, u.bt.inputs > 0),
        page: MenuId::Bluetooth,
    },
    StatusBox {
        label: "USB",
        shown: |u| u.stick.mounted,
        width: |_| 46,
        chip: true,
        draw: |u, cv, r, label| u.draw_chip(cv, r, label, true),
        page: MenuId::Storage,
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    /// The LAN and WLAN boxes show no address by default, only whether there is one; System > Network > Address in
    /// top bar shows it, and the setting is kept with the system settings.
    #[test]
    fn the_top_bar_shows_an_address_only_when_switched_on() {
        let mut ui = crate::ui::testkit::demo_ui();
        assert_eq!(ui.link_line(None), "-");
        let dash = ui.net_box().w;
        ui.net.ifs = vec![("eth0".into(), "203.0.113.255".into())];
        ui.net.eth_link = true;
        let addr = ui.addr_of("eth0");
        assert!(!ui.sys.show_addr);
        assert_eq!(ui.link_line(addr), "", "no address and no word by default: the green dot says it");
        let hidden = ui.net_box().w;
        assert!(ui.network_items().contains(&Item::NetShowAddr));
        ui.select_option(Item::NetShowAddr, "On");
        assert!(ui.sys.show_addr && ui.sys.text().contains("net.show_addr = on"));
        let addr = ui.addr_of("eth0");
        assert_eq!(ui.link_line(addr), "203.0.113.255");
        assert_eq!(ui.link_line(None), "-");
        // the box is as wide as what it shows: narrow with the dash, wide for an address only while one is shown
        assert!(dash <= hidden && hidden < ui.net_box().w, "{} {} {}", dash, hidden, ui.net_box().w);
    }

    #[test]
    fn generator_and_la_light_their_front_panel_leds() {
        let mut ui = crate::ui::testkit::demo_ui();
        let off = ui.leds();
        assert_eq!(off & (1 << 8), 0, "LA off");
        assert_eq!(off & (1 << 9), 0, "GEN off");
        ui.m.afg_on = true;
        assert_ne!(ui.leds() & (1 << 9), 0, "G1 on while generator output is on");
        ui.m.afg_on = false;
        ui.m.la.on = true;
        assert_ne!(ui.leds() & (1 << 8), 0, "LA on");
        assert_eq!(ui.leds() & (1 << 9), 0, "G1 stays off without afg_on");
    }
}
