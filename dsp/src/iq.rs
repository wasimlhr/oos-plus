// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: an I/Q record (the baseband of a spectrum view, a complex signal around a centre frequency) as the real
//! signal it stands for. The record is interpolated up to a rate that holds the centre frequency and mixed up to it,
//! `x(t) = I(t) cos(2 pi fc t) - Q(t) sin(2 pi fc t)`, so the spectrum, the eye and the measurements read it like any
//! other record and a line at `fc + f` in the complex signal is a line at `fc + f` in the spectrum.

use crate::interp::lanczos8_grid;

/// The samples of the real signal, at most, that `iq_to_real` makes.
pub const IQ_MAX_SAMPLES: usize = 32_000_000;

/// The real passband signal of the I/Q record `iq` (I and Q one after the other, `dt_pair` between two pairs) around
/// `fc` Hz, and its sample step. The rate is the lowest multiple of the I/Q rate that is at least 2.2 times the
/// highest frequency of the record (`fc` plus half the I/Q rate); the interpolation is a Lanczos kernel of eight
/// lobes (images 60 dB down or more). The time of the first sample is that of the first pair. Err with the reason when
/// the record is too short, the step or the centre is no number, or the signal would have more than `max_len` samples.
pub fn iq_to_real(iq: &[f32], dt_pair: f64, fc: f64, max_len: usize) -> Result<(Vec<f32>, f64), String> {
    let pairs = iq.len() / 2;
    if pairs < 16 {
        return Err("an I/Q record needs at least 16 pairs".into());
    }
    if !(dt_pair.is_finite() && dt_pair > 0.0) || !(fc.is_finite() && fc >= 0.0) {
        return Err("an I/Q record needs a time step and a centre frequency".into());
    }
    let top = fc + 0.5 / dt_pair;
    let up = (2.2 * top * dt_pair).ceil().max(2.0);
    let n_out = (pairs as f64 - 1.0) * up + 1.0;
    if n_out > max_len.min(IQ_MAX_SAMPLES) as f64 {
        return Err(format!(
            "a centre of {fc:.0} Hz needs {n_out:.0} samples as a real signal; at most {} are made",
            max_len.min(IQ_MAX_SAMPLES)
        ));
    }
    let (up, n_out) = (up as usize, n_out as usize);
    let (mut i, mut q) = (Vec::with_capacity(pairs), Vec::with_capacity(pairs));
    for pair in iq.chunks_exact(2) {
        i.push(pair[0]);
        q.push(pair[1]);
    }
    let (mut ii, mut qq) = (Vec::new(), Vec::new());
    lanczos8_grid(&i, 0.0, 1.0 / up as f64, n_out, &mut ii);
    lanczos8_grid(&q, 0.0, 1.0 / up as f64, n_out, &mut qq);
    let dt_out = dt_pair / up as f64;
    let w = std::f64::consts::TAU * fc * dt_out;
    let v = (0..n_out)
        .map(|k| {
            let (s, c) = (w * k as f64).sin_cos();
            (ii[k] as f64 * c - qq[k] as f64 * s) as f32
        })
        .collect();
    Ok((v, dt_out))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tone at `fc + df` in the complex signal is a sine at that frequency in the real one, whichever side of the
    /// centre it lies on; the ends, where the interpolation has no neighbours, are left out.
    #[test]
    fn a_tone_of_the_complex_signal_is_a_sine_at_its_frequency() {
        let (dt_pair, fc) = (80e-9, 98e6);
        for df in [1e6, -2.5e6] {
            let iq: Vec<f32> = (0..4000)
                .flat_map(|k| {
                    let ph = std::f64::consts::TAU * df * k as f64 * dt_pair;
                    [ph.cos() as f32, ph.sin() as f32]
                })
                .collect();
            let (v, dt) = iq_to_real(&iq, dt_pair, fc, IQ_MAX_SAMPLES).unwrap();
            let up = (dt_pair / dt).round() as usize;
            assert!(up >= 17 && v.len() == 3999 * up + 1, "up {up} len {}", v.len());
            let worst = (20 * up..v.len() - 20 * up)
                .map(|k| {
                    let t = k as f64 * dt;
                    (v[k] as f64 - (std::f64::consts::TAU * (fc + df) * t).cos()).abs()
                })
                .fold(0.0, f64::max);
            assert!(worst < 0.02, "df {df}: the worst difference from the ideal sine is {worst}");
        }
    }

    /// What cannot be made is refused with its reason: a record that is too short, no step, a centre that needs more
    /// samples than allowed.
    #[test]
    fn what_cannot_be_made_is_refused() {
        let iq = vec![0.5f32; 400];
        assert!(iq_to_real(&iq[..20], 80e-9, 98e6, 1 << 24).is_err());
        assert!(iq_to_real(&iq, 0.0, 98e6, 1 << 24).is_err());
        assert!(iq_to_real(&iq, 80e-9, f64::NAN, 1 << 24).is_err());
        let err = iq_to_real(&iq, 80e-9, 98e6, 1000).unwrap_err();
        assert!(err.contains("samples"), "{err}");
        assert!(iq_to_real(&iq, 80e-9, 0.0, 1 << 24).is_ok());
    }
}
