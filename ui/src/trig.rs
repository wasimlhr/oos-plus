// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The software trigger of all sources: the trigger types of oos-dsp, and the protocol trigger (TrigType::Bus), which
//! decodes the record with the Decode menu's settings and triggers at the end of the matching field.

use crate::model::*;
use crate::protocols::Decoder;
use oos_dsp::decode::{Kind, Packet};
use oos_dsp::trigger::Input;

/// Time scale of the trigger condition of `m` (s): the search resolution must resolve it.
pub fn feature(m: &Model) -> f64 {
    let t = |x: f32| x as f64;
    match m.trig_type {
        TrigType::Edge | TrigType::Pattern => f64::INFINITY,
        TrigType::Bus => m.bus_bit() / 20.0,
        TrigType::Video => 0.25e-6,
        TrigType::SetupHold => t(m.setup.min(m.hold)) / 4.0,
        TrigType::Timeout => t(m.trig_t1) / 20.0,
        TrigType::NthEdge => t(m.nth_idle) / 20.0,
        TrigType::Runt if m.trig_when == WhenKind::Off => f64::INFINITY,
        TrigType::Window if m.win_mode != WinMode::Inside => f64::INFINITY,
        _ => {
            t(m.trig_t1.min(if matches!(m.trig_when, WhenKind::In | WhenKind::Out) { m.trig_t2 } else { f32::MAX }))
                / 20.0
        }
    }
}

/// Record length the trigger of `m` needs to decide (s).
pub fn span(m: &Model) -> f64 {
    let t = |x: f32| x as f64;
    match m.trig_type {
        TrigType::Video => match m.video_sync {
            VSync::AllLines => 4.0 * m.video_std.line(),
            _ => 1.1 * m.video_std.lines() as f64 * m.video_std.line(),
        },
        TrigType::Timeout | TrigType::Duration => 3.0 * t(m.trig_t1.max(m.trig_t2)) + 6e-3,
        TrigType::NthEdge => 4.0 * t(m.nth_idle) + 2e-3,
        // room for a whole frame of the protocol before the trigger point (a module may say: USB auto, low-speed packets)
        TrigType::Bus => m.bus_frame_span(),
        TrigType::Edge | TrigType::Pattern => 0.0,
        _ => 3.0 * t(m.trig_t1.max(m.trig_t2)) + 3e-3,
    }
}

/// Packets of the decoder settings in `m` on the channels of `inp` (times = fractional sample indices).
pub fn decode(m: &Model, ch: [&[f32]; NCH], dt: f64) -> Result<Vec<Packet>, &'static str> {
    if m.dec_src >= NCH {
        return Err("stored decoder inputs need the reference adapter");
    }
    decode_at(m, ch, dt, m.dec_thr, 0.1 * m.ch[m.dec_src].volts_per_div())
}

/// The mean of every point and its neighbour: the line a decoder reads of points that are minimum and maximum in
/// turn (`Record::alternates`: Peak detect, the pairs of a screen record). Same length; an edge crosses a level once,
/// where the points themselves pass it three times and every bit begins with two pulses of one point.
pub(crate) fn pair_means(v: &[f32]) -> Vec<f32> {
    (0..v.len()).map(|i| (v[i] + v[(i + 1).min(v.len() - 1)]) / 2.0).collect()
}

