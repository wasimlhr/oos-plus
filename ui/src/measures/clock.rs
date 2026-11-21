// SPDX-License-Identifier: GPL-2.0-or-later

//! Jitter of a clock on one source (`oos_dsp::clock`): the rising edges at the middle level, each interval between
//! two of them one period. Unlike the jitter of the eye page there is no clock recovery and no data model, so these
//! are the figures to take for a clock; the eye page's figures are those of NRZ data.

use super::*;
use Mark::*;

/// Numbers: unique over all modules and never reused (saved setups and SCPI `CAT<id>` refer to them).
pub const PERIOD_JITTER_RMS: u16 = 52;
pub const PERIOD_JITTER_PP: u16 = 53;
pub const CYCLE_TO_CYCLE: u16 = 54;
pub const CLOCK_TIE_RMS: u16 = 55;
pub const CLOCK_TIE_PP: u16 = 56;

/// Whether `id` is one of this module's measurements.
pub fn owns(id: u16) -> bool {
    (PERIOD_JITTER_RMS..=CLOCK_TIE_PP).contains(&id)
}

const G: &[(u8, &str)] = &[(JITTER, "CLOCK JITTER")];

static DEFS: [Def; 5] = [
    module(PERIOD_JITTER_RMS, "Period Jitter RMS", "s", Slope, &[0], G, "Standard deviation of the periods between rising edges over the record."),
    module(PERIOD_JITTER_PP, "Period Jitter Pk-Pk", "s", Slope, &[0], G, "Longest period minus the shortest over the record."),
    module(CYCLE_TO_CYCLE, "Cycle-to-Cycle", "s", Slope, &[0], G, "Largest change between two adjacent periods, in magnitude."),
    module(CLOCK_TIE_RMS, "Clock TIE RMS", "s", Slope, &[0], G, "RMS deviation of the rising edges from the straight line through them."),
    module(CLOCK_TIE_PP, "Clock TIE Pk-Pk", "s", Slope, &[0], G, "Peak-to-peak deviation of the rising edges from that line."),
];

/// What every value includes: the record's own timing and noise.
const QUALITY: &str = "Rising edges at the middle level; includes the scope's own timing jitter, noise and interpolation";

pub struct Clock;

impl MeasModule for Clock {
    fn key(&self) -> &'static str {
        "clock_jitter"
    }

    fn defs(&self) -> &'static [Def] {
        &DEFS
    }

    fn compute(&self, x: &Ctx, ids: &[u16]) -> Values {
        let j = x
            .wave(x.row[0])
            .and_then(|v| oos_dsp::clock::clock_jitter(&oos_dsp::measure::analyze(v, x.dt).rises));
        let values = ids
            .iter()
            .map(|&id| {
                let v = j.map_or(f64::NAN, |j| match id {
                    PERIOD_JITTER_RMS => j.period_rms,
                    PERIOD_JITTER_PP => j.period_pp,
                    CYCLE_TO_CYCLE => j.c2c_max,
                    CLOCK_TIE_RMS => j.tie_rms,
                    _ => j.tie_pp,
                });
                (id, v)
            })
            .collect();
        Values { values, quality: QUALITY }
    }

    fn pending_quality(&self, _id: u16) -> &'static str {
        QUALITY
    }

    fn missing_reason(&self, _id: u16) -> Option<&'static str> {
        Some("Needs at least four rising edges in the record")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 40 MHz clock at 1.25 GSa/s whose every 10th rising edge is 100 ps late: the module reads the record's edges,
    /// so the figures show that jitter; a clean one shows next to none. The clock is a sine: the edge times come from
    /// sin(x)/x interpolation, which is exact only for a signal without corners. Each cycle is delayed from one crest
    /// to the next, where the delay makes no step worth speaking of, and holds one rising edge.
    #[test]
    fn a_clock_with_late_edges() {
        let m = Model::default();
        let dt = 0.8e-9;
        let wave = |late: f64| -> Vec<f32> {
            (0..100_000)
                .map(|k| {
                    let t = k as f64 * dt;
                    let n = ((t - 6.25e-9) / 25e-9).floor() as i64;
                    let shift = if n.rem_euclid(10) == 0 { late } else { 0.0 };
                    (std::f64::consts::TAU * 40e6 * (t - shift)).sin() as f32
                })
                .collect()
        };
        let ids = [PERIOD_JITTER_RMS, PERIOD_JITTER_PP, CYCLE_TO_CYCLE, CLOCK_TIE_RMS, CLOCK_TIE_PP];
        let run = |v: Vec<f32>| {
            let waves = vec![(WaveId::Ch(0), v)];
            let x = Ctx { model: &m, t0: 0.0, dt, row: [WaveId::Ch(0); 4], waves: &waves };
            Clock.compute(&x, &ids).values.iter().map(|v| v.1).collect::<Vec<_>>()
        };
        let clean = run(wave(0.0));
        assert!(clean.iter().all(|v| v.abs() < 5e-12), "{clean:?}");
        let late = run(wave(100e-12));
        // the late edge shortens the period before it and lengthens the one after: pk-pk 200 ps, adjacent change
        // up to 200 ps
        assert!((late[1] - 200e-12).abs() < 10e-12, "{late:?}");
        assert!((late[2] - 200e-12).abs() < 10e-12, "{late:?}");
        assert!((late[4] - 100e-12).abs() < 10e-12, "{late:?}");
    }

    #[test]
    fn no_source_is_nan() {
        let m = Model::default();
        let x = Ctx { model: &m, t0: 0.0, dt: 1e-9, row: [WaveId::Ch(0); 4], waves: &[] };
        assert!(Clock.compute(&x, &[PERIOD_JITTER_RMS]).values[0].1.is_nan());
    }
}
