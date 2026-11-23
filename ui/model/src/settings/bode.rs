// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The settings of the Bode page: their rows in the Bode menu (ui/setting_items.rs) and the commands of the mode, the
//! gain unit and the amplitude, `:BODE:MODE`, `:BODE:UNIT`, `:BODE:AMPLitude` (the `:BODE` branch of
//! scpi/dispatch.rs; a value it does not take is refused with -224). A change in
//! the menu leaves the rest as it is (no `settings_changed`), as before the table; a new mode also clears the points
//! of the last sweep.
//!
//! The page keeps most of them in setup lines of several values (`bode = <start> <stop> <points> <amplitude> <in>
//! <out> <generator> <address>`, `bode_sweep = <time> <log> <sync> <edge> <level> <input> <ref>` for the free sweep),
//! so the keys of those rows name a field of a line (`bode start`). store.rs writes and reads the lines by hand, its
//! numbers with `store::p_f64`: a line sets none of its fields after the first that is wrong, the points are kept
//! within 2..=1000 and the sync channel 0 counts as 1 (quirks kept). `bode_link`, `bode_mode`, `bode_unit` and
//! `bode_amp_unit` are keys of their own, written by their slices where the keys were, so the keys of a setup file
//! keep their order.
//!
//! Still written out by hand: the input and the output (the menu offers the waves the screen shows, the free sweep's
//! input the channels or none), the generator (a list of presets in ui/bode.rs), its address and `:BODE:GENerator`
//! (one word, `usb` or the address, which sets the connection too), and the sync channel (the menu offers the
//! channels of the unit).

use super::{Arg, Choice, Scpi, Setting, Ty, Unknown, Val, NO_COMMAND_YET};
use crate::model::{decades, ladder_step, si_short, Slope};
use oos_dsp::bode::Mode;

/// The slices, for the docs table and the SCPI handler.
pub const SLICES: [&[Setting]; 6] = [&MODE, &UNIT, &AMP_UNIT, &SWEEP, &LINK, &FREE];

/// The modes by their place in the list.
const MODES: [Mode; 3] = [Mode::Loop, Mode::Psrr, Mode::Z];

/// Written after `spec_acpr`.
pub const MODE: [Setting; 1] = [Setting {
    key: "bode_mode",
    scpi: Scpi::Cmd("MODE"),
    ty: Ty::Choice(
        &[
            Choice { store: "Loop", scpi: "LOOP", label: "Loop" },
            Choice { store: "Psrr", scpi: "PSRR", label: "PSRR" },
            Choice { store: "Z", scpi: "IMPedance", label: "Z" },
        ],
        Unknown::Value,
        Arg::Keyword,
    ),
    label: "Mode",
    #[cfg(test)]
    doc: "What a sweep stores in the middle field of each point: the gain of a control loop in dB, the power-supply \
          rejection (the negated gain) in dB, or for an impedance the linear ratio of output to input, not ohms. A \
          free sweep always measures the gain.",
    accept: None,
    step: None,
    get: |m, _| Val::Index(MODES.iter().position(|&x| x == m.bode_mode).unwrap_or(0)),
    put: |m, _, v| m.bode_mode = MODES[v.index()],
}];

/// Written after `bode_mode`.
pub const UNIT: [Setting; 1] = [Setting {
    key: "bode_unit",
    scpi: Scpi::Cmd("UNIT"),
    ty: Ty::Choice(
        &[Choice { store: "dB", scpi: "DB", label: "dB" }, Choice { store: "V/V", scpi: "RATio", label: "V/V" }],
        Unknown::Value,
        Arg::Keyword,
    ),
    label: "Gain unit",
    #[cfg(test)]
    doc: "How the plot, the cursor and the title show the gain: in dB, 20 log10(Vout/Vin), or as the linear ratio \
          Vout/Vin (for PSRR Vin/Vout), whatever the mode measures. The CSV file and `:BODE:DATA?` keep what the mode \
          stores. The menu changes it on an odd step.",
    accept: None,
    step: Some(|m, _, d| odd(m.bode_vv, d)),
    get: |m, _| Val::Index(m.bode_vv as usize),
    put: |m, _, v| m.bode_vv = v.index() != 0,
}];

/// Written after `bode_unit`.
pub const AMP_UNIT: [Setting; 1] = [Setting {
    key: "bode_amp_unit",
    scpi: Scpi::none("a choice of the menu: `:BODE:AMPLitude` is always Vpp"),
    ty: Ty::Choice(
        &[Choice { store: "Vpp", scpi: "", label: "Vpp" }, Choice { store: "Vrms", scpi: "", label: "Vrms" }],
        Unknown::Value,
        Arg::Keyword,
    ),
    label: "Amp unit",
    #[cfg(test)]
    doc: "Whether the menu shows and the number pad takes the amplitude peak to peak or as RMS, Vrms = Vpp / \
          (2 sqrt 2) for the sine of the sweep. The generator gets Vpp either way. The menu changes it on an odd \
          step.",
    accept: None,
    step: Some(|m, _, d| odd(m.bode_rms, d)),
    get: |m, _| Val::Index(m.bode_rms as usize),
    put: |m, _, v| m.bode_rms = v.index() != 0,
}];

