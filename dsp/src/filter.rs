// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the filter designer. A filter is a kind (low-pass, high-pass, band-pass, band-stop), a response
//! (Butterworth, Chebyshev, Bessel, Gaussian, raised cosine, root raised cosine, all-pass delay), an order and the
//! cut-off frequencies, designed for a sample step `dt`. The result is either a cascade of biquads (the IIR
//! responses: bilinear transform with the cut-off pre-warped, so the -3 dB point lies at the frequency asked for) or
//! a list of taps (the FIR responses), and it can be applied to a record and asked for its own response: magnitude,
//! phase, impulse and step, which is what a designer shows before the filter is applied.
//!
//! Band-pass is a high-pass at the lower and a low-pass at the upper cut-off in series; band-stop is a low-pass at the
//! lower and a high-pass at the upper cut-off in parallel (assumed to be what a user of a scope expects; the sum of the
//! two is flat away from the band only when their skirts do not overlap).

use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Lowpass,
    Highpass,
    Bandpass,
    Bandstop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Response {
    Butterworth,
    /// Chebyshev type I, the ripple in `Spec::ripple_db`
    Chebyshev,
    /// maximally flat group delay, -3 dB at the cut-off
    Bessel,
    /// FIR with a Gaussian magnitude, -3 dB at the cut-off
    Gaussian,
    /// raised cosine, `Spec::symbol` seconds per symbol, `Spec::rolloff`
    RaisedCosine,
    RootRaisedCosine,
    /// a delay of `Spec::delay` seconds, unit magnitude
    Allpass,
}

#[derive(Clone, Copy, Debug)]
pub struct Spec {
    pub kind: Kind,
    pub response: Response,
    /// IIR: the order of each pass; Gaussian: ignored
    pub order: usize,
    /// cut-off (low-pass, high-pass) or lower edge (band filters), Hz
    pub f1: f64,
    /// upper edge of a band filter, Hz
    pub f2: f64,
    pub ripple_db: f64,
    pub symbol: f64,
    pub rolloff: f64,
    pub delay: f64,
}

impl Default for Spec {
    fn default() -> Spec {
        Spec {
            kind: Kind::Lowpass,
            response: Response::Butterworth,
            order: 4,
            f1: 1e6,
            f2: 2e6,
            ripple_db: 1.0,
            symbol: 1e-6,
            rolloff: 0.35,
            delay: 0.0,
        }
    }
}

/// A biquad, (b0, b1, b2, a1, a2) with a0 = 1.
type Sos = [f64; 5];

/// A designed filter: `branches` are summed; each branch is a cascade of biquads, a gain, or FIR taps.
#[derive(Clone, Debug)]
pub struct Filter {
    branches: Vec<Branch>,
    pub dt: f64,
}

#[derive(Clone, Debug)]
enum Branch {
    Iir(Vec<Sos>),
    /// taps and the index of the tap that is time zero (the output is aligned with the input)
    Fir(Vec<f64>, usize),
}

/// The poles of the analog low-pass prototype of order `n` with its -3 dB point at 1 rad/s (left half plane,
/// upper half and the real pole, as pairs `(re, im)` with `im >= 0`) and, for Chebyshev, nothing else is needed.
fn prototype(resp: Response, n: usize, ripple_db: f64) -> Vec<(f64, f64)> {
    let n = n.clamp(1, 16);
    match resp {
        Response::Chebyshev => {
            let eps = (10f64.powf(ripple_db.clamp(0.01, 6.0) / 10.0) - 1.0).sqrt();
            let a = (1.0 / eps).asinh() / n as f64;
            let poles: Vec<(f64, f64)> = (0..n)
                .map(|k| {
                    let th = PI * (2 * k + 1) as f64 / (2 * n) as f64;
                    (-a.sinh() * th.sin(), a.cosh() * th.cos())
                })
                .filter(|p| p.1 >= -1e-12)
                .collect();
            // the cut-off of the Chebyshev is its ripple edge; move the -3 dB point to 1 rad/s
            let w3 = ((1.0 / eps).acosh() / n as f64).cosh();
            poles.into_iter().map(|(r, i)| (r / w3, i / w3)).collect()
        }
        Response::Bessel => {
            let p = bessel_poles(n);
            let w3 = bessel_w3(&p, n);
            p.into_iter().filter(|p| p.1 >= -1e-12).map(|(r, i)| (r / w3, i / w3)).collect()
        }
        _ => (0..n)
            .map(|k| {
                let th = PI * (2 * k + 1) as f64 / (2 * n) as f64;
                (-th.sin(), th.cos())
            })
            .filter(|p| p.1 >= -1e-12)
            .collect(),
    }
}

