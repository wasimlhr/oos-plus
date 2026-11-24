// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Tests of `:MEASure`, `:CURSor` and the eye page's figures (scpi/measure.rs, scpi/cursor.rs, scpi/analysis.rs). A
//! test of a new command of these goes here; its query also goes into ui/tests/scpi.txt, its line into docs/scpi.md
//! (AGENTS.md).

use super::*;

#[test]
fn selected_catalog_rows_are_exported_and_setup_clear_accepts_an_empty_catalog() {
    let (mut s, mut ui, cv) = session();
    for amplitude in [2.0, 0.5] {
        let mut csv = String::from("Time (s),Volts\n");
        for i in 0..1000 {
            csv.push_str(&format!(
                "{:.12e},{}\n",
                i as f64 * 1e-6,
                amplitude * (i as f64 * std::f64::consts::TAU / 20.0).sin()
            ));
        }
        ui.load_waveform_auto(csv.as_bytes(), "power.csv").unwrap();
    }
    ui.set_offline(true);
    ui.m.meas_full_record = true;
    ui.m.meas.clear();
    ui.m.meas2.clear();
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:SET cat = 90:R1:R2:3:4;:SYST:ERR?"), "0,\"No error\"");
    let end = std::time::Instant::now() + Duration::from_secs(10);
    let answer = loop {
        let answer = line(&mut s, &mut ui, &cv, ":MEAS:STRIP?");
        let value = answer.rsplit(',').next().unwrap().parse::<f64>().unwrap();
        if value.abs() < 1e30 {
            assert!((value - 0.5).abs() < 0.01, "{answer}");
            break answer;
        }
        assert!(std::time::Instant::now() < end, "{answer}");
        ui.step(0.0);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(answer.starts_with("CAT90,R1,R2,CHAN3,CHAN4,"), "{answer}");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:READOUT?"), answer);
    assert_eq!(captures(&ui), 0.0, "SCPI row exports are not acquisitions");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:SET cat = ;:SYST:ERR?"), "0,\"No error\"");
    assert!(ui.m.cat.is_empty());
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STRIP?"), "");
}

#[test]
fn strip_and_readout_export_the_configured_row_levels_and_physical_time() {
    let (mut s, mut ui, cv) = session();
    let mut csv = String::from("Time (s),Volts\n");
    for i in 0..800 {
        let k = i % 400;
        let v = match k {
            0..=99 => -0.5,
            100..=199 => -0.5 + (k - 100) as f64 / 99.0,
            200..=299 => 0.5,
            _ => 0.5 - (k - 300) as f64 / 99.0,
        };
        csv.push_str(&format!("{:.12e},{v}\n", 0.001 + i as f64 * 1e-9));
    }
    ui.load_waveform_bytes(csv.as_bytes(), "row-levels.csv").unwrap();
    ui.m.run = RunState::Stop;
    ui.m.meas_full_record = true;
    let id = WaveId::Ref(0);
    ui.m.meas = vec![(Meas::Rise, id), (Meas::TVmax, id)];
    ui.m.meas2.clear();
    ui.set_meas_refs(0, 0.2, 0.8);
    ui.set_meas_refs(1, 0.2, 0.8);
    let shared = ui.meas_at(id, Meas::Rise);
    let own = ui.fact_meas(Meas::Rise, id);
    assert!(own < shared * 0.85, "row {own}, shared {shared}");
    let time = ui.fact_meas(Meas::TVmax, id);
    assert!((time - (0.001 + 199e-9)).abs() < 1e-13, "{time}");
    let expected = format!("{},R1,{};{},R1,{}", meas_token(Meas::Rise), e(own), meas_token(Meas::TVmax), e(time));
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STRIP?"), expected);
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:READOUT?"), expected);
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
    assert_eq!(captures(&ui), 0.0);
}

#[test]
fn measurement_gating_screen_tracks_pan_while_none_measures_the_full_record() {
    let (mut s, mut ui, cv) = session();
    ui.set_offline(true);
    let mut csv = String::from("Time (s),Volts\n");
    for index in 0..4000 {
        let amplitude = if index < 2000 { 0.25 } else { 0.75 };
        let value = if index % 40 < 20 { -amplitude } else { amplitude };
        csv.push_str(&format!("{},{}\n", index as f64 * 1e-9, value));
    }
    ui.load_waveform_bytes(csv.as_bytes(), "gate-regions.csv").unwrap();
    line(&mut s, &mut ui, &cv, ":TIM:SCAL 1e-8;:TIM:OFFS 1e-6;:MEAS:GAT SCREEN");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:GATING?"), "SCREEN");
    let vpp = |s: &mut Scpi, ui: &mut Ui| line(s, ui, &cv, ":MEAS:ITEM? VPP,R1").parse::<f64>().unwrap();
    let first = vpp(&mut s, &mut ui);
    assert!(
        (first - 0.5).abs() < 1e-6,
        "first screen Vpp {first}, timebase {:?}, delay {}, zoom {}, state {}",
        ui.m.time_per_div(),
        ui.m.delay,
        ui.m.zoom,
        ui.workflow_state()
    );
    line(&mut s, &mut ui, &cv, ":TIM:OFFS 3e-6");
    assert!((vpp(&mut s, &mut ui) - 1.5).abs() < 1e-6, "SCREEN follows the stopped viewport");

    line(&mut s, &mut ui, &cv, ":MEAS:GATING NONE");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:GAT?"), "NONE");
    assert!((vpp(&mut s, &mut ui) - 1.5).abs() < 1e-6);
    line(&mut s, &mut ui, &cv, ":TIM:OFFS 1e-6");
    assert!((vpp(&mut s, &mut ui) - 1.5).abs() < 1e-6, "NONE keeps the full record while panning");

    line(&mut s, &mut ui, &cv, ":MEAS:GAT SCREEN");
    assert!((vpp(&mut s, &mut ui) - 0.5).abs() < 1e-6, "restoring SCREEN recomputes the stopped gate");
    assert_eq!((ui.m.run, captures(&ui)), (RunState::Stop, 0.0), "gate changes and pan must not acquire");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");

    line(&mut s, &mut ui, &cv, ":MEAS:GAT unsupported");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:GAT?"), "SCREEN");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
}

