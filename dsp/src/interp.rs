// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Interpolation between samples for the screen: linear, and windowed sinc (Lanczos) for sparse views where only a
//! few samples fall on the screen (fast time bases). A band-limited signal sampled above twice its bandwidth is
//! reconstructed by the sinc sum; the Lanczos window (a lobes) keeps it local and fast.

/// Linear interpolation at fractional index `x` (None outside the samples or on a non-finite neighbour).
pub fn linear(v: &[f32], x: f64) -> Option<f32> {
    if x < 0.0 || x > (v.len() as f64 - 1.0) || v.len() < 2 {
        return None;
    }
    let i = (x as usize).min(v.len() - 2);
    let fr = (x - i as f64) as f32;
    let (a, b) = (v[i], v[i + 1]);
    (a.is_finite() && b.is_finite()).then_some(a * (1.0 - fr) + b * fr)
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-9 {
        1.0
    } else {
        let p = std::f64::consts::PI * x;
        p.sin() / p
    }
}

/// Lanczos-windowed sinc interpolation with `a` lobes at fractional index `x`. Near the ends the missing neighbours
/// are left out and the kernel weights are renormalised, so the edges stay finite.
pub fn lanczos(v: &[f32], x: f64, a: usize) -> Option<f32> {
    if x < 0.0 || x > (v.len() as f64 - 1.0) || v.len() < 2 {
        return None;
    }
    let i0 = x.floor() as i64;
    let a = a.max(1) as i64;
    let (mut sum, mut wsum) = (0.0f64, 0.0f64);
    for i in (i0 - a + 1)..=(i0 + a) {
        if i < 0 || i >= v.len() as i64 {
            continue;
        }
        let s = v[i as usize];
        if !s.is_finite() {
            return linear(v, x);
        }
        let d = x - i as f64;
        let w = sinc(d) * sinc(d / a as f64);
        sum += w * s as f64;
        wsum += w;
    }
    (wsum.abs() > 1e-12).then(|| (sum / wsum) as f32)
}

/// Phases of the precomputed Lanczos kernel (`lanczos8`).
const PHASES: usize = 1024;
/// Lobes of `lanczos8`.
const A: usize = 8;

/// Kernel table of `lanczos8`: for each phase p (fraction p / PHASES) the 2A weights of the samples i0 - A + 1 ..= i0 + A.
fn table() -> &'static [f32] {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = Vec::with_capacity((PHASES + 1) * 2 * A);
        for p in 0..=PHASES {
            let fr = p as f64 / PHASES as f64;
            for j in 0..2 * A {
                let d = fr - (j as f64 - (A as f64 - 1.0));
                t.push((sinc(d) * sinc(d / A as f64)) as f32);
            }
        }
        t
    })
}

/// The kernel of `table` divided by the sum of its phase's weights: the inner part of `lanczos8` needs no
/// renormalisation per point.
fn table_norm() -> &'static [f32] {
    static T: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        table()
            .as_chunks::<{ 2 * A }>()
            .0
            .iter()
            .flat_map(|w| {
                let s: f64 = w.iter().map(|&x| x as f64).sum();
                w.iter().map(move |&x| (x as f64 / s) as f32)
            })
            .collect()
    })
}

/// The `2 * A` samples of `v` from `first` on, None when one of them is missing.
fn window(v: &[f32], first: i64) -> Option<&[f32; 2 * A]> {
    v.get(usize::try_from(first).ok()?..)?.first_chunk()
}

/// The `2 * A` weights of phase `p` in a kernel table (`table` or `table_norm`).
fn weights(t: &[f32], p: usize) -> &[f32; 2 * A] {
    &t.as_chunks().0[p]
}

/// The 16-term dot product of `lanczos8`, in four partial sums (vector registers; one long chain otherwise).
#[inline(always)]
fn dot16(s: &[f32; 2 * A], w: &[f32; 2 * A]) -> f32 {
    let mut acc = [0.0f32; 4];
    for j in 0..2 * A / 4 {
        for l in 0..4 {
            acc[l] += s[4 * j + l] * w[4 * j + l];
        }
    }
    (acc[0] + acc[1]) + (acc[2] + acc[3])
}

