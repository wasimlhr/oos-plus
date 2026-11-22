// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Math channel operations (computed by oos-dsp).

use oos_dsp::linecode::{self, Gate};
pub use oos_dsp::math::Window;
use oos_dsp::math::{binary, unary, Op1, Op2};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MathOp {
    Off,
    Add,
    Sub,
    Mul,
    Div,
    Intg,
    Diff,
    Abs,
    Sqrt,
    Lg,
    Ln,
    Exp,
    /// A x + B
    Lin,
    /// a formula over the channels (`oos_dsp::expr`)
    Expr,
    Lpf,
    Hpf,
    Bpf,
    Bsf,
    Fft,
    Fir,
    And,
    Or,
    Xor,
    Nand,
    Nor,
    Not,
    A2d,
    D2a,
    Conv,
    Unnrzi,
}

/// Math menu categories: Arithmetic, Function, Filter, Logic, Convert.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MathFamily {
    Arith,
    Fn,
    Filter,
    Logic,
    Convert,
}

impl MathFamily {
    pub const ALL: [MathFamily; 5] =
        [MathFamily::Arith, MathFamily::Fn, MathFamily::Filter, MathFamily::Logic, MathFamily::Convert];

    pub fn name(self) -> &'static str {
        match self {
            MathFamily::Arith => "Arithmetic",
            MathFamily::Fn => "Function",
            MathFamily::Filter => "Filter",
            MathFamily::Logic => "Logic",
            MathFamily::Convert => "Convert",
        }
    }

    /// Operations of this category (not Off).
    pub const fn ops(self) -> &'static [MathOp] {
        match self {
            MathFamily::Arith => &[MathOp::Add, MathOp::Sub, MathOp::Mul, MathOp::Div, MathOp::Conv],
            MathFamily::Fn => &[
                MathOp::Intg,
                MathOp::Diff,
                MathOp::Abs,
                MathOp::Sqrt,
                MathOp::Lg,
                MathOp::Ln,
                MathOp::Exp,
                MathOp::Lin,
                MathOp::Expr,
            ],
            MathFamily::Filter => &[MathOp::Lpf, MathOp::Hpf, MathOp::Bpf, MathOp::Bsf, MathOp::Fir, MathOp::Fft],
            MathFamily::Logic => &[MathOp::And, MathOp::Or, MathOp::Xor, MathOp::Nand, MathOp::Nor, MathOp::Not],
            MathFamily::Convert => &[MathOp::A2d, MathOp::D2a, MathOp::Unnrzi],
        }
    }
}

/// Every operation: Off, then the operations of each category in menu order.
pub const MATH_OPS: [MathOp; 30] = {
    let mut all = [MathOp::Off; 30];
    let (mut n, mut f) = (1, 0);
    while f < MathFamily::ALL.len() {
        let ops = MathFamily::ALL[f].ops();
        let mut i = 0;
        while i < ops.len() {
            all[n] = ops[i];
            (n, i) = (n + 1, i + 1);
        }
        f += 1;
    }
    assert!(n == all.len());
    all
};

/// Math channels (8, math on math).
pub const NMATH: usize = 8;
/// Keep existing CH and Math setup indices stable; references follow M8.
pub const MATH_REF_BASE: usize = crate::model::NCH + NMATH;
pub const MATH_SOURCES: usize = MATH_REF_BASE + crate::model::NREF;

/// Name of a math source: 0..3 = CH1..CH4, 4.. = M1..M8.
pub fn src_name(i: usize) -> String {
    if i < 4 {
        format!("CH{}", i + 1)
    } else if i < MATH_REF_BASE {
        format!("M{}", i - 3)
    } else {
        format!("R{}", i - MATH_REF_BASE + 1)
    }
}

pub fn parse_expr(text: &str) -> Result<oos_dsp::expr::Expr, String> {
    let names: Vec<_> = (0..MATH_SOURCES).map(src_name).collect();
    let names: Vec<_> = names.iter().map(String::as_str).collect();
    oos_dsp::expr::Expr::parse_sources(text, &names)
}

pub const WINDOWS: [Window; 5] = [Window::Rect, Window::Hann, Window::Hamming, Window::Blackman, Window::Flattop];

