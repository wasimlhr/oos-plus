// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! CAN and LIN decoders against generated frames with known content.

use oos_dsp::decode::{can, lin, lin_checksum, lin_pid, lin_with, Kind, LinChecksum, Packet};
use oos_dsp::sim::{add_noise, bits_logic, can_bits, can_logic, lin_logic, sample, CanFrame};

fn ids(p: &[Packet]) -> Vec<(u32, bool, bool)> {
    p.iter().filter_map(|x| if let Kind::Id { ext, rtr } = x.kind { Some((x.value, ext, rtr)) } else { None }).collect()
}
fn data(p: &[Packet]) -> Vec<u32> {
    p.iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect()
}
fn crcs(p: &[Packet]) -> Vec<bool> {
    p.iter().filter(|x| x.kind == Kind::Crc).map(|x| x.ok).collect()
}

#[test]
fn can_standard_extended_and_remote_frames() {
    let frames = [
        CanFrame { id: 0x123, ext: false, rtr: false, data: vec![0xde, 0xad, 0xbe, 0xef] },
        CanFrame { id: 0x1abc_def0 & 0x1fff_ffff, ext: true, rtr: false, data: vec![0x00, 0xff, 0x55] },
        CanFrame { id: 0x7ff, ext: false, rtr: true, data: vec![0; 2] },
        CanFrame { id: 0x0f0, ext: false, rtr: false, data: vec![0xff; 8] }, // long stuffed runs
    ];
    let br = 500e3;
    let l = can_logic(&frames, br, 0.0, 3.3);
    let dt = 1.0 / 25e6; // 50 samples per bit
    let v = sample(|t| l.v(t), 0.0, dt, 25_000);
    let p = can(&v, dt, br, 1.65, 0.2, false);
    assert_eq!(
        ids(&p),
        vec![
            (0x123, false, false),
            (0x1abc_def0 & 0x1fff_ffff, true, false),
            (0x7ff, false, true),
            (0x0f0, false, false)
        ]
    );
    let mut want = vec![0xde, 0xad, 0xbe, 0xef, 0x00, 0xff, 0x55];
    want.extend([0xff; 8]);
    assert_eq!(data(&p), want);
    assert_eq!(crcs(&p), vec![true; 4]);
    assert!(p.iter().filter(|x| x.kind == Kind::Ack).all(|x| x.ok));
}

#[test]
fn can_detects_a_flipped_bit_and_a_missing_ack() {
    let f = CanFrame { id: 0x2a5, ext: false, rtr: false, data: vec![0x12, 0x34] };
    let mut bits = can_bits(&f, false);
    // flip one data bit that is not next to a stuff position: CRC must fail (or the frame be rejected)
    let i = 1 + 11 + 3 + 4 + 5;
    bits[i] = !bits[i];
    let l = bits_logic(&bits, 1.0 / 250e3, 12, 0.0, 5.0);
    let dt = 1.0 / 10e6;
    let v = sample(|t| l.v(t), 0.0, dt, 6000);
    let p = can(&v, dt, 250e3, 2.5, 0.2, false);
    assert!(crcs(&p).iter().all(|&ok| !ok), "{:?}", p);
    assert!(p.iter().filter(|x| x.kind == Kind::Ack).all(|x| !x.ok));
}

#[test]
fn can_on_can_h_with_noise() {
    // CAN_H against ground: recessive 2.5 V, dominant 3.5 V (inverted logic), 30 mV rms noise, 1 Mbit/s
    let f = [CanFrame { id: 0x555, ext: false, rtr: false, data: vec![1, 2, 3] }];
    let l = can_logic(&f, 1e6, 3.5, 2.5);
    let dt = 1.0 / 62.5e6;
    let mut v = sample(|t| l.v(t), 0.0, dt, 10_000);
    add_noise(&mut v, 0.03, 7);
    let p = can(&v, dt, 1e6, 3.0, 0.1, true);
    assert_eq!(ids(&p), vec![(0x555, false, false)]);
    assert_eq!(data(&p), vec![1, 2, 3]);
    assert_eq!(crcs(&p), vec![true]);
}

