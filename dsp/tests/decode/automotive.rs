// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The FlexRay and SENT (SAE J2716) decoders against generated signals with known content.
//!
//! The tests of a new decoder of the family go here, as the decoder goes into the family's file in `dsp/src/decode/`;
//! its protocol module, registered in `ui/src/protocols/mod.rs` (ADR 0002), has its own tests in the ui.

use super::*;

fn fr(id: u16, cycle: u8, payload: Vec<u8>, channel_b: bool) -> FrFrame {
    FrFrame { ppi: false, nf: true, sync: false, startup: false, id, cycle, payload, channel_b }
}

fn fr_logic(frames: &[FrFrame], tss: usize, flip: Option<usize>) -> Vec<bool> {
    let mut all = Vec::new();
    for f in frames {
        let mut b = gen::flexray_bits(f, tss);
        if let Some(i) = flip {
            b[i] = !b[i];
        }
        all.extend(b);
        all.extend([true; 20]);
    }
    all
}

#[test]
fn flexray_frames_on_rxd() {
    let frames = [
        fr(5, 3, (0..8).collect(), false),
        FrFrame {
            ppi: false,
            nf: true,
            sync: true,
            startup: true,
            id: 2047,
            cycle: 63,
            payload: (0..16).map(|i| 0xf0 ^ i).collect(),
            channel_b: true,
        },
        fr(100, 0, vec![], false),
    ];
    let bits = fr_logic(&frames, 11, None);
    let tb = 1e-7;
    let l = bits_logic(&bits, tb, 20, 0.0, 3.3);
    let dt = tb / 8.0;
    let mut v = l.sample(dt, 20.0 * tb);
    add_noise(&mut v, 0.2, 6);
    let p = flexray(&v, dt, 10e6, 1.65, 0.4, false);
    assert_eq!(vals(&p, "TSS"), [11, 11, 11]);
    assert_eq!(vals(&p, "ID"), [5, 2047, 100]);
    assert_eq!(vals(&p, "IND"), [0b00100, 0b00111, 0b00100]);
    assert_eq!(vals(&p, "LEN"), [4, 8, 0]);
    assert_eq!(vals(&p, "CYC"), [3, 63, 0]);
    assert_eq!(oks(&p, "HCRC"), [true; 3]);
    let mut want: Vec<u32> = (0..8).collect();
    want.extend((0..16).map(|i| 0xf0 ^ i));
    assert_eq!(vals(&p, "D"), want);
    assert_eq!(oks(&p, "CRC"), [true; 3]);
    assert_eq!(
        p.iter().filter(|x| x.kind == Kind::Crc).map(|x| x.value2).collect::<Vec<_>>(),
        [Some(0), Some(1), Some(0)]
    );
    assert!(p.iter().all(|x| x.ok), "{:?}", lvo(&p));
}

#[test]
fn flexray_differential_pair() {
    // BP/BM: idle both 2.5 V (0 V difference), Data_1 = BP 3.0 / BM 2.0, Data_0 the inverse
    let frames = [fr(0x1a, 9, vec![0x12, 0x34, 0x56, 0x78], false), fr(0x1b, 10, vec![0xaa, 0x55], false)];
    let tb = 1e-7;
    let (mut bp, mut bm) =
        (Levels { seg: vec![(0.0, 2.5)], tr: tb / 10.0 }, Levels { seg: vec![(0.0, 2.5)], tr: tb / 10.0 });
    let mut t = 30.0 * tb;
    for f in &frames {
        for b in gen::flexray_bits(f, 9) {
            bp.seg.push((t, if b { 3.0 } else { 2.0 }));
            bm.seg.push((t, if b { 2.0 } else { 3.0 }));
            t += tb;
        }
        bp.seg.push((t, 2.5));
        bm.seg.push((t, 2.5));
        t += 25.0 * tb;
    }
    let dt = tb / 8.0;
    let n = (t / dt) as usize;
    let mut a = sample(|x| bp.v(x), 0.0, dt, n);
    let mut b = sample(|x| bm.v(x), 0.0, dt, n);
    add_noise(&mut a, 0.05, 1);
    add_noise(&mut b, 0.05, 2);
    let p = flexray(&diff(&a, &b), dt, 10e6, 0.0, 0.3, false);
    assert_eq!(vals(&p, "ID"), [0x1a, 0x1b]);
    assert_eq!(vals(&p, "D"), [0x12, 0x34, 0x56, 0x78, 0xaa, 0x55]);
    assert_eq!(oks(&p, "CRC"), [true, true]);
    assert_eq!(oks(&p, "HCRC"), [true, true]);
}

