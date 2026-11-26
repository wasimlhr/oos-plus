// SPDX-License-Identifier: GPL-2.0-or-later

//! The frame clock of the loop on the scope (`Device::pass` in main.rs): when the next step of the screen starts.
//!
//! `pacing.clock = flip` in system.conf, the default (verified on the unit, DHO924S 2026-11-25): a step is due at every
//! `DIVISOR`th vblank of the panel, which the DRM file reports as a sequence event (`drm::Display::ask_vblank`). The
//! panel's refresh comes from its mode (about 97.4 Hz on the DHO900 by the fork's measurement, assumed here: 32.5
//! pictures a second, and no beat against the panel). When the panel's last flip is still in work at that vblank, the
//! step is held and starts as soon as it is done; the GPU helper's last frame is waited for inside the step, after the
//! new frame's layers (main.rs `frame`), so the two overlap. Without a vblank event for `FALLBACK`
//! (the start, a CRTC that refuses them) the timer runs until events come again. `pacing.clock = timer`, a monitor on
//! HDMI (a refresh of its own) and every build without the panel: the timer alone, a step `TIMER` after the last one
//! began, 30 pictures a second at most.

use std::time::{Duration, Instant};

/// The timer's period: 30 steps a second.
pub const TIMER: Duration = Duration::from_nanos(1_000_000_000 / 30);
/// No vblank event for this long: the timer.
pub const FALLBACK: Duration = Duration::from_millis(50);
/// Vblanks per step on the flip clock.
pub const DIVISOR: u64 = 3;

/// The period of a step now (µs): `TIMER` on the timer, `DIVISOR` vblanks on the flip clock (`set_period`). What the
/// screen's work may take in a step follows from it (`Ui::fast_budget`).
static PERIOD_US: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(33_333);

/// The period of a step: on the flip clock `DIVISOR` vblanks of a panel of `refresh_hz`, else (None, or a refresh that
/// is not one) the timer's.
pub fn set_period(refresh_hz: Option<f64>) {
    PERIOD_US.store(period_us_of(refresh_hz), std::sync::atomic::Ordering::Relaxed);
}

/// `set_period`'s period (µs).
pub fn period_us_of(refresh_hz: Option<f64>) -> u32 {
    match refresh_hz.filter(|&hz| (20.0..=240.0).contains(&hz)) {
        Some(hz) => (DIVISOR as f64 * 1e6 / hz) as u32,
        None => TIMER.as_micros() as u32,
    }
}

/// The period of a step now (ms).
pub fn period_ms() -> f32 {
    PERIOD_US.load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0
}

/// `period_ms` for a refresh (`set_period`).
pub fn period_ms_of(refresh_hz: Option<f64>) -> f32 {
    period_us_of(refresh_hz) as f32 / 1000.0
}

/// The clock `pacing.clock` chooses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Clock {
    Flip,
    Timer,
}

impl Clock {
    /// The clock of a `pacing.clock` value: `timer`, else the flip clock.
    pub fn of(v: Option<&str>) -> Clock {
        if v == Some("timer") {
            Clock::Timer
        } else {
            Clock::Flip
        }
    }
}

/// On the timer: whether a step starts at `now` when the last one began at `last`.
pub fn timer_due(now: Instant, last: Instant) -> bool {
    now.saturating_duration_since(last) >= TIMER
}

/// On the timer: how long until the next step.
pub fn timer_wait(now: Instant, last: Instant) -> Duration {
    (last + TIMER).saturating_duration_since(now)
}

/// The flip clock's state (the pure part: the loop hands it the events and the time).
#[derive(Debug)]
pub struct Pacer {
    divisor: u64,
    /// the vblank a step was last due at
    anchor: Option<u64>,
    /// when the last vblank event came
    seen: Option<Instant>,
    /// a step due at a vblank and not started yet
    held: bool,
    /// on the timer at the last look
    timer: bool,
    /// how often the clock went from the vblanks to the timer
    pub fallbacks: u32,
}

impl Pacer {
    pub fn new(divisor: u64) -> Pacer {
        Pacer { divisor: divisor.max(1), anchor: None, seen: None, held: false, timer: true, fallbacks: 0 }
    }

    /// A vblank event read at `now`, with its sequence number: a step is due at the first one, and then at one
    /// `divisor` vblanks or more after the vblank of the last due step (a vblank missed while the loop was busy only
    /// shifts the cadence). A newer due vblank replaces a held one.
    pub fn vblank(&mut self, seq: u64, now: Instant) {
        self.seen = Some(now);
        if self.anchor.is_none_or(|a| seq < a || seq - a >= self.divisor) {
            (self.anchor, self.held) = (Some(seq), true);
        }
    }

    /// The vblank the next event is to come at (None: the next one, whichever).
    pub fn next(&self) -> Option<u64> {
        self.anchor.map(|a| a + self.divisor)
    }

    /// A step is held: due, and the panel's flip was in work.
    #[cfg(test)]
    pub fn held(&self) -> bool {
        self.held
    }

    /// On the timer: no vblank event for `FALLBACK`.
    pub fn on_timer(&self, now: Instant) -> bool {
        self.seen.is_none_or(|t| now.saturating_duration_since(t) >= FALLBACK)
    }

    /// Whether a step starts at `now`; `last`: when the last one began, `busy`: the panel's flip still in work. On the timer the step itself waits for them, as it always did.
    pub fn due(&mut self, now: Instant, last: Instant, busy: bool) -> bool {
        let timer = self.on_timer(now);
        self.fallbacks += (timer && !self.timer && self.seen.is_some()) as u32;
        self.timer = timer;
        if timer {
            self.held = false;
            return timer_due(now, last);
        }
        let go = self.held && !busy;
        self.held &= !go;
        go
    }

