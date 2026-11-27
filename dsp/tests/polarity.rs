// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The Polarity row of the decoders that have one: an inverted line decodes as it should with Inverted or Auto, and
//! as parity errors with Normal. Signals made by the generators of `oos_dsp::sim` and `oos_dsp::decode::gen`.

use oos_dsp::decode::gen::Wave;
use oos_dsp::decode::{gen, Kind, Packet, Parity};
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
    run_with(key, v, dt, (thr, hy), [0; OPTS], pol, shared())
}

/// The same with the other option rows `opt` and shared rows `sh` of the setup.
fn run_with(
    key: &str,
    v: &[f32],
    dt: f64,
    (thr, hy): (f32, f32),
    mut opt: [usize; OPTS],
    pol: Polarity,
    sh: Shared,
) -> (Vec<Packet>, bool) {
    let m = MODULES.iter().find(|m| m.key == key).unwrap();
    let (slot, _) = m.polarity.unwrap_or_else(|| panic!("{key}: no polarity row"));
    opt[slot] = pol as usize;
    let l = Lines { lines: [Some(v), None, None, None], dt, thr, hy, opt, sh, chans: [None; 4], src: [None; 4] };
    m.decode_used(&l).unwrap()
}

/// `v` as a bus wired the wrong way round shows it: mirrored about the threshold 1.65 V of a 3.3 V line.
fn flip(v: &[f32]) -> Vec<f32> {
    v.iter().map(|x| 3.3 - x).collect()
}

/// The rule every decoder with an Auto row keeps: Normal reads the bus as it is (failed fields on a swapped one),
/// Inverted and Auto read what the good bus gives and say so, and Auto leaves a good bus as Normal.
fn assert_swap_is_read(
    key: &str,
    good: &[f32],
    dt: f64,
    opt: [usize; OPTS],
    sh: Shared,
    what: fn(&[Packet]) -> Vec<u32>,
) {
    let swapped = flip(good);
    let go = |v: &[f32], pol| run_with(key, v, dt, (1.65, 0.4), opt, pol, sh);
    let (clean, used) = go(good, Polarity::Normal);
    assert!(!clean.is_empty() && bad(&clean) == 0 && !used, "{key}: the good bus: {} failed", bad(&clean));
    let (normal, used) = go(&swapped, Polarity::Normal);
    assert!(!used && (bad(&normal) > 0 || what(&normal) != what(&clean)), "{key}: Normal read a swapped bus right");
    for pol in [Polarity::Inverted, Polarity::Auto] {
        let (p, used) = go(&swapped, pol);
        assert!(used, "{key} {pol:?}: not read inverted");
        assert_eq!(what(&p), what(&clean), "{key} {pol:?}");
        assert_eq!(bad(&p), 0, "{key} {pol:?}");
    }
    let (p, used) = go(good, Polarity::Auto);
    assert!(!used && what(&p) == what(&clean), "{key}: Auto on a good bus");
}

fn data(p: &[Packet]) -> Vec<u32> {
    p.iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect()
}

fn uart_line(parity: Parity, bytes: &[u8], baud: f64) -> (Vec<f32>, f64) {
    let mut w = Wave::new(0.0, 3.3, 0.02 / baud);
    w.level(true, 30.0 / baud);
    for &b in bytes {
        w.uart(b as u32, 8, parity, 1, baud);
        w.level(true, 3.0 / baud);
    }
    let dt = 1.0 / baud / 16.0;
    (w.done().sample(dt, 20.0 / baud), dt)
}

