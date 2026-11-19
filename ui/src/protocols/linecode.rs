// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Line codes on one line: Manchester (IEEE 802.3: a rising mid-bit edge is a 1) and MLT-3 (a change of level is a 1),
//! at the baud rate of the option row. Built in until OOS 0.1.12, modules since; their keys are the names of the old
//! setups, and a setup that kept the rate in the shared Baud row brings it along (`legacy`).

use super::*;
use oos_dsp::decode::Kind;

/// The baud rate of option row 0.
fn baud(opt: &[usize; OPTS]) -> f64 {
    crate::decode::BAUDS[opt[0].min(crate::decode::BAUDS.len() - 1)]
}

const OPTIONS: [Opt; 1] = [Opt { name: "Baud", choices: &BAUD_NAMES, default: BAUD_DEFAULT }];

/// The rates of the Manchester decoder: those of the list, and Auto, the rate of the signal itself. A Manchester
/// line carries its clock (an edge in the middle of every bit), so its rate need not be one of a list: a bus at
/// 125 kbit/s had no entry. Auto is last, so the entries of a set-up from before mean what they meant, and it is what
/// a new decoder starts with.
const MANCHESTER_RATES: [&str; 11] =
    ["1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "460800", "921600", "Auto"];
const MANCHESTER_AUTO: usize = MANCHESTER_RATES.len() - 1;
const MANCHESTER_OPTIONS: [Opt; 4] = [
    Opt { name: "Baud", choices: &MANCHESTER_RATES, default: MANCHESTER_AUTO },
    // which edge in the middle of a bit is a one: rising (IEEE 802.3, Ethernet) or falling (G. E. Thomas), the
    // two conventions in use
    Opt { name: "A one is", choices: &["Rising (802.3)", "Falling (Thomas)"], default: 0 },
    // the fields of a packet (a line that is quiet between its packets): so many bits of sync, so many of header,
    // then words of Data bits, and what is left is the trailer. Both at 0: a packet is not cut
    Opt { name: "Sync bits", choices: &FIELD_BITS, default: 0 },
    Opt { name: "Header bits", choices: &FIELD_BITS, default: 0 },
];
/// The widths a sync or a header of a Manchester packet can have.
const FIELD_BITS: [&str; 17] =
    ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "16"];

/// A Manchester packet cut into its fields: `sync` bits, `header` bits, words of `word` bits, and the bits that fill
/// no word as the trailer. A field is its bits with the first one sent as the least significant, as a byte of the
/// line is. `bits`: the middle of each bit (a sample index) and its value; `half`: half a bit time in samples.
/// The shortest Ethernet frame, from its destination address to its check.
const ETHERNET_MIN: usize = 64;

/// A packet that is an Ethernet frame (10BASE-T), from its `bytes`: a preamble of 0x55 and the start delimiter 0xD5,
/// then at least the two addresses and the type. Its parts in place of the bytes that hold them: the preamble with
/// its delimiter (value = how many bytes), the destination and the source address, the type, the bytes behind it
/// as they are, and the frame check as the last four bytes where the packet is `whole` (the record has its end)
/// and long enough for a frame: `ok` where the CRC-32 over the frame is that check. None where the packet is no
/// such frame (and with the other convention for a one it is none: every byte is the other one).
///
/// Seen on a unit: a board that sends a ping and frames changed on purpose, which differ in a few header bytes and
/// have a check that does not fit; as bytes alone the two looked alike.
fn ethernet_frame(bytes: &[Packet], whole: bool) -> Option<Vec<Packet>> {
    let sfd = bytes.iter().position(|b| b.value != 0x55).filter(|&i| (1..=8).contains(&i) && bytes[i].value == 0xd5)?;
    let frame = &bytes[sfd + 1..];
    if frame.len() < 14 {
        return None;
    }
    let be = |r: std::ops::Range<usize>| frame[r].iter().fold(0u32, |v, b| v << 8 | b.value & 0xff);
    let span = |kind, of: &[Packet], value: u32, value2: Option<u32>, ok: bool| {
        let (t0, t1) = (of[0].t0, of[of.len() - 1].t1);
        Packet { t0, t1, kind, value, value2, ok }
    };
    let mut out = vec![
        span(Kind::Field("PRE"), &bytes[..=sfd], sfd as u32 + 1, None, true),
        span(Kind::Field("MAC"), &frame[..6], be(0..4), Some(be(4..6)), true),
        span(Kind::Field("SRC"), &frame[6..12], be(6..10), Some(be(10..12)), true),
        span(Kind::Field("TYPE"), &frame[12..14], be(12..14), None, true),
    ];
    let checked = whole && frame.len() >= ETHERNET_MIN;
    let end = frame.len() - if checked { 4 } else { 0 };
    out.extend(frame[14..end].iter().cloned());
    if checked {
        let raw: Vec<u8> = frame.iter().map(|b| b.value as u8).collect();
        // the check is sent with its least significant byte first
        let sent = u32::from_le_bytes([raw[end], raw[end + 1], raw[end + 2], raw[end + 3]]);
        out.push(span(Kind::Crc, &frame[end..], sent, None, oos_dsp::decode_ext2::usbpd_crc32(&raw[..end]) == sent));
    }
    Some(out)
}

