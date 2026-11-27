// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! A greeting on 24 to 26 December 2026 and on 1 January 2027: a card over the screen with a short text, snow or a
//! small firework behind it. It comes only with a clock that was set in this boot (NTP or by hand), so a date is never
//! guessed, and only at the start: at the first moment the clock is known, within `WINDOW` seconds. A tap or a key
//! closes it, and it does not come again (`greeting.seen` next to system.conf).

use super::*;
use crate::clock::Local;

/// Seconds after the start in which the greeting may still open.
const WINDOW: f64 = 300.0;
/// Seconds before a tap or a key closes the card: a touch that was already on its way does not.
const SETTLE: f64 = 0.5;
/// Next to system.conf: one line per greeting that was closed.
const SEEN_FILE: &str = "greeting.seen";

const SNOW: Rgb = TEXT;
const SKIN: Rgb = rgb(0xf1, 0xc2, 0x9b);
const CHEEK: Rgb = rgb(0xe8, 0x8a, 0x80);
const HAT: Rgb = RED;
const BEARD: Rgb = ACCENT;
const EYE: Rgb = DARK_TEXT;

const FLAKES: u32 = 140;
const SPARKS: u32 = 40;
/// Seconds between two rockets, of a rocket's rise and of its sparks.
const LAUNCH: f64 = 0.45;
const RISE: f64 = 0.6;
const GLOW: f64 = 1.8;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Occasion {
    Christmas,
    NewYear,
}

impl Occasion {
    fn of(day: &Local) -> Option<Occasion> {
        match (day.y, day.mo, day.d) {
            (2026, 12, 24..=26) => Some(Occasion::Christmas),
            (2027, 1, 1) => Some(Occasion::NewYear),
            _ => None,
        }
    }