/// `lanczos(v, x, 8)` with a precomputed kernel (1024 phases): no trigonometry per point, for drawing thousands of
/// screen points per record. Same edge handling (missing neighbours left out, weights renormalised).
pub fn lanczos8(v: &[f32], x: f64) -> Option<f32> {
    if x < 0.0 || x > (v.len() as f64 - 1.0) || v.len() < 2 {
        return None;
    }
    let i0 = x.floor() as i64;
    let p = (((x - i0 as f64) * PHASES as f64).round() as usize).min(PHASES);
    let first = i0 - A as i64 + 1;
    if let Some(s) = window(v, first) {
        // inner part: all neighbours present, the weights of the phase sum to 1
        let y = dot16(s, weights(table_norm(), p));
        return if y.is_finite() { Some(y) } else { linear(v, x) };
    }
    let w = weights(table(), p);
    let (mut sum, mut wsum) = (0.0f32, 0.0f32);
    for (j, &wj) in w.iter().enumerate() {
        let i = first + j as i64;
        if i < 0 || i >= v.len() as i64 {
            continue;
        }
        let s = v[i as usize];
        if !s.is_finite() {
            return linear(v, x);
        }
        sum += s * wj;
        wsum += wj;
    }
    (wsum.abs() > 1e-6).then(|| sum / wsum)
}

/// Fraction bits of the fixed-point kernel of `lanczos8_fixed`.
const Q_BITS: u32 = 14;

/// The kernel of `lanczos8` in fixed point: per phase the 2A weights in units of 2^-`Q_BITS`, normalised as
/// `table_norm`'s, the rounding left over added to the tap nearest the point so that they sum to exactly one (a
/// constant signal stays exactly constant).
fn table_fixed() -> &'static [[i16; 2 * A]] {
    static T: std::sync::OnceLock<Vec<[i16; 2 * A]>> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let unit = 1i32 << Q_BITS;
        (0..=PHASES)
            .map(|p| {
                let fr = p as f64 / PHASES as f64;
                let w: [f64; 2 * A] = std::array::from_fn(|j| {
                    let d = fr - (j as f64 - (A as f64 - 1.0));
                    sinc(d) * sinc(d / A as f64)
                });
                let sum: f64 = w.iter().sum();
                let mut t = w.map(|x| (x / sum * unit as f64).round() as i32);
                t[if 2 * p > PHASES { A } else { A - 1 }] += unit - t.iter().sum::<i32>();
                t.map(|x| i16::try_from(x).expect("a tap of the Lanczos kernel fits 16 bits"))
            })
            .collect()
    })
}

/// The 16-term dot product of `lanczos8_fixed`: 16-bit products summed in 32 bits, the halves side by side (the
/// compiler makes widening multiply-adds of it).
#[inline(never)]
fn dot16_fixed(s: &[i16; 2 * A], w: &[i16; 2 * A]) -> i32 {
    let mut acc = [0i32; A];
    for j in 0..A {
        acc[j] = s[j] as i32 * w[j] as i32 + s[j + A] as i32 * w[j + A] as i32;
    }
    acc.iter().sum()
}

/// `dot16_fixed` of two pairs side by side (their multiply-add chains overlap on an in-order core).
#[inline(never)]
fn dot16_fixed2(s: [&[i16; 2 * A]; 2], w: [&[i16; 2 * A]; 2]) -> [i32; 2] {
    let mut acc = [[0i32; A]; 2];
    for j in 0..A {
        for q in 0..2 {
            acc[q][j] = s[q][j] as i32 * w[q][j] as i32 + s[q][j + A] as i32 * w[q][j + A] as i32;
        }
    }
    acc.map(|a| a.iter().sum())
}

/// Fraction bits of the positions of `lanczos8_fixed` (in samples).
const POS_BITS: u32 = 36;

