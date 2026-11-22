// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The math settings of the selected math channel that are one value each: the FFT window and the filters'
//! frequencies. Their commands are `:MATH<n>:...` (scpi/math.rs), which reads and sets channel n as if it were the
//! selected one; their rows the Math menu (ui/setting_items.rs). The slices are the places of `store::setup_text`
//! that write them, so the keys of a setup file keep their order.
//!
//! Still written out by hand: `math` (the menu offers the operators of the category the menu shows, `:OPERator?`
//! answers `OFF` that `:OPERator` does not take, `:DISPlay`, `:FFT:SOURce`, `:FFT:SCALe` and `:FILTer:TYPE` set it
//! too), `math_a` and `math_b` (the menu offers the sources the channel may use; `:SOURce1` and `:SOURce2` refuse an
//! empty reference with -230), `math_scale` (an index into the dB steps under FFT and into the volt steps otherwise;
//! `:SCALe` sets the nearest step of the operator's), `math_offset` (the menu shows divisions with two decimals and
//! steps by 0.1 without a limit, `:OFFSet` keeps it within 1e9), `math_lin` (one key for the menu rows A and B),
//! `math_sel` (the menu swaps the channels' settings, `Model::math_select`; a setup only sets the number),
//! `math_expr<N>`, `math_window`, and the other channels, `m<N>.*`: one block per channel under other keys (`op`,
//! `win`, ...), which an indexed row would carry for three of their nine fields.

use super::{Arg, Choice, Scpi, Setting, Ty, Unknown, Val};
use crate::math::{FILT_RESPS, WINDOWS};

use crate::model::{step_index, time_ladder};

/// Both slices, in the order of the docs table.
pub const SLICES: [&[Setting]; 3] = [&WINDOW_BW, &FC, &FILT];

/// Why the filters' frequencies have no command of their own.
#[cfg(test)]
const CORNERS: &str = "set through `:MATH<n>:FILTer:W1` and `:W2`, the corner frequencies (below)";

/// The windows of `WINDOWS` by their place: their word in a setup, their keyword of `:FFT:WINDow`, their name.
pub const WINDOWS_CHOICES: [Choice; 5] = [
    Choice { store: "Rect", scpi: "RECTangle", label: "Rect" },
    Choice { store: "Hann", scpi: "HANNing", label: "Hann" },
    Choice { store: "Hamming", scpi: "HAMMing", label: "Hamming" },
    Choice { store: "Blackman", scpi: "BLACkman", label: "Blackman" },
    Choice { store: "Flattop", scpi: "FLATtop", label: "Flattop" },
];

/// Written after `math_offset`.
pub const WINDOW_BW: [Setting; 2] = [
    Setting {
        key: "fft_win",
        scpi: Scpi::Cmd("FFT:WINDow"),
        ty: Ty::Choice(&WINDOWS_CHOICES, Unknown::Value, Arg::Keyword),
        label: "Window",
        #[cfg(test)]
        doc: "The window the FFT multiplies the record with before the transform.",
        accept: None,
        step: None,
        get: |m, _| Val::Index(WINDOWS.iter().position(|&w| w == m.fft_win).unwrap_or(0)),
        put: |m, _, v| m.fft_win = WINDOWS[v.index()],
    },
    Setting {
        key: "math_bw",
        scpi: Scpi::None {
            #[cfg(test)]
            why: CORNERS,
        },
        ty: Ty::Hertz { ladder: freq_step },
        label: "Bandwidth",
        #[cfg(test)]
        doc: "The width of the band of the band-pass and band-stop filters. The menu steps 1-2-5 in periods.",
        accept: None,
        step: None,
        get: |m, _| Val::F32(m.math_bw),
        put: |m, _, v| m.math_bw = v.secs(),
    },
];

/// Written after `math_scale`.
pub const FC: [Setting; 1] = [Setting {
    key: "math_fc",
    scpi: Scpi::None {
        #[cfg(test)]
        why: CORNERS,
    },
    ty: Ty::Hertz { ladder: freq_step },
    label: "Frequency",
    #[cfg(test)]
    doc: "The cut-off frequency of the low- and high-pass filters, the centre of the band filters. The menu steps \
          1-2-5 in periods.",
    accept: None,
    step: None,
    get: |m, _| Val::F32(m.math_fc),
    put: |m, _, v| m.math_fc = v.secs(),
}];

/// Why the designer's settings have no command of their own.
#[cfg(test)]
const DESIGNER: &str = "the designer's settings are set from the menu and a setup only";

/// The responses of `FILT_RESPS` by their place.
pub const FILT_CHOICES: [Choice; 8] = [
    Choice { store: "Simple", scpi: "SIMPle", label: "Simple" },
    Choice { store: "Butterworth", scpi: "BUTTerworth", label: "Butterworth" },
    Choice { store: "Chebyshev", scpi: "CHEBychev", label: "Chebyshev" },
    Choice { store: "Bessel", scpi: "BESSel", label: "Bessel" },
    Choice { store: "Gaussian", scpi: "GAUSsian", label: "Gaussian" },
    Choice { store: "RaisedCosine", scpi: "RCOSine", label: "Raised cosine" },
    Choice { store: "RootRaisedCosine", scpi: "RRCosine", label: "Root raised cosine" },
    Choice { store: "Delay", scpi: "DELay", label: "Delay" },
];

