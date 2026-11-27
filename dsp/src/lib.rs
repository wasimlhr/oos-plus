// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! OOS signal processing, platform independent, no dependencies.
//!
//! - `bandwidth`: equivalent-time folding of periodic signals (also above Nyquist), step response -> rise time and
//!   magnitude response (front-end bandwidth test)
//! - `interleave`: error budget of an interleaved ADC from one sine record (each core's offset, gain and time error)
//! - `edges`: comparator with hysteresis -> edge list with interpolated times (the base of all triggers)
//! - `trigger`: software trigger types of a modern scope (edge, pulse, slope, video, pattern, duration, timeout, runt,
//!   window, delay, setup/hold, Nth edge) with time conditions
//! - `measure`: voltage, time and two-channel measurements
//! - `clock`: jitter of a clock from its edges (period, cycle-to-cycle, time interval error)
//! - `mask`: mask (pass/fail) test: envelope around a reference waveform, records tested against it
//! - `expr`: math expressions over the channels
//! - `math`: arithmetic, integral, derivative, filters, FFT with windows
//! - `magnetic`: inductance, B/H/μr, core loss and the I vs ∫V loop
//! - `motor`: Clarke/Park DQ0, shaft speed and the mechanical power ratio
//! - `dpt`: wide-bandgap double-pulse timings, recovery and Qoss
//! - `sim`: signal generators for tests and the demo source (pulse trains with glitches/runts, composite video, ...)
//! - `protocols`: the protocol decoders as modules (key, name, lines, the call, timing, trigger rule), their registry
//! - `serial`: nominal high-speed serial records for the SIM menu (PRBS, tanh edges, Butterworth, de-emphasis)
//!
//! Waveforms are `&[f32]` volts sampled every `dt` seconds; times inside a waveform are fractional sample indices.

/// Analog channels the triggers and the expressions take. A board with more needs them extended here.
pub const CHANNELS: usize = 4;

/// SI prefixes from tera to pico, the largest first.
pub const SI_PREFIXES: [(f64, &str); 9] = [
    (1e12, "T"),
    (1e9, "G"),
    (1e6, "M"),
    (1e3, "k"),
    (1.0, ""),
    (1e-3, "m"),
    (1e-6, "\u{b5}"),
    (1e-9, "n"),
    (1e-12, "p"),
];

/// The prefix of `SI_PREFIXES[from..]` for the magnitude `a`: the largest it reaches (half a per mille short counts),
/// pico below them all.
pub fn si_prefix(a: f64, from: usize) -> (f64, &'static str) {
    SI_PREFIXES[from..].iter().copied().find(|(m, _)| a >= *m * 0.9995).unwrap_or((1e-12, "p"))
}

/// A decimal number without the trailing zeros of its fraction nor a bare point ("1.250" -> "1.25", "2.00" -> "2").
pub fn trim_zeros(s: &str) -> &str {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.')
    } else {
        s
    }
}

pub mod bandwidth;
pub mod bode;
pub mod clock;
pub mod cphy;
pub mod dcc_yard;
pub mod decode;
pub mod diagnosis;
pub mod dpt;
pub mod edges;
pub mod expr;
pub mod eye;
pub mod filter;
#[cfg(test)]
mod hostile;
pub mod hss;
pub mod identify;
pub mod interleave;
pub mod interp;
pub mod iq;
pub mod levels;
pub mod linecode;
pub mod magnetic;
pub mod mask;
pub mod math;
pub mod measure;
pub mod motor;
pub mod power;
pub mod protocols;
pub mod serial;
pub mod sim;
pub mod spectrum;
pub mod trigger;