#[test]
fn flexray_crc_errors() {
    let f = [fr(0x33, 1, vec![1, 2, 3, 4], false)];
    let tb = 1e-7;
    let dt = tb / 8.0;
    let run = |flip: usize| {
        let l = bits_logic(&fr_logic(&f, 5, Some(flip)), tb, 20, 0.0, 3.3);
        flexray(&l.sample(dt, 20.0 * tb), dt, 10e6, 1.65, 0.4, false)
    };
    // bit index of byte k, bit j (MSB = 0): TSS 5 + FSS 1 + 10 per byte + BSS 2
    let at = |k: usize, j: usize| 5 + 1 + 10 * k + 2 + j;
    // a payload bit: header CRC fine, frame CRC fails
    let p = run(at(6, 3));
    assert_eq!((oks(&p, "HCRC"), oks(&p, "CRC")), (vec![true], vec![false]));
    assert_eq!(vals(&p, "D"), [1, 2 ^ 0x10, 3, 4]);
    // a frame ID bit: both fail
    let p = run(at(1, 2));
    assert_eq!((oks(&p, "HCRC"), oks(&p, "CRC")), (vec![false], vec![false]));
    // a BSS low bit missing: the frame breaks off with an error
    let p = run(at(7, 0) - 1);
    assert!(oks(&p, "!ERR").contains(&false), "{:?}", lvo(&p));
}

/// A record that starts or ends inside a frame: the cut frames give no errors (the protocol trigger's "Error" would
/// fire on them), the whole frame in between decodes.
#[test]
fn flexray_frames_cut_by_the_record() {
    // zero bytes: long low runs that look like a TSS inside the frame
    let frames = vec![fr(0x40, 1, vec![0, 0, 0x01, 0, 0x10, 0, 0, 0], false); 3];
    let tb = 1e-7;
    let dt = tb / 8.0;
    let v = bits_logic(&fr_logic(&frames, 11, None), tb, 20, 0.0, 3.3).sample(dt, 20.0 * tb);
    // frames of 11 + 1 + 10 * 16 + 2 bits, 20 idle bits apart, the first after 20 idle bits
    let len = 11.0 + 1.0 + 160.0 + 2.0 + 20.0;
    for cut in [30.0, 60.0, 95.0, 120.0, 150.0] {
        let a = ((20.0 + cut) * 8.0) as usize;
        let b = ((20.0 + 2.0 * len + cut) * 8.0) as usize;
        let p = flexray(&v[a..b], dt, 10e6, 1.65, 0.4, false);
        assert!(p.iter().all(|x| x.ok), "cut at {}: {:?}", cut, lvo(&p));
        // the second frame, and the header of the third when the record holds it
        let ids = vals(&p, "ID");
        assert!(ids == [0x40] || (cut > 52.0 && ids == [0x40, 0x40]), "cut at {}: {:?}", cut, lvo(&p));
        assert_eq!(oks(&p, "CRC"), [true], "cut at {}", cut);
    }
}

/// `kv` with the measured SENT tick rounded to 10 ns
fn skv(p: &[Packet]) -> Vec<(Kind, u32, bool)> {
    p.iter().map(|x| (x.kind, if x.kind == F("SYNC") { (x.value + 5) / 10 * 10 } else { x.value }, x.ok)).collect()
}

#[test]
fn sent_crc_table_and_methods() {
    // one nibble step of the CRC is the J2716 lookup table: crc(seed s, no data, legacy) = s, so check via data
    // [x] from seed 5: the table entry of 5 xor x
    let table = [0u8, 13, 7, 10, 14, 3, 9, 4, 1, 12, 6, 11, 15, 2, 8, 5];
    for x in 0..16u8 {
        assert_eq!(sent_crc(&[x], true), table[5] ^ x);
    }
    // the recommended method appends a zero nibble: processing the data and its CRC (legacy) gives 0
    for data in [vec![0u8, 0, 0, 0, 0, 0], vec![0xa, 0xb, 0xc, 1, 2, 3], vec![7], vec![15, 15, 15]] {
        let c = sent_crc(&data, false);
        assert!(c < 16);
        let mut d = data.clone();
        d.push(c);
        assert_eq!(sent_crc(&d, true), 0, "{:?}", data);
        assert_eq!(c, table[sent_crc(&data, true) as usize]);
    }
}

fn frames() -> Vec<SentFrame> {
    vec![
        SentFrame { status: 0x0, data: vec![0xa, 0xb, 0xc, 0x1, 0x2, 0x3], crc: None, pause: 0 },
        SentFrame { status: 0x8, data: vec![0x0, 0x0, 0x0, 0xf, 0xf, 0xf], crc: None, pause: 100 },
        SentFrame { status: 0x3, data: vec![0x7, 0x8, 0x9, 0x5, 0x5, 0x5], crc: None, pause: 56 },
        SentFrame { status: 0xf, data: vec![0x1, 0x2, 0x3, 0x4, 0x5, 0x6], crc: None, pause: 0 },
    ]
}