#[test]
fn lin_classic_and_enhanced_frames() {
    let frames =
        [(0x10u8, vec![0x01, 0x02, 0x03], false), (0x3c, vec![0xaa, 0x55, 0x00, 0xff, 0x12, 0x34, 0x56, 0x78], true)];
    let l = lin_logic(&frames, 19200.0, 0.0, 12.0);
    let dt = 1.0 / 1e6;
    let v = sample(|t| l.v(t), 0.0, dt, 20_000);
    let p = lin(&v, dt, 19200.0, 6.0, 0.5);
    assert_eq!(p.iter().filter(|x| x.kind == Kind::Break).count(), 2);
    assert_eq!(ids(&p).iter().map(|x| x.0).collect::<Vec<_>>(), vec![0x10, 0x3c]);
    assert!(p.iter().filter(|x| matches!(x.kind, Kind::Id { .. })).all(|x| x.ok), "PID parity");
    let d = data(&p);
    // sync bytes 0x55 come first in each frame
    assert_eq!(d, vec![0x55, 0x01, 0x02, 0x03, 0x55, 0xaa, 0x55, 0x00, 0xff, 0x12, 0x34, 0x56, 0x78]);
    assert_eq!(crcs(&p), vec![true, true]);
}

#[test]
fn lin_wrong_checksum_is_flagged() {
    // build a frame with a corrupted last data byte after the checksum was computed: regenerate with other data
    let good = lin_logic(&[(0x22, vec![9, 8, 7], true)], 9600.0, 0.0, 12.0);
    let dt = 1.0 / 500e3;
    let mut v = sample(|t| good.v(t), 0.0, dt, 9000);
    // invert one data bit of the second data byte (byte 4 of the frame incl. sync/PID, bit 3)
    let bit = 1.0 / 9600.0;
    let t0 = 12.0 * bit + 14.0 * bit + 4.0 * 11.0 * bit + (1.0 + 3.0 + 0.2) * bit;
    let (i0, i1) = ((t0 / dt) as usize, ((t0 + 0.6 * bit) / dt) as usize);
    let lo_v = v[i0];
    for x in &mut v[i0..i1] {
        *x = if lo_v > 6.0 { 0.0 } else { 12.0 };
    }
    let p = lin(&v, dt, 9600.0, 6.0, 0.5);
    assert_eq!(crcs(&p), vec![false], "{:?}", p);
}