fn manchester_fields(bits: &[(f64, bool)], half: f64, sync: usize, header: usize, word: usize) -> Vec<Packet> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut take = |kind: Kind, n: usize, counted: bool, out: &mut Vec<Packet>| {
        let n = n.min(bits.len() - at);
        if n == 0 {
            return;
        }
        let f = &bits[at..at + n];
        let value = f.iter().enumerate().fold(0u32, |v, (k, b)| v | (b.1 as u32) << k);
        let value2 = counted.then_some(n as u32);
        out.push(Packet { t0: f[0].0 - half, t1: f[n - 1].0 + half, kind, value, value2, ok: true });
        at += n;
    };
    take(Kind::Field("SYNC"), sync, false, &mut out);
    take(Kind::Field("HDR"), header, false, &mut out);
    let word = word.clamp(1, 32);
    let words = bits.len().saturating_sub(sync + header) / word;
    (0..words).for_each(|_| take(Kind::Data, word, false, &mut out));
    take(Kind::Field("TRL"), word - 1, true, &mut out);
    out
}

/// The bit rate of the Manchester option row; 0 = from the signal (`oos_dsp::linecode::manchester`).
fn manchester_rate(opt: &[usize; OPTS]) -> f64 {
    if opt[0] >= MANCHESTER_AUTO { 0.0 } else { baud(opt) }
}

/// What the demo lines of this file carry.
const DEMO_BYTES: [u8; 6] = [0xa5, 0x3c, 0x0f, 0x99, 0x5a, 0xc3];

pub struct Manchester;

