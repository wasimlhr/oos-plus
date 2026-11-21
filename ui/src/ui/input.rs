// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Input of the screen: panel keys and knobs (with acceleration and long presses), touch (single and two fingers, drags,
//! the zoom box, the mask zones) and the taps on the top bar, the plot and the cards. Every event comes in through
//! `Ui::handle`; a new key goes into `Ui::key`, a new touch target into `Ui::tap` or `Ui::touch_in`.

use super::*;

/// The power dialog as the screen keeps it (ADR 0062): the power key opens and closes it, a key or a tap closes it.
#[derive(Default)]
pub struct Power {
    /// the dialog is open; it is drawn over everything and takes the next tap
    pub open: bool,
    /// what the dialog asked for, taken by the main loop (main.rs)
    pub action: Option<PowerAction>,
}

/// What the input keeps between two events (ADR 0062).
pub(super) struct State {
    /// a channel key held on the front panel: channel, since when, and whether it was long enough for AC/DC already
    /// (`panel_key`)
    pub ch: Option<(usize, f64, bool)>,
    /// a finger on the card of a channel that is on, since when (a long touch: AC/DC, as the key)
    pub card: Option<(usize, f64)>,
    /// when each knob turned last, for its acceleration (`knob_accel`)
    pub knob_t: [f64; 16],
    /// screen path of the zone being drawn
    pub zone_path: Vec<(i32, i32)>,
    /// two fingers on the plot
    pub pinch: Pinch,
}

impl Default for State {
    fn default() -> State {
        State {
            ch: None,
            card: None,
            knob_t: [0.0; 16],
            zone_path: Vec::new(),
            pinch: Pinch {
                active: false,
                p0: (0, 0),
                p1: (0, 0),
                ref_dx: 0,
                ref_dy: 0,
                area: Rect::new(0, 0, 0, 0),
                detail: false,
            },
        }
    }
}

pub(super) struct Pinch {
    pub active: bool,
    p0: (i32, i32),
    p1: (i32, i32),
    ref_dx: i32,
    ref_dy: i32,
    area: Rect,
    detail: bool,
}

/// What a press hits, found before the touch state starts.
struct PressHits {
    in_plot: bool,
    zmap: Option<(Rect, f32, f32)>,
    grip: Option<(scroll::List, scroll::Grip)>,
    /// a divider of the event table's columns under the finger
    column: Option<usize>,
}

impl Ui {
    pub fn handle(&mut self, ev: Ev) {
        if self.start_up_input(&ev) {
            return;
        }
        match ev {
            Ev::Key(k) => self.panel_key(k),
            Ev::Touch { id, phase, x, y } => self.multi_touch(id, phase, x, y),
            Ev::Wheel { x, y, steps, ctrl, shift, alt } => {
                if self.menu_open {
                    self.knob_turn(key::MULTI1_CW, steps);
                } else if !ctrl && !shift && !alt && self.panel_wheel(x, y, steps) {
                } else if alt {
                    self.knob_turn(key::TLEVEL_CW, steps.clamp(-32, 32) * 5);
                } else if ctrl {
                    self.pointer_wheel(x, y, steps, true);
                } else if shift {
                    self.pointer_wheel_motion(x, y, steps, false, true);
                } else {
                    self.pointer_wheel(x, y, steps, false);
                }
            }
        }
    }

    /// `n` steps of the knob whose clockwise key is `cw` (`KNOB_NAMES`), counterclockwise for `n` < 0: the mouse wheel,
    /// and `knob <name> <n>` of a script or the web page (cmd.rs). At most 100 at a time, a turn and not a loop that
    /// stops the screen for minutes; 0.2 s apart, so a scripted turn gets no acceleration.
    pub fn knob_turn(&mut self, cw: u8, n: i32) {
        let n = n.clamp(-100, 100);
        for _ in 0..n.abs() {
            self.key(if n > 0 { cw } else { cw + 1 });
            self.now += 0.2;
        }
    }

    fn knob_accel(&mut self, k: u8) -> i32 {
        let i = ((k >> 4) & 15) as usize;
        let dt = self.now - self.input.knob_t[i];
        self.input.knob_t[i] = self.now;
        if dt < 0.04 {
            4
        } else if dt < 0.1 {
            2
        } else {
            1
        }
    }

    /// Force belongs to the current acquisition owner, independent of modal UI.
    pub(crate) fn force_trigger(&mut self) {
        if self.m.run == RunState::Stop && self.m.trig_mode == TrigMode::Single {
            self.scpi_arm_operation();
            self.single_armed = false;
        }
        if self.record_mode() {
            self.record_key(key::FORCE);
            return;
        }
        self.rec_action(":TFORce");
        self.bus_candidate_key(key::FORCE);
        self.m.run = RunState::Run;
        self.src.force_hw();
        self.toast("Force trigger");
    }

    /// A key of the front panel (the panel sends a key's code when it is pressed and 0 when it is let go): as `key`,
    /// but a channel key acts when it is let go, and held for `LONG_PRESS` it switches the channel between AC and DC
    /// coupling instead (at once, while still held). Scripts, the control FIFO and the web remote press keys through
    /// `key`, as before.
    pub fn panel_key(&mut self, k: u8) {
        let c = k.wrapping_sub(key::CH1) as usize;
        if c < 4 && c < n_ch() {
            // a second channel key while one is held: the held one counts as a short press
            self.ch_release();
            self.input.ch = Some((c, self.now, false));
            return;
        }
        if k == 0 || self.input.ch.is_some() {
            self.ch_release();
        }
        self.key(k);
    }

    /// The push of the horizontal SCALE knob: the fine time base on or off, and a line that says which, since the
    /// knob itself shows nothing. The time base stays where it is; the next turn goes on from there.
    fn push_fine_time(&mut self) {
        self.adjust(Item::HFine, 1);
        self.toast(if self.m.fine_t { "Time base: fine steps" } else { "Time base: 1-2-5 steps" });
    }

    /// A held channel key let go: its own action, unless it was held long enough for AC/DC.
    fn ch_release(&mut self) {
        if let Some((c, _, long)) = self.input.ch.take() {
            if !long {
                self.key(key::CH1 + c as u8);
            }
        }
    }

    /// A channel key held for `LONG_PRESS`: AC/DC once; letting it go then does nothing more. A finger held on a
    /// channel's card opens that channel's quick actions (never a silent AC/DC); the lift is no tap.
    pub(super) fn ch_hold_poll(&mut self) {
        if let Some((c, t)) = self.input.card {
            let still = self.touch.down
                && (self.touch.x - self.touch.x0).abs() < 16
                && (self.touch.y - self.touch.y0).abs() < 16;
            if !still {
                self.input.card = None;
            } else if self.now - t >= LONG_PRESS {
                self.input.card = None;
                let (x, y) = (self.touch.x0, self.touch.y0);
                self.touch.down = false;
                self.channel_popup(c, x, y);
                self.workspace.lift_pending = true;
            }
        }
        if let Some((c, t, false)) = self.input.ch {
            if self.now - t >= LONG_PRESS {
                self.input.ch = Some((c, t, true));
                self.toggle_ac_dc(c);
            }
        }
    }

    /// A long press of a channel key: AC or DC coupling.
    pub(crate) fn toggle_ac_dc(&mut self, c: usize) {
        let to = if self.m.ch[c].coupling == Coupling::Ac { Coupling::Dc } else { Coupling::Ac };
        if !board().couplings().contains(&to) {
            return self.toast(&format!("CH{}: no {} coupling on this scope", c + 1, to.name()));
        }
        self.m.ch[c].coupling = to;
        self.toast(&format!("CH{}: {} coupling", c + 1, to.name()));
        self.settings_changed();
    }

    /// What Default leaves alone, taken from the set-up `from` into `to`: the Quick key action and the text size are
    /// preferences, not part of the set-up that Default resets. The SCPI recorder asks too (scpi/recorder.rs): a
    /// reset takes what was set before it with everything else, but not these.
    pub(crate) fn default_keeps(from: &Model, to: &mut Model) {
        (to.quick, to.text_size) = (from.quick, from.text_size);
    }

