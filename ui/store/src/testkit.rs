// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Waveform files of other instruments, built for the tests of their importers here and for the hostile-input tests
//! of oos-ui (the feature `testkit`, which oos-ui turns on for its tests only): each takes the time of the first
//! sample, the sample step and the samples.

pub fn isf_i16(t0: f64, dt: f64, ymult: f64, codes: &[i16]) -> Vec<u8> {
    let mut curve = Vec::new();
    for c in codes {
        curve.extend_from_slice(&c.to_le_bytes());
    }
    let len = curve.len().to_string();
    let mut b = format!(":WFMPRE:BYT_NR 2;ENCDG BIN;BN_FMT RI;BYT_OR LSB;NR_PT {};PT_FMT Y;XINCR {dt:.9e};XZERO {t0:.9e};PT_OFF 0;YMULT {ymult:.9e};YOFF 0;YZERO 0;WFID \"lane\";:CURVE #{}{len}", codes.len(), len.len()).into_bytes();
    b.extend(curve);
    b
}

pub fn wavedesc_i16(t0: f64, dt: f64, gain: f32, off: f32, codes: &[i16]) -> Vec<u8> {
    let desc = 346usize;
    let mut b = vec![0u8; desc + codes.len() * 2];
    b[..8].copy_from_slice(b"WAVEDESC");
    b[32] = 1;
    b[34] = 1;
    b[36..40].copy_from_slice(&(desc as u32).to_le_bytes());
    b[60..64].copy_from_slice(&((codes.len() * 2) as u32).to_le_bytes());
    b[116..120].copy_from_slice(&(codes.len() as u32).to_le_bytes());
    b[96..98].copy_from_slice(b"C1");
    b[156..160].copy_from_slice(&gain.to_le_bytes());
    b[160..164].copy_from_slice(&off.to_le_bytes());
    b[176..180].copy_from_slice(&(dt as f32).to_le_bytes());
    b[180..188].copy_from_slice(&t0.to_le_bytes());
    for (k, c) in codes.iter().enumerate() {
        b[desc + k * 2..desc + k * 2 + 2].copy_from_slice(&c.to_le_bytes());
    }
    b
}

pub fn ag_f32(t0: f64, dt: f64, v: &[f32], label: &str) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"AG10");
    let size_at = b.len();
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&1u32.to_le_bytes());
    let mut h = vec![0u8; 140];
    h[..4].copy_from_slice(&140u32.to_le_bytes());
    h[4..8].copy_from_slice(&1u32.to_le_bytes());
    h[8..12].copy_from_slice(&1u32.to_le_bytes());
    h[12..16].copy_from_slice(&(v.len() as u32).to_le_bytes());
    h[32..40].copy_from_slice(&dt.to_le_bytes());
    h[40..48].copy_from_slice(&t0.to_le_bytes());
    h[112..112 + label.len()].copy_from_slice(label.as_bytes());
    b.extend(h);
    b.extend_from_slice(&12u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&((v.len() * 4) as u32).to_le_bytes());
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    let n = b.len() as u32;
    b[size_at..size_at + 4].copy_from_slice(&n.to_le_bytes());
    b
}

fn tr0_block(payload: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"\x04\x00\x00\x00\x00\x00\x00\x00\x04\x00\x00\x00");
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    b.extend_from_slice(payload);
    b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    b
}

pub fn tr0_f32(t0: f64, dt: f64, v: &[f32]) -> Vec<u8> {
    let mut header = String::from("00020000000000009601*x ");
    for name in ["TIME", "v(1)"] {
        header.push_str(&format!("{name:<16}"));
    }
    header.push_str("$&%#");
    let mut data = Vec::new();
    for (k, x) in v.iter().enumerate() {
        let t = (t0 + k as f64 * dt) as f32;
        data.extend_from_slice(&t.to_le_bytes());
        data.extend_from_slice(&x.to_le_bytes());
    }
    data.extend_from_slice(&1.0e30_f32.to_le_bytes());
    let mut b = tr0_block(header.as_bytes());
    b.extend(tr0_block(&data));
    b
}