#[test]
fn a_remote_eye_view_analyzes_a_stopped_reference_without_another_capture() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":SIM:PRES \"USB 2.0 HS\";:SIM:PLAY ON;:STOP");
    let acquired = captures(&ui);
    line(&mut s, &mut ui, &cv, ":VIEW EYE");
    let unit_interval: f64 = line(&mut s, &mut ui, &cv, ":EYE:ITEM? UI").parse().unwrap();
    assert!((unit_interval * 480e6 - 1.0).abs() < 0.02, "UI {unit_interval}");
    let edges: f64 = line(&mut s, &mut ui, &cv, ":EYE:ITEM? EDGES").parse().unwrap();
    assert!(edges > 1000.0, "Eye needs the full reference, got {edges} edges");
    assert_eq!(captures(&ui), acquired, "opening Eye must not acquire another window");
    assert_eq!(ui.m.run, RunState::Stop);
}

// Break it catches (seen on a unit, 2026-11-17): after *RST the read-out was still on the channel, RAW and WORD
// of before, a note of the old set-up still hung on the screen and a closed zoom kept its vertical factor. A new
// set-up is in the Default layout with the stock bars, whatever the screen was before.
#[test]
fn a_reset_puts_the_read_out_back_and_starts_in_the_default_layout() {
    let (mut s, mut ui, cv) = session();
    (ui.m.layout, ui.m.bar_compact) = (Layout::Studio, true);
    line(&mut s, &mut ui, &cv, "*CLS;:CHAN2:DISP ON;:WAV:SOUR CHAN2;:WAV:MODE RAW;:WAV:FORM WORD;:WAV:STAR 10;:WAV:STOP 20;:CALL:ADD CHAN2,0,\"old\";:TIM:DEL:VZO 4;*ESE 32");
    line(&mut s, &mut ui, &cv, "*RST");
    assert_eq!(line(&mut s, &mut ui, &cv, ":WAV:SOUR?;:WAV:MODE?;:WAV:FORM?;:WAV:STAR?"), "CHAN1;NORM;BYTE;1");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CALL:COUN?;:TIM:DEL:VZO?;*ESE?"), "0;1.000000E+00;32", "notes and zoom go, the enable register stays (IEEE 488.2)");
    assert!(ui.m.layout == Layout::Default && !ui.m.compact(), "the stock bars");
    // a notice on the screen does not swallow the reset, and stays for the person it is for (on a unit a *RST
    // closed the notice of an update and reset nothing)
    ui.dialog = Some(vec!["Update".into(), "Tap to close".into()]);
    line(&mut s, &mut ui, &cv, ":TIM:SCAL 1e-6;*RST");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TIM:SCAL?"), "2.000000E-04");
    assert!(ui.dialog.is_some(), "the notice stays");
    ui.dialog = None;
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

// Break it catches (seen on a unit, 2026-11-18): a threshold preset was only taken by its whole name, where the
// table of the commands names `TTL` and `CMOS`; the threshold and the preamble of the logic record were written
// as `1.4e0` and `8e-6`, not as every other number.
#[test]
fn a_logic_threshold_preset_by_the_beginning_of_its_name() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, "*CLS;:LA:STAT ON;:LA:POD1:PRES ECL;:LA:POD2:PRES lvcmos_1.8");
    assert_eq!(line(&mut s, &mut ui, &cv, ":LA:POD1:PRES?;:LA:POD2:PRES?;:SYST:ERR?"), "ECL_-1.3_V;LVCMOS_1.8_V:_0.9_V;0,\"No error\"");
    assert_eq!(line(&mut s, &mut ui, &cv, ":LA:POD1:THR?;:LA:POD2:THR?"), "-1.300000E+00;9.000000E-01");
    line(&mut s, &mut ui, &cv, ":LA:POD1:PRES CMOS_5_V:_2.5_V;:LA:POD2:PRES CMOS");
    assert_eq!(line(&mut s, &mut ui, &cv, ":LA:POD1:PRES?;:LA:POD2:PRES?"), "CMOS_5_V:_2.5_V;LVCMOS_1.8_V:_0.9_V", "two names begin with CMOS");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
}

// Break it catches: a set-up (and so a session) did not hold the generator, a recall put it back to 1 kHz (seen on
// a unit, 2026-11-18); and a recall that switches an output on, which it must never do.
#[test]
fn a_setup_holds_the_generator_but_never_switches_its_output_on() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, "*CLS;:SOUR:FUNC SQU;:SOUR:FREQ 12345;:SOUR:VOLT 0.5;:SOUR:VOLT:OFFS 0.1;:SOUR:FUNC:SQU:DUTY 30;:SOUR:OUTP ON");
    let before = line(&mut s, &mut ui, &cv, ":SOUR:FUNC?;:SOUR:FREQ?;:SOUR:VOLT?;:SOUR:VOLT:OFFS?;:SOUR:FUNC:SQU:DUTY?");
    let text = ui.setup_text_full();
    assert!(text.lines().any(|l| l.starts_with("gen = Square 12345 ")), "{:?}", text.lines().filter(|l| l.starts_with("gen")).collect::<Vec<_>>());
    assert!(!text.contains("afg_on") && !text.to_ascii_lowercase().contains("output"), "the output state is not in a set-up");
    line(&mut s, &mut ui, &cv, "*RST");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SOUR:FREQ?;:SOUR:OUTP?"), "1.000000E+03;0");
    line(&mut s, &mut ui, &cv, ":SOUR:OUTP ON");
    ui.apply_setup_text(&text).unwrap();
    assert_eq!(line(&mut s, &mut ui, &cv, ":SOUR:FUNC?;:SOUR:FREQ?;:SOUR:VOLT?;:SOUR:VOLT:OFFS?;:SOUR:FUNC:SQU:DUTY?"), before);
    assert_eq!(line(&mut s, &mut ui, &cv, ":SOUR:OUTP?"), "0", "a recall leaves the output off, also one that was on");
    // a set-up of before the keys leaves the generator at its defaults, and a bad line is an error, not a guess
    let old: String = text.lines().filter(|l| !l.starts_with("gen")).map(|l| format!("{l}
")).collect();
    ui.apply_setup_text(&old).unwrap();
    assert_eq!(line(&mut s, &mut ui, &cv, ":SOUR:FREQ?"), "1.000000E+03");
    assert!(crate::store::set(&mut ui.m, "gen", "Arb 1000 1 0 50 50").is_err() && crate::store::set(&mut ui.m, "gen", "Sine 1e3").is_err());
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

// Break it catches: :TRIGger:ZONE<n>:SOURce set and answered the source of the trigger, so a zone stayed on the
// channel it had and a "must intersect" zone on a channel that is off refused every record (seen on a unit,
// 2026-11-17: zone on CH3, the signal on CH2, no record in any box).
#[test]
fn the_source_of_a_zone_is_not_the_source_of_the_trigger() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, "*CLS;:TRIG:EDGE:SOUR CHAN1;:TRIG:ZONE1:SOUR CHAN3;:TRIG:ZONE2:SOUR CHAN2");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TRIG:EDGE:SOUR?;:TRIG:ZONE1:SOUR?;:TRIG:ZONE2:SOUR?"), "CHAN1;CHAN3;CHAN2");
    assert_eq!((ui.m.trig_src, ui.m.zones[0].src, ui.m.zones[1].src), (0, 2, 1));
    line(&mut s, &mut ui, &cv, ":TRIG:EDGE:SOUR CHAN4");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TRIG:ZONE1:SOUR?;:SYST:ERR?"), "CHAN3;0,\"No error\"");
}

