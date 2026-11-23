// SPDX-License-Identifier: GPL-2.0-or-later
//! The number pads, one per layout family.
//!
//! Default: upstream's pad (issue #22). A tap on the middle of a value row whose number can be typed (the scale and
//! offset of a channel, the time base and its position, the trigger level) opens a picker of keys: the digits, the
//! point, the sign, and a key for each prefix of the row's unit (`nV` ... `kV`, `ns` ... `ks`), which ends the entry
//! and sets the row. The value is taken as SCPI takes it (`:CHANnel<n>:SCALe`, `:TIMebase:SCALe`, ...): onto the
//! nearest step of a ladder, the fine scale onto its calibrated row. Its state is `Ui::row_pad`, its text `Ui::keypad`.
//!
//! Minimal and Studio: the numeric entry pad of the Studio sheets: a tap on the value of a "- value +" row opens it.
//! The typed text and the unit are in the field at the top (the current value is selected: the first key replaces
//! it); the digits, the sign, Exp, Bksp and Clear edit it; a prefix key (G M k m u n p) or None ends the entry with
//! that prefix; Min and Max set the limits of the item; Enter parses and sets the value, Cancel closes with no change.
//! The text is `Ui::keypad`, as for the other keypads. The value goes in through the stepper's own path (`adjust`), or
//! straight into the setting where the stepper's grid is coarser than what can be typed (the trigger level).
use super::*;

/// Default's pad (upstream's): the row it enters into, while it is open.
#[derive(Default)]
pub(super) struct RowPad {
    pub target: Option<Item>,
}

/// The keys of Default's pad, four in a row; a prefix key (`n`, `u`, `m`, `1`, `k`) is shown with the row's unit and
/// ends the entry.
const ROW_PAD_KEYS: [&str; 20] =
    ["7", "8", "9", "n", "4", "5", "6", "u", "1", "2", "3", "m", "0", ".", "-", "1x", "Del", "Clear", "Cancel", "k"];

/// The factor and the shown prefix of a prefix key of Default's pad, from `SI_PREFIXES`.
fn row_pad_prefix(key: &str) -> Option<(f64, &'static str)> {
    let shown = match key {
        "n" | "m" | "k" => key,
        "u" => "\u{b5}",
        "1x" => "",
        _ => return None,
    };
    SI_PREFIXES.iter().copied().find(|&(_, p)| p == shown)
}

/// Longest entry of Default's pad, in characters.
const ROW_PAD_MAX: usize = 12;

/// The pad.
#[derive(Clone, Debug)]
pub(super) struct NumPad {
    pub it: Item,
    /// the text is the current value and the first key replaces it
    pub fresh: bool,
    /// the unit of the item without a prefix: "V", "s", "Hz"
    pub unit: String,
}

/// The keys of the pad, in the order of `numpad_geo`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum NumKey {
    Digit(u8),
    Dot,
    Sign,
    Exp,
    Bksp,
    Clear,
    /// a prefix with its power of ten
    Prefix(i32),
    Min,
    Max,
    Cancel,
    Enter,
}

pub(super) const PAD_W: i32 = 484;
pub(super) const PAD_H: i32 = 352;
const KEY_H: i32 = 48;

/// The SI prefixes of the pad: label and power of ten ("None" is 0).
pub(super) const PREFIXES: [(&str, i32); 8] = [("G", 9), ("M", 6), ("k", 3), ("m", -3), ("\u{b5}", -6), ("n", -9), ("p", -12), ("None", 0)];

