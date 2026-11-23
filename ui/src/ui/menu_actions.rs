// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What a menu item does: turning the knob on it or tapping its arrows (`Ui::adjust`, the segment items in
//! `Ui::seg_adjust`), pressing it (`Ui::activate`), and switching the decoder protocol and the trigger type with the
//! settings they bring along. A new item gets its action here and its text in `menu_text.rs`.

use super::*;

impl Ui {
    /// The shared rows a protocol's set-up gives (its baud rate, bits, parity ...) into the decoder of `m`.
    pub(crate) fn dec_apply_rows(m: &mut Model, rows: &[(crate::protocols::Row, u32)]) {
        use crate::protocols::Row;
        use oos_dsp::decode::Parity;
        for &(r, v) in rows {
            match r {
                Row::Baud => m.dec_baud = v as usize,
                Row::Bits => m.dec_bits = v,
                Row::Parity => m.dec_parity = [Parity::None, Parity::Even, Parity::Odd][(v as usize).min(2)],
                Row::SpiMode => m.dec_mode = v as u8,
                Row::CanRate => m.dec_canrate = v as usize,
                Row::CanH => m.dec_canh = v != 0,
                Row::UsbSpeed => m.dec_uspeed = v as usize,
                Row::CphyRate | Row::Fmt | Row::Pol | Row::SpiCs => {}
            }
        }
    }

    /// Switches the decoder protocol. The demo takes the sources, channels and trigger that match its bus traffic
    /// (`Protocol::setup`). On every other source (the hardware, Live, Replay of a recorded file) one setting must not
    /// change another (the forum, eTobey): the channels and the trigger stay as they are, the sources the protocols
    /// share stay, and a role the old protocol did not have takes a channel that is on and no other role uses, or none.
    /// The protocol's own parameters (threshold, baud, bits ...) are set on both.
    pub(super) fn set_dec_proto(&mut self, p: Proto) {
        let Proto::Module(i) = p;
        let s = crate::protocols::get(i).setup();
        if Self::decoder_uses_refs(&self.m) {
            self.m.dec_proto = p;
            if s.la_off {
                (self.m.dec_thr, self.m.dec_la_in) = (s.thr, false);
            }
            return;
        }
        let demo = self.src.is_demo();
        // Default is upstream's: on the demo a protocol moves the trigger onto its traffic; Studio and Minimal leave
        // the trigger alone (a protocol only configures the decoder)
        let upstream = !self.m.compact();
        let m = &mut self.m;
        let changed = m.dec_proto != p;
        let old = m.dec_proto.roles();
        m.dec_proto = p;
        // its threshold and shared rows; its own options from their defaults when it is newly chosen
        m.dec_thr = s.thr;
        let r = p.roles();
        if demo {
            // its demo channels and LA lines; roles the protocol doesn't have keep their sources
            m.dec_la = s.la;
            m.dec_src = s.src[0].unwrap_or(2);
            if r[2].is_some() {
                m.dec_cs = s.src[2];
            }
            if r[3].is_some() {
                m.dec_src4 = s.src[3];
            }
            if let Some(c) = s.src[1] {
                m.dec_src2 = c;
            }
        } else {
            // a new role: a channel that is on and that no other role of the protocol uses
            let mut used = vec![m.dec_src];
            if r[1].is_some() && old[1].is_some() && m.dec_src2 != m.dec_src {
                used.push(m.dec_src2);
            }
            used.extend(
                [(r[2], old[2], m.dec_cs), (r[3], old[3], m.dec_src4)]
                    .into_iter()
                    .filter_map(|(n, o, c)| n.and(o).and(c)),
            );
            let mut free = |m: &Model| {
                let c = (0..NCH).find(|&c| m.ch[c].on && !used.contains(&c));
                used.extend(c);
                c
            };
            if r[1].is_some() && (old[1].is_none() || m.dec_src2 == m.dec_src) {
                if let Some(c) = free(m) {
                    m.dec_src2 = c;
                }
            }
            if r[2].is_some() && (old[2].is_none() || m.dec_cs.is_none()) {
                m.dec_cs = free(m);
            }
            // a fourth line the protocol's set-up has no place for is one it reads without (the MISO of SPI): it
            // stays unset, where a channel that happens to be on would be read as that line
            if r[3].is_some() && s.src[3].is_none() {
                if old[3].is_none() {
                    m.dec_src4 = None;
                }
            } else if r[3].is_some() && (old[3].is_none() || m.dec_src4.is_none()) {
                m.dec_src4 = free(m);
            }
        }
        if s.la_off {
            m.dec_la_in = false;
        }
        Self::dec_apply_rows(m, s.rows);
        if changed {
            m.dec_opt = crate::protocols::opt_default(i);
            // a module with a view of its own: open it, when no other pane is open
            if m.pane == 0
                && crate::protocols::get(i).view(&[], &crate::protocols::opt_fit(i, m.dec_opt), &mut None).is_some()
            {
                m.pane = tables::PANE_BUS;
            }
        }
        if !demo {
            self.dirty_map = true;
            return;
        }
        // the demo: every source the protocol uses; the traffic of some is on all its channels or on every channel
        m.ch[m.dec_src].on = true;
        if r[1].is_some() {
            m.ch[m.dec_src2].on = true;
        }
        if let (Some(_), Some(c)) = (r[2], m.dec_cs) {
            m.ch[c].on = true;
        }
        if let (Some(_), Some(c)) = (r[3], m.dec_src4) {
            m.ch[c].on = true;
        }
        if s.show {
            for c in s.src.into_iter().flatten().filter(|&c| c < NCH) {
                m.ch[c].on = true;
            }
        }
        if s.show_all {
            for channel in &mut m.ch {
                channel.on = true;
            }
        }
        // Studio and Minimal: the trigger is not touched, choosing a protocol only configures the decoder (and shows
        // its sources). Default (upstream): on the demo, a trigger that holds its traffic still, on the channel of the
        // role
        if let Some(t) = s.trig.filter(|_| demo && upstream) {
            let c = [Some(m.dec_src), Some(m.dec_src2), m.dec_cs, m.dec_src4][t.role.min(3)];
            if let Some(c) = c.filter(|&c| c < NCH) {
                (m.trig_src, m.slope, m.trig_level) = (c, t.slope, t.level);
                if t.idle > 0.0 {
                    (m.trig_type, m.nth_idle, m.nth_n) = (TrigType::NthEdge, t.idle, 1);
                } else {
                    m.trig_type = TrigType::Edge;
                }
            }
        }
        self.dirty_map = true;
    }

    /// Default (upstream): Bus switches the decoder on with its demo sources and a value its traffic contains, and
    /// remembers what it switched on (`bus_auto`), which leaving Bus switches off again.
    fn bus_with_decoder(&mut self, was_bus: bool) {
        let before: [bool; NCH] = std::array::from_fn(|c| self.m.ch[c].on);
        let dec_before = self.m.dec_on;
        self.m.dec_on = true;
        let p = self.m.dec_proto;
        self.set_dec_proto(p);
        // Bus chosen again keeps what the first choice switched on
        if !was_bus {
            self.bus_auto = Some((std::array::from_fn(|c| self.m.ch[c].on && !before[c]), !dec_before));
        }
        let m = &mut self.m;
        m.trig_type = TrigType::Bus;
        let Proto::Module(i) = p;
        let (what, val) = crate::protocols::get(i).info().trigger_default;
        (m.trig_bus, m.trig_bus_val, m.ser) = (what, val as u64, SerialTrig::default());
        self.dirty_map = true;
    }

