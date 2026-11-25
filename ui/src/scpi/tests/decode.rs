// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Tests of the decoder commands, `:BUS<n>` and `:DECode` (scpi/decode.rs). A test of a new decoder command goes
//! here; its query also goes into ui/tests/scpi.txt, its line into docs/scpi.md (AGENTS.md).

use super::*;

fn decoded_reference_fixture(ui: &mut Ui, points: usize) {
    ui.set_offline(true);
    let mut values = vec![3.3; points];
    let bits = std::iter::once(false).chain((0..8).map(|bit| 0xa5u8 >> bit & 1 != 0)).chain(std::iter::once(true));
    for (bit, high) in bits.enumerate() {
        values[100 + bit * 20..100 + (bit + 1) * 20].fill(if high { 3.3 } else { 0.0 });
    }
    ui.commit_waveform_traces(
        vec![crate::store::WaveTrace {
            v: values,
            dt: 1.0 / (115_200.0 * 20.0),
            t0: -0.008,
            label: "UART reference".into(),
        }],
        "reference",
        true,
    )
    .unwrap();
}

#[test]
fn decoder_scpi_configures_refs_reports_original_packet_times_and_rejects_empty_sources() {
    let (mut s, mut ui, cv) = session();
    decoded_reference_fixture(&mut ui, 1000);
    assert_eq!(ui.m.n_on(), 0);
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:PROTO UART;:DEC:SOUR R1;:DEC:THR 1.65;:DEC:STAT ON;:SYST:SET dec_baud = 7;:DEC:SOUR?;:DEC:PROTO?;:DEC:STAT?"), "R1;UART;1");
    let events = line(&mut s, &mut ui, &cv, ":DEC:EVEN?");
    assert!(events.contains("D1 UART R1") && events.contains("A5"), "{events}");
    let first_time: f64 = events.split(',').next().unwrap().parse().unwrap();
    assert!(first_time > -0.008 && first_time < -0.007);
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:BUSY?"), "0");
    let state = crate::store::setup_text(&ui.m);
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:SOUR R10;:DEC:SOUR?"), "R1");
    assert_eq!(crate::store::setup_text(&ui.m), state);
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
    assert_eq!(
        line(&mut s, &mut ui, &cv, ":DEC:CS NONE;:DEC:SOUR4 NONE;:DEC:DIFF NONE;:DEC:CS?;:DEC:SOUR4?;:DEC:DIFF?"),
        "NONE;NONE;NONE"
    );
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:SEL 2;:DEC:SEL?;:DEC:SEL 1;:DEC:SOUR?"), "2;R1");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

#[test]
fn decoder_scpi_large_reference_reports_busy_then_fresh_events_without_acquisition() {
    let (mut s, mut ui, cv) = session();
    decoded_reference_fixture(&mut ui, 300_000);
    line(&mut s, &mut ui, &cv, ":DEC:PROTO UART;:DEC:SOUR R1;:DEC:THR 1.65;:DEC:STAT ON;:SYST:SET dec_baud = 7");
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:EVEN?"), "BUSY");
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while line(&mut s, &mut ui, &cv, ":DEC:BUSY?") == "1" {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    }
    let events = line(&mut s, &mut ui, &cv, ":DEC:EVEN?");
    assert!(events.contains("D1 UART R1") && events.contains("A5"), "{events}");
    assert_eq!(ui.m.run, RunState::Stop);
    assert!(ui.workflow_state().contains("\"captures\":0"));
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

#[test]
fn cphy_scpi_configures_three_refs_and_exports_the_published_16_bit_word() {
    let (mut s, mut ui, cv) = session();
    let mut symbols = vec![3; 12];
    symbols.extend([3, 4, 4, 4, 4, 4, 3]);
    symbols.extend([0, 0, 1, 4, 2, 2, 0]); // 7290, its least significant symbol first
    symbols.extend([4; 7]);
    let states = oos_dsp::cphy::states_for_symbols(0, &symbols);
    let traces = (0..3)
        .map(|wire| crate::store::WaveTrace {
            v: states
                .iter()
                .flat_map(|&state| std::iter::repeat_n(oos_dsp::cphy::wire_values(state)[wire], 20))
                .collect(),
            dt: 5e-12,
            t0: -12e-9,
            label: format!("C-PHY {}", ['A', 'B', 'C'][wire]),
        })
        .collect();
    ui.commit_waveform_traces(traces, "C-PHY fixture", true).unwrap();
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:PROTO CPHY;:DEC:SOUR R1;:DEC:SOUR2 R2;:DEC:CS R3;:DEC:STAT ON;:DEC:PROTO?;:DEC:SOUR?;:DEC:SOUR2?;:DEC:CS?"), "C-PHY;R1;R2;R3");
    let events = line(&mut s, &mut ui, &cv, ":DEC:EVEN?");
    assert!(events.contains("D1 C-PHY R1") && events.contains("SYNC") && events.contains("7290"), "{events}");
    let lines: Vec<_> = events.lines().filter(|line| line.contains("7290")).collect();
    assert_eq!(lines.len(), 1);
    assert!(lines[0].split(',').next().unwrap().parse::<f64>().unwrap() < 0.0);
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:CS NONE;:DEC:CS?"), "R3");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

#[test]
fn cphy_known_rate_roundtrips_per_decoder_setup_and_rejects_invalid_values() {
    let (mut s, mut ui, cv) = session();
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:CPHY:RATE?"), "AUTO");
    assert_eq!(
        line(&mut s, &mut ui, &cv, ":DEC:PROTO CPHY;:DEC:CPHY:RATE 2.25e9;:DEC:CPHY:RATE?;:DEC:SRAT?"),
        "2.250000E+09;2.250000E+09"
    );
    let configured = crate::store::setup_text(&ui.m);
    assert_eq!(crate::store::parse_setup(&configured).unwrap().dec_cphy_rate, 2.25e9);
    let old_setup = configured
        .lines()
        .filter(|line| !line.trim_start().starts_with("dec_cphy_rate") && !line.contains(".dec_cphy_rate"))
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(crate::store::parse_setup(&old_setup).unwrap().dec_cphy_rate, 0.0, "old setups keep Auto");
    for bad in ["-1", "NaN", "inf", "1e13", "garbage"] {
        line(&mut s, &mut ui, &cv, &format!(":DEC:CPHY:RATE {bad}"));
        assert_eq!(crate::store::setup_text(&ui.m), configured);
        assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
        let mut restored = ui.m.clone();
        assert!(crate::store::set(&mut restored, "dec_cphy_rate", bad).is_err());
        assert_eq!(restored.dec_cphy_rate, 2.25e9);
    }
    assert_eq!(
        line(&mut s, &mut ui, &cv, ":DEC:SEL 2;:DEC:CPHY:RATE?;:DEC:SEL 1;:DEC:CPHY:RATE?"),
        "AUTO;2.250000E+09"
    );
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:CPHY:RATE AUTO;:DEC:CPHY:RATE?"), "AUTO");
    assert_eq!(line(&mut s, &mut ui, &cv, ":SYST:ERR?"), "0,\"No error\"");
}

#[test]
fn math_reset_autoscales_the_shown_trace() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":MATH1:RES");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-221"));
    assert!(line(&mut s, &mut ui, &cv, ":MATH1:RES?").starts_with("-113"));
    ui.step(0.2);
    line(&mut s, &mut ui, &cv, ":MATH1:OPER ABS;:MATH1:SOUR1 CHAN1;:MATH1:SCAL 50;:MATH1:OFFS 10");
    line(&mut s, &mut ui, &cv, ":MATH1:RES");
    let scale: f64 = line(&mut s, &mut ui, &cv, ":MATH1:SCAL?").parse().unwrap();
    let offset: f64 = line(&mut s, &mut ui, &cv, ":MATH1:OFFS?").parse().unwrap();
    assert!(scale < 50.0 && offset.abs() < 5.0, "{scale} {offset}");
    line(&mut s, &mut ui, &cv, ":MATH2:FFT:SOUR CHAN1;:MATH2:FFT:SCAL 1;:MATH2:OFFS 4");
    line(&mut s, &mut ui, &cv, ":MATH2:RES");
    let db: f64 = line(&mut s, &mut ui, &cv, ":MATH2:SCAL?").parse().unwrap();
    let off: f64 = line(&mut s, &mut ui, &cv, ":MATH2:OFFS?").parse().unwrap();
    let kept: f64 = line(&mut s, &mut ui, &cv, ":MATH1:SCAL?").parse().unwrap();
    assert!((db - 10.0).abs() < 1e-6 && off.abs() < 1e-6 && (kept - scale).abs() < 1e-9, "{db} {off} {kept}");
}

#[test]
fn a_bus_sets_uart_spi_and_reads_the_event_block() {
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, "*CLS;:BUS1:MODE RS232;:BUS1:DISP ON;:BUS1:FORM ASC;:BUS1:EVEN ON");
    line(&mut s, &mut ui, &cv, ":BUS1:RS232:TX CHAN1;:BUS1:RS232:RX CHAN2;:BUS1:RS232:BAUD 9600;:BUS1:RS232:PAR EVEN;:BUS1:RS232:DBIT 8;:BUS1:THR 1.25");
    assert_eq!(line(&mut s, &mut ui, &cv, ":BUS1:MODE?;:BUS1:DISP?;:BUS1:FORM?;:BUS1:EVEN?"), "RS232;1;ASC;1");
    assert_eq!(
        line(
            &mut s,
            &mut ui,
            &cv,
            ":BUS1:RS232:TX?;:BUS1:RS232:RX?;:BUS1:RS232:BAUD?;:BUS1:RS232:PAR?;:BUS1:RS232:DBIT?"
        ),
        "CHAN1;CHAN2;9600;EVEN;8"
    );
    let thr: f64 = line(&mut s, &mut ui, &cv, ":BUS1:THR?").parse().unwrap();
    assert!((thr - 1.25).abs() < 1e-6, "{thr}");
    line(&mut s, &mut ui, &cv, ":BUS2:MODE SPI;:BUS2:DISP ON;:BUS2:SPI:SCLK:SOUR CHAN2;:BUS2:SPI:MOSI:SOUR CHAN1;:BUS2:SPI:SS:SOUR CHAN3;:BUS2:SPI:DBIT 12");
    assert_eq!(
        line(
            &mut s,
            &mut ui,
            &cv,
            ":BUS2:MODE?;:BUS2:SPI:SCLK:SOUR?;:BUS2:SPI:MOSI:SOUR?;:BUS2:SPI:SS:SOUR?;:BUS2:SPI:DBIT?"
        ),
        "SPI;CHAN2;CHAN1;CHAN3;12"
    );
    assert_eq!(
        line(&mut s, &mut ui, &cv, ":BUS1:MODE?;:BUS1:RS232:BAUD?"),
        "RS232;9600",
        "the second bus must not replace the first"
    );
    line(&mut s, &mut ui, &cv, ":BUS2:SPI:SS:SOUR OFF");
    assert_eq!(line(&mut s, &mut ui, &cv, ":BUS2:SPI:SS:SOUR?"), "OFF");
    let data = line(&mut s, &mut ui, &cv, ":BUS1:DATA?");
    assert!(data.starts_with("#9") && data.contains("RS232"), "{data}");
    line(&mut s, &mut ui, &cv, "*CLS;:BUS1:FORM DEC");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224"), "only hex and ASCII are on this screen");
}