/// The amplitudes the generator takes, Vpp.
pub const AMP_RANGE: (f64, f64) = (0.01, 20.0);

/// Vpp over Vrms of a sine, 2 sqrt 2.
pub const VPP_PER_VRMS: f64 = 2.0 * std::f64::consts::SQRT_2;

/// The amplitudes the knob steps along, in the unit the menu shows (#52): 10 mV steps from 10 to 100 mV, 50 mV steps
/// from there to 20 V.
fn amp_steps() -> Vec<f64> {
    (1..=10).map(|k| k as f64 / 100.0).chain((3..=400).map(|k| k as f64 / 20.0)).collect()
}

/// The amplitude `d` knob steps from `vpp`, in Vpp: along `amp_steps` in Vpp or, with the menu in Vrms, in Vrms;
/// kept within `AMP_RANGE`.
fn amp_step(vpp: f64, rms: bool, d: i32) -> f64 {
    let k = if rms { VPP_PER_VRMS } else { 1.0 };
    (ladder(&amp_steps(), vpp / k, d) * k).clamp(AMP_RANGE.0, AMP_RANGE.1)
}

/// `d` steps along the ladder `l` from `x`; a value above all starts from the top.
fn ladder(l: &[f64], x: f64, d: i32) -> f64 {
    let at = |i: i32| l[i.clamp(0, l.len() as i32 - 1) as usize];
    // on a step (within rounding): `d` steps from it
    if let Some(k) = l.iter().position(|&v| (v - x).abs() <= v.abs() * 1e-9) {
        return at(k as i32 + d);
    }
    // between two steps (an amplitude typed on the pad, #52): up to the next step above, down to the next below
    if d > 0 {
        l.iter().position(|&v| v > x).map_or(at(l.len() as i32 - 1), |i| at(i as i32 + d - 1))
    } else if d < 0 {
        l.iter().rposition(|&v| v < x).map_or(at(0), |j| at(j as i32 + d + 1))
    } else {
        ladder_step(l, x, d, l.len() - 1)
    }
}

/// The frequencies of a sweep: 1-2-5 from 1 Hz to 5 GHz.
fn freq(x: f64, d: i32) -> f64 {
    ladder(&decades(0, 10, false), x, d)
}

/// A choice of two that the menu changes on an odd step only.
fn odd(on: bool, d: i32) -> Val {
    Val::Index((on ^ (d % 2 != 0)) as usize)
}

/// The fields of the `bode` line.
pub const SWEEP: [Setting; 4] = [
    Setting {
        key: "bode start",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::Real { range: None, shown: |x| si_short(x, "Hz") },
        label: "Start freq",
        #[cfg(test)]
        doc: "The first frequency of the sweep. The menu steps 1-2-5 and keeps it at or below half the stop.",
        accept: None,
        step: Some(|m, _, d| Val::F64(freq(m.bode_start, d).min(m.bode_stop / 2.0))),
        get: |m, _| Val::F64(m.bode_start),
        put: |m, _, v| m.bode_start = v.real(),
    },
    Setting {
        key: "bode stop",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::Real { range: None, shown: |x| si_short(x, "Hz") },
        label: "Stop freq",
        #[cfg(test)]
        doc: "The last frequency of the sweep. The menu steps 1-2-5 and keeps it at or above twice the start.",
        accept: None,
        step: Some(|m, _, d| Val::F64(freq(m.bode_stop, d).max(m.bode_start * 2.0))),
        get: |m, _| Val::F64(m.bode_stop),
        put: |m, _, v| m.bode_stop = v.real(),
    },
    Setting {
        key: "bode points",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::U32 { min: 5, max: 200, step: 5, unit: "" },
        label: "Points",
        #[cfg(test)]
        doc: "The number of frequencies of the sweep, spaced as Sweep sets: logarithmically or linearly. The menu \
              steps by 5 from 5 to 200; a setup line keeps 2 to 1000.",
        accept: None,
        step: None,
        get: |m, _| Val::U32(m.bode_points as u32),
        put: |m, _, v| m.bode_points = v.index(),
    },
    Setting {
        key: "bode amp",
        scpi: Scpi::Cmd("AMPLitude"),
        ty: Ty::Real { range: None, shown: |x| si_short(x, "Vpp") },
        label: "Amplitude",
        #[cfg(test)]
        doc: "The amplitude the generator puts out, peak to peak, 10 mVpp to 20 Vpp. The menu shows it in Vpp or Vrms \
              (Amp unit) and steps by 10 mV up to 100 mV and by 50 mV above; a tap on the value opens the number pad \
              for any value in that range. The command takes and answers Vpp.",
        accept: Some(AMP_RANGE),
        step: Some(|m, _, d| Val::F64(amp_step(m.bode_amp, m.bode_rms, d))),
        get: |m, _| Val::F64(m.bode_amp),
        put: |m, _, v| m.bode_amp = v.real(),
    },
];