// Break it catches (all seen on a unit, 2026-11-17): :RUN after :SINGle left Normal sweep for Auto; the status
// commands a VISA or IVI driver sends at open were undefined headers; :REFerence:COLor RED was read as a slot.
#[test]
fn run_after_single_keeps_the_sweep_and_the_ieee_status_commands_answer() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":TRIG:SWE NORM;:SING");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TRIG:SWE?"), "SING");
    line(&mut s, &mut ui, &cv, ":RUN");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TRIG:SWE?"), "NORM");
    line(&mut s, &mut ui, &cv, ":TRIG:SWE AUTO;:TRIG:SWE SING;:RUN");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TRIG:SWE?"), "AUTO");
    // the front panel: Single, then Run/Stop twice (stop, run)
    ui.m.trig_mode = TrigMode::Normal;
    ui.key(crate::ui::key::SINGLE);
    ui.m.run = RunState::Stop;
    ui.key(crate::ui::key::RUNSTOP);
    assert_eq!((ui.m.run, ui.m.trig_mode), (RunState::Run, TrigMode::Normal));

    line(&mut s, &mut ui, &cv, "*CLS");
    assert_eq!(line(&mut s, &mut ui, &cv, "*STB?;*ESE?;*SRE?;*PSC?;*TST?;*OPT?"), "0;0;0;1;0;0");
    line(&mut s, &mut ui, &cv, "*ESE 32;*SRE 36;*PSC 0;*WAI");
    assert_eq!(line(&mut s, &mut ui, &cv, "*ESE?;*SRE?;*PSC?"), "32;36;0");
    line(&mut s, &mut ui, &cv, ":NOSUCH:HEADER 1");
    assert_eq!(line(&mut s, &mut ui, &cv, "*STB?"), "100", "error queue, event summary and master summary");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR:NEXT?").starts_with("-113,"));
    assert_eq!(line(&mut s, &mut ui, &cv, "*STB?"), "96", "the queue is empty, the event bit stays until *ESR?");
    assert_eq!(line(&mut s, &mut ui, &cv, "*ESR?;*STB?"), "32;0");
    line(&mut s, &mut ui, &cv, "*ESE 256");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:VERS?"), "1999.0");
    line(&mut s, &mut ui, &cv, "*CLS;*TRG");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

#[test]
fn a_reference_colour_by_name_and_a_track_cursor_y() {
    let (mut s, mut ui, cv) = reference_session();
    line(&mut s, &mut ui, &cv, "*CLS;:REF:CURR 1;:REF:COL RED");
    assert_eq!(line(&mut s, &mut ui, &cv, ":REF:COL?;:SYST:ERR?"), "RED;0,\"No error\"");
    line(&mut s, &mut ui, &cv, ":REF:COL 1,BLUE;:REF:COL R99,RED");
    assert_eq!(line(&mut s, &mut ui, &cv, ":REF:COL? 1"), "BLUE");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"), "a slot that does not exist is still refused");
    line(&mut s, &mut ui, &cv, "*CLS;:CURS:MODE TRAC;:CURS:TRAC:SOUR1 R1;:CURS:TRAC:CAX 0");
    let (y, value) = (line(&mut s, &mut ui, &cv, ":CURS:TRAC:CAY?"), line(&mut s, &mut ui, &cv, ":CURS:TRAC:AYV?"));
    assert_eq!(y, value);
    assert_ne!(y, "9.9E37");
}