#[test]
fn frozen_bus_scpi_preserves_reference_roles_and_inactive_slot_acquisition() {
    let (mut scpi, mut ui, canvas) = reference_session();
    ui.commit_waveform_traces_at(
        vec![crate::store::WaveTrace { v: vec![0., 1., 0., 1.], dt: 1e-9, t0: 0., label: "clock".into() }],
        "R2",
        1,
    )
    .unwrap();
    line(&mut scpi, &mut ui, &canvas, ":BUS1:IIC:SDA:SOUR R1;:BUS1:IIC:SCLK:SOUR R2");
    assert_eq!(line(&mut scpi, &mut ui, &canvas, ":BUS1:IIC:SDA:SOUR?;:BUS1:IIC:SCLK:SOUR?"), "R1;R2");
    assert_eq!((ui.m.dec_src, ui.m.dec_src2), (DEC_REF_BASE, DEC_REF_BASE + 1));
    line(&mut scpi, &mut ui, &canvas, ":TRIG:EDGE:LEV 100;:SINGLE");
    let before = ui.workflow_state();
    let source = ui.m.trigger_wave();
    let mut pending = Program::new("*OPC?");
    assert!(scpi.poll_program(&mut ui, &canvas, &mut pending).is_none());
    line(&mut scpi, &mut ui, &canvas, ":BUS2:SPI:MOSI:SOUR R2;:BUS2:SPI:SCLK:SOUR R1;:BUS2:SPI:SS:SOUR OFF");
    assert_eq!(
        line(&mut scpi, &mut ui, &canvas, ":BUS2:SPI:MOSI:SOUR?;:BUS2:SPI:SCLK:SOUR?;:BUS2:SPI:SS:SOUR?"),
        "R2;R1;OFF"
    );
    assert_eq!(ui.m.dec_sel, 0);
    assert_eq!(ui.m.trigger_wave(), source);
    assert_eq!(ui.workflow_state(), before);
    assert!(
        scpi.poll_program(&mut ui, &canvas, &mut pending).is_none(),
        "inactive decoder configuration must not rearm the accepted acquisition context"
    );
    let old = ui.m.dec_slot(1);
    line(&mut scpi, &mut ui, &canvas, ":BUS2:SPI:MOSI:SOUR R10");
    assert_eq!(ui.m.dec_slot(1).src, old.src);
    assert!(line(&mut scpi, &mut ui, &canvas, ":SYST:ERR?").starts_with("-224,"));
}