    /// The Default key and `*RST`: a new set-up. `*RST` calls this itself, so that a notice or a menu on the screen,
    /// which takes a key press for its own, does not swallow a reset from a script.
    pub(crate) fn default_setup(&mut self) {
        self.rec_action("*RST");
        // the layout is part of the set-up: a new one starts in the Default layout with the stock bars (5533112)
        let mut m = Model { meas: Self::default_meas(), ..Model::default() };
        Self::default_keeps(&self.m, &mut m);
        // the hardware session's stamps go on: a new set-up is not a new acquisition thread (#391: SINGLE never ended
        // after *RST)
        m.keep_session(&self.m);
        self.m = m;
        // what hangs on the signals of the set-up that is gone: the notes, and the vertical zoom a closed zoom
        // window kept for its next opening
        self.callouts.clear();
        (self.zoom_view.vfactor, self.zoom_view.vcenter) = (1.0, 0.0);
        self.bus_auto = None;
        self.clear_measurement_statistics();
        self.select_workspace_source(WaveId::Ch(self.m.active));
        self.meas_own.rows.clear();
        self.meas_own.dialog = None;
        self.ensure_meas_own();
        self.results.on = false;
        self.xy = false;
        self.mask_sw.mask = None;
        self.mask_sw.reference = None;
        self.close_menu();
        self.dirty_map = true;
        self.toast("Default set-up");
    }

    pub fn key(&mut self, k: u8) {
        if k == 0 {
            return; // release
        }
        if k == key::FORCE {
            self.force_trigger();
            return;
        }
        if self.load_slot_ask {
            self.cancel_load_slot();
            return;
        }
        if (k == key::RUNSTOP && self.m.run == RunState::Run)
            || k == key::SINGLE
            || k == key::DEFAULT
            || (0x15..=0x18).contains(&k)
        {
            self.sim_cancel_generation();
        }
        if k == key::POWER {
            self.power_key();
            return;
        }
        if self.power.open {
            self.power.open = false;
            return;
        }
        if self.wlan.osk.is_some() {
            return self.osk_key(k);
        }
        if self.dialog.is_some() {
            self.dialog = None;
            return;
        }
        if self.record_key(k) {
            return;
        }
        self.bus_candidate_key(k);
        // for the SCPI recorder: Run / Stop and Single act below (on a record `record_key` has done them)
        match k {
            key::RUNSTOP => self.rec_action(if self.m.run == RunState::Run { ":STOP" } else { ":RUN" }),
            key::SINGLE => self.rec_action(":SINGle"),
            _ => {}
        }
        let m = &mut self.m;
        match k {
            0x15..=0x18 if (k - 0x15) as usize >= n_ch() => {
                let name = &oos_hal::model::platform().caps.name;
                self.toast(&format!("CH{}: the {} has {} channels", k - 0x14, name, n_ch()));
            }
            0x15..=0x18 => self.channel_key(k),
            key::RUNSTOP => {
                m.run = if m.run == RunState::Run { RunState::Stop } else { RunState::Run };
                if m.run == RunState::Run {
                    m.run_sweep();
                }
                if m.run == RunState::Run && (m.n_on() > 0 || m.la_on()) {
                    m.floor_live_tdiv();
                }
            }
            key::SINGLE => {
                self.ops.arm_single();
                m.set_sweep(TrigMode::Single);
                m.run = RunState::Run;
                self.single_armed = false;
                self.dirty_map = true;
                if m.n_on() > 0 || m.la_on() {
                    m.floor_live_tdiv();
                }
            }
            key::AUTO => {
                self.autoset_live_only();
            }
            key::CLEAR => self.clear_display(),
            key::DEFAULT => self.default_setup(),
            key::SLOPE => self.adjust(Item::TrigSlope, 1),
            key::FORCE => {
                self.force_trigger();
            }
            key::MEASURE => self.open_meas_list(),
            key::CURSOR => {
                if self.m.cursor == CursorMode::Off {
                    self.m.cursor = CursorMode::Time;
                }
                self.toggle_menu(MenuId::Cursor)
            }
            key::TMENU => self.toggle_menu(MenuId::Trigger),
            // the Horizontal Menu key: Acquire (with Horizontal as a sub menu); the TIME card opens Horizontal
            key::HMENU => self.toggle_menu(MenuId::Acquire),
            key::QUICK => self.quick(),
            key::TOUCHLOCK => {
                self.m.touch_lock = !self.m.touch_lock;
                let s = if self.m.touch_lock { "Touch locked" } else { "Touch on" };
                self.toast(s);
            }
            key::MATH => self.toggle_menu(MenuId::Math),
            key::REF => self.toggle_menu(MenuId::Ref),
            key::ANALYSE => self.toggle_menu(MenuId::Decode),
            // history: back / forward step through the stored records (stops the acquisition), play-stop = live
            0x08 => {
                self.history_step(-1);
            }
            0x06 => {
                self.history_step(1);
            }
            0x07 | key::NAVIGATE => {
                if self.history.pos.is_some() {
                    self.history.pos = None;
                    self.m.run = RunState::Run;
                    self.toast("Live");
                } else {
                    self.history_step(0);
                }
            }
            key::LA | key::GI => self.aux_key(k),
            0xf1..=0xf4 => self.toast(&format!("CH{} overload", k - 0xf0)),
            _ => self.knob(k),
        }
    }

    /// The power key: the MCU is told not to power down; the power dialog opens or closes.
    fn power_key(&mut self) {
        // the stock Launcher answers every power-key press with "cancel power-down" (FA 05 02); the MCU very
        // likely cuts power by itself otherwise (board.md section 4)
        self.panel_tx.push(crate::board::PanelCmd::CancelPowerDown);
        self.power.open = !self.power.open;
        if self.power.open {
            self.dialog = None;
            self.close_menu();
        }
    }

    /// A channel key: turns the channel on, selects it, opens or closes its menu.
    fn channel_key(&mut self, k: u8) {
        let m = &mut self.m;
        let c = (k - 0x15) as usize;
        if !m.ch[c].on {
            m.ch[c].on = true;
            m.active = c;
            if m.run == RunState::Run {
                m.floor_live_tdiv();
            }
            self.open_menu(MenuId::Channel(c));
        } else if m.n_on() > 1 && m.active != c {
            // several channels on: the first press only selects the channel, so the knobs act on it; the
            // next press opens its menu. Another channel's menu would show the wrong channel: it closes.
            m.active = c;
            if self.menu_open && matches!(self.menu, MenuId::Channel(_)) {
                self.close_menu();
            }
        } else if self.menu_open && self.menu == MenuId::Channel(c) {
            m.ch[c].on = false;
            self.menu_open = false;
            if let Some(o) = (0..NCH).find(|&i| self.m.ch[i].on) {
                self.m.active = o;
            }
        } else {
            m.active = c;
            self.open_menu(MenuId::Channel(c));
        }
        self.select_workspace_source(WaveId::Ch(if self.m.ch[c].on { c } else { self.m.active }));
        self.settings_changed();
    }

    /// The LA and generator keys: their page, or a note when the scope has no such port.
    fn aux_key(&mut self, k: u8) {
        let p = oos_hal::model::platform();
        let (name, has) =
            if k == key::LA { ("logic analyser port", p.caps.la) } else { ("generator", p.caps.has_gen()) };
        if !has {
            self.toast(&format!("The {} has no {}", p.caps.name, name));
        } else if k == key::GI {
            self.open_menu(MenuId::Afg);
        } else {
            self.la_key();
        }
    }