    /// Default (upstream): on the demo each trigger type also goes to the channel that shows it, switched on. Studio and
    /// Minimal keep the source and the channels: a trigger type change only touches the trigger.
    fn upstream_demo_source(m: &mut Model, t: TrigType) {
        let on = |c: usize, m: &mut Model| m.ch[c].on = true;
        match t {
            TrigType::Edge | TrigType::Slope | TrigType::Duration | TrigType::Timeout | TrigType::Runt => m.trig_src = 0,
            TrigType::Pulse => {
                on(3, m);
                m.trig_src = 3;
            }
            TrigType::Video => {
                on(1, m);
                m.trig_src = 1;
                m.ch[1].vscale = VSCALES.iter().position(|&v| (v - 0.2).abs() < 1e-6).unwrap_or(8);
                m.ch[1].offset = -0.5;
            }
            TrigType::Pattern => {
                on(3, m);
                m.trig_src = 0;
            }
            TrigType::Window => {
                on(1, m);
                m.trig_src = 1;
            }
            TrigType::Delay => {
                on(3, m);
                (m.trig_src, m.trig_src2) = (0, 3);
            }
            TrigType::SetupHold => {
                on(2, m);
                on(3, m);
                (m.trig_src, m.trig_src2) = (2, 3);
            }
            TrigType::NthEdge => {
                on(2, m);
                m.trig_src = 2;
            }
            TrigType::Bus => {}
        }
    }

    /// Switches the trigger type and sets starting values that trigger on the demo signals.
    pub(super) fn set_trig_type(&mut self, t: TrigType) {
        let was_bus = self.m.trig_type == TrigType::Bus;
        // leaving Bus: off again what entering it switched on (the decoder kept the live picture raw, its demo sources
        // stayed on); a type set elsewhere since leaves nothing to undo
        let undo = if t != TrigType::Bus { self.bus_auto.take().filter(|_| was_bus) } else { None };
        if let Some((chans, dec)) = undo {
            for c in (0..NCH).filter(|&c| chans[c]) {
                self.m.ch[c].on = false;
            }
            if dec {
                self.m.dec_on = false;
            }
        }
        if t == TrigType::Bus && Self::decoder_uses_refs(&self.m) {
            if let Some(source) = decode_wave(self.m.dec_src) {
                self.set_trigger_source(source, false);
                self.m.trig_type = t;
                self.m.dec_on = true;
                self.replay.reset_search();
                return;
            }
        }
        if self.set_reference_trigger_type(t) {
            return;
        }
        if t == TrigType::Bus && !self.m.compact() {
            self.bus_with_decoder(was_bus);
            return;
        }
        if t == TrigType::Bus {
            // Studio and Minimal: the decoder's protocol defines the bus: the starting match is a value its traffic
            // contains. Choosing the trigger type neither switches a decoder on nor touches its sources or any channel.
            let m = &mut self.m;
            m.trig_type = t;
            // a value the protocol's demo traffic contains
            let Proto::Module(i) = m.dec_proto;
            let (what, val) = crate::protocols::get(i).info().trigger_default;
            (m.trig_bus, m.trig_bus_val, m.ser) = (what, val as u64, SerialTrig::default());
            self.dirty_map = true;
            return;
        }
        // the presets below suit the demo signals (each type on a channel that shows it). Every other source (the
        // hardware, a live or replayed record) keeps what the user set: the channels, source, level and slope, and
        // also the width, the pattern, the lower level and the rest that several types share (the forum, eTobey: one
        // setting must not change another). A SIM session triggers on its reference: set_reference_trigger_type
        let demo = self.src.is_demo();
        let upstream = !self.m.compact();
        let m = &mut self.m;
        m.trig_type = t;
        // Only fields of the trigger itself change: a source that is still valid stays, and no channel is switched on.
        let keep = |m: &Model, c: usize| {
            if c < NCH && m.ch[c].on {
                c
            } else {
                (0..NCH).find(|&k| m.ch[k].on).unwrap_or(c.min(NCH - 1))
            }
        };
        m.trig_src = keep(m, m.trig_src);
        let src = m.trig_src;
        let second = |m: &Model| {
            let c = m.trig_src2;
            if c != src && c < NCH && m.ch[c].on {
                c
            } else {
                let fallback = if c != src && c < NCH { c } else { (src + 1) % NCH };
                (0..NCH).find(|&k| k != src && m.ch[k].on).unwrap_or(fallback)
            }
        };
        match t {
            _ if !demo => {}
            TrigType::Bus => {} // handled above
            TrigType::Edge => {
                m.trig_level = 1.5;
            }
            TrigType::Pulse => {
                (m.trig_pol, m.trig_when, m.trig_t1, m.trig_level) = (Pol::Pos, WhenKind::Lt, 30e-6, 1.65);
            }
            TrigType::Slope => {
                (m.slope, m.trig_when, m.trig_t1, m.trig_level, m.trig_level2) =
                    (Slope::Rise, WhenKind::Gt, 1e-6, 2.0, 0.5);
            }
            TrigType::Video => {
                m.trig_level = 0.15;
            }
            TrigType::Pattern => {
                m.pat = std::array::from_fn(|i| {
                    if i == 0 {
                        Bit::H
                    } else if i == NCH - 1 {
                        Bit::R
                    } else {
                        Bit::X
                    }
                });
            }
            TrigType::Duration => {
                m.pat = std::array::from_fn(|i| if i == 0 { Bit::H } else { Bit::X });
                (m.trig_when, m.trig_t1) = (WhenKind::Gt, 200e-6);
            }
            TrigType::Timeout => {
                (m.slope, m.trig_t1, m.trig_level) = (Slope::Rise, 1.2e-3, 1.5);
            }
            TrigType::Runt => {
                (m.trig_pol, m.trig_when, m.trig_level, m.trig_level2) = (Pol::Pos, WhenKind::Off, 2.0, 0.5);
            }
            TrigType::Window => {
                (m.win_mode, m.trig_level, m.trig_level2) = (WinMode::Enter, 0.5, -0.5);
            }
            TrigType::Delay => {
                let s2 = second(m);
                (m.slope, m.trig_src2, m.trig_slope2, m.trig_when, m.trig_t1, m.trig_level, m.trig_level2) =
                    (Slope::Rise, s2, Slope::Rise, WhenKind::Gt, 40e-6, 1.5, 1.65);
            }
            TrigType::SetupHold => {
                let s2 = second(m);
                (m.trig_src2, m.trig_slope2, m.setup, m.hold, m.trig_level, m.trig_level2) =
                    (s2, Slope::Rise, 2e-6, 2e-6, 1.65, 1.65);
            }
            TrigType::NthEdge => {
                (m.slope, m.nth_idle, m.nth_n, m.trig_level) = (Slope::Fall, 50e-6, 1, 1.65);
            }
        }
        if demo && upstream {
            Self::upstream_demo_source(m, t);
        }
        if m.trig_type != TrigType::Runt && m.trig_when == WhenKind::Off {
            m.trig_when = WhenKind::Lt;
        }
        if !demo {
            // the second source of Delay and Setup/Hold on a channel that is on (PR #8, W4R)
            let src = m.trig_src;
            if matches!(m.trig_type, TrigType::Delay | TrigType::SetupHold)
                && (m.trig_src2 == src || !m.ch[m.trig_src2.min(NCH - 1)].on)
            {
                if let Some(c) = (0..NCH).find(|&k| k != src && m.ch[k].on) {
                    m.trig_src2 = c;
                }
            }
        }
        // the lower level the type finds (Delay's level B, say) not above the shared upper one
        m.order_trigger_levels();
        self.dirty_map = true;
    }