/// Written after `bode`.
pub const LINK: [Setting; 1] = [Setting {
    key: "bode_link",
    scpi: Scpi::none("set with the address by `:BODE:GENerator`"),
    ty: Ty::Choice(
        &[Choice { store: "lan", scpi: "", label: "LAN" }, Choice { store: "usb", scpi: "", label: "USB" }],
        Unknown::Text("not lan or usb:"),
        Arg::Keyword,
    ),
    label: "Connection",
    #[cfg(test)]
    doc: "How an external generator is reached: on the network at its address, or on the USB-A port. The menu \
          changes it on an odd step.",
    accept: None,
    step: Some(|m, _, d| odd(m.bode_usb, d)),
    get: |m, _| Val::Index(m.bode_usb as usize),
    put: |m, _, v| m.bode_usb = v.index() != 0,
}];

/// The sync edges by their place in the menu.
const EDGES: [Slope; 2] = [Slope::Rise, Slope::Fall];

/// The fields of the `bode_sweep` line (the free sweep).
pub const FREE: [Setting; 5] = [
    Setting {
        key: "bode_sweep time",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::Real { range: None, shown: |x| si_short(x, "s") },
        label: "Sweep time",
        #[cfg(test)]
        doc: "How long the generator takes for one sweep. The menu steps 1-2-5 from 1 ms to 500 s.",
        accept: None,
        step: Some(|m, _, d| Val::F64(ladder(&decades(-3, 3, false), m.bode_sw_time, d))),
        get: |m, _| Val::F64(m.bode_sw_time),
        put: |m, _, v| m.bode_sw_time = v.real(),
    },
    Setting {
        key: "bode_sweep log",
        scpi: Scpi::none(NO_COMMAND_YET),
        // the line keeps the switch as `true` or `false`
        ty: Ty::Choice(
            &[Choice { store: "false", scpi: "", label: "Lin" }, Choice { store: "true", scpi: "", label: "Log" }],
            Unknown::Value,
            Arg::Keyword,
        ),
        label: "Sweep",
        #[cfg(test)]
        doc: "Whether the frequencies of the sweep are spaced linearly (equal steps) or logarithmically (equal \
              ratio), and the frequency axis of the plot with them; with the free sweep also how the generator \
              sweeps over the sweep time. The menu changes it on an odd step.",
        accept: None,
        step: Some(|m, _, d| odd(m.bode_sw_log, d)),
        get: |m, _| Val::Index(m.bode_sw_log as usize),
        put: |m, _, v| m.bode_sw_log = v.index() != 0,
    },
    Setting {
        key: "bode_sweep edge",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::Choice(
            &[
                Choice { store: "Rise", scpi: "", label: "Rising" },
                Choice { store: "Fall", scpi: "", label: "Falling" },
            ],
            Unknown::Value,
            Arg::Keyword,
        ),
        label: "Sync edge",
        #[cfg(test)]
        doc: "The edge of the sync channel that marks the start of a sweep. The menu changes it on an odd step.",
        accept: None,
        step: Some(|m, _, d| odd(m.bode_sync_slope == Slope::Fall, d)),
        get: |m, _| Val::Index((m.bode_sync_slope == Slope::Fall) as usize),
        put: |m, _, v| m.bode_sync_slope = EDGES[v.index()],
    },
    Setting {
        key: "bode_sweep level",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::Real { range: None, shown: |x| si_short(x, "V") },
        label: "Sync level",
        #[cfg(test)]
        doc: "The level of the sync edge. The menu steps by 0.1 V from -20 V to 20 V.",
        accept: None,
        step: Some(|m, _, d| {
            Val::F32((((m.bode_sync_level * 10.0).round() as i64 + d as i64).clamp(-200, 200) as f32) / 10.0)
        }),
        get: |m, _| Val::F32(m.bode_sync_level),
        put: |m, _, v| m.bode_sync_level = v.secs(),
    },
    Setting {
        key: "bode_sweep ref",
        scpi: Scpi::none(NO_COMMAND_YET),
        ty: Ty::Real { range: None, shown: |x| format!("{:.1} dBV", x) },
        label: "Ref level",
        #[cfg(test)]
        doc: "The level the gain is measured against when the free sweep has no input channel, in dBV. The menu \
              steps by 1 dB from -80 to 40 dBV.",
        accept: None,
        step: Some(|m, _, d| Val::F64((m.bode_ref.round() + d as f64).clamp(-80.0, 40.0))),
        get: |m, _| Val::F64(m.bode_ref),
        put: |m, _, v| m.bode_ref = v.real(),
    },
];