    fn knob(&mut self, k: u8) {
        let base = k & 0xf0;
        let sub = k & 0x0f;
        if !(0x8..=0xe).contains(&(base >> 4)) || !(0xa..=0xc).contains(&sub) {
            return;
        }
        let push = sub == 0xc;
        let dir = if sub == 0xa { 1 } else { -1 };
        let acc = if push { 1 } else { self.knob_accel(k) };
        if let Some(WaveId::Ref(slot)) = self.selected_workspace_source() {
            self.ref_page.sel = slot;
            match base {
                0xd0 => {
                    if push {
                        self.toggle_menu(MenuId::Ref);
                    } else {
                        self.adjust(Item::RefVpd, -dir);
                    }
                    return;
                }
                0x90 => {
                    if push {
                        if let Some(r) = self.refs[self.ref_page.sel].as_mut() {
                            r.offset = 0.0;
                        }
                        self.settings_changed();
                    } else {
                        self.adjust(Item::RefOffset, dir * acc);
                    }
                    return;
                }
                _ => {}
            }
        } else if self.record_mode() && matches!(base, 0xd0 | 0x90) {
            self.toast("Select a loaded reference for the vertical controls");
            return;
        }
        let c = self.m.active;
        match base {
            0xd0 => {
                if push {
                    self.toggle_menu(MenuId::Channel(c));
                } else {
                    // CW = more sensitive (smaller V/div)
                    self.adjust(Item::ChScale(c), -dir);
                }
            }
            0x90 => {
                if push {
                    self.m.ch[c].offset = 0.0;
                    self.settings_changed();
                } else {
                    self.adjust(Item::ChOffset(c), dir * acc);
                }
            }
            0xc0 => {
                if push {
                    // a push switches between the 1-2-5 steps and the fine time base, as the name of the knob's
                    // push says on the panel of other scopes. It opened the zoom: a time base left in fine steps
                    // (the row is in the Horizontal menu) then looked like a broken knob, with no way back from
                    // the panel (reported from a unit, 2026-11-18). The zoom has its button on the screen and its
                    // row; while it is open the knob is the zoom's, and a push closes it.
                    if self.m.zoom { self.set_zoom(false); } else { self.push_fine_time(); }
                } else if self.m.zoom {
                    self.adjust(Item::ZScale, -dir);
                } else {
                    self.adjust(Item::HScale, -dir);
                }
            }
            0x80 => {
                if push {
                    if self.m.zoom {
                        self.m.zdelay = self.m.delay;
                    } else {
                        self.m.delay = 0.0;
                    }
                    self.settings_changed();
                } else if self.m.zoom {
                    self.adjust(Item::ZDelay, -dir * acc);
                } else {
                    self.adjust(Item::HDelay, -dir * acc);
                }
            }
            0xe0 => {
                if push {
                    self.level_50();
                } else {
                    self.adjust(Item::TrigLevel, dir * acc);
                }
            }
            0xb0 | 0xa0 => {
                // Cursors on the screen: the two knobs are theirs, the first moves cursor A and the second cursor B,
                // with no menu open or with the Cursor menu, which the Cursor key and button open. A turn opened
                // the Menu, and in the Cursor menu it went through its rows, so the knobs moved no cursor at all
                // (reported from a unit, 2026-11-18). Other menus keep the knobs for their rows.
                if self.m.cursor != CursorMode::Off && (!self.menu_open || self.menu_root() == MenuId::Cursor) {
                    return self.cursor_knob(base == 0xb0, push, dir * acc);
                }
                let items = self.items(self.menu);
                if !self.menu_open {
                    self.open_menu(MenuId::Main);
                    return;
                }
                if items.is_empty() {
                    return;
                }
                self.focus = self.focus.min(items.len() - 1);
                let it = items[self.focus];
                let picker = self
                    .dlg_layout()
                    .and_then(|d| d.rows.get(self.focus).map(|r| matches!(r.ctl, Ctl::Pick)))
                    .unwrap_or(false);
                if push {
                    if picker {
                        // push on a choice with a long list: the list
                        self.open_picker(it);
                    } else if Self::kind(it) == Kind::Value {
                        // push on a value: next item
                        self.focus = (self.focus + 1) % items.len();
                    } else {
                        self.activate(it);
                    }
                } else if base == 0xb0 {
                    // multi 1 selects the item
                    self.focus = (self.focus as i32 + dir).rem_euclid(items.len() as i32) as usize;
                    self.pick_follow_focus();
                } else {
                    // multi 2 changes the value
                    if Self::kind(it) != Kind::Action {
                        self.adjust(it, dir * acc);
                    }
                }
            }
            _ => {}
        }
    }

    /// A multipurpose knob while the cursors are on: `first` is the knob of cursor A, the other the one of B. With
    /// the time and the level pair on (XY) a push of either knob gives the knobs to the other pair; in the other
    /// modes a push opens or closes the Cursor menu. Which knob the panel calls A is assumed, not verified.
    fn cursor_knob(&mut self, first: bool, push: bool, d: i32) {
        let xy = self.m.cursor == CursorMode::Xy;
        if push {
            if xy {
                self.cur_knob_level = !self.cur_knob_level;
                self.toast(if self.cur_knob_level { "Knobs: the level cursors" } else { "Knobs: the time cursors" });
            } else {
                self.toggle_menu(MenuId::Cursor);
            }
            return;
        }
        let it = match (xy && self.cur_knob_level, first) {
            (false, true) => Item::CurA,
            (false, false) => Item::CurB,
            (true, true) => Item::CurC,
            (true, false) => Item::CurD,
        };
        self.adjust(it, d);
    }

    /// Two fingers in the grid: horizontal spread = time/div, vertical spread = V/div of the active channel.
    /// A second finger makes the touch a pinch or a two-finger swipe: it is never a long press. Every hold that is
    /// counting stops, and a context menu the first finger's hold had already opened goes away again.
    fn second_finger_down(&mut self) {
        if self.workspace_cancel_hold() {
            self.studio.ctx = None;
            if self.menu_open && self.menu == MenuId::MeasHold { self.close_menu(); }
        }
        self.studio.md.hold = None;
        self.studio.sz.press = None;
        self.studio.dock.press = None;
        // the lift of the first finger is not a tap either
        if self.touch.down { self.touch.held = true; }
    }

    fn multi_touch(&mut self, id: i32, ph: Phase, x: i32, y: i32) {
        if id == 1 && ph == Phase::Down { self.second_finger_down(); }
        if self.load_slot_ask {
            if id == 0 && ph == Phase::Up {
                self.load_slot_tap(x, y);
            }
            return;
        }
        if self.eye_open() && self.eye_pane_rect().contains(x, y) {
            if id == 0 {
                self.touch_ev(ph, x, y);
            }
            return;
        }
        if self.m.touch_lock {
            if id == 0 {
                self.touch_ev(ph, x, y);
            }
            return;
        }
        // the second finger on an axis strip: a pinch along that axis
        if id == 1 && self.m.layout == Layout::Studio && self.studio.axis.touch.is_some() {
            if ph == Phase::Down {
                self.axis_pinch_down(x, y);
            } else {
                self.axis_pinch_event(ph, x, y);
            }
            return;
        }
        let map = self.zoom_drag_map(self.touch.x, self.touch.y);
        let modal =
            self.load_slot_ask || self.menu_open || self.dialog.is_some() || self.power.open || self.wlan.osk.is_some();
        let detail_y = self.split_zoom(self.plot.grid).1.y;
        let p = &mut self.input.pinch;
        if id == 1 {
            match ph {
                Phase::Down => {
                    let on_plot = map.filter(|(area, _, _)| area.contains(x, y));
                    if let Some((area, _, _)) = on_plot.filter(|_| !modal && self.touch.down) {
                        p.active = true;
                        p.area = area;
                        p.detail = self.m.zoom && p.area.y >= detail_y;
                        p.p0 = (self.touch.x, self.touch.y);
                        p.p1 = (x, y);
                        p.ref_dx = (p.p1.0 - p.p0.0).abs();
                        p.ref_dy = (p.p1.1 - p.p0.1).abs();
                        // cancel the one-finger drag
                        self.touch.drag = Drag::None;
                        self.zoom_box.path.clear();
                        self.zoom_box.map = None;
                    }
                }
                Phase::Move => p.p1 = (x, y),
                Phase::Up => {
                    p.active = false;
                    self.touch.drag = Drag::None;
                    self.touch.down = false; // no tap when the first finger lifts
                }
            }
        } else if p.active {
            match ph {
                Phase::Move => p.p0 = (x, y),
                Phase::Up => {
                    p.active = false;
                    self.touch.down = false;
                }
                Phase::Down => {}
            }
        } else {
            self.touch_ev(ph, x, y);
            return;
        }
        let p = &mut self.input.pinch;
        if !p.active {
            return;
        }
        let dx = (p.p1.0 - p.p0.0).abs();
        let dy = (p.p1.1 - p.p0.1).abs();
        let mut dt = 0;
        let mut dv = 0;
        if p.ref_dx > 40 && dx * 100 > p.ref_dx * 135 {
            dt = -1;
            p.ref_dx = dx;
        } else if p.ref_dx > 40 && dx * 100 < p.ref_dx * 74 {
            dt = 1;
            p.ref_dx = dx;
        }
        if p.ref_dy > 40 && dy * 100 > p.ref_dy * 135 {
            dv = -1;
            p.ref_dy = dy;
        } else if p.ref_dy > 40 && dy * 100 < p.ref_dy * 74 {
            dv = 1;
            p.ref_dy = dy;
        }
        let area = p.area;
        let detail = p.detail;
        let midpoint = ((p.p0.0 + p.p1.0) as f32 * 0.5, (p.p0.1 + p.p1.1) as f32 * 0.5);
        let fx = ((midpoint.0 - area.x as f32) / area.w.max(1) as f32).clamp(0.0, 1.0);
        let fy = ((midpoint.1 - area.y as f32) / area.h.max(1) as f32).clamp(0.0, 1.0);
        let studio_zoom = self.m.layout == Layout::Studio && self.m.zoom;
        if dt != 0 {
            if studio_zoom {
                // the overview: the width of the zoom box; the zoomed waveform: nothing
                if !detail {
                    self.overview_box_step(-dt, midpoint.0 as i32);
                }
            } else if detail {
                self.zoom_horizontal_step(dt, area, fx);
            } else {
                self.main_horizontal_step(dt, fx);
            }
        }
        if dv != 0 {
            // Studio's zoomed waveform: nothing
            if detail {
                if !studio_zoom {
                    self.zoom_vertical_step(-dv, area, fy);
                }
            } else if !self.m.zoom {
                match self.selected_workspace_source() {
                    Some(WaveId::Ref(i)) => {
                        self.ref_page.sel = i;
                        self.adjust(Item::RefVpd, dv);
                    }
                    _ => self.adjust(Item::ChScale(self.m.active), dv),
                }
            }
        }
    }