// Break it catches: a UART that reads a line with the idle level low as garbage with Inverted or Auto, a swap
// that Auto does not find, and the serial trigger's own polarity (which the decoder follows) being forgotten
#[test]
fn an_inverted_uart_is_read_with_inverted_or_auto() {
    let baud = 9600.0;
    let (good, dt) = uart_line(Parity::Even, b"Hello, scope", baud);
    let sh = Shared { baud, bits: 8, parity: Parity::Even, ..shared() };
    assert_swap_is_read("Uart", &good, dt, [0; OPTS], sh, data);
    // the serial trigger set to Inverted (idle low) is the line as it is wired: the Polarity row reverses that
    let trig = Shared { uart_inv: true, ..sh };
    let (idle_low, used) = run_with("Uart", &flip(&good), dt, (1.65, 0.4), [0; OPTS], Polarity::Normal, trig);
    let (as_wired, _) = run_with("Uart", &good, dt, (1.65, 0.4), [0; OPTS], Polarity::Normal, sh);
    assert!(!used && bad(&idle_low) == 0 && data(&idle_low) == data(&as_wired));
}

// Break it catches: Modbus RTU whose CRC and parity fail on a swapped RS-485 pair without Auto finding it
#[test]
fn an_inverted_modbus_rtu_line_is_read_with_inverted_or_auto() {
    let baud = 19200.0;
    let f = gen::modbus_rtu_frame(0x11, &[0x03, 0x00, 0x6b, 0x00, 0x03]);
    let mut w = Wave::new(0.0, 3.3, 0.02 / baud);
    w.level(true, 50.0 / baud);
    for &b in &f {
        w.uart(b as u32, 8, Parity::Even, 1, baud);
    }
    w.level(true, 60.0 / baud);
    let dt = 1.0 / baud / 16.0;
    let good = w.done().sample(dt, 20.0 / baud);
    let all = |p: &[Packet]| -> Vec<u32> { p.iter().map(|x| x.value).collect() };
    // option rows: baud 19200 (index 4), even parity (1)
    assert_swap_is_read("ModbusRtu", &good, dt, [4, 1, 0, 0], shared(), all);
}

// Break it catches: Modbus ASCII whose LRC and parity fail on a swapped pair without Auto finding it
#[test]
fn an_inverted_modbus_ascii_line_is_read_with_inverted_or_auto() {
    let baud = 9600.0;
    let f = gen::modbus_ascii_frame(0x11, &[0x03, 0x00, 0x6b, 0x00, 0x03]);
    let mut w = Wave::new(0.0, 3.3, 0.02 / baud);
    w.level(true, 50.0 / baud);
    for &b in &f {
        w.uart(b as u32, 7, Parity::Even, 1, baud);
    }
    w.level(true, 60.0 / baud);
    let dt = 1.0 / baud / 16.0;
    let good = w.done().sample(dt, 20.0 / baud);
    let all = |p: &[Packet]| -> Vec<u32> { p.iter().map(|x| x.value).collect() };
    // baud 9600 (3), 7 data bits (0), even parity (0)
    assert_swap_is_read("ModbusAscii", &good, dt, [3, 0, 0, 0], shared(), all);
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
    let want = ["Uart", "Can", "Lin", "Dmx512", "Uart9", "ModbusRtu", "ModbusAscii", "CanFd", "FlexRay", "Arinc429"];
    assert_eq!(with, [&want[..], &["Mil1553", "Usb"]].concat());
}

fn every_value(p: &[Packet]) -> Vec<u32> {
    p.iter().map(|x| x.value).collect()
}

// Break it catches: CAN that reads a bus with the signal row on the wrong choice (CAN_H against RX) as errors
// without Inverted or Auto finding the other reading
#[test]
fn an_inverted_can_line_is_read_with_inverted_or_auto() {
    use oos_dsp::sim::{can_logic, CanFrame};
    let f = [
        CanFrame { id: 0x123, ext: false, rtr: false, data: vec![1, 2, 3, 4] },
        CanFrame { id: 0x1abc_def0, ext: true, rtr: false, data: vec![0xde, 0xad] },
    ];
    let dt = 1.0 / 500e3 / 16.0;
    let good = can_logic(&f, 500e3, 0.0, 3.3).sample(dt, 20.0 / 500e3);
    assert_swap_is_read("Can", &good, dt, [0; OPTS], shared(), every_value);
}