/// A displayed value split into its number (in base units) and its unit without the prefix: "449 mV" is (0.449, "V").
pub(super) fn split_value(s: &str) -> Option<(f64, String)> {
    let s = s.trim();
    let end = s.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || ((c == '-' || c == '+') && i == 0))).map_or(s.len(), |(i, _)| i);
    let num: f64 = s[..end].parse().ok()?;
    let rest = s[end..].trim();
    let mut cs = rest.chars();
    let (factor, unit) = match cs.next() {
        Some(p) if cs.as_str().chars().next().is_some_and(|c| c.is_alphabetic() || c == '%') => {
            let f = match p { 'G' => 1e9, 'M' => 1e6, 'k' => 1e3, 'm' => 1e-3, '\u{b5}' | 'u' => 1e-6, 'n' => 1e-9, 'p' => 1e-12, _ => 0.0 };
            if f != 0.0 { (f, cs.as_str().to_string()) } else { (1.0, rest.to_string()) }
        }
        _ => (1.0, rest.to_string()),
    };
    Some((num * factor, unit))
}

/// A number as typed: no exponent notation, no trailing zeros.
fn plain(v: f64) -> String {
    let mut s = format!("{v:.9}");
    if s.contains('.') { s = s.trim_end_matches('0').trim_end_matches('.').to_string(); }
    if s == "-0" || s.is_empty() { s = "0".into(); }
    s
}

impl Ui {
    /// The value of an item as a number in base units, and its unit.
    pub(super) fn numpad_value(&self, it: Item) -> Option<(f64, String)> {
        split_value(&self.value(it))
    }

    /// Opens the pad on a "- value +" item. False when its value is not a number.
    pub(super) fn open_numpad(&mut self, it: Item) -> bool {
        if self.item_blocked_toast(it) { return true; }
        let Some((v, unit)) = self.numpad_value(it) else { return false };
        self.keypad = plain(v);
        self.numpad = Some(NumPad { it, fresh: true, unit });
        true
    }

    /// The limits of an item: its own from the range table, or where the stepper stops.
    pub(super) fn numpad_limits(&mut self, it: Item) -> Option<(f64, f64)> {
        if let Some(r) = self.item_range(it) {
            return Some((r.lo, r.hi));
        }
        let (cur, _) = self.numpad_value(it)?;
        let saved = self.m.clone();
        let mut ends = [cur, cur];
        for (k, dir) in [-1, 1].into_iter().enumerate() {
            let mut last = cur;
            for _ in 0..8 {
                self.adjust(it, dir * 512);
                let Some((v, _)) = self.numpad_value(it) else { break };
                if (v - last).abs() <= last.abs() * 1e-9 { break; }
                last = v;
            }
            ends[k] = last;
            self.m = saved.clone();
        }
        Some((ends[0].min(ends[1]), ends[0].max(ends[1])))
    }

    /// Sets an item to `want` (clamped to its limits) and returns the value it has now.
    pub(super) fn numpad_apply(&mut self, it: Item, want: f64) -> f64 {
        let limits = self.numpad_limits(it);
        let want = limits.map_or(want, |(lo, hi)| want.clamp(lo, hi));
        // the items that add a step: straight into the setting, through the same setter as the stepper and SCPI
        if let Some(v) = self.range_set(it, want) {
            if it == Item::SpecRef {
                self.azr.spec_avg = None;
                self.analyzer_update();
            }
            self.settings_changed();
            return v;
        }
        // the stepper's own path: big steps first, then smaller ones, as long as the value does not pass the wanted one
        let mut cur = self.numpad_value(it).map_or(want, |v| v.0);
        for d in [1024, 256, 64, 16, 4, 1] {
            for _ in 0..80 {
                let dir = if want > cur { 1 } else { -1 };
                let before = self.m.clone();
                self.adjust(it, dir * d);
                let Some((nv, _)) = self.numpad_value(it) else { break };
                if (nv - cur).abs() <= cur.abs() * 1e-12 { break; }
                let past = (dir > 0 && nv > want) || (dir < 0 && nv < want);
                if past && !(d == 1 && (nv - want).abs() < (cur - want).abs()) {
                    self.m = before;
                    break;
                }
                cur = nv;
                if past { break; }
            }
        }
        self.settings_changed();
        self.numpad_value(it).map_or(want, |v| v.0)
    }