    /// A finger in a split plot works in the window it went down in: its channels, its rectangle.
    pub(super) fn touch_ev(&mut self, ph: Phase, x: i32, y: i32) {
        if self.studio_touch(ph, x, y) {
            return;
        }
        if self.workspace_touch(ph, x, y) {
            return;
        }
        if self.plot.wins.len() < 2 {
            return self.touch_in(ph, x, y);
        }
        let keep = (self.plot.area, self.plot.cur_win);
        let w = if ph == Phase::Down {
            self.plot.wins.iter().position(|r| r.contains(x, y)).unwrap_or(0)
        } else {
            self.touch.win.min(self.plot.wins.len() - 1)
        };
        (self.plot.area, self.plot.cur_win) = (self.plot.wins[w], w);
        self.touch_in(ph, x, y);
        (self.plot.area, self.plot.cur_win) = keep;
    }

    /// One finger's press, move or lift in the area of the current window.
    fn touch_in(&mut self, ph: Phase, x: i32, y: i32) {
        let covered = self.menu_rect().is_some_and(|d| d.contains(x, y));
        // the corner buttons are taps that stay in place: a press on one is no drag of the trace or of the button
        let on_btn = !covered
            && (self.full_btn().1.contains(x, y)
                || (self.plot_btns_shown() && (self.zoom_btn().1.contains(x, y) || self.win_btn().1.contains(x, y))));
        let in_plot = ph == Phase::Down
            && self.plot.grid.contains(x, y)
            && !covered
            && !on_btn
            && !(self.eye_open() && self.eye_pane_rect().contains(x, y));
        // a zoom box starts on the waveform, not on the zoom bar or the view button
        let on_chrome = ph == Phase::Down
            && ((self.zoom_btn().1.contains(x, y) && !self.view_on())
                || (self.m.zoom && self.zoom_readout().contains(x, y))
                || (self.view_on() && self.view_btn().contains(x, y)));
        let zmap = if self.zoom_box.arm && in_plot && !on_chrome { self.zoom_drag_map(x, y) } else { None };
        // a finger on a list that scrolls (the measurement page, the Results pane, a table) moves it
        let free = ph == Phase::Down
            && !self.m.touch_lock
            && self.dialog.is_none()
            && !self.power.open
            && self.wlan.osk.is_none();
        let column = if free { self.table_col_grip(x, y) } else { None };
        let grip = if free && column.is_none() { self.scroll_grip(x, y) } else { None };
        match ph {
            Phase::Down => self.touch_down(x, y, PressHits { in_plot, zmap, grip, column }),
            Phase::Move => self.touch_move(x, y),
            _ => self.touch_up(x, y),
        }
    }

    /// A press: the touch state starts, and what the finger is on picks the drag it may become.
    fn touch_down(&mut self, x: i32, y: i32, hits: PressHits) {
        let PressHits { in_plot, zmap, grip, column } = hits;
        let t = &mut self.touch;
        let sd = if self.m.zoom { self.m.zdelay } else { self.m.delay };
        *t = TouchState {
            down: true,
            x0: x,
            y0: y,
            x,
            y,
            t0: self.now,
            held: false,
            drag: Drag::None,
            start_offset: 0.0,
            start_delay: sd,
            start_level: self.m.trig_level,
            start_cur: 0.0,
            win: self.plot.cur_win,
        };
        if self.m.touch_lock || self.dialog.is_some() || self.power.open || self.wlan.osk.is_some() {
            return;
        }
        if let Some(k) = column {
            t.drag = Drag::TableCol(k);
            return;
        }
        if let Some((l, g)) = grip {
            t.drag = Drag::Scroll(l, g);
            return;
        }
        if !in_plot {
            // a finger held on the card of a channel that is on: AC/DC, as the channel key
            let (chs, _, _) = self.cards();
            let y = self.bar_y(y);
            self.input.card =
                chs.iter().position(|r| r.contains(x, y)).filter(|&c| self.m.ch[c].on).map(|c| (c, self.now));
        } else {
            // an edge strip of the plot swipes between the views (not while a zoom box is armed)
            let edge = zmap.is_none() && self.view_edge(x);
            let t = &mut self.touch;
            t.drag = if edge { Drag::ViewEdge } else { Drag::Pending };
            if let Some(map) = zmap {
                t.drag = Drag::ZoomBox;
                self.zoom_box.path = vec![(x, y), (x, y)];
                self.zoom_box.map = Some(map);
            } else if let Some((k, _)) = self.zone_trig.arm {
                t.drag = Drag::Zone(k);
                self.input.zone_path = vec![(x, y)];
            }
        }
    }

    /// A move: a scroll or a view swipe goes on, a pending press picks its drag, and the drag follows the finger.
    fn touch_move(&mut self, x: i32, y: i32) {
        let t = &mut self.touch;
        if !t.down {
            return;
        }
        t.x = x;
        t.y = y;
        if let Drag::TableCol(k) = t.drag {
            self.table_col_set(k, x);
            return;
        }
        if let Drag::Scroll(l, g) = t.drag {
            let y0 = t.y0;
            self.scroll_move(l, g, y0, y);
            return;
        }
        if t.drag == Drag::ViewEdge {
            let (x0, dx, dy) = (t.x0, x - t.x0, y - t.y0);
            match self.view_swipe(x0, dx, dy) {
                Some(true) => self.touch.drag = Drag::None,
                Some(false) => return,
                None => self.touch.drag = Drag::Pending,
            }
        }
        let t = &mut self.touch;
        let (tx0, ty0, tdrag) = (t.x0, t.y0, t.drag);
        let (dx, dy) = (x - tx0, y - ty0);
        if tdrag == Drag::Pending && (dx.abs() > 10 || dy.abs() > 10) {
            // choose what to drag
            let drag = self.pick_drag(tx0, ty0, dx, dy);
            self.touch_start_drag(drag);
        }
        self.touch_drag_to(x, y);
    }