fn sent_want(fr: &[SentFrame], tick_ns: u32) -> Vec<(Kind, u32, bool)> {
    let mut want = Vec::new();
    for f in fr {
        let value = f.data.iter().fold(0u32, |a, &x| a << 4 | x as u32);
        want.extend([
            (F("SYNC"), tick_ns, true),
            (F("STAT"), f.status as u32, true),
            (Kind::Data, value, true),
            (Kind::Crc, sent_crc(&f.data, false) as u32, true),
        ]);
        if f.pause > 0 {
            want.push((F("PAUSE"), f.pause, true));
        }
    }
    want
}

#[test]
fn sent_frames_with_pause_and_tick_tolerance() {
    let fr = frames();
    // a nominal 3 us transmitter, one 15 % slow (3.45 us), and one at a 10 us tick decoded without a nominal tick
    for (tick, nominal) in [(3e-6, Some(3e-6)), (3.45e-6, Some(3e-6)), (3e-6, None), (10e-6, None)] {
        let l = gen::sent(&fr, tick, 0.0, 5.0);
        let dt = tick / 10.0;
        let mut v = l.sample(dt, 10.0 * tick);
        add_noise(&mut v, 0.1, 3);
        let p = sent(&v, dt, nominal, 6, 2.5, 0.5);
        assert_eq!(skv(&p), sent_want(&fr, (tick * 1e9).round() as u32), "tick {} nominal {:?}", tick, nominal);
        // the status nibble spans 12 + 0 ticks
        assert!((p[1].t1 - p[1].t0 - 12.0 * tick / dt).abs() < 0.2 * tick / dt);
        assert_eq!(p[2].value2, Some(6));
    }
    // a tick 30 % off the nominal: not taken for a sync
    let v = gen::sent(&fr, 3.9e-6, 0.0, 5.0).sample(0.3e-6, 0.0);
    assert!(sent(&v, 0.3e-6, Some(3e-6), 6, 2.5, 0.5).is_empty());
    // fewer data nibbles
    let fr3 = vec![
        SentFrame { status: 1, data: vec![0x1, 0x2, 0x3], crc: None, pause: 20 },
        SentFrame { status: 2, data: vec![0xf, 0xe, 0xd], crc: None, pause: 0 },
    ];
    let v = gen::sent(&fr3, 3e-6, 0.0, 5.0).sample(0.3e-6, 0.0);
    assert_eq!(skv(&sent(&v, 0.3e-6, Some(3e-6), 3, 2.5, 0.5)), sent_want(&fr3, 3000));
}

/// The status nibbles of an enhanced serial message of the slow channel: identifier of 8 bits, 12 data bits.
fn sent_enhanced(id: u32, data: u32) -> Vec<u8> {
    let bit = |v: u32, i: u32| v >> i & 1 == 1;
    // bit 3 of the 18 frames: 1111110, the configuration bit 0, four identifier bits, 0, four more, 0
    let mut b3 = vec![true, true, true, true, true, true, false, false];
    b3.extend((4..8).rev().map(|i| bit(id, i)));
    b3.push(false);
    b3.extend((0..4).rev().map(|i| bit(id, i)));
    b3.push(false);
    let d: Vec<bool> = (0..12).rev().map(|i| bit(data, i)).collect();
    let message: Vec<bool> = (0..12).flat_map(|n| [d[n], b3[6 + n]]).collect();
    let crc = sent_crc6(&message) as u32;
    let b2: Vec<bool> = (0..6).rev().map(|i| bit(crc, i)).chain(d).collect();
    // (the two low bits of the status nibble are the sensor's own)
    (0..18).map(|k| (b3[k] as u8) << 3 | (b2[k] as u8) << 2 | 1).collect()
}