    /// Enters `v` (base units): sets the item, closes the pad, and says so when the value had to be limited.
    fn numpad_enter(&mut self, it: Item, v: f64) {
        let (lo, hi) = self.numpad_limits(it).unwrap_or((f64::MIN, f64::MAX));
        let clamped = v < lo || v > hi;
        self.numpad_apply(it, v);
        self.numpad = None;
        if clamped { let s = self.value(it); self.toast(&format!("Limited to {s}")); }
    }

    // ------------------------------------------------------------------------------------------------ geometry

    pub(super) fn numpad_rect(&self) -> Rect {
        self.scr.rect().center(PAD_W, PAD_H)
    }

    /// The entry field, and the keys with their rectangles.
    pub(super) fn numpad_geo(&self) -> (Rect, Vec<(NumKey, Rect)>) {
        let p = self.numpad_rect();
        let field = Rect::new(p.x + 12, p.y + 12, p.w - 24, 56);
        let (x0, y0, kw, gap) = (p.x + 12, p.y + 76, 76, 4);
        let col = |c: i32| x0 + c * (kw + gap);
        let row = |r: i32| y0 + r * (KEY_H + gap);
        let mut keys = Vec::new();
        let layout = [
            [NumKey::Digit(7), NumKey::Digit(8), NumKey::Digit(9), NumKey::Bksp],
            [NumKey::Digit(4), NumKey::Digit(5), NumKey::Digit(6), NumKey::Clear],
            [NumKey::Digit(1), NumKey::Digit(2), NumKey::Digit(3), NumKey::Sign],
        ];
        for (r, line) in layout.iter().enumerate() {
            for (c, k) in line.iter().enumerate() { keys.push((*k, Rect::new(col(c as i32), row(r as i32), kw, KEY_H))); }
        }
        keys.push((NumKey::Digit(0), Rect::new(col(0), row(3), kw * 2 + gap, KEY_H)));
        keys.push((NumKey::Dot, Rect::new(col(2), row(3), kw, KEY_H)));
        keys.push((NumKey::Exp, Rect::new(col(3), row(3), kw, KEY_H)));
        // the prefixes: two columns at the right
        let px = col(4) + 8;
        for (i, (_, e)) in PREFIXES.iter().enumerate() {
            keys.push((NumKey::Prefix(*e), Rect::new(px + (i as i32 % 2) * 64, row(i as i32 / 2), 60, KEY_H)));
        }
        // the bottom row: Min, Max, Cancel, Enter
        let by = row(4) + 4;
        let bw = (p.w - 24 - 3 * gap) / 4;
        for (i, k) in [NumKey::Min, NumKey::Max, NumKey::Cancel, NumKey::Enter].into_iter().enumerate() {
            keys.push((k, Rect::new(x0 + i as i32 * (bw + gap), by, bw, KEY_H)));
        }
        (field, keys)
    }

    pub(super) fn numpad_label(k: NumKey) -> String {
        match k {
            NumKey::Digit(d) => d.to_string(),
            NumKey::Dot => ".".into(),
            NumKey::Sign => "\u{b1}".into(),
            NumKey::Exp => "Exp".into(),
            NumKey::Bksp => "Bksp".into(),
            NumKey::Clear => "Clear".into(),
            NumKey::Prefix(e) => PREFIXES.iter().find(|p| p.1 == e).map_or("", |p| p.0).to_string(),
            NumKey::Min => "Min".into(),
            NumKey::Max => "Max".into(),
            NumKey::Cancel => "Cancel".into(),
            NumKey::Enter => "Enter".into(),
        }
    }

    // ------------------------------------------------------------------------------------------------ keys

    /// The typed text as a number (None while it is not one).
    pub(super) fn numpad_typed(&self) -> Option<f64> {
        self.keypad.trim().parse::<f64>().ok().filter(|v| v.is_finite())
    }