fn one_decimal(x: f64) -> String {
    format!("{x:.2}")
}

fn db(x: f64) -> String {
    format!("{x:.2} dB")
}

/// Written after `math_bw`: the designer of the filters of the Filter category (`oos_dsp::filter`).
pub const FILT: [Setting; 6] = [
    Setting {
        key: "filt_resp",
        scpi: Scpi::None {
            #[cfg(test)]
            why: DESIGNER,
        },
        ty: Ty::Choice(&FILT_CHOICES, Unknown::Value, Arg::Keyword),
        label: "Response",
        #[cfg(test)]
        doc: "The response of the filter: Simple (the plain filter of the operator) or one the designer makes:               Butterworth, Chebyshev, Bessel, Gaussian, raised cosine, root raised cosine, or a delay.",
        accept: None,
        step: None,
        get: |m, _| Val::Index(FILT_RESPS.iter().position(|&r| r == m.math_filt.resp).unwrap_or(0)),
        put: |m, _, v| m.math_filt.resp = FILT_RESPS[v.index()],
    },
    Setting {
        key: "filt_order",
        scpi: Scpi::None {
            #[cfg(test)]
            why: DESIGNER,
        },
        ty: Ty::U32 { min: 1, max: 16, step: 1, unit: "" },
        label: "Order",
        #[cfg(test)]
        doc: "The order of a Butterworth, Chebyshev or Bessel filter (of each pass of a band filter).",
        accept: None,
        step: None,
        get: |m, _| Val::U32(m.math_filt.order),
        put: |m, _, v| m.math_filt.order = v.index() as u32,
    },
    Setting {
        key: "filt_ripple",
        scpi: Scpi::None {
            #[cfg(test)]
            why: DESIGNER,
        },
        ty: Ty::Real { range: Some((0.01, 6.0)), shown: db },
        label: "Ripple",
        #[cfg(test)]
        doc: "The pass band ripple of a Chebyshev filter. The menu steps by 0.25 dB.",
        accept: None,
        step: Some(|m, _, d| Val::F32((m.math_filt.ripple + 0.25 * d as f32).clamp(0.01, 6.0))),
        get: |m, _| Val::F32(m.math_filt.ripple),
        put: |m, _, v| m.math_filt.ripple = v.secs(),
    },
    Setting {
        key: "filt_symbol",
        scpi: Scpi::None {
            #[cfg(test)]
            why: DESIGNER,
        },
        ty: Ty::Seconds { ladder: symbol_step, zero: None },
        label: "Symbol time",
        #[cfg(test)]
        doc: "The symbol duration of a raised cosine or root raised cosine filter. The menu steps 1-2-5.",
        accept: None,
        step: None,
        get: |m, _| Val::F32(m.math_filt.symbol),
        put: |m, _, v| m.math_filt.symbol = v.secs(),
    },
    Setting {
        key: "filt_rolloff",
        scpi: Scpi::None {
            #[cfg(test)]
            why: DESIGNER,
        },
        ty: Ty::Real { range: Some((0.0, 1.0)), shown: one_decimal },
        label: "Roll-off",
        #[cfg(test)]
        doc: "The roll-off factor of a raised cosine or root raised cosine filter, 0 to 1. The menu steps by 0.05.",
        accept: None,
        step: Some(|m, _, d| Val::F32((m.math_filt.rolloff + 0.05 * d as f32).clamp(0.0, 1.0))),
        get: |m, _| Val::F32(m.math_filt.rolloff),
        put: |m, _, v| m.math_filt.rolloff = v.secs(),
    },
    Setting {
        key: "filt_delay",
        scpi: Scpi::None {
            #[cfg(test)]
            why: DESIGNER,
        },
        ty: Ty::Seconds { ladder: symbol_step, zero: Some("0") },
        label: "Delay",
        #[cfg(test)]
        doc: "The delay of the Delay response. The menu steps 1-2-5.",
        accept: None,
        step: None,
        get: |m, _| Val::F32(m.math_filt.delay),
        put: |m, _, v| m.math_filt.delay = v.secs(),
    },
];

/// One step of a time on the 1-2-5 ladder of the time base.
fn symbol_step(t: f32, d: i32) -> f32 {
    let l = time_ladder();
    let i = l.iter().position(|&x| x >= t * 0.999).unwrap_or(0);
    l[step_index(i, l.len(), d)]
}

/// One step of a frequency on the 1-2-5 ladder of its period, from the nearest period (the first one at the start
/// when the period is longer than all); a step up is a shorter period.
fn freq_step(f: f32, d: i32) -> f32 {
    let l = time_ladder();
    let per = 1.0 / f.max(1e-6);
    let i = l.iter().position(|&x| x >= per * 0.999).unwrap_or(0);
    1.0 / l[step_index(i, l.len(), -d)]
}