    /// The segment items of the Acquire menu; false for any other item.
    fn seg_adjust(&mut self, it: Item, d: i32) -> bool {
        match it {
            Item::AcqSeg => {
                let o = self.seg_options();
                let cur = if self.m.segments == 0 { 0 } else { self.seg_count() };
                let i = o.iter().position(|&n| n >= cur).unwrap_or(0);
                self.m.segments = o[cycle(i, o.len(), d)];
            }
            Item::AcqSegIvl => {
                let i = SEG_IVLS.iter().position(|&x| x >= self.m.seg_ivl).unwrap_or(0);
                self.m.seg_ivl = SEG_IVLS[cycle(i, SEG_IVLS.len(), d)];
            }
            _ => return false,
        }
        true
    }

    /// One press (or `d` presses) of a stepper. The limits are in the range table (`ranges.rs`).
    pub(super) fn adjust(&mut self, it: Item, d: i32) {
        if self.item_blocked_toast(it) {
            return;
        }
        // a channel the user switches is theirs from then on, not one a source row turned on
        if let Item::ChOn(c) = it {
            self.auto_on.retain(|&a| a != c);
        }
        self.adjust_item(it, d);
        self.range_enforce(it);
    }

    fn adjust_item(&mut self, it: Item, d: i32) {
        if matches!(it, Item::CatSourceC | Item::CatSourceD) {
            if it == Item::CatSourceC {
                self.cat.c = self.step_wave(self.cat.c, d);
            } else {
                self.cat.d = self.step_wave(self.cat.d, d);
            }
            return;
        }
        if matches!(it, Item::MathA | Item::MathB) {
            let sources = self.math_source_ids(self.m.math_sel);
            let current = if it == Item::MathA { self.m.math_a } else { self.m.math_b };
            if !sources.is_empty() {
                let i = sources.iter().position(|&s| s == current).unwrap_or(0);
                let source = sources[(i as i32 + d).rem_euclid(sources.len() as i32) as usize];
                if it == Item::MathA {
                    self.m.math_a = source;
                } else {
                    self.m.math_b = source;
                }
                self.settings_changed();
            }
            return;
        }
        if it == Item::Traces {
            let stacked = !self.trace_stacked();
            self.choose_trace_mode(stacked);
            return;
        }
        // oos-plus: the trigger times step on the 1-2-5 ladder; a value between two rungs (30 us) goes to the nearest
        // one in the direction of the press and the press back returns to it (`ladder_move`)
        if matches!(it, Item::TrigT1 | Item::TrigT2 | Item::NthIdle | Item::Setup | Item::Hold) {
            let m = &self.m;
            let cur = match it {
                Item::TrigT1 => m.trig_t1,
                Item::TrigT2 => m.trig_t2,
                Item::NthIdle => m.nth_idle,
                Item::Setup => m.setup,
                _ => m.hold,
            } as f64;
            let l: Vec<f64> = crate::model::time_ladder().iter().map(|&x| x as f64).collect();
            let v = self.ladder_move(it, cur, d, &l) as f32;
            match it {
                Item::TrigT1 => self.m.trig_t1 = v,
                Item::TrigT2 => self.m.trig_t2 = v,
                Item::NthIdle => self.m.nth_idle = v,
                Item::Setup => self.m.setup = v,
                _ => self.m.hold = v,
            }
            self.settings_changed();
            return;
        }
        if PARTS.iter().any(|p| p.adjust.is_some_and(|f| f(self, it, d))) {
            return;
        }
        if matches!(it, Item::TrigSrc | Item::TrigSrc2) {
            let secondary = it == Item::TrigSrc2;
            let sources = self.trigger_sources();
            let selected = if secondary { self.m.trigger_wave2() } else { self.m.trigger_wave() };
            if !sources.is_empty() {
                let pos = sources.iter().position(|&id| id == selected).unwrap_or(0);
                let id = sources[(pos as i32 + d).rem_euclid(sources.len() as i32) as usize];
                self.set_trigger_source(id, secondary);
            }
            return;
        }
        // the steppers that add a step to a value and stop at the limits of the range table (`ranges.rs`)
        if self.range_step(it, d) {
            // the reference level restarts the averaging and the picture of the spectrum
            if it == Item::SpecRef {
                self.azr.spec_avg = None;
                self.analyzer_update();
            }
            // a level of a reference trigger does not restart what was measured
            if !(matches!(it, Item::TrigLevel | Item::TrigLevel2) && self.m.trig_ref.is_some()) {
                self.settings_changed();
            }
            return;
        }
        if self.seg_adjust(it, d) {
            return;
        }
        // a step of the search switch changes no setting: no new acquisition, no statistics cleared
        if it == Item::SearchOn {
            self.search_on = !self.search_on;
            return;
        }
        let _ = self.adjust_scope(it, d) || self.adjust_tools(it, d) || self.adjust_more(it, d);
        // nor does which values a trend chart shows
        if matches!(it, Item::TrendSel(_)) {
            self.relayout();
            if self.m.run == RunState::Run { self.dirty_map = true; }
            return;
        }
        self.settings_changed();
    }