#[test]
fn a_math_channel_is_read_and_measured() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":CHAN1:DISP ON;:CHAN2:DISP ON");
    ui.step(0.05);
    line(&mut s, &mut ui, &cv, ":STOP;*CLS;:MATH1:DISP ON;:MATH1:OPER ADD;:MATH1:SOUR1 CHAN1;:MATH1:SOUR2 CHAN1");
    line(&mut s, &mut ui, &cv, ":WAV:SOUR MATH1;:WAV:MODE NORM;:WAV:FORM ASC");
    assert_eq!(line(&mut s, &mut ui, &cv, ":WAV:SOUR?"), "MATH1");
    let points: usize = line(&mut s, &mut ui, &cv, ":WAV:POIN?").parse().unwrap();
    assert!(points > 100, "{points}");
    let data = s.exec_line(&mut ui, &cv, ":WAV:DATA?");
    let sum: Vec<f64> = std::str::from_utf8(&data[11..]).unwrap().split(',').map(|v| v.parse().unwrap()).collect();
    assert_eq!(sum.len(), points);
    line(&mut s, &mut ui, &cv, ":WAV:SOUR CHAN1");
    let data = s.exec_line(&mut ui, &cv, ":WAV:DATA?");
    let one: Vec<f64> = std::str::from_utf8(&data[11..]).unwrap().split(',').map(|v| v.parse().unwrap()).collect();
    assert_eq!(one.len(), sum.len());
    assert!(one.iter().zip(&sum).all(|(a, b)| (2.0 * a - b).abs() < 1e-4), "CH1 + CH1 is twice CH1");
    let (math, chan): (f64, f64) = (line(&mut s, &mut ui, &cv, ":MEAS:ITEM? VPP,MATH1").parse().unwrap(), line(&mut s, &mut ui, &cv, ":MEAS:ITEM? VPP,CHAN1").parse().unwrap());
    assert!(chan > 0.0 && (math / chan - 2.0).abs() < 0.05, "{math} {chan}");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
    // an FFT is a spectrum, not a record in time: no points, and no stale ones of the channel
    line(&mut s, &mut ui, &cv, ":MATH1:OPER FFT;:WAV:SOUR MATH1");
    assert_eq!(line(&mut s, &mut ui, &cv, ":WAV:POIN?"), "0");
    // a stop beyond the record reads back as its last point; a channel that is off has no record to read
    line(&mut s, &mut ui, &cv, ":WAV:SOUR CHAN1;:WAV:STAR 1;:WAV:STOP 99999999");
    assert_eq!(line(&mut s, &mut ui, &cv, ":WAV:STOP?"), points.to_string());
    line(&mut s, &mut ui, &cv, "*CLS;:CHAN3:DISP OFF;:WAV:SOUR CHAN3");
    assert!(line(&mut s, &mut ui, &cv, ":WAV:DATA?").starts_with("-221,"));
    // switched off after the stop, a channel still has its samples of the record shown; running, it has none
    line(&mut s, &mut ui, &cv, "*CLS;:CHAN2:DISP OFF;:WAV:SOUR CHAN2");
    assert!(s.exec_line(&mut ui, &cv, ":WAV:DATA?").starts_with(b"#9"), "CH2 was on when the record was taken");
    line(&mut s, &mut ui, &cv, ":RUN");
    ui.step(0.05);
    assert!(line(&mut s, &mut ui, &cv, ":WAV:DATA?").starts_with("-221,"));
}

/// The bench report of a FlexRay run: a 100 kpt memory read in pages of 25 kpts, in every mode and format, running and
/// stopped. NORMal and MAXimum (running) answer the screen record, RAW the whole memory; no combination stays silent.
/// The body runs on its own thread and the test waits a fixed time for it, so a hang fails instead of stalling the
/// suite, and a slow machine does not change the result.
#[test]
fn a_paged_read_of_a_100_kpt_memory_answers_in_every_mode_and_format() {
    let (done, wait) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let (mut s, mut ui, cv) = session();
        line(&mut s, &mut ui, &cv, ":CHAN1:DISP ON;:ACQ:MDEP 100k");
        let mut seen = Vec::new();
        for state in [":RUN", ":STOP"] {
            line(&mut s, &mut ui, &cv, state);
            // the record of the 100 kpt memory comes after a few steps; counted in steps, not in time
            line(&mut s, &mut ui, &cv, ":WAV:SOUR CHAN1;:WAV:MODE RAW;:WAV:STAR 1;:WAV:STOP 100000");
            let mut steps = 0;
            while line(&mut s, &mut ui, &cv, ":WAV:POIN?") != "100000" && steps < 2000 {
                ui.step(0.05);
                std::thread::sleep(Duration::from_millis(1));
                steps += 1;
            }
            for mode in ["NORM", "MAX", "RAW"] {
                for (format, width) in [("BYTE", 1), ("WORD", 2), ("ASC", 0)] {
                    line(&mut s, &mut ui, &cv, &format!("*CLS;:WAV:SOUR CHAN1;:WAV:MODE {mode};:WAV:FORM {format}"));
                    line(&mut s, &mut ui, &cv, ":WAV:STAR 1;:WAV:STOP 100000");
                    let points: usize = line(&mut s, &mut ui, &cv, ":WAV:POIN?").parse().unwrap();
                    let (mut total, mut start) = (0, 1);
                    while start <= points {
                        let stop = (start + 24_999).min(points);
                        line(&mut s, &mut ui, &cv, &format!(":WAV:STAR {start};:WAV:STOP {stop}"));
                        let block = s.exec_line(&mut ui, &cv, ":WAV:DATA?");
                        assert!(block.starts_with(b"#9"), "{state} {mode} {format}: {:?}", String::from_utf8_lossy(&block));
                        let length: usize = std::str::from_utf8(&block[2..11]).unwrap().parse().unwrap();
                        assert_eq!(block.len(), 11 + length, "{state} {mode} {format}");
                        total += if width == 0 { block[11..].split(|&b| b == b',').count() } else { length / width };
                        start = stop + 1;
                    }
                    assert_eq!(total, points, "{state} {mode} {format}");
                    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"", "{state} {mode} {format}");
                    seen.push((state, mode, points));
                }
            }
        }
        let _ = done.send(seen);
    });
    let seen = wait.recv_timeout(Duration::from_secs(120)).expect("a paged read did not answer");
    for (state, mode, points) in seen {
        if mode == "RAW" || (mode == "MAX" && state == ":STOP") {
            assert_eq!(points, 100_000, "{state} {mode}: the memory");
        } else {
            assert!(points < 100_000, "{state} {mode}: the screen record");
        }
    }
}