/// Packets of logic analyser lines (`Model::dec_la`, the decoder's roles source / second / third / fourth line). The
/// lines become 0 / 1 waves in the channel slots 0..3 and the normal decoders run on them, threshold 0.5; the LA's
/// own comparators have already decided the levels. Times = fractional sample indices of the LA record.
pub fn decode_la(m: &Model, words: &[u16], dt: f64) -> Result<Vec<Packet>, &'static str> {
    let wave = |n: u8| -> Vec<f32> { words.iter().map(|w| (w >> n & 1) as f32).collect() };
    let (thr, hy) = (0.5, 0.1);
    let la = m.dec_la;
    if m.dec_proto == Proto::PARALLEL {
        // up to 16 data lines upwards from the main source, skipping the clock line
        let first = la[0].unwrap_or(0);
        let lines: Vec<Vec<f32>> = (0..16u8)
            .map(|k| (first + k) % 16)
            .filter(|&n| Some(n) != la[2])
            .take(m.dec_bits.clamp(1, 16) as usize)
            .map(wave)
            .collect();
        let refs: Vec<&[f32]> = lines.iter().map(|l| &l[..]).collect();
        let clk = la[2].map(wave);
        return Ok(oos_dsp::decode::parallel(&refs, clk.as_deref(), oos_dsp::trigger::Slope::Rising, thr, hy));
    }
    let slots: [Vec<f32>; NCH] = std::array::from_fn(|i| la.get(i).copied().flatten().map_or(Vec::new(), &wave));
    let mut s = m.clone();
    // the LA's comparators have decided the levels: no line has a threshold here
    s.dec_thr_line = [None; 3];
    (s.dec_src, s.dec_src2, s.dec_cs, s.dec_src4, s.dec_diff) = (0, 1, la[2].map(|_| 2), la[3].map(|_| 3), None);
    decode_at(&s, std::array::from_fn(|c| &slots[c][..]), dt, thr, hy)
}

/// `decode` with the threshold and hysteresis given.
pub(crate) fn decode_at(m: &Model, ch: [&[f32]; NCH], dt: f64, thr: f32, hy: f32) -> Result<Vec<Packet>, &'static str> {
    use oos_dsp::decode;
    let get = |c: usize| ch.get(c).copied().filter(|v| !v.is_empty()).ok_or("source channel off");
    let opt = |c: Option<usize>| -> Result<Option<&[f32]>, &'static str> { c.map(get).transpose() };
    // RS-485 / differential: A - B, threshold around 0 V like the pair's receiver
    let diff;
    let line: &[f32] = match (m.dec_proto.diff_ok(), m.dec_diff) {
        (true, Some(b)) => {
            diff = decode::diff(get(m.dec_src)?, get(b)?);
            &diff
        }
        _ => get(m.dec_src)?,
    };
    // the protocol module: the lines of its roles; a role it names must have a source (the third and fourth may be
    // "none")
    let Proto::Module(i) = m.dec_proto;
    let p = crate::protocols::get(i);
    let r = p.info().roles;
    let lines = [
        Some(line),
        if r[1].is_some() { Some(get(m.dec_src2)?) } else { None },
        if r[2].is_some() { opt(m.dec_cs)? } else { None },
        if r[3].is_some() { opt(m.dec_src4)? } else { None },
    ];
    // a line with a threshold of its own is moved so that this threshold lies on the decoder's: the decoders take
    // one threshold for all their lines (the hysteresis stays that of the main line; oos-plus)
    let moved: [Option<Vec<f32>>; 3] = std::array::from_fn(|k| {
        let by = m.dec_thr_line[k]? - m.dec_thr;
        if by == 0.0 {
            return None;
        }
        lines[k + 1].map(|v| v.iter().map(|x| x - by).collect())
    });
    let line = |k: usize| moved[k].as_deref().or(lines[k + 1]);
    let lines = [lines[0], line(0), line(1), line(2)];
    let chans: [Option<&[f32]>; 4] = std::array::from_fn(|c| ch.get(c).copied().filter(|v| !v.is_empty()));
    let src = [Some(m.dec_src), Some(m.dec_src2), m.dec_cs, m.dec_src4];
    let l = crate::protocols::Lines { lines, dt, thr, hy, opt: m.dec_opts(), sh: crate::protocols::shared(m), chans, src };
    let (packets, inverted) = p.info().decode_used(&l)?;
    let key = polarity_key(m);
    if p.info().polarity_of(&l.opt) == oos_dsp::protocols::Polarity::Auto {
        POLARITY_USED.lock().unwrap_or_else(|e| e.into_inner()).insert(key.clone(), inverted);
    }
    // what this decode found out about its line: kept for the band, the Events pane, :DECode:DIAGnosis? and MCP
    let mut kept = DIAGNOSIS.lock().unwrap_or_else(|e| e.into_inner());
    if kept.len() >= DIAGNOSES {
        kept.clear();
    }
    kept.insert(key, oos_dsp::diagnosis::diagnose(p.info(), &l, &packets));
    Ok(packets)
}

/// The decoder settings the diagnosis of the last decode is kept for.
const DIAGNOSES: usize = 32;

