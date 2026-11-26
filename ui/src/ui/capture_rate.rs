// SPDX-License-Identifier: GPL-2.0-or-later

//! The waveform rate under the run state in the top bar (`bars.rs`): while the hardware runs, the waveforms a second
//! that reach the screen (the pipeline's `used` counter over at least a second) and the share of the time they
//! capture (the rate times one record's span, at most 100 %). The idea comes from fumingcaps' fork (release v0.2.1),
//! which shows the same two figures in its run state box.

use super::*;
use std::time::{Duration, Instant};

/// The last count the rate was worked out from, and the caption it gave.
#[derive(Default)]
pub struct CaptureRate {
    start: Option<(Instant, u64)>,
    label: Option<String>,
}

impl CaptureRate {
    /// The count `used` of waveforms that reached the screen at `at`, with one record's span `span` (s): once at
    /// least a second has passed since the last count, the caption of the rate and the share in between.
    fn observe(&mut self, at: Instant, used: u64, span: Option<f64>) {
        let Some((t0, n0)) = self.start else {
            self.start = Some((at, used));
            return;
        };
        let elapsed = at.saturating_duration_since(t0);
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let rate = used.saturating_sub(n0) as f64 / elapsed.as_secs_f64();
        let share = span.filter(|s| s.is_finite() && *s > 0.0).map(|s| (rate * s).clamp(0.0, 1.0));
        self.label = Some(caption(rate, share));
        self.start = Some((at, used));
    }
}

/// "12.3k wfm/s 4.5%": the rate (three digits from 1000 on) and the share of the time (one decimal under 10 %).
fn caption(rate: f64, share: Option<f64>) -> String {
    let rate = match rate {
        r if r < 999.5 => format!("{r:.0}"),
        r if r < 9_995.0 => format!("{:.2}k", r / 1e3),
        r if r < 99_950.0 => format!("{:.1}k", r / 1e3),
        r => format!("{:.0}k", r / 1e3),
    };
    let share = share.map_or_else(
        || "--".to_string(),
        |s| if s * 100.0 < 10.0 { format!("{:.1}", s * 100.0) } else { format!("{:.0}", s * 100.0) },
    );
    format!("{rate} wfm/s {share}%")
}

impl Ui {
    /// The hardware runs and the screen shows its acquisition (not a reference or a held record).
    fn capture_running(&self) -> bool {
        self.m.run == RunState::Run && self.m.status != TrigStatus::Stop && self.src.is_hw() && !self.record_mode()
    }

    /// A step of the screen: the waveform rate counted on, or forgotten while the hardware does not run.
    pub(super) fn sample_capture_rate(&mut self) {
        self.sample_capture_rate_at(Instant::now());
    }

    /// `sample_capture_rate` at the instant `at`.
    fn sample_capture_rate_at(&mut self, at: Instant) {
        if !self.capture_running() {
            self.counts.capture = CaptureRate::default();
            return;
        }
        let (_, used, _) = crate::board::pipe::counters().read();
        let span = self.src.hw_rate_depth().map(|(rate, n)| n as f64 / rate);
        self.counts.capture.observe(at, used, span);
    }

    /// The caption under the run state, while there is one to show.
    pub(super) fn capture_caption(&self) -> Option<&str> {
        self.counts.capture.label.as_deref().filter(|_| self.capture_running())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Break it catches: a rate from less than a second of counts, a share above 100 % or without a span, a caption
    // that does not shorten large rates.
    #[test]
    fn the_rate_is_counted_over_a_second_with_the_share_of_the_time() {
        let t0 = Instant::now();
        let mut c = CaptureRate::default();
        c.observe(t0, 1000, Some(1e-5));
        assert_eq!(c.label, None);
        c.observe(t0 + Duration::from_millis(500), 2000, Some(1e-5));
        assert_eq!(c.label, None, "less than a second");
        c.observe(t0 + Duration::from_secs(2), 9000, Some(1e-5));
        assert_eq!(c.label.as_deref(), Some("4.00k wfm/s 4.0%"));
        c.observe(t0 + Duration::from_secs(3), 9035, Some(0.02));
        assert_eq!(c.label.as_deref(), Some("35 wfm/s 70%"));
        c.observe(t0 + Duration::from_secs(4), 9135, Some(0.02));
        assert_eq!(c.label.as_deref(), Some("100 wfm/s 100%"), "at most 100 %");
        c.observe(t0 + Duration::from_secs(5), 24_335, None);
        assert_eq!(c.label.as_deref(), Some("15.2k wfm/s --%"));
        assert_eq!(caption(123_456.0, Some(0.5)), "123k wfm/s 50%");
    }

    // Break it catches: a rate shown in STOP, in a session (offline or SIM) or with another source than the hardware,
    // or a rate of an earlier run shown again before a new second of counts.
    #[test]
    fn the_rate_shows_only_while_the_hardware_runs() {
        let t0 = Instant::now();
        let mut u = Ui::new(Source::Hw { shared: Default::default(), seen: 0 });
        (u.m.run, u.m.status) = (RunState::Run, TrigStatus::Triggered);
        let counted = |u: &mut Ui, s: u64| u.sample_capture_rate_at(t0 + Duration::from_secs(s));
        counted(&mut u, 0);
        assert_eq!(u.capture_caption(), None, "less than a second counted");
        counted(&mut u, 2);
        assert!(u.capture_caption().is_some_and(|c| c.contains(" wfm/s ")), "running: {:?}", u.capture_caption());
        u.m.run = RunState::Stop;
        assert_eq!(u.capture_caption(), None, "STOP");
        counted(&mut u, 3);
        u.m.run = RunState::Run;
        assert_eq!(u.capture_caption(), None, "the rate of the earlier run is forgotten");
        counted(&mut u, 4);
        counted(&mut u, 6);
        assert!(u.capture_caption().is_some(), "counted again");
        u.offline.on = true;
        assert_eq!(u.capture_caption(), None, "a session");
        u.offline.on = false;
        u.m.status = TrigStatus::Stop;
        assert_eq!(u.capture_caption(), None, "stopped");
        let mut demo = Ui::new(Source::Demo { t: 0.0 });
        (demo.m.run, demo.m.status) = (RunState::Run, TrigStatus::Triggered);
        demo.sample_capture_rate_at(t0);
        demo.sample_capture_rate_at(t0 + Duration::from_secs(2));
        assert_eq!(demo.capture_caption(), None, "the demo source");
    }
}
