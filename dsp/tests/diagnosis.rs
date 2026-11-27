// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The diagnosis of a decoder that decoded nothing, or mostly errors (`oos_dsp::diagnosis`): the sentences name the
//! cause and the setting to change, on signals made for exactly that fault, and a decode that is fine says nothing.

use oos_dsp::decode::{gen, Parity, UsbSpeed};
use oos_dsp::diagnosis::{diagnose, Diagnosis};
use oos_dsp::protocols::{Lines, Shared, MODULES, OPTS};
use oos_dsp::sim::{mil1553_message, sample};

fn shared(baud: f64) -> Shared {
    Shared {
        baud,
        bits: 8,
        parity: Parity::None,
        spi_mode: 0,
        can_rate: 500e3,
        canh: false,
        usb_speed: UsbSpeed::Auto,
        cphy_rate: 0.0,
        uart_inv: false,
        spi_timeout: 0.0,
        spi_cs_high: false,
    }
}

/// The module of `key` decoding `v` with its option rows `opt`, and what the diagnosis says of it.
fn run(key: &str, v: &[f32], dt: f64, thr: f32, hy: f32, opt: [usize; OPTS], baud: f64) -> Diagnosis {
    let m = MODULES.iter().find(|m| m.key == key).unwrap();
    let l = Lines {
        lines: [Some(v), None, None, None],
        dt,
        thr,
        hy,
        opt,
        sh: shared(baud),
        chans: [None; 4],
        src: [None; 4],
    };
    let p = m.decode(&l).unwrap();
    diagnose(m, &l, &p)
}

fn uart(baud: f64, per_bit: f64, lo: f64, hi: f64) -> (Vec<f32>, f64) {
    let mut w = gen::Wave::new(lo, hi, 0.02 / baud);
    w.level(true, 20.0 / baud);
    for c in [0x48u32, 0x69, 0x21, 0x0a] {
        w.uart(c, 8, Parity::None, 1, baud);
    }
    w.level(true, 10.0 / baud);
    let dt = 1.0 / baud / per_bit;
    (w.done().sample(dt, 20.0 / baud), dt)
}

fn mil(invert: bool) -> (Vec<f32>, f64) {
    let w = [(true, 0x0c23, 7.0), (true, 0x0800, 0.0), (false, 0xbeef, 0.0), (false, 0x1234, 0.0), (false, 0x00ff, 4.0)];
    let l = mil1553_message(&w, 7.0);
    let dt = 1e-6 / 50.0;
    let mut v = sample(|t| l.v(t), 0.0, dt, 50 * 140);
    if invert {
        v.iter_mut().for_each(|x| *x = -*x);
    }
    (v, dt)
}

// Break it catches: a diagnosis on a decode that is fine (it is empty then), a bit rate measured from the edges that
// is not the line's, error figures that disagree with the packets
#[test]
fn a_clean_decode_has_no_diagnosis_and_reports_what_it_measured() {
    let (v, dt) = uart(9600.0, 20.0, 0.0, 3.3);
    let d = run("Uart", &v, dt, 1.65, 0.3, [0; OPTS], 9600.0);
    assert!(d.lines.is_empty(), "{:?}", d.lines);
    assert_eq!((d.packets > 0, d.failed), (true, 0));
    let rate = d.bit_rate.unwrap();
    assert!((rate / 9600.0 - 1.0).abs() < 0.05, "{rate}");
    assert!((d.vmin - 0.0).abs() < 0.01 && (d.vmax - 3.3).abs() < 0.01, "{} {}", d.vmin, d.vmax);
}

// Break it catches: the polarity sentence missing for a bus every word of which fails its parity, naming the wrong
// row, or still saying it when Polarity is already Auto and the words decode
#[test]
fn an_inverted_1553_bus_says_to_use_polarity_and_auto_clears_it() {
    let (v, dt) = mil(true);
    let d = run("Mil1553", &v, dt, 0.0, 1.0, [0; OPTS], 9600.0);
    assert!(d.failed > 0 && d.error_fraction() > 0.5, "{} of {}", d.failed, d.packets);
    let text = d.lines.join("\n");
    assert!(text.contains("words fail their check") && text.contains("inverted"), "{text}");
    assert!(text.contains("Polarity to Auto or Inverted"), "{text}");
    let d = run("Mil1553", &v, dt, 0.0, 1.0, [2, 0, 0, 0], 9600.0);
    assert!(d.lines.is_empty() && d.failed == 0, "{:?}", d.lines);
    // the bus that is right never gets the sentence
    let (good, dt) = mil(false);
    assert!(run("Mil1553", &good, dt, 0.0, 1.0, [0; OPTS], 9600.0).lines.is_empty());
}

// Break it catches: a sample rate that is too low and decodes nothing, with the diagnosis naming neither the rate nor
// what to use
#[test]
fn a_sample_rate_too_low_for_the_bit_rate_names_both() {
    // 9600 baud sampled twice a bit: the UART decoder needs more and returns nothing
    let (v, dt) = uart(9600.0, 2.0, 0.0, 3.3);
    let d = run("Uart", &v, dt, 1.65, 0.3, [0; OPTS], 9600.0);
    assert_eq!(d.packets, 0);
    let text = d.lines.join("\n");
    assert!(text.contains("sample rate 19.2 kSa/s is too low for 9.60 kb/s"), "{text}");
    assert!(text.contains("at least 96.0 kSa/s"), "{text}");
}

#[test]
fn a_threshold_outside_the_signal_and_a_flat_line_are_named() {
    let (v, dt) = uart(9600.0, 20.0, 0.0, 3.3);
    let d = run("Uart", &v, dt, 5.0, 0.3, [0; OPTS], 9600.0);
    let text = d.lines.join("\n");
    assert!(text.contains("threshold 5.00 V is outside the signal (0 V to 3.30 V)"), "{text}");
    assert!(text.contains("about 1.65 V"), "{text}");
    let flat = vec![0.0f32; v.len()];
    let d = run("Uart", &flat, dt, 1.65, 0.3, [0; OPTS], 9600.0);
    let text = d.lines.join("\n");
    assert!(text.contains("flat") && text.contains("check the probe"), "{text}");
}

#[test]
fn a_record_shorter_than_a_frame_and_a_wrong_bit_rate_are_named() {
    let (v, dt) = uart(9600.0, 20.0, 0.0, 3.3);
    // ten bit times of it
    let d = run("Uart", &v[..200], dt, 1.65, 0.3, [0; OPTS], 9600.0);
    assert!(d.lines.join("\n").contains("record is 1.04 ms long, under 20 bit times"), "{:?}", d.lines);
    // set to 115200 baud, the line runs at 9600: nothing decodes, and the edges say what the rate is
    let d = run("Uart", &v, dt, 1.65, 0.3, [0; OPTS], 115200.0);
    let text = d.lines.join("\n");
    assert!(text.contains("the decoder is set to") && text.contains("change its bit rate"), "{text}");
}