/// The diagnosis of the last decode of each decoder setting (`Model::dec_cfg`, as text). A table of its own: decoding
/// runs on several threads.
static DIAGNOSIS: std::sync::Mutex<std::collections::BTreeMap<String, oos_dsp::diagnosis::Diagnosis>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// What the decode of `from` found out and chose is also what the decoder `to` found: a decode that ran on the
/// settings of another model (the reference decoder moves the sources into its own slots) is the decode of `to`.
pub(crate) fn share_findings(from: &Model, to: &Model) {
    let (a, b) = (polarity_key(from), polarity_key(to));
    if a == b {
        return;
    }
    let used = POLARITY_USED.lock().unwrap_or_else(|e| e.into_inner()).get(&a).copied();
    if let Some(u) = used {
        POLARITY_USED.lock().unwrap_or_else(|e| e.into_inner()).insert(b.clone(), u);
    }
    let d = DIAGNOSIS.lock().unwrap_or_else(|e| e.into_inner()).get(&a).cloned();
    if let Some(d) = d {
        DIAGNOSIS.lock().unwrap_or_else(|e| e.into_inner()).insert(b, d);
    }
}

/// What the last decode of the decoder `m` is the view of found out about its line (`oos_dsp::diagnosis`): `None`
/// before one ran.
pub fn diagnosis(m: &Model) -> Option<oos_dsp::diagnosis::Diagnosis> {
    DIAGNOSIS.lock().unwrap_or_else(|e| e.into_inner()).get(&polarity_key(m)).cloned()
}

/// What Polarity Auto chose the last time each decoder setting was decoded: the decoder settings
/// (`Model::dec_cfg`, as text) and whether the line was read inverted. Decoding runs on several threads, hence a
/// table of its own.
static POLARITY_USED: std::sync::Mutex<std::collections::BTreeMap<String, bool>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

fn polarity_key(m: &Model) -> String {
    format!("{:?}", m.dec_cfg())
}

/// Whether the decoder of `m` reads its line inverted: Polarity Inverted, or Auto where the last decode of these
/// settings found the inverted reading better. `false` for a decoder without a polarity row.
pub fn read_inverted(m: &Model) -> bool {
    let Proto::Module(i) = m.dec_proto;
    match crate::protocols::get(i).info().polarity_of(&m.dec_opts()) {
        oos_dsp::protocols::Polarity::Normal => false,
        oos_dsp::protocols::Polarity::Inverted => true,
        oos_dsp::protocols::Polarity::Auto => {
            POLARITY_USED.lock().unwrap_or_else(|e| e.into_inner()).get(&polarity_key(m)).copied().unwrap_or(false)
        }
    }
}

/// Whether a packet satisfies the protocol trigger condition of `m` on its own. UART and SPI data take the relation,
/// limit and mask of `m.ser`; I2C addresses and data need the frame around them (`points`).
pub fn matches(m: &Model, p: &Packet) -> bool {
    let v = m.trig_bus_val;
    if matches!(m.dec_proto, Proto::UART | Proto::SPI) && m.trig_bus == BusWhat::Data {
        return p.kind == Kind::Data && m.ser.rel.test(p.value as u64, v, m.ser.max, m.ser.mask);
    }
    if m.dec_proto == Proto::I2C && matches!(m.trig_bus, BusWhat::Id | BusWhat::Data) {
        return false;
    }
    let Proto::Module(i) = m.dec_proto;
    crate::protocols::get(i).info().trigger_match(m.trig_bus, v as u32, p)
}

/// I2C packets byte by byte, as they are on the bus: the decoder joins the two bytes of a 10-bit address into one
/// packet for the screen (`I2C_TEN_BIT`), and the trigger compares bytes, with the address width of its own setting.
/// The first byte of a joined address ends at its ninth clock of eighteen, taken as evenly spaced.
fn i2c_on_the_bus(packets: &[Packet]) -> Vec<Packet> {
    let mut out = Vec::with_capacity(packets.len());
    for p in packets {
        match p.kind {
            Kind::Addr { read } if p.value2 == Some(oos_dsp::decode::I2C_TEN_BIT) => {
                let high = Packet { value: 0x78 | (p.value >> 8 & 3), value2: None, ..p.clone() };
                if read {
                    out.push(high);
                } else {
                    let step = (p.t1 - p.t0) / 17.0;
                    out.push(Packet { t1: p.t0 + 8.0 * step, ..high });
                    let low = Packet { kind: Kind::Data, value: p.value & 0xff, value2: None, ..p.clone() };
                    out.push(Packet { t0: p.t0 + 9.0 * step, ..low });
                }
            }
            _ => out.push(p.clone()),
        }
    }
    out
}