/// The roots of the reverse Bessel polynomial of order `n` (Durand-Kerner), the poles of the Bessel filter with unit
/// delay at DC.
fn bessel_poles(n: usize) -> Vec<(f64, f64)> {
    // coefficients a_k = (2n-k)! / (2^(n-k) k! (n-k)!) of s^k
    let fact = |m: usize| (1..=m).fold(1.0f64, |a, b| a * b as f64);
    let c: Vec<f64> = (0..=n).map(|k| fact(2 * n - k) / (2f64.powi((n - k) as i32) * fact(k) * fact(n - k))).collect();
    // polynomial sum a_k s^k; make it monic
    let lead = c[n];
    let coef: Vec<f64> = (0..=n).map(|k| c[k] / lead).collect(); // lowest power first
    let mut z: Vec<(f64, f64)> = Vec::new();
    let mut cur = (1.0f64, 0.0f64);
    for _ in 0..n {
        cur = (cur.0 * 0.4 - cur.1 * 0.9, cur.0 * 0.9 + cur.1 * 0.4);
        z.push(cur);
    }
    let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
    let div = |a: (f64, f64), b: (f64, f64)| {
        let d = b.0 * b.0 + b.1 * b.1;
        ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
    };
    let eval = |x: (f64, f64)| {
        let mut acc = (0.0, 0.0);
        for k in (0..=n).rev() {
            let m = mul(acc, x);
            acc = (m.0 + coef[k], m.1);
        }
        acc
    };
    for _ in 0..500 {
        let mut moved = 0.0f64;
        for i in 0..n {
            let mut den = (1.0, 0.0);
            for j in 0..n {
                if i != j {
                    den = mul(den, (z[i].0 - z[j].0, z[i].1 - z[j].1));
                }
            }
            let d = div(eval(z[i]), den);
            z[i] = (z[i].0 - d.0, z[i].1 - d.1);
            moved = moved.max(d.0.abs() + d.1.abs());
        }
        if moved < 1e-13 {
            break;
        }
    }
    z
}