thread_local! {
    /// the samples of `lanczos8_fixed` in fixed point
    static FIXED: std::cell::RefCell<Vec<i16>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// `lanczos8` at `x0 + k * step` for k in 0..n onto `out` (NaN where it gives None), in fixed point: the positions in
/// steps of 2^-36 samples, the samples the points read as 16-bit steps of their range, the kernel in steps of 2^-14
/// (`table_fixed`), the sums in 32 bits, and the points between the same two samples share their 16 neighbours.
/// Within about 1e-4 of the samples' range of `lanczos8` (the test bounds it), not the same bits: less than half its
/// cost on the scope's small cores, where the sparse frames are drawn. Points near the ends of the record, where
/// neighbours are missing, are `lanczos8`'s own, and so are all points when a sample they read is not finite.
pub fn lanczos8_fixed(v: &[f32], x0: f64, step: f64, n: usize, out: &mut Vec<f32>) {
    out.reserve(n);
    let unit = (1u64 << POS_BITS) as f64;
    let (p0, dp) = ((x0 * unit).round(), (step * unit).round());
    let pos = |k: usize| p0 as i64 + dp as i64 * k as i64;
    // where the fixed point does not apply: `lanczos8` at the grid's points, as the grid in floating point gives them
    let exact = |k: usize| lanczos8(v, x0 + k as f64 * step).unwrap_or(f32::NAN);
    // the inner points: all 2A neighbours there (A - 1 <= x < len - A); positions within range of i64, forward
    let fits = p0.abs() < 1e18 && dp > 0.0 && dp * (n as f64) < 1e18 && v.len() >= 2 * A;
    let lo = ((A - 1) as i64) << POS_BITS;
    // (a record shorter than the kernel takes `lanczos8` below: `fits`)
    let hi = (v.len().saturating_sub(A) as i64) << POS_BITS;
    let k0 = if !fits || p0 as i64 >= lo { 0 } else { ((lo - p0 as i64) as u64).div_ceil(dp as u64) as usize };
    let k1 = if fits && hi > p0 as i64 { (((hi - p0 as i64) as u64).div_ceil(dp as u64) as usize).min(n) } else { 0 };
    if !fits || k0 >= k1 {
        out.extend((0..n).map(exact));
        return;
    }
    let a = (pos(k0) >> POS_BITS) as usize + 1 - A;
    let b = (pos(k1 - 1) >> POS_BITS) as usize + 1 + A;
    let read = &v[a..b];
    let (lo_v, hi_v) = read.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(l, h), &y| (l.min(y), h.max(y)));
    // (`min` and `max` pass a NaN by, so each sample is looked at)
    if !read.iter().all(|y| y.is_finite()) {
        out.extend((0..n).map(exact));
        return;
    }
    let (mid, half) = ((lo_v as f64 + hi_v as f64) / 2.0, (hi_v as f64 - lo_v as f64) / 2.0);
    let scale = if half > 0.0 { 32767.0 / half } else { 1.0 };
    let back = (1.0 / (scale * (1i32 << Q_BITS) as f64)) as f32;
    let tf = table_fixed();
    out.extend((0..k0).map(exact));
    FIXED.with(|q| {
        let mut q = q.borrow_mut();
        q.clear();
        q.extend(read.iter().map(|&y| ((y as f64 - mid) * scale).round() as i16));
        // the phase: the position's fraction rounded to 1 / PHASES
        let shift = POS_BITS - PHASES.trailing_zeros();
        let (mid, frac, half_phase) = (mid as f32, (1i64 << POS_BITS) - 1, 1i64 << (shift - 1));
        let (mut p, dp) = (pos(k0), dp as i64);
        let at = |p: i64| -> (&[i16; 2 * A], usize) {
            let i = (p >> POS_BITS) as usize;
            let s = q[i + 1 - A - a..].first_chunk().expect("an inner point has all its neighbours");
            (s, (((p & frac) + half_phase) >> shift) as usize)
        };
        let mut k = k0;
        while k + 2 <= k1 {
            let ((sa, pa), (sb, pb)) = (at(p), at(p + dp));
            let [ya, yb] = dot16_fixed2([sa, sb], [&tf[pa], &tf[pb]]);
            out.push(mid + ya as f32 * back);
            out.push(mid + yb as f32 * back);
            p += 2 * dp;
            k += 2;
        }
        if k < k1 {
            let (s, ph) = at(p);
            out.push(mid + dot16_fixed(s, &tf[ph]) as f32 * back);
        }
    });
    out.extend((k1..n).map(exact));
}

