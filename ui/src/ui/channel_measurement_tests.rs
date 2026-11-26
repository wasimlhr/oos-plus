// SPDX-License-Identifier: GPL-2.0-or-later
use super::*;
use std::time::{Duration, Instant};

fn native(count: usize) -> Ui {
    let mut ui = Ui::new(Source::Demo { t: 0.0 });
    ui.m.run = RunState::Stop;
    ui.m.ch.iter_mut().enumerate().for_each(|(c, ch)| ch.on = c == 0);
    ui.m.meas = vec![(Meas::Vpp, WaveId::Ch(0))];
    ui.m.meas2.clear();
    ui.m.meas_full_record = true;
    let values: Vec<f32> = (0..count).map(|k| if k % 40 < 20 { -0.25 } else { 0.25 }).collect();
    let basic = crate::board::BasicStats::of(&values);
    let frame = Arc::new(crate::board::Frame {
        seq: 1,
        dt: 1e-9,
        pre: count / 2,
        ch: [values, Vec::new(), Vec::new(), Vec::new()],
        basic: [basic, Default::default(), Default::default(), Default::default()],
        ..Default::default()
    });
    let mut record = crate::wave::to_record(&frame, &ui.m, 3600, count as f64 * 1e-9, -(count as f64) * 0.5e-9);
    // A physical origin away from trigger zero catches timestamp/gate errors.
    record.raw.as_mut().unwrap().t0 = 0.01 + record.raw.as_ref().unwrap().i0 as f64 * frame.dt;
    ui.last[0] = Some(record);
    ui
}