#[test]
fn saved_files_are_listed_fetched_and_deleted_and_the_memory_is_saved() {
    let dir = std::env::temp_dir().join(format!("oos-mmem-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (mut s, mut ui, cv) = session();
    ui.files.dir = dir.clone();
    ui.step(0.05);
    line(&mut s, &mut ui, &cv, ":STOP;*CLS;:SAVE:DEST DESK;:SAVE:WAV \"a.csv\";:SAVE:MEM:WAV \"m.bin\";:SAVE:MEM:WAV \"m.csv\"");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
    // a reference exported under a name is loaded by that name (it lies in the save folder, and the load looked
    // beside the setups only: -256 on a unit, 2026-11-18)
    line(&mut s, &mut ui, &cv, ":REF:CURR 1;:REF:SOUR 1,CHAN1;:REF:SAVE;:REF:EXP \"r-export.csv\";:REF:CURR 2;:REF:LOAD \"r-export.csv\"");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
    line(&mut s, &mut ui, &cv, ":WAV:SOUR R2");
    assert!(line(&mut s, &mut ui, &cv, ":WAV:POIN?").parse::<usize>().unwrap() > 100);
    line(&mut s, &mut ui, &cv, ":WAV:SOUR CHAN1;:REF:CLE;:MMEM:DEL \"r-export.csv\"");
    let catalog = line(&mut s, &mut ui, &cv, ":MMEM:CAT?");
    let size = std::fs::metadata(dir.join("a.csv")).unwrap().len();
    assert!(catalog.contains(&format!("\"a.csv,,{size}\"")) && catalog.contains("\"m.bin,,") && catalog.contains("\"m.csv,,"), "{catalog}");
    let data = s.exec_line(&mut ui, &cv, ":MMEM:DATA? \"a.csv\"");
    assert_eq!(&data[..11], format!("#9{size:09}").as_bytes());
    assert_eq!(&data[11..], std::fs::read(dir.join("a.csv")).unwrap());
    assert!(std::fs::read(dir.join("m.bin")).unwrap().starts_with(b"OOSW"));
    // only plain names of the save folder: no way up or out of it
    for name in ["../a.csv", "/etc/passwd", "sub/a.csv", "..", "", "C:a.csv", "a.csv:x"] {
        line(&mut s, &mut ui, &cv, "*CLS");
        let reply = line(&mut s, &mut ui, &cv, &format!(":MMEM:DATA? \"{name}\""));
        assert!(reply.starts_with("-224,"), "{name}: {reply}");
    }
    assert!(line(&mut s, &mut ui, &cv, ":MMEM:DATA? \"none.csv\"").starts_with("-256,"));
    line(&mut s, &mut ui, &cv, "*CLS;:MMEM:DEL \"a.csv\"");
    assert!(!dir.join("a.csv").exists() && dir.join("m.bin").exists());
    line(&mut s, &mut ui, &cv, ":MMEM:DEL \"a.csv\"");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-256,"));
    line(&mut s, &mut ui, &cv, ":LOAD:SET \"none.txt\"");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-256,"), "a setup file that is not there");
    // a save that cannot be written is an error of the command, not only a note on the screen
    line(&mut s, &mut ui, &cv, "*CLS;:SAVE:MEM:WAV \"m.bin/under-a-file.bin\"");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-250,"), "a path under a file");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn cursors_and_a_readout_round_trip() {
    let (mut s, mut ui, cv) = reference_session();
    line(&mut s, &mut ui, &cv, ":TIM:MAIN:SCAL 1e-6;:TIM:MAIN:OFFS 0;:REF:VSC 1,1;:REF:VOFF 1,0");
    line(
        &mut s,
        &mut ui,
        &cv,
        ":CURS:MODE MAN;:CURS:MAN:TYPE TIME;:CURS:MAN:SOUR R1;:CURS:MAN:CAX 0;:CURS:MAN:CBX 1e-6",
    );
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MODE?;:CURS:MAN:TYPE?;:CURS:MAN:SOUR?"), "MAN;TIME;R1");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MAN:CAX?"), "0.000000E+00");
    let dx: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:XDEL?").parse().unwrap();
    let inv: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:IXD?").parse().unwrap();
    assert!((dx - 1e-6).abs() < 1e-9, "{dx}");
    assert!((inv - 1e6).abs() < 1.0, "{inv}");

    line(&mut s, &mut ui, &cv, ":CURS:MAN:TYPE AMPL;:CURS:MAN:CAY 0.5;:CURS:MAN:CBY -0.5");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MODE?;:CURS:MAN:TYPE?"), "MAN;AMPL");
    let ay: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:AYV?").parse().unwrap();
    let dy: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:YDEL?").parse().unwrap();
    assert!((ay - 0.5).abs() < 1e-4, "{ay}");
    assert!((dy + 1.0).abs() < 1e-3, "{dy}");

    line(&mut s, &mut ui, &cv, ":CURS:MODE XY;:CURS:XY:AX 1e-6;:CURS:XY:AY 0.25");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MODE?"), "XY");
    let ax: f64 = line(&mut s, &mut ui, &cv, ":CURS:XY:AX?").parse().unwrap();
    let y: f64 = line(&mut s, &mut ui, &cv, ":CURS:XY:AY?").parse().unwrap();
    assert!((ax - 1e-6).abs() < 1e-9 && (y - 0.25).abs() < 1e-4, "{ax} {y}");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MODE OFF;:CURS:MAN:XDEL?"), "9.9E37");

    line(&mut s, &mut ui, &cv, ":MEAS:CLE;:MEAS:SOUR R1;:MEAS:ITEM VPP;:MEAS:ITEM RRDELAY,CHAN1,CHAN2");
    let strip = line(&mut s, &mut ui, &cv, ":MEAS:STR?");
    assert!(strip.contains("VPP,R1,"), "{strip}");
    assert!(strip.contains("RRDELAY,CHAN1,CHAN2,"), "{strip}");
    let vpp: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? VPP").parse().unwrap();
    assert!((vpp - 1.0).abs() < 1e-3, "the saved square is 1 Vpp, got {vpp}");
    line(&mut s, &mut ui, &cv, ":MEAS:CLE");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STR?"), "");

    line(
        &mut s,
        &mut ui,
        &cv,
        ":DVM:ENAB ON;:DVM:SOUR CHAN2;:DVM:MODE ACRMS;:COUN:ENAB ON;:COUN:MODE PER;:COUN:NDIG 6",
    );
    assert_eq!(
        line(&mut s, &mut ui, &cv, ":DVM:ENAB?;:DVM:SOUR?;:DVM:MODE?;:COUN:ENAB?;:COUN:MODE?;:COUN:NDIG?"),
        "1;CHAN2;ACRMS;1;PER;6"
    );
    assert!(!line(&mut s, &mut ui, &cv, ":DVM:CURR?;:COUN:CURR?").is_empty());
    line(&mut s, &mut ui, &cv, ":COUN:TOT:CLE");
}