/// An error flag is listed: six dominant bits and more in a row, which no frame holds. Here seven of them (the flag
/// of one node and the start of another's behind it) between idle line and a frame; the frame after it is read.
#[test]
fn can_an_error_flag_is_listed() {
    let frame = CanFrame { id: 0x123, ext: false, rtr: false, data: vec![1, 2] };
    let mut bits = vec![true; 12];
    bits.extend([false; 7]);
    bits.extend([true; 12]);
    bits.extend(can_bits(&frame, true));
    bits.extend([true; 12]);
    let rate = 500e3;
    let l = bits_logic(&bits, 1.0 / rate, 12, 0.0, 3.3);
    let dt = 1.0 / 25e6;
    let v = sample(|t| l.v(t), 0.0, dt, 12_000);
    let flag = Kind::Field("!ERROR FRAME");
    for p in [can(&v, dt, rate, 1.65, 0.1, false), oos_dsp::decode::can_fd(&v, dt, rate, 2e6, 1.65, 0.1, false)] {
        let flags: Vec<&Packet> = p.iter().filter(|x| x.kind == flag).collect();
        assert_eq!(flags.len(), 1, "{p:?}");
        assert!(!flags[0].ok && p[0].kind == flag, "it comes first and is an error");
        let bit_times = (flags[0].t1 - flags[0].t0) * dt * rate;
        assert!((bit_times - 7.0).abs() < 0.2, "{bit_times} bit times");
        assert_eq!(ids(&p), vec![(0x123, false, false)]);
        assert_eq!((data(&p), crcs(&p)), (vec![1, 2], vec![true]));
    }
    // a frame alone has no such field: five equal bits are the most it holds
    let l = can_logic(&[CanFrame { id: 0, ext: false, rtr: false, data: vec![0; 8] }], rate, 0.0, 3.3);
    let v = sample(|t| l.v(t), 0.0, dt, 12_000);
    assert!(can(&v, dt, rate, 1.65, 0.1, false).iter().all(|x| x.kind != flag));
    // a line that is no CAN bus has long low pulses and no frame: nothing is listed, no error either (a UART at
    // 115200 baud read as CAN at 500 kbit/s; a trigger on errors fired on it)
    let l = oos_dsp::sim::uart_logic(b"OOS 0.1 OOS", 115200.0, 8, None, 0.0, 0.0, 3.3);
    let v = sample(|t| l.v(t), 0.0, dt, 30_000);
    assert!(can(&v, dt, rate, 1.65, 0.1, false).iter().all(|x| x.kind != flag));
    assert!(oos_dsp::decode::can_fd(&v, dt, rate, 2e6, 1.65, 0.1, false).iter().all(|x| x.kind != flag));
    // and a pulse longer than the flags of two nodes is no flag: a bus held low
    use oos_dsp::decode::can_error_flag;
    assert!(!can_error_flag(13.0) && can_error_flag(6.0) && !can_error_flag(5.0));
}

/// What a packet says, without its times.
fn said(p: &[Packet]) -> Vec<(Kind, u32, bool)> {
    p.iter().map(|x| (x.kind, if x.kind == Kind::Break { 0 } else { x.value }, x.ok)).collect()
}

/// The bits of a LIN frame from its bytes as they are sent: a break of 13 bits and its delimiter, then every byte
/// with a start bit, eight bits LSB first, a stop bit and a bit of pause; then `idle` bit times of idle line.
fn lin_bits(bytes: &[u8], idle: usize) -> Vec<bool> {
    let mut bits = vec![false; 13];
    bits.push(true);
    for &v in bytes {
        bits.push(false);
        bits.extend((0..8).map(|i| (v >> i) & 1 == 1));
        bits.extend([true, true]);
    }
    bits.extend(vec![true; idle]);
    bits
}

/// The bit rate of a frame is that of its sync field, as a node takes it: a bus at 18350 bit/s reads the same
/// whatever rate the decoder is set to. It was read at the rate that was set, and a rate that is not in the list
/// could not be set.
#[test]
fn lin_takes_its_bit_rate_from_the_sync_field() {
    let frames = [(0x09u8, vec![0x70, 0xd3, 0xbb, 0x98, 0xd8, 0xc5, 0x46, 0xfe], true), (0x0a, vec![0xd0, 0x88], true)];
    let l = lin_logic(&frames, 18350.0, 0.0, 12.0);
    let dt = 1.0 / 1e6;
    let v = sample(|t| l.v(t), 0.0, dt, 14_000);
    let want = lin(&v, dt, 18350.0, 6.0, 0.5);
    assert_eq!(ids(&want).iter().map(|x| x.0).collect::<Vec<_>>(), [0x09, 0x0a]);
    assert_eq!((crcs(&want), want.iter().all(|x| x.ok)), (vec![true, true], true));
    assert_eq!(data(&want), [0x55, 0x70, 0xd3, 0xbb, 0x98, 0xd8, 0xc5, 0x46, 0xfe, 0x55, 0xd0, 0x88]);
    for baud in [19200.0, 9600.0, 2400.0, 115200.0] {
        let p = lin(&v, dt, baud, 6.0, 0.5);
        assert_eq!(said(&p), said(&want), "set to {baud}");
        for b in p.iter().filter(|x| x.kind == Kind::Break) {
            assert!((b.value as f64 / 18350.0 - 1.0).abs() < 0.01, "measured {} bit/s", b.value);
        }
    }
}