impl Protocol for Manchester {
    fn key(&self) -> &'static str {
        "Manchester"
    }
    fn name(&self) -> &'static str {
        "Manchester"
    }
    fn options(&self) -> &'static [Opt] {
        &MANCHESTER_OPTIONS
    }
    /// (With Auto the rate is not known before a record is read: the bit time of the list's usual entry.)
    fn bit_time(&self, opt: &[usize; OPTS]) -> f64 {
        let rate = manchester_rate(opt);
        1.0 / if rate > 0.0 { rate } else { crate::decode::BAUDS[BAUD_DEFAULT] }
    }
    /// Six bytes at the rate of the list's usual entry, least significant bit first, a one rising in the middle of
    /// its bit, 0 to 3.3 V, with a pause of 24 bit times between the rounds.
    fn demo(&self) -> Option<Demo> {
        let half = 0.5 / crate::decode::BAUDS[BAUD_DEFAULT];
        let bits = DEMO_BYTES.iter().flat_map(|b| (0..8).map(move |i| b >> i & 1 == 1));
        let halves: Vec<bool> = bits.flat_map(|one| [!one, one]).collect();
        let period = (halves.len() + 16 + 48) as f64 * half;
        let line = oos_dsp::sim::bits_logic(&halves, half, 16, 0.0, 3.3);
        Some(Demo { period, lines: [Some(Box::new(move |t| line.v(t))), None, None, None] })
    }
    /// Data bits is the width of a word of a packet that is cut into fields.
    fn shared(&self) -> &'static [Row] {
        &[Row::Bits]
    }
    /// A packet of a line that is quiet between its packets is a message: it begins with its mark, as an SPI frame
    /// does with its chip select.
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        Some(|p| p.kind == Kind::Start)
    }
    /// The mark where a packet begins is no field of the line.
    fn mark(&self, p: &Packet) -> bool {
        p.kind == Kind::Start
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        let (rate, other) = (manchester_rate(&l.opt), l.opt[1] == 1);
        let (packets, half) = oos_dsp::linecode::manchester_packets(l.main(), l.dt, rate, l.thr, l.hy);
        if packets.len() <= 1 {
            // a line without a pause: one stream of bytes, with the fields of an Ethernet frame where it has them
            let p = oos_dsp::linecode::manchester(l.main(), l.dt, rate, l.thr, l.hy);
            if !other {
                return Ok(p);
            }
            // the other convention: every bit is the other one. The bytes alone: the fields of an Ethernet frame
            // the decoder finds are those of 802.3
            let data = p.into_iter().filter(|x| x.kind == Kind::Data);
            return Ok(data.map(|x| Packet { value: !x.value & 0xff, ..x }).collect());
        }
        // a line that is quiet between its packets: each packet from its mark, cut into fields where the settings
        // say how, else as its bytes, or as its bits where they are no whole bytes
        let (sync, header, word) = (l.opt[2], l.opt[3], l.sh.bits as usize);
        let mut out = Vec::new();
        for bits in packets.iter().filter(|b| !b.is_empty()) {
            let bits: Vec<(f64, bool)> = bits.iter().map(|&(t, one)| (t, one != other)).collect();
            let (n, t0) = (bits.len(), bits[0].0 - half);
            out.push(Packet { t0, t1: t0, kind: Kind::Start, value: 0, value2: None, ok: true });
            if sync + header > 0 {
                out.extend(manchester_fields(&bits, half, sync, header, word));
            } else if n > 32 || n % 8 == 0 {
                let bytes = manchester_fields(&bits[..n - n % 8], half, 0, 0, 8);
                // (whole: the pause that ends the packet is in the record)
                let whole = bits[n - 1].0 + 4.0 * half < l.main().len() as f64 - 1.0;
                out.extend(ethernet_frame(&bytes, whole).unwrap_or(bytes));
            } else {
                let value = bits.iter().fold(0u32, |v, b| v << 1 | b.1 as u32);
                let (kind, t1) = (Kind::Field("PKT"), bits[n - 1].0 + half);
                out.push(Packet { t0, t1, kind, value, value2: Some(n as u32), ok: true });
            }
        }
        Ok(out)
    }
    /// A packet that is no whole number of bytes: its bits in the order they were sent ("0011100111101"), which
    /// says nothing of where its fields end; over 16 bits as one number with the first bit the most significant,
    /// and how many they are ("1F2A7 (17 b)").
    fn text(&self, p: &Packet, _: &[usize; OPTS]) -> Option<String> {
        // (the trailer of a packet cut into fields has its number of bits beside its value, which is not said)
        if p.kind == Kind::Field("TRL") {
            return Some(format!("TRL {:X}", p.value));
        }
        let n = p.value2.filter(|_| p.kind == Kind::Field("PKT"))? as usize;
        Some(if n <= 16 {
            format!("{:0n$b}", p.value)
        } else {
            format!("{:0w$X} ({} b)", p.value, n, w = n.div_ceil(4))
        })
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.baud, 0, 0, 0])
    }
}

pub struct Mlt3;