#[test]
fn cursor_setters_preserve_genuine_statistics_and_pending_single() {
    let (mut scpi, mut ui, canvas) = session();
    for _ in 0..4 {
        ui.step(0.05);
    }
    let statistics = ui.measurement_statistics_snapshot_for_test();
    assert!(statistics.iter().any(|&(count, _)| count >= 3));
    let record = ui.record_volts(0).to_vec();
    ui.m.trig_level = 100.0;
    ui.key(crate::ui::key::SINGLE);
    let stamp = ui.m.hw_stamp;
    let source = ui.m.trigger_wave();
    let mut waiting = Program::new("*OPC?;:RUN");
    assert!(scpi.poll_program(&mut ui, &canvas, &mut waiting).is_none());
    for command in [
        ":CURS:MODE MAN",
        ":CURS:MAN:TYPE TIME",
        ":CURS:MAN:SOUR CHAN1",
        ":CURS:MAN:CAX 0",
        ":CURS:MAN:CBX 1e-8",
        ":CURS:MAN:TYPE AMPL",
        ":CURS:MAN:CAY 0.2",
        ":CURS:MAN:CBY -0.2",
        ":CURS:MODE XY",
        ":CURS:XY:AX 0",
        ":CURS:XY:AY 0.1",
        ":CURS:XY:BX 1e-8",
        ":CURS:XY:BY -0.1",
        ":CURS:MODE TRAC",
        ":CURS:TRAC:SOUR2 CHAN1",
        ":CURS:MODE OFF",
    ] {
        line(&mut scpi, &mut ui, &canvas, command);
        assert_eq!(ui.measurement_statistics_snapshot_for_test(), statistics, "{command}");
        assert_eq!(ui.record_volts(0), record, "{command}");
        assert_eq!(ui.m.hw_stamp, stamp, "{command}");
        assert_eq!(ui.m.trigger_wave(), source);
        assert_eq!(ui.m.trig_mode, TrigMode::Single);
        assert!(scpi.poll_program(&mut ui, &canvas, &mut waiting).is_none(), "{command}");
    }
    assert_eq!(line(&mut scpi, &mut ui, &canvas, ":SYST:ERR?"), "0,\"No error\"");
}

#[test]
fn manual_cursors_keep_time_and_voltage_together() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":TIM:MAIN:SCAL 1e-6;:TIM:MAIN:OFFS 0;:CHAN1:SCAL 1;:CHAN1:OFFS 0");
    line(
        &mut s,
        &mut ui,
        &cv,
        ":CURS:MODE MAN;:CURS:MAN:CAX 0;:CURS:MAN:CBX 1e-6;:CURS:MAN:CAY 0.5;:CURS:MAN:CBY -0.5",
    );
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MODE?"), "XY", "both axes are the screen's XY cursors");
    let ax: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:CAX?").parse().unwrap();
    let bx: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:CBX?").parse().unwrap();
    let ay: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:CAY?").parse().unwrap();
    let by: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:CBY?").parse().unwrap();
    assert!((ax).abs() < 1e-9 && (bx - 1e-6).abs() < 1e-9, "times {ax} {bx}");
    assert!((ay - 0.5).abs() < 1e-4 && (by + 0.5).abs() < 1e-4, "volts {ay} {by}");
    line(&mut s, &mut ui, &cv, ":CURS:MODE OFF;:CURS:MAN:TYPE AMPL;:CURS:MAN:CAY 0.25;:CURS:MAN:CAX 0");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:MODE?"), "XY");
    let kept: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:CAY?").parse().unwrap();
    let moved: f64 = line(&mut s, &mut ui, &cv, ":CURS:MAN:CAX?").parse().unwrap();
    assert!((kept - 0.25).abs() < 1e-4 && moved.abs() < 1e-9, "voltage survived the time cursor: {kept} {moved}");
}

#[test]
fn track_cursor_b_follows_its_own_channel() {
    let (mut s, mut ui, cv) = session();
    line(
        &mut s,
        &mut ui,
        &cv,
        "*CLS;:CURS:MODE TRAC;:CURS:TRAC:SOUR1 CHAN1;:CURS:TRAC:SOUR2 CHAN2;:CURS:TRAC:CAX 0;:CURS:TRAC:CBX 1e-6",
    );
    for _ in 0..4 {
        ui.step(0.05);
    }
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:TRAC:SOUR1?;:CURS:TRAC:SOUR2?"), "CHAN1;CHAN2");
    let ay = line(&mut s, &mut ui, &cv, ":CURS:TRAC:AYV?");
    let by = line(&mut s, &mut ui, &cv, ":CURS:TRAC:BYV?");
    assert!(ay.parse::<f64>().unwrap().is_finite() && by.parse::<f64>().unwrap().is_finite(), "{ay} {by}");
    line(&mut s, &mut ui, &cv, "*CLS;:CURS:MODE MAN;:CURS:MAN:SOUR2 CHAN2");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224"), "manual mode keeps one source");
    line(&mut s, &mut ui, &cv, "*CLS;:CURS:MODE TRAC;:CURS:TRAC:MODE X");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:TRAC:MODE?;:SYST:ERR?"), "X;0,\"No error\"");
    line(&mut s, &mut ui, &cv, ":CURS:TRAC:MODE Y");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224"), "track cursors follow time");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CURS:TRAC:MODE?"), "X");
}