impl MathOp {
    pub fn name(self) -> &'static str {
        match self {
            MathOp::Off => "Off",
            MathOp::Add => "A+B",
            MathOp::Sub => "A-B",
            MathOp::Mul => "AxB",
            MathOp::Div => "A/B",
            MathOp::Intg => "Intg",
            MathOp::Diff => "Diff",
            MathOp::Abs => "Abs",
            MathOp::Sqrt => "Sqrt",
            MathOp::Lg => "Lg",
            MathOp::Ln => "Ln",
            MathOp::Lin => "Ax+B",
            MathOp::Expr => "Formula",
            MathOp::Exp => "Exp",
            MathOp::Lpf => "Low-pass",
            MathOp::Hpf => "High-pass",
            MathOp::Bpf => "Band-pass",
            MathOp::Bsf => "Band-stop",
            MathOp::Fft => "FFT",
            MathOp::Fir => "FIR",
            MathOp::And => "AND",
            MathOp::Or => "OR",
            MathOp::Xor => "XOR",
            MathOp::Nand => "NAND",
            MathOp::Nor => "NOR",
            MathOp::Not => "NOT",
            MathOp::A2d => "A2D",
            MathOp::D2a => "D2A",
            MathOp::Conv => "Conv",
            MathOp::Unnrzi => "NRZI inv",
        }
    }
    pub fn family(self) -> MathFamily {
        match self {
            MathOp::Off | MathOp::Add | MathOp::Sub | MathOp::Mul | MathOp::Div | MathOp::Conv => MathFamily::Arith,
            MathOp::Intg
            | MathOp::Diff
            | MathOp::Abs
            | MathOp::Sqrt
            | MathOp::Lg
            | MathOp::Ln
            | MathOp::Exp
            | MathOp::Lin
            | MathOp::Expr => MathFamily::Fn,
            MathOp::Lpf | MathOp::Hpf | MathOp::Bpf | MathOp::Bsf | MathOp::Fir | MathOp::Fft => MathFamily::Filter,
            MathOp::And | MathOp::Or | MathOp::Xor | MathOp::Nand | MathOp::Nor | MathOp::Not => MathFamily::Logic,
            MathOp::A2d | MathOp::D2a | MathOp::Unnrzi => MathFamily::Convert,
        }
    }
    pub fn two_sources(self) -> bool {
        matches!(
            self,
            MathOp::Add
                | MathOp::Sub
                | MathOp::Mul
                | MathOp::Div
                | MathOp::And
                | MathOp::Or
                | MathOp::Xor
                | MathOp::Nand
                | MathOp::Nor
                | MathOp::Conv
        )
    }
    pub fn unit(self) -> &'static str {
        match self {
            MathOp::Mul => "V\u{b2}",
            MathOp::Div | MathOp::Lg | MathOp::Ln | MathOp::Exp | MathOp::Expr => "",
            MathOp::Intg => "Vs",
            MathOp::Diff => "V/s",
            MathOp::Fft => "dB",
            _ => "V",
        }
    }
    /// Label of the formula, e.g. "CH1 + CH2", "Intg(M1)" (sources as `src_name` indices).
    pub fn formula(self, a: usize, b: usize) -> String {
        let (a, b) = (src_name(a), src_name(b));
        match self {
            MathOp::Add => format!("{} + {}", a, b),
            MathOp::Sub => format!("{} - {}", a, b),
            MathOp::Mul => format!("{} x {}", a, b),
            MathOp::Div => format!("{} / {}", a, b),
            MathOp::And => format!("{} AND {}", a, b),
            MathOp::Or => format!("{} OR {}", a, b),
            MathOp::Xor => format!("{} XOR {}", a, b),
            MathOp::Nand => format!("{} NAND {}", a, b),
            MathOp::Nor => format!("{} NOR {}", a, b),
            MathOp::Conv => format!("Conv({}, {})", a, b),
            op => format!("{}({})", op.name(), a),
        }
    }
    /// Filters with a corner/centre frequency; band filters also have a bandwidth.
    pub fn has_fc(self) -> bool {
        matches!(self, MathOp::Lpf | MathOp::Hpf | MathOp::Bpf | MathOp::Bsf | MathOp::Fir)
    }
    pub fn has_bw(self) -> bool {
        matches!(self, MathOp::Bpf | MathOp::Bsf)
    }
}

/// The response of a filter of the Filter category: `Simple` is the plain filter of the operator (first-order
/// low- and high-pass, zero-phase band filters); the others are those of the designer (`oos_dsp::filter`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FiltResp {
    Simple,
    Butterworth,
    Chebyshev,
    Bessel,
    Gaussian,
    RaisedCosine,
    RootRaisedCosine,
    /// a delay
    Delay,
}

