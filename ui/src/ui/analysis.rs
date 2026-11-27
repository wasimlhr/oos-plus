// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The analysers in the plot area: spectrum analyser (RBW, trace modes, peak markers, waterfall,
//! THD/SNR/SFDR), eye diagram with jitter (eyepage.rs), power analysis, and Identify (names the signals and applies a
//! trigger and decoder). The maths is in oos-dsp (`spectrum`, `eye`, `power`, `identify`); this module holds the menus
//! (`analysis/menu.rs`), the per-record update on every captured sample (`Ui::meas_wave`, `analysis/update.rs`), the
//! drawing (`analysis/draw.rs`) and the measurements of channels and references (`analysis/measure.rs`); this file
//! keeps the state they share.

use super::magnetic::missing_wave;
use super::worker::Worker;
use super::*;
use oos_dsp::{eye, identify, power, spectrum};
use std::{
    cell::{Cell, RefCell},
    sync::Arc,
};

mod draw;
mod measure;
#[cfg(test)]
mod measurement_gate_tests;
mod menu;
mod update;

pub(in crate::ui) use draw::{PowerPage, SpectrumPage};
use measure::*;
pub(in crate::ui) use measure::{Measured, RefMeasureKey, Sources};
pub(in crate::ui) use menu::FREQ_TOP;

#[cfg(test)]
#[path = "channel_measurement_tests.rs"]
mod channel_measurement_tests;
#[path = "measure_own.rs"]
mod own;

/// Analyser state kept between records.
#[derive(Default)]
pub struct State {
    pub(super) catalog_measurements: RefCell<super::meascatalog::backend::State>,
    /// The shared worker survives replay resets and additive imports.
    pub(super) reference_search: RefCell<super::reference_search::Cache>,
    pub(super) reference_decode: RefCell<super::reference_decode::Cache>,
    pub(super) measure_pairs: RefCell<super::measure_pairs::State>,
    /// Shared analyses avoid rescanning or copying a long record for each readout.
    ref_measure: [RefCell<Option<RefMeasureCache>>; NREF],
    ref_measure_pending: [Cell<Option<RefMeasureKey>>; NREF],
    ch_measure: [RefCell<Option<RefMeasureCache>>; NCH],
    ch_measure_pending: [Cell<Option<RefMeasureKey>>; NCH],
    ref_measure_worker: RefCell<Option<RefMeasureWorker>>,
    own_measure: RefCell<own::State>,
    measure_view: Option<(u32, u32, bool)>,
    pub spec: Option<spectrum::Spectrum>,
    pub spec_avg: Option<spectrum::Avg>,
    pub wf: Option<spectrum::Waterfall>,
    pub dist: Option<spectrum::Distortion>,
    /// the lines for table and markers, in the order of the table
    pub peaks: Vec<spectrum::Marker>,
    /// top of the band the pane follows (0 = none yet), and when the Results pane last had a spectrum
    pub spec_stop: f64,
    pub spec_t: f64,
    /// the limit line, and of the traces since it was set: how many, how many went over, the worst bin
    pub limit: Option<spectrum::Limit>,
    pub limit_n: u64,
    pub limit_over: u64,
    pub limit_worst: Option<(f64, f32)>,
    pub eye: Option<eye::Eye>,
    pub(super) eye_auto: super::eyepage::automatic::State,
    /// what the eye was built for (source, clock, scale, offset, rate, raster size)
    pub eye_key: (u8, usize, u8, usize, u32, u32, u64, i32, i32),
    /// the ref record already folded in, so a second pass does not count it again
    pub eye_ref: Option<(usize, u64)>,
    pub eye_sim_revision: Option<(usize, u64)>,
    pub eye_res: Option<(eye::EyeMeas, eye::Jitter, f64)>,
    pub page: super::eyepage::Page,
    pub power: Vec<String>,
    /// the Switching page's figures as a table (a row of cells each: name, mean, min, max, standard deviation, count)
    /// and the plot of the voltage against the current: (V, I, kind), the kind 1 in a turn-on, 2 in a turn-off, else 0
    pub power_table: Vec<[String; 6]>,
    pub power_traj: Vec<(f32, f32, u8)>,
    pub mag: Option<oos_dsp::magnetic::Magnetic>,
    pub mag_note: String,
    pub dq: Option<oos_dsp::motor::Dq0>,
    pub mech: Option<oos_dsp::motor::Mech>,
    pub inv_note: String,
    pub dpt: Option<oos_dsp::dpt::Pulse>,
    /// E on / E off from `power::switching`, printed beside the double-pulse figures
    pub dpt_loss: Option<(f64, f64)>,
    pub dpt_note: String,
    /// min/max of Vds, Id, Vgs inside the zoom window
    pub dpt_t: (f64, f64),
    pub dpt_v: Vec<(f32, f32)>,
    pub dpt_i: Vec<(f32, f32)>,
    pub dpt_g: Vec<(f32, f32)>,
    pub findings: Vec<identify::Finding>,
    /// protocol watch: on/off, the bus last named per channel group, time of the last check
    pub watch: bool,
    pub watch_seen: Vec<String>,
    pub watch_t: f64,
    /// bandwidth test (bwtest.rs): result, info line, time of the last update
    pub bw: Option<oos_dsp::bandwidth::StepResponse>,
    pub bw_info: String,
    pub bw_t: f64,
    /// the monitor view's page results while the panel's are in place (`swap_monitor`)
    monitor: Option<Box<State>>,
}

impl State {
    /// Puts the page results of the monitor view (Display > Monitor view, monitor.rs) in place of the panel's, or the
    /// panel's back: each view works out and draws its own spectrum, eye and figures. The measurement caches and
    /// workers stay shared.
    pub(super) fn swap_monitor(&mut self) {
        let mut o = self.monitor.take().unwrap_or_default();
        macro_rules! swap {
            ($($f:ident),*) => { $(std::mem::swap(&mut self.$f, &mut o.$f);)* };
        }
        swap!(spec, spec_avg, wf, dist, peaks, spec_stop, spec_t, limit, limit_n, limit_over, limit_worst, eye,
              eye_auto, eye_key, eye_ref, eye_sim_revision, eye_res, page, power, mag, mag_note, dq, mech, inv_note,
              dpt, dpt_loss, dpt_note, dpt_t, dpt_v, dpt_i, dpt_g);
        self.monitor = Some(o);
    }
}
