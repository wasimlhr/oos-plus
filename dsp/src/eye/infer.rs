// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The clock of a record for the eye and the jitter measurements: thresholds from the record's levels, the mean
//! clock, then a PLL near it. The eye page and the measurement modules of oos-ui both start from here.

use super::{recover_clock, recover_clock_pll, Clock};
use crate::levels::stats;

#[derive(Clone, Debug)]
/// What the clock recovery of `infer` found: the rate, the decision threshold and its hysteresis in volts, the
/// samples per unit interval, how the clock was recovered, and a warning for a doubtful result (empty when none).
pub struct Summary {
    pub rate: f64,
    pub threshold: f32,
    pub hysteresis: f32,
    pub samples_per_ui: f64,
    pub recovery: &'static str,
    pub warning: String,
}

/// The clock of an NRZ record `v` (one sample every `dt` seconds): with `requested`, near that rate; with
/// `explicit_clock`, `v` is the clock itself and its mean period is taken. Err says why it cannot be found.
pub fn infer(v: &[f32], dt: f64, requested: Option<f64>, explicit_clock: bool) -> Result<(Summary, Clock), String> {
    if v.len() < 8 || !dt.is_finite() || dt <= 0.0 {
        return Err("Need at least 8 samples with a valid interval".into());
    }
    let st = stats(v, dt);
    let span = st.vmax - st.vmin;
    if !span.is_finite() || span <= 1e-9 {
        return Err("Flat source: no transitions to recover a clock".into());
    }
    let level = if explicit_clock { None } else { decision_level(v) };
    let amplitude = level.map_or(span, |(_, a)| a.max(span * 0.25));
    let threshold = level.map_or(0.5 * (st.vmax + st.vmin), |(t, _)| t);
    // Voltage thresholds follow the data, never the waveform's display zoom.
    let hysteresis = amplitude * if explicit_clock { 0.2 } else { 0.1 };
    let mean_clock = recover_clock(v, dt, threshold, hysteresis, requested)
        .ok_or_else(|| "Need at least 3 usable transitions; set a rate or explicit clock".to_string())?;
    let samples_per_ui = mean_clock.period / dt;
    if !samples_per_ui.is_finite() || samples_per_ui < 2.0 {
        return Err("Recovered UI is below 2 samples: set a known rate/clock or use a faster record".into());
    }
    let clock = if explicit_clock {
        mean_clock
    } else {
        recover_clock_pll(v, dt, threshold, hysteresis, Some(mean_clock.rate()), 0.0, true).unwrap_or(mean_clock)
    };
    let warning = if samples_per_ui < 10.0 {
        format!("Limited sampling: {:.1} samples/UI", samples_per_ui)
    } else if span < 0.05 {
        "Low amplitude: check signal/noise before interpreting jitter".into()
    } else {
        String::new()
    };
    Ok((
        Summary {
            rate: clock.rate(),
            threshold,
            hysteresis,
            samples_per_ui,
            recovery: if explicit_clock {
                "Explicit clock"
            } else if requested.is_some() {
                "NRZ PLL near set rate"
            } else {
                "Auto NRZ PLL"
            },
            warning,
        },
        clock,
    ))
}

/// The decision level of an NRZ record and the amplitude around it: the middle of top and base as the measurements
/// find them (`measure::top_base_auto`), so the eye's edges are timed at the 50 % level of Measure's Top and Base.
/// The levels of `levels::stats` count a level only when one of 256 bins holds 5 % of the samples: the levels of a
/// fast, noisy record spread wider than that, it fell back to the extremes, and one undershoot moved every edge
/// off the crossing (the jitter of a 2.5 Gb/s record three times too large next to another analyser's, 2026-11-21).
pub fn decision_level(v: &[f32]) -> Option<(f32, f32)> {
    let (top, base) = crate::measure::top_base_auto(v)?;
    Some((0.5 * (top + base), top - base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_nrz_pattern_gives_its_rate() {
        // a 1 MHz PRBS-like pattern, 20 samples a bit at 20 MSa/s
        let mut lfsr = 0x5au8;
        let mut v = Vec::new();
        for _ in 0..400 {
            lfsr = lfsr << 1 | ((lfsr >> 7 ^ lfsr >> 5 ^ lfsr >> 4 ^ lfsr >> 3) & 1);
            v.extend(std::iter::repeat_n(if lfsr & 1 == 1 { 1.0f32 } else { 0.0 }, 20));
        }
        let (summary, clock) = infer(&v, 50e-9, None, false).unwrap();
        assert!((summary.rate - 1e6).abs() < 1e3, "{}", summary.rate);
        assert!((summary.samples_per_ui - 20.0).abs() < 0.1, "{}", summary.samples_per_ui);
        assert!((clock.rate() - 1e6).abs() < 1e3);
    }

    #[test]
    fn edges_are_timed_at_the_middle_of_top_and_base_of_a_noisy_record_with_an_undershoot() {
        // 8 samples a bit, levels 0.4 V and -0.3 V (middle 0.05 V), edges over 4 samples, noise of about 40 mV that
        // spreads each level over more bins than the 5 % of `levels::stats` allows, and one undershoot to -0.7 V
        // that puts the middle of the extremes near -0.12 V
        let (mut lfsr, mut seed) = (0x5au8, 12345u32);
        let mut uniform = || {
            seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
            ((seed >> 8) % 1000) as f32 / 1000.0 * 0.07 - 0.035
        };
        // the sum of two: a peak at the level, as noise has
        let mut noise = || uniform() + uniform();
        let (hi, lo) = (0.4f32, -0.3f32);
        let mut level = lo;
        let mut v = Vec::new();
        for _ in 0..2000 {
            lfsr = lfsr << 1 | ((lfsr >> 7 ^ lfsr >> 5 ^ lfsr >> 4 ^ lfsr >> 3) & 1);
            let next = if lfsr & 1 == 1 { hi } else { lo };
            for k in 0..8 {
                let x = if k < 4 { level + (next - level) * (k as f32 + 0.5) / 4.0 } else { next };
                v.push(x + noise());
            }
            level = next;
        }
        v[5000] = -0.7;
        let (top, base) = crate::measure::top_base_auto(&v).unwrap();
        assert!((top - hi).abs() < 0.03 && (base - lo).abs() < 0.03, "{top} {base}");
        let st = stats(&v, 1e-9);
        assert_eq!((st.top, st.base), (st.vmax, st.vmin), "the levels of `stats` fall back to the extremes here");
        let (summary, clock) = infer(&v, 1e-9, Some(125e6), false).unwrap();
        assert!((summary.threshold - 0.05).abs() < 0.03, "{}", summary.threshold);
        // timed at the middle, rising and falling edges sit on the same clock: no duty-cycle distortion
        let tie = super::super::tie(&clock.edges, &clock);
        let mean = |rising: bool| {
            let of = |i: usize| (i % 2 == 0) == (clock.first_rising == rising);
            let x: Vec<f64> = tie.iter().enumerate().filter(|&(i, _)| of(i)).map(|(_, &t)| t).collect();
            x.iter().sum::<f64>() / x.len() as f64
        };
        let dcd = (mean(true) - mean(false)).abs() / clock.period;
        assert!(dcd < 0.05, "DCD {dcd} UI");
    }

    #[test]
    fn a_flat_or_short_record_says_why() {
        assert!(infer(&[0.0; 4], 1e-9, None, false).unwrap_err().contains("8 samples"));
        assert!(infer(&[0.5; 64], 1e-9, None, false).unwrap_err().contains("Flat"));
    }
}