    /// What a press at (`tx0`, `ty0`) drags once it moved by (`dx`, `dy`): zoom edges, cursors, levels, offsets or delay.
    fn pick_drag(&self, tx0: i32, ty0: i32, dx: i32, dy: i32) -> Drag {
        let near_right = tx0 > self.plot.area.x + self.plot.area.w - 50;
        let near_left = tx0 < self.plot.area.x + 50;
        let cur_hit = |frac: f32, vertical: bool| -> bool {
            if vertical {
                (self.plot.area.x + (frac * self.plot.area.w as f32) as i32 - tx0).abs() < 24
            } else {
                (self.plot.area.y + (frac * self.plot.area.h as f32) as i32 - ty0).abs() < 24
            }
        };
        let m = &self.m;
        // the cursors can be grabbed where they are shown: in the lane or window of their source; a time cursor
        // also in any other lane, where its band edge stands at the same time
        let here = self.cursor_visible_here();
        let cur_hit =
            |frac: f32, vertical: bool| (here || (vertical && self.plot.wins.len() > 1)) && cur_hit(frac, vertical);
        let under = if dy.abs() > dx.abs() && !near_left && !m.zoom { self.trace_under(tx0, ty0) } else { None };
        let overview_drag = if m.zoom {
            let overview = self.split_zoom(self.plot.grid).0;
            if overview.contains(tx0, ty0) {
                let ppd = overview.w as f32 / HDIV;
                let half = tscales()[m.zscale] * HDIV * 0.5;
                let edge = |time| overview.x + overview.w / 2 + ((time - m.delay) / m.time_per_div() * ppd) as i32;
                let left = (tx0 - edge(m.zdelay - half)).abs();
                let right = (tx0 - edge(m.zdelay + half)).abs();
                Some(if left < 20 && left <= right {
                    Drag::ZoomLeft
                } else if right < 20 {
                    Drag::ZoomRight
                } else if m.layout == Layout::Studio && {
                    let (xa, xb) = self.zoom_window_x(overview);
                    tx0 < xa || tx0 > xb
                } {
                    Drag::ZoomNew
                } else {
                    Drag::Overview
                })
            } else {
                None
            }
        } else {
            None
        };
        if let Some(drag) = overview_drag {
            drag
        } else if m.cursor == CursorMode::Xy && dy.abs() >= dx.abs() && cur_hit(m.cur_c, false) {
            Drag::CursorC
        } else if m.cursor == CursorMode::Xy && dy.abs() >= dx.abs() && cur_hit(m.cur_d, false) {
            Drag::CursorD
        } else if matches!(m.cursor, CursorMode::Time | CursorMode::Track | CursorMode::Xy) && cur_hit(m.cur_a, true) {
            Drag::CursorA
        } else if matches!(m.cursor, CursorMode::Time | CursorMode::Track | CursorMode::Xy) && cur_hit(m.cur_b, true) {
            Drag::CursorB
        } else if m.cursor == CursorMode::Volt && cur_hit(m.cur_a, false) {
            Drag::CursorA
        } else if m.cursor == CursorMode::Volt && cur_hit(m.cur_b, false) {
            Drag::CursorB
        } else if near_right
            && dy.abs() >= dx.abs()
            && self.trigger_absolute_level_visible(false)
            && (m.trig_ref.is_some() || self.shows(m.trig_src))
        {
            let secondary = matches!(m.trig_type, TrigType::Delay | TrigType::SetupHold);
            let upper = self.trigger_level_y(m.trig_level, false).unwrap_or(ty0);
            let lower = self.trigger_level_y(m.trig_level2, secondary).unwrap_or(ty0);
            if matches!(
                m.trig_type,
                TrigType::Runt | TrigType::Slope | TrigType::Window | TrigType::Delay | TrigType::SetupHold
            ) && (ty0 - lower).abs() < (ty0 - upper).abs()
            {
                Drag::Level2
            } else {
                Drag::Level
            }
        } else if m.zoom && dy.abs() > dx.abs() {
            Drag::ZoomVertical
        } else if under.is_some() && !(m.bus.on && (self.bus_y() - ty0).abs() <= 18) {
            // the trace under the finger moves, not the active channel
            match under {
                Some(WaveId::Ref(i)) => Drag::RefOffset(i),
                Some(WaveId::Ch(c)) => Drag::Offset(c),
                _ => Drag::Delay,
            }
        } else if let Some(lane) = self.math_lane_at(tx0, ty0).filter(|_| dy.abs() > dx.abs()) {
            // a lane of a math channel: it moves like a channel's does
            Drag::MathOffset(lane)
        } else if let Some(r) = self.drag_reference().filter(|_| dy.abs() > dx.abs()) {
            Drag::RefOffset(r)
        } else if dy.abs() > dx.abs() {
            // nearest enabled channel marker when starting at the left edge, else the active channel (in a
            // split plot: of this window)
            let mut c = if self.shows(m.active) {
                m.active
            } else {
                (0..NCH).find(|&i| m.ch[i].on && self.shows(i)).unwrap_or(m.active)
            };
            let mut best = i32::MAX;
            if near_left {
                for i in 0..NCH {
                    if m.ch[i].on && self.shows(i) {
                        let d = (self.y_of_volts(i, 0.0) - ty0).abs();
                        if d < best {
                            best = d;
                            c = i;
                        }
                    }
                }
            }
            let bus_d = (self.bus_y() - ty0).abs();
            if m.bus.on && bus_d <= 18 && bus_d < best {
                Drag::Bus
            } else {
                Drag::Offset(c)
            }
        } else {
            Drag::Delay
        }
    }

    /// The start values of a drag just picked, and the source it selects.
    fn touch_start_drag(&mut self, drag: Drag) {
        let start_offset = match drag {
            Drag::Offset(c) => self.m.ch[c].offset,
            Drag::RefOffset(i) => self.refs[i].as_ref().map_or(0.0, |reference| reference.offset),
            Drag::MathOffset(k) => self.m.math_ch(k).offset,
            Drag::ZoomLeft | Drag::ZoomRight => tscales()[self.m.zscale],
            Drag::ZoomVertical => self.zoom_view.vcenter,
            Drag::Bus => self.m.bus.pos,
            _ => 0.0,
        };
        let start_cur = match drag {
            Drag::ZoomNew => {
                let o = self.split_zoom(self.plot.grid).0;
                self.m.delay + (self.touch.x0 - o.x - o.w / 2) as f32 / (o.w as f32 / HDIV) * self.m.time_per_div()
            }
            Drag::ZoomLeft => self.m.zdelay + tscales()[self.m.zscale] * HDIV * 0.5,
            Drag::ZoomRight => self.m.zdelay - tscales()[self.m.zscale] * HDIV * 0.5,
            Drag::CursorA => self.m.cur_a,
            Drag::CursorB => self.m.cur_b,
            Drag::CursorC => self.m.cur_c,
            Drag::CursorD => self.m.cur_d,
            _ => 0.0,
        };
        match drag {
            Drag::ZoomVertical => {
                if let Some(i) = self.drag_reference() {
                    self.select_workspace_source(WaveId::Ref(i));
                } else if let Some(c) = (0..NCH).find(|&c| self.m.ch[c].on && self.shows(c)) {
                    self.select_workspace_source(WaveId::Ch(c));
                }
            }
            Drag::Offset(c) => self.select_workspace_source(WaveId::Ch(c)),
            Drag::RefOffset(i) => self.select_workspace_source(WaveId::Ref(i)),
            _ => {}
        }
        let start_level = if drag == Drag::Level2 { self.m.trig_level2 } else { self.m.trig_level };
        let t = &mut self.touch;
        t.start_level = start_level;
        t.drag = drag;
        t.start_offset = start_offset;
        t.start_cur = start_cur;
    }