    fn key(self) -> &'static str {
        match self {
            Occasion::Christmas => "christmas-2026",
            Occasion::NewYear => "newyear-2027",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Occasion::Christmas => "Merry Christmas!",
            Occasion::NewYear => "Happy New Year 2027!",
        }
    }

    fn text(self) -> &'static str {
        match self {
            Occasion::Christmas => {
                "To everyone who probes, measures, builds and shares: thank you. May your traces be clean, your \
                 triggers stable and your ground loops far away. Wishing you and your loved ones a peaceful \
                 Christmas, good health and much happiness."
            }
            Occasion::NewYear => {
                "Here's to another year of curious minds and open hardware. May your signals rise fast and settle \
                 clean, your ideas find their way from breadboard to reality, and the new year bring you health, \
                 luck and plenty of moments that make you say \"huh, interesting...\""
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Greeting {
    /// decided for this run: opened, or nothing to open
    decided: bool,
    /// the card on the screen and the UI time it opened
    open: Option<(Occasion, f64)>,
}

/// Where the card's parts go.
struct CardLayout {
    card: Rect,
    /// Santa's place (Christmas only)
    art: Option<Rect>,
    title: Rect,
    lines: Vec<String>,
    /// top of the first text line, and the distance between lines
    text_x: i32,
    text_y: i32,
    line_h: i32,
    button: Rect,
}

impl Ui {
    /// Each frame: opens the card at the first moment the clock is known, if that is early enough and the day is one.
    pub(super) fn greeting_tick(&mut self) {
        if self.greeting.decided || !self.clock_known || self.start_up_active() {
            return;
        }
        let day = self.local_now();
        self.greeting_decide(&day);
    }

    fn greeting_decide(&mut self, day: &Local) {
        self.greeting.decided = true;
        if self.now > WINDOW {
            return;
        }
        if let Some(o) = Occasion::of(day).filter(|o| !self.greeting_seen(*o)) {
            self.greeting.open = Some((o, self.now));
            // nothing of the workspace stays over the card or waits for a hold under it
            self.workspace_close_popup();
        }
    }

    fn greeting_seen_path(&self) -> Option<std::path::PathBuf> {
        Some(self.sys_path.as_ref()?.parent()?.join(SEEN_FILE))
    }

    fn greeting_seen(&self, o: Occasion) -> bool {
        self.greeting_seen_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .is_some_and(|t| t.lines().any(|l| l.trim() == o.key()))
    }

    fn greeting_close(&mut self) {
        let Some((o, _)) = self.greeting.open.take() else { return };
        if let Some(p) = self.greeting_seen_path() {
            use std::io::Write;
            let f = std::fs::OpenOptions::new().create(true).append(true).open(&p);
            if let Err(e) = f.and_then(|mut f| writeln!(f, "{}", o.key())) {
                eprintln!("oos-ui: {}: {}", p.display(), e);
            }
        }
    }

    /// Input while the card is up: the power key and its dialog work, anything else closes the card.
    pub(super) fn greeting_input(&mut self, ev: &Ev) -> bool {
        let Some((_, t)) = self.greeting.open else { return false };
        if self.power.open || matches!(ev, Ev::Key(k) if *k == key::POWER) {
            return false;
        }
        let pressed = match ev {
            Ev::Key(k) => *k != 0,
            Ev::Touch { phase, .. } => *phase == Phase::Up,
            Ev::Wheel { .. } => false,
        };
        if pressed && self.now - t >= SETTLE {
            self.greeting_close();
        }
        true
    }

    fn greeting_layout(&self, o: Occasion) -> CardLayout {
        let f = self.fonts();
        let scr = self.scr.rect();
        let pad = 24;
        let w = (scr.w * 3 / 5).clamp(380.min(scr.w - 20), scr.w - 20);
        let art_w = if o == Occasion::Christmas { 110 } else { 0 };
        let text_x = pad + art_w;
        let lines = super::meascatalog::wrap_lines(&f.m, o.text(), w - text_x - pad);
        let line_h = f.m.size + 8;
        let title_h = f.lb.size + 14;
        let button_h = 40;
        let h = (pad + title_h + lines.len() as i32 * line_h + 18 + button_h + pad).min(scr.h - 20);
        let card = scr.center(w, h);
        let art = (art_w > 0).then(|| Rect::new(card.x + 8, card.y + pad, art_w, (h - 2 * pad).min(140)));
        let title = Rect::new(card.x + text_x, card.y + pad, w - text_x - pad, title_h);
        let button = Rect::new(card.x + card.w / 2 - 80, card.y + h - pad - button_h, 160, button_h);
        CardLayout { card, art, title, lines, text_x: card.x + text_x, text_y: card.y + pad + title_h, line_h, button }
    }

    /// The card, with snow or a firework behind it. Over everything but the power dialog.
    pub(super) fn draw_greeting(&self, cv: &mut Canvas) {
        let Some((o, t0)) = self.greeting.open else { return };
        let t = (self.now - t0).max(0.0);
        let scr = self.scr.rect();
        cv.blend(scr, SHADE, 150);
        match o {
            Occasion::Christmas => snow(cv, scr, t),
            Occasion::NewYear => firework(cv, scr, t),
        }
        let f = self.fonts();
        let l = self.greeting_layout(o);
        cv.framed(l.card, 8, 2, ACCENT, 7, DLG_BG);
        if let Some(a) = l.art {
            let bob = (2.0 * (t * 2.0).sin()).round() as i32;
            santa(cv, a.x + a.w / 2, a.y + a.h / 2 + bob);
        }
        cv.text_in(&f.lb, l.title, 0, o.title(), ACCENT);
        for (i, s) in l.lines.iter().enumerate() {
            cv.text(&f.m, l.text_x, l.text_y + i as i32 * l.line_h + f.m.size, s, TEXT);
        }
        btn(cv, l.button, mix(DLG_BG, ACCENT, 40), ACCENT);
        cv.text_in(&f.mb, l.button, 1, "Thanks!", ACCENT);
        if o == Occasion::Christmas {
            // a drift of snow on the card's top edge, thicker towards the corners
            let n = l.card.w / 9;
            for i in 0..=n {
                let x = l.card.x + 6 + i * (l.card.w - 12) / n;
                let edge = (2 * i - n).abs() as f64 / n as f64;
                let rad = 4 + (edge * edge * 5.0 + hash(i as u32 + 7) * 3.0) as i32;
                cv.circle(x, l.card.y + 2, rad, SNOW);
            }
        }
    }
}

/// A number in [0, 1) from `i`, the same each time (no state behind the animation).
fn hash(i: u32) -> f64 {
    let mut x = i.wrapping_mul(0x9e37_79b9) ^ 0x85eb_ca6b;
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x as f64 / 4_294_967_296.0
}

/// Flakes falling over the screen, each at its own speed, size and sway.
fn snow(cv: &mut Canvas, scr: Rect, t: f64) {
    let (w, h) = (scr.w as f64, scr.h as f64 + 12.0);
    for i in 0..FLAKES {
        let r = |k: u32| hash(i * 8 + k);
        let size = 1 + (r(2) * 3.0) as i32;
        let speed = 20.0 + 15.0 * size as f64 + 20.0 * r(1);
        let y = (r(0) * h + speed * t).rem_euclid(h) - 6.0;
        let x = (r(3) * w + 14.0 * (t * (0.5 + r(4)) + r(5) * std::f64::consts::TAU).sin()).rem_euclid(w);
        let a = 120 + size as u32 * 40;
        let (x, y) = (scr.x + x as i32, scr.y + y as i32);
        cv.round(Rect::new(x - size, y - size, 2 * size, 2 * size), size, SNOW, a);
    }
}

/// Rockets one after the other: each rises, bursts into sparks in a channel colour, and the sparks fall and fade.
fn firework(cv: &mut Canvas, scr: Rect, t: f64) {
    let last = (t / LAUNCH).floor() as i64;
    let first = ((t - RISE - GLOW) / LAUNCH).floor().max(0.0) as i64;
    for j in first..=last {
        let age = t - j as f64 * LAUNCH;
        if age < 0.0 {
            continue;
        }
        let r = |k: u32| hash(j as u32 * 16 + k);
        let px = scr.x as f64 + scr.w as f64 * (0.12 + 0.76 * r(0));
        let py = scr.y as f64 + scr.h as f64 * (0.12 + 0.3 * r(1));
        let col = CH_COLORS[j as usize % NCH];
        if age < RISE {
            // the rocket: fast at the start, slowing to the top
            let k = 1.0 - (1.0 - age / RISE).powi(2);
            let y0 = (scr.y + scr.h) as f64;
            let y = y0 + (py - y0) * k;
            for s in 0..4 {
                let a = 255 - s * 60;
                cv.round(Rect::new(px as i32 - 2, y as i32 + s as i32 * 6 - 2, 4, 4), 2, TEXT, a);
            }
            continue;
        }
        let a = age - RISE;
        if a >= GLOW {
            continue;
        }
        let fade = 1.0 - a / GLOW;
        if a < 0.12 {
            // the flash of the burst
            let rad = 6 + (a * 80.0) as i32;
            cv.round(Rect::new(px as i32 - rad, py as i32 - rad, 2 * rad, 2 * rad), rad, TEXT, 180);
        }
        let speed = 100.0 + 60.0 * r(2);
        let head = mix(col, TEXT, 70);
        for s in 0..SPARKS {
            let ang = s as f64 * std::f64::consts::TAU / SPARKS as f64 + 0.25 * r(3 + s % 8);
            let v = speed * (0.75 + 0.25 * r(11 + s % 5));
            // the spark and two points of its trail
            for (k, (rad, a_k)) in [(3, 1.0), (2, 0.55), (1, 0.3)].into_iter().enumerate() {
                let at = (a - k as f64 * 0.05).max(0.0);
                let x = px + ang.cos() * v * at;
                let y = py + ang.sin() * v * at + 45.0 * at * at;
                let rad = if fade > 0.4 { rad } else { (rad - 1).max(1) };
                let c = if k == 0 { head } else { col };
                let (x, y) = (x as i32, y as i32);
                cv.round(Rect::new(x - rad, y - rad, 2 * rad, 2 * rad), rad, c, (255.0 * fade * a_k) as u32);
            }
        }
    }
}

/// Santa's face, about 90 px high, centred at (`cx`, `cy`): hat, face, beard.
fn santa(cv: &mut Canvas, cx: i32, cy: i32) {
    // the hat, its tip bent to the right, with the pompom
    cv.triangle([(cx - 26, cy - 14), (cx + 26, cy - 14), (cx + 22, cy - 50)], HAT);
    cv.triangle([(cx + 10, cy - 40), (cx + 22, cy - 50), (cx + 34, cy - 38)], HAT);
    cv.circle(cx + 34, cy - 38, 7, BEARD);
    // face, cheeks, eyes
    cv.circle(cx, cy + 2, 21, SKIN);
    cv.round(Rect::new(cx - 17, cy + 2, 9, 7), 3, CHEEK, 150);
    cv.round(Rect::new(cx + 8, cy + 2, 9, 7), 3, CHEEK, 150);
    cv.circle(cx - 8, cy - 3, 2, EYE);
    cv.circle(cx + 8, cy - 3, 2, EYE);
    // beard and moustache, the nose over them
    cv.circle(cx, cy + 26, 22, BEARD);
    cv.circle(cx - 17, cy + 16, 12, BEARD);
    cv.circle(cx + 17, cy + 16, 12, BEARD);
    cv.circle(cx - 7, cy + 10, 7, BEARD);
    cv.circle(cx + 7, cy + 10, 7, BEARD);
    cv.circle(cx, cy + 5, 4, CHEEK);
    // the hat's brim over the forehead
    cv.round(Rect::new(cx - 30, cy - 20, 60, 12), 6, BEARD, 255);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i64, mo: u32, d: u32) -> Local {
        Local { y, mo, d, h: 9, mi: 0, s: 0 }
    }

    fn ui() -> Ui {
        let mut u = crate::ui::testkit::demo_ui();
        u.relayout();
        u
    }

    fn tap() -> Ev {
        Ev::Touch { id: 0, phase: Phase::Up, x: 10, y: 10 }
    }

    #[test]
    fn only_on_the_days_of_this_year() {
        let o = |y, mo, d| Occasion::of(&day(y, mo, d));
        assert_eq!(o(2026, 12, 23), None);
        for d in 24..=26 {
            assert_eq!(o(2026, 12, d), Some(Occasion::Christmas));
        }
        assert_eq!(o(2026, 12, 27), None);
        assert_eq!(o(2026, 12, 31), None);
        assert_eq!(o(2027, 1, 1), Some(Occasion::NewYear));
        assert_eq!(o(2027, 1, 2), None);
        assert_eq!(o(2027, 12, 24), None);
        assert_eq!(o(2026, 1, 1), None);
    }

    #[test]
    fn nothing_without_a_set_clock() {
        let mut u = ui();
        u.clock_known = false;
        u.greeting_tick();
        assert!(u.greeting.open.is_none() && !u.greeting.decided);
    }

    #[test]
    fn opens_once_and_a_tap_closes_it() {
        let mut u = ui();
        u.now = 20.0;
        u.greeting_decide(&day(2026, 12, 24));
        assert_eq!(u.greeting.open.map(|g| g.0), Some(Occasion::Christmas));
        // a touch already on its way does not close it
        assert!(u.greeting_input(&tap()));
        assert!(u.greeting.open.is_some());
        u.now += SETTLE;
        assert!(u.greeting_input(&tap()));
        assert!(u.greeting.open.is_none());
        // decided for this run: the next frames do not open it again
        u.clock_known = true;
        u.greeting_tick();
        assert!(u.greeting.open.is_none());
        assert!(!u.greeting_input(&tap()), "input goes to the screen again");
    }

    #[test]
    fn no_context_menu_stays_over_the_card() {
        let mut u = ui();
        let g = u.plot.grid;
        let (x, y) = (g.x + g.w / 2, g.y + g.h / 2);
        u.now = 10.0;
        u.workspace_touch(Phase::Down, x, y);
        u.now = 12.0;
        u.workspace_hold_tick();
        assert!(u.workspace.popup.is_some());
        u.greeting_decide(&day(2026, 12, 25));
        assert!(u.greeting.open.is_some());
        assert!(u.workspace.popup.is_none());
        // the finger still down opens no new one under the card
        u.now = 14.0;
        u.workspace_hold_tick();
        assert!(u.workspace.popup.is_none());
    }

    #[test]
    fn not_after_the_start() {
        let mut u = ui();
        u.now = WINDOW + 1.0;
        u.greeting_decide(&day(2027, 1, 1));
        assert!(u.greeting.open.is_none() && u.greeting.decided);
    }

    #[test]
    fn the_power_key_still_works() {
        let mut u = ui();
        u.greeting_decide(&day(2027, 1, 1));
        u.now += SETTLE;
        assert!(!u.greeting_input(&Ev::Key(key::POWER)));
        assert!(u.greeting.open.is_some());
    }

    #[test]
    fn a_closed_greeting_does_not_come_again() {
        let dir = std::env::temp_dir().join(format!("oos-greeting-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let _ = std::fs::remove_file(dir.join(SEEN_FILE));
        let mut u = ui();
        u.sys_path = Some(dir.join("system.conf"));
        u.greeting_decide(&day(2026, 12, 25));
        u.now += SETTLE;
        u.greeting_input(&tap());
        let mut again = ui();
        again.sys_path = Some(dir.join("system.conf"));
        again.greeting_decide(&day(2026, 12, 26));
        assert!(again.greeting.open.is_none(), "Christmas was closed");
        let mut ny = ui();
        ny.sys_path = Some(dir.join("system.conf"));
        ny.greeting_decide(&day(2027, 1, 1));
        assert!(ny.greeting.open.is_some(), "New Year is another greeting");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_card_fits_at_every_text_size() {
        for ts in TEXT_SIZES {
            for o in [Occasion::Christmas, Occasion::NewYear] {
                let mut u = ui();
                u.m.text_size = ts;
                u.relayout();
                let l = u.greeting_layout(o);
                let scr = u.scr.rect();
                assert!(l.card.x >= scr.x && l.card.right() <= scr.right(), "{:?} {:?}", ts, o);
                assert!(l.card.y >= scr.y && l.card.y + l.card.h <= scr.y + scr.h, "{:?} {:?}", ts, o);
                let f = u.fonts();
                let last = l.text_y + l.lines.len() as i32 * l.line_h;
                assert!(last <= l.button.y - 8, "the text ends above the button: {:?} {:?}", ts, o);
                assert!(l.lines.iter().all(|s| l.text_x + f.m.width(s) <= l.card.right() - 16), "{:?} {:?}", ts, o);
                assert!(f.lb.width(o.title()) <= l.title.w, "{:?} {:?}", ts, o);
            }
        }
    }

    #[test]
    fn draws_over_the_screen_at_any_time() {
        for o in [Occasion::Christmas, Occasion::NewYear] {
            let mut u = ui();
            u.greeting.open = Some((o, 0.0));
            for t in [0.0, 0.3, 0.7, 1.9, 5.0, 61.3] {
                u.now = t;
                let mut cv = Canvas::new(u.scr.rect().w, u.scr.rect().h);
                u.render(&mut cv);
            }
        }
    }
}