fn wait(ui: &mut Ui) -> (Arc<oos_dsp::measure::Analysis>, f64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        ui.poll_reference_measurements();
        if let Some(result) = ui.channel_measurement(0, true) {
            return result;
        }
        assert!(Instant::now() < deadline, "CH measurement worker did not finish");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn native_full_basic_values_are_inline_and_do_not_start_edge_analysis() {
    let mut ui = native(600_000);
    let before = ui.measurement_statistics_snapshot_for_test();
    let start = Instant::now();
    ui.measure_last();
    for _ in 0..100 {
        assert_eq!(ui.meas_at(WaveId::Ch(0), Meas::Vpp), 0.5);
        assert!(ui.fact_meas(Meas::Vavg, WaveId::Ch(0)).abs() < 1e-15);
        assert_eq!(ui.meas_at(WaveId::Ch(0), Meas::Vrms), 0.25);
        assert!((ui.meas_at(WaveId::Ch(0), Meas::StdDev) - 0.25).abs() < 1e-15);
    }
    assert!(start.elapsed() < Duration::from_millis(250));
    assert!(ui.azr.ref_measure_worker.borrow().is_none());
    assert_eq!(ui.measurement_statistics_snapshot_for_test(), before, "polling never counts an acquisition");
    assert!(ui.measured.ch[0].rises.is_empty());
    assert!(!ui.measurement_busy());
}

#[test]
fn native_full_edge_job_is_nonblocking_shared_and_reused_after_pan() {
    let mut ui = native(600_000);
    ui.m.meas.push((Meas::Freq, WaveId::Ch(0)));
    let start = Instant::now();
    ui.measure_last();
    assert!(start.elapsed() < Duration::from_millis(250));
    assert!(ui.measurement_busy());
    assert!(ui.meas_at(WaveId::Ch(0), Meas::Freq).is_nan());
    let rows = ui.meas_rows();
    assert!(!rows[0].computing && rows[1].computing);
    assert_eq!(ui.azr.ref_measure_worker.borrow().as_ref().unwrap().submitted, 1);
    let (analysis, t0) = wait(&mut ui);
    assert!((t0 - 0.01).abs() < 1e-12);
    assert!((ui.meas_at(WaveId::Ch(0), Meas::Freq) - 25e6).abs() < 1.0);
    let reads = Instant::now();
    for _ in 0..1_000 {
        assert!((ui.fact_meas(Meas::PWidth, WaveId::Ch(0)) - 20e-9).abs() < 1e-12);
        assert!((ui.meas_at(WaveId::Ch(0), Meas::PDuty) - 50.0).abs() < 1e-6);
    }
    assert!(reads.elapsed() < Duration::from_millis(150), "cached width/duty reads took {:?}", reads.elapsed());
    ui.m.delay = 1.0;
    let (again, again_t0) = ui.channel_measurement(0, true).unwrap();
    assert!(Arc::ptr_eq(&analysis, &again));
    assert_eq!(again_t0, t0);
    assert_eq!(ui.azr.ref_measure_worker.borrow().as_ref().unwrap().submitted, 1);
    assert_eq!((ui.m.run, ui.counts.records), (RunState::Stop, 0));
}

#[test]
fn large_screen_pan_rejects_old_gate_and_preserves_native_timebase() {
    let mut ui = native(1_200_000);
    ui.m.meas_full_record = false;
    ui.m.meas = vec![(Meas::Freq, WaveId::Ch(0))];
    ui.m.tscale = tscales().iter().position(|&t| (t - 50e-6).abs() < 1e-10).unwrap();
    ui.m.delay = 0.0103;
    let first = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(first.len > BACKGROUND_MEASUREMENT_POINTS);
    assert!(ui.channel_measurement(0, true).is_none());
    ui.m.delay = 0.0109;
    let second = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(first != second);
    assert!(ui.channel_measurement(0, true).is_none());
    let (analysis, t0) = wait(&mut ui);
    assert_eq!(ui.azr.ch_measure[0].borrow().as_ref().unwrap().key, second);
    assert!(t0 > 0.0106 && t0 < 0.0107);
    assert!((analysis.get(Meas::Period) - 40e-9).abs() < 1e-12);
    assert_eq!(ui.last[0].as_ref().unwrap().raw.as_ref().unwrap().frame.dt, 1e-9);
    assert_eq!((ui.m.run, ui.counts.records), (RunState::Stop, 0));
}

#[test]
fn native_cache_rejects_conditioning_changes_and_new_acceptance_without_mutating_samples() {
    let mut ui = native(600_000);
    ui.m.meas = vec![(Meas::Freq, WaveId::Ch(0))];
    let frame = Arc::clone(&ui.last[0].as_ref().unwrap().raw.as_ref().unwrap().frame);
    let original = frame.ch[0][0].to_bits();
    let first = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(ui.channel_measurement(0, true).is_none());
    ui.m.ch[0].invert = true;
    ui.m.ch[0].coupling = Coupling::Ac;
    ui.m.ch[0].bw = BW_20M;
    let changed = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(first != changed);
    let _ = wait(&mut ui);
    assert_eq!(ui.azr.ch_measure[0].borrow().as_ref().unwrap().key, changed);
    assert_eq!(frame.ch[0][0].to_bits(), original, "settings invalidate results, not immutable captures");
    // Replay can keep seq=1 and an allocator may repeat addresses. The accepted
    // acquisition counter must still invalidate that otherwise identical key.
    ui.counts.wfm_total += 1;
    let accepted = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(accepted != changed);
    assert!(ui.channel_measurement(0, true).is_none());
    let _ = wait(&mut ui);
    assert_eq!(ui.azr.ch_measure[0].borrow().as_ref().unwrap().key, accepted);
    assert_eq!(frame.ch[0][0].to_bits(), original);
    assert_eq!((ui.m.run, ui.counts.records), (RunState::Stop, 0));
}

#[test]
fn a_row_writes_ratios_and_counts_without_a_prefix() {
    // breaks when a row formats a value without an SI unit with SI prefixes again: "498 m", "1.00", "1.50 krpm" (#198)
    let row = |v: f64, unit: &'static str| {
        let row = tools::MeasRow {
            name: String::new(),
            src: WaveId::Ch(0),
            v,
            unit,
            computing: false,
            reason: String::new(),
        };
        (row.fmt(v), row.fmt4(v))
    };
    assert_eq!(row(0.498, ""), ("0.498".into(), "0.4980".into()));
    assert_eq!(row(1.0, ""), ("1".into(), "1".into()));
    assert_eq!(row(1500.0, "rpm"), ("1500 rpm".into(), "1500 rpm".into()));
    assert_eq!(row(0.002, "V"), ("2.00 mV".into(), "2.000 mV".into()));
}