/// The trigger points (packet ends) of `m`'s protocol condition among `packets`: `matches` per packet, and for I2C
/// the address (relation, direction, 10-bit) and the first `ser.bytes` data bytes after it, big-endian, as the TPU's
/// receiver compares them.
pub fn points(m: &Model, packets: &[Packet]) -> Vec<f64> {
    if m.dec_proto != Proto::I2C || !matches!(m.trig_bus, BusWhat::Id | BusWhat::Data) {
        return packets.iter().filter(|p| matches(m, p)).map(|p| p.t1).collect();
    }
    let bytes = i2c_on_the_bus(packets);
    let packets = &bytes[..];
    let s = &m.ser;
    let mut out = Vec::new();
    // per frame: the address (10-bit: its high bits until the second byte), then the data bytes so far
    let (mut addr, mut hi10, mut read, mut n, mut acc) = (None::<u32>, None::<u32>, false, 0u8, 0u64);
    for p in packets {
        match p.kind {
            Kind::Start | Kind::RepStart | Kind::Stop => (addr, hi10, n, acc) = (None, None, 0, 0),
            Kind::Addr { read: r } => {
                read = r;
                (n, acc) = (0, 0);
                if s.ten_bit && p.value & 0x7c == 0x78 {
                    (addr, hi10) = (None, Some(p.value & 3));
                } else {
                    (addr, hi10) = (Some(p.value), None);
                    if m.trig_bus == BusWhat::Id && addr_ok(m, p.value, read) {
                        out.push(p.t1);
                    }
                }
            }
            Kind::Data => {
                if let Some(h) = hi10.take() {
                    let a = h << 8 | (p.value & 0xff);
                    addr = Some(a);
                    if m.trig_bus == BusWhat::Id && addr_ok(m, a, read) {
                        out.push(p.t1);
                    }
                    continue;
                }
                if m.trig_bus != BusWhat::Data || addr.is_none() {
                    continue;
                }
                // any data byte of the frame
                if s.bytes == 0 {
                    if s.rel.test((p.value & 0xff) as u64, m.trig_bus_val, s.max, s.mask & 0xff) {
                        out.push(p.t1);
                    }
                    continue;
                }
                if n >= s.bytes.min(5) {
                    continue;
                }
                acc = acc << 8 | (p.value & 0xff) as u64;
                n += 1;
                let mask = s.mask & ((1u64 << (8 * n as u32)) - 1);
                if n == s.bytes.min(5) && s.rel.test(acc, m.trig_bus_val, s.max, mask) {
                    out.push(p.t1);
                }
            }
            _ => {}
        }
    }
    out
}

/// The I2C address condition: relation against the value and `addr_max`, and the direction.
fn addr_ok(m: &Model, a: u32, read: bool) -> bool {
    let s = &m.ser;
    let dir = match s.dir {
        I2cDir::Either => true,
        I2cDir::Read => read,
        I2cDir::Write => !read,
    };
    dir && s.addr_rel.test(a as u64, m.trig_bus_val, s.addr_max as u64, 0x3ff)
}

/// All trigger points of `m`'s condition in `inp` (fractional sample indices).
pub fn find_all(m: &Model, inp: &Input) -> Vec<f64> {
    if m.trig_type == TrigType::Bus {
        return points(m, &decode(m, inp.ch, inp.dt).unwrap_or_default());
    }
    oos_dsp::trigger::find_all(&m.trigger(), inp)
}

/// The first trigger point after `from`.
pub fn find_first(m: &Model, inp: &Input, from: f64) -> Option<f64> {
    find_all(m, inp).into_iter().find(|&t| t > from)
}

#[cfg(test)]
pub(crate) mod peak_tests {
    use super::*;