    /// The drag under way follows the finger at (`x`, `y`).
    fn touch_drag_to(&mut self, x: i32, y: i32) {
        let t = &self.touch;
        let (dx, dy) = (x - t.x0, y - t.y0);
        let ppx = self.plot.area.w as f32 / HDIV;
        let ppy = self.plot.area.h as f32 / VDIV;
        match t.drag {
            Drag::ZoomVertical => {
                self.zoom_view.vcenter = t.start_offset + dy as f32 / ppy / self.zoom_view.vfactor;
                self.dirty_map = true;
            }
            Drag::RefOffset(i) => {
                if let Some(reference) = &mut self.refs[i] {
                    reference.offset = t.start_offset - dy as f32 / ppy * reference.vpd;
                }
            }
            Drag::MathOffset(k) => {
                // a math channel's position is in divisions
                let mut c = self.m.math_ch(k);
                c.offset = t.start_offset - dy as f32 / ppy;
                if k == self.m.math_sel { self.m.math_offset = c.offset; }
                self.m.maths[k] = c;
                self.settings_changed();
            }
            Drag::Overview => {
                let overview = self.split_zoom(self.plot.grid).0;
                self.m.zdelay = t.start_delay + dx as f32 / (overview.w as f32 / HDIV) * self.m.time_per_div();
                self.clamp_zoom();
                self.settings_changed();
            }
            Drag::ZoomNew => {
                let overview = self.split_zoom(self.plot.grid).0;
                let at = x.clamp(overview.x, overview.right()) - overview.x - overview.w / 2;
                let time = self.m.delay + at as f32 / (overview.w as f32 / HDIV) * self.m.time_per_div();
                let (t0, t1) = (t.start_cur.min(time), t.start_cur.max(time));
                if t1 - t0 < 4.0 * self.m.time_per_div() * HDIV / overview.w.max(1) as f32 { return; }
                // the finest step whose window holds the box, and not slower than the main time base
                let scales = tscales();
                let mut z = 0;
                while z < self.m.tscale && scales[z] * HDIV < (t1 - t0) * 0.999 { z += 1; }
                self.m.zscale = z;
                self.m.zdelay = (t0 + t1) * 0.5;
                self.clamp_zoom(); self.settings_changed();
            }
            Drag::ZoomLeft | Drag::ZoomRight => {
                let overview = self.split_zoom(self.plot.grid).0;
                let time = self.m.delay
                    + (x - overview.x - overview.w / 2) as f32 / (overview.w as f32 / HDIV) * self.m.time_per_div();
                let span = if t.drag == Drag::ZoomLeft { t.start_cur - time } else { time - t.start_cur };
                let wanted = (span / HDIV).max(tscales()[0]);
                self.m.zscale = tscales()
                    .iter()
                    .enumerate()
                    .min_by(|a, b| (a.1 - wanted).abs().total_cmp(&(b.1 - wanted).abs()))
                    .map_or(0, |(i, _)| i)
                    .min(self.m.tscale);
                let half = tscales()[self.m.zscale] * HDIV * 0.5;
                self.m.zdelay = if t.drag == Drag::ZoomLeft { t.start_cur - half } else { t.start_cur + half };
                self.clamp_zoom();
                self.settings_changed();
            }
            Drag::Offset(c) => {
                let vpd = self.m.ch[c].volts_per_div();
                self.m.ch[c].offset = t.start_offset - dy as f32 / ppy * vpd;
                self.settings_changed();
            }
            Drag::Delay => {
                if self.m.zoom {
                    let zt = tscales()[self.m.zscale];
                    self.m.zdelay = t.start_delay - dx as f32 / ppx * zt;
                    self.clamp_zoom();
                } else {
                    self.m.delay = t.start_delay - dx as f32 / ppx * self.m.time_per_div();
                }
                self.settings_changed();
            }
            Drag::Level | Drag::Level2 => {
                let lower = t.drag == Drag::Level2;
                let secondary = lower && matches!(self.m.trig_type, TrigType::Delay | TrigType::SetupHold);
                let id = if secondary { self.m.trigger_wave2() } else { self.m.trigger_wave() };
                let ppy = self
                    .source_area(id)
                    .map_or(ppy, |area| area.h as f32 / VDIV * self.zoom_vertical_transform(area).0);
                let level = t.start_level - dy as f32 / ppy * self.trigger_vpd(secondary);
                // a two-level type: the dragged line stops at the other one, as the knob does
                let two = self.m.trig_type.two_level();
                if lower {
                    self.m.trig_level2 = if two { level.min(self.m.trig_level) } else { level };
                } else {
                    self.m.trig_level = if two { level.max(self.m.trig_level2) } else { level };
                }
                self.settings_changed();
            }
            Drag::Bus => {
                self.m.bus.pos = (t.start_offset - dy as f32 / ppy).clamp(BUS_POS_MIN, BUS_POS_MAX);
                self.settings_changed();
            }
            Drag::Zone(_) => {
                let shape = self.zone_trig.arm.is_some_and(|a| a.1);
                let last = self.input.zone_path.last().copied().unwrap_or((x, y));
                if !shape {
                    self.input.zone_path.truncate(1);
                    self.input.zone_path.push((x, y));
                } else if (x - last.0).abs() + (y - last.1).abs() >= 6 {
                    self.input.zone_path.push((x, y));
                }
            }
            Drag::ZoomBox => {
                if let Some(p) = self.zoom_box.path.last_mut() {
                    *p = (x, y);
                }
            }
            Drag::CursorA | Drag::CursorB | Drag::CursorC | Drag::CursorD => {
                let v = if self.m.cursor == CursorMode::Volt || matches!(t.drag, Drag::CursorC | Drag::CursorD) {
                    t.start_cur + dy as f32 / self.plot.area.h as f32
                } else {
                    t.start_cur + dx as f32 / self.plot.area.w as f32
                }
                .clamp(0.0, 1.0);
                match t.drag {
                    Drag::CursorA => {
                        let moved = v - self.m.cur_a;
                        self.m.cur_a = v;
                        if self.studio.cur_linked && self.m.layout == Layout::Studio { self.m.cur_b = (self.m.cur_b + moved).clamp(0.0, 1.0); }
                    }
                    Drag::CursorB => {
                        let moved = v - self.m.cur_b;
                        self.m.cur_b = v;
                        if self.studio.cur_linked && self.m.layout == Layout::Studio { self.m.cur_a = (self.m.cur_a + moved).clamp(0.0, 1.0); }
                    }
                    Drag::CursorC => self.m.cur_c = v,
                    Drag::CursorD => self.m.cur_d = v,
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// A lift: it ends the drag, finishes a zone or a zoom box, or is a tap.
    fn touch_up(&mut self, x: i32, y: i32) {
        let t = &mut self.touch;
        let was = (t.down, t.drag, t.x0, t.y0, t.held);
        t.down = false;
        t.drag = Drag::None;
        t.held = false;
        if !was.0 {
            return;
        }
        // the short menu is already open; this lift runs its shortcut instead of a tap
        if was.4 {
            self.meas_hold_up(x, y);
            return;
        }
        if let Drag::Zone(k) = was.1 {
            self.finish_zone(k);
            return;
        }
        if was.1 == Drag::ZoomBox {
            self.finish_zoom_box();
            return;
        }
        if self.dialog.is_some() {
            self.dialog = None;
            return;
        }
        if self.m.touch_lock {
            // the Menu button ("Locked") is the one touch target left: it unlocks
            if self.menu_btn().contains(was.2, was.3) {
                self.key(key::TOUCHLOCK);
            }
            return;
        }
        // a press on a list that did not move it is a tap on what is under the finger
        if let Drag::Scroll(_, g) = was.1 {
            if matches!(g, scroll::Grip::Content { .. }) && (x - was.2).abs() < 16 && (y - was.3).abs() < scroll::SLOP {
                self.tap(was.2, was.3);
            }
            return;
        }
        if (was.1 == Drag::None || was.1 == Drag::Pending || was.1 == Drag::ViewEdge)
            && (x - was.2).abs() < 16
            && (y - was.3).abs() < 16
        {
            self.tap(was.2, was.3);
        }
    }

    /// A box magnifies both axes without changing source scales or acquisition.
    fn finish_zoom_box(&mut self) {
        let path = std::mem::take(&mut self.zoom_box.path);
        let map = self.zoom_box.map.take();
        if path.len() != 2 {
            return;
        }
        let ((x0, y0), (x1, y1)) = (path[0], path[1]);
        // A plain tap ends box drawing; valid boxes can be drawn repeatedly.
        if (x1 - x0).abs() < 12 || (y1 - y0).abs() < 12 {
            if (x1 - x0).abs() < 12 && (y1 - y0).abs() < 12 {
                self.zoom_box.arm = false;
            }
            return;
        }
        let Some((a, tdiv, delay)) = map else { return };
        if a.w < 8 || tdiv <= 0.0 {
            return;
        }
        let ppd = a.w as f32 / HDIV;
        let at = |x: i32| {
            let x = x.clamp(a.x, a.x + a.w - 1);
            delay + (x - (a.x + a.w / 2)) as f32 / ppd * tdiv
        };
        let (t0, t1) = (at(x0.min(x1)), at(x0.max(x1)));
        let span = t1 - t0;
        if span <= 0.0 {
            return;
        }
        // finest step whose window still holds the box, and not slower than the main time base
        let scales = tscales();
        let mut z = 0;
        while z < self.m.tscale && scales[z] * HDIV < span * 0.999 {
            z += 1;
        }
        let on = self.m.zoom;
        let (factor, center) = self.zoom_vertical_transform(a);
        let low = y0.min(y1).clamp(a.y, a.y + a.h);
        let high = y0.max(y1).clamp(a.y, a.y + a.h);
        let midpoint = (low + high) as f32 * 0.5;
        let next_center = center + (0.5 - (midpoint - a.y as f32) / a.h as f32) * VDIV / factor;
        let next_factor = (factor * a.h as f32 / (high - low).max(1) as f32).clamp(1.0, 1024.0);
        if !on {
            self.zoom_view.saved = Some((self.m.tscale, self.m.tfine, self.m.delay));
        }
        self.m.zscale = z;
        self.m.zdelay = (t0 + t1) * 0.5;
        self.m.zoom = true;
        self.zoom_view.vfactor = next_factor;
        self.zoom_view.vcenter = next_center;
        self.clamp_zoom();
        self.zoom_box.arm = true;
        self.dirty_map = true;
        if !on {
            self.relayout();
        }
    }

    /// Turns the drawn screen path into zone `k` (waveform coordinates of its source channel).
    fn finish_zone(&mut self, k: usize) {
        let shape = self.zone_trig.arm.is_some_and(|a| a.1);
        self.zone_trig.arm = None;
        let path = std::mem::take(&mut self.input.zone_path);
        let a = self.plot.area;
        let (tdiv, delay) = self.view();
        let ch = &self.m.ch[self.m.zones[k].src];
        let ppx = a.w as f64 / HDIV as f64;
        let to_wave = |(x, y): (i32, i32)| {
            let t = (x - (a.x + a.w / 2)) as f64 / ppx * tdiv as f64 + delay as f64;
            let v = self.zoom_divs_at(a, (y - a.y) as f32 / a.h as f32) as f64 * ch.volts_per_div() as f64
                - ch.offset as f64;
            (t, v)
        };
        let ok = if shape {
            path.len() >= 3
        } else {
            path.len() == 2 && (path[0].0 - path[1].0).abs() > 4 && (path[0].1 - path[1].1).abs() > 4
        };
        if !ok {
            self.toast("Zone too small");
            return;
        }
        let points = path.into_iter().map(to_wave).collect();
        let z = &mut self.m.zones[k];
        z.pts = points;
        z.on = true;
        self.toast(&format!("Zone {} set", k + 1));
    }

    /// Zones (green = must intersect, red = must not) and the zone being drawn.
    pub(super) fn draw_zones(&self, cv: &mut Canvas) {
        let a = self.plot.area;
        let (tdiv, delay) = self.view();
        let ppx = a.w as f64 / HDIV as f64;
        cv.set_clip(a);
        for (k, z) in self.m.zones.iter().enumerate() {
            if z.pts.len() < 2 || !self.shows(z.src) {
                continue;
            }
            let ch = &self.m.ch[z.src];
            let px = |(t, v): (f64, f64)| {
                let x = a.x as f64 + a.w as f64 / 2.0 + (t - delay as f64) / tdiv as f64 * ppx;
                let y = self.zoom_y_of_divs(a, ((v + ch.offset as f64) / ch.volts_per_div() as f64) as f32) as f64;
                (x.clamp(-1e4, 1e4).round() as i32, y.clamp(-1e4, 1e4).round() as i32)
            };
            let poly: Vec<(i32, i32)> = z.polygon().into_iter().map(px).collect();
            let col = if !z.on {
                DIM
            } else if z.inside {
                GREEN
            } else {
                RED
            };
            for i in 0..poly.len() {
                let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
                cv.line(p.0, p.1, q.0, q.1, col);
                cv.line(p.0 + 1, p.1, q.0 + 1, q.1, col);
            }
            let (lx, ly) = poly.iter().fold((i32::MAX, i32::MAX), |(x, y), p| (x.min(p.0), y.min(p.1)));
            cv.text(&self.fonts().s, lx + 4, ly + 14, &format!("Z{}", k + 1), col);
        }
        // the path being drawn is in screen coordinates: once, over the whole plot
        if self.plot.cur_win != 0 {
            cv.no_clip();
            return;
        }
        cv.set_clip(self.plot.grid);
        if self.zone_trig.arm.is_some_and(|a| a.1) {
            for w in self.input.zone_path.windows(2) {
                cv.line(w[0].0, w[0].1, w[1].0, w[1].1, ACCENT);
            }
        }
        if self.zone_trig.arm.is_some_and(|a| !a.1) && self.input.zone_path.len() == 2 {
            let ((x0, y0), (x1, y1)) = (self.input.zone_path[0], self.input.zone_path[1]);
            for (p, q) in [((x0, y0), (x1, y0)), ((x1, y0), (x1, y1)), ((x1, y1), (x0, y1)), ((x0, y1), (x0, y0))] {
                cv.line(p.0, p.1, q.0, q.1, ACCENT);
            }
        }
        cv.no_clip();
    }

    pub fn tap(&mut self, x: i32, y: i32) {
        if self.numpad_tap(x, y) {
            return;
        }
        if self.load_slot_tap(x, y) {
            return;
        }
        let studio = self.m.layout == Layout::Studio;
        let box_button = (!studio
            && self.m.zoom
            && self.zoom_controls().iter().any(|(rect, text)| *text == "Box" && rect.contains(x, y)))
            || (studio && self.studio_zoom_bar().is_some_and(|b| b.boxr.contains(x, self.zoom_row_y(x, y))));
        if !box_button {
            self.zoom_box.arm = false;
        }
        if self.workspace_popup_tap(x, y) {
            return;
        }
        if self.single_btn().contains(x, y) {
            self.key(key::SINGLE);
            return;
        }
        if studio && self.full_on() && self.full_chrome_tap(x, y) {
            return;
        }
        if studio && !self.full_on() && self.studio_tap(x, y) {
            return;
        }
        if self.m.layout == Layout::Minimal && !self.full_on() && self.workspace_footer_tap(x, self.bar_y(y)) {
            return;
        }
        if self.power.open {
            self.power_tap(x, y);
            return;
        }
        if self.wlan.osk.is_some() {
            return self.osk_tap(x, y);
        }
        if self.dialog.is_some() {
            self.dialog = None;
            return;
        }
        if self.tap_menu(x, y) {
            return;
        }
        if self.waves_board_tap(x, y) {
            return;
        }
        if let Some(r) = self.live_ref_fit_rect() {
            if r.contains(x, y) {
                self.fit_shown_reference();
                return;
            }
        }
        if !self.menu_open && self.measurement_navigation_tap(x, y) {
            return;
        }
        if self.auto_bar_tap(x, y) {
            return;
        }
        if self.full_btn().1.contains(x, y) {
            self.toggle_expand();
            return;
        }
        if !self.menu_open && self.eye_open() && self.eye_pane_rect().contains(x, y) {
            if self.eye_tap(x, y) || self.eye_column_tap(x, y) {
                return;
            }
            let a = self.eye_pane_rect();
            if y < a.y + workspace::HEADER {
                self.eye_header_tap(x, y);
            }
            return;
        }
        if self.zoom_btn().1.contains(x, y) && !self.view_on() {
            self.set_zoom(!self.m.zoom);
            return;
        }
        if self.win_btn().1.contains(x, y) && self.plot_btns_shown() {
            return self.toggle_menu(MenuId::Windows);
        }
        if self.zoom_close_tap(x, y) {
            return;
        }
        if (self.view_on() || self.view_tile_rect().is_some()) && self.view_btn().contains(x, y) {
            return self.toggle_menu(MenuId::ViewMore);
        }
        if self.view_fix_tap(x, y) {
            return;
        }
        if !self.menu_open && self.callout_tap(x, y) {
            return;
        }
        if !self.menu_open
            && self.m.view == AView::Eye
            && !self.xy_over()
            && (self.eye_tap(x, y) || self.eye_column_tap(x, y))
        {
            return;
        }
        if self.table_tap(x, y) || self.pane_tap(x, y) {
            return;
        }
        // a box of a decoder's band: its row
        if !self.menu_open && self.band_tap(x, y) {
            return;
        }
        if self.eye_tap(x, y) {
            return;
        }
        if self.full_on() {
            // no top bar and no cards: only what is over the plot
            if let Some((h, _)) = self.results_rects() {
                if Rect::new(h.x, h.y - 5, h.w, h.h + 10).contains(x, y) {
                    self.results.folded = !self.results.folded;
                    return;
                }
            }
            if self.menu_open && self.plot.grid.contains(x, y) {
                self.close_menu();
            }
            return;
        }
        self.tap_top_bar(x, y);
    }

    /// A tap while the power dialog is open: it closes; Power off or Restart is taken unless an update runs.
    fn power_tap(&mut self, x: i32, y: i32) {
        self.power.open = false;
        let (_, [off, restart, _]) = self.pwr_layout();
        if off.contains(x, y) {
            self.power_request(PowerAction::Off);
        } else if restart.contains(x, y) {
            self.power_request(PowerAction::Restart);
        }
    }

    /// A tap on the top bar, the menu button or the results header; the rest goes to the cards.
    fn tap_top_bar(&mut self, x: i32, y: i32) {
        let pill = self.run_pill();
        if self.top_hit(pill).contains(x, y) {
            return self.key(key::RUNSTOP);
        }
        if let Some((b, _)) = self.status_boxes().into_iter().find(|(_, r)| self.top_hit(*r).contains(x, y)) {
            return self.toggle_menu(b.page);
        }
        if self.view_box_tap(x, y) {
            return;
        }
        // sample rate, depth and mode in the top bar: the page that sets them
        let end = self.view_box().map_or(self.top_buttons().first().map_or(self.top_right(), |b| b.2.x), |v| v.x) - 4;
        if self.top_hit(Rect::new(pill.x + pill.w, pill.y, end - pill.x - pill.w, pill.h)).contains(x, y) {
            return self.toggle_menu(if self.record_mode() { MenuId::Ref } else { MenuId::Acquire });
        }
        for (b, _, r) in self.top_buttons() {
            if self.top_hit(r).contains(x, y) {
                return match b {
                    // a tap opens the measurement list; a press-and-hold opens the short menu (meas_hold_tick)
                    TopBtn::Menu(MenuId::Measure) => self.open_meas_list(),
                    // as the CURSOR key: the cursors come on with their sheet
                    TopBtn::Menu(MenuId::Cursor) if self.m.compact() => self.key(key::CURSOR),
                    TopBtn::Menu(id) => self.toggle_menu(id),
                    TopBtn::Live => self.return_to_live(),
                    TopBtn::System => self.toggle_menu(MenuId::Utility),
                    TopBtn::Load => self.request_waveform(),
                };
            }
        }
        if self.menu_btn().contains(x, y) {
            return self.toggle_menu(MenuId::Main);
        }
        // header of the results overlay (40 px touch area): fold / unfold
        if let Some((h, _)) = self.results_rects() {
            if Rect::new(h.x, h.y - 5, h.w, h.h + 10).contains(x, y) {
                self.results.folded = !self.results.folded;
                return;
            }
        }
        self.tap_cards(x, y);
    }

    /// A tap on the channel, LA, generator, time, record or trigger cards, or on the grid to close the menu.
    fn tap_cards(&mut self, x: i32, y: i32) {
        let (chs, tr, trg) = self.cards();
        for (c, r) in chs.iter().enumerate() {
            if r.contains(x, y) {
                if !self.m.ch[c].on {
                    self.m.ch[c].on = true;
                    self.settings_changed();
                } else if self.m.n_on() > 1 && self.m.active != c {
                    // as the channel key: with several channels on, the first tap only selects; the next opens the
                    // menu. Another channel's menu would show the wrong channel: it closes.
                    self.m.active = c;
                    if self.menu_open && matches!(self.menu, MenuId::Channel(_)) {
                        self.close_menu();
                    }
                    return self.select_workspace_source(WaveId::Ch(c));
                }
                self.m.active = c;
                return self.toggle_menu(MenuId::Channel(c));
            }
        }
        let (la, gen) = self.aux_cards();
        if la.is_some_and(|r| r.contains(x, y)) {
            // like a channel: an LA that is off comes on with its page
            if self.m.la.on {
                return self.toggle_menu(MenuId::La);
            }
            return self.la_key(); // says so when there is no probe
        }
        if gen.is_some_and(|r| r.contains(x, y)) {
            // only the page: an output does not come on by touching a card
            return self.toggle_menu(MenuId::Afg);
        }
        if self.card_layout().refs.is_some_and(|r| r.contains(x, y)) { return self.cycle_active_ref(); }
        if tr.contains(x, y) {
            return self.toggle_menu(MenuId::Horizontal);
        }
        if self.record_mode() {
            for (slot, rect) in self.record_cards() {
                if rect.contains(x, y) {
                    self.ref_page.sel = slot;
                    self.offline.slot = slot;
                    self.sources.a = WaveId::Ref(slot);
                    if x >= rect.x + rect.w - 42 && y < rect.y + 28 {
                        if let Some(reference) = self.refs[slot].as_mut() {
                            reference.on = !reference.on;
                        }
                        return;
                    }
                    return self.toggle_menu(MenuId::Ref);
                }
            }
        }
        if trg.contains(x, y) {
            if self.record_mode() {
                let (run, live) = Self::session_buttons(trg);
                if run.contains(x, y) {
                    self.key(key::RUNSTOP);
                } else if live.contains(x, y) {
                    self.return_to_live();
                } else {
                    self.toggle_menu(MenuId::Trigger);
                }
                return;
            }
            // the Source and Mode buttons are what is drawn as buttons; the rest of the card opens the trigger page
            let (sb, mb) = Self::trig_parts(trg);
            let near = |b: Rect| Rect::new(b.x - 2, b.y - 2, b.w + 4, b.h + 4).contains(x, y);
            if near(sb) { return self.adjust(Item::TrigSrc, 1); }
            if near(mb) { return self.adjust(Item::TrigMode, 1); }
            return self.toggle_menu(MenuId::Trigger);
        }
        if self.plot_tap(x, y) { return; }
        if self.menu_open && self.plot.grid.contains(x, y) {
            self.close_menu();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::testkit::demo_ui;

    fn wheel(ui: &mut Ui, steps: i32, alt: bool) {
        ui.handle(Ev::Wheel { x: 10, y: 10, steps, ctrl: false, shift: false, alt });
    }

    /// What a turn left: the menu's focus and the whole setup.
    fn state(ui: &Ui) -> (usize, String) {
        (ui.focus, crate::store::setup_text(&ui.m))
    }

    /// With a menu open the wheel is Multi 1, with Alt the trigger level (5 steps a notch): each as `knob <name> <n>`
    /// of a script turns it, and the turn changes something.
    #[test]
    fn the_wheel_turns_multi1_in_a_menu_and_the_trigger_level_with_alt() {
        let (mut wheeled, mut scripted, untouched) = (demo_ui(), demo_ui(), demo_ui());
        for ui in [&mut wheeled, &mut scripted] {
            ui.open_menu(MenuId::Channel(0));
        }
        wheel(&mut wheeled, 2, false);
        crate::cmd::run(&mut scripted, "knob multi1 +2");
        assert_eq!(state(&wheeled), state(&scripted));
        assert_ne!(state(&wheeled).0, untouched.focus, "Multi 1 moved the focus");

        let (mut wheeled, mut scripted, untouched) = (demo_ui(), demo_ui(), demo_ui());
        wheel(&mut wheeled, -3, true);
        crate::cmd::run(&mut scripted, "knob tlevel -15");
        assert_eq!(state(&wheeled), state(&scripted));
        assert_ne!(state(&wheeled).1, state(&untouched).1, "the trigger level moved");
    }
}
