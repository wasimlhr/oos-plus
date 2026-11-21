// SPDX-License-Identifier: GPL-2.0-or-later

//! Jitter of a clock from the times of its edges of one polarity: period jitter, cycle-to-cycle jitter and the time
//! interval error against the straight line through the edges. No clock recovery and no data model: a clock has one
//! edge of a polarity per cycle, so each interval between two of them is one period.
//!
//! The edge times carry what the record carries: the scope's own sample-clock jitter, its noise turned into time by
//! the slope of the edge, and the error of the interpolation between samples. The figures are therefore an upper
//! bound of the clock's own jitter, not the clock's jitter alone.

/// The jitter figures of one record, in the unit of the edge times (seconds when the times are).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClockJitter {
    /// Standard deviation of the periods (edge to next edge).
    pub period_rms: f64,
    /// Largest period minus the smallest.
    pub period_pp: f64,
    /// Largest difference of two adjacent periods, in magnitude (JEDEC's cycle-to-cycle jitter).
    pub c2c_max: f64,
    /// RMS of the differences of adjacent periods.
    pub c2c_rms: f64,
    /// RMS of the edge times' deviation from the least-squares line through them (the ideal clock of the record).
    pub tie_rms: f64,
    /// Largest deviation minus the smallest.
    pub tie_pp: f64,
    /// Periods the figures are taken over.
    pub periods: usize,
}

/// The jitter of the edges `t` (increasing times of edges of one polarity), or `None` with fewer than four edges
/// (three periods: two differences of adjacent periods).
pub fn clock_jitter(t: &[f64]) -> Option<ClockJitter> {
    if t.len() < 4 || t.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let per: Vec<f64> = t.windows(2).map(|w| w[1] - w[0]).collect();
    let n = per.len() as f64;
    let mean = per.iter().sum::<f64>() / n;
    let period_rms = (per.iter().map(|p| (p - mean) * (p - mean)).sum::<f64>() / n).sqrt();
    let (lo, hi) = min_max(&per);
    let d: Vec<f64> = per.windows(2).map(|w| w[1] - w[0]).collect();
    let c2c_max = d.iter().fold(0.0f64, |a, x| a.max(x.abs()));
    let c2c_rms = (d.iter().map(|x| x * x).sum::<f64>() / d.len() as f64).sqrt();
    // least-squares line t = a + b k over the edge numbers k, centred so the sums stay small
    let m = t.len() as f64;
    let kc = (m - 1.0) / 2.0;
    let tc = t.iter().sum::<f64>() / m;
    let (mut sxy, mut sxx) = (0.0, 0.0);
    for (k, &x) in t.iter().enumerate() {
        let dk = k as f64 - kc;
        sxy += dk * (x - tc);
        sxx += dk * dk;
    }
    let b = sxy / sxx;
    let tie: Vec<f64> = t.iter().enumerate().map(|(k, &x)| x - (tc + b * (k as f64 - kc))).collect();
    let tie_rms = (tie.iter().map(|e| e * e).sum::<f64>() / m).sqrt();
    let (elo, ehi) = min_max(&tie);
    Some(ClockJitter { period_rms, period_pp: hi - lo, c2c_max, c2c_rms, tie_rms, tie_pp: ehi - elo, periods: per.len() })
}

fn min_max(v: &[f64]) -> (f64, f64) {
    v.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &x| (a.min(x), b.max(x)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A perfect clock has no jitter, whatever its phase and period.
    #[test]
    fn a_perfect_clock_has_none() {
        let t: Vec<f64> = (0..100).map(|k| 3.7e-9 + k as f64 * 25e-9).collect();
        let j = clock_jitter(&t).unwrap();
        assert_eq!(j.periods, 99);
        for x in [j.period_rms, j.period_pp, j.c2c_max, j.c2c_rms, j.tie_rms, j.tie_pp] {
            assert!(x.abs() < 1e-18, "{j:?}");
        }
    }

    /// One edge late by `e`: the TIE peak-to-peak is about `e`, two periods change by `±e` (period pk-pk `2e`), and
    /// the adjacent differences reach `2e` (`+e`, then `-e` against `+e`).
    #[test]
    fn one_late_edge() {
        let e = 1e-10;
        let mut t: Vec<f64> = (0..1000).map(|k| k as f64 * 25e-9).collect();
        t[500] += e;
        let j = clock_jitter(&t).unwrap();
        assert!((j.period_pp - 2.0 * e).abs() < 1e-15, "{j:?}");
        assert!((j.c2c_max - 2.0 * e).abs() < 1e-15, "{j:?}");
        assert!((j.tie_pp - e).abs() < 1e-12, "{j:?}");
        // the rms of one deviation among 1000: e / sqrt(1000), less the small part the fitted line takes up
        assert!((j.tie_rms - e / 1000f64.sqrt()).abs() < 0.01 * e, "{j:?}");
    }

    /// A frequency other than the nominal one is no jitter: the line follows the clock's own period.
    #[test]
    fn the_line_takes_the_clocks_own_period() {
        let t: Vec<f64> = (0..50).map(|k| k as f64 * 24.99e-9).collect();
        assert!(clock_jitter(&t).unwrap().tie_rms < 1e-18);
    }

    /// Alternating periods (duty-cycle-like modulation of the rising edges): period rms is the half difference,
    /// cycle-to-cycle the full one.
    #[test]
    fn alternating_periods() {
        let mut t = vec![0.0];
        for k in 0..200 {
            let p = if k % 2 == 0 { 25.1e-9 } else { 24.9e-9 };
            t.push(t.last().unwrap() + p);
        }
        let j = clock_jitter(&t).unwrap();
        assert!((j.period_rms - 0.1e-9).abs() < 1e-15, "{j:?}");
        assert!((j.c2c_rms - 0.2e-9).abs() < 1e-15 && (j.c2c_max - 0.2e-9).abs() < 1e-15, "{j:?}");
    }

    #[test]
    fn too_few_edges() {
        assert!(clock_jitter(&[0.0, 1.0, 2.0]).is_none());
        assert!(clock_jitter(&[0.0, 1.0, f64::NAN, 3.0]).is_none());
    }
}