/// The slow channel: a serial message over the status nibbles of 18 frames (enhanced) or 16 (short) is listed at the
/// end of the frame that completes it, with its identifier, its data and its CRC.
#[test]
fn sent_slow_channel_messages() {
    let slow = |p: &[Packet]| -> Vec<(Kind, u32, Option<u32>, bool)> {
        let keep = |x: &&Packet| matches!(x.kind, Kind::Field("SID" | "SDATA" | "SCRC"));
        p.iter().filter(keep).map(|x| (x.kind, x.value, x.value2, x.ok)).collect()
    };
    // identifier 07, data 075: its CRC is 01, as a capture of a real bus has it and the instrument that took it
    // reads it
    let status = sent_enhanced(0x07, 0x075);
    let chain: Vec<Option<(u32, f64)>> = status.iter().enumerate().map(|(k, &s)| Some((s as u32, k as f64))).collect();
    let got = sent_slow(&chain);
    let want = [(F("SID"), 0x07, None, true), (F("SDATA"), 0x075, Some(12), true), (F("SCRC"), 0x01, None, true)];
    assert_eq!(slow(&got), want);
    assert!(got.iter().all(|x| x.t0 == 17.0 && x.t1 == 17.0), "at the end of the 18th frame");
    // one data bit turned: the CRC does not match
    let mut turned = chain.clone();
    turned[10] = turned[10].map(|(s, t)| (s ^ 4, t));
    assert_eq!(slow(&sent_slow(&turned))[2].3, false);
    // a frame missing in the chain: no message
    let mut broken = chain.clone();
    broken[9] = None;
    assert!(sent_slow(&broken).is_empty());
    // two messages one after the other, with frames before them that are none
    let mut twice = vec![Some((0u32, 0.0)); 3];
    twice.extend(chain.iter().cloned());
    twice.extend(chain.iter().cloned());
    assert_eq!(sent_slow(&twice).len(), 6);
    // the short message, 16 frames: identifier 3, data A5, the CRC of the fast channel over its three nibbles
    let crc = sent_crc(&[0x3, 0xa, 0x5], false) as u32;
    let word = 0x3 << 12 | 0xa5 << 4 | crc;
    let status = |k: u32| (k == 0) as u32 * 8 | (word >> (15 - k) & 1) * 4;
    let short: Vec<Option<(u32, f64)>> = (0..16).map(|k| Some((status(k), k as f64))).collect();
    let want_short = [(F("SID"), 3, None, true), (F("SDATA"), 0xa5, Some(8), true), (F("SCRC"), crc, None, true)];
    assert_eq!(slow(&sent_slow(&short)), want_short);
    // through the signal: 18 frames with those status nibbles, and the message stands in the last of them
    let frame = |&s: &u8| SentFrame { status: s, data: vec![1, 2, 3, 4, 5, 6], crc: None, pause: 77 };
    let fr: Vec<SentFrame> = sent_enhanced(0x07, 0x075).iter().map(frame).collect();
    let (tick, dt) = (3e-6, 0.3e-6);
    let p = sent(&gen::sent(&fr, tick, 0.0, 5.0).sample(dt, 10.0 * tick), dt, Some(tick), 6, 2.5, 0.5);
    assert_eq!(slow(&p), want);
    let at = p.iter().position(|x| x.kind == F("SID")).unwrap();
    let syncs_before = p[..at].iter().filter(|x| x.kind == F("SYNC")).count();
    let syncs_after = p[at + 3..].iter().filter(|x| x.kind == F("SYNC")).count();
    assert_eq!((syncs_before, syncs_after), (18, 0), "in the 18th frame");
}

#[test]
fn sent_crc_error_invalid_nibble_and_cut_frames() {
    let mut fr = frames();
    let good = sent_crc(&fr[1].data, false);
    fr[1].crc = Some(good ^ 1);
    let tick = 3e-6;
    let dt = tick / 8.0;
    let v = gen::sent(&fr, tick, 0.0, 5.0).sample(dt, 0.0);
    let p = sent(&v, dt, Some(tick), 6, 2.5, 0.5);
    let crcs: Vec<(u32, bool)> = p.iter().filter(|x| x.kind == Kind::Crc).map(|x| (x.value, x.ok)).collect();
    assert_eq!(
        crcs,
        [
            (sent_crc(&fr[0].data, false) as u32, true),
            (good as u32 ^ 1, false),
            (sent_crc(&fr[2].data, false) as u32, true),
            (sent_crc(&fr[3].data, false) as u32, true)
        ]
    );
    // a nibble of 30 ticks (value 18) in the second frame: an error with the nominal tick, skipped without one
    let mut t = gen::sent_ticks(&frames());
    t[9 + 3] = 30; // frame 2 (after 9 intervals): sync, status, data 1, data 2
    let v = gen::sent_logic(&t, tick, 0.0, 5.0).sample(dt, 0.0);
    let p = sent(&v, dt, Some(tick), 6, 2.5, 0.5);
    let k = skv(&p);
    assert_eq!(k[..4], sent_want(&frames()[..1], 3000)[..]);
    assert_eq!(k[4..6], [(F("SYNC"), 3000, true), (F("!ERR"), 0, false)]);
    assert_eq!(k[6..], sent_want(&frames()[2..], 3000)[..]);
    let p = sent(&v, dt, None, 6, 2.5, 0.5);
    assert_eq!(skv(&p), [sent_want(&frames()[..1], 3000), sent_want(&frames()[2..], 3000)].concat());
    // a record that ends inside a frame: no error
    let w = gen::sent(&frames(), tick, 0.0, 5.0).sample(dt, 0.0);
    let half = &w[..w.len() * 3 / 5];
    assert!(sent(half, dt, Some(tick), 6, 2.5, 0.5).iter().all(|x| x.ok));
}