impl Protocol for Mlt3 {
    fn key(&self) -> &'static str {
        "Mlt3"
    }
    fn name(&self) -> &'static str {
        "MLT-3"
    }
    fn options(&self) -> &'static [Opt] {
        &OPTIONS
    }
    fn bit_time(&self, opt: &[usize; OPTS]) -> f64 {
        1.0 / baud(opt)
    }
    fn decode(&self, l: &Lines) -> Result<Vec<Packet>, &'static str> {
        Ok(oos_dsp::linecode::mlt3(l.main(), l.dt, baud(&l.opt), l.thr, l.hy))
    }
    /// The line has no frames: every byte starts one, as for the UART (Trigger > Bus > Frame never fired).
    fn trigger_match(&self, what: BusWhat, _value: u32, p: &Packet) -> Option<bool> {
        (what == BusWhat::Frame && p.kind == Kind::Data).then_some(true)
    }
    /// The same six bytes at the rate of the list's usual entry: a one steps the level on through 0, +1, 0, -1 V,
    /// a zero holds it; 16 bit times of quiet line before them and 24 after.
    fn demo(&self) -> Option<Demo> {
        let bit = 1.0 / crate::decode::BAUDS[BAUD_DEFAULT];
        let mut line = oos_dsp::sim::Levels { seg: vec![(0.0, 0.0)], tr: bit / 20.0 };
        let (mut at, mut t) = (0usize, 16.0 * bit);
        for one in DEMO_BYTES.iter().flat_map(|b| (0..8).map(move |i| b >> i & 1 == 1)) {
            if one {
                at += 1;
                line.seg.push((t, [0.0, 1.0, 0.0, -1.0][at % 4]));
            }
            t += bit;
        }
        // (back to where a round begins: the next round is the same picture)
        line.seg.push((t, 0.0));
        Some(Demo { period: t + 24.0 * bit, lines: [Some(Box::new(move |t| line.v(t))), None, None, None] })
    }
    fn legacy(&self, old: &Legacy) -> Option<[usize; OPTS]> {
        Some([old.baud, 0, 0, 0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Break it catches: a setup written while the line codes were built in losing its protocol or its baud rate
    #[test]
    fn old_setups_keep_protocol_and_baud() {
        for (key, name) in [("Manchester", "Manchester"), ("Mlt3", "MLT-3")] {
            let m = crate::store::parse_setup(&format!("dec_on = true\ndec_proto = {}\ndec_baud = 3\n", key)).unwrap();
            assert_eq!(m.dec_proto.name(), name);
            assert!(m.dec_on && m.dec_opt[0] == 3, "{} {:?}", key, m.dec_opt);
            assert_eq!(m.bus_bit(), 1.0 / 9600.0);
        }
    }

    /// A packet cut into fields by the settings: sync, header, words and the trailer, each with its first bit
    /// sent as the least significant. The five packets are those of a capture of a real line, and the values
    /// those its instrument lists for them.
    #[test]
    fn a_manchester_packet_is_cut_into_its_fields() {
        let cut = |wire: &str| -> Vec<(Kind, u32)> {
            let bit = |(k, b): (usize, u8)| (16.0 * k as f64 + 8.0, b == b'1');
            let bits: Vec<(f64, bool)> = wire.bytes().enumerate().map(bit).collect();
            manchester_fields(&bits, 8.0, 2, 3, 6).iter().map(|p| (p.kind, p.value)).collect()
        };
        let (sync, hdr, trl) = (Kind::Field("SYNC"), Kind::Field("HDR"), Kind::Field("TRL"));
        let rows = [
            ("0011100111101", 7, 0x3c, 2),
            ("0001100111101", 6, 0x3c, 2),
            ("0001100111011", 6, 0x1c, 3),
            ("0011100111011", 7, 0x1c, 3),
            ("0001100101101", 6, 0x34, 2),
        ];
        for (wire, header, data, trailer) in rows {
            assert_eq!(cut(wire), [(sync, 0), (hdr, header), (Kind::Data, data), (trl, trailer)], "{wire}");
        }
        // where a field lies: the header from the third bit to the fifth
        let bits: Vec<(f64, bool)> = (0..13).map(|k| (16.0 * k as f64 + 8.0, true)).collect();
        let p = manchester_fields(&bits, 8.0, 2, 3, 6);
        assert_eq!((p[0].t0, p[1].t0, p[1].t1), (0.0, 32.0, 80.0));
        // two words and no trailer; a packet shorter than its sync and header
        let kinds = |n: usize, word: usize| -> Vec<Kind> {
            manchester_fields(&bits[..n], 8.0, 2, 3, word).iter().map(|p| p.kind).collect()
        };
        assert_eq!(kinds(13, 4), [sync, hdr, Kind::Data, Kind::Data]);
        assert_eq!(kinds(4, 6), [sync, hdr]);
        assert_eq!(Manchester.text(&p[3], &[0; OPTS]).as_deref(), Some("TRL 3"));
    }

    /// A line of packets through the module: each packet begins with its mark; as its bits where nothing says how
    /// to cut it, as its fields with the settings, and with the other convention every bit the other one.
    #[test]
    fn a_line_of_manchester_packets_through_the_module() {
        let wires = ["0011100111101", "0001100111011"];
        // 16 samples a bit; a one rising in the middle; 2.5 bit times of quiet line after a packet
        let mut v = vec![0.0f32; 48];
        for wire in wires {
            for b in wire.bytes() {
                let one = b == b'1';
                v.extend(std::iter::repeat_n(if one { 0.0 } else { 1.0 }, 8));
                v.extend(std::iter::repeat_n(if one { 1.0 } else { 0.0 }, 8));
            }
            v.extend(std::iter::repeat_n(0.0, 40));
        }
        let said = |opt: [usize; OPTS], bits: u32| -> String {
            let sh = Shared { bits, ..Shared::default() };
            let lines = [Some(&v[..]), None, None, None];
            let l = Lines { lines, dt: 1.0, thr: 0.5, hy: 0.05, opt, sh, chans: [None; 4], src: [None; 4] };
            let p = Manchester.decode(&l).unwrap();
            let text = |x: &Packet| match (x.kind, Manchester.text(x, &opt)) {
                (Kind::Start, _) => "|".to_string(),
                (_, Some(t)) => t,
                (Kind::Field(l), None) => format!("{l} {:X}", x.value),
                _ => format!("{:02X}", x.value),
            };
            p.iter().map(text).collect::<Vec<_>>().join(" ")
        };
        let auto = MANCHESTER_AUTO;
        assert_eq!(said([auto, 0, 0, 0], 8), "| 0011100111101 | 0001100111011");
        assert_eq!(said([auto, 0, 2, 3], 6), "| SYNC 0 HDR 7 3C TRL 2 | SYNC 0 HDR 6 1C TRL 3");
        assert_eq!(said([auto, 1, 0, 0], 8), "| 1100011000010 | 1110011000100");
        let p = Manchester.frame_start().unwrap();
        let start = Packet { t0: 0.0, t1: 0.0, kind: Kind::Start, value: 0, value2: None, ok: true };
        assert!(p(&start) && Manchester.mark(&start));
    }

    /// Packets of 10BASE-T: an Ethernet frame is its preamble, its addresses, its type, its bytes and its check, and
    /// a frame with a byte changed has a check that does not fit. A frame that the record cuts off has no check,
    /// and with the other convention for a one the packet is its bytes.
    #[test]
    fn an_ethernet_frame_in_manchester_packets_has_its_fields_and_its_check() {
        // (addresses of the kind a station gives itself, none of a real one)
        let mut frame: Vec<u8> = vec![0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x02];
        frame.extend([0x08, 0x00]);
        frame.extend((0..46u8).map(|i| i.wrapping_mul(7)));
        let fcs = oos_dsp::decode_ext2::usbpd_crc32(&frame);
        frame.extend(fcs.to_le_bytes());
        let mut changed = frame.clone();
        changed[14] ^= 0x02;
        // 16 samples a bit, a one rising in the middle, the first bit of a byte the least significant; 4 bit times
        // of quiet line between the packets
        let mut v = vec![0.0f32; 64];
        for f in [&frame, &changed, &frame] {
            let bytes = [0x55u8; 7].iter().chain([0xd5u8].iter()).chain(f.iter());
            for one in bytes.flat_map(|b| (0..8).map(move |k| b >> k & 1 == 1)) {
                v.extend(std::iter::repeat_n(if one { 0.0 } else { 1.0 }, 8));
                v.extend(std::iter::repeat_n(if one { 1.0 } else { 0.0 }, 8));
            }
            v.extend(std::iter::repeat_n(0.0, 64));
        }
        // the record ends in the third frame
        v.truncate(v.len() - 64 - 20 * 16);
        let decode = |opt: [usize; OPTS]| {
            let lines = [Some(&v[..]), None, None, None];
            let sh = Shared::default();
            let l = Lines { lines, dt: 1.0, thr: 0.5, hy: 0.05, opt, sh, chans: [None; 4], src: [None; 4] };
            Manchester.decode(&l).unwrap()
        };
        let p = decode([MANCHESTER_AUTO, 0, 0, 0]);
        let frames = oos_dsp::decode::frames(&p, |x| x.kind == Kind::Start);
        assert_eq!(frames.len(), 3);
        let kinds = |r: &std::ops::Range<usize>| -> Vec<Kind> {
            p[r.clone()].iter().filter(|x| x.kind != Kind::Data).map(|x| x.kind).collect()
        };
        let (pre, mac, src, ty) = (Kind::Field("PRE"), Kind::Field("MAC"), Kind::Field("SRC"), Kind::Field("TYPE"));
        let whole = vec![Kind::Start, pre, mac, src, ty, Kind::Crc];
        assert_eq!(kinds(&frames[0]), whole);
        assert_eq!(kinds(&frames[1]), whole);
        assert_eq!(kinds(&frames[2]), whole[..5], "a frame the record cuts off has no check");
        let field = |r: &std::ops::Range<usize>, kind: Kind| -> Packet {
            p[r.clone()].iter().find(|x| x.kind == kind).cloned().unwrap()
        };
        let (to, from) = (field(&frames[0], mac), field(&frames[0], src));
        assert_eq!((to.value, to.value2, from.value, from.value2), (0x0200_0000, Some(1), 0x0200_0000, Some(2)));
        assert_eq!((field(&frames[0], pre).value, field(&frames[0], ty).value), (8, 0x0800));
        // 46 bytes behind the type, the check with its value, right in the first frame and wrong in the second
        let data = |r: &std::ops::Range<usize>| p[r.clone()].iter().filter(|x| x.kind == Kind::Data).count();
        assert_eq!((data(&frames[0]), data(&frames[1])), (46, 46));
        let (first, second) = (field(&frames[0], Kind::Crc), field(&frames[1], Kind::Crc));
        assert_eq!((first.value, first.ok, second.value, second.ok), (fcs, true, fcs, false));
        // every part lies where its bytes are: the fields of a frame follow each other without a gap
        for w in p[frames[0].clone()].windows(2).skip(1) {
            assert!((w[1].t0 - w[0].t1).abs() < 1.0, "{:?} {:?}", w[0], w[1]);
        }
        // the other convention: no frame, the bytes
        let other = decode([MANCHESTER_AUTO, 1, 0, 0]);
        assert!(other.iter().all(|x| matches!(x.kind, Kind::Start | Kind::Data)));
        assert_eq!(other.iter().find(|x| x.kind == Kind::Data).map(|x| x.value), Some(0xaa));
    }

    /// A packet that is no whole number of bytes as its text.
    #[test]
    fn a_manchester_packet_is_one_value_with_its_bits() {
        let pkt = Packet { t0: 0.0, t1: 1.0, kind: Kind::Field("PKT"), value: 0x73d, value2: Some(13), ok: true };
        let text = |p: Packet| Manchester.text(&p, &[0; OPTS]);
        assert_eq!(text(pkt.clone()).as_deref(), Some("0011100111101"));
        assert_eq!(text(Packet { value: 5, value2: Some(3), ..pkt.clone() }).as_deref(), Some("101"));
        assert_eq!(text(Packet { value: 0x1f2a7, value2: Some(17), ..pkt.clone() }).as_deref(), Some("1F2A7 (17 b)"));
        assert_eq!(text(Packet { kind: Kind::Data, value2: None, ..pkt }), None);
    }

    /// The demo lines of Manchester and MLT-3 carry the six bytes, and a new decoder of each reads them.
    #[test]
    fn the_demo_lines_carry_their_bytes() {
        let modules: [&dyn Protocol; 2] = [&Manchester, &Mlt3];
        for m in modules {
            let demo = m.demo().unwrap();
            let opt: [usize; OPTS] = std::array::from_fn(|k| m.options().get(k).map_or(0, |o| o.default));
            let dt = m.bit_time(&opt) / 20.0;
            let line = demo.lines[0].as_ref().unwrap();
            let n = (2.0 * demo.period / dt) as usize;
            let v: Vec<f32> = (0..n).map(|i| line((i as f64 * dt) % demo.period) as f32).collect();
            let (lines, sh) = ([Some(&v[..]), None, None, None], Shared::default());
            let thr = if m.key() == "Mlt3" { 0.0 } else { 1.65 };
            let l = Lines { lines, dt, thr, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            let data = m.decode(&l).unwrap().into_iter().filter(|x| x.kind == Kind::Data);
            let got: Vec<u32> = data.map(|x| x.value).collect();
            let want: Vec<u32> = DEMO_BYTES.iter().map(|&b| b as u32).collect();
            assert!(got.len() >= 6 && got[..6] == want[..], "{}: {:x?}", m.name(), got);
        }
    }

    /// The Manchester decoder takes the rate of the signal with Auto, which a new decoder has: a line at 125 kbit/s,
    /// which the list does not hold, is read; and with the other convention every bit is the other one.
    #[test]
    fn manchester_reads_a_rate_that_is_not_in_the_list() {
        let m = &Manchester;
        assert_eq!(m.options()[0].choices[m.options()[0].default], "Auto");
        // the bytes A5 3C 0F 99 as bits LSB first, a one rising in the middle of its bit (two half bits each)
        let bytes = [0xa5u8, 0x3c, 0x0f, 0x99, 0x5a, 0xc3];
        let bits = bytes.iter().flat_map(|b| (0..8).map(move |i| b >> i & 1 == 1));
        let halves: Vec<bool> = bits.flat_map(|one| [!one, one]).collect();
        let half = 0.5 / 125e3;
        let dt = half / 20.0;
        let wave = oos_dsp::sim::bits_logic(&halves, half, 8, 0.0, 3.3);
        let v = oos_dsp::sim::sample(|t| wave.v(t), 0.0, dt, 20 * (halves.len() + 20));
        let values = |opt: [usize; OPTS]| -> Vec<u32> {
            let (lines, sh) = ([Some(&v[..]), None, None, None], Shared::default());
            let l = Lines { lines, dt, thr: 1.65, hy: 0.2, opt, sh, chans: [None; 4], src: [None; 4] };
            m.decode(&l).unwrap().iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect()
        };
        let auto = values([MANCHESTER_AUTO, 0, 0, 0]);
        assert!(auto.len() >= 4, "{auto:02X?}");
        let other = values([MANCHESTER_AUTO, 1, 0, 0]);
        assert_eq!(other, auto.iter().map(|v| !v & 0xff).collect::<Vec<_>>());
        // a rate that is set and 8 % off still reads the line: the clock follows the edge in the middle of every
        // bit. At less than half the rate of the line it does not
        assert_eq!(values([7, 0, 0, 0]), auto, "115200 is 8 % off");
        assert_ne!(values([6, 0, 0, 0]), auto, "57600");
    }

    #[test]
    fn baud_names_are_the_bauds() {
        for (n, b) in BAUD_NAMES.iter().zip(crate::decode::BAUDS) {
            assert_eq!(n.parse::<f64>().unwrap(), b);
        }
    }
}