    /// `adjust` of the channels, the time base, the logger and the trigger. True when `it` is one of them.
    fn adjust_scope(&mut self, it: Item, d: i32) -> bool {
        let m = &mut self.m;
        match it {
            Item::ChCoupling(c) => {
                let all = board().couplings();
                let i = all.iter().position(|&x| x == m.ch[c].coupling).unwrap_or(0);
                m.ch[c].coupling = all[cycle(i, all.len(), d)];
            }
            Item::ChProbe(c) => {
                let all = [1.0, 10.0, 100.0];
                let i = all.iter().position(|&x| x == m.ch[c].probe).unwrap_or(0);
                m.ch[c].probe = all[cycle(i, 3, d)];
            }
            Item::ChHide(c) => m.ch_hidden[c] = !m.ch_hidden[c],
            Item::ChScale(c) => {
                let old = m.ch[c].volts_per_div();
                if m.fine_v {
                    // row by row through the calibration table: each scale has its own record
                    let rows = board().vscale_steps_nv();
                    let now = VSCALES[m.ch[c].vscale] as f64 * m.ch[c].vfine as f64 * 1e9 * 0.999999;
                    let at = rows.iter().position(|&k| k as f64 >= now).unwrap_or(rows.len() - 1) as i32;
                    let want = (rows[(at + d).clamp(0, rows.len() as i32 - 1) as usize] as f64 * 1e-9) as f32;
                    let i = VSCALES.iter().rposition(|&s| s <= want * 1.000001).unwrap_or(0);
                    let f = want / VSCALES[i];
                    (m.ch[c].vscale, m.ch[c].vfine) = (i, if (f - 1.0).abs() < 1e-6 { 1.0 } else { f });
                } else {
                    m.ch[c].vscale = step_index(m.ch[c].vscale, VSCALES.len(), d);
                    m.ch[c].vfine = 1.0;
                }
                // around ground: the zero line keeps its place on the screen
                if m.v_expand_gnd {
                    m.ch[c].offset *= m.ch[c].volts_per_div() / old;
                }
            }
            Item::HFine => m.fine_t = !m.fine_t,
            Item::StoreRecScpi => self.scpi_rec_want = !self.scpi_rec_want,
            Item::LogOn => {
                if self.log.file.is_some() {
                    self.log_stop("Log ended");
                } else {
                    self.log_start();
                }
            }
            Item::LogIvl => m.log_ivl = step_index(m.log_ivl, logger::LOG_STEPS.len(), d),
            Item::HScale => {
                let old = m.time_per_div();
                if m.fine_t {
                    (m.tscale, m.tfine) = fine_time(old, d);
                } else {
                    // from a value between two steps (the fine time base left it there, or a pinch): the step
                    // beside it comes first. `tscale` is the step above a factor under 1 and the one below a
                    // factor over 1, so one step towards it is that step itself: 26 us went to 100 us, past 50
                    let d = if m.tfine < 1.0 && d > 0 { d - 1 } else if m.tfine > 1.0 && d < 0 { d + 1 } else { d };
                    m.tscale = step_index(m.tscale, tscales().len(), d);
                    m.tfine = 1.0;
                }
                // Studio: the zoom keeps its factor and its place in the overview when the main time base changes
                let keep = (m.layout == Layout::Studio && m.zoom)
                    .then(|| (old / tscales()[m.zscale], (m.zdelay - m.delay) / (old * HDIV)));
                m.keep_expand_point(old);
                let new = m.time_per_div();
                if let Some((factor, place)) = keep {
                    let want = new / factor;
                    m.zscale = tscales().iter().enumerate().min_by(|a, b| (a.1 / want).ln().abs().total_cmp(&(b.1 / want).ln().abs())).map_or(0, |(i, _)| i).min(m.tscale);
                    m.zdelay = m.delay + place * new * HDIV;
                }
            }
            Item::TrigSrc => {
                m.trig_src = src_step(m.trig_src, NCH, d);
            }
            Item::TrigType => {
                let i = TRIG_TYPES.iter().position(|&x| x == m.trig_type).unwrap_or(0);
                let t = TRIG_TYPES[cycle(i, TRIG_TYPES.len(), d)];
                self.set_trig_type(t);
            }
            Item::TrigLevel2 => {
                let c =
                    if matches!(m.trig_type, TrigType::Delay | TrigType::SetupHold) { m.trig_src2 } else { m.trig_src };
                m.trig_level2 += d as f32 * m.ch[c].volts_per_div() / 25.0;
                if m.trig_type.two_level() {
                    m.trig_level2 = m.trig_level2.min(m.trig_level);
                }
            }
            Item::TrigSrc2 => m.trig_src2 = src_step(m.trig_src2, NCH, d),
            Item::TrigWhen => {
                let all: &[WhenKind] = if m.trig_type == TrigType::Runt {
                    &[WhenKind::Off, WhenKind::Gt, WhenKind::Lt, WhenKind::In, WhenKind::Out]
                } else {
                    &[WhenKind::Gt, WhenKind::Lt, WhenKind::In, WhenKind::Out]
                };
                let i = all.iter().position(|&x| x == m.trig_when).unwrap_or(0);
                m.trig_when = all[cycle(i, all.len(), d)];
            }
            Item::Pat(c) => {
                let all = if m.trig_type == TrigType::Duration {
                    &[Bit::H, Bit::L, Bit::X][..]
                } else {
                    &[Bit::H, Bit::L, Bit::X, Bit::R, Bit::F][..]
                };
                let i = all.iter().position(|&x| x == m.pat[c]).unwrap_or(0);
                m.pat[c] = all[cycle(i, all.len(), d)];
            }
            Item::PatLevel => {
                let c = m.trig_src;
                m.pat_levels[c] += d as f32 * m.ch[c].volts_per_div() / 25.0;
            }
            _ => return false,
        }
        true
    }

