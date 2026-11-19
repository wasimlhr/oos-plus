// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! I2S and MIL-STD-1553 decoders against generated streams with known content.

use oos_dsp::decode::{i2s, mil1553, Kind};
use oos_dsp::sim::{add_noise, i2s_logic, mil1553_message, mil1553_wave, sample};

#[test]
fn i2s_stereo_16_and_24_bit() {
    for bits in [16u32, 24] {
        let frames = [(0x1234i32, -2i32), (0x7fff, 0x0100), (-32768, 5)];
        let mask = (1u32 << bits) - 1;
        let (sck, ws, sd) = i2s_logic(&frames, bits, 48e3, 0.0, 3.3);
        let tb = 1.0 / (48e3 * 2.0 * bits as f64);
        let dt = tb / 20.0;
        let n = ((frames.len() as f64 * 2.0 * bits as f64 + 8.0) * tb / dt) as usize;
        let (c, w, d) =
            (sample(|t| sck.v(t), 0.0, dt, n), sample(|t| ws.v(t), 0.0, dt, n), sample(|t| sd.v(t), 0.0, dt, n));
        let p = i2s(&c, Some(&w), &d, bits, 1.65, 0.2);
        let got: Vec<(bool, u32)> = p
            .iter()
            .filter_map(|x| if let Kind::Sample { right } = x.kind { Some((right, x.value)) } else { None })
            .collect();
        let want: Vec<(bool, u32)> =
            frames.iter().flat_map(|&(l, r)| [(false, l as u32 & mask), (true, r as u32 & mask)]).collect();
        // the first word may be cut (no WS edge before it); all complete words must match
        assert!(got.len() >= want.len() - 1, "{} bits: {:?}", bits, got);
        let off = want.len() - got.len().min(want.len());
        assert_eq!(&got[..want.len() - off], &want[off..], "{} bits", bits);
    }
}

#[test]
fn mil1553_command_and_data_words_with_noise() {
    let words = [(true, 0x0c21u16), (false, 0xbeef), (false, 0x0000), (false, 0xffff)];
    let l = mil1553_wave(&words, 7.0, 4.0);
    let dt = 1e-6 / 50.0;
    let mut v = sample(|t| l.v(t), 0.0, dt, 50 * 110);
    add_noise(&mut v, 0.3, 3);
    let p = mil1553(&v, dt, 0.0, 1.0);
    let got: Vec<(bool, u32, bool)> = p
        .iter()
        .filter_map(|x| if let Kind::Word { cmd } = x.kind { Some((cmd, x.value, x.ok)) } else { None })
        .collect();
    assert_eq!(got, vec![(true, 0x0c21, true), (false, 0xbeef, true), (false, 0, true), (false, 0xffff, true)]);
}