#[test]
fn statistics_channel_labels_and_the_zoom_window_round_trip() {
    let (mut s, mut ui, cv) = reference_session();
    line(&mut s, &mut ui, &cv, ":CHAN1:LAB:CONT Vbus;:CHAN1:UNIT AMP;:CHAN1:VERN ON;:CHAN1:POS 0.2");
    assert_eq!(
        line(&mut s, &mut ui, &cv, ":CHAN1:LAB:CONT?;:CHAN1:LAB:SHOW?;:CHAN1:UNIT?;:CHAN1:VERN?"),
        "Vbus;1;AMP;1"
    );
    let pos: f64 = line(&mut s, &mut ui, &cv, ":CHAN1:OFFS?").parse().unwrap();
    assert!((pos - 0.2).abs() < 1e-4, "{pos}");
    line(&mut s, &mut ui, &cv, ":CHAN1:LAB:SHOW OFF");
    assert_eq!(line(&mut s, &mut ui, &cv, ":CHAN1:LAB:SHOW?;:CHAN1:LAB:CONT?"), "0;");

    line(&mut s, &mut ui, &cv, ":TIM:MAIN:SCAL 1e-3;:TIM:HREF:MODE TRIG;:TIM:VERN ON;:TIM:DEL:ENAB ON;:TIM:DEL:SCAL 1e-4;:TIM:DEL:OFFS 0;:TIM:XY:ENAB ON");
    assert_eq!(line(&mut s, &mut ui, &cv, ":TIM:HREF:MODE?;:TIM:VERN?;:TIM:DEL:ENAB?;:TIM:XY:ENAB?"), "TRIG;1;1;1");
    let z: f64 = line(&mut s, &mut ui, &cv, ":TIM:DEL:SCAL?").parse().unwrap();
    assert!((z - 1e-4).abs() < 1e-8, "{z}");

    line(&mut s, &mut ui, &cv, ":MEAS:CLE;:MEAS:STAT:DISP ON;:MEAS:STAT:ITEM VPP,R1;:RUN");
    ui.step(0.25);
    ui.step(0.25);
    let current: f64 = line(&mut s, &mut ui, &cv, ":MEAS:STAT:ITEM? CURR,VPP,R1").parse().unwrap();
    let mean: f64 = line(&mut s, &mut ui, &cv, ":MEAS:STAT:ITEM? AVER,VPP,R1").parse().unwrap();
    let n: f64 = line(&mut s, &mut ui, &cv, ":MEAS:STAT:ITEM? CNT,VPP,R1").parse().unwrap();
    assert!((current - 1.0).abs() < 1e-3, "{current}");
    assert!((mean - 1.0).abs() < 1e-3 && n >= 2.0, "mean {mean} count {n}");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STAT:DISP?;:MEAS:STR?"), "1;VPP,R1,1.000000E+00");
    line(&mut s, &mut ui, &cv, ":MEAS:STAT:RES");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STAT:ITEM? CNT,VPP,R1"), "0.000000E+00");
}

#[test]
fn rigol_measurement_names_match_the_screen_items() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":TIM:SCAL 1e-3");
    for _ in 0..8 {
        ui.step(0.05);
    }
    for (rigol, screen) in [("MAREA", "AREA"), ("MPAREA", "CAREA"), ("PVRMS", "CRMS"), ("PSLEWRATE", "PSLEW"), ("NSLEWRATE", "NSLEW")] {
        let a: f64 = line(&mut s, &mut ui, &cv, &format!(":MEAS:ITEM? {rigol},CHAN1")).parse().unwrap();
        let b: f64 = line(&mut s, &mut ui, &cv, &format!(":MEAS:ITEM? {screen},CHAN1")).parse().unwrap();
        assert!((a - b).abs() < 1e-9 || (a > 1e37 && b > 1e37), "{rigol} {a} {screen} {b}");
        assert!(a.is_finite() && a < 1e37, "{rigol} {a}");
    }
}

#[test]
fn zero_a_channel_and_place_the_readout() {
    let (mut s, mut ui, cv) = session();
    for _ in 0..8 {
        ui.step(0.05);
    }
    let before: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? VAVG,CHAN1").parse().unwrap();
    assert!(before.is_finite() && before < 1e37, "{before}");
    line(&mut s, &mut ui, &cv, ":CHAN1:ZERO");
    let bias: f64 = line(&mut s, &mut ui, &cv, ":CHAN1:BIAS?").parse().unwrap();
    assert!((bias - before).abs() < 0.05, "bias {bias} mean {before}");
    for _ in 0..4 {
        ui.step(0.05);
    }
    let after: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? VAVG,CHAN1").parse().unwrap();
    assert!(after.abs() < 0.05, "the next record is corrected, {after}");

    line(&mut s, &mut ui, &cv, "*CLS;:MEAS:STR:DOCK LEFT");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STR:DOCK?"), "LEFT");
    line(&mut s, &mut ui, &cv, ":MEAS:STR:DOCK ABOVE");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STR:DOCK?"), "ABOVE");
    line(&mut s, &mut ui, &cv, ":MEAS:STR:DOCK MIDDLE");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STR:DOCK?"), "ABOVE");

    line(&mut s, &mut ui, &cv, ":MEAS:COUN:ENAB ON;:MEAS:COUN:SOUR CHAN2");
    assert_eq!(line(&mut s, &mut ui, &cv, ":COUN:ENAB?;:COUN:SOUR?"), "1;CHAN2");
    let value: f64 = line(&mut s, &mut ui, &cv, ":MEAS:COUN:VAL?").parse().unwrap();
    assert!(value.is_finite(), "{value}");
}

#[test]
fn measurement_gate_changes_burst_outside_and_duration() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":TIM:SCAL 1e-3");
    for _ in 0..8 {
        ui.step(0.05);
    }
    line(&mut s, &mut ui, &cv, "*CLS;:MEAS:BURS:IDLE 0;:MEAS:OUTS BOTH;:MEAS:NPER 1;:MEAS:EDG CLOC");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:OUTS?;:MEAS:NPER?;:MEAS:EDG?"), "BOTH;1;CLOC");
    let idle: f64 = line(&mut s, &mut ui, &cv, ":MEAS:BURS:IDLE?").parse().unwrap();
    assert!(idle.abs() < 1e-15, "{idle}");
    let one: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? NPER,CHAN1").parse().unwrap();
    line(&mut s, &mut ui, &cv, ":MEAS:NPER 2");
    let two: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? NPER,CHAN1").parse().unwrap();
    assert!(one.is_finite() && one < 1e37 && (two / one - 2.0).abs() < 0.15, "one {one} two {two}");
    line(&mut s, &mut ui, &cv, ":MEAS:NPER 1;:MEAS:EDG DATA");
    let data: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? NPER,CHAN1").parse().unwrap();
    assert!((data / one - 0.5).abs() < 0.15, "clock {one} data {data}");
    let burst: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? BWID,CHAN1").parse().unwrap();
    assert!(burst.is_finite() && burst < 1e37, "{burst}");
    line(&mut s, &mut ui, &cv, ":MEAS:BURS:IDLE 1e-6");
    let split: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? BWID,CHAN1").parse().unwrap();
    assert!(split.is_finite() && split <= burst, "a shorter idle never makes a burst longer, {split} {burst}");
    let mut csv = String::from("Time (s),Volts\n");
    for index in 0..2000 {
        let value = if index % 100 < 10 { 1.0 } else { 0.0 };
        csv.push_str(&format!("{},{}\n", index as f64 * 1e-6, value));
    }
    ui.load_waveform_bytes(csv.as_bytes(), "pulse.csv").unwrap();
    line(&mut s, &mut ui, &cv, ":MEAS:OUTS ABOV");
    let above: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? TOUT,R1").parse().unwrap();
    line(&mut s, &mut ui, &cv, ":MEAS:OUTS BEL");
    let below: f64 = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? TOUT,R1").parse().unwrap();
    assert!(above > 0.0 && below > above * 5.0, "a 10 % pulse is high briefly, above {above} below {below}");
    line(&mut s, &mut ui, &cv, ":MEAS:NPER 0;:MEAS:BURS:IDLE -1;:MEAS:EDG BOTH");
    let err = line(&mut s, &mut ui, &cv, ":SYST:ERR?");
    assert!(err.starts_with("-224,"), "{err}");
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:NPER?;:MEAS:EDG?"), "1;DATA");
}