#[test]
fn decoder_scpi_takes_a_math_channel_as_a_source() {
    let (mut s, mut ui, cv) = session();
    decoded_reference_fixture(&mut ui, 1000);
    // M1 is off: no source yet
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:PROTO UART;:DEC:SOUR MATH1;:DEC:SOUR?"), "CHAN3");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
    ui.m.math = crate::math::MathOp::Sub;
    ui.m.math_a = crate::math::MATH_REF_BASE;
    ui.m.math_b = crate::math::MATH_REF_BASE;
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:SOUR MATH1;:DEC:SOUR?"), "MATH1");
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:SOUR2 M1;:DEC:SOUR2?"), "MATH1");
    assert_eq!(line(&mut s, &mut ui, &cv, ":DEC:SOUR MATH9;:DEC:SOUR?"), "MATH1");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
}

/// `:REFerence:SOURce MATH1` makes a save copy the result of a math channel, so the Eye or a decoder can read a
/// differential (CH1 - CH2) from a reference slot.
#[test]
fn a_reference_save_copies_the_result_of_a_math_channel() {
    let (mut s, mut ui, cv) = session();
    decoded_reference_fixture(&mut ui, 1000);
    line(&mut s, &mut ui, &cv, ":REFerence:SLOT R3");
    // M1 is off: refused, the source stays a channel
    assert_eq!(line(&mut s, &mut ui, &cv, ":REFerence:SOURce MATH1;:REFerence:SOURce?"), "CHAN1");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with("-224,"));
    ui.m.math = crate::math::MathOp::Sub;
    ui.m.math_a = crate::math::MATH_REF_BASE;
    ui.m.math_b = crate::math::MATH_REF_BASE;
    assert_eq!(line(&mut s, &mut ui, &cv, ":REFerence:SOURce 3,MATH1;:REFerence:SOURce? 3"), "MATH1");
    for _ in 0..200 {
        if ui.math_view(0).is_some() {
            break;
        }
        ui.step(0.05);
    }
    assert!(ui.math_view(0).is_some(), "the math result is ready");
    line(&mut s, &mut ui, &cv, ":REFerence:SAVE");
    assert!(line(&mut s, &mut ui, &cv, ":SYST:ERR?").starts_with('0'));
    let (n, dt_saved, _, _, _) = ui.ref_info(2).expect("slot R3 holds the math result");
    let (v, dt, _) = ui.math_view(0).unwrap();
    assert_eq!((dt_saved, n), (dt, v.len()));
    // a channel as the source again
    assert_eq!(line(&mut s, &mut ui, &cv, ":REFerence:SOURce 3,CHAN2;:REFerence:SOURce? 3"), "CHAN2");
}