#[cfg(test)]
mod tests {
    use super::*;

    // Break it catches: a fixed-point grid off `lanczos8` by more than its rounding (a wrong phase, tap, scale or
    // sample window), points near the ends or over a gap that are not `lanczos8`'s own, a constant that does not stay
    // exactly constant.
    #[test]
    fn the_fixed_grid_is_lanczos8_within_its_rounding() {
        let v: Vec<f32> = (0..300).map(|k| (k as f32 * 0.37).sin() + 0.1 * (k as f32 * 2.9).cos()).collect();
        let range = 2.2f32;
        for (x0, step) in [(-5.3, 0.0917), (3.1, 0.013), (100.0, 0.0371), (290.5, 0.002), (-40.0, 0.25)] {
            let n = 3600;
            let mut g = Vec::new();
            lanczos8_fixed(&v, x0, step, n, &mut g);
            assert_eq!(g.len(), n);
            for (k, &y) in g.iter().enumerate() {
                let x = x0 + k as f64 * step;
                let want = lanczos8(&v, x).unwrap_or(f32::NAN);
                // inside within the rounding; at the ends `lanczos8`'s own points (a point at the border may fall
                // on either side by the rounding of its position to 2^-36 samples)
                assert!((y - want).abs() < 4e-4 * range || y.is_nan() && want.is_nan(), "x {x}: {y} vs {want}");
                if !(6.0..v.len() as f64 - 7.0).contains(&x) {
                    assert!((y - want).abs() < 1e-6 || y.is_nan() && want.is_nan(), "end x {x}: {y} vs {want}");
                }
            }
        }
        // a sample that is not finite among those read: all points `lanczos8`'s
        let mut holed = v.clone();
        holed[150] = f32::NAN;
        let (mut g, mut want) = (Vec::new(), Vec::new());
        lanczos8_fixed(&holed, 100.0, 0.0371, 3600, &mut g);
        want.extend((0..3600).map(|k| lanczos8(&holed, 100.0 + k as f64 * 0.0371).unwrap_or(f32::NAN)));
        let bad = g.iter().zip(&want).position(|(a, b)| !((a - b).abs() < 1e-5 || a.is_nan() && b.is_nan()));
        assert!(bad.is_none(), "{bad:?} {:?}", bad.map(|k| (g[k], want[k])));
        // records shorter than the kernel: `lanczos8`'s points
        for len in 1..2 * A {
            let short: Vec<f32> = (0..len).map(|k| (k as f32 * 0.9).sin()).collect();
            g.clear();
            lanczos8_fixed(&short, -0.5, 0.01, 900, &mut g);
            for (k, &y) in g.iter().enumerate() {
                let want = lanczos8(&short, -0.5 + k as f64 * 0.01).unwrap_or(f32::NAN);
                assert!(y.to_bits() == want.to_bits() || y.is_nan() && want.is_nan(), "len {len} k {k}");
            }
        }
        // a constant stays exactly constant (the taps of each phase sum to one)
        let flat = vec![0.731f32; 64];
        g.clear();
        lanczos8_fixed(&flat, 7.0, 0.0173, 2000, &mut g);
        assert!(g.iter().all(|&y| y == 0.731 || y.is_nan()), "{:?}", g.iter().find(|&&y| y != 0.731));
    }