pub const FILT_RESPS: [FiltResp; 8] = [
    FiltResp::Simple,
    FiltResp::Butterworth,
    FiltResp::Chebyshev,
    FiltResp::Bessel,
    FiltResp::Gaussian,
    FiltResp::RaisedCosine,
    FiltResp::RootRaisedCosine,
    FiltResp::Delay,
];

impl FiltResp {
    /// The designer's response, None for `Simple`.
    fn designed(self) -> Option<oos_dsp::filter::Response> {
        use oos_dsp::filter::Response as R;
        Some(match self {
            FiltResp::Simple => return None,
            FiltResp::Butterworth => R::Butterworth,
            FiltResp::Chebyshev => R::Chebyshev,
            FiltResp::Bessel => R::Bessel,
            FiltResp::Gaussian => R::Gaussian,
            FiltResp::RaisedCosine => R::RaisedCosine,
            FiltResp::RootRaisedCosine => R::RootRaisedCosine,
            FiltResp::Delay => R::Allpass,
        })
    }
    /// Needs the order (the IIR responses).
    pub fn has_order(self) -> bool {
        matches!(self, FiltResp::Butterworth | FiltResp::Chebyshev | FiltResp::Bessel)
    }
    /// Takes the cut-off frequency (the others are set by symbol or delay).
    pub fn has_cutoff(self) -> bool {
        !matches!(self, FiltResp::RaisedCosine | FiltResp::RootRaisedCosine | FiltResp::Delay)
    }
}

/// What the designer needs besides the cut-off frequencies; the defaults are those of a Butterworth of order 4.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct FiltCfg {
    pub resp: FiltResp,
    pub order: u32,
    /// Chebyshev ripple, dB
    pub ripple: f32,
    /// raised cosine symbol time, s
    pub symbol: f32,
    pub rolloff: f32,
    /// the delay of `FiltResp::Delay`, s
    pub delay: f32,
}

impl Default for FiltCfg {
    fn default() -> FiltCfg {
        FiltCfg { resp: FiltResp::Simple, order: 4, ripple: 1.0, symbol: 1e-6, rolloff: 0.35, delay: 1e-6 }
    }
}

/// The designer's specification of the filter operator `op` (`Lpf`, `Hpf`, `Bpf` or `Bsf` give its kind) with the centre
/// `fc` and width `bw` of its band: None for the `Simple` response, which the operator computes itself.
pub fn filter_spec(op: MathOp, fc: f64, bw: f64, filt: &FiltCfg) -> Option<oos_dsp::filter::Spec> {
    use oos_dsp::filter::{Kind, Spec};
    let response = filt.resp.designed()?;
    let kind = match op {
        MathOp::Hpf => Kind::Highpass,
        MathOp::Bpf => Kind::Bandpass,
        MathOp::Bsf => Kind::Bandstop,
        _ => Kind::Lowpass,
    };
    let half = if matches!(kind, Kind::Bandpass | Kind::Bandstop) { bw / 2.0 } else { 0.0 };
    Some(Spec {
        kind,
        response,
        order: filt.order as usize,
        f1: (fc - half).max(1.0),
        f2: fc + half,
        ripple_db: filt.ripple as f64,
        symbol: filt.symbol as f64,
        rolloff: filt.rolloff as f64,
        delay: filt.delay as f64,
    })
}

/// The filter `filt` makes of a record for the filter operator `op`: None for the `Simple` response.
fn designed(op: MathOp, a: &[f32], dt: f64, fc: f64, bw: f64, filt: &FiltCfg) -> Option<Vec<f32>> {
    let spec = filter_spec(op, fc, bw, filt)?;
    Some(oos_dsp::filter::Filter::design(&spec, dt).apply(a))
}

/// One math channel's settings.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct MathCh {
    pub op: MathOp,
    pub a: usize,
    pub b: usize,
    pub scale: usize,
    pub fc: f32,
    pub bw: f32,
    pub offset: f32,
    pub win: Window,
    /// A and B of Ax+B
    pub lin: (f32, f32),
    pub filt: FiltCfg,
}

impl Default for MathCh {
    fn default() -> MathCh {
        MathCh {
            op: MathOp::Off,
            a: 0,
            b: 1,
            scale: 3,
            fc: 10e3,
            bw: 2e3,
            offset: 0.0,
            win: Window::Hann,
            lin: (1.0, 0.0),
            filt: FiltCfg::default(),
        }
    }
}

