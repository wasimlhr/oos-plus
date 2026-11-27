// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The Polarity row of the decoders that have one: an inverted line decodes as it should with Inverted or Auto, and
//! as parity errors with Normal. Signals made by the generators of `oos_dsp::sim` and `oos_dsp::decode::gen`.

use oos_dsp::decode::{gen, Kind, Packet};
use oos_dsp::protocols::{Lines, Polarity, Shared, MODULES, OPTS, POLARITY_NAMES};
use oos_dsp::sim::{mil1553_message, sample};

fn shared() -> Shared {
    use oos_dsp::decode::{Parity, UsbSpeed};
    Shared {
        baud: 9600.0,
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

/// The module of `key` on one line with its polarity row set to `pol`: the packets and whether it read inverted.
fn run(key: &str, v: &[f32], dt: f64, thr: f32, hy: f32, pol: Polarity) -> (Vec<Packet>, bool) {
    let m = MODULES.iter().find(|m| m.key == key).unwrap();
    let (slot, _) = m.polarity.unwrap_or_else(|| panic!("{key}: no polarity row"));
    let mut opt = [0usize; OPTS];
    opt[slot] = pol as usize;
    let l =
        Lines { lines: [Some(v), None, None, None], dt, thr, hy, opt, sh: shared(), chans: [None; 4], src: [None; 4] };
    m.decode_used(&l).unwrap()
}

fn bad(p: &[Packet]) -> usize {
    p.iter().filter(|x| !x.ok).count()
}

fn words(p: &[Packet]) -> Vec<u32> {
    p.iter().filter(|x| matches!(x.kind, Kind::Word { .. })).map(|x| x.value).collect()
}

fn mil_line(invert: bool) -> (Vec<f32>, f64) {
    let w =
        [(true, 0x0c23, 7.0), (true, 0x0800, 0.0), (false, 0xbeef, 0.0), (false, 0x1234, 0.0), (false, 0x00ff, 4.0)];
    let l = mil1553_message(&w, 7.0);
    let dt = 1e-6 / 50.0;
    let mut v = sample(|t| l.v(t), 0.0, dt, 50 * 140);
    if invert {
        v.iter_mut().for_each(|x| *x = -*x);
    }
    (v, dt)
}

// Break it catches: Auto that keeps Normal on an inverted bus (the parity error on every word stays), Inverted that
// leaves the line alone, a tie that does not keep Normal, a clean bus that Auto reports as inverted
#[test]
fn an_inverted_1553_bus_is_read_with_inverted_or_auto_and_fails_parity_as_it_is() {
    let (good, dt) = mil_line(false);
    let (flipped, _) = mil_line(true);
    let (clean, used) = run("Mil1553", &good, dt, 0.0, 1.0, Polarity::Normal);
    assert!(!clean.is_empty() && bad(&clean) == 0 && !used);
    // the same bus, wires swapped: Normal reads parity errors
    let (normal, used) = run("Mil1553", &flipped, dt, 0.0, 1.0, Polarity::Normal);
    assert!(!used && bad(&normal) > 0, "{} words, {} failed", normal.len(), bad(&normal));
    // Inverted and Auto read what the good bus gives, and Auto says it read inverted
    for pol in [Polarity::Inverted, Polarity::Auto] {
        let (p, used) = run("Mil1553", &flipped, dt, 0.0, 1.0, pol);
        assert!(used, "{pol:?}");
        assert_eq!(words(&p), words(&clean), "{pol:?}");
        assert_eq!(bad(&p), 0, "{pol:?}");
    }
    // a bus that is right: Auto keeps Normal (a tie as well)
    let (p, used) = run("Mil1553", &good, dt, 0.0, 1.0, Polarity::Auto);
    assert!(!used && words(&p) == words(&clean));
}

#[test]
fn an_inverted_arinc_line_is_read_with_inverted_or_auto() {
    let w: Vec<Vec<bool>> = [(0o310u8, 1u8, 0x5_a5a5u32, 3u8), (0o203, 0, 1, 0), (0o377, 2, 0x7_ffff, 1)]
        .iter()
        .map(|&(l, s, d, m)| gen::arinc429_word(l, s, d, m, false))
        .collect();
    let l = gen::arinc429(&w, 100e3, 10.0, 4);
    let dt = 1e-5 / 40.0;
    let good = oos_dsp::sim::sample(|t| l.v(t), 0.0, dt, (l.seg.last().unwrap().0 / dt) as usize);
    let flipped: Vec<f32> = good.iter().map(|x| -x).collect();
    let (clean, used) = run("Arinc429", &good, dt, 5.0, 0.5, Polarity::Normal);
    assert!(!used && bad(&clean) == 0 && !clean.is_empty());
    // a swapped pair complements all 32 bits, and the complement of an odd-parity word has odd parity as well: no
    // parity error tells the swap, so Normal reads other labels and Auto, which looks at failed fields, cannot
    // choose; the ARINC row offers Normal and Inverted only
    let ids = |p: &[Packet]| -> Vec<u32> {
        p.iter().filter(|x| matches!(x.kind, Kind::Id { .. })).map(|x| x.value).collect()
    };
    let (normal, _) = run("Arinc429", &flipped, dt, 5.0, 0.5, Polarity::Normal);
    assert_eq!(bad(&normal), 0);
    assert_ne!(ids(&normal), [0o310, 0o203, 0o377]);
    let (p, used) = run("Arinc429", &flipped, dt, 5.0, 0.5, Polarity::Inverted);
    assert!(used);
    assert_eq!(bad(&p), 0);
    assert_eq!(ids(&p), [0o310, 0o203, 0o377]);
}

#[test]
fn polarity_rows_have_one_set_of_names_and_are_where_the_info_says() {
    assert_eq!(POLARITY_NAMES, ["Normal", "Inverted", "Auto"]);
    assert_eq!(Polarity::from_index(2), Polarity::Auto);
    let with: Vec<&str> = MODULES.iter().filter(|m| m.polarity.is_some()).map(|m| m.key).collect();
    assert_eq!(with, ["Dmx512", "Uart9", "Arinc429", "Mil1553"]);
}