    #[test]
    fn sinc_reconstructs_a_sparse_sine_much_better_than_linear() {
        // 2.5 samples per period (0.4 of the sampling rate), like a 500 MHz sine at 1.25 GSa/s
        let f = 0.4;
        let v: Vec<f32> = (0..200).map(|k| (2.0 * std::f64::consts::PI * f * k as f64).sin() as f32).collect();
        let (mut el, mut es) = (0.0f64, 0.0f64);
        let mut n = 0;
        let mut x = 50.0;
        while x < 150.0 {
            let truth = (2.0 * std::f64::consts::PI * f * x).sin();
            el = el.max((linear(&v, x).unwrap() as f64 - truth).abs());
            es = es.max((lanczos(&v, x, 8).unwrap() as f64 - truth).abs());
            x += 0.07;
            n += 1;
        }
        assert!(n > 1000);
        assert!(el > 0.5, "linear error {}", el);
        assert!(es < 0.08, "sinc error {}", es);
    }

    // Issue #335 step 1. Break it catches: a kernel (fewer lobes, a coarser phase table) that rings on the signals Auto
    // bandwidth lets through. Auto puts the nominal filter corner near a quarter of the channel's rate
    // (`hal::dho::afe::auto_c`), so a sine up to 0.25 of the rate must come back almost exactly, and one up to 0.4 (the
    // filter's slope) still close; the error is the largest over many phases between the samples.
    #[test]
    fn the_screen_kernel_stays_clean_up_to_the_auto_corner() {
        let worst = |f: f64| {
            let v: Vec<f32> =
                (0..400).map(|k| (2.0 * std::f64::consts::PI * f * k as f64 + 0.3).sin() as f32).collect();
            let mut out = Vec::new();
            lanczos8_fixed(&v, 100.0, 0.0371, 5000, &mut out);
            out.iter()
                .enumerate()
                .map(|(k, &y)| {
                    (y as f64 - (2.0 * std::f64::consts::PI * f * (100.0 + k as f64 * 0.0371) + 0.3).sin()).abs()
                })
                .fold(0.0, f64::max)
        };
        // host, 2026-11-21: at most 0.0023 up to 0.25, 0.0076 at 0.35, 0.023 at 0.4. The bound up to the corner
        // (-48 dB of the amplitude) stays under the scope's own noise (SINAD 43.4 dB, docs/models.md 4.1).
        for (f, bound) in [(0.01, 0.004), (0.05, 0.004), (0.1, 0.004), (0.15, 0.004), (0.2, 0.004), (0.25, 0.004)] {
            assert!(worst(f) < bound, "{} of the rate: error {}", f, worst(f));
        }
        for (f, bound) in [(0.3, 0.01), (0.35, 0.015), (0.4, 0.04)] {
            assert!(worst(f) < bound, "{} of the rate: error {}", f, worst(f));
        }
    }

    #[test]
    fn table_kernel_matches_the_exact_one() {
        let v: Vec<f32> = (0..300).map(|k| ((k as f64 * 0.37).sin() + 0.3 * (k as f64 * 1.9).cos()) as f32).collect();
        let mut x = 0.0;
        while x < 299.0 {
            let (a, b) = (lanczos(&v, x, 8).unwrap(), lanczos8(&v, x).unwrap());
            assert!((a - b).abs() < 2e-3, "x {} exact {} table {}", x, a, b);
            x += 0.013;
        }
        assert_eq!(lanczos8(&v, -0.5), None);
    }

    #[test]
    fn exact_at_samples_and_safe_at_the_edges() {
        let v = [0.0f32, 1.0, 0.5, -0.25, 2.0];
        for (i, &s) in v.iter().enumerate() {
            assert!((lanczos(&v, i as f64, 4).unwrap() - s).abs() < 1e-6);
        }
        assert!(lanczos(&v, 0.3, 4).unwrap().is_finite() && lanczos(&v, 3.9, 4).unwrap().is_finite());
        assert_eq!(lanczos(&v, -0.1, 4), None);
        assert_eq!(lanczos(&v, 4.1, 4), None);
        let w = [0.0f32, f32::NAN, 1.0];
        assert_eq!(lanczos(&w, 0.5, 4), None); // falls back to linear, which refuses the NaN neighbour
    }
}