    /// A LIN frame (identifier 0x21, four bytes) as a Peak detect record gives it: points 0.4 us apart, each the
    /// largest or the smallest of the four point times around it, in turn. With the decoder set for it, at another
    /// rate than the bus has: the frame is read by its sync or not at all.
    pub(crate) fn lin_in_peak_detect() -> (Model, Vec<f32>, f64) {
        let frame = (0x21u8, vec![0x10u8, 0x20, 0x30, 0x40], true);
        let mut line = oos_dsp::sim::lin_logic(&[frame], 19200.0, 0.6, 4.1);
        line.tr = 4e-6;
        let (dt, n) = (0.4e-6, 30_000);
        let fine = |k: i64| line.v(k as f64 * dt / 4.0 - 1e-3) as f32;
        let peak: Vec<f32> = (0..n as i64)
            .map(|i| {
                let span = (4 * (i - 2)..4 * (i + 2)).map(fine);
                if i % 2 == 0 {
                    span.fold(f32::MIN, f32::max)
                } else {
                    span.fold(f32::MAX, f32::min)
                }
            })
            .collect();
        let mut m = Model::default();
        (m.dec_proto, m.dec_src, m.dec_thr) = (Proto::LIN, 0, 2.35);
        m.dec_baud = 0;
        (m, peak, dt)
    }

    /// The data bytes of the packets of `v`, and whether every packet is good.
    pub(crate) fn lin_read(m: &Model, v: &[f32], dt: f64) -> (Vec<u32>, bool) {
        let ch: [&[f32]; NCH] = std::array::from_fn(|c| if c == 0 { v } else { &[][..] });
        let p = decode(m, ch, dt).unwrap();
        (p.iter().filter(|x| x.kind == Kind::Data).map(|x| x.value).collect(), p.iter().all(|x| x.ok))
    }

    /// The bytes of the frame of `lin_in_peak_detect`, with its sync, all good.
    pub(crate) fn lin_right() -> (Vec<u32>, bool) {
        (vec![0x55, 0x10, 0x20, 0x30, 0x40], true)
    }

    /// Break it catches: a decoder read the points of a Peak detect record as samples. They are minimum and maximum
    /// in turn, so an edge of a few points passed the threshold three times (LIN at 2.5 MSa/s: the sync of no frame
    /// measured, every byte wrong; the same line in Normal decoded). The means of neighbours read like the line.
    #[test]
    fn a_peak_detect_record_decodes_like_its_signal() {
        let (m, peak, dt) = lin_in_peak_detect();
        assert_ne!(lin_read(&m, &peak, dt), lin_right(), "the record made here has doubled edges");
        assert_eq!(lin_read(&m, &pair_means(&peak), dt), lin_right());
        assert_eq!(pair_means(&[0.0, 2.0, 2.0]), [1.0, 2.0, 2.0]);
    }
}

#[cfg(test)]
mod serial_tests {
    use super::*;

    fn pk(t: f64, kind: Kind, value: u32) -> Packet {
        Packet { t0: t, t1: t, kind, value, value2: None, ok: true }
    }

    #[test]
    fn relations_mask_and_limits() {
        // (relation, x, value, max, mask) -> match
        let cases = [
            (Rel::Eq, 0x41, 0x41, 0, u64::MAX, true),
            (Rel::Ne, 0x41, 0x41, 0, u64::MAX, false),
            (Rel::Gt, 0x42, 0x41, 0, u64::MAX, true),
            (Rel::Lt, 0x41, 0, 0x41, u64::MAX, false),
            (Rel::In, 0x35, 0x30, 0x39, u64::MAX, true),
            (Rel::Out, 0x35, 0x30, 0x39, u64::MAX, false),
            (Rel::Out, 0x40, 0x30, 0x39, u64::MAX, true),
            // the mask applies to x and both limits
            (Rel::Eq, 0x4f, 0x40, 0, 0xf0, true),
        ];
        for (r, x, v, max, mask, want) in cases {
            assert_eq!(r.test(x, v, max, mask), want, "{:?} {:#x}", r, x);
        }
    }