    /// How long the loop may wait for inputs before it looks again; a vblank event or the flip's event ends the wait
    /// earlier (main.rs adds the display's descriptor).
    pub fn wait(&self, now: Instant, last: Instant) -> Duration {
        match self.seen.filter(|_| !self.on_timer(now)) {
            Some(t) => (t + FALLBACK).saturating_duration_since(now),
            None => timer_wait(now, last),
        }
    }
}

/// What a read of the DRM file gave (events: a type and a length, u32 each, then their data): the number of
/// flip-complete events (`DRM_EVENT_FLIP_COMPLETE`, 2) and the sequence number of the last vblank event
/// (`DRM_EVENT_CRTC_SEQUENCE`, 3: user data, time in ns, then the sequence as u64 at byte 24).
pub fn drm_events(buf: &[u8]) -> (u32, Option<u64>) {
    let (mut at, mut flips, mut seq) = (0, 0, None);
    while let Some(h) = buf.get(at..at + 8) {
        let typ = u32::from_ne_bytes([h[0], h[1], h[2], h[3]]);
        let len = u32::from_ne_bytes([h[4], h[5], h[6], h[7]]) as usize;
        if len < 8 {
            break;
        }
        flips += (typ == 2) as u32;
        if let Some(s) = buf.get(at + 24..at + 32).filter(|_| typ == 3 && len >= 32) {
            seq = Some(u64::from_ne_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]));
        }
        at += len;
    }
    (flips, seq)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(t0: Instant, n: u64) -> Instant {
        t0 + Duration::from_millis(n)
    }

    // Break it catches: a step at every vblank (or every second) instead of every third, or a missed vblank that
    // stops the clock
    #[test]
    fn a_step_is_due_at_every_third_vblank() {
        let t0 = Instant::now();
        let mut p = Pacer::new(DIVISOR);
        let mut steps = Vec::new();
        for (i, seq) in (100..110).chain(112..114).enumerate() {
            let now = ms(t0, 10 * i as u64);
            p.vblank(seq, now);
            if p.due(now, t0, false) {
                steps.push(seq);
            }
        }
        assert_eq!(steps, [100, 103, 106, 109, 112]);
        assert_eq!(p.next(), Some(115));
    }

    // Break it catches: a step dropped while the panel or the helper is busy, or started before they are free
    #[test]
    fn a_due_step_is_held_while_busy_and_starts_once_free() {
        let t0 = Instant::now();
        let mut p = Pacer::new(DIVISOR);
        p.vblank(7, t0);
        assert!(!p.due(ms(t0, 1), t0, true));
        p.vblank(8, ms(t0, 10));
        assert!(!p.due(ms(t0, 12), t0, true) && p.held());
        assert!(p.due(ms(t0, 14), t0, false), "starts as soon as nothing is busy");
        assert!(!p.held() && !p.due(ms(t0, 15), t0, false), "once");
        assert_eq!(p.next(), Some(10), "the cadence stays on the vblank it was due at");
    }

    // Break it catches: a loop that stands still without vblank events, or keeps the timer once they come again
    #[test]
    fn without_vblank_events_for_50_ms_the_timer_runs() {
        let t0 = Instant::now();
        let mut p = Pacer::new(DIVISOR);
        assert!(p.on_timer(t0) && p.due(ms(t0, 34), t0, false), "the timer before the first event");
        assert!(!p.due(ms(t0, 30), t0, false));
        p.vblank(1, ms(t0, 40));
        assert!(p.due(ms(t0, 40), t0, false));
        assert!(!p.due(ms(t0, 89), ms(t0, 40), false) && p.wait(ms(t0, 89), ms(t0, 40)) == Duration::from_millis(1));
        assert!(p.due(ms(t0, 90), ms(t0, 40), true), "the timer, busy or not");
        assert_eq!(p.fallbacks, 1);
        assert_eq!(p.wait(ms(t0, 100), ms(t0, 90)), Duration::from_millis(23) + Duration::from_nanos(333_333));
        p.vblank(9, ms(t0, 101));
        assert!(!p.on_timer(ms(t0, 101)) && p.due(ms(t0, 101), ms(t0, 90), false), "back on the vblanks");
    }

    #[test]
    fn the_clock_is_flip_unless_timer() {
        assert_eq!(Clock::of(None), Clock::Flip);
        assert_eq!(Clock::of(Some("flip")), Clock::Flip);
        assert_eq!(Clock::of(Some("timer")), Clock::Timer);
    }

    // Break it catches: a vblank event taken for a flip (or lost), which would leave the flip's wait or the clock
    #[test]
    fn flip_and_vblank_events_are_told_apart() {
        let mut b = Vec::new();
        for (typ, tail) in [(2u32, [5u8; 24]), (3, [0; 24]), (3, [0; 24])] {
            b.extend(typ.to_ne_bytes());
            b.extend(32u32.to_ne_bytes());
            b.extend(tail);
        }
        b[64 + 24..64 + 32].copy_from_slice(&41u64.to_ne_bytes());
        b[32 + 24..32 + 32].copy_from_slice(&40u64.to_ne_bytes());
        assert_eq!(drm_events(&b), (1, Some(41)));
        assert_eq!(drm_events(&b[..32]), (1, None));
        assert_eq!(drm_events(&b[..40]), (1, None), "a cut event is no event");
    }
}