pub fn raw_f64(t0: f64, dt: f64, v: &[f32]) -> Vec<u8> {
    let mut b = format!("Title: t\nDate: today\nPlotname: Transient Analysis\nFlags: real\nNo. Variables: 2\nNo. Points: {}\nVariables:\n\t0\ttime\ttime\n\t1\tv(1)\tvoltage\nBinary:\n", v.len()).into_bytes();
    for (k, x) in v.iter().enumerate() {
        b.extend_from_slice(&(t0 + k as f64 * dt).to_le_bytes());
        b.extend_from_slice(&(*x as f64).to_le_bytes());
    }
    b
}

pub fn wfmx_f32(t0: f64, dt: f64, v: &[f32]) -> Vec<u8> {
    let mut b = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<DcaFile><Header><FileType>WaveformInternal</FileType></Header><Points>{}</Points><XOrg>{t0}</XOrg><XInc>{dt}</XInc><Data Lengths=\"{}, 1\"></Data></DcaFile>\n", v.len(), v.len()).into_bytes();
    for x in v {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b
}

/// A short WFM curve of f32 samples, little endian, of version `magic` (`:WFM#001`, `:WFM#002` or `:WFM#003`).
pub fn wfm_f32(magic: &[u8; 8], t0: f64, dt: f64, v: &[f32], label: &str) -> Vec<u8> {
    let (explicit, implicit, specification, curve) =
        if magic == b":WFM#001" { (166, 478, 790, 820) } else { (168, 488, 808, 838) };
    let mut b = vec![0u8; curve + v.len() * 4 + 8];
    b[0..2].copy_from_slice(&0x0F0Fu16.to_le_bytes());
    b[2..10].copy_from_slice(magic);
    b[15] = 4;
    b[16..20].copy_from_slice(&(curve as i32).to_le_bytes());
    let lab = label.as_bytes();
    b[40..40 + lab.len()].copy_from_slice(lab);
    let put_f64 = |b: &mut [u8], o: usize, x: f64| b[o..o + 8].copy_from_slice(&x.to_le_bytes());
    let put_u32 = |b: &mut [u8], o: usize, x: u32| b[o..o + 4].copy_from_slice(&x.to_le_bytes());
    put_f64(&mut b, explicit, 1.0);
    put_f64(&mut b, explicit + 8, 0.0);
    put_u32(&mut b, explicit + 72, 4);
    put_f64(&mut b, implicit, dt);
    put_f64(&mut b, implicit + 8, t0);
    put_u32(&mut b, specification + 14, 0);
    put_u32(&mut b, specification + 18, (v.len() * 4) as u32);
    for (k, &x) in v.iter().enumerate() {
        b[curve + k * 4..curve + k * 4 + 4].copy_from_slice(&x.to_le_bytes());
    }
    b
}

/// A deflate stream of one block with dynamic codes (RFC 1951 3.2.7) that holds `data` as literals: the end of the
/// block has a code of one bit, every byte one of nine. The code lengths are sent with a code of two symbols of one
/// bit each (the lengths 1 and 9).
pub fn deflate_dynamic(data: &[u8]) -> Vec<u8> {
    let (mut out, mut used) = (Vec::new(), 0usize);
    // a value goes in from its lowest bit on, into a byte from its lowest bit on
    let mut value = |v: u32, n: u32| {
        for k in 0..n {
            if used / 8 == out.len() {
                out.push(0u8);
            }
            out[used / 8] |= ((v >> k) as u8 & 1) << (used % 8);
            used += 1;
        }
    };
    // the last block, of type 2; 257 literal and length codes, 1 distance code, 18 lengths of the code of lengths
    for (v, n) in [(1, 1), (2, 2), (0, 5), (0, 5), (14, 4)] {
        value(v, n);
    }
    // in the order they are sent in: one bit for the lengths 9 and 1, no code for the other symbols
    for symbol in [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1] {
        value((symbol == 9 || symbol == 1) as u32, 3);
    }
    // the lengths: 9 for each of the 256 bytes (code 1), 1 for the end of the block and for the one distance (code 0)
    (0..256).for_each(|_| value(1, 1));
    value(0, 2);
    // a Huffman code goes in from its highest bit on: the byte x has the code 256 + x, the end of the block 0
    for &byte in data {
        (0..9).rev().for_each(|k| value((256 + byte as u32) >> k, 1));
    }
    value(0, 1);
    out
}