#[test]
fn measurement_values_without_an_si_unit_take_no_prefix() {
    // breaks when a ratio gets an SI prefix again ("498 m", #198)
    assert_eq!(tools::meas_digits(0.498, "", 3), "0.498");
    assert_eq!(tools::meas_digits(-0.0123, "", 3), "-0.0123");
    assert_eq!(tools::meas_digits(12.345, "", 3), "12.3");
    assert_eq!(tools::meas_digits(9.996, "", 3), "10.0");
    assert_eq!(tools::meas_digits(0.4981, "", 4), "0.4981");
    // breaks when a count is written with decimals again ("1.00")
    assert_eq!(tools::meas_digits(1.0, "", 3), "1");
    assert_eq!(tools::meas_digits(0.0, "", 3), "0");
    assert_eq!(tools::meas_digits(123456.0, "", 3), "123456");
    // a large count and a tiny ratio go to exponent form
    assert_eq!(tools::meas_digits(2.5e7, "", 3), "2.50e7");
    assert_eq!(tools::meas_digits(1.5e-5, "", 3), "1.50e-5");
    // breaks when the units that are not SI get a prefix again ("1.50 krpm", "12.0 nppm")
    assert_eq!(tools::meas_digits(1500.0, "rpm", 3), "1500 rpm");
    assert_eq!(tools::meas_digits(-250.0, "rpm/s", 3), "-250 rpm/s");
    assert_eq!(tools::meas_digits(12.0, "ppm", 3), "12.0 ppm");
    assert_eq!(tools::meas_digits(0.012, "ppm", 3), "0.0120 ppm");
    assert_eq!(tools::meas_digits(f64::NAN, "ppm", 3), "---- ppm");
    // the SI units keep their prefix
    assert_eq!(tools::meas_digits(0.498, "V", 3), si_digits(0.498, "V", 3));
    assert_eq!(tools::meas_digits(1.0, "A", 4), "1.000 A");
}

/// A 40 Hz sine as min/max pairs of a roll stream (as the dsp test `min_max_pairs_give_the_frequency_of_the_signal`):
/// 1800 pairs over 2 s, each the extremes of the two spans around it.
fn sine_pairs_record(pairs: bool) -> Record {
    let (span, dt) = (40i64, 2.0 / 1800.0 / 40.0);
    let x = |i: i64| (2.0 * std::f64::consts::PI * 40.0 * i as f64 * dt + 0.3).sin() as f32;
    let v: Vec<f32> = (0..1800i64)
        .flat_map(|j| {
            let mid = j * span + span / 4;
            let (lo, hi) = (mid - span..mid + span).map(x).fold((f32::MAX, f32::MIN), |(l, h), y| (l.min(y), h.max(y)));
            [hi, lo]
        })
        .collect();
    Record {
        t0: -1.0,
        dt: span as f64 * dt / 2.0,
        ch: [v, Vec::new(), Vec::new(), Vec::new()],
        triggered: true,
        partial: false,
        raw: None,
        la: None,
        pairs,
        peak: false,
    }
}

// Break it catches: a channel of min/max pairs (roll, scan, compressed deep records) measured in time on the pairs
// themselves, whose zigzag on every edge counted extra edges (upstream #118: 25 Hz read up to 50 Hz); the pair mode
// left out of the cache key, so a cached analysis of pairs answered after high res was switched on while stopped.
#[test]
fn min_max_pairs_are_measured_in_time_on_the_mean_of_neighbours_on_every_path() {
    let mut ui = Ui::new(Source::Demo { t: 0.0 });
    ui.m.run = RunState::Stop;
    ui.m.ch.iter_mut().enumerate().for_each(|(c, ch)| ch.on = c == 0);
    ui.m.meas = vec![(Meas::Freq, WaveId::Ch(0)), (Meas::Vpp, WaveId::Ch(0))];
    ui.m.meas2.clear();
    ui.m.meas_full_record = true;
    // the frame loop's path (measure_channels) and the one of SCPI and the screen numbers (channel_measurement)
    let record = sine_pairs_record(true);
    ui.measure_rec(&record);
    assert!(
        (ui.measured.ch[0].get(Meas::Freq) - 40.0).abs() < 0.05,
        "measure_channels: {}",
        ui.measured.ch[0].get(Meas::Freq)
    );
    ui.last[0] = Some(record);
    ui.measure_last();
    let key = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(key.pairs);
    assert!(
        (ui.meas_at(WaveId::Ch(0), Meas::Freq) - 40.0).abs() < 0.05,
        "channel_measurement: {}",
        ui.meas_at(WaveId::Ch(0), Meas::Freq)
    );
    let vpp = ui.meas_at(WaveId::Ch(0), Meas::Vpp);
    assert!(vpp > 1.998, "Pk-pk from the pairs: {}", vpp);
    // the same points not flagged as pairs: the zigzag shows, so the flag is what the test above measures
    let mut ui2 = Ui::new(Source::Demo { t: 0.0 });
    ui2.m.run = RunState::Stop;
    ui2.m.ch.iter_mut().enumerate().for_each(|(c, ch)| ch.on = c == 0);
    ui2.m.meas = ui.m.meas.clone();
    ui2.m.meas2.clear();
    ui2.m.meas_full_record = true;
    ui2.last[0] = Some(sine_pairs_record(false));
    ui2.measure_last();
    assert!((ui2.meas_at(WaveId::Ch(0), Meas::Freq) - 40.0).abs() > 5.0, "the zigzag must be a test here");
    // high res averages the pairs: another key, measured again on the points
    ui.m.acq = AcqMode::HiRes;
    let hires = ui.channel_measurement_key(0, false).unwrap().0;
    assert!(!hires.pairs);
    assert!(hires != key);
    assert!(ui.channel_measurement(0, true).is_some_and(|(a, _)| (a.get(Meas::Freq) - 40.0).abs() > 5.0));
}