// Break it catches: CAN-FD likewise, with the bit rate switch
#[test]
fn an_inverted_can_fd_line_is_read_with_inverted_or_auto() {
    let f = [gen::FdFrame { id: 0x123, ext: false, fd: true, brs: true, esi: false, data: (1..=12).collect() }];
    let dt = 1.0 / 2e6 / 8.0;
    let good = gen::can_fd_logic(&f, 500e3, 2e6, 0.0, 3.3).sample(dt, 20.0 / 500e3);
    // bit rate 500 kbit/s (2), data rate 2 Mbit/s (1), the signal RX/TX (0)
    assert_swap_is_read("CanFd", &good, dt, [2, 1, 0, 0], shared(), every_value);
}

// Break it catches: LIN (idle high, checksum) that is not read on an inverted line
#[test]
fn an_inverted_lin_line_is_read_with_inverted_or_auto() {
    let dt = 1.0 / 19200.0 / 16.0;
    let good = oos_dsp::sim::lin_logic(&[(0x21, vec![0x10, 0x20, 0x30, 0x40], true)], 19200.0, 0.0, 3.3)
        .sample(dt, 10.0 / 19200.0);
    let sh = Shared { baud: 19200.0, ..shared() };
    assert_swap_is_read("Lin", &good, dt, [0; OPTS], sh, every_value);
}

// Break it catches: FlexRay (a swapped BP/BM pair) whose header and frame CRCs fail without Auto finding it
#[test]
fn an_inverted_flexray_line_is_read_with_inverted_or_auto() {
    let fr = |id: u16, payload: Vec<u8>| gen::FrFrame {
        ppi: false,
        nf: true,
        sync: false,
        startup: false,
        id,
        cycle: 3,
        payload,
        channel_b: false,
    };
    let frames = [fr(5, (0..8).collect()), fr(100, vec![0xa5, 0x5a])];
    let mut bits = Vec::new();
    for f in &frames {
        bits.extend(gen::flexray_bits(f, 11));
        bits.extend([true; 20]);
    }
    let tb = 1e-7;
    let dt = tb / 8.0;
    let good = oos_dsp::sim::bits_logic(&bits, tb, 20, 0.0, 3.3).sample(dt, 20.0 * tb);
    assert_swap_is_read("FlexRay", &good, dt, [0; OPTS], shared(), every_value);
}