    /// `adjust` of the acquisition depth, measurements, cursors, references, zones, display and math. True when `it`
    /// is one of them.
    fn adjust_tools(&mut self, it: Item, d: i32) -> bool {
        let m = &mut self.m;
        match it {
            Item::AcqDepth => m.mdepth = cycle(m.mdepth, mdepths().len(), d),
            Item::MeasSrc => self.sources.a = self.step_wave(self.sources.a, d),
            Item::MeasSrcB => self.sources.b = self.step_wave(self.sources.b, d),
            // Track cursor B on its own source (follows cursor A's until set)
            Item::CurSrcB => {
                let cur = self.cursor_b_source();
                let next = self.step_wave(cur, d);
                self.set_cursor_b_source(next);
            }
            Item::DvmSrc => m.dvm_src = src_step(m.dvm_src, NCH, d),
            Item::CntSrc => m.cnt_src = src_step(m.cnt_src, NCH, d),
            Item::MathLinA => {
                m.math_lin.0 = (m.math_lin.0 + 0.1 * d as f32).clamp(ranges::MATH_LIN.0, ranges::MATH_LIN.1)
            }
            Item::MathLinB => {
                m.math_lin.1 = (m.math_lin.1 + 0.1 * d as f32).clamp(ranges::MATH_LIN.0, ranges::MATH_LIN.1)
            }
            Item::CurMode => {
                let all = [CursorMode::Off, CursorMode::Time, CursorMode::Volt, CursorMode::Track, CursorMode::Xy];
                let i = all.iter().position(|&x| x == m.cursor).unwrap_or(0);
                m.cursor = all[cycle(i, all.len(), d)];
            }
            // a cursor on a voltage reads higher the further up the screen it is: + raises the reading (moves it up)
            Item::CurA | Item::CurB | Item::CurC | Item::CurD => {
                let vertical = matches!(it, Item::CurC | Item::CurD) || m.cursor == CursorMode::Volt;
                let delta = d as f32 * 0.005 * if vertical { -1.0 } else { 1.0 };
                let field = match it { Item::CurA => &mut m.cur_a, Item::CurB => &mut m.cur_b, Item::CurC => &mut m.cur_c, _ => &mut m.cur_d };
                *field = (*field + delta).clamp(0.0, 1.0);
            }
            Item::RefSel => {
                let slot = (self.ref_page.sel as i32 + d).clamp(0, NREF as i32 - 1) as usize;
                self.select_workspace_source(WaveId::Ref(slot));
            }
            Item::RefColor => {
                if let Some(r) = &mut self.refs[self.ref_page.sel] {
                    r.color = cycle(r.color.min(REF_COLORS.len() - 1), REF_COLORS.len(), d);
                }
            }
            Item::RefVpd => {
                if let Some(r) = &mut self.refs[self.ref_page.sel] {
                    let i = VSCALES.iter().position(|&s| s >= r.vpd * 0.999).unwrap_or(VSCALES.len() - 1);
                    r.vpd = VSCALES[step_index(i, VSCALES.len(), d)];
                }
                self.measure_refs();
            }
            Item::RefOffset => {
                if let Some(r) = &mut self.refs[self.ref_page.sel] {
                    r.offset += 0.1 * r.vpd * d as f32;
                }
            }
            Item::ZoneSel => self.zone_trig.sel = cycle(self.zone_trig.sel, 2, d),
            Item::ZoneOn => {
                let z = &mut m.zones[self.zone_trig.sel];
                z.on = !z.on;
            }
            Item::ZoneSrc => {
                let z = &mut m.zones[self.zone_trig.sel];
                z.src = src_step(z.src, NCH, d);
            }
            Item::ZoneMode => {
                let z = &mut m.zones[self.zone_trig.sel];
                z.inside = !z.inside;
            }
            Item::Layout => {
                let i = LAYOUTS.iter().position(|&l| l == m.layout).unwrap_or(0);
                m.layout = LAYOUTS[cycle(i, LAYOUTS.len(), d)];
                self.studio.enter |= m.layout == Layout::Studio;
                self.meas_own.dialog = None; // a row's settings are the card in Minimal and Studio, a dialog in Default
                // Default is the stock OOS screen: its cards and top bar, not the compact bars a Bottom bar choice left
                if m.layout == Layout::Default {
                    m.bar_compact = false;
                }
                self.dirty_map = true;
            }
            Item::ZInput => {
                let i = m.z_src.map_or(0, |c| c + 1);
                let i = opt_src_step(i, d);
                m.z_src = if i == 0 { None } else { Some(i - 1) };
                self.dirty_map = true;
            }
            Item::AuxOut => {
                let all = AuxOut::choices(board().mask_hw() || board().zone_hw());
                let i = all.iter().position(|&a| a == m.aux_out).unwrap_or(0);
                m.aux_out = all[cycle(i, all.len(), d)];
            }
            Item::QuickAct => {
                let i = QUICKS.iter().position(|&q| q == m.quick).unwrap_or(0);
                m.quick = QUICKS[cycle(i, QUICKS.len(), d)];
            }
            Item::MathOpI => {
                let fam = self.math.family;
                let mut ops = vec![MathOp::Off];
                ops.extend_from_slice(fam.ops());
                let i = ops.iter().position(|&x| x == m.math).unwrap_or(0);
                m.math = ops[cycle(i, ops.len(), d)];
                m.math_offset = 0.0;
                self.math_autoscale();
            }
            Item::MathFiltPreset => {
                let i = self.math.preset.map_or(if d > 0 { FILT_PRESETS.len() - 1 } else { 0 }, |i| i);
                self.math_apply_preset(cycle(i, FILT_PRESETS.len(), d));
            }
            Item::MathFam => {
                let i = MathFamily::ALL.iter().position(|&f| f == self.math.family).unwrap_or(0);
                self.math.family = MathFamily::ALL[cycle(i, MathFamily::ALL.len(), d)];
                if m.math != MathOp::Off && m.math.family() != self.math.family {
                    m.math = self.math.family.ops()[0];
                    m.math_offset = 0.0;
                    self.math_autoscale();
                }
            }
            // sources: CH1..4 and the math channels below the selected one
            Item::MathA => m.math_a = src_step(m.math_a.min(NCH + m.math_sel - 1), NCH + m.math_sel, d),
            Item::MathB => m.math_b = src_step(m.math_b.min(NCH + m.math_sel - 1), NCH + m.math_sel, d),
            Item::MathSel => {
                let k = cycle(m.math_sel, NMATH, d);
                m.math_select(k);
                if m.math != MathOp::Off {
                    self.math.family = m.math.family();
                }
            }
            Item::MathScale => {
                let n = if m.math == MathOp::Fft { FFT_DB.len() } else { math_scales().len() };
                m.math_scale = step_index(m.math_scale, n, d);
            }
            Item::MeasStats => self.results.on = !self.results.on,
            Item::StripDock => {
                let (all, m) = (self.strip_docks(), &mut self.m);
                let i = all.iter().position(|&s| s == m.strip_dock).unwrap_or(0);
                m.strip_dock = all[step_index(i, all.len(), d)];
                // the statistics card follows the dock the strip uses
                self.workspace.measure_docked = true;
                self.workspace.measure_float = None;
            }
            _ => return false,
        }
        true
    }

    /// `adjust` of the decoder, zoom, XY, boot and storage choices, the mask and the reference slots. True when `it`
    /// is one of them.
    fn adjust_more(&mut self, it: Item, d: i32) -> bool {
        match it {
            Item::DecOn => self.m.dec_on = !self.m.dec_on,
            Item::DecSel => {
                let k = (self.m.dec_sel as i32 + d).rem_euclid(DECODERS as i32) as usize;
                self.m.dec_select(k);
            }
            Item::TrendSel(k) => {
                let n = self.trend_options().len() as i32;
                self.m.trend_sel[k] = (self.m.trend_sel[k].min(n as usize - 1) as i32 + d).rem_euclid(n) as usize;
            }
            Item::DecSrc => {
                let next = self.decoder_source_step(Some(self.m.dec_src), false, d);
                self.configure_decoder_source(0, next);
            }
            Item::DecBaud => self.m.dec_baud = step_index(self.m.dec_baud, crate::decode::BAUDS.len(), d),
            Item::DecCphyRate => {
                let rates = crate::decode::CPHY_RATES;
                let index = rates.iter().position(|&rate| rate >= self.m.dec_cphy_rate).unwrap_or(rates.len() - 1);
                self.m.dec_cphy_rate = rates[cycle(index, rates.len(), d)];
            }
            Item::DecCanRate => self.m.dec_canrate = step_index(self.m.dec_canrate, crate::decode::CAN_RATES.len(), d),
            Item::TrigBusWhat => {
                let i = BUS_WHATS.iter().position(|&x| x == self.m.trig_bus).unwrap_or(0);
                self.m.trig_bus = BUS_WHATS[cycle(i, BUS_WHATS.len(), d)];
            }
            Item::TrigBusVal => {
                self.m.trig_bus_val = (self.m.trig_bus_val as i64 + d as i64).clamp(0, BUS_VAL_MAX as i64) as u64
            }
            Item::SerRel
            | Item::SerMax
            | Item::SerMask
            | Item::UartPol
            | Item::UartStop
            | Item::UartErr
            | Item::SpiCs
            | Item::SpiTimeout
            | Item::I2cDir
            | Item::I2cAddrBits
            | Item::I2cBytes => self.serial_adjust(it, d),
            Item::DecCanH => self.m.dec_canh = !self.m.dec_canh,
            Item::DecFmt => self.m.dec_ascii = !self.m.dec_ascii,
            Item::DecProto => {
                // only the decoders System > Features offers (none: a hand-edited system.conf, see `load_system`)
                let on = self.protos_on();
                if !on.is_empty() {
                    let i = on.iter().position(|&x| x == self.m.dec_proto).unwrap_or(0);
                    self.set_dec_proto(on[cycle(i, on.len(), d)]);
                }
            }
            Item::DecUsbSpeed => self.m.dec_uspeed = cycle(self.m.dec_uspeed, crate::decode::USB_SPEEDS.len(), d),
            Item::DecOpt(k) => {
                if let Some(o) = self.m.dec_proto.module().and_then(|p| p.options().get(k)) {
                    self.m.dec_opt[k] = cycle(self.m.dec_opts()[k], o.choices.len(), d) as u8;
                }
            }
            Item::DecSrc4 => {
                let next = self.decoder_source_step(self.m.dec_src4, true, d);
                self.configure_decoder_source(3, next);
            }
            Item::DecDiff => {
                let next = self.decoder_source_step(self.m.dec_diff, true, d);
                self.configure_decoder_source(4, next);
            }
            Item::DecSrc2 => {
                let next = self.decoder_source_step(Some(self.m.dec_src2), false, d);
                self.configure_decoder_source(1, next);
            }
            Item::DecCs => {
                let next = self.decoder_source_step(self.m.dec_cs, self.m.dec_proto != Proto::CPHY, d);
                self.configure_decoder_source(2, next);
            }
            Item::DecMode => self.m.dec_mode = cycle(self.m.dec_mode as usize, 4, d) as u8,
            Item::DecBits => {
                let (lo, hi) = if self.m.dec_proto == Proto::UART { (5, 9) } else { (4, 32) };
                self.m.dec_bits = (self.m.dec_bits as i32 + d).clamp(lo, hi) as u32;
            }
            Item::DecParity => {
                use oos_dsp::decode::Parity;
                let all = [Parity::None, Parity::Even, Parity::Odd];
                let i = all.iter().position(|&x| x == self.m.dec_parity).unwrap_or(0);
                self.m.dec_parity = all[cycle(i, 3, d)];
            }
            Item::Zoom => {
                let on = !self.m.zoom;
                self.set_zoom(on);
            }
            Item::ZScale => {
                self.m.zscale = step_index(self.m.zscale, tscales().len(), d);
                self.clamp_zoom();
            }
            Item::ZDelay => {
                self.m.zdelay += d as f32 * tscales()[self.m.zscale] / 25.0;
                self.clamp_zoom();
            }
            Item::XyMode => self.xy = !self.xy_over(),
            Item::BootMode => self.boot_mode_toggle(),
            Item::Overclock => self.overclock_step(d),
            Item::StoreSlot => self.files.slot = (self.files.slot as i32 + d).clamp(ranges::STORE_SLOT.0, ranges::STORE_SLOT.1) as u32,
            Item::StoreDest => {
                let i = crate::store::StoreDest::ALL.iter().position(|&x| x == self.files.dest).unwrap_or(0);
                self.files.dest = crate::store::StoreDest::ALL[cycle(i, crate::store::StoreDest::ALL.len(), d)];
            }
            Item::MaskSrc => {
                // a mask belongs to its channel: the new source needs a new one
                self.m.mask_src = src_step(self.m.mask_src, NCH, d);
                self.mask_sw.forget();
                self.m.mask_on = false;
            }
            Item::MaskX => {
                self.m.mask_x = ((self.m.mask_x + d as f32 * 0.04) * 100.0).round().clamp(0.0, 200.0) / 100.0;
                self.build_mask();
            }
            Item::MaskY => {
                self.m.mask_y = ((self.m.mask_y + d as f32 * 0.04) * 100.0).round().clamp(4.0, 400.0) / 100.0;
                self.build_mask();
            }
            Item::RefSrc => self.ref_page.src = src_step(self.ref_page.src, NCH, d),
            Item::RefShow(i) => {
                if let Some(r) = &mut self.refs[i] {
                    r.on = !r.on;
                }
            }
            _ => return false,
        }
        true
    }