// A record that is replaced before the worker has measured it (seen on a unit 2026-11-20: a 40 MHz square in 1 Mpts at
// 20 us/div).

/// The next record of a running acquisition: the same signal, a new frame, its samples `shift` earlier (the trigger
/// falls at another sample).
fn next_record(u: &mut Ui, shift: usize) {
    let mut last = u.last[0].as_ref().unwrap().raw.as_ref().unwrap().frame.as_ref().clone();
    last.ch[0].rotate_left(shift);
    let frame = Arc::new(crate::board::Frame { seq: last.seq + 1, ..last });
    let count = frame.ch[0].len() as f64;
    u.last[0] = Some(crate::wave::to_record(&frame, &u.m, 3600, count * 1e-9, -count * 0.5e-9));
    u.counts.wfm_total += 1;
    u.measure_last();
}

/// Polls until the worker's answer is taken and the next record is given to it.
fn answer(u: &mut Ui) {
    let jobs = |u: &Ui| u.azr.ref_measure_worker.borrow().as_ref().map_or(0, |w| w.submitted);
    let before = jobs(u);
    let deadline = Instant::now() + Duration::from_secs(20);
    while jobs(u) == before {
        assert!(Instant::now() < deadline, "the measurement worker did not answer");
        std::thread::sleep(Duration::from_millis(2));
        u.poll_reference_measurements();
    }
}

#[test]
fn a_running_record_larger_than_the_worker_is_fast_shows_the_value_of_the_record_before() {
    let mut u = native(600_000);
    u.m.meas = vec![(Meas::Freq, WaveId::Ch(0)), (Meas::Rise, WaveId::Ch(0))];
    u.m.meas2.clear();
    u.m.run = RunState::Run;
    let freq = |u: &Ui| u.meas_at(WaveId::Ch(0), Meas::Freq);
    next_record(&mut u, 0);
    assert!(freq(&u).is_nan() && u.meas_rows()[0].computing, "nothing is measured yet");
    // every answer comes after the next record is there
    for round in 0..4 {
        next_record(&mut u, 0);
        answer(&mut u);
        assert!((freq(&u) - 25e6).abs() < 1e3, "round {round}: {}", freq(&u));
        assert!(u.meas_rows().iter().all(|r| !r.computing), "round {round}: a row says Computing");
    }
    // other settings: the value of the old ones does not stand in
    u.m.meas_hpct = 80.0;
    next_record(&mut u, 0);
    answer(&mut u);
    assert!(freq(&u).is_nan() && u.meas_rows()[0].computing, "an answer for other levels is not shown");
    next_record(&mut u, 0);
    answer(&mut u);
    assert!((freq(&u) - 25e6).abs() < 1e3, "{}", freq(&u));
    // stopped, the record stays and its own answer comes
    u.m.run = RunState::Stop;
    next_record(&mut u, 0);
    assert!(freq(&u).is_nan(), "a stopped record is not given the value of another");
}