// Break it catches: USB with D+ and D- swapped (J and K exchanged: every sync and PID fails) that Inverted or Auto
// do not read as the good pair, in the full-speed and the automatic setting, or D+ alone not mirrored
#[test]
fn a_usb_pair_with_d_plus_and_d_minus_swapped_is_read_with_inverted_or_auto() {
    use oos_dsp::decode::{usb_pid, UsbSpeed};
    let pk = [
        gen::usb_sof(0x710),
        gen::usb_token(0x2D, 0x15, 0xe),
        gen::usb_data(0xC3, &[0x80, 0x06, 0x00, 0x01, 0x00, 0x00, 0x40, 0x00]),
        vec![usb_pid(2)],
    ];
    let mut items = vec![gen::UsbItem::Idle(1e-6)];
    items.extend(pk.iter().flat_map(|p| [gen::UsbItem::Idle(1e-6), gen::UsbItem::packet(p, 12e6)]));
    let (p, m) = gen::usb(&items, false, 15e-9, 0.0, 3.3);
    let dt = 1.0 / 12e6 / 8.0;
    let n = ((p.seg.last().map_or(0.0, |s| s.0) + 5e-6) / dt) as usize;
    let (dp, dm) = (sample(|t| p.v(t), 0.0, dt, n), sample(|t| m.v(t), 0.0, dt, n));
    let m_info = MODULES.iter().find(|m| m.key == "Usb").unwrap();
    let read = |a: &[f32], b: Option<&[f32]>, speed: UsbSpeed, pol: Polarity| {
        let opt = [pol as usize, 0, 0, 0];
        let sh = Shared { usb_speed: speed, ..shared() };
        let lines = [Some(a), None, b, None];
        let l = Lines { lines, dt, thr: 1.65, hy: 0.3, opt, sh, chans: [None; 4], src: [None; 4] };
        m_info.decode_used(&l).unwrap()
    };
    for speed in [UsbSpeed::Full, UsbSpeed::Auto] {
        let (clean, used) = read(&dp, Some(&dm), speed, Polarity::Normal);
        assert!(!used && clean.len() > 20 && bad(&clean) == 0, "{speed:?}: {} fields, {} failed", clean.len(), bad(&clean));
        // the pair connected the other way round: D+ carries what D- did
        let (normal, _) = read(&dm, Some(&dp), speed, Polarity::Normal);
        assert!(bad(&normal) > 0 || every_value(&normal) != every_value(&clean), "{speed:?}: Normal read it right");
        for pol in [Polarity::Inverted, Polarity::Auto] {
            let (got, used) = read(&dm, Some(&dp), speed, pol);
            assert!(used && bad(&got) == 0 && every_value(&got) == every_value(&clean), "{speed:?} {pol:?}");
        }
        let (got, used) = read(&dp, Some(&dm), speed, Polarity::Auto);
        assert!(!used && every_value(&got) == every_value(&clean), "{speed:?}: Auto on a good pair");
    }
    // D+ alone on a line mirrored about the threshold
    let flipped = flip(&dp);
    let (clean, _) = read(&dp, None, UsbSpeed::Full, Polarity::Normal);
    let (got, used) = read(&flipped, None, UsbSpeed::Full, Polarity::Inverted);
    assert!(used && every_value(&got) == every_value(&clean) && !clean.is_empty());
}

/// The sentences the diagnosis of module `key` gives for `v` decoded with the polarity Normal.
fn sentences(key: &str, v: &[f32], dt: f64, opt: [usize; OPTS], sh: Shared) -> String {
    let m = MODULES.iter().find(|m| m.key == key).unwrap();
    let lines = [Some(v), None, None, None];
    let l = Lines { lines, dt, thr: 1.65, hy: 0.4, opt, sh, chans: [None; 4], src: [None; 4] };
    let packets = m.decode(&l).unwrap_or_default();
    oos_dsp::diagnosis::diagnose(m, &l, &packets).lines.join(" ")
}

const HINT: &str = "set Polarity to Auto or Inverted";

// Break it catches: the hint missing for a decoder that has the row (an agent stuck on an inverted bus), or given
// for one that has none (a row that is not there cannot be set)
#[test]
fn the_polarity_hint_comes_for_decoders_that_have_the_row_and_for_no_other() {
    let baud = 9600.0;
    let (good, dt) = uart_line(Parity::Even, b"Hello, scope", baud);
    let swapped = flip(&good);
    let sh = Shared { baud, bits: 8, parity: Parity::Even, ..shared() };
    assert!(sentences("Uart", &swapped, dt, [0; OPTS], sh).contains(HINT));
    assert!(!sentences("Uart", &good, dt, [0; OPTS], sh).contains(HINT));
    // every decoder without a Polarity row on the same swapped line
    for m in MODULES.iter().filter(|m| m.polarity.is_none()) {
        let text = sentences(m.key, &swapped, dt, [0; OPTS], sh);
        assert!(!text.contains(HINT), "{}: {text}", m.key);
    }
}