/// A frame whose sync field is not 0x55 is listed with its sync as an error, read at the rate of the other frames;
/// it was a break alone, without a word of what was wrong.
#[test]
fn lin_a_bad_sync_is_listed_as_an_error() {
    let pid = lin_pid(0x04);
    let mut bits = lin_bits(&[0x55, lin_pid(0x01), 0x11, lin_checksum(&[lin_pid(0x01), 0x11])], 20);
    bits.extend(lin_bits(&[0x54, pid, 0x3f, lin_checksum(&[pid, 0x3f])], 20));
    let l = bits_logic(&bits, 1.0 / 19200.0, 12, 0.0, 12.0);
    let dt = 1.0 / 1e6;
    let v = sample(|t| l.v(t), 0.0, dt, 9_000);
    let p = lin(&v, dt, 19200.0, 6.0, 0.5);
    assert_eq!(ids(&p).iter().map(|x| x.0).collect::<Vec<_>>(), [0x01, 0x04]);
    let sync = |x: &&Packet| x.kind == Kind::Data && matches!(x.value, 0x54 | 0x55);
    let syncs: Vec<(u32, bool)> = p.iter().filter(sync).map(|x| (x.value, x.ok)).collect();
    assert_eq!(syncs, [(0x55, true), (0x54, false)]);
    assert_eq!((data(&p), crcs(&p)), (vec![0x55, 0x11, 0x54, 0x3f], vec![true, true]));
    // it is read at the rate of the frame beside it, whatever rate is set
    assert_eq!(said(&lin(&v, dt, 115200.0, 6.0, 0.5)), said(&p));
}

/// A byte long after a frame is not its checksum: the frame ends with its own, and the byte is not listed. The last
/// byte before the next break was the checksum, however late it came.
#[test]
fn lin_a_late_byte_is_not_the_checksum_of_the_frame_before() {
    let pid = lin_pid(0x08);
    let frame = [0x55, pid, 0x67, 0x80, lin_checksum(&[pid, 0x67, 0x80])];
    // the frame, 1500 bit times of idle line, a byte by itself, idle line, the frame again
    let mut bits = lin_bits(&frame, 1500);
    bits.push(false);
    bits.extend((0..8).map(|i| (0xf0u8 >> i) & 1 == 1));
    bits.extend(vec![true; 300]);
    bits.extend(lin_bits(&frame, 20));
    let l = bits_logic(&bits, 1.0 / 19200.0, 12, 0.0, 12.0);
    let dt = 1.0 / 500e3;
    let v = sample(|t| l.v(t), 0.0, dt, 52_000);
    let p = lin(&v, dt, 19200.0, 6.0, 0.5);
    assert_eq!(p.iter().filter(|x| x.kind == Kind::Break).count(), 2);
    assert_eq!((data(&p), crcs(&p)), (vec![0x55, 0x67, 0x80, 0x55, 0x67, 0x80], vec![true, true]));
    assert!(p.iter().all(|x| x.ok), "{p:?}");
}

/// The checksum a frame is checked against: either, the enhanced one of LIN 2.x (the diagnostic frames 3C and 3D
/// keep the classic one), or the classic one of LIN 1.x.
#[test]
fn lin_checksum_version() {
    let data = vec![0x01, 0x02, 0x03];
    let frames = [(0x10u8, data.clone(), false), (0x11, data, true), (0x3c, vec![0xaa, 0x55], false)];
    let l = lin_logic(&frames, 19200.0, 0.0, 12.0);
    let dt = 1.0 / 1e6;
    let v = sample(|t| l.v(t), 0.0, dt, 16_000);
    let sums = |c: LinChecksum| crcs(&lin_with(&v, dt, 19200.0, c, 6.0, 0.5));
    assert_eq!(sums(LinChecksum::Either), [true, true, true]);
    assert_eq!(sums(LinChecksum::Enhanced), [false, true, true]);
    assert_eq!(sums(LinChecksum::Classic), [true, false, true]);
}