    #[test]
    fn uart_and_spi_data_take_the_relation() {
        let mut m = Model { dec_proto: Proto::UART, trig_bus: BusWhat::Data, trig_bus_val: 0x30, ..Model::default() };
        m.ser.rel = Rel::In;
        m.ser.max = 0x39;
        let p = [pk(1.0, Kind::Data, 0x2f), pk(2.0, Kind::Data, 0x35), pk(3.0, Kind::Data, 0x3a)];
        assert_eq!(points(&m, &p), [2.0]);
        m.dec_proto = Proto::SPI;
        m.ser.rel = Rel::Ne;
        assert_eq!(points(&m, &p), [1.0, 2.0, 3.0]);
    }

    #[test]
    fn i2c_address_direction_ten_bit_and_data_bytes() {
        use Kind::*;
        let mut m = Model { dec_proto: Proto::I2C, trig_bus: BusWhat::Id, trig_bus_val: 0x50, ..Model::default() };
        // write to 0x50: 0x12 0x34 0x56; read from 0x50: 0xab; 10-bit write to 0x2a5: 0x01
        let p = [
            pk(0.0, Start, 0),
            pk(1.0, Addr { read: false }, 0x50),
            pk(2.0, Data, 0x12),
            pk(3.0, Data, 0x34),
            pk(4.0, Data, 0x56),
            pk(5.0, RepStart, 0),
            pk(6.0, Addr { read: true }, 0x50),
            pk(7.0, Data, 0xab),
            pk(8.0, Stop, 0),
            pk(9.0, Start, 0),
            pk(10.0, Addr { read: false }, 0x7a),
            pk(11.0, Data, 0xa5),
            pk(12.0, Data, 0x01),
            pk(13.0, Stop, 0),
        ];
        assert_eq!(points(&m, &p), [1.0, 6.0]);
        m.ser.dir = I2cDir::Read;
        assert_eq!(points(&m, &p), [6.0]);
        // 10-bit: the address completes with the second byte
        (m.ser.dir, m.ser.ten_bit, m.trig_bus_val) = (I2cDir::Either, true, 0x2a5);
        assert_eq!(points(&m, &p), [11.0]);
        // the decoder joins the two bytes of that address into one packet: the points are the same, and with the
        // trigger set to seven bits its first byte is still the address 7A and its second a data byte
        let mut joined = p[..10].to_vec();
        let ten = Some(oos_dsp::decode::I2C_TEN_BIT);
        joined.push(Packet { t0: 9.5, t1: 11.0, value2: ten, ..pk(11.0, Addr { read: false }, 0x2a5) });
        joined.extend_from_slice(&p[12..]);
        assert_eq!(points(&m, &joined), [11.0]);
        (m.ser.ten_bit, m.trig_bus_val) = (false, 0x7a);
        let seven = points(&m, &joined);
        assert_eq!(seven.len(), 1);
        assert!(seven[0] > 9.5 && seven[0] < 11.0, "at the end of the first byte: {seven:?}");
        (m.trig_bus, m.trig_bus_val) = (BusWhat::Data, 0xa5);
        assert_eq!(points(&m, &joined), [11.0], "the low byte is a data byte to a trigger on seven bits");
        (m.trig_bus, m.ser.ten_bit, m.trig_bus_val) = (BusWhat::Id, true, 0x2a5);
        // data: any byte, then the first two bytes after the address, big-endian
        (m.ser.ten_bit, m.trig_bus, m.trig_bus_val) = (false, BusWhat::Data, 0x34);
        assert_eq!(points(&m, &p), [3.0]);
        (m.ser.bytes, m.trig_bus_val) = (2, 0x1234);
        assert_eq!(points(&m, &p), [3.0]);
        (m.ser.bytes, m.trig_bus_val) = (3, 0x12_3456);
        assert_eq!(points(&m, &p), [4.0]);
        // a mask over the second byte only, and a range over the first byte
        (m.ser.bytes, m.trig_bus_val, m.ser.mask) = (2, 0x0034, 0x00ff);
        assert_eq!(points(&m, &p), [3.0]);
        (m.ser.bytes, m.ser.mask, m.ser.rel, m.trig_bus_val, m.ser.max) = (1, u64::MAX, Rel::In, 0x10, 0xaf);
        // the first byte of each frame: 0x12, 0xab and 0xa5 (the 7-bit reading of the third frame's address)
        assert_eq!(points(&m, &p), [2.0, 7.0, 11.0]);
    }
}