// Break it catches: `:STOP` throws the accumulated statistics away (on a unit, 2026-11-20: the count of a running
// Frequency row stood at 104, one second after `:STOP` at 1)
#[test]
fn stop_keeps_the_accumulated_statistics() {
    let (mut s, mut ui, cv) = session();
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:STAT:ITEM FREQ,CHAN1;:MEAS:STAT:RES;:RUN;:SYST:ERR?"), "0,\"No error\"");
    let count = |s: &mut _, ui: &mut Ui| line(s, ui, &cv, ":MEAS:STAT:ITEM? CNT,FREQ,CHAN1").parse::<f64>().unwrap();
    let end = std::time::Instant::now() + Duration::from_secs(20);
    while count(&mut s, &mut ui) < 5.0 {
        assert!(std::time::Instant::now() < end, "no statistics while running");
        ui.step(0.05);
    }
    let before = count(&mut s, &mut ui);
    line(&mut s, &mut ui, &cv, ":STOP");
    for _ in 0..20 {
        ui.step(0.05);
    }
    let after = count(&mut s, &mut ui);
    assert!(after >= before, "the count went from {before} to {after} on :STOP");
}

/// A pulse train of `f` Hz, 30 % duty, 0 to 1 V, 20 000 points at 100 MSa/s, as a CSV for a reference slot.
fn pulse_csv(f: f64) -> String {
    let mut csv = String::from("Time (s),Volts\n");
    for i in 0..20_000 {
        let t = i as f64 * 1e-8;
        csv.push_str(&format!("{:.12e},{}\n", t, if (t * f).fract() < 0.3 { 1.0 } else { 0.0 }));
    }
    csv
}

// Break it catches: `:SPECtrum:RBW:ACTual?` and `:SPECtrum:PEAK:LIST?` answer the page's old result while the page is
// not shown (on a unit, 2026-11-20: the same answer for ten new records), and the list stops at 8 lines whatever the
// Harmonics setting (240, 320 and 400 MHz of a 40 MHz clock were left out)
#[test]
fn spectrum_queries_answer_the_record_now_with_every_harmonic_asked_for() {
    let (mut s, mut ui, cv) = session();
    ui.load_waveform_auto(pulse_csv(1e6).as_bytes(), "a.csv").unwrap();
    ui.load_waveform_auto(pulse_csv(2e6).as_bytes(), "b.csv").unwrap();
    ui.set_offline(true);
    let lines = |s: &mut _, ui: &mut Ui, src: &str| -> Vec<(f64, f64)> {
        line(s, ui, &cv, &format!(":VIEW SCOPE;:SPEC:SOUR {src};:SPEC:SPAN 0;:SPEC:PEAK:HARM 20"));
        let a = line(s, ui, &cv, ":SPEC:PEAK:LIST?");
        a.split(';').map(|p| {
            let mut x = p.split(',').map(|v| v.parse::<f64>().unwrap());
            (x.next().unwrap(), x.next().unwrap())
        }).collect()
    };
    assert!(!ui.spectrum_runs(), "the page is not shown");
    let one = lines(&mut s, &mut ui, "R1");
    let two = lines(&mut s, &mut ui, "R2");
    let fundamental = |l: &[(f64, f64)]| l.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1)).unwrap().0;
    assert!((fundamental(&one) - 1e6).abs() < 2e4, "{one:?}");
    assert!((fundamental(&two) - 2e6).abs() < 4e4, "the answer follows the source: {two:?}");
    assert!(one.len() > 8, "more than 8 lines when 20 harmonics are asked for: {one:?}");
    assert!(one.windows(2).all(|w| w[0].0 < w[1].0), "in the order of the table (by frequency): {one:?}");
    let rbw = line(&mut s, &mut ui, &cv, ":SPEC:RBW:ACT?").parse::<f64>().unwrap();
    assert!(rbw > 0.0 && rbw < 1e5, "{rbw}");
}

// Break it catches: a script cannot tell a measurement still computed from one that does not exist (both 9.9E37; on
// a unit, 2026-11-20, one to four reads after a Single): `:MEASure:PENDing?` says which, and at 0 the value is there
#[test]
fn pending_says_when_the_measurements_of_a_single_are_done() {
    let (mut s, mut ui, cv) = session();
    assert_eq!(line(&mut s, &mut ui, &cv, ":MEAS:ITEM VPP,CHAN1;:SING;:SYST:ERR?"), "0,\"No error\"");
    let end = std::time::Instant::now() + Duration::from_secs(20);
    loop {
        ui.step(0.05);
        let stopped = line(&mut s, &mut ui, &cv, ":TRIG:STAT?") == "STOP";
        if stopped && line(&mut s, &mut ui, &cv, ":MEAS:PEND?") == "0" {
            break;
        }
        assert!(std::time::Instant::now() < end, "still pending");
    }
    let v = line(&mut s, &mut ui, &cv, ":MEAS:ITEM? VPP,CHAN1").parse::<f64>().unwrap();
    assert!(v.abs() < 1e30, "at 0 the value is there: {v}");
}