    pub(super) fn clear_measurement_statistics(&mut self) {
        self.results.rows.clear();
        // the settings of a row that is no longer selected go with it
        if !self.m.row_cfg.is_empty() {
            let keys = self.m.row_keys();
            self.m.row_cfg.retain(|(k, _)| keys.contains(k));
        }
        self.replay.clear_deferred_statistics();
        self.reset_catalog_statistics();
    }

    pub(super) fn settings_changed(&mut self) {
        self.acquiring_changed();
        self.clear_measurement_statistics();
    }

    /// What a change of a setting and a Run, Stop or Single have in common, without clearing the statistics: those
    /// three change no setting, so the statistics of what was acquired stay.
    pub(super) fn acquiring_changed(&mut self) {
        // whatever moved the upper level (50 %, SCPI's LEVel, a fit) leaves the two levels in order
        self.m.order_trigger_levels();
        self.cancel_bus_candidate(true);
        if self.m.trig_mode == TrigMode::Single && self.m.run == RunState::Run {
            self.single_armed = false;
            self.scpi_arm_operation();
        }
        if self.m.run == RunState::Run {
            self.dirty_map = true;
        }
    }

    pub(super) fn activate(&mut self, it: Item) {
        if self.item_blocked_toast(it) {
            return;
        }
        // the Add rail and the measurement row sheet act on any row of theirs, not only on actions
        if self.addrail_activate(it) || self.measrow_activate(it) {
            return;
        }
        if Self::kind(it) == Kind::Action && PARTS.iter().any(|p| p.activate.is_some_and(|f| f(self, it))) {
            return;
        }
        match Self::kind(it) {
            Kind::Choice => self.adjust(it, 1),
            Kind::Value => {}
            Kind::Action => {
                let _ = self.activate_menus(it) || self.activate_measure(it) || self.activate_more(it);
            }
        }
    }