#[cfg(test)]
mod polarity_tests {
    use super::*;

    /// An inverted 1553 message on CH1, 50 samples a microsecond.
    fn inverted_1553() -> (Model, Vec<f32>, f64) {
        let words = [(true, 0x0c23, 7.0), (true, 0x0800, 0.0), (false, 0xbeef, 0.0), (false, 0x1234, 0.0)];
        let l = oos_dsp::sim::mil1553_message(&words, 7.0);
        let dt = 1e-6 / 50.0;
        let v: Vec<f32> = oos_dsp::sim::sample(|t| -l.v(t), 0.0, dt, 50 * 120);
        let mut m = Model::default();
        (m.dec_proto, m.dec_src, m.dec_thr, m.dec_on) = (Proto::from_key("Mil1553").unwrap(), 0, 0.0, true);
        (m, v, dt)
    }

    // Break it catches: a UART line wired the wrong way round that Normal reads as garbage without the diagnosis naming
    // the Polarity row, or Auto that does not say it read the line inverted (the band's "(inverted)")
    #[test]
    fn an_inverted_uart_line_is_diagnosed_and_read_right_with_auto() {
        let baud = crate::decode::BAUDS[Model::default().dec_baud];
        let mut w = oos_dsp::decode::gen::Wave::new(3.3, 0.0, 0.02 / baud);
        w.level(true, 30.0 / baud);
        for b in b"Hello" {
            w.uart(*b as u32, 8, oos_dsp::decode::Parity::Even, 1, baud);
            w.level(true, 3.0 / baud);
        }
        let dt = 1.0 / baud / 16.0;
        // low where the line should be high: idle low, start bit high
        let v = w.done().sample(dt, 20.0 / baud);
        let mut m = Model::default();
        (m.dec_proto, m.dec_src, m.dec_thr, m.dec_on) = (Proto::from_key("Uart").unwrap(), 0, 1.65, true);
        m.dec_parity = oos_dsp::decode::Parity::Even;
        m.dec_bits = 8;
        let ch: [&[f32]; NCH] = std::array::from_fn(|c| if c == 0 { &v[..] } else { &[][..] });
        let good: Vec<f32> = v.iter().map(|x| 3.3 - x).collect();
        let chg: [&[f32]; NCH] = std::array::from_fn(|c| if c == 0 { &good[..] } else { &[][..] });
        assert!(decode(&m, chg, dt).unwrap().iter().all(|x| x.ok));
        assert!(diagnosis(&m).unwrap().lines.is_empty());
        let p = decode(&m, ch, dt).unwrap();
        assert!(p.is_empty() || p.iter().any(|x| !x.ok));
        let d = diagnosis(&m).unwrap();
        assert!(d.lines.join(" ").contains("set Polarity to Auto or Inverted"), "{:?}", d.lines);
        m.dec_opt[0] = 2;
        let p = decode(&m, ch, dt).unwrap();
        assert!(p.len() >= 5 && p.iter().all(|x| x.ok) && read_inverted(&m));
    }

    // Break it catches: the diagnosis of a decode not kept where the band, the Events pane, SCPI and MCP read it, Auto
    // not saying it read inverted, or Normal's sentence missing the row that fixes it
    #[test]
    fn an_inverted_bus_is_diagnosed_as_it_is_and_read_right_with_auto() {
        let (mut m, v, dt) = inverted_1553();
        let ch: [&[f32]; NCH] = std::array::from_fn(|c| if c == 0 { &v[..] } else { &[][..] });
        let p = decode(&m, ch, dt).unwrap();
        assert!(p.iter().any(|x| !x.ok));
        let d = diagnosis(&m).unwrap();
        assert!(d.lines.join(" ").contains("set Polarity to Auto or Inverted"), "{:?}", d.lines);
        assert!(d.error_fraction() > 0.5 && !read_inverted(&m));
        // Auto reads it right, says so, and has nothing to diagnose
        m.dec_opt[0] = 2;
        let p = decode(&m, ch, dt).unwrap();
        assert!(p.iter().all(|x| x.ok) && !p.is_empty());
        assert!(read_inverted(&m));
        assert!(diagnosis(&m).unwrap().lines.is_empty());
    }
}