/// One file of an archive built by `zip`.
pub struct ZipFile<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
    /// the deflate stream of `data`; None: stored as it is
    pub deflated: Option<Vec<u8>>,
    /// the sizes and the check sum stand after the bytes (a data descriptor), and the header in front has zeros
    pub late: bool,
}

impl<'a> ZipFile<'a> {
    pub fn stored(name: &'a str, data: &'a [u8]) -> Self {
        ZipFile { name, data, deflated: None, late: false }
    }

    /// Deflated with dynamic codes, the sizes after the bytes: as most entries of a session archive are.
    pub fn deflated(name: &'a str, data: &'a [u8]) -> Self {
        ZipFile { name, data, deflated: Some(deflate_dynamic(data)), late: true }
    }
}

/// A zip archive of `files`: per file a header and its bytes, then the list of entries and the end record.
pub fn zip(files: &[ZipFile]) -> Vec<u8> {
    let (mut b, mut list) = (Vec::new(), Vec::new());
    for f in files {
        let packed = f.deflated.as_deref().unwrap_or(f.data);
        let sums: Vec<u8> = [oos_dsp::decode::crc32(f.data), packed.len() as u32, f.data.len() as u32]
            .iter()
            .flat_map(|x| x.to_le_bytes())
            .collect();
        // version 2.0, the flags (bit 3: sizes after the bytes), the method, no time and no date
        let mut head = vec![20, 0, (f.late as u8) << 3, 0, if f.deflated.is_some() { 8 } else { 0 }, 0, 0, 0, 0, 0];
        let at = b.len() as u32;
        b.extend_from_slice(b"PK\x03\x04");
        b.extend_from_slice(&head);
        b.extend_from_slice(if f.late { &[0; 12] } else { &sums });
        b.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
        b.extend_from_slice(&[0, 0]);
        b.extend_from_slice(f.name.as_bytes());
        b.extend_from_slice(packed);
        if f.late {
            b.extend_from_slice(b"PK\x07\x08");
            b.extend_from_slice(&sums);
        }
        list.extend_from_slice(b"PK\x01\x02");
        head.splice(0..0, [20, 0]);
        list.extend_from_slice(&head);
        list.extend_from_slice(&sums);
        list.extend_from_slice(&(f.name.len() as u16).to_le_bytes());
        // no extra field, no comment, disk 0, no attributes
        list.extend_from_slice(&[0; 12]);
        list.extend_from_slice(&at.to_le_bytes());
        list.extend_from_slice(f.name.as_bytes());
    }
    let at = b.len() as u32;
    b.extend_from_slice(&list);
    b.extend_from_slice(b"PK\x05\x06\0\0\0\0");
    b.extend_from_slice(&(files.len() as u16).to_le_bytes());
    b.extend_from_slice(&(files.len() as u16).to_le_bytes());
    b.extend_from_slice(&(list.len() as u32).to_le_bytes());
    b.extend_from_slice(&at.to_le_bytes());
    b.extend_from_slice(&[0, 0]);
    b
}

/// The archive `.set` of a session archive (docs/tss.md): the settings `setup` as a text, the list that names the
/// file of each reference, and the reference waveforms (number, file name, bytes).
pub fn tss_set(setup: &str, refs: &[(usize, &str, Vec<u8>)]) -> Vec<u8> {
    let ini: String =
        refs.iter().map(|(n, name, _)| format!("recallSubsystem\\refMgr\\ref{n}\\sourceFileName={name}\n")).collect();
    let mut inner = vec![ZipFile::deflated("bench.set", setup.as_bytes())];
    if !refs.is_empty() {
        inner.push(ZipFile::stored("recall.ini", ini.as_bytes()));
    }
    inner.extend(refs.iter().map(|(_, name, bytes)| ZipFile::deflated(name, bytes)));
    zip(&inner)
}

/// A session archive as the PC program of another instrument stores one: a picture, the archive `.set` (`tss_set`)
/// and the files of `beside` (the records of the channels) next to it.
pub fn tss(setup: &str, refs: &[(usize, &str, Vec<u8>)], beside: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let set = tss_set(setup, refs);
    let mut outer = vec![ZipFile::stored("shot.png", b"a picture"), ZipFile::deflated("bench.set", &set)];
    outer.extend(beside.iter().map(|(name, bytes)| ZipFile::stored(name, bytes)));
    zip(&outer)
}