// Break it catches: the claim that a swapped pair needs no Polarity row for MLT-3 and USB PD (the docs say so): the
// reading of the mirrored line must be the reading of the line. S/PDIF has the same proof in decode/audio.rs.
#[test]
fn a_swapped_pair_reads_the_same_in_the_decoders_without_a_polarity_row() {
    let bytes = [0xffu8, 0xff, 0x55, 0xd5, 0x0f, 0xa3];
    let cycle = [0.0f32, 1.0, 0.0, -1.0];
    let mut v = vec![0.0f32; 10];
    let mut at = 0usize;
    for b in bytes {
        for k in 0..8 {
            at += usize::from(b >> k & 1 == 1);
            v.extend(std::iter::repeat_n(cycle[at % 4], 10));
        }
    }
    let flipped: Vec<f32> = v.iter().map(|x| -x).collect();
    let mlt = |v: &[f32]| data(&oos_dsp::linecode::mlt3(v, 1e-8, 1e7, 0.0, 0.1));
    assert_eq!(mlt(&flipped), mlt(&v));
    assert_eq!(mlt(&v), bytes.map(u32::from));
    // USB PD: BMC at 300 kbit/s, which carries every bit in a transition
    let f = [gen::PdFrame::Msg { sop: 0, header: 0x1161, data: vec![0x0001_912c], bad_crc: false }];
    let l = gen::usbpd(&f, 300e3, 0.0, 1.1);
    let dt = 1.0 / 12e6;
    let good: Vec<f32> = oos_dsp::sim::sample(|t| l.v(t), 0.0, dt, (l.seg.last().unwrap().0 / dt) as usize + 100);
    let swapped: Vec<f32> = good.iter().map(|x| 1.1 - x).collect();
    let pd = |v: &[f32]| -> Vec<(u32, bool)> {
        oos_dsp::decode::usbpd(v, dt, 0.55, 0.2).iter().map(|x| (x.value, x.ok)).collect()
    };
    assert!(pd(&good).iter().all(|x| x.1) && pd(&good).len() > 4);
    assert_eq!(pd(&swapped), pd(&good));
    // DCC: the bits are the lengths of the two halves of a period, the same on a swapped pair
    let dcc_frames = [
        gen::DccFrame { address: 0x03, data: vec![0x3f], xor: None, preamble: 14 },
        gen::DccFrame { address: 0x28, data: vec![0x00, 0x28], xor: None, preamble: 12 },
    ];
    let dt = oos_dsp::decode::DCC_ONE_HALF / 8.0;
    let good = gen::dcc(&dcc_frames, 0.0, 3.3).sample(dt, 200e-6);
    let swapped = flip(&good);
    let dcc = |v: &[f32]| -> Vec<(u32, bool)> {
        oos_dsp::decode::dcc(v, dt, 1.65, 0.3).iter().map(|x| (x.value, x.ok)).collect()
    };
    assert!(dcc(&good).len() > 6 && dcc(&good).iter().all(|x| x.1));
    assert_eq!(dcc(&swapped), dcc(&good));
    // SENT: every nibble is the time between two falling edges, and the same between two rising ones
    let fr = [
        gen::SentFrame { status: 0x3, data: vec![0x7, 0x8, 0x9, 0x5, 0x5, 0x5], crc: None, pause: 56 },
        gen::SentFrame { status: 0xf, data: vec![0x1, 0x2, 0x3, 0x4, 0x5, 0x6], crc: None, pause: 0 },
        gen::SentFrame { status: 0x0, data: vec![0xa, 0xb, 0xc, 0x1, 0x2, 0x3], crc: None, pause: 0 },
    ];
    let dt = 0.3e-6;
    let good = gen::sent(&fr, 3e-6, 0.0, 5.0).sample(dt, 20e-6);
    let swapped: Vec<f32> = good.iter().map(|x| 5.0 - x).collect();
    let sent = |v: &[f32]| -> Vec<(u32, bool)> {
        oos_dsp::decode::sent(v, dt, Some(3e-6), 6, 2.5, 0.5).iter().map(|x| (x.value, x.ok)).collect()
    };
    assert!(sent(&good).len() > 8 && sent(&good).iter().all(|x| x.1));
    assert_eq!(sent(&swapped), sent(&good));
}