/// The frequency, rad/s, at which the all-pole filter with poles `p` (all of them, conjugates included, or the upper
/// half) has fallen by 3 dB, by bisection.
fn bessel_w3(p: &[(f64, f64)], n: usize) -> f64 {
    let all: Vec<(f64, f64)> = p.to_vec();
    let gain = |w: f64| {
        let mut g = 1.0f64;
        let mut dc = 1.0f64;
        for q in &all {
            dc *= (q.0 * q.0 + q.1 * q.1).sqrt();
            g *= ((q.0 * q.0) + (w - q.1) * (w - q.1)).sqrt();
        }
        let _ = n;
        dc / g
    };
    let (mut lo, mut hi) = (0.0f64, 50.0f64);
    for _ in 0..80 {
        let mid = 0.5 * (lo + hi);
        if gain(mid) > std::f64::consts::FRAC_1_SQRT_2 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// The biquads of a low-pass or high-pass of the given response at the cut-off `fc`, sample step `dt`: the prototype
/// poles scaled to the pre-warped cut-off (a high-pass takes the reciprocal poles, and zeros at DC), each pair or real
/// pole as an analog section made a biquad by the bilinear transform.
fn iir_pass(resp: Response, high: bool, n: usize, fc: f64, ripple: f64, dt: f64) -> Vec<Sos> {
    let fc = fc.clamp(1e-9 / dt, 0.499 / dt);
    let wc = 2.0 / dt * (PI * fc * dt).tan();
    let k = 2.0 / dt;
    let mut out = Vec::new();
    for (re, im) in prototype(resp, n, ripple) {
        let m2 = re * re + im * im;
        let (qr, qi) = if high { (re / m2, -im / m2) } else { (re, im) };
        let real = im.abs() < 1e-9;
        // analog section (c0 s^2 + c1 s + c2) / (d0 s^2 + d1 s + d2)
        let (num, den) = if real {
            let r = qr * wc;
            (if high { [0.0, 1.0, 0.0] } else { [0.0, 0.0, -r] }, [0.0, 1.0, -r])
        } else {
            let q2 = (qr * qr + qi * qi) * wc * wc;
            (if high { [1.0, 0.0, 0.0] } else { [0.0, 0.0, q2] }, [1.0, -2.0 * qr * wc, q2])
        };
        let z = |c: [f64; 3]| -> [f64; 3] {
            [c[0] * k * k + c[1] * k + c[2], -2.0 * c[0] * k * k + 2.0 * c[2], c[0] * k * k - c[1] * k + c[2]]
        };
        let (b, d) = (z(num), z(den));
        out.push([b[0] / d[0], b[1] / d[0], b[2] / d[0], d[1] / d[0], d[2] / d[0]]);
    }
    // the Chebyshev of even order passes DC (and, as a high-pass, the top) at the bottom of its ripple
    if resp == Response::Chebyshev && n.clamp(1, 16).is_multiple_of(2) {
        let g = 1.0 / (1.0 + (10f64.powf(ripple.clamp(0.01, 6.0) / 10.0) - 1.0)).sqrt();
        if let Some(first) = out.first_mut() {
            for v in &mut first[..3] {
                *v *= g;
            }
        }
    }
    out
}

fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

fn hann(i: usize, n: usize) -> f64 {
    0.5 - 0.5 * (2.0 * PI * i as f64 / (n - 1).max(1) as f64).cos()
}

/// A windowed-sinc low-pass or the like as taps; `centre` the index of time zero.
fn fir(spec: &Spec, dt: f64) -> (Vec<f64>, usize) {
    let sps = (spec.symbol / dt).max(1.0);
    let (taps, centre): (Vec<f64>, usize) = match spec.response {
        Response::Gaussian => {
            let sigma = (2f64.ln()).sqrt() / (2.0 * PI * spec.f1.max(1.0)) / dt;
            let half = (4.0 * sigma).ceil().max(2.0) as usize;
            ((0..=2 * half).map(|i| (-0.5 * ((i as f64 - half as f64) / sigma).powi(2)).exp()).collect(), half)
        }
        Response::RaisedCosine | Response::RootRaisedCosine => {
            let b = spec.rolloff.clamp(0.0, 1.0);
            let half = ((6.0 * sps).ceil() as usize).clamp(2, 2048);
            let rrc = spec.response == Response::RootRaisedCosine;
            let h = |t: f64| -> f64 {
                // t in symbols
                if rrc {
                    if t.abs() < 1e-9 {
                        1.0 - b + 4.0 * b / PI
                    } else if b > 0.0 && (t.abs() - 1.0 / (4.0 * b)).abs() < 1e-9 {
                        b / 2f64.sqrt()
                            * ((1.0 + 2.0 / PI) * (PI / (4.0 * b)).sin() + (1.0 - 2.0 / PI) * (PI / (4.0 * b)).cos())
                    } else {
                        let a = PI * t;
                        ((a * (1.0 - b)).sin() + 4.0 * b * t * (a * (1.0 + b)).cos())
                            / (a * (1.0 - (4.0 * b * t).powi(2)))
                    }
                } else if b > 0.0 && ((2.0 * b * t).abs() - 1.0).abs() < 1e-9 {
                    PI / 4.0 * sinc(1.0 / (2.0 * b))
                } else {
                    sinc(t) * (PI * b * t).cos() / (1.0 - (2.0 * b * t).powi(2))
                }
            };
            ((0..=2 * half).map(|i| h((i as f64 - half as f64) / sps) * hann(i, 2 * half + 1)).collect(), half)
        }
        Response::Allpass => {
            // a delay by a windowed-sinc fractional step; whole samples are a shift
            let d = (spec.delay / dt).max(0.0);
            let whole = d.floor() as usize;
            let frac = d - d.floor();
            let half = 16usize;
            let mut t = vec![0.0; whole + 2 * half + 2];
            for (i, tap) in t.iter_mut().enumerate() {
                let x = i as f64 - (whole + half) as f64 - frac;
                // the window is centred on the peak of the sinc, whatever the delay
                *tap = sinc(x)
                    * (0.5 + 0.5 * (PI * x / (half as f64 + 1.0)).cos())
                    * (x.abs() < half as f64 + 1.0) as u8 as f64;
            }
            let s: f64 = t.iter().sum();
            return (t.iter().map(|v| v / s).collect(), half);
        }
        _ => (vec![1.0], 0),
    };
    let s: f64 = taps.iter().sum();
    let s = if s.abs() > 1e-12 { s } else { 1.0 };
    (taps.iter().map(|v| v / s).collect(), centre)
}

impl Filter {
    /// Designs `spec` for the sample step `dt`.
    pub fn design(spec: &Spec, dt: f64) -> Filter {
        let n = spec.order.clamp(1, 16);
        let pass = |high: bool, f: f64| Branch::Iir(iir_pass(spec.response, high, n, f, spec.ripple_db, dt));
        let fir_like = matches!(
            spec.response,
            Response::Gaussian | Response::RaisedCosine | Response::RootRaisedCosine | Response::Allpass
        );
        let branches = if fir_like {
            let (t, c) = fir(spec, dt);
            let t = if spec.response == Response::Gaussian && matches!(spec.kind, Kind::Highpass) {
                // the complement of the low-pass
                t.iter().enumerate().map(|(i, v)| if i == c { 1.0 - v } else { -v }).collect()
            } else {
                t
            };
            vec![Branch::Fir(t, c)]
        } else {
            match spec.kind {
                Kind::Lowpass => vec![pass(false, spec.f1)],
                Kind::Highpass => vec![pass(true, spec.f1)],
                Kind::Bandpass => {
                    let (Branch::Iir(mut a), Branch::Iir(b)) = (pass(true, spec.f1), pass(false, spec.f2)) else {
                        unreachable!()
                    };
                    a.extend(b);
                    vec![Branch::Iir(a)]
                }
                Kind::Bandstop => vec![pass(false, spec.f1), pass(true, spec.f2)],
            }
        };
        Filter { branches, dt }
    }

    /// A filter of taps of its own, `h` sampled every `dt_taps` s (a filter file), for a record sampled every `dt` s: the
    /// impulse response is read at the new step by linear interpolation and scaled by `dt / dt_taps`, so the gain of
    /// the filter stays. Its centre is the middle tap, so a symmetric filter keeps the record in place. None when the
    /// steps are no numbers, `h` has fewer than two taps, or it would have more than `MAX_CUSTOM_TAPS` at `dt`.
    pub fn from_taps(h: &[f32], dt_taps: f64, dt: f64) -> Option<Filter> {
        if h.len() < 2 || !(dt_taps.is_finite() && dt_taps > 0.0 && dt.is_finite() && dt > 0.0) {
            return None;
        }
        let ratio = dt / dt_taps;
        let n = (((h.len() - 1) as f64 / ratio).ceil() as usize).saturating_add(1);
        if n > MAX_CUSTOM_TAPS {
            return None;
        }
        let taps: Vec<f64> = (0..n)
            .map(|i| {
                let x = i as f64 * ratio;
                let k = (x.floor() as usize).min(h.len() - 1);
                let f = x - k as f64;
                let next = h.get(k + 1).copied().unwrap_or(h[k]) as f64;
                ((1.0 - f) * h[k] as f64 + f * next) * ratio
            })
            .collect();
        Some(Filter { branches: vec![Branch::Fir(taps, n / 2)], dt })
    }

    /// The record `x` (sampled every `dt` s) filtered by taps of a file (`h` sampled every `dt_taps` s). When the taps
    /// at the step of the record would be too many (a filter made for a slow record, a fast record), the record is
    /// brought to the step of the filter by averaging, filtered there, and read back at its own step by linear
    /// interpolation: what the filter passes lies far below the record's rate. None when the steps are no numbers or
    /// the taps are fewer than two.
    pub fn apply_taps(x: &[f32], dt: f64, h: &[f32], dt_taps: f64) -> Option<Vec<f32>> {
        if let Some(f) = Filter::from_taps(h, dt_taps, dt) {
            return Some(f.apply(x));
        }
        if !(dt > 0.0 && dt < dt_taps && dt.is_finite() && dt_taps.is_finite()) || x.len() < 2 {
            return None;
        }
        let ratio = dt_taps / dt;
        let n = ((x.len() as f64 / ratio).ceil() as usize).max(2);
        // the averages of `ratio` samples: sample i of the slow record is the mean of those around (i + 0.5) * ratio
        let slow: Vec<f32> = (0..n)
            .map(|i| {
                let (a, b) = ((i as f64 * ratio) as usize, (((i + 1) as f64 * ratio) as usize).min(x.len()));
                let part = &x[a.min(x.len() - 1)..b.max(a + 1).min(x.len())];
                part.iter().sum::<f32>() / part.len().max(1) as f32
            })
            .collect();
        let y = Filter::from_taps(h, dt_taps, dt_taps)?.apply(&slow);
        Some(
            (0..x.len())
                .map(|j| {
                    let pos = (j as f64 / ratio - 0.5).clamp(0.0, (n - 1) as f64);
                    let k = (pos.floor() as usize).min(n - 1);
                    let f = (pos - k as f64) as f32;
                    y[k] + (y[(k + 1).min(n - 1)] - y[k]) * f
                })
                .collect(),
        )
    }

    /// The length of the longest FIR branch in taps, 0 for an IIR filter.
    pub fn taps(&self) -> usize {
        self.branches.iter().map(|b| if let Branch::Fir(t, _) = b { t.len() } else { 0 }).max().unwrap_or(0)
    }

    /// The filtered record. A FIR filter is aligned with its input (its group delay is taken out), an IIR one is
    /// causal; the start is primed with the first sample so a level does not start a step.
    pub fn apply(&self, x: &[f32]) -> Vec<f32> {
        let mut sum = vec![0.0f64; x.len()];
        for b in &self.branches {
            let y = match b {
                Branch::Iir(s) => run_sos(s, x),
                Branch::Fir(t, c) => run_fir(t, *c, x),
            };
            for (a, v) in sum.iter_mut().zip(y) {
                *a += v;
            }
        }
        sum.into_iter().map(|v| v as f32).collect()
    }

    /// The complex gain at `f` Hz.
    pub fn gain(&self, f: f64) -> (f64, f64) {
        let w = 2.0 * PI * f * self.dt;
        let (c1, s1, c2, s2) = (w.cos(), -w.sin(), (2.0 * w).cos(), -(2.0 * w).sin());
        let mul = |a: (f64, f64), b: (f64, f64)| (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0);
        let div = |a: (f64, f64), b: (f64, f64)| {
            let d = b.0 * b.0 + b.1 * b.1;
            ((a.0 * b.0 + a.1 * b.1) / d, (a.1 * b.0 - a.0 * b.1) / d)
        };
        let mut total = (0.0, 0.0);
        for b in &self.branches {
            let h = match b {
                Branch::Iir(s) => s.iter().fold((1.0, 0.0), |acc, k| {
                    let num = (k[0] + k[1] * c1 + k[2] * c2, k[1] * s1 + k[2] * s2);
                    let den = (1.0 + k[3] * c1 + k[4] * c2, k[3] * s1 + k[4] * s2);
                    mul(acc, div(num, den))
                }),
                Branch::Fir(t, c) => {
                    let mut h = (0.0, 0.0);
                    for (i, v) in t.iter().enumerate() {
                        let a = w * (i as f64 - *c as f64);
                        h = (h.0 + v * a.cos(), h.1 - v * a.sin());
                    }
                    h
                }
            };
            total = (total.0 + h.0, total.1 + h.1);
        }
        total
    }

    /// Magnitude in dB and phase in degrees at `n` frequencies from `lo` to `hi` Hz (logarithmic when `log`).
    pub fn response(&self, lo: f64, hi: f64, n: usize, log: bool) -> Vec<(f64, f64, f64)> {
        (0..n)
            .map(|i| {
                let u = i as f64 / (n - 1).max(1) as f64;
                let f = if log { lo * (hi / lo).powf(u) } else { lo + (hi - lo) * u };
                let (re, im) = self.gain(f);
                (f, 20.0 * (re.hypot(im)).max(1e-12).log10(), im.atan2(re).to_degrees())
            })
            .collect()
    }

    /// The response to a unit impulse, `n` samples.
    pub fn impulse(&self, n: usize) -> Vec<f32> {
        let mut x = vec![0.0f32; n];
        if n > 0 {
            x[n.min(4) / 2] = 1.0;
        }
        self.apply_unprimed(&x)
    }

    /// The response to a unit step starting a quarter into the `n` samples.
    pub fn step(&self, n: usize) -> Vec<f32> {
        let x: Vec<f32> = (0..n).map(|i| if i >= n / 4 { 1.0 } else { 0.0 }).collect();
        self.apply_unprimed(&x)
    }

    fn apply_unprimed(&self, x: &[f32]) -> Vec<f32> {
        // a zero start needs no priming
        self.apply(x)
    }
}

fn run_sos(s: &[Sos], x: &[f32]) -> Vec<f64> {
    let mut y: Vec<f64> = x.iter().map(|&v| v as f64).collect();
    for k in s {
        let x0 = y.first().copied().unwrap_or(0.0);
        let g = (k[0] + k[1] + k[2]) / (1.0 + k[3] + k[4]);
        let (mut x1, mut x2, mut y1, mut y2) = (x0, x0, x0 * g, x0 * g);
        for v in y.iter_mut() {
            let o = k[0] * *v + k[1] * x1 + k[2] * x2 - k[3] * y1 - k[4] * y2;
            (x2, x1, y2, y1) = (x1, *v, y1, o);
            *v = o;
        }
    }
    y
}

/// The most taps a filter of a file may have at the sample step of the record it is applied to.
pub const MAX_CUSTOM_TAPS: usize = 1 << 17;

/// The taps below which the direct sum is cheaper than the transform.
const FFT_TAPS: usize = 48;

/// Output `i` takes input `i + c - j` for each tap `j`, the edges repeat the end samples. Long filters (a raised cosine
/// or a delay at a fast sample rate has thousands of taps) go by overlap-save with the FFT, in blocks of a few times
/// their length, so the work is that of a few transforms a sample and the memory does not grow with the record.
fn run_fir(t: &[f64], c: usize, x: &[f32]) -> Vec<f64> {
    if t.len() <= FFT_TAPS || x.len() < 4 * t.len() {
        return run_fir_direct(t, c, x);
    }
    let (n, l) = (x.len(), t.len());
    let nfft = (4 * l).next_power_of_two().max(1024);
    let step = nfft - (l - 1);
    let (mut hr, mut hi) = (vec![0.0; nfft], vec![0.0; nfft]);
    hr[..l].copy_from_slice(t);
    crate::math::fft(&mut hr, &mut hi);
    let u = |m: isize| x[m.clamp(0, n as isize - 1) as usize] as f64;
    let mut out = Vec::with_capacity(n);
    let (mut re, mut im) = (vec![0.0; nfft], vec![0.0; nfft]);
    for s in (0..n).step_by(step) {
        // the block of the extended input that starts l - 1 before the first output of this step
        let a = s as isize + c as isize - (l as isize - 1);
        for k in 0..nfft {
            re[k] = u(a + k as isize);
            im[k] = 0.0;
        }
        crate::math::fft(&mut re, &mut im);
        for k in 0..nfft {
            let (r, i) = (re[k] * hr[k] - im[k] * hi[k], re[k] * hi[k] + im[k] * hr[k]);
            // the inverse transform by conjugating on the way in and out
            (re[k], im[k]) = (r, -i);
        }
        crate::math::fft(&mut re, &mut im);
        for k in (l - 1)..nfft {
            if out.len() == n {
                break;
            }
            out.push(re[k] / nfft as f64);
        }
    }
    out
}

fn run_fir_direct(t: &[f64], c: usize, x: &[f32]) -> Vec<f64> {
    let n = x.len();
    (0..n)
        .map(|i| {
            let mut a = 0.0;
            for (j, tap) in t.iter().enumerate() {
                // output i takes input i + c - j, the edges repeat the end samples
                let k = (i as isize + c as isize - j as isize).clamp(0, n as isize - 1) as usize;
                a += tap * x[k] as f64;
            }
            a
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db_at(f: &Filter, hz: f64) -> f64 {
        let (re, im) = f.gain(hz);
        20.0 * re.hypot(im).log10()
    }

    /// Butterworth, Chebyshev and Bessel low-passes have their -3 dB point at the cut-off, DC gain one and fall off
    /// the faster the higher the order.
    #[test]
    fn the_iir_low_passes_are_three_db_down_at_the_cut_off() {
        let dt = 1e-8;
        for resp in [Response::Butterworth, Response::Chebyshev, Response::Bessel] {
            for order in [2, 3, 5, 8] {
                let s = Spec { response: resp, order, f1: 1e6, ripple_db: 1.0, ..Spec::default() };
                let f = Filter::design(&s, dt);
                let at_cut = db_at(&f, 1e6);
                let tol = 0.15;
                assert!((at_cut + 3.01).abs() < tol, "{resp:?} order {order}: {at_cut} dB at the cut-off");
                if resp != Response::Chebyshev {
                    assert!(db_at(&f, 1e3).abs() < 0.05, "{resp:?} {order}: DC gain");
                }
                assert!(db_at(&f, 1e7) < -10.0 * order as f64 * 0.5, "{resp:?} {order}: stop band");
            }
        }
    }

    /// The Butterworth of order n falls 20 n dB per decade.
    #[test]
    fn butterworth_falls_twenty_n_db_per_decade() {
        let f = Filter::design(&Spec { order: 4, f1: 1e5, ..Spec::default() }, 1e-8);
        let slope = db_at(&f, 3e5) - db_at(&f, 3e6);
        assert!((slope - 80.0).abs() < 3.0, "{slope}");
    }

    /// The Bessel step response does not overshoot, the Butterworth of the same order does.
    #[test]
    fn bessel_does_not_overshoot() {
        let step = |r| {
            let f = Filter::design(&Spec { response: r, order: 5, f1: 1e6, ..Spec::default() }, 1e-8);
            f.step(4000).into_iter().fold(0.0f32, f32::max)
        };
        assert!(step(Response::Bessel) < 1.01, "bessel {}", step(Response::Bessel));
        assert!(step(Response::Butterworth) > 1.05, "butterworth {}", step(Response::Butterworth));
    }

    /// A high-pass passes high and blocks DC; a band-pass peaks between its edges; a band-stop dips there.
    #[test]
    fn the_other_kinds_have_their_shape() {
        let dt = 1e-8;
        let hp = Filter::design(&Spec { kind: Kind::Highpass, f1: 1e6, ..Spec::default() }, dt);
        assert!(db_at(&hp, 1e4) < -40.0 && db_at(&hp, 2e7).abs() < 0.5);
        let bp = Filter::design(&Spec { kind: Kind::Bandpass, f1: 4e5, f2: 1e6, order: 2, ..Spec::default() }, dt);
        assert!(db_at(&bp, 6e5) > -3.0 && db_at(&bp, 1e4) < -20.0 && db_at(&bp, 2e7) < -20.0);
        let bs = Filter::design(&Spec { kind: Kind::Bandstop, f1: 2e5, f2: 4e6, order: 2, ..Spec::default() }, dt);
        assert!(db_at(&bs, 8e5) < db_at(&bs, 5e4) - 3.0 && db_at(&bs, 1e3).abs() < 1.0);
    }

    /// The Gaussian FIR is 3 dB down at its cut-off; the raised cosine has Nyquist gain one half and the root
    /// raised cosine applied twice is the raised cosine.
    #[test]
    fn the_fir_responses_have_their_defining_points() {
        let dt = 2e-8;
        let g = Filter::design(&Spec { response: Response::Gaussian, f1: 5e6, ..Spec::default() }, dt);
        assert!((db_at(&g, 5e6) + 3.01).abs() < 0.2, "{}", db_at(&g, 5e6));
        let sym = 1e-6;
        let rc = Filter::design(
            &Spec { response: Response::RaisedCosine, symbol: sym, rolloff: 0.35, ..Spec::default() },
            dt,
        );
        let nyq = 10f64.powf(db_at(&rc, 0.5 / sym) / 20.0) / 10f64.powf(db_at(&rc, 1e3) / 20.0);
        assert!((nyq - 0.5).abs() < 0.03, "{nyq}");
        let rrc = Filter::design(
            &Spec { response: Response::RootRaisedCosine, symbol: sym, rolloff: 0.35, ..Spec::default() },
            dt,
        );
        for f in [1e5, 3e5, 4e5, 5e5] {
            let twice = 2.0 * db_at(&rrc, f);
            let d = (twice - db_at(&rc, f)).abs();
            assert!(d < 1.0, "at {f}: rrc twice {twice} dB, rc {} dB", db_at(&rc, f));
        }
    }

    /// An all-pass delay shifts a pulse by the delay and keeps its height.
    #[test]
    fn the_all_pass_delays_without_changing_the_level() {
        let dt = 1e-8;
        let f = Filter::design(&Spec { response: Response::Allpass, delay: 2.5e-7, ..Spec::default() }, dt);
        assert!(db_at(&f, 1e6).abs() < 0.3);
        let mut x = vec![0.0f32; 200];
        for v in &mut x[60..80] {
            *v = 1.0;
        }
        let y = f.apply(&x);
        // the pulse centre moves by 25 samples
        let centre = |v: &[f32]| v.iter().enumerate().map(|(i, a)| i as f32 * a).sum::<f32>() / v.iter().sum::<f32>();
        assert!(((centre(&y) - centre(&x)) - 25.0).abs() < 0.5, "{}", centre(&y) - centre(&x));
    }

    /// The transform and the direct sum give the same output, for taps and records of different lengths and a centre
    /// anywhere in the taps.
    #[test]
    fn the_fft_convolution_matches_the_direct_sum() {
        for (taps, n, c) in [(49usize, 900usize, 24usize), (301, 5000, 150), (1025, 4400, 0), (700, 3000, 699)] {
            let t: Vec<f64> = (0..taps).map(|i| ((i * 7 % 13) as f64 - 6.0) / (taps as f64)).collect();
            let x: Vec<f32> = (0..n).map(|i| ((i * 31 % 17) as f32 - 8.0) * 0.1 + (i as f32 * 0.01).sin()).collect();
            let (a, b) = (run_fir(&t, c, &x), run_fir_direct(&t, c, &x));
            assert_eq!(a.len(), n);
            let worst = a.iter().zip(&b).map(|(p, q)| (p - q).abs()).fold(0.0, f64::max);
            assert!(worst < 1e-9, "{taps} taps {n} samples centre {c}: worst difference {worst}");
        }
    }

    /// The taps of a filter read at another step make the same filter: a Gaussian designed at one step, its taps
    /// given at that step and used for a record three times as fast, has the same gain at the cut-off; a filter that
    /// would need too many taps is refused.
    #[test]
    fn a_filter_of_its_own_taps_keeps_its_gain_at_another_step() {
        let spec = Spec { response: Response::Gaussian, f1: 1e5, ..Spec::default() };
        let dt1 = 1e-6;
        let Filter { branches, .. } = Filter::design(&spec, dt1);
        let Branch::Fir(t, _) = &branches[0] else { panic!("a FIR") };
        let taps: Vec<f32> = t.iter().map(|&v| v as f32).collect();
        for dt2 in [dt1 / 3.0, dt1, dt1 * 2.0] {
            let own = Filter::from_taps(&taps, dt1, dt2).expect("taps");
            let (own_db, want) = (db_at(&own, 1e5), db_at(&Filter::design(&spec, dt1), 1e5));
            assert!((own_db - want).abs() < 0.5, "dt {dt2}: {own_db} dB against {want} dB");
            assert!(db_at(&own, 1e3).abs() < 0.1, "unit gain at low frequency");
        }
        assert!(Filter::from_taps(&taps, 1e-3, 1e-12).is_none(), "too many taps");
        assert!(Filter::from_taps(&taps[..1], dt1, dt1).is_none());
    }

    /// A filter of a file made for a slow record, applied to a record a hundred times faster, filters at the step of
    /// the filter: a tone far below its cut-off passes, one far above is gone, and the record's length stays.
    #[test]
    fn taps_for_a_slow_record_filter_a_fast_one() {
        let spec = Spec { response: Response::Gaussian, f1: 1e5, ..Spec::default() };
        let dt_taps = 1e-6;
        let Filter { branches, .. } = Filter::design(&spec, dt_taps);
        let Branch::Fir(t, _) = &branches[0] else { panic!("a FIR") };
        let taps: Vec<f32> = t.iter().map(|&v| v as f32).collect();
        let dt = dt_taps / 100.0;
        let tone = |f: f64| -> Vec<f32> { (0..20000).map(|i| (2.0 * PI * f * i as f64 * dt).sin() as f32).collect() };
        assert!(Filter::from_taps(&taps, dt_taps, dt).is_some() == (taps.len() * 100 <= MAX_CUSTOM_TAPS));
        for (f, low) in [(1e4, true), (2e6, false)] {
            let x = tone(f);
            let y = Filter::apply_taps(&x, dt, &taps, dt_taps).expect("a filter");
            assert_eq!(y.len(), x.len());
            let mid = &y[8000..12000];
            let amp = mid.iter().fold(0f32, |a, &v| a.max(v.abs()));
            assert!(if low { amp > 0.9 } else { amp < 0.1 }, "{f} Hz comes out with {amp}");
        }
        assert!(Filter::apply_taps(&tone(1e4), dt_taps * 2.0, &taps[..1], dt_taps).is_none());
    }
}