    /// `activate` of an action: the start menu, its key functions and the pickers. True when `it` is one of them.
    fn activate_menus(&mut self, it: Item) -> bool {
        match it {
            Item::Open(MenuId::La) if !self.m.la.on && !self.la_probe_ok() => self.toast("No LA probe connected"),
            // start menu > Measure: the measurement page, as the Measure button and key open it
            Item::Open(MenuId::Measure) if self.menu == MenuId::Main => {
                self.ensure_meas_own();
                self.open_submenu(MenuId::MeasAdd);
                // Studio: the Menu opens the page itself; Add > Measurement and the key open the quick Add panel
                self.studio.meas_page = self.m.layout == Layout::Studio;
            }
            // the footer of the Measure sheet: the same list, with a source that is on the screen
            Item::Open(MenuId::MeasAdd) => {
                self.ensure_meas_own();
                self.open_submenu(MenuId::MeasAdd);
                if self.m.layout == Layout::Studio { self.sheet_add_open(); }
            }
            Item::Open(id) => {
                if id == MenuId::Cursor && self.m.cursor == CursorMode::Off {
                    // like the CURSOR key
                    self.m.cursor = CursorMode::Time;
                }
                self.open_submenu(id);
            }
            // start menu: the front-panel key functions; one-shot actions close the menu, stepping ones keep it
            Item::RunStop | Item::SingleAct | Item::AutosetAct | Item::DefaultAct | Item::ForceAct | Item::LockAct => {
                self.close_menu();
                self.key(match it {
                    Item::RunStop => key::RUNSTOP,
                    Item::SingleAct => key::SINGLE,
                    Item::AutosetAct => key::AUTO,
                    Item::DefaultAct => key::DEFAULT,
                    Item::ForceAct => key::FORCE,
                    _ => key::TOUCHLOCK,
                });
            }
            Item::SlopeAct => self.key(key::SLOPE),
            Item::NavHist => self.key(key::NAVIGATE),
            Item::NavBack => self.key(0x08),
            Item::NavFwd => self.key(0x06),
            Item::QuickRun => {
                self.close_menu();
                self.quick();
            }
            Item::PowerAct => {
                // the dialog of the POWER key, without the MCU answer that belongs to a key press
                self.close_menu();
                self.dialog = None;
                self.power.open = true;
            }
            Item::HZero => {
                self.m.delay = 0.0;
                self.settings_changed();
            }
            Item::AutoDeskew => self.auto_deskew(),
            Item::ClearDeskew => self.clear_deskew(),
            Item::Trig50 => self.level_50(),
            Item::PickOpt(k) if self.pick.is_some_and(|p| Self::hex_item(p.it)) => {
                // keypad: digits edit the entry, OK sets the value and goes back
                match KEYPAD.get(k).copied() {
                    Some("OK") => {
                        let v = u64::from_str_radix(if self.keypad.is_empty() { "0" } else { &self.keypad }, 16)
                            .unwrap_or(0)
                            .min(BUS_VAL_MAX);
                        if let Some(p) = self.pick {
                            self.set_hex(p.it, v);
                        }
                        self.close_picker();
                    }
                    Some("Clear") => self.keypad.clear(),
                    Some("Del") => {
                        self.keypad.pop();
                    }
                    Some(d) if self.keypad.len() < 10 => {
                        if self.keypad == "0" {
                            self.keypad.clear();
                        }
                        self.keypad.push_str(d);
                    }
                    _ => {}
                }
            }
            Item::PickOpt(k) if self.pick.is_some_and(|p| p.it == Item::AfgFreqEntry) => self.freq_key(k),
            Item::PickOpt(k) if self.pick.is_some_and(|p| p.it == Item::NumEntry) => self.row_pad_key(k),
            // a picker its page answers for (ADR 0060)
            Item::PickOpt(k) if self.pick.is_some_and(|p| Self::page_of_pick(p.it).is_some()) => {
                if let Some((it, page)) = self.pick.and_then(|p| Some((p.it, Self::page_of_pick(p.it)?))) {
                    self.close_picker();
                    (page.pick)(self, it, k);
                }
            }
            Item::PickOpt(k) if self.pick.is_some_and(|p| p.it == Item::StoreFile) => self.file_pick(k),
            Item::PickOpt(k) if self.pick.is_some_and(|p| p.it == Item::StoreRunScpi) => {
                let p = self.scpi_files.get(k).cloned();
                self.close_picker();
                self.scpi_replay_file = p;
            }
            Item::PickOpt(k) if self.pick.is_some_and(|p| p.it == Item::RefImport) => {
                let p = self.ref_page.files.get(k).cloned();
                self.close_picker();
                if let Some(p) = p {
                    self.ref_load_path(&p);
                }
            }
            Item::PickOpt(k) => {
                if let Some(p) = self.pick {
                    let opts = self.options(p.it);
                    self.close_picker();
                    if let Some(o) = opts.get(k) {
                        self.select_option(p.it, o);
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// `activate` of an action: the measurements, math, references. True when `it` is one of them.
    fn activate_measure(&mut self, it: Item) -> bool {
        match it {
            Item::MeasCfg(i) => self.open_meas_cfg(i as usize),
            Item::MeasAdd(k) => {
                let e = (k, self.sources.a);
                if let Some(index) = self.m.meas.iter().position(|item| *item == e) {
                    self.m.meas.remove(index);
                    self.meas_own.rows.remove(&e);
                    if self.meas_own.dialog == Some(index as u16) {
                        self.meas_own.dialog = None;
                    } else if self.meas_own.dialog.is_some_and(|n| n as usize > index) {
                        self.meas_own.dialog = self.meas_own.dialog.map(|n| n - 1);
                    }
                    self.clear_measurement_statistics();
                    self.toast(&format!(
                        "{} {} removed",
                        meascatalog::meas_display_name(k),
                        self.wave_name(self.sources.a)
                    ));
                    return true;
                } else {
                    if !self.meas_make_room() {
                        return true;
                    }
                    self.m.meas.push(e);
                    self.clear_measurement_statistics();
                }
                if k == Meas::Burst && !(self.m.meas_idle > 0.0) {
                    if let Some((a, _)) = self.meas_analysis(self.sources.a) {
                        let p = a.period();
                        if p.is_finite() && p > 0.0 {
                            self.m.meas_idle = p;
                        }
                    }
                }
                self.ensure_meas_own();
                self.toast(&format!("{} {} added", meascatalog::meas_display_name(k), self.wave_name(self.sources.a)));
            }
            Item::MeasPair(k) => self.meas_pair_toggle(k),
            Item::CntClear => {
                if self.cnt.total == 0 && self.cnt.hw.is_none() {
                    self.toast(NOTHING_TO_CLEAR);
                } else {
                    self.counter_clear();
                    self.toast("Counter cleared");
                }
            }
            Item::DecTable => self.decode_table_toggle(),
            Item::DecThrMid => self.dec_threshold_to_middle(),
            Item::DecExport => self.decode_export(),
            Item::SearchTable => self.table_toggle(tables::TableKind::Search),
            Item::SearchSave => self.events_export(tables::TableKind::Search),
            Item::StoreBin => self.file_save_ask(FileKind::WaveBin),
            Item::RefExport => self.ref_export(),
            Item::RefImport if self.sheets_on() => self.studio_load_file(self.ref_page.sel),
            Item::RefImport => self.ref_import(),
            Item::RefLoad => self.ref_load(),
            Item::MaskSave => self.mask_save(),
            Item::MaskLoad => self.mask_load(),
            Item::ChLabel(c) => self.wlan.osk = Some(wlan::Osk::for_label(c, &self.m.ch_label[c])),
            Item::BusLabel => self.wlan.osk = Some(wlan::Osk::for_bus(&self.m.bus.label)),
            Item::DecLabel => self.wlan.osk = Some(wlan::Osk::for_decoder(&self.m.dec_label)),
            Item::RefLabel => match &self.refs[self.ref_page.sel] {
                Some(r) => self.wlan.osk = Some(wlan::Osk::for_ref(self.ref_page.sel, &r.label)),
                None => self.toast("That slot is empty"),
            },
            Item::ChBiasZero(c) => self.zero_input(c),
            Item::MathExprEdit => {
                self.wlan.osk = Some(wlan::Osk::for_expr(self.m.math_sel, &self.m.math_expr[self.m.math_sel]))
            }
            Item::MeasRemove(index) => self.remove_measurement(index as usize),
            Item::MeasClear => {
                self.m.meas.clear();
                self.m.meas2.clear();
                self.m.cat.clear();
                self.meas_own.rows.clear();
                self.meas_own.dialog = None;
                self.clear_measurement_statistics();
            }
            Item::RefSave | Item::RefSaveNext => {
                let c = self.ref_page.src;
                let rec = self.last[0].clone();
                let saved = rec.as_ref().filter(|r| !r.ch[c].is_empty()).map(|r| {
                    let (v, dt) = self.meas_wave(r, c);
                    self.ref_of_channel(r, c, v, dt)
                });
                match saved {
                    Some(w) => {
                        let next = self.refs.iter().position(|r| r.is_none()).unwrap_or(NREF - 1);
                        // Studio: into the slot the picker shows; the secondary action goes to the next free one
                        let slot = if self.sheets_on() && it != Item::RefSaveNext { self.ref_page.sel } else { next };
                        self.toast(&format!("CH{} saved to R{}", c + 1, slot + 1));
                        self.ref_put(slot, w);
                    }
                    None => self.toast("Source channel has no data"),
                }
            }
            Item::RefClearOne => self.ref_clear_slot(self.ref_page.sel),
            Item::RefClear => self.ref_clear_all(),
            _ => return false,
        }
        true
    }

    /// `activate` of an action: display, storage, zones, mask, system and network. True when `it` is one of them.
    fn activate_more(&mut self, it: Item) -> bool {
        match it {
            Item::ClearDisp => {
                if self.menu == MenuId::Main {
                    self.close_menu();
                }
                self.clear_display();
            }
            Item::SysInfo => self.sysinfo_open(),
            Item::ViewExit => self.view_exit(),
            Item::ViewSettings => self.view_settings(),
            Item::About => {
                let p = oos_hal::model::platform();
                let build = std::fs::read_to_string("/etc/oos-build").unwrap_or_default();
                let mut d = vec![
                    "OOS".into(),
                    "An open operating system for the DHO800/DHO900".into(),
                    if build.is_empty() {
                        format!("oos-ui {}", env!("CARGO_PKG_VERSION"))
                    } else {
                        format!("Build {}", oos_hal::update::short(build.trim()))
                    },
                    // the commit this binary was built from: the same line on the scope and on a desktop
                    option_env!("OOS_GIT").map_or(String::new(), |g| format!("Commit {g}")),
                    self.info.clone(),
                    format!("Rigol {}", p.ident()),
                    p.caps_line(),
                ];
                // experimental limits (limits.conf) and detection problems: the details are on System info
                if p.experimental() {
                    d.push(format!("EXPERIMENTAL limits ({}), see System info", p.overrides.len()));
                }
                if !p.warnings.is_empty() {
                    d.push(format!("{} warning(s), see System info", p.warnings.len()));
                }
                d.extend([
                    format!("Source: {}   {:.0} fps", self.src.name(), self.counts.fps),
                    self.start_up_summary().unwrap_or_default(),
                    "Tap to close".into(),
                ]);
                d.retain(|l| !l.is_empty());
                self.dialog = Some(d)
            }
            Item::Shot => {
                self.rec_action(":SAVE:IMAGe");
                // without the dialog on it
                self.close_menu();
                self.asks.shot = Some(String::new());
            }
            Item::MathAuto => self.math_autoscale(),
            Item::MathAddFilter => self.math_add_filter(),
            Item::MathToRef => self.math_to_ref(),
            Item::MathOff => self.math_turn_off(self.m.math_sel),
            Item::ZoneBox | Item::ZoneShape => {
                self.zone_trig.arm = Some((self.zone_trig.sel, it == Item::ZoneShape));
                self.close_menu();
                self.toast(if it == Item::ZoneShape {
                    "Draw the zone with one finger"
                } else {
                    "Drag a box for the zone"
                });
            }
            Item::ZoneClear => {
                let blank = crate::zone::Zone { src: self.m.zones[self.zone_trig.sel].src, ..Default::default() };
                if self.m.zones[self.zone_trig.sel] == blank {
                    self.toast(NOTHING_TO_CLEAR);
                } else {
                    self.m.zones[self.zone_trig.sel] = blank;
                    self.toast(&format!("Zone {} deleted", self.zone_trig.sel + 1));
                }
            }
            Item::StoreSaveSetup => {
                // for the SCPI recorder: the slot is the menu's and no part of the set-up, so the command names
                // it. `:SAVE:SETup` would write the slot the scope that runs the recording happens to stand at
                self.rec_action(&format!("*SAV {}", self.files.slot));
                let p = self.setup_path();
                let r = crate::store::write(&p, self.setup_text_full().as_bytes());
                self.toast_saved(&p, r);
            }
            // the rows below ask for the name of their file, or which file to load (ui/files.rs); the slots do not
            Item::StoreSaveScpi => self.file_save_ask(FileKind::SetupScpi),
            Item::StoreRunScpi => self.scpi_run_ask(),
            Item::StoreRecall => self.recall_setup(),
            Item::StoreSetupSave => self.file_save_ask(FileKind::Setup),
            Item::StoreSetupLoad => self.file_load_ask(FileKind::Setup),
            Item::StoreCsv => self.file_save_ask(FileKind::WaveCsv),
            Item::StorePng => self.file_save_ask(FileKind::Shot),
            Item::MaskCreate => self.create_mask(),
            Item::MaskFromRef => self.create_mask_from_ref(),
            Item::MaskClear => {
                if self.mask_sw.mask.is_none() && self.mask_sw.reference.is_none() && !self.m.mask_on && self.mask_sw.pass == 0 && self.mask_sw.fail == 0 {
                    self.toast(NOTHING_TO_CLEAR);
                } else {
                    self.mask_sw.mask = None;
                    self.mask_sw.reference = None;
                    self.m.mask_on = false;
                    self.mask_counts_reset();
                    self.toast("Mask cleared");
                }
            }
            Item::MaskReset => {
                if self.mask_sw.pass == 0 && self.mask_sw.fail == 0 {
                    self.toast(NOTHING_TO_CLEAR);
                } else {
                    self.mask_counts_reset();
                    self.toast("Counts reset");
                }
            }
            Item::StoreFfSave => self.file_save_ask(FileKind::FastFrame),
            Item::StoreFfLoad => self.file_load_ask(FileKind::FastFrame),
            Item::StoreSessSave => self.file_save_ask(FileKind::Session),
            Item::StoreSessLoad => self.file_load_ask(FileKind::Session),
            Item::Stock => self.stock_tap(),
            Item::BootAgain => self.boot_again_tap(),
            Item::Update => self.dialog = Some(self.update_lines()),
            Item::AcqSegTable => self.table_toggle(tables::TableKind::Segments),
            Item::SrcOn => {
                let v = self.off_sources(self.menu);
                for &c in &v {
                    self.m.ch[c].on = true;
                }
                self.focus = 0;
                self.settings_changed();
                let names: Vec<String> = v.iter().map(|c| format!("CH{}", c + 1)).collect();
                self.toast(&format!("{} on", names.join(", ")));
            }
            Item::WlanInfo => self.dialog = Some(self.wlan_lines()),
            Item::WlanScan => self.wlan_scan(),
            Item::WlanForget => self.wlan_forget_tap(),
            Item::UpdateRollback => self.update_rollback_tap(),
            _ => return false,
        }
        true
    }
}

/// Text on a dark backing, for labels drawn over the waveforms; returns the x after the text.
/// `fine_time`: next fine time base from `now`, by `d` hundredths on three digits, as (step, factor). The step is the
/// one at or above the value; from the roll time base on the screen is the record, so there only the steps remain.
fn fine_time(now: f32, d: i32) -> (usize, f32) {
    let ts = tscales();
    let raw = now as f64 * (1.0 + 0.01 * d as f64);
    let q = 10f64.powf(raw.max(1e-12).log10().floor() - 2.0);
    let want = (((raw / q).round() * q) as f32).clamp(ts[0], ts[ts.len() - 1]);
    let i = ts.iter().position(|&t| t >= want * 0.99999).unwrap_or(ts.len() - 1);
    if ts[i] as f64 >= board().roll_tdiv() * 0.999 {
        let at = ts.iter().position(|&t| t >= now * 0.99999).unwrap_or(ts.len() - 1) as i32;
        return ((at + d.signum()).clamp(0, ts.len() as i32 - 1) as usize, 1.0);
    }
    let f = want / ts[i];
    (i, if (f - 1.0).abs() < 1e-5 { 1.0 } else { f })
}