/// The math waveform (not for FFT/Off); `fc` = filter corner/centre frequency, `bw` = band filter bandwidth.
pub fn compute(op: MathOp, a: &[f32], b: &[f32], dt: f64, fc: f64, bw: f64, filt: &FiltCfg) -> Vec<f32> {
    if matches!(op, MathOp::Lpf | MathOp::Hpf | MathOp::Bpf | MathOp::Bsf) {
        if let Some(v) = designed(op, a, dt, fc, bw, filt) {
            return v;
        }
    }
    match op {
        MathOp::Add => binary(Op2::Add, a, b),
        MathOp::Sub => binary(Op2::Sub, a, b),
        MathOp::Mul => binary(Op2::Mul, a, b),
        MathOp::Div => binary(Op2::Div, a, b),
        MathOp::Intg => unary(Op1::Intg, a, dt),
        MathOp::Diff => unary(Op1::Diff, a, dt),
        MathOp::Abs => unary(Op1::Abs, a, dt),
        MathOp::Sqrt => unary(Op1::Sqrt, a, dt),
        MathOp::Lg => unary(Op1::Lg, a, dt),
        MathOp::Ln => unary(Op1::Ln, a, dt),
        // applied by the caller, which knows A and B
        MathOp::Lin | MathOp::Expr => a.to_vec(),
        MathOp::Exp => unary(Op1::Exp, a, dt),
        MathOp::Lpf => unary(Op1::Lowpass(fc), a, dt),
        MathOp::Hpf => unary(Op1::Highpass(fc), a, dt),
        MathOp::Bpf => unary(Op1::Bandpass(fc, bw), a, dt),
        MathOp::Bsf => unary(Op1::Bandstop(fc, bw), a, dt),
        MathOp::Off | MathOp::Fft => Vec::new(),
        MathOp::Fir => linecode::fir_lowpass(a, dt, fc),
        MathOp::And => linecode::gate(a, b, Gate::And),
        MathOp::Or => linecode::gate(a, b, Gate::Or),
        MathOp::Xor => linecode::gate(a, b, Gate::Xor),
        MathOp::Nand => linecode::gate(a, b, Gate::Nand),
        MathOp::Nor => linecode::gate(a, b, Gate::Nor),
        MathOp::Not => linecode::not(a),
        MathOp::A2d => linecode::a2d(a),
        MathOp::D2a => linecode::d2a(a),
        MathOp::Conv => linecode::conv(a, b),
        MathOp::Unnrzi => linecode::unnrzi(a),
    }
}

/// 1-2-5 scale ladder for the math channel: 1 n .. 1 G (units per division).
pub fn math_scales() -> Vec<f64> {
    let mut v = Vec::new();
    let mut d = 1e-9;
    while d < 2e9 {
        for m in [1.0, 2.0, 5.0] {
            v.push(d * m);
        }
        d *= 10.0;
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A response other than Simple makes the filter of the designer: a Butterworth of order 4 passes a tone well below
    /// its cut-off and blocks one well above it, which the Simple first-order filter does not; the Simple response
    /// is the filter the operator always was.
    #[test]
    fn a_designed_response_replaces_the_simple_filter() {
        let dt = 1e-8;
        let tone = |f: f64| -> Vec<f32> { (0..20000).map(|k| (std::f64::consts::TAU * f * k as f64 * dt).sin() as f32).collect() };
        let amp = |v: &[f32]| v[10000..].iter().fold(0.0f32, |m, x| m.max(x.abs()));
        let mut cfg = FiltCfg::default();
        let simple = compute(MathOp::Lpf, &tone(1e6), &[], dt, 1e5, 0.0, &cfg);
        cfg.resp = FiltResp::Butterworth;
        let designed = compute(MathOp::Lpf, &tone(1e6), &[], dt, 1e5, 0.0, &cfg);
        assert!(amp(&designed) < 0.01 && amp(&simple) > 0.05, "designed {} simple {}", amp(&designed), amp(&simple));
        let low = compute(MathOp::Lpf, &tone(1e4), &[], dt, 1e5, 0.0, &cfg);
        assert!(amp(&low) > 0.98, "{}", amp(&low));
        let hp = compute(MathOp::Hpf, &tone(1e4), &[], dt, 1e5, 0.0, &cfg);
        assert!(amp(&hp) < 0.01, "{}", amp(&hp));
    }

    #[test]
    fn every_op_has_a_category() {
        for &op in &MATH_OPS {
            if op == MathOp::Off {
                continue;
            }
            assert!(op.family().ops().contains(&op), "{:?} not listed under {:?}", op, op.family());
        }
        let n: usize = MathFamily::ALL.iter().map(|f| f.ops().len()).sum();
        assert_eq!(n, MATH_OPS.len() - 1);
    }
}