/// A stuff bit right after the last CRC bit (ISO 11898-1: stuffing covers SOF .. CRC sequence) must not be taken for
/// the CRC delimiter.
#[test]
fn can_stuff_bit_after_the_crc() {
    let br = 500e3;
    let dt = 1.0 / 25e6;
    let mut checked = 0;
    for id in 0..0x800u32 {
        let f = CanFrame { id, ext: false, rtr: false, data: vec![id as u8, 0x5a] };
        // does the stuffed bit stream end its CRC with five equal bits? (the frame before the delimiter)
        let crc_end = {
            let mut raw = vec![false];
            (0..11).rev().for_each(|i| raw.push((id >> i) & 1 == 1));
            raw.extend([false, false, false]); // RTR, IDE, r0
            (0..4).rev().for_each(|i| raw.push((2u32 >> i) & 1 == 1));
            for b in &f.data {
                (0..8).rev().for_each(|i| raw.push((b >> i) & 1 == 1));
            }
            let crc = oos_dsp::decode::can_crc15(&raw);
            (0..15).rev().for_each(|i| raw.push((crc >> i) & 1 == 1));
            // run length at the end after stuffing
            let (mut run, mut last, mut cnt) = (0, None, 0);
            for &b in &raw {
                if Some(b) == last {
                    run += 1;
                } else {
                    run = 1;
                    last = Some(b);
                }
                if run == 5 {
                    run = 1;
                    last = Some(!b);
                    cnt = 0;
                } else {
                    cnt += 1;
                }
            }
            cnt == 0 // the last bit completed a run of five: a stuff bit follows
        };
        if !crc_end {
            continue;
        }
        let mut bits = can_bits(&f, true);
        bits.extend([true; 11]);
        let l = bits_logic(&bits, 1.0 / br, 12, 0.0, 3.3);
        let v = sample(|t| l.v(t), 0.0, dt, 8_000);
        let p = can(&v, dt, br, 1.65, 0.2, false);
        assert_eq!(crcs(&p), vec![true], "id {:#x}", id);
        assert!(
            p.iter().any(|x| x.kind == Kind::Ack && x.ok),
            "id {:#x}: {:?}",
            id,
            p.iter().filter(|x| x.kind == Kind::Ack).collect::<Vec<_>>()
        );
        checked += 1;
        if checked >= 5 {
            break;
        }
    }
    assert!(checked > 0, "no frame with a stuff bit after the CRC found");
}