    pub(super) fn numpad_key(&mut self, k: NumKey) {
        let Some(pad) = self.numpad.clone() else { return };
        match k {
            NumKey::Digit(d) => {
                if pad.fresh { self.keypad.clear(); }
                if self.keypad == "0" { self.keypad.clear(); }
                if self.keypad == "-0" { self.keypad = "-".into(); }
                if self.keypad.len() < 14 { self.keypad.push((b'0' + d) as char); }
            }
            NumKey::Dot => {
                if pad.fresh { self.keypad.clear(); }
                let tail = self.keypad.rsplit(['e', 'E']).next().unwrap_or("");
                if !self.keypad.contains(['e', 'E']) && !tail.contains('.') {
                    if self.keypad.is_empty() || self.keypad == "-" { self.keypad.push('0'); }
                    self.keypad.push('.');
                }
            }
            NumKey::Sign => {
                if let Some(p) = self.keypad.find(['e', 'E']) {
                    // the sign of the exponent
                    if self.keypad[p + 1..].starts_with('-') { self.keypad.remove(p + 1); } else { self.keypad.insert(p + 1, '-'); }
                } else if self.keypad.starts_with('-') { self.keypad.remove(0); } else { self.keypad.insert(0, '-'); }
            }
            NumKey::Exp => {
                if pad.fresh { self.keypad.clear(); }
                if !self.keypad.contains(['e', 'E']) && !self.keypad.is_empty() && self.keypad != "-" { self.keypad.push('e'); }
            }
            NumKey::Bksp => {
                if pad.fresh { self.keypad.clear(); } else { self.keypad.pop(); }
            }
            NumKey::Clear => self.keypad.clear(),
            NumKey::Prefix(e) => {
                if let Some(v) = self.numpad_typed() { self.numpad_enter(pad.it, v * 10f64.powi(e)); }
                return;
            }
            NumKey::Min | NumKey::Max => {
                if let Some((lo, hi)) = self.numpad_limits(pad.it) { self.numpad_enter(pad.it, if k == NumKey::Min { lo } else { hi }); }
                return;
            }
            NumKey::Cancel => {
                self.numpad = None;
                return;
            }
            NumKey::Enter => {
                match self.numpad_typed() {
                    Some(v) => self.numpad_enter(pad.it, v),
                    None => self.toast("Enter a number"),
                }
                return;
            }
        }
        if let Some(p) = &mut self.numpad { p.fresh = false; }
    }

    /// A tap while the pad is open: a key, or outside it (nothing: the pad is modal). Always taken.
    pub(super) fn numpad_tap(&mut self, x: i32, y: i32) -> bool {
        if self.numpad.is_none() { return false; }
        let (_, keys) = self.numpad_geo();
        if let Some(&(k, _)) = keys.iter().find(|(_, r)| Self::hit40(*r).contains(x, y) && r.contains(x, y)) {
            self.numpad_key(k);
        }
        true
    }

    // ------------------------------------------------------------------------------------------------ drawing

    pub(super) fn draw_numpad(&self, cv: &mut Canvas) {
        let Some(pad) = &self.numpad else { return };
        let f = self.fonts();
        let p = self.numpad_rect();
        cv.blend(self.scr.rect(), BG, 120);
        cv.round(p, 10, ACCENT, 255);
        cv.round(p.inset(2), 9, DLG_BG, 255);
        let (field, keys) = self.numpad_geo();
        cv.round(field, 6, BTN_BORDER, 255);
        cv.round(field.inset(1), 5, BG, 255);
        // the name of the item, small, over the typed text and the unit
        cv.text(&f.s, field.x + 10, field.y + 16, &self.item_label(pad.it), DIM);
        let text = if self.keypad.is_empty() { "0".to_string() } else { self.keypad.clone() };
        let sel = pad.fresh;
        let tw = f.lb.width(&text);
        if sel { cv.fill(Rect::new(field.x + 8, field.y + 21, tw + 4, 30), mix(BG, ACCENT, 90)); }
        cv.text(&f.lb, field.x + 10, field.y + 44, &text, if sel { DARK_TEXT } else { TEXT });
        cv.text(&f.mb, field.x + 16 + tw, field.y + 44, &pad.unit, DIM);
        let (lo, hi) = (self.numpad_limit_text(pad.it, false), self.numpad_limit_text(pad.it, true));
        if let (Some(lo), Some(hi)) = (lo, hi) {
            cv.text_in(&f.s, Rect::new(field.x, field.y + 4, field.w - 10, 20), 2, &format!("{lo} to {hi}"), DIM);
        }
        for (k, r) in keys {
            let label = Self::numpad_label(k);
            let (fill, border, ink) = match k {
                NumKey::Enter => (ACCENT, ACCENT, DARK_TEXT),
                NumKey::Cancel => (mix(DLG_BG, RED, 30), RED, RED),
                NumKey::Prefix(_) | NumKey::Min | NumKey::Max | NumKey::Exp => (mix(DLG_BG, ACCENT, 25), BTN_BORDER, TEXT),
                _ => (BTN, BTN_BORDER, TEXT),
            };
            btn(cv, r, fill, border);
            cv.text_in(&f.mb, r, 1, &label, ink);
        }
    }