/// The trigger of a running acquisition falls at another sample from record to record, so each record starts at
/// another time: still the same settings, the value of the record before stands in. A 250 kHz square, slow enough for
/// the trigger to find its edge (a faster one is smoothed away and the hardware point stands).
#[test]
fn a_running_record_whose_trigger_moves_shows_the_value_of_the_record_before() {
    let mut u = native(600_000);
    u.m.meas = vec![(Meas::Freq, WaveId::Ch(0))];
    u.m.meas2.clear();
    u.m.run = RunState::Run;
    u.m.trig_level = 0.0;
    let values: Vec<f32> = (0..600_000).map(|k| if k % 4000 < 2000 { -1.0 } else { 1.0 }).collect();
    let basic = crate::board::BasicStats::of(&values);
    let frame = crate::board::Frame {
        seq: 1,
        dt: 1e-9,
        pre: 300_000,
        ch: [values, Vec::new(), Vec::new(), Vec::new()],
        basic: [basic, Default::default(), Default::default(), Default::default()],
        ..Default::default()
    };
    u.last[0] = Some(crate::wave::to_record(&Arc::new(frame), &u.m, 3600, 600e-6, -300e-6));
    let freq = |u: &Ui| u.meas_at(WaveId::Ch(0), Meas::Freq);
    let start = |u: &Ui| u.channel_measurement_key(0, false).map(|k| k.1);
    next_record(&mut u, 0);
    let first = start(&u);
    for round in 0..4 {
        next_record(&mut u, 3);
        assert_ne!(start(&u), first, "round {round}: the record starts at another time");
        answer(&mut u);
        assert!((freq(&u) - 250e3).abs() < 10.0, "round {round}: {}", freq(&u));
        assert!(u.meas_rows().iter().all(|r| !r.computing), "round {round}: a row says Computing");
    }
    // another coupling of the channel: the value of the old one does not stand in
    u.m.ch[0].invert = true;
    next_record(&mut u, 3);
    assert!(freq(&u).is_nan() && u.meas_rows()[0].computing, "a value of the channel before inverting is not shown");
}

/// In roll and scan the last finished value can be from before a change of the signal: it does not stand in.
#[test]
fn a_running_roll_or_scan_record_does_not_show_the_value_of_the_record_before() {
    for (roll, scan) in [(true, false), (false, true)] {
        let mut u = native(600_000);
        u.m.meas = vec![(Meas::Freq, WaveId::Ch(0))];
        u.m.meas2.clear();
        u.m.run = RunState::Run;
        let mut frame = u.last[0].as_ref().unwrap().raw.as_ref().unwrap().frame.as_ref().clone();
        (frame.roll, frame.scan) = (roll, scan);
        u.last[0] = Some(crate::wave::to_record(&Arc::new(frame), &u.m, 3600, 600e-6, -300e-6));
        next_record(&mut u, 0);
        for round in 0..3 {
            next_record(&mut u, 0);
            answer(&mut u);
            let f = u.meas_at(WaveId::Ch(0), Meas::Freq);
            assert!(f.is_nan(), "roll {roll} scan {scan} round {round}: the value of the record before shows ({f})");
        }
    }
}

#[test]
fn a_running_line_without_edges_says_no_value_not_computing() {
    let mut u = native(600_000);
    // a level with a little noise, no edge
    let mut frame = u.last[0].as_ref().unwrap().raw.as_ref().unwrap().frame.as_ref().clone();
    frame.ch[0] = (0..600_000).map(|k| 0.1 + 0.001 * ((k % 7) as f32 - 3.0)).collect();
    frame.basic[0] = crate::board::BasicStats::of(&frame.ch[0]);
    u.last[0].as_mut().unwrap().raw.as_mut().unwrap().frame = Arc::new(frame);
    u.m.meas = vec![(Meas::Freq, WaveId::Ch(0))];
    u.m.meas2.clear();
    u.m.run = RunState::Run;
    next_record(&mut u, 0);
    assert_eq!(u.meas_rows()[0].text(), "Computing", "nothing is measured yet");
    for round in 0..3 {
        next_record(&mut u, 0);
        answer(&mut u);
        assert_eq!(u.meas_rows()[0].text(), "----", "round {round}");
    }
}