/// A word with the command sync is a command or a status word by its place in the message: the formats of the
/// standard, with the response time before a status word and no time between the words of one sender.
#[test]
fn mil1553_tells_commands_and_status_words_apart() {
    use oos_dsp::decode::{MIL1553_COMMAND as C, MIL1553_COMMAND_2 as C2, MIL1553_STATUS as S};
    let roles = |words: &[(bool, u16, f64)]| -> Vec<Option<u32>> {
        let l = mil1553_message(words, 7.0);
        let dt = 1e-6 / 50.0;
        let us = words.iter().map(|w| 20.0 + w.2).sum::<f64>() + 14.0;
        let v = sample(|t| l.v(t), 0.0, dt, (us * 50.0) as usize);
        let p = mil1553(&v, dt, 0.0, 1.0);
        assert_eq!(p.iter().map(|x| x.value).collect::<Vec<_>>(), words.iter().map(|w| w.1 as u32).collect::<Vec<_>>());
        p.iter().map(|x| x.value2).collect()
    };
    let cmd = |rt: u16, transmit: bool, sa: u16, n: u16| rt << 11 | (transmit as u16) << 10 | sa << 5 | n;
    let st = |rt: u16| rt << 11 | 1;
    let (word, data) = (|v: u16, gap: f64| (true, v, gap), |v: u16, gap: f64| (false, v, gap));
    // receive: the data at once, the status after the response time. Transmit: the status, then the data
    let receive = [word(cmd(17, false, 2, 2), 0.0), data(5, 0.0), data(6, 7.0), word(st(17), 30.0)];
    let transmit = [word(cmd(17, true, 2, 2), 7.0), word(st(17), 0.0), data(8, 0.0), data(9, 30.0)];
    let both: Vec<_> = receive.iter().chain(&transmit).cloned().collect();
    assert_eq!(roles(&both), [Some(C), None, None, Some(S), Some(C), Some(S), None, None]);
    // the next command 5 us after a message that is complete is no status word of it
    let close = [
        word(cmd(17, false, 2, 1), 0.0),
        data(5, 7.0),
        word(st(17), 5.0),
        word(cmd(18, true, 1, 1), 7.0),
        word(st(18), 0.0),
        data(1, 30.0),
    ];
    assert_eq!(roles(&close), [Some(C), None, Some(S), Some(C), Some(S), None]);
    // between two terminals: two commands at once, the status and the data of the sender, the receiver's status
    let two = [
        word(cmd(3, false, 1, 1), 0.0),
        word(cmd(4, true, 1, 1), 7.0),
        word(st(4), 0.0),
        data(0xaa55, 7.0),
        word(st(3), 30.0),
    ];
    assert_eq!(roles(&two), [Some(C), Some(C2), Some(S), None, Some(S)]);
    // a mode command without a data word (mode code 2) and one with (18)
    let mode = [
        word(cmd(17, true, 0, 2), 7.0),
        word(st(17), 30.0),
        word(cmd(17, true, 31, 18), 7.0),
        word(st(17), 0.0),
        data(0x8842, 30.0),
    ];
    assert_eq!(roles(&mode), [Some(C), Some(S), Some(C), Some(S), None]);
    // a broadcast is not answered: the word 7 us after it is the next command. A terminal that does not answer
    // within 14 us: the next word is a command too
    let quiet = [
        word(cmd(31, false, 2, 1), 0.0),
        data(5, 7.0),
        word(cmd(17, true, 2, 1), 30.0),
        word(cmd(17, true, 2, 1), 7.0),
        word(st(17), 0.0),
        data(1, 30.0),
    ];
    assert_eq!(roles(&quiet), [Some(C), None, Some(C), Some(C), Some(S), None]);
    // a record that begins inside a message: its data words stay as they are
    let inside = [data(8, 7.0), word(st(17), 30.0), word(cmd(17, true, 0, 2), 7.0), word(st(17), 30.0)];
    assert_eq!(roles(&inside), [None, Some(C), Some(C), Some(S)]);
}

/// A bus a few percent off its rate is read: the decoder follows the edge in the middle of every bit. On the 1 us
/// grid of the edge in the sync the last bits of such a word were read in the wrong half.
#[test]
fn mil1553_follows_a_bus_off_its_rate() {
    let words = [(true, 0x8bf2u16), (false, 0x0008), (true, 0x9001), (false, 0x0005), (true, 0x8842), (false, 0xffff)];
    let l = mil1553_wave(&words, 2.3, 9.0);
    let dt = 1e-6 / 50.0;
    let mut v = sample(|t| l.v(t), 0.0, dt, 50 * 180);
    add_noise(&mut v, 0.1, 5);
    let want: Vec<(bool, u32, bool)> = words.iter().map(|&(cmd, w)| (cmd, w as u32, true)).collect();
    // the same samples with a sample interval 4 % shorter and longer: a bus 4 % slow and 4 % fast
    for off in [1.0, 0.96, 1.04, 0.95, 1.05] {
        let p = mil1553(&v, dt * off, 0.0, 0.3);
        let word = |x: &oos_dsp::decode::Packet| match x.kind {
            Kind::Word { cmd } => Some((cmd, x.value, x.ok)),
            _ => None,
        };
        let got: Vec<(bool, u32, bool)> = p.iter().filter_map(word).collect();
        assert_eq!(got, want, "sample interval times {off}");
    }
}

#[test]
fn mil1553_bad_parity_is_flagged() {
    let l = mil1553_wave(&[(false, 0x1234)], 7.0, 4.0);
    let dt = 1e-6 / 50.0;
    let mut v = sample(|t| l.v(t), 0.0, dt, 50 * 30);
    // swap the halves of the parity bit (bit 17 starts 4 + 3 + 16 us after t = 0)
    let t = 4.0 + 3.0 + 16.0;
    let (a, b) = ((t * 50.0) as usize, ((t + 1.0) * 50.0) as usize);
    for x in &mut v[a..b] {
        *x = -*x;
    }
    let p = mil1553(&v, dt, 0.0, 1.0);
    assert_eq!(p.len(), 1);
    assert!(!p[0].ok);
    assert_eq!(p[0].value, 0x1234);
}