    /// The limit of an item as text (for the field): from the range table, for the items that have one.
    fn numpad_limit_text(&self, it: Item, high: bool) -> Option<String> {
        let r = self.item_range(it)?;
        let unit = self.numpad_value(it).map(|v| v.1).unwrap_or_default();
        Some(si(if high { r.hi } else { r.lo }, &unit))
    }

    // ------------------------------------------------------------------------------------------------ Default's pad

    /// The unit of a row whose number can be typed in Default's pad, or none.
    pub(super) fn row_pad_unit(&self, it: Item) -> Option<&'static str> {
        let m = &self.m;
        match it {
            Item::ChScale(c) | Item::ChOffset(c) if c < NCH => Some(m.unit(c)),
            Item::HScale | Item::HDelay => Some("s"),
            Item::TrigLevel if m.trig_src < NCH => Some(m.unit(m.trig_src)),
            // any amplitude of the sweep, not only the knob's steps, in the unit of the menu (#52)
            Item::BodeAmp if m.bode_rms => Some("Vrms"),
            Item::BodeAmp => Some("Vpp"),
            _ => None,
        }
    }

    /// Opens Default's pad for row `it`, with an empty entry.
    pub(super) fn open_row_pad(&mut self, it: Item) {
        self.row_pad.target = Some(it);
        self.keypad.clear();
        self.open_picker(Item::NumEntry);
    }

    /// The keys of Default's pad, the prefix keys with the unit of its row.
    pub(super) fn row_pad_keys(&self) -> Vec<String> {
        let unit = self.row_pad.target.and_then(|it| self.row_pad_unit(it)).unwrap_or("");
        ROW_PAD_KEYS
            .iter()
            .map(|&k| match row_pad_prefix(k) {
                Some((_, p)) => format!("{}{}", p, unit),
                None => k.to_string(),
            })
            .collect()
    }

    /// An entry pad that is open (the number pad, the generator's frequency, a hex value of a bus trigger): its title
    /// and the entry so far, which shows large above the keys. The pads open centred and larger (ADR 0077).
    pub(super) fn pad_entry(&self) -> Option<(String, String)> {
        let p = self.pick.filter(|_| self.menu_open && self.menu == MenuId::Pick)?;
        if Self::hex_item(p.it) {
            return Some((self.item_label(p.it).to_string(), format!("0x{}_", self.keypad)));
        }
        let title = match p.it {
            Item::NumEntry => self.row_pad.target.map_or(String::new(), |it| self.item_label(it).to_string()),
            Item::AfgFreqEntry => format!("Frequency (max {})", si_short(self.m.gen_max_hz(), "Hz")),
            _ => return None,
        };
        Some((title, format!("{}_", self.keypad)))
    }

    /// The value the row of the number pad (or the generator's frequency) has now, shown dim in its field until
    /// something is typed.
    pub(super) fn pad_now(&self) -> Option<String> {
        if !self.keypad.is_empty() || self.pad_entry().is_none() {
            return None;
        }
        let it = match self.pick?.it {
            Item::NumEntry => self.row_pad.target?,
            Item::AfgFreqEntry => Item::AfgFreq,
            _ => return None,
        };
        Some(self.value(it))
    }

    /// Key `k` of Default's pad was tapped.
    pub(super) fn row_pad_key(&mut self, k: usize) {
        let Some(&key) = ROW_PAD_KEYS.get(k) else { return };
        if let Some((factor, _)) = row_pad_prefix(key) {
            let typed = crate::words::num64(&self.keypad);
            let target = self.row_pad.target.take();
            self.close_picker();
            if let (Some(v), Some(it)) = (typed, target) {
                self.row_pad_set(it, v * factor);
            }
            return;
        }
        match key {
            "Cancel" => {
                self.row_pad.target = None;
                self.close_picker();
            }
            "Clear" => self.keypad.clear(),
            "Del" => {
                self.keypad.pop();
            }
            "." if self.keypad.contains('.') => {}
            "-" if !self.keypad.is_empty() => {}
            d if self.keypad.chars().count() < ROW_PAD_MAX => self.keypad.push_str(d),
            _ => {}
        }
    }

    /// Sets row `it` to `v` (in its unit), as SCPI would.
    fn row_pad_set(&mut self, it: Item, v: f64) {
        // the amplitude is kept as f64 in Vpp, not rounded to f32 as the rows below, within the generator's range
        if it == Item::BodeAmp {
            use crate::settings::bode::{AMP_RANGE, VPP_PER_VRMS};
            let (k, unit) = if self.m.bode_rms { (VPP_PER_VRMS, "Vrms") } else { (1.0, "Vpp") };
            // within the range; the clamp below only takes up the rounding of Vrms to Vpp at its ends
            let vpp = v * k;
            if !(AMP_RANGE.0 * (1.0 - 1e-9)..=AMP_RANGE.1 * (1.0 + 1e-9)).contains(&vpp) {
                let (lo, hi) = (si_short(AMP_RANGE.0 / k, unit), si_short(AMP_RANGE.1 / k, unit));
                return self.toast(&format!("The amplitude must be {} to {}", lo, hi));
            }
            // as - and + of the Bode rows: no settings_changed (setting_items.rs)
            self.m.bode_amp = vpp.clamp(AMP_RANGE.0, AMP_RANGE.1);
            return;
        }
        let m = &mut self.m;
        let v = v as f32;
        match it {
            Item::ChScale(c) => {
                if v <= 0.0 {
                    return self.toast("The scale must be above 0");
                }
                let old = m.ch[c].volts_per_div();
                let want = v / m.ch[c].probe;
                (m.ch[c].vscale, m.ch[c].vfine) = Self::vscale_for(want, m.fine_v);
                // around ground: the zero line keeps its place on the screen
                if m.v_expand_gnd && old > 0.0 {
                    m.ch[c].offset *= m.ch[c].volts_per_div() / old;
                }
            }
            Item::ChOffset(c) => m.ch[c].offset = v,
            Item::HScale => {
                if v <= 0.0 {
                    return self.toast("The scale must be above 0");
                }
                let old = m.time_per_div();
                (m.tscale, m.tfine) = (nearest_step(&tscales(), v), 1.0);
                m.keep_expand_point(old);
            }
            Item::HDelay => m.delay = v,
            Item::TrigLevel => m.trig_level = v,
            _ => return,
        }
        self.settings_changed();
    }

    pub(super) fn row_pad_kind(it: Item) -> Option<Kind> {
        (it == Item::NumEntry).then_some(Kind::Choice)
    }

    pub(super) fn row_pad_label(it: Item) -> Option<&'static str> {
        (it == Item::NumEntry).then_some("Enter a value")
    }

    /// The `vscale` step and its fine factor for `want` V/div at the BNC: the nearest step of the ladder, or with the
    /// fine scale the first row of the calibrated table at or above it, so the range never cuts the signal off (SCPI's
    /// `:CHANnel<n>:SCALe`).
    pub(crate) fn vscale_for(want: f32, fine: bool) -> (usize, f32) {
        if !fine {
            return (nearest_step(&VSCALES, want), 1.0);
        }
        let rows = board().vscale_steps_nv();
        let nv = want as f64 * 1e9 * (1.0 - 1e-6);
        let row = rows.iter().copied().find(|&k| k as f64 >= nv).unwrap_or(rows[rows.len() - 1]);
        let row = (row as f64 * 1e-9) as f32;
        let i = VSCALES.iter().rposition(|&s| s <= row * 1.000001).unwrap_or(0);
        let f = row / VSCALES[i];
        (i, if (f - 1.0).abs() < 1e-6 { 1.0 } else { f })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(u: &mut Ui, keys: &[&str]) {
        let shown = u.row_pad_keys();
        for k in keys {
            let i = shown.iter().position(|s| s == k).unwrap_or_else(|| panic!("no key {:?} in {:?}", k, shown));
            u.activate(Item::PickOpt(i));
        }
    }

    fn opened(it: Item) -> Ui {
        let mut u = crate::ui::testkit::demo_ui();
        assert_eq!(u.m.layout, Layout::Default);
        u.open_menu(MenuId::Channel(0));
        u.open_row_pad(it);
        assert!(u.menu == MenuId::Pick && u.pick.is_some_and(|p| p.it == Item::NumEntry));
        u
    }

    /// Default: upstream's pad (issue #22); the unit key ends the entry, the value lands where SCPI puts it.
    #[test]
    fn default_a_typed_scale_offset_time_and_level_are_set() {
        let mut u = opened(Item::ChScale(0));
        assert_eq!(u.row_pad_keys()[11], "mV");
        press(&mut u, &["2", "0", "0", "mV"]);
        assert!((u.m.ch[0].volts_per_div() - 0.2).abs() < 1e-6, "{}", u.m.ch[0].volts_per_div());
        assert!(u.menu != MenuId::Pick && u.row_pad.target.is_none(), "the pad closes");
        // between two steps of the ladder: the nearest step, as `:CHANnel1:SCALe 0.23` would
        let mut u = opened(Item::ChScale(0));
        press(&mut u, &["2", "3", "0", "mV"]);
        assert!((u.m.ch[0].volts_per_div() - 0.2).abs() < 1e-6, "{}", u.m.ch[0].volts_per_div());

        let mut u = opened(Item::ChOffset(0));
        press(&mut u, &["-", "1", ".", "5", "V"]);
        assert_eq!(u.m.ch[0].offset, -1.5);

        let mut u = opened(Item::HScale);
        press(&mut u, &["5", "\u{b5}s"]);
        assert!((u.m.time_per_div() - 5e-6).abs() < 1e-12, "{}", u.m.time_per_div());

        let mut u = opened(Item::HDelay);
        press(&mut u, &["1", "2", "ns"]);
        assert!((u.m.delay - 12e-9).abs() < 1e-15);

        let mut u = opened(Item::TrigLevel);
        press(&mut u, &["3", "0", "0", "mV"]);
        assert!((u.m.trig_level - 0.3).abs() < 1e-6);
    }

    // Break it catches (ADR 0077): the pad opened small beside its menu, its entry in the title line
    #[test]
    fn the_pad_opens_centred_with_large_keys_and_the_entry_above_them() {
        let u = opened(Item::ChScale(0));
        let d = u.dlg_layout().expect("the pad is a dialog");
        let centre = d.r.x + d.r.w / 2;
        assert!((centre - u.scr.w / 2).abs() <= 1, "centred: {:?} on {}", d.r, u.scr.w);
        assert!(d.rows.iter().all(|r| r.ctrl.h > DLG_CTRL), "larger keys");
        assert!(d.rows.iter().all(|r| r.ctrl.y >= d.r.y + DLG_HEAD + PAD_ENTRY_H), "the keys under the entry field");
        assert_eq!(u.menu_title(MenuId::Pick), u.item_label(Item::ChScale(0)), "the title is the row");
        assert_eq!(u.pad_now(), Some(u.value(Item::ChScale(0))), "before typing, the field shows the value now");
    }

    #[test]
    fn default_the_entry_keys_and_cancel() {
        let mut u = opened(Item::ChOffset(0));
        let before = u.m.ch[0].offset;
        press(&mut u, &["1", ".", ".", "2", "-", "Del", "3"]);
        assert_eq!(u.keypad, "1.3", "one point, the sign only first, Del takes the last");
        assert_eq!(u.pad_entry().map(|e| e.1), Some("1.3_".to_string()), "the entry shows above the keys");
        assert_eq!(u.pad_now(), None, "the value now shows only while nothing is typed");
        press(&mut u, &["Clear", "Cancel"]);
        assert_eq!(u.m.ch[0].offset, before);
        assert!(u.menu != MenuId::Pick && u.row_pad.target.is_none());
        // nothing typed: the unit key closes and sets nothing; a scale of 0 is refused
        let mut u = opened(Item::ChScale(0));
        let scale = u.m.ch[0].volts_per_div();
        press(&mut u, &["V"]);
        assert_eq!(u.m.ch[0].volts_per_div(), scale);
        let mut u = opened(Item::ChScale(0));
        press(&mut u, &["0", "V"]);
        assert_eq!(u.m.ch[0].volts_per_div(), scale);
        assert_eq!(u.toast.as_ref().map(|t| t.0.as_str()), Some("The scale must be above 0"));
    }

    /// Default: a tap on the middle of a value row with a unit opens upstream's pad; of a value row without one it
    /// does not, and the pad of Minimal and Studio does not open.
    #[test]
    fn default_a_tap_on_the_value_opens_upstreams_pad_only_on_rows_with_a_unit() {
        let mut u = crate::ui::testkit::demo_ui();
        u.open_menu(MenuId::Channel(0));
        let row = |u: &Ui, it: Item| u.dlg_layout().unwrap().rows.into_iter().find(|r| r.it == it).unwrap().ctrl;
        let r = row(&u, Item::ChScale(0));
        u.tap(r.x + r.w / 2, r.y + r.h / 2);
        assert_eq!(u.row_pad.target, Some(Item::ChScale(0)));
        assert!(u.pick.is_some_and(|p| p.it == Item::NumEntry) && u.numpad.is_none());
        // Display > Intensity is a number without a unit: upstream offers no pad there
        let mut u = crate::ui::testkit::demo_ui();
        u.open_menu(MenuId::Display);
        let r = row(&u, Item::Intensity);
        u.tap(r.x + r.w / 2, r.y + r.h / 2);
        assert!(u.row_pad.target.is_none() && u.numpad.is_none() && u.menu == MenuId::Display);
    }

    /// Minimal and Studio keep oos-plus's pad (Exp, Min, Max, G ... p) on their number rows, not upstream's.
    #[test]
    fn minimal_and_studio_keep_their_own_pad() {
        for layout in [Layout::Minimal, Layout::Studio] {
            let mut u = crate::ui::testkit::demo_ui();
            u.m.layout = layout;
            u.relayout();
            u.open_menu(MenuId::Channel(0));
            for _ in 0..3 {
                u.step(0.05);
            }
            let it = Item::ChScale(0);
            let r = if u.sheet_active() {
                u.sheet_geo().and_then(|g| g.rows.into_iter().find(|r| r.it == it).map(|r| r.ctrl))
            } else {
                u.dlg_layout().and_then(|d| d.rows.into_iter().find(|r| r.it == it).map(|r| r.ctrl))
            };
            let r = r.unwrap_or_else(|| panic!("{layout:?}: no scale row"));
            let mid = Ui::value_parts(r).1;
            u.tap(mid.x + mid.w / 2, mid.y + mid.h / 2);
            assert_eq!(u.numpad.as_ref().map(|p| p.it), Some(it), "{layout:?}: oos-plus's pad");
            assert!(u.row_pad.target.is_none() && u.pick.is_none(), "{layout:?}: not upstream's");
        }
    }
}