/// A bus that is slow on its way back to recessive, as CAN_H was on a unit (500 kbit/s, the edge to dominant in
/// under 0.1 us, the way back with a time constant of 1.6 us: at the threshold in the middle a dominant bit read
/// 3.1 us long and a recessive one 0.9 us). The bit timing follows the edges to dominant alone, as a node's does,
/// and every frame is read right with both decoders and both polarities. With a resynchronisation on every edge the
/// bit behind a slow edge was sampled in its neighbour: no frame, or one with a wrong identifier.
#[test]
fn can_with_a_slow_edge_to_recessive_is_read_right() {
    let frames = [
        CanFrame { id: 0x519, ext: false, rtr: false, data: vec![0x11, 0x22, 0x33] },
        CanFrame { id: 0x1597_eeb2, ext: true, rtr: false, data: vec![0xde, 0x55, 0xcb, 0xfa, 0x5d, 0x45, 0xad, 0x8c] },
        CanFrame { id: 0x757, ext: false, rtr: true, data: vec![] },
        CanFrame { id: 0x000, ext: false, rtr: false, data: vec![0x00; 8] },
    ];
    let rate = 500e3;
    let l = can_logic(&frames, rate, 0.0, 3.3);
    let dt = 1.0 / 25e6;
    let ideal = sample(|t| l.v(t), 0.0, dt, 60_000);
    // dominant (low on this line) at once, back to recessive with a time constant of 0.8 bit times
    let k = (1.0 - (-dt / (0.8 / rate)).exp()) as f32;
    let mut y = ideal[0];
    let slow: Vec<f32> = ideal
        .iter()
        .map(|&x| {
            y = if x < 1.65 { x } else { y + (x - y) * k };
            y
        })
        .collect();
    let high: Vec<f32> = slow.iter().map(|x| 3.3 - x).collect();
    let want: Vec<(u32, bool, bool)> = frames.iter().map(|f| (f.id, f.ext, f.rtr)).collect();
    for (v, invert) in [(&slow, false), (&high, true)] {
        let fd = oos_dsp::decode::can_fd(v, dt, rate, 2e6, 1.65, 0.1, invert);
        for p in [can(v, dt, rate, 1.65, 0.1, invert), fd] {
            assert_eq!(ids(&p), want, "invert {invert}");
            assert_eq!(crcs(&p), [true; 4], "invert {invert}");
            assert!(p.iter().all(|x| x.ok), "invert {invert}: {:?}", p.iter().find(|x| !x.ok));
        }
    }
}

/// A frame that the end of the record cuts off: both decoders give what is whole of it, the start and the identifier
/// once the record holds them, then each data byte it holds, and no CRC. With ten bits of the frame, nothing.
#[test]
fn can_gives_the_whole_fields_of_a_frame_the_record_cuts_off() {
    use oos_dsp::decode::can_fd;
    let rate = 500e3;
    let dt = 1.0 / 25e6;
    let bit = (1.0 / rate / dt) as usize;
    for (frame, id_bits) in [
        (CanFrame { id: 0x057, ext: false, rtr: false, data: vec![0x45, 0x68, 0x6c, 0x65, 0x72, 0x73] }, 20),
        (CanFrame { id: 0x1597_eea3, ext: true, rtr: false, data: vec![0xdc, 0x55, 0xcb, 0xfa] }, 42),
    ] {
        let l = can_logic(std::slice::from_ref(&frame), rate, 0.0, 3.3);
        let whole = sample(|t| l.v(t), 0.0, dt, 12_000);
        let want = (frame.id, frame.ext, false);
        // the 12 idle bits in front, then so many bits of the frame (stuff bits among them)
        let first = |bits: usize| &whole[..(12 + bits) * bit];
        type Decoder = fn(&[f32]) -> Vec<Packet>;
        const DT: f64 = 1.0 / 25e6;
        let plain: Decoder = |v| can(v, DT, 500e3, 1.65, 0.1, false);
        let fd: Decoder = |v| can_fd(v, DT, 500e3, 2e6, 1.65, 0.1, false);
        for decode in [plain, fd] {
            // the whole frame, with its CRC
            let p = decode(&whole);
            assert_eq!((ids(&p), crcs(&p)), (vec![want], vec![true]));
            // ten bits of it: nothing, the identifier is not whole
            assert!(decode(first(10)).is_empty());
            // the identifier and a few bits more: start and identifier
            let p = decode(first(id_bits));
            assert_eq!((p.len(), p[0].kind, ids(&p)), (2, Kind::Start, vec![want]), "{p:?}");
            // into the data field: the bytes that are whole, in their places
            let v = first(id_bits + 30);
            let p = decode(v);
            let got = data(&p);
            let sent: Vec<u32> = frame.data.iter().map(|&b| b as u32).collect();
            assert!((2..=3).contains(&got.len()) && got[..] == sent[..got.len()], "{p:?}");
            assert_eq!(p.len(), 2 + got.len());
            assert!(p.iter().all(|x| x.ok && x.t1 <= v.len() as f64));
        }
    }
}
