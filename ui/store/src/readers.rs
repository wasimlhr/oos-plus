// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the waveform files of other scopes that `parse_wave` (import.rs) recognises by their header: Rohde &
//! Schwarz (a description and a data file, `parse_wave_at`), Siglent, and RIGOL's own `.wfm` (with the inflate its
//! DHO files need, and the deflate tables and Adler-32 that oos-ui's PNG writer shares). Upstream OOS has no such
//! readers; they came over from oos-ui's store with the crate.

use super::*;
#[cfg(test)]
use crate::import::{json_members, rigol_csv_header, RIGOL_CSV_CELLS, RIGOL_CSV_LINE};

/// Adler-32 (zlib). Sixteen lanes side by side (the compiler makes vector code of the lane loop): lane `k` sums the
/// bytes at `16 j + k` (`sa`) and those sums again (`sb`), and per chunk of 5552 bytes `b` grows by `len * a`,
/// `16 * sum(sb)` less `sum(k * sa[k])` (each byte counted `len - position` times), the same as byte by byte.
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u64, 0u64);
    for chunk in data.chunks(5552) {
        let (mut sa, mut sb) = ([0u32; 16], [0u32; 16]);
        let (blocks, rest) = chunk.as_chunks::<16>();
        for blk in blocks {
            for k in 0..16 {
                sa[k] += blk[k] as u32;
                sb[k] += sa[k];
            }
        }
        let n = (chunk.len() - rest.len()) as u64;
        let sum_a: u64 = sa.iter().map(|&x| x as u64).sum();
        let sum_b: u64 = sb.iter().map(|&x| x as u64).sum();
        let weighted: u64 = sa.iter().enumerate().map(|(k, &x)| k as u64 * x as u64).sum();
        b += n * a + 16 * sum_b - weighted;
        a += sum_a;
        for &x in rest {
            a += x as u64;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    ((b << 16) | a) as u32
}

/// Deflate length codes 257..285 (RFC 1951 3.2.5): base length and extra bits.
pub const LEN_BASE: [(u16, u8); 29] = [
    (3, 0),
    (4, 0),
    (5, 0),
    (6, 0),
    (7, 0),
    (8, 0),
    (9, 0),
    (10, 0),
    (11, 1),
    (13, 1),
    (15, 1),
    (17, 1),
    (19, 2),
    (23, 2),
    (27, 2),
    (31, 2),
    (35, 3),
    (43, 3),
    (51, 3),
    (59, 3),
    (67, 4),
    (83, 4),
    (99, 4),
    (115, 4),
    (131, 5),
    (163, 5),
    (195, 5),
    (227, 5),
    (258, 0),
];
/// Deflate distance codes 0..29: base distance and extra bits.
pub const DIST_BASE: [(u16, u8); 30] = [
    (1, 0),
    (2, 0),
    (3, 0),
    (4, 0),
    (5, 1),
    (7, 1),
    (9, 2),
    (13, 2),
    (17, 3),
    (25, 3),
    (33, 4),
    (49, 4),
    (65, 5),
    (97, 5),
    (129, 6),
    (193, 6),
    (257, 7),
    (385, 7),
    (513, 8),
    (769, 8),
    (1025, 9),
    (1537, 9),
    (2049, 10),
    (3073, 10),
    (4097, 11),
    (6145, 11),
    (8193, 12),
    (12289, 12),
    (16385, 13),
    (24577, 13),
];

#[cfg(test)]
mod waveform_record_tests {
    use super::*;

    fn record(magic: &[u8; 8], header: &str, samples: &[u8]) -> Vec<u8> {
        let mut b = magic.to_vec();
        b.extend_from_slice(&(header.len() as u32).to_le_bytes());
        b.extend_from_slice(header.as_bytes());
        b.extend_from_slice(samples);
        b
    }

    fn words(counts: &[u16]) -> Vec<u8> {
        counts.iter().flat_map(|c| c.to_le_bytes()).collect()
    }

    #[test]
    fn a_word_record_scales_counts_and_places_the_trigger() {
        // the nested preamble repeats the scaling keys with other values: only the members of the header count
        let header = r#"{"version":1,"sampleType":"uint16","nPoints":4,"xincr":2e-9,"xzero":-1e-6,"ptOff":10,"yoff":32768,"ymult":0.001,"yzero":0.5,"yunit":"V","channel":"CH3","preamble":{"xincr":9,"nPoints":99,"yoff":[1,2,{"ymult":7}],"note":"a } and a , in \"text\""},"nativeByteOrder":"little"}"#;
        let bytes = record(b"PWWF0001", header, &words(&[32768, 33768, 31768, 32769, 7]));
        let traces = parse_wave(&bytes, "ch3.pwwf").unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!(t.label, "CH3");
        assert_eq!(t.v.len(), 4, "samples past nPoints are not read");
        for (got, want) in t.v.iter().zip([0.5_f32, 1.5, -0.5, 0.501]) {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
        assert!((t.dt - 2e-9).abs() < 1e-21);
        assert!((t.t0 - (-1e-6 - 10.0 * 2e-9)).abs() < 1e-18);
    }

    #[test]
    fn every_sample_type_and_the_older_magic_are_read() {
        let int8 = record(b"TKWF0001", r#"{"sampleType":"int8","nPoints":3,"xincr":1e-6,"ymult":0.5}"#, &[0xfe, 0, 4]);
        assert_eq!(parse_wave(&int8, "a.tkwf").unwrap()[0].v, [-1.0, 0.0, 2.0]);
        let int16 = record(b"PWWF0001", r#"{"sampleType":"int16","nPoints":2,"xincr":1e-6,"ymult":2}"#, &words(&[0xffff, 3]));
        assert_eq!(parse_wave(&int16, "a.pwwf").unwrap()[0].v, [-2.0, 6.0]);
        let floats: Vec<u8> = [0.25_f32, -1.5].iter().flat_map(|x| x.to_le_bytes()).collect();
        let float32 = record(b"PWWF0001", r#"{ "sampleType" : "float32" , "nPoints" : 2 , "xincr" : 1e-3 , "ptOff" : null }"#, &floats);
        let t = &parse_wave(&float32, "a.pwwf").unwrap()[0];
        assert_eq!(t.v, [0.25, -1.5]);
        assert_eq!((t.t0, t.label.as_str()), (0.0, "Record"));
    }

    #[test]
    fn a_damaged_record_is_refused_with_its_reason() {
        let good = r#"{"sampleType":"uint16","nPoints":3,"xincr":1e-6}"#;
        let refused = |bytes: Vec<u8>| parse_wave(&bytes, "x.pwwf").map(|_| ()).unwrap_err();
        assert_eq!(refused(record(b"PWWF0001", good, &words(&[1, 2]))), "truncated waveform record");
        let mut cut = record(b"PWWF0001", good, &[]);
        cut.truncate(20);
        assert_eq!(refused(cut), "truncated waveform record");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","nPoints":3"#, &words(&[1, 2, 3]))), "the waveform record header is not readable");
        assert_eq!(refused(record(b"PWWF0001", r#"["sampleType","uint16"]"#, &words(&[1, 2, 3]))), "the waveform record header is not readable");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","xincr":1e-6}"#, &words(&[1, 2, 3]))), "the waveform record header is not readable");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","nPoints":3,"xincr":0}"#, &words(&[1, 2, 3]))), "that file holds no waveform");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","nPoints":1,"xincr":1e-6}"#, &words(&[1]))), "that file holds no waveform");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"float64","nPoints":3,"xincr":1e-6}"#, &[0; 24])), "waveform record sample type \"float64\" is not read");
        let big = format!(r#"{{"sampleType":"int8","nPoints":{},"xincr":1e-6}}"#, MAX_WAVEFORM_SAMPLES + 1);
        assert_eq!(refused(record(b"PWWF0001", &big, &[0; 8])), "waveform exceeds the 32-million-sample import limit");
        let nan: Vec<u8> = [f32::NAN, 0.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"float32","nPoints":2,"xincr":1e-6}"#, &nan)), "the waveform record holds samples that are not numbers");
    }

    #[test]
    fn json_members_keeps_nested_values_whole_and_unescapes_strings() {
        let members = json_members(r#" { "a" : "x\n\"y\" é" , "b" : -1.5e3 , "c" : { "a" : [ 1 , "}" ] } , "d" : [ ] , "e" : true } "#).unwrap();
        let get = |key: &str| members.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap();
        assert_eq!(get("a"), "x\n\"y\" \u{e9}");
        assert_eq!(get("b"), "-1.5e3");
        assert_eq!(get("c"), r#"{ "a" : [ 1 , "}" ] }"#);
        assert_eq!((get("d"), get("e")), ("[ ]", "true"));
        assert_eq!(json_members("{}").unwrap().len(), 0);
        for bad in ["", "[]", "{", r#"{"a"}"#, r#"{"a":}"#, r#"{,"a":1}"#, r#"{"a":1} x"#, r#"{"a":"unterminated}"#, r#"{"a":1]}"#] {
            assert!(json_members(bad).is_none(), "{bad}");
        }
    }
}

#[cfg(test)]
mod rigol_bin_tests {
    use super::*;

    /// One waveform of an `RG` file: the 140-byte waveform header, the 16-byte buffer header and the buffer.
    fn waveform(wave_type: u32, buffer_type: u16, bytes_per_point: u16, dt: f64, origin: f64, label: &str, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; 140];
        h[..4].copy_from_slice(&140u32.to_le_bytes());
        h[4..8].copy_from_slice(&wave_type.to_le_bytes());
        h[8..12].copy_from_slice(&1u32.to_le_bytes());
        h[12..16].copy_from_slice(&((data.len() / bytes_per_point as usize) as u32).to_le_bytes());
        h[32..40].copy_from_slice(&dt.to_le_bytes());
        h[40..48].copy_from_slice(&origin.to_le_bytes());
        h[48..52].copy_from_slice(&2u32.to_le_bytes());
        h[52..56].copy_from_slice(&1u32.to_le_bytes());
        h[88..88 + 12].copy_from_slice(b"DHO924S:DHO9");
        h[112..112 + label.len()].copy_from_slice(label.as_bytes());
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&buffer_type.to_le_bytes());
        h.extend_from_slice(&bytes_per_point.to_le_bytes());
        h.extend_from_slice(&(data.len() as u64).to_le_bytes());
        h.extend_from_slice(data);
        h
    }

    fn file(waveforms: &[Vec<u8>]) -> Vec<u8> {
        let mut b = b"RG01".to_vec();
        let size = 16 + waveforms.iter().map(Vec::len).sum::<usize>();
        b.extend_from_slice(&(size as u64).to_le_bytes());
        b.extend_from_slice(&(waveforms.len() as u32).to_le_bytes());
        for w in waveforms {
            b.extend_from_slice(w);
        }
        b
    }

    fn floats(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn every_analog_channel_is_a_trace_and_the_record_starts_before_the_trigger() {
        let a = [0.0_f32, 0.5, 1.0, -0.25];
        let b = [3.3_f32, 3.2, 3.1, 3.0];
        let bytes = file(&[
            waveform(1, 1, 4, 4e-7, 2e-3, "CH1", &floats(&a)),
            waveform(6, 5, 1, 4e-7, 2e-3, "D0", &[0, 1, 1, 0]),
            waveform(1, 1, 4, 4e-7, 2e-3, "CH3", &floats(&b)),
        ]);
        let traces = parse_wave(&bytes, "capture.bin").unwrap();
        assert_eq!(traces.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH3"]);
        assert_eq!((traces[0].v.as_slice(), traces[1].v.as_slice()), (&a[..], &b[..]));
        for t in &traces {
            assert!((t.dt - 4e-7).abs() < 1e-18);
            // X Origin holds the distance before the trigger: the first point is at its negative
            assert!((t.t0 + 2e-3).abs() < 1e-15, "{}", t.t0);
        }
    }

    #[test]
    fn a_file_of_logic_channels_or_a_cut_file_is_refused_with_its_reason() {
        let logic = file(&[waveform(6, 5, 1, 1e-6, 0.0, "D0", &[0, 1, 1, 0])]);
        assert_eq!(parse_wave(&logic, "la.bin").map(|_| ()).unwrap_err(), "that file holds logic channels only, and they are not read");
        let mut cut = file(&[waveform(1, 1, 4, 1e-6, 0.0, "CH1", &floats(&[1.0, 2.0, 3.0]))]);
        cut.truncate(cut.len() - 5);
        assert_eq!(parse_wave(&cut, "cut.bin").map(|_| ()).unwrap_err(), "truncated waveform");
        // a buffer size past the end of the file, in the upper half of its eight bytes too
        let mut huge = file(&[waveform(1, 1, 4, 1e-6, 0.0, "CH1", &floats(&[1.0, 2.0]))]);
        let at = 16 + 140 + 8;
        huge[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(parse_wave(&huge, "huge.bin").map(|_| ()).unwrap_err(), "truncated waveform");
        huge[at..at + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert_eq!(parse_wave(&huge, "huge.bin").map(|_| ()).unwrap_err(), "truncated waveform");
    }

    #[test]
    fn the_other_maker_of_the_format_still_starts_at_its_x_origin() {
        // cookie AG: a 12-byte file header and a 12-byte buffer header, and X Origin is the first point
        let v = [1.0_f32, 2.0, 3.0];
        let mut b = b"AG10".to_vec();
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut h = waveform(1, 1, 4, 1e-9, -5e-6, "lane", &[]);
        h.truncate(140);
        h[12..16].copy_from_slice(&3u32.to_le_bytes());
        b.extend(h);
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend(floats(&v));
        let t = &parse_wave(&b, "k.bin").unwrap()[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&v[..], "lane"));
        assert!((t.t0 + 5e-6).abs() < 1e-18);
    }
}

#[cfg(test)]
mod keysight_file_tests {
    use super::*;

    /// The header of the example in Keysight's description of the Y-values file, with these amplitudes.
    fn y_values(extra: &str, values: &str) -> String {
        format!("File Format, WaveformYValues\r\nFormat Version, 2\r\nInstrument, D9300A\r\nSwVersion, P.26.01.214\r\nSerialNumber, MXL3441NC2\r\nDate, 3/30/2026 13:33:17 GMT-06:00\r\n\r\nPoints, 4\r\nSignal Type, Unspecified\r\nSource Name, Channel 1\r\nChannel Noise, 0.002\r\nIntrinsic Jitter, 1E-14\r\nInterpolation Factor, 4\r\n{extra}Bandwidth, 128000000000\r\nX Units, Second\r\nY Units, Volt\r\nData,\r\nfloat\r\n{values}")
    }

    #[test]
    fn a_y_values_file_takes_its_time_scale_from_the_header() {
        let text = y_values("XOrg, -5.0000000000000004E-08\r\nXInc, 9.765625E-13\r\n", "0.248322\r\n0.25\r\n-0.125\r\n1.5E-01\r\n");
        let traces = parse_wave(text.as_bytes(), "ch1.txt").unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!(t.v, [0.248322, 0.25, -0.125, 0.15]);
        assert_eq!((t.t0, t.dt, t.label.as_str()), (-5.0000000000000004E-08, 9.765625E-13, "Channel 1"));
        // version 1 has no number-type line; a byte-order mark before the first line is not part of it
        let v1 = format!("{}{}", char::from_u32(0xfeff).unwrap(), text.replace("float\r\n", "").replace("Format Version, 2", "Format Version, 1"));
        assert_eq!(parse_wave(v1.as_bytes(), "ch1.txt").unwrap()[0].v, [0.248322, 0.25, -0.125, 0.15]);
    }

    #[test]
    fn a_y_values_file_without_a_time_scale_is_refused_with_its_reason() {
        let refused = |text: String| parse_wave(text.as_bytes(), "y.txt").map(|_| ()).unwrap_err();
        assert_eq!(refused(y_values("", "1\r\n2\r\n")), "the Y-values file has no XOrg and XInc in its header");
        assert_eq!(refused(y_values("XOrg, 0\r\nXInc, 0\r\n", "1\r\n2\r\n")), "that file holds no waveform");
        assert_eq!(refused(y_values("XOrg, 0\r\nXInc, 1E-9\r\n", "1\r\n")), "that file holds no waveform");
        // amplitudes alone, the file saved without its header: one column, no time
        assert_eq!(parse_wave(b"0.1\r\n0.2\r\n0.3\r\n", "y.txt").map(|_| ()).unwrap_err(), "that file holds no waveform");
    }

    #[test]
    fn an_xy_values_file_is_read_with_the_name_of_its_source() {
        let text = "File Format, WaveformXYValues\r\nFormat Version, 2\r\nInstrument, D9300A\r\nDate, 3/30/2026 13:33:48 GMT-06:00\r\n\r\nPoints, 3\r\nSource Name, Channel 2\r\nInterpolation Factor, 0\r\nBandwidth, 128000000000\r\nX Units, Second\r\nY Units, Volt\r\nData,\r\ndouble, float\r\n-5.0E-08, 0.248322,\r\n-4.9E-08, 0.25,\r\n-4.8E-08, -0.125,\r\n";
        let traces = parse_wave(text.as_bytes(), "wave.csv").unwrap();
        assert_eq!(traces.len(), 1, "a line that ends with its separator has no column after it");
        let t = &traces[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&[0.248322f32, 0.25, -0.125][..], "Channel 2"));
        assert!((t.t0 + 5.0e-8).abs() < 1e-20 && (t.dt - 1.0e-9).abs() < 1e-18, "{} {}", t.t0, t.dt);
        // a plain time,volts table keeps the name of its file
        assert_eq!(parse_wave(b"0,1\n1,2\n2,3\n", "plain.csv").unwrap()[0].label, "plain.csv");
    }

    #[test]
    fn containers_that_are_not_read_say_so_and_how_to_get_round_it() {
        let hdf5 = [0x89, b'H', b'D', b'F', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert_eq!(parse_wave(&hdf5, "w.h5").map(|_| ()).unwrap_err(), "HDF5 (.h5) waveform files are not read; save the waveform as BIN (.bin) or CSV");
        let mat = b"MATLAB 5.0 MAT-file, Platform: PCWIN64";
        assert_eq!(parse_wave(mat, "w.mat").map(|_| ()).unwrap_err(), "MATLAB (.mat) waveform files are not read; save the waveform as BIN (.bin) or CSV");
    }

    /// An `AG` file of one waveform with one buffer of this type.
    fn ag(wave_type: u32, buffer_type: u16, bytes_per_point: u16, data: &[u8]) -> Vec<u8> {
        let mut b = b"AG10".to_vec();
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut h = vec![0u8; 140];
        h[..4].copy_from_slice(&140u32.to_le_bytes());
        h[4..8].copy_from_slice(&wave_type.to_le_bytes());
        h[8..12].copy_from_slice(&1u32.to_le_bytes());
        h[12..16].copy_from_slice(&((data.len() / bytes_per_point as usize) as u32).to_le_bytes());
        h[32..40].copy_from_slice(&1e-9f64.to_le_bytes());
        h[40..48].copy_from_slice(&(-2e-9f64).to_le_bytes());
        b.extend(h);
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&buffer_type.to_le_bytes());
        b.extend_from_slice(&bytes_per_point.to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn the_digital_buffer_of_an_ag_file_is_type_six_and_its_counts_are_not_logic() {
        let refused = |bytes: Vec<u8>| parse_wave(&bytes, "k.bin").map(|_| ()).unwrap_err();
        assert_eq!(refused(ag(6, 6, 1, &[0, 1, 1, 0])), "that file holds logic channels only, and they are not read");
        // type 5 in an AG file is the counts of a histogram: no trace, and not called a logic channel
        let counts: Vec<u8> = [3.0f32, 9.0, 4.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(refused(ag(5, 5, 4, &counts)), "that file holds no waveform");
        let volts: Vec<u8> = [0.5f32, -0.5, 0.25].iter().flat_map(|x| x.to_le_bytes()).collect();
        let t = &parse_wave(&ag(1, 1, 4, &volts), "k.bin").unwrap()[0];
        assert_eq!((t.v.as_slice(), t.t0, t.dt), (&[0.5f32, -0.5, 0.25][..], -2e-9, 1e-9));
    }
}

// ── Rohde & Schwarz RTO / RTE / RTP: a description file and a data file ──────────────────────────────────────

/// The elements of a description file that carry a `Name`, each with its attributes; the first of a name is kept.
pub(super) struct RsTags(Vec<(String, Vec<(String, String)>)>);

impl RsTags {
    fn read(xml: &str) -> Self {
        let mut tags: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut rest = xml;
        while let Some(open) = rest.find('<') {
            rest = &rest[open + 1..];
            let Some(close) = rest.find('>') else { break };
            let tag = &rest[..close];
            rest = &rest[close + 1..];
            if tag.starts_with('?') || tag.starts_with('!') || tag.starts_with('/') {
                continue;
            }
            let attributes = rs_attributes(tag);
            let Some(name) = attributes.iter().find(|(key, _)| key == "Name").map(|(_, value)| value.clone()) else { continue };
            if !tags.iter().any(|(known, _)| *known == name) {
                tags.push((name, attributes));
            }
        }
        Self(tags)
    }

    fn attribute(&self, name: &str, key: &str) -> Option<&str> {
        let (_, attributes) = self.0.iter().find(|(known, _)| known == name)?;
        attributes.iter().find(|(k, _)| k == key).map(|(_, value)| value.as_str())
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.attribute(name, "Value")
    }

    fn number(&self, name: &str, key: &str) -> Result<f64, String> {
        self.attribute(name, key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|x| x.is_finite())
            .ok_or_else(|| format!("the Rohde & Schwarz description has no usable {name}"))
    }

    fn count(&self, name: &str) -> Result<usize, String> {
        let x = self.number(name, "Value")?;
        if x < 0.0 || x.fract() != 0.0 || x > u32::MAX as f64 {
            return Err(format!("the Rohde & Schwarz description has no usable {name}"));
        }
        Ok(x as usize)
    }
}

/// The `key="value"` pairs of one tag.
pub(super) fn rs_attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(equals) = rest.find('=') {
        let key = rest[..equals].split_whitespace().last().unwrap_or("").to_string();
        let after = rest[equals + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            rest = &rest[equals + 1..];
            continue;
        };
        let body = &after[1..];
        let Some(end) = body.find(quote) else { break };
        if !key.is_empty() {
            out.push((key, body[..end].to_string()));
        }
        rest = &body[end + 1..];
    }
    out
}

pub(super) fn bytes_hold(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// The description file of a Rohde & Schwarz RTO/RTE/RTP waveform: XML that names a signal format and a record
/// length. It has the extension `.bin`, and the samples are in `<name>.Wfm.bin` beside it.
pub(super) fn looks_rs_description(b: &[u8]) -> bool {
    let head = &b[..b.len().min(1 << 20)];
    let start = head.iter().position(|c| !c.is_ascii_whitespace() && ![0xEFu8, 0xBB, 0xBF].contains(c));
    start.is_some_and(|at| head[at] == b'<') && bytes_hold(head, b"eRS_SIGNAL_FORMAT_") && bytes_hold(head, b"RecordLength")
}

/// The data file of such a waveform, by its name and its first word: the format code.
pub(super) fn looks_rs_data(b: &[u8], name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".wfm.bin") && b.len() >= 8 && [0u32, 1, 4, 6].contains(&u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The name a source of such a file is shown by: `eRS_SIGNAL_SOURCE_CH1_TR1` and `C1W1` are CH1; another source
/// keeps its own name.
pub(super) fn rs_source_label(source: &str) -> String {
    let plain = source.trim().trim_start_matches("eRS_SIGNAL_SOURCE_");
    let digit_after = |mark: &str| plain.find(mark).and_then(|at| plain[at + mark.len()..].chars().next()).filter(|c| ('1'..='8').contains(c));
    match digit_after("CH").or_else(|| if plain.starts_with('C') { digit_after("C") } else { None }) {
        Some(n) => format!("CH{n}"),
        None if plain.is_empty() || plain == "NONE" => "CH1".into(),
        None => plain.chars().take(10).collect(),
    }
}

/// A Rohde & Schwarz RTO/RTE/RTP waveform from its two files. The description gives, by `Name`: `SignalFormat`
/// (INT8BIT, INT16BIT, FLOAT or XYDOUBLEFLOAT), `RecordLength`, the `SignalHardwareRecordLength` rows the data
/// file holds and the `LeadingSettlingSamples` before the record in them, `XStart` and `XStop`, and for integer
/// samples the scaling: volts = raw * StepFactor * VerticalScale / NofQuantisationLevels + VerticalOffset -
/// VerticalPosition * VerticalScale. With `MultiChannelExport` on, the channels switched on in
/// `MultiChannelExportState` (`I_0` to `I_3`) are interleaved row by row, each with its own scale, position and
/// offset. The data file starts with a u32 format code (0, 1, 4 or 6) and a u32 row count, little endian; a row
/// of XYDOUBLEFLOAT starts with its time as an f64.
/// Layout after the RigolWFM project's reader of these files; one acquisition, a normal or averaged trace.
pub(super) fn parse_rs(xml: &str, data: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let tags = RsTags::read(xml);
    let Some(format) = tags.value("SignalFormat").map(|f| f.trim_start_matches("eRS_SIGNAL_FORMAT_")) else {
        return Err("the Rohde & Schwarz description has no SignalFormat".into());
    };
    let (code, sample, xy): (u32, usize, bool) = match format {
        "INT8BIT" => (0, 1, false),
        "INT16BIT" => (1, 2, false),
        "FLOAT" => (4, 4, false),
        "XYDOUBLEFLOAT" => (6, 4, true),
        other => return Err(format!("Rohde & Schwarz signal format {other} is not read")),
    };
    if let Some(trace) = tags.value("TraceType").map(|t| t.trim_start_matches("eRS_TRACE_TYPE_")) {
        if trace != "NORMAL" && trace != "AVERAGE" {
            return Err(format!("a Rohde & Schwarz {} trace is not read; save a normal trace", trace.to_ascii_lowercase()));
        }
    }
    if tags.value("NumberOfAcquisitions").is_some() && tags.count("NumberOfAcquisitions")? != 1 {
        return Err("the Rohde & Schwarz file holds several acquisitions; save one acquisition".into());
    }
    let record = tags.count("RecordLength")?;
    let leading = if tags.value("LeadingSettlingSamples").is_some() { tags.count("LeadingSettlingSamples")? } else { 0 };
    let rows = if tags.value("SignalHardwareRecordLength").is_some() { tags.count("SignalHardwareRecordLength")? } else { leading + record };
    if record > MAX_WAVEFORM_SAMPLES {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    if record < 2 || leading.checked_add(record).is_none_or(|end| end > rows) {
        return Err("that file holds no waveform".into());
    }
    let (start, stop) = (tags.number("XStart", "Value")?, tags.number("XStop", "Value")?);

    // the columns of a row: one, or the channels of a multi-channel export that are on
    let multi = tags.value("MultiChannelExport").is_some_and(|v| v.ends_with("_ON"));
    let columns: Vec<(Option<&str>, String)> = if multi {
        ["I_0", "I_1", "I_2", "I_3"].iter().copied()
            .filter(|key| tags.attribute("MultiChannelExportState", key).is_some_and(|v| v.ends_with("_ON")))
            .map(|key| (Some(key), rs_source_label(tags.attribute("MultiChannelSource", key).unwrap_or("")))).collect()
    } else {
        vec![(None, rs_source_label(tags.value("Source").unwrap_or("")))]
    };
    if columns.is_empty() {
        return Err("the Rohde & Schwarz description switches no channel on".into());
    }
    let scale = |column: Option<&str>| -> Result<(f32, f32), String> {
        if sample == 4 {
            return Ok((1.0, 0.0)); // float samples are volts
        }
        let levels = tags.number("NofQuantisationLevels", "Value")?;
        let (scale, position, offset, step) = match column {
            Some(key) => (tags.number("MultiChannelVerticalScale", key)?, tags.number("MultiChannelVerticalPosition", key)?,
                tags.number("MultiChannelVerticalOffset", key)?, tags.number("MultiChannelVerticalScale", "StepFactor")?),
            None => (tags.number("VerticalScale", "Value")?, tags.number("VerticalPosition", "Value")?,
                tags.number("VerticalOffset", "Value")?, tags.number("VerticalScale", "StepFactor")?),
        };
        if levels <= 0.0 {
            return Err("the Rohde & Schwarz description has no usable NofQuantisationLevels".into());
        }
        Ok(((step * scale / levels) as f32, (offset - position * scale) as f32))
    };
    let mut scales = Vec::with_capacity(columns.len());
    for (key, _) in &columns {
        scales.push(scale(*key)?);
    }

    if data.len() < 8 {
        return Err("truncated Rohde & Schwarz waveform data".into());
    }
    let found = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let held = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if found != code || held != rows {
        return Err("the Rohde & Schwarz data file does not belong to this description".into());
    }
    let lead = if xy { 8 } else { 0 };
    let row = lead + columns.len() * sample;
    let body = &data[8..];
    if rows.checked_mul(row).is_none_or(|bytes| bytes > body.len()) {
        return Err("truncated Rohde & Schwarz waveform data".into());
    }
    let window = &body[leading * row..(leading + record) * row];
    let value = |cell: &[u8]| match sample {
        1 => cell[0] as i8 as f32,
        2 => i16::from_le_bytes([cell[0], cell[1]]) as f32,
        _ => f32::from_le_bytes([cell[0], cell[1], cell[2], cell[3]]),
    };
    let (t0, dt) = if xy {
        // each row carries its own time
        let time = |r: usize| {
            let mut at = [0u8; 8];
            at.copy_from_slice(&window[r * row..r * row + 8]);
            f64::from_le_bytes(at)
        };
        (time(0), time(1) - time(0))
    } else {
        (start, (stop - start) / record as f64)
    };
    if !(dt > 0.0) || !dt.is_finite() || !t0.is_finite() {
        return Err("that file holds no waveform".into());
    }
    let mut out = Vec::with_capacity(columns.len());
    for (k, ((_, label), (factor, offset))) in columns.into_iter().zip(scales).enumerate() {
        let at = lead + k * sample;
        let v: Vec<f32> = window.chunks_exact(row).map(|r| value(&r[at..at + sample]) * factor + offset).collect();
        if v.iter().any(|x| !x.is_finite()) {
            return Err("the Rohde & Schwarz data file holds samples that are not numbers".into());
        }
        out.push(WaveTrace { t0, dt, v, label });
    }
    Ok(out)
}

/// A waveform file read where it lies, so that a format of two files finds its other file beside it. A
/// Rohde & Schwarz description (`name.bin`) is read with `name.Wfm.bin`; that data file opened by itself is read
/// with its description. Every other file is `parse_wave` of its bytes.
pub fn parse_wave_at(bytes: &[u8], path: &Path) -> Result<Vec<WaveTrace>, String> {
    let name = file_name(path);
    if looks_rs_description(bytes) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let data = ["Wfm.bin", "wfm.bin", "WFM.BIN", "Wfm.BIN"].iter().map(|end| path.with_file_name(format!("{stem}.{end}"))).find(|p| p.is_file())
            .ok_or_else(|| format!("{name} describes a Rohde & Schwarz waveform; its data file {stem}.Wfm.bin is not beside it"))?;
        return parse_rs(&String::from_utf8_lossy(bytes), &read_waveform_file(&data)?);
    }
    if looks_rs_data(bytes, &name) {
        // the name ends with the eight ASCII characters of .wfm.bin, whatever their case
        let stem = &name[..name.len() - 8];
        let description = path.with_file_name(format!("{stem}.bin"));
        return match read_waveform_file(&description) {
            Ok(xml) if looks_rs_description(&xml) => parse_rs(&String::from_utf8_lossy(&xml), bytes),
            _ => Err(format!("{name} is the data of a Rohde & Schwarz waveform; its description {stem}.bin is not beside it")),
        };
    }
    parse_wave(bytes, &name)
}

#[cfg(test)]
mod rohde_schwarz_file_tests {
    use super::*;

    fn prop(name: &str, value: &str) -> String {
        format!("  <Prop Name=\"{name}\" Value=\"{value}\"/>\n")
    }

    /// A description as the scope writes it: a database of named properties.
    fn description(format: &str, record: usize, leading: usize, rows: usize, more: &str) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Database FWVersion=\"4.60.1.6\">\n <Group Name=\"WaveformData\">\n");
        xml += &prop("SignalFormat", &format!("eRS_SIGNAL_FORMAT_{format}"));
        xml += &prop("RecordLength", &record.to_string());
        xml += &prop("LeadingSettlingSamples", &leading.to_string());
        xml += &prop("SignalHardwareRecordLength", &rows.to_string());
        xml += &prop("XStart", "-2e-3");
        xml += &prop("XStop", "2e-3");
        xml += &prop("NofQuantisationLevels", "128");
        xml += more;
        xml + " </Group>\n</Database>\n"
    }

    /// One channel: 0.5 V/div with a step factor of 8 over 128 levels is 31.25 mV a step; position 1 div and
    /// offset 0.25 V put a raw 0 at -0.25 V.
    fn one_channel(source: &str) -> String {
        format!("  <Prop Name=\"VerticalScale\" Value=\"0.5\" StepFactor=\"8\"/>\n{}{}{}", prop("VerticalPosition", "1"), prop("VerticalOffset", "0.25"),
            prop("Source", source))
    }

    fn data(code: u32, rows: usize, body: &[u8]) -> Vec<u8> {
        let mut out = code.to_le_bytes().to_vec();
        out.extend_from_slice(&(rows as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn words(raw: &[i16]) -> Vec<u8> {
        raw.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn a_sixteen_bit_record_is_scaled_and_keeps_only_the_rows_of_the_record() {
        // two settling rows before the record and two after it
        let xml = description("INT16BIT", 4, 2, 8, &one_channel("eRS_SIGNAL_SOURCE_CH2_TR1"));
        let traces = parse_rs(&xml, &data(1, 8, &words(&[999, 999, 32, -32, 0, 64, 999, 999]))).unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!((t.v.as_slice(), t.t0, t.dt, t.label.as_str()), (&[0.75f32, -1.25, -0.25, 1.75][..], -2e-3, 1e-3, "CH2"));
    }

    #[test]
    fn an_eight_bit_record_is_scaled_the_same_way() {
        let xml = description("INT8BIT", 3, 0, 3, &one_channel("C1W1"));
        let t = &parse_rs(&xml, &data(0, 3, &[32, 0xE0, 0])).unwrap()[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&[0.75f32, -1.25, -0.25][..], "CH1"));
    }

    #[test]
    fn float_samples_are_volts_as_they_stand() {
        let xml = description("FLOAT", 3, 1, 4, &prop("Source", "eRS_SIGNAL_SOURCE_CH4_TR1"));
        let body: Vec<u8> = [9.0f32, 0.5, -0.5, 0.25].iter().flat_map(|x| x.to_le_bytes()).collect();
        let t = &parse_rs(&xml, &data(4, 4, &body)).unwrap()[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&[0.5f32, -0.5, 0.25][..], "CH4"));
    }

    #[test]
    fn a_file_of_time_and_value_rows_takes_its_time_from_the_rows() {
        let xml = description("XYDOUBLEFLOAT", 3, 0, 3, &prop("Source", "eRS_SIGNAL_SOURCE_CH1_TR1"));
        let mut body = Vec::new();
        for (time, volts) in [(0.5f64, 1.0f32), (0.75, 2.0), (1.0, 3.0)] {
            body.extend_from_slice(&time.to_le_bytes());
            body.extend_from_slice(&volts.to_le_bytes());
        }
        let t = &parse_rs(&xml, &data(6, 3, &body)).unwrap()[0];
        assert_eq!((t.v.as_slice(), t.t0, t.dt), (&[1.0f32, 2.0, 3.0][..], 0.5, 0.25));
    }

    /// Channels 1 and 3 of a multi-channel export, channel 2 off: two columns, each with its own scaling.
    fn two_of_three() -> String {
        let per = |name: &str, a: &str, b: &str, c: &str, more: &str| format!("  <Prop Name=\"{name}\" I_0=\"{a}\" I_1=\"{b}\" I_2=\"{c}\"{more}/>\n");
        description("INT16BIT", 2, 0, 2, &[
            prop("MultiChannelExport", "eRS_ONOFF_ON"),
            per("MultiChannelExportState", "eRS_ONOFF_ON", "eRS_ONOFF_OFF", "eRS_ONOFF_ON", ""),
            per("MultiChannelSource", "eRS_SIGNAL_SOURCE_CH1_TR1", "eRS_SIGNAL_SOURCE_CH2_TR1", "eRS_SIGNAL_SOURCE_CH3_TR1", ""),
            per("MultiChannelVerticalScale", "0.5", "1", "1", " StepFactor=\"8\""),
            per("MultiChannelVerticalPosition", "0", "0", "0", ""),
            per("MultiChannelVerticalOffset", "0", "0", "1", ""),
        ].concat())
    }

    #[test]
    fn the_channels_of_a_multi_channel_export_are_taken_apart_row_by_row() {
        // rows: (CH1, CH3), (CH1, CH3)
        let traces = parse_rs(&two_of_three(), &data(1, 2, &words(&[32, 16, -32, -16]))).unwrap();
        let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
        assert_eq!(seen, [("CH1", &[1.0f32, -1.0][..]), ("CH3", &[2.0f32, 0.0][..])]);
        assert!(traces.iter().all(|t| t.t0 == -2e-3 && t.dt == 2e-3));
    }

    #[test]
    fn what_is_not_one_plain_record_is_refused_by_name() {
        let plain = one_channel("eRS_SIGNAL_SOURCE_CH1_TR1");
        let body = data(1, 3, &words(&[1, 2, 3]));
        let refused = |more: &str| parse_rs(&description("INT16BIT", 3, 0, 3, &format!("{plain}{more}")), &body).unwrap_err();
        assert!(refused(&prop("TraceType", "eRS_TRACE_TYPE_ENVELOPE")).contains("envelope trace is not read"));
        assert!(refused(&prop("NumberOfAcquisitions", "5")).contains("several acquisitions"));
        assert!(parse_rs(&description("INT16BIT", 3, 0, 3, ""), &body).unwrap_err().contains("no usable VerticalScale"));
        assert!(parse_rs(&description("CUSTOM", 3, 0, 3, &plain), &body).unwrap_err().contains("CUSTOM is not read"));
        // a data file of another format, of another length, or cut short
        let good = description("INT16BIT", 3, 0, 3, &plain);
        assert!(parse_rs(&good, &data(0, 3, &words(&[1, 2, 3]))).unwrap_err().contains("does not belong"));
        assert!(parse_rs(&good, &data(1, 4, &words(&[1, 2, 3, 4]))).unwrap_err().contains("does not belong"));
        assert!(parse_rs(&good, &data(1, 3, &words(&[1, 2]))).unwrap_err().contains("truncated"));
        assert!(parse_rs(&good, &[1, 0, 0]).unwrap_err().contains("truncated"));
        // more settling rows than the file has
        assert!(parse_rs(&description("INT16BIT", 3, 2, 3, &plain), &body).unwrap_err().contains("no waveform"));
        assert!(parse_rs(&good, &body).is_ok());
    }

    #[test]
    fn the_first_property_of_a_name_wins_and_either_quote_is_read() {
        let tags = RsTags::read("<a Name='X' Value='1'/><b Name=\"X\" Value=\"2\"/><c Value=\"3\"/><!-- Name=\"Y\" --><d Name = \"Z\" Value = \"a=b\">");
        assert_eq!((tags.value("X"), tags.value("Y"), tags.value("Z"), tags.0.len()), (Some("1"), None, Some("a=b"), 2));
    }

    #[test]
    fn a_source_is_shown_by_its_channel() {
        let shown: Vec<String> = ["eRS_SIGNAL_SOURCE_CH3_TR2", "C2W1", "", "eRS_SIGNAL_SOURCE_NONE", "eRS_SIGNAL_SOURCE_MATH1"].iter().map(|s| rs_source_label(s)).collect();
        assert_eq!(shown, ["CH3", "CH2", "CH1", "CH1", "MATH1"]);
    }

    #[test]
    fn either_file_of_the_pair_opens_the_waveform_and_a_lone_one_says_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("oos-rs-pair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xml = description("INT16BIT", 4, 2, 8, &one_channel("eRS_SIGNAL_SOURCE_CH2_TR1"));
        let body = data(1, 8, &words(&[999, 999, 32, -32, 0, 64, 999, 999]));
        let (first, second) = (dir.join("capture.bin"), dir.join("capture.Wfm.bin"));
        std::fs::write(&first, &xml).unwrap();

        // the description by itself: named as one half, with the name of the other
        let alone = parse_wave_at(xml.as_bytes(), &first).unwrap_err();
        assert!(alone.contains("capture.Wfm.bin is not beside it"), "{alone}");
        assert!(parse_wave(xml.as_bytes(), "capture.bin").unwrap_err().contains("open it from its folder"));

        std::fs::write(&second, &body).unwrap();
        for (bytes, path) in [(xml.as_bytes(), &first), (body.as_slice(), &second)] {
            let traces = parse_wave_at(bytes, path).unwrap();
            assert_eq!((traces.len(), traces[0].v.as_slice(), traces[0].label.as_str()), (1, &[0.75f32, -1.25, -0.25, 1.75][..], "CH2"));
        }

        // the data by itself
        std::fs::remove_file(&first).unwrap();
        let alone = parse_wave_at(&body, &second).unwrap_err();
        assert!(alone.contains("its description capture.bin is not beside it"), "{alone}");

        // every other file is read as before
        let csv = dir.join("plain.csv");
        std::fs::write(&csv, "t,v\n0,1\n1e-3,2\n2e-3,3\n").unwrap();
        assert_eq!(parse_wave_at(&std::fs::read(&csv).unwrap(), &csv).unwrap()[0].v, [1.0, 2.0, 3.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ── Siglent: binary waveform files ───────────────────────────────────────────────────────────────────────────

pub(super) fn sg_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

pub(super) fn sg_f64(b: &[u8], at: usize) -> Option<f64> {
    b.get(at..at.checked_add(8)?).map(|s| {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(s);
        f64::from_le_bytes(raw)
    })
}

/// A value with its unit, 40 bytes: an f64, the index of its prefix (8 is none, 7 milli, 9 kilo, in steps of a
/// thousand from yocto at 0 to yotta at 16) and seven words that describe the unit. Returned without the prefix.
pub(super) fn sg_value(b: &[u8], at: usize) -> Option<f64> {
    let (value, prefix, unit) = (sg_f64(b, at)?, sg_u32(b, at + 8)?, sg_u32(b, at + 12)?);
    if !value.is_finite() || prefix > 16 || unit > 12 {
        return None;
    }
    let thousands = prefix as i32 - 8;
    // a division by a whole power of ten is exact to the last bit; a product with its inverse is not
    Some(if thousands >= 0 { value * 10f64.powi(3 * thousands) } else { value / 10f64.powi(-3 * thousands) })
}

/// The version of a Siglent binary waveform file: its first word, and a header that holds what that version
/// puts there. Versions 2, 3 and 4 are one fixed header and the channels after it; version 6 is a list of
/// records. The files of older firmware, which start without a version, are not recognized.
pub(super) fn siglent_version(b: &[u8]) -> Option<u32> {
    let version = sg_u32(b, 0)?;
    let flags = |at: usize| (0..4).all(|k| sg_u32(b, at + 4 * k).is_some_and(|f| f <= 1));
    let flag_byte = |at: usize| b.get(at).is_some_and(|x| *x <= 1);
    let known = match version {
        2 | 3 => b.len() >= 0x800 && flags(0x04) && sg_value(b, 0x198).is_some() && sg_value(b, 0x1EC).is_some() && flag_byte(0x260),
        4 => sg_u32(b, 4).is_some_and(|data| (0x1000..=b.len()).contains(&(data as usize))) && flags(0x08)
            && sg_value(b, 0x19C).is_some() && sg_value(b, 0x1F0).is_some() && flag_byte(0x264) && flag_byte(0x265),
        6 => b.len() >= 108 && u16::from_le_bytes([b[4], b[5]]) >= 108 && sg_u32(b, 0x68).is_some_and(|n| n > 0) && b[8].is_ascii_graphic(),
        _ => false,
    };
    known.then_some(version)
}

/// The unsigned samples of a block, `width` bytes each.
pub(super) fn sg_codes(block: &[u8], width: usize, big: bool) -> impl Iterator<Item = f64> + '_ {
    block.chunks_exact(width).map(move |c| match (width, big) {
        (1, _) => c[0] as f64,
        (2, false) => u16::from_le_bytes([c[0], c[1]]) as f64,
        (2, true) => u16::from_be_bytes([c[0], c[1]]) as f64,
        _ => u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64,
    })
}

/// A Siglent binary waveform file (SDS800X HD to SDS7000A, SDS2000X Plus, SDS5000X, SDS6000). Addresses are
/// those of versions 2 and 3; version 4 has the address of its data at 0x04 and every later field 4 bytes on.
/// 0x04 four channels on; 0x14 volts per division and 0xB4 vertical offset, a 40-byte value each; 0x198 time per
/// division; 0x1C0 delay; 0x1E8 the samples of a channel; 0x1EC sample rate; 0x240 probe factors (f64);
/// 0x260 one byte, 1 for 16-bit samples; 0x261 one byte, 1 for the high byte first; 0x268 horizontal divisions;
/// 0x26C codes per division. The channels that are on follow from 0x800, one after the other, unsigned:
/// volts = ((code - centre) * volts per division / codes per division - offset) * probe, the centre 128 or 32768,
/// and the first sample is at -(time per division * divisions / 2) - delay.
/// Layout from Siglent's "How to Extract Data from the Binary File"; the sign of the offset and the place of the
/// probe factor from the RigolWFM project, which found the document's own formula wrong on SDS800X HD files.
/// Checked on that project's SDS814X HD files: channels with a 1x and a 10x probe, a math trace and a zoom save.
/// Digital data, which follows the math traces, and channels 5 to 8 are not read.
pub(super) fn parse_siglent(b: &[u8], version: u32) -> Result<Vec<WaveTrace>, String> {
    if version == 6 {
        return parse_siglent_records(b);
    }
    let short = || "truncated Siglent waveform file".to_string();
    let shift = if version == 4 { 4 } else { 0 };
    let word = |at: usize| sg_u32(b, at + shift).ok_or_else(short);
    let value = |at: usize, what: &str| sg_value(b, at + shift).ok_or_else(|| format!("the Siglent file has no usable {what}"));
    let data = if version == 4 { sg_u32(b, 4).ok_or_else(short)? as usize } else { 0x800 };

    let points = word(0x1E8)? as usize;
    if points > MAX_WAVEFORM_SAMPLES {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    let rate = value(0x1EC, "sample rate")?;
    // a save of a math trace alone has no channel on, and need not say how long a channel is
    let channels = (0..4).any(|k| sg_u32(b, 0x04 + shift + 4 * k) == Some(1));
    if channels && (points < 2 || !(rate > 0.0)) {
        return Err("that file holds no waveform".into());
    }
    let flag_byte = |at: usize| b.get(at + shift).copied().ok_or_else(short);
    let (width, big) = (if flag_byte(0x260)? == 1 { 2 } else { 1 }, flag_byte(0x261)? == 1);
    let centre = if width == 2 { 32768.0 } else { 128.0 };
    // a header of version 2 that leaves these two empty is of a scope with 14 divisions and 25 codes in each
    let divisions = match word(0x268)? {
        n @ 1..=20 => n as f64,
        _ if version == 2 => 14.0,
        _ => 10.0,
    };
    let mut t0 = -(value(0x198, "time per division")? * divisions / 2.0) - value(0x1C0, "delay")?;
    if version == 4 && data >= 0xB48 && sg_u32(b, 0xAF4) == Some(1) {
        // a zoom window saved by itself has its own time base, its delay counted the other way
        if let (Some(per), Some(delay)) = (sg_value(b, 0xAF8), sg_value(b, 0xB20)) {
            t0 = delay - per * divisions / 2.0;
        }
    }
    if !t0.is_finite() {
        return Err("the Siglent file has no usable time per division".into());
    }

    let mut out = Vec::new();
    let mut at = data;
    for k in 0..4 {
        if word(0x04 + 4 * k)? != 1 {
            continue;
        }
        let per = value(0x14 + 0x28 * k, "vertical scale")?;
        let offset = value(0xB4 + 0x28 * k, "vertical offset")?;
        let probe = sg_f64(b, 0x240 + shift + 8 * k).filter(|p| p.is_finite() && *p > 0.0).unwrap_or(1.0);
        let codes = match word(0x26C + 4 * k)? as i32 {
            n if n > 0 => n as f64,
            _ if version == 2 => 25.0,
            _ => return Err("the Siglent file has no usable codes per division".into()),
        };
        let block = at.checked_add(points * width).and_then(|end| b.get(at..end)).ok_or_else(short)?;
        at += points * width;
        let step = per / codes;
        let v: Vec<f32> = sg_codes(block, width, big).map(|code| (((code - centre) * step - offset) * probe) as f32).collect();
        if v.iter().any(|x| !x.is_finite()) {
            return Err("the Siglent file has no usable vertical scale".into());
        }
        out.push(WaveTrace { t0, dt: 1.0 / rate, v, label: format!("CH{}", k + 1) });
    }
    // the math traces that are on follow the channels: 0x27C four on; 0x28C volts per division and 0x32C
    // position, a 40-byte value each; 0x3CC their samples; 0x3DC the time between samples (f64); 0x3FC the codes
    // per division of them all. Scaled as a channel is, without a probe factor. A math trace whose header or
    // samples are not all there is left out; the channels before it stand.
    for k in 0..4 {
        if version < 3 || sg_u32(b, 0x27C + shift + 4 * k) != Some(1) {
            continue;
        }
        let (Some(per), Some(position)) = (sg_value(b, 0x28C + shift + 0x28 * k), sg_value(b, 0x32C + shift + 0x28 * k)) else { break };
        let (Some(points), Some(dt), Some(codes)) = (sg_u32(b, 0x3CC + shift + 4 * k), sg_f64(b, 0x3DC + shift + 8 * k), sg_u32(b, 0x3FC + shift)) else { break };
        let (points, codes) = (points as usize, codes as i32);
        if points > MAX_WAVEFORM_SAMPLES || codes <= 0 || !(dt > 0.0) || !dt.is_finite() {
            break;
        }
        let Some(block) = at.checked_add(points * width).and_then(|end| b.get(at..end)) else { break };
        at += points * width;
        if points < 2 {
            continue;
        }
        let step = per / codes as f64;
        let v: Vec<f32> = sg_codes(block, width, big).map(|code| ((code - centre) * step - position) as f32).collect();
        if v.iter().all(|x| x.is_finite()) {
            out.push(WaveTrace { t0, dt, v, label: format!("F{}", k + 1) });
        }
    }
    if out.is_empty() {
        return Err("the Siglent file holds no channel and no math trace".into());
    }
    Ok(out)
}

/// A Siglent binary waveform file of version 6: a header of `header_bytes` (u16 at 0x04) with the number of
/// records (u32 at 0x68), then each record: a header whose size is its u32 at 0x04, the extra bytes counted by
/// its u32 at 0xF0, and the samples. In a record header: 0x0E channel index (u16); 0x10 label; 0x60 the sample
/// at time zero and 0x68 the time between samples; 0xB0 the code of zero volts and 0xB8 the volts of one code
/// (f64 each); 0xF8 the number of samples and 0x100 their bytes (u64 each). The samples are unsigned.
/// Layout from Siglent's document, the reading of the samples from the RigolWFM project.
pub(super) fn parse_siglent_records(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let short = || "truncated Siglent waveform file".to_string();
    let wide = |at: usize| sg_f64(b, at).map(f64::to_bits).ok_or_else(short);
    let records = sg_u32(b, 0x68).ok_or_else(short)?;
    let mut at = u16::from_le_bytes([b[4], b[5]]) as usize;
    let mut out = Vec::new();
    for _ in 0..records {
        let head = sg_u32(b, at + 4).ok_or_else(short)? as usize;
        if head < 264 {
            return Err("that Siglent waveform record is not read".into());
        }
        let extra = sg_u32(b, at + 0xF0).ok_or_else(short)? as usize;
        let (number, bytes) = (wide(at + 0xF8)?, wide(at + 0x100)?);
        if number > MAX_WAVEFORM_SAMPLES as u64 {
            return Err("waveform exceeds the 32-million-sample import limit".into());
        }
        let (number, bytes) = (number as usize, usize::try_from(bytes).map_err(|_| short())?);
        let start = at + head + extra;
        let block = start.checked_add(bytes).and_then(|end| b.get(start..end)).ok_or_else(short)?;
        let record = at;
        at = start + bytes;
        if number < 2 {
            continue;
        }
        let width = bytes / number;
        if bytes % number != 0 || ![1, 2, 4].contains(&width) {
            return Err("that Siglent sample size is not read".into());
        }
        let field = |offset: usize| sg_f64(b, record + offset).filter(|x| x.is_finite()).ok_or_else(|| "the Siglent file has no usable scale".to_string());
        let (zero_at, dt, zero_code, step) = (field(0x60)?, field(0x68)?, field(0xB0)?, field(0xB8)?);
        if !(dt > 0.0) || step == 0.0 {
            return Err("the Siglent file has no usable scale".into());
        }
        let named: String = b[record + 0x10..record + 0x20].iter().take_while(|c| **c != 0).filter(|c| c.is_ascii_graphic()).map(|c| *c as char).take(10).collect();
        let label = if named.is_empty() { format!("CH{}", u16::from_le_bytes([b[record + 0x0E], b[record + 0x0F]]).max(1)) } else { named };
        let v: Vec<f32> = sg_codes(block, width, false).map(|code| ((code - zero_code) * step) as f32).collect();
        out.push(WaveTrace { t0: -zero_at * dt, dt, v, label });
    }
    if out.is_empty() {
        return Err("that file holds no waveform".into());
    }
    Ok(out)
}

#[cfg(test)]
mod siglent_file_tests {
    use super::*;

    fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
        b[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// A value in volts with a prefix: the unit words say "V to the power 1".
    fn put_value(b: &mut [u8], at: usize, value: f64, prefix: u32) {
        put(b, at, &value.to_le_bytes());
        for (k, word) in [prefix, 0, 1, 1, 0, 1, 0, 1].iter().enumerate() {
            put(b, at + 8 + 4 * k, &word.to_le_bytes());
        }
    }

    /// A header of version 2, 3 or 4: 500 mV per division, 0.25 V offset, 32 codes per division, 2 s per division
    /// over 10 divisions, 0.5 s delay and 1 kSa/s. Then the data.
    fn file(version: u32, on: [u32; 4], points: u32, body: &[u8]) -> Vec<u8> {
        let (shift, data) = if version == 4 { (4, 0x1000) } else { (0, 0x800) };
        let mut b = vec![0u8; data];
        put(&mut b, 0, &version.to_le_bytes());
        if version == 4 {
            put(&mut b, 4, &(data as u32).to_le_bytes());
        }
        for k in 0..4 {
            put(&mut b, shift + 0x04 + 4 * k, &on[k].to_le_bytes());
            put_value(&mut b, shift + 0x14 + 0x28 * k, 500.0, 7);
            put_value(&mut b, shift + 0xB4 + 0x28 * k, 0.25, 8);
            put(&mut b, shift + 0x240 + 8 * k, &1.0f64.to_le_bytes());
            put(&mut b, shift + 0x26C + 4 * k, &32u32.to_le_bytes());
        }
        put_value(&mut b, shift + 0x198, 2.0, 8);
        put_value(&mut b, shift + 0x1C0, 500.0, 7);
        put(&mut b, shift + 0x1E8, &points.to_le_bytes());
        put_value(&mut b, shift + 0x1EC, 1.0, 9);
        put(&mut b, shift + 0x268, &10u32.to_le_bytes());
        b.extend_from_slice(body);
        b
    }

    #[test]
    fn the_channels_that_are_on_follow_one_another_and_are_scaled() {
        // CH2 and CH4 on: three samples of the one, then three of the other
        for version in [2, 3, 4] {
            let traces = parse_wave(&file(version, [0, 1, 0, 1], 3, &[160, 128, 96, 128, 192, 64]), "s.bin").unwrap();
            let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
            assert_eq!(seen, [("CH2", &[0.25f32, -0.25, -0.75][..]), ("CH4", &[-0.25f32, 0.75, -1.25][..])], "version {version}");
            assert!(traces.iter().all(|t| t.t0 == -10.5 && t.dt == 1e-3), "version {version}");
        }
    }

    #[test]
    fn sixteen_bit_samples_are_read_in_the_byte_order_the_header_names() {
        for (big, shift, version) in [(false, 4, 4), (true, 4, 4), (true, 0, 3)] {
            let codes = [32800u16, 32768, 32736];
            let body: Vec<u8> = codes.iter().flat_map(|c| if big { c.to_be_bytes() } else { c.to_le_bytes() }).collect();
            let mut b = file(version, [1, 0, 0, 0], 3, &body);
            b[shift + 0x260] = 1;
            b[shift + 0x261] = big as u8;
            assert_eq!(parse_wave(&b, "s.bin").unwrap()[0].v, [0.25, -0.25, -0.75], "version {version}, high byte first {big}");
        }
    }

    #[test]
    fn the_probe_factor_multiplies_what_the_header_scales_at_the_input() {
        let mut b = file(4, [1, 0, 0, 0], 3, &[160, 128, 96]);
        put(&mut b, 4 + 0x240, &10.0f64.to_le_bytes());
        assert_eq!(parse_wave(&b, "s.bin").unwrap()[0].v, [2.5, -2.5, -7.5]);
    }

    #[test]
    fn an_old_header_without_its_counts_is_of_a_scope_with_25_codes_and_14_divisions() {
        let mut b = file(2, [1, 0, 0, 0], 3, &[153, 128, 103]);
        put(&mut b, 0x268, &[0; 4]);
        put(&mut b, 0x26C, &[0; 4]);
        let t = &parse_wave(&b, "s.bin").unwrap()[0];
        assert_eq!((t.v.as_slice(), t.t0), (&[0.25f32, -0.25, -0.75][..], -14.5));
        // a later version has to say them
        put(&mut b, 0, &3u32.to_le_bytes());
        assert!(parse_wave(&b, "s.bin").unwrap_err().contains("codes per division"));
    }

    #[test]
    fn a_zoom_window_saved_by_itself_is_placed_by_its_own_time_base() {
        let mut b = file(4, [1, 0, 0, 0], 3, &[160, 128, 96]);
        put(&mut b, 0xAF4, &1u32.to_le_bytes());
        put_value(&mut b, 0xAF8, 1.0, 8);
        put_value(&mut b, 0xB20, 3.0, 8);
        assert_eq!(parse_wave(&b, "s.bin").unwrap()[0].t0, -2.0);
    }

    #[test]
    fn a_file_without_its_samples_or_without_a_channel_is_refused() {
        assert!(parse_wave(&file(4, [1, 1, 0, 0], 3, &[160, 128, 96, 1, 2]), "s.bin").unwrap_err().contains("truncated"));
        assert!(parse_wave(&file(4, [0, 0, 0, 0], 3, &[160, 128, 96]), "s.bin").unwrap_err().contains("no channel and no math trace"));
        assert!(parse_wave(&file(3, [1, 0, 0, 0], 1, &[160]), "s.bin").unwrap_err().contains("no waveform"));
        // a first word that is a version, over a header that is not one of that version
        let mut b = file(4, [1, 0, 0, 0], 3, &[160, 128, 96]);
        put(&mut b, 0x08, &7u32.to_le_bytes());
        assert_eq!(siglent_version(&b), None);
        assert_eq!(siglent_version(&file(3, [1, 0, 0, 0], 3, &[1, 2, 3])), Some(3));
        assert_eq!(siglent_version(&5u32.to_le_bytes()), None);
    }

    /// Switches math trace F2 on: 2 V per division, position -1 V, four samples 0.5 s apart, 32 codes per division.
    fn with_math(mut b: Vec<u8>, shift: usize, samples: &[u8]) -> Vec<u8> {
        put(&mut b, shift + 0x27C + 4, &1u32.to_le_bytes());
        put_value(&mut b, shift + 0x28C + 0x28, 2.0, 8);
        put_value(&mut b, shift + 0x32C + 0x28, -1.0, 8);
        put(&mut b, shift + 0x3CC + 4, &4u32.to_le_bytes());
        put(&mut b, shift + 0x3DC + 8, &0.5f64.to_le_bytes());
        put(&mut b, shift + 0x3FC, &32u32.to_le_bytes());
        b.extend_from_slice(samples);
        b
    }

    #[test]
    fn a_math_trace_follows_the_channels_and_is_read_alone_too() {
        // after one channel
        for (version, shift) in [(3, 0), (4, 4)] {
            let traces = parse_wave(&with_math(file(version, [1, 0, 0, 0], 3, &[160, 128, 96]), shift, &[144, 128, 112, 96]), "s.bin").unwrap();
            let seen: Vec<(&str, &[f32], f64)> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice(), t.dt)).collect();
            assert_eq!(seen, [("CH1", &[0.25f32, -0.25, -0.75][..], 1e-3), ("F2", &[2.0f32, 1.0, 0.0, -1.0][..], 0.5)], "version {version}");
            assert!(traces.iter().all(|t| t.t0 == -10.5));
        }
        // saved by itself: no channel is on
        let alone = parse_wave(&with_math(file(4, [0, 0, 0, 0], 0, &[]), 4, &[144, 128, 112, 96]), "s.bin").unwrap();
        assert_eq!((alone.len(), alone[0].label.as_str(), alone[0].v.as_slice()), (1, "F2", &[2.0f32, 1.0, 0.0, -1.0][..]));
        // cut short, the math trace is left out and the channel stands; a version 2 file has none
        let cut = parse_wave(&with_math(file(4, [1, 0, 0, 0], 3, &[160, 128, 96]), 4, &[144, 128]), "s.bin").unwrap();
        assert_eq!((cut.len(), cut[0].label.as_str()), (1, "CH1"));
        assert_eq!(parse_wave(&with_math(file(2, [1, 0, 0, 0], 3, &[160, 128, 96]), 0, &[144, 128, 112, 96]), "s.bin").unwrap().len(), 1);
    }

    /// One record of a version 6 file: 264 header bytes, four extra, then the samples.
    fn record(label: &str, index: u16, width: usize, codes: &[u32]) -> Vec<u8> {
        let mut r = vec![0u8; 264 + 4];
        put(&mut r, 0x04, &264u32.to_le_bytes());
        put(&mut r, 0x0E, &index.to_le_bytes());
        put(&mut r, 0x10, label.as_bytes());
        put(&mut r, 0x60, &2.0f64.to_le_bytes());
        put(&mut r, 0x68, &0.5f64.to_le_bytes());
        put(&mut r, 0xB0, &128.0f64.to_le_bytes());
        put(&mut r, 0xB8, &0.25f64.to_le_bytes());
        put(&mut r, 0xF0, &4u32.to_le_bytes());
        put(&mut r, 0xF8, &(codes.len() as u64).to_le_bytes());
        put(&mut r, 0x100, &((codes.len() * width) as u64).to_le_bytes());
        for code in codes {
            r.extend_from_slice(&code.to_le_bytes()[..width]);
        }
        r
    }

    fn records(list: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 108];
        put(&mut b, 0, &6u32.to_le_bytes());
        put(&mut b, 4, &108u16.to_le_bytes());
        put(&mut b, 8, b"SDS1202X-E");
        put(&mut b, 0x68, &(list.len() as u32).to_le_bytes());
        for r in list {
            b.extend_from_slice(r);
        }
        b
    }

    #[test]
    fn a_file_of_records_gives_a_trace_for_each_record() {
        let b = records(&[record("C1", 1, 1, &[130, 128, 126]), record("", 2, 2, &[132, 128, 124])]);
        let traces = parse_wave(&b, "s.bin").unwrap();
        let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
        assert_eq!(seen, [("C1", &[0.5f32, 0.0, -0.5][..]), ("CH2", &[1.0f32, 0.0, -1.0][..])]);
        assert!(traces.iter().all(|t| t.t0 == -1.0 && t.dt == 0.5));
        // cut short inside the second record, and a sample size that is none
        assert!(parse_wave(&b[..b.len() - 1], "s.bin").unwrap_err().contains("truncated"));
        let mut odd = record("C1", 1, 1, &[130, 128, 126]);
        put(&mut odd, 0xF8, &2u64.to_le_bytes());
        assert!(parse_wave(&records(&[odd]), "s.bin").unwrap_err().contains("sample size"));
    }

    #[test]
    fn the_csv_file_of_a_siglent_scope_is_read_past_its_settings() {
        let text = "Record Length,Analog:14000\nSample Rate,100000000.0\nVertical Scale,CH1: 0.05\nVertical Offset,CH1:0.00000\nHorizontal Scale, 0.000010000\nSecond,Volt\n-0.00007000000,-0.19400\n-0.00006999000,-0.19200,\n-0.00006998000,-0.19000,\n";
        let t = &parse_wave(text.as_bytes(), "siglent.csv").unwrap()[0];
        assert_eq!(t.v, [-0.194, -0.192, -0.19]);
        assert!((t.t0 + 7e-5).abs() < 1e-18 && (t.dt - 1e-8).abs() < 1e-14);
    }
}

// ── Inflate (RFC 1951): the compressed blocks of a Rigol DHO waveform file ───────────────────────────────────

/// Bit reader of a deflate stream: values LSB first.
pub(super) struct BitIn<'a> {
    data: &'a [u8],
    at: usize,
    hold: u32,
    held: u32,
}

impl BitIn<'_> {
    /// The next `n` bits, at most 16.
    fn bits(&mut self, n: u32) -> Option<u32> {
        while self.held < n {
            self.hold |= (*self.data.get(self.at)? as u32) << self.held;
            self.at += 1;
            self.held += 8;
        }
        let value = self.hold & ((1u32 << n) - 1);
        self.hold >>= n;
        self.held -= n;
        Some(value)
    }
}

/// A canonical Huffman code: how many codes each length has, and the symbols in the order of their codes.
pub(super) struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// From the code length of each symbol, 0 for a symbol without a code. None when the lengths ask for more
    /// codes than there are.
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut count = [0u16; 16];
        for &length in lengths {
            count[(length & 15) as usize] += 1;
        }
        count[0] = 0;
        let mut left = 1i32;
        for &n in &count[1..] {
            left = (left << 1) - n as i32;
            if left < 0 {
                return None;
            }
        }
        let mut next = [0u16; 16];
        for length in 1..15 {
            next[length + 1] = next[length] + count[length];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (s, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let at = &mut next[(length & 15) as usize];
                symbol[*at as usize] = s as u16;
                *at += 1;
            }
        }
        Some(Self { count, symbol })
    }

    /// The symbol of the code that comes next, read a bit at a time, the first bit the highest.
    fn decode(&self, bits: &mut BitIn) -> Option<usize> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for &n in &self.count[1..] {
            code |= bits.bits(1)? as i32;
            let n = n as i32;
            if code - n < first {
                return self.symbol.get((index + code - first) as usize).map(|s| *s as usize);
            }
            index += n;
            first = (first + n) << 1;
            code <<= 1;
        }
        None
    }
}

/// The two codes of a block with dynamic Huffman codes (RFC 1951 3.2.7): for literals and lengths, and for distances.
pub(super) fn inflate_codes(bits: &mut BitIn) -> Option<(Huffman, Huffman)> {
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let (literals, distances, coded) = (bits.bits(5)? as usize + 257, bits.bits(5)? as usize + 1, bits.bits(4)? as usize + 4);
    if literals > 286 || distances > 30 {
        return None;
    }
    let mut of_lengths = [0u8; 19];
    for &at in &ORDER[..coded] {
        of_lengths[at] = bits.bits(3)? as u8;
    }
    let of_lengths = Huffman::new(&of_lengths)?;
    let mut lengths = [0u8; 286 + 30];
    let mut n = 0;
    while n < literals + distances {
        let (length, times) = match of_lengths.decode(bits)? {
            symbol @ 0..=15 => (symbol as u8, 1),
            16 => (*lengths.get(n.checked_sub(1)?)?, 3 + bits.bits(2)? as usize),
            17 => (0, 3 + bits.bits(3)? as usize),
            _ => (0, 11 + bits.bits(7)? as usize),
        };
        if n + times > literals + distances {
            return None;
        }
        lengths[n..n + times].fill(length);
        n += times;
    }
    if lengths[256] == 0 {
        return None; // no code for the end of the block
    }
    Some((Huffman::new(&lengths[..literals])?, Huffman::new(&lengths[literals..literals + distances])?))
}

/// The bytes a deflate stream stands for, at most `limit` of them, and how many bytes of `data` the stream took.
/// None for a stream that is damaged, cut short or longer than the limit.
pub(super) fn inflate(data: &[u8], limit: usize) -> Option<(Vec<u8>, usize)> {
    let mut bits = BitIn { data, at: 0, hold: 0, held: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = bits.bits(1)? == 1;
        match bits.bits(2)? {
            0 => {
                // stored: from the next whole byte, a length, its complement and the bytes
                (bits.hold, bits.held) = (0, 0);
                let head = data.get(bits.at..bits.at.checked_add(4)?)?;
                let (length, check) = (u16::from_le_bytes([head[0], head[1]]), u16::from_le_bytes([head[2], head[3]]));
                let block = data.get(bits.at + 4..bits.at + 4 + length as usize)?;
                if length != !check || out.len() + block.len() > limit {
                    return None;
                }
                out.extend_from_slice(block);
                bits.at += 4 + length as usize;
            }
            kind @ (1 | 2) => {
                let (literals, distances) = if kind == 2 {
                    inflate_codes(&mut bits)?
                } else {
                    let mut fixed = [8u8; 288];
                    fixed[144..256].fill(9);
                    fixed[256..280].fill(7);
                    (Huffman::new(&fixed)?, Huffman::new(&[5u8; 30])?)
                };
                loop {
                    let symbol = literals.decode(&mut bits)?;
                    if symbol == 256 {
                        break;
                    }
                    if symbol < 256 {
                        if out.len() >= limit {
                            return None;
                        }
                        out.push(symbol as u8);
                        continue;
                    }
                    let (base, extra) = *LEN_BASE.get(symbol - 257)?;
                    let length = base as usize + bits.bits(extra as u32)? as usize;
                    let (base, extra) = *DIST_BASE.get(distances.decode(&mut bits)?)?;
                    let distance = base as usize + bits.bits(extra as u32)? as usize;
                    if distance > out.len() || out.len() + length > limit {
                        return None;
                    }
                    for _ in 0..length {
                        out.push(out[out.len() - distance]);
                    }
                }
            }
            _ => return None,
        }
        if last {
            return Some((out, bits.at));
        }
    }
}

/// The bytes of a zlib stream (RFC 1950): two header bytes, a deflate stream and the Adler-32 of what it holds.
pub(super) fn zlib_inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    if data.len() < 6 || data[0] & 0x0F != 8 || u16::from_be_bytes([data[0], data[1]]) % 31 != 0 || data[1] & 0x20 != 0 {
        return None;
    }
    let (out, taken) = inflate(&data[2..], limit)?;
    let sum = data.get(2 + taken..2 + taken + 4)?;
    (u32::from_be_bytes([sum[0], sum[1], sum[2], sum[3]]) == adler32(&out)).then_some(out)
}

// ── Rigol: waveform files of the scope's own format (.wfm) ───────────────────────────────────────────────────

/// What a Rigol `.wfm` file is, by its first four bytes.
pub(super) enum RigolWfm {
    /// DHO800, DHO900 and DHO1000: settings in compressed blocks, 16-bit samples.
    Dho,
    /// DS1000E: one fixed header, 8-bit samples.
    E,
    /// A family whose file is known by its first bytes and not read.
    Unread(&'static str),
}

pub(super) fn rigol_wfm_kind(b: &[u8]) -> Option<RigolWfm> {
    match b.get(..4)? {
        [2, 0, 0, 0] if rigol_dho_blocks(b).is_some_and(|(blocks, _)| blocks.iter().any(|block| matches!(block.kind, 5 | 9))) => Some(RigolWfm::Dho),
        [0xA5, 0xA5, 0, 0] if b.len() >= 276 => Some(RigolWfm::E),
        [1, 0xFF, 0xFF, 0xFF] if b.get(4) == Some(&0xA5) => Some(RigolWfm::Unread("DS1000Z and MSO1000Z")),
        [0xA5, 0xA5, 0x38, 0] => Some(RigolWfm::Unread("DS2000, DS4000 and DS6000")),
        [0xA5, 0xA5, 0xA4, 1] => Some(RigolWfm::Unread("DS1000B")),
        [0xA1, 0xA5, 0, 0] => Some(RigolWfm::Unread("DS1000C")),
        _ => None,
    }
}

pub(super) fn parse_rigol_wfm(b: &[u8], kind: RigolWfm) -> Result<Vec<WaveTrace>, String> {
    match kind {
        RigolWfm::Dho => parse_rigol_dho(b),
        RigolWfm::E => parse_rigol_e(b),
        // a DS1000Z file is left unread on purpose: no known reading of it gives the vertical offset the scope
        // showed, and the project this layout comes from says to check its result against a measurement
        RigolWfm::Unread(family) if family.starts_with("DS1000Z") => {
            Err(format!("RIGOL {family} waveform files (.wfm) are not read: the vertical offset cannot be told from the file; save the waveform as CSV"))
        }
        RigolWfm::Unread(family) => Err(format!("RIGOL {family} waveform files (.wfm) are not read; save the waveform as CSV")),
    }
}

/// A settings block of a DHO waveform file: its number, its kind, the size its header names for the settings and
/// the bytes the file holds for them. The list of a file borrows the file: nothing is decompressed for it.
pub(super) struct DhoBlock<'a> {
    id: u16,
    kind: u16,
    size: usize,
    content: &'a [u8],
}

/// The most one settings block may decompress to, and the most the blocks read of one file may hold together.
/// The header of a block names the size of its settings in 16 bits, so a block of a scope is taken to stay under
/// 64 KiB (assumed: no file of a scope was at hand); 4 MiB is 64 blocks of that size, where a file has four
/// channels. A file beyond it is refused: a list of 4096 blocks of 1 MiB each is 4 GiB to decompress.
pub(super) const DHO_BLOCK_MAX: usize = 1 << 20;
pub(super) const DHO_SETTINGS_MAX: usize = 4 << 20;

impl DhoBlock<'_> {
    /// The settings of the block, decompressed. A block whose header names two different sizes is a zlib stream:
    /// one that does not decompress is an error, not settings. A block whose two sizes are the same and which is
    /// no stream is read as it stands (assumed: the source of the layout reads such a block so, and does not say
    /// whether a scope writes one).
    fn settings(&self) -> Result<Vec<u8>, String> {
        match zlib_inflate(self.content, DHO_BLOCK_MAX) {
            Some(settings) => Ok(settings),
            None if self.size == self.content.len() => Ok(self.content.to_vec()),
            None => Err("the RIGOL file has settings that do not decompress".into()),
        }
    }
}

/// The settings blocks of a DHO waveform file, and where they end. They start at byte 24, each with a header of
/// six u16: number, kind, size decompressed, size compressed, bytes taken in the file, zero. The first
/// `compressed` bytes are a zlib stream, or the settings themselves (`DhoBlock::settings`); a block that takes no
/// bytes ends the list. Only the headers are read here: this runs for every file that starts like a DHO file,
/// while what the file is is still being found out.
pub(super) fn rigol_dho_blocks(b: &[u8]) -> Option<(Vec<DhoBlock<'_>>, usize)> {
    let mut blocks = Vec::new();
    let mut at = 24usize;
    loop {
        let head = b.get(at..at + 12)?;
        let word = |k: usize| u16::from_le_bytes([head[2 * k], head[2 * k + 1]]);
        let (compressed, taken) = (word(3) as usize, word(4) as usize);
        if compressed == 0 && taken == 0 {
            return Some((blocks, at));
        }
        if compressed > taken || blocks.len() >= 4096 {
            return None;
        }
        let content = b.get(at + 12..at + 12 + taken)?;
        blocks.push(DhoBlock { id: word(0), kind: word(1), size: word(2) as usize, content: &content[..compressed] });
        at += 12 + taken;
    }
}

/// The frame header of a DHO waveform file, if it starts at `at`: the samples of one channel, the channels and
/// the ticks between samples. The samples it counts have to be in the file; the error says what is missing.
pub(super) fn rigol_dho_frame(b: &[u8], at: usize) -> Result<(usize, usize, u32), String> {
    let short = || "truncated RIGOL waveform file".to_string();
    let total = sg_f64(b, at).ok_or_else(short)?.to_bits();
    let (ticks, hint) = (sg_u32(b, at + 16).ok_or_else(short)?, sg_u32(b, at + 24).ok_or_else(short)? as u64);
    if total == 0 || total > 4 * MAX_WAVEFORM_SAMPLES as u64 {
        return Err("that file holds no waveform".into());
    }
    // the count of all channels can run a few samples over: the channels are the nearest whole number
    let (points, channels) = if hint > 0 { (hint, (total + hint / 2) / hint) } else { (total, 1) };
    if points < 2 || !(1..=4).contains(&channels) {
        return Err("that file holds no waveform".into());
    }
    if points > MAX_WAVEFORM_SAMPLES as u64 {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    // no interval is put in the place of one the file does not give: the times of the trace would be made up
    if ticks == 0 || ticks > 1_000_000_000 {
        return Err("the RIGOL file has no usable sample interval".into());
    }
    let (points, channels) = (points as usize, channels as usize);
    if b.len().saturating_sub(at + 40) < points * channels * 2 {
        return Err(short());
    }
    Ok((points, channels, ticks))
}

/// A waveform file of a RIGOL DHO800, DHO900 or DHO1000. RIGOL does not publish this layout; it is the one the
/// RigolWFM project worked out and checked against the scope's own BIN export of the same captures.
/// The settings blocks (`rigol_dho_blocks`) hold the vertical scale: in the block numbered as its channel, of kind
/// 5 (DHO800) or 9 (DHO1000), an i64 at byte 1 and the centre at byte 38 (i32 or i64):
/// DHO800: volts of one code = i64 / 7.5e12, centre = -i32 / 1e9, volts = code * volts of one code + centre - 32768 codes;
/// DHO1000: volts of one code = i64 / 7.5e11, centre = i64 / 1e8, volts = code * volts of one code - centre - 32768 codes.
/// After the blocks and the zeros that follow them comes the frame header: u64 samples of all channels; at +16
/// the time between samples as a u32 count of 0.8 ns (DHO800) or 10 ns (DHO1000), 0 or more than 1e9 being no
/// interval; at +24 the samples of one channel; at +40 the samples, u16, the channels taking turns.
/// How many zeros there are is not known (the source of the layout skips them up to the first byte that is not
/// zero), and a count that is a multiple of 256, as 4 000 000 is, begins with zeros of its own: the first byte
/// that is not zero is then one to seven bytes into the count. So the count is looked for at each of the eight
/// places it can begin at, the earliest first, and it is where the samples it counts are all in the file: one
/// byte too early it reads 256 times as many. Assumed, not checked on a file of a scope: that nothing as long
/// as 255 times the samples follows them.
/// The channels are taken to be CH1 onwards, and the middle sample to be the trigger: the file says neither
/// where the sources looked.
pub(super) fn parse_rigol_dho(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let short = || "truncated RIGOL waveform file".to_string();
    let (blocks, at) = rigol_dho_blocks(b).ok_or_else(short)?;
    let dho800 = blocks.iter().any(|block| block.kind == 5 && (1..=4).contains(&block.id));
    let mut scales: Vec<(u16, f64, f64)> = Vec::new();
    let mut held = 0usize;
    for block in blocks.iter().filter(|block| block.kind == if dho800 { 5 } else { 9 } && (1..=4).contains(&block.id)) {
        let settings = block.settings()?;
        held += settings.len();
        if held > DHO_SETTINGS_MAX {
            return Err("the RIGOL file has more than 4 MiB of channel settings".into());
        }
        let Some(number) = sg_f64(&settings, 1).map(|x| x.to_bits() as i64 as f64) else { continue };
        let scale = if dho800 {
            sg_u32(&settings, 38).map(|centre| (number / 7.5e12, -(centre as i32 as f64) / 1e9))
        } else {
            sg_f64(&settings, 38).map(|centre| (number / 7.5e11, -(centre.to_bits() as i64 as f64) / 1e8))
        };
        if let Some((step, centre)) = scale.filter(|(step, _)| *step != 0.0) {
            scales.push((block.id, step, centre - step * 32768.0));
        }
    }
    let Some(&(_, first_step, first_zero)) = scales.first() else {
        return Err("the RIGOL file has no usable vertical scale".into());
    };
    let mut filled = at;
    while b.get(filled) == Some(&0) {
        filled += 1;
    }
    // the place the count begins at: the last one tried is the first byte that is not zero, and says why not
    let mut frame = Err(short());
    for start in filled.saturating_sub(7).max(at)..=filled {
        frame = rigol_dho_frame(b, start).map(|found| (start, found));
        if frame.is_ok() {
            break;
        }
    }
    let (at, (points, channels, ticks)) = frame?;
    let dt = ticks as f64 * if dho800 { 8e-10 } else { 1e-8 };
    let t0 = -(points as f64 / 2.0) * dt;
    let samples = &b[at + 40..];
    Ok((0..channels).map(|k| {
        let (step, zero) = scales.iter().find(|(id, _, _)| *id as usize == k + 1).map_or((first_step, first_zero), |&(_, step, zero)| (step, zero));
        let v = samples.chunks_exact(2 * channels).take(points).map(|row| (u16::from_le_bytes([row[2 * k], row[2 * k + 1]]) as f64 * step + zero) as f32).collect();
        WaveTrace { t0, dt, v, label: format!("CH{}", k + 1) }
    }).collect())
}

/// A waveform file of a RIGOL DS1000E. One header of 276 bytes, little endian: 20 roll stop (u32); 28 the samples
/// of CH1 (u32); a channel header at 34 (CH1) and 58 (CH2), in it +10 probe factor (f32), +15 on (u8), +18
/// microvolts per division at the input (i32) and +22 position in codes (i16); 100 sample rate (f32, Hz);
/// 112 time of the middle of the record (i64, ps); 232 the samples of CH2 (u32), 0 for as many as CH1.
/// The samples follow, CH1 then CH2, 8-bit, 25 codes a division, a higher code a lower voltage:
/// volts = (125 - code - position) * volts per division * probe / 25. In roll mode the last `roll stop + 2`
/// bytes of a channel are not samples. Layout from the RigolWFM project. A DS1000D file has the same bytes and
/// its scale already holds the probe factor; it is read as a DS1000E file. The logic channels, and the second
/// time base of an alternate trigger, are not read.
pub(super) fn parse_rigol_e(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let short = || "truncated RIGOL waveform file".to_string();
    let word = |at: usize| sg_u32(b, at).ok_or_else(short);
    let float = |at: usize| word(at).map(|bits| f32::from_bits(bits) as f64);
    let depth = [word(28)? as usize, match word(232)? { 0 => word(28)?, n => n } as usize];
    let on = [b[34 + 15] != 0, b[58 + 15] != 0];
    let held: usize = (0..2).filter(|k| on[*k]).map(|k| depth[k]).sum();
    // a DS1000C file has the same first bytes and a shorter header
    if b.len() == 256 + held || b.len() == 272 + held {
        return Err("RIGOL DS1000C waveform files (.wfm) are not read".into());
    }
    let rate = float(100)?;
    if !(rate > 0.0) || !rate.is_finite() {
        return Err("the RIGOL file has no usable sample rate".into());
    }
    let middle = sg_f64(b, 112).ok_or_else(short)?.to_bits() as i64 as f64 * 1e-12;
    let skip = match word(20)? as usize {
        0 => 0,
        stop => stop + 2,
    };
    let mut out = Vec::new();
    let mut at = 276usize;
    for k in 0..2 {
        if !on[k] {
            continue;
        }
        if depth[k] > MAX_WAVEFORM_SAMPLES {
            return Err("waveform exceeds the 32-million-sample import limit".into());
        }
        let head = 34 + 24 * k;
        let block = at.checked_add(depth[k]).and_then(|end| b.get(at..end)).ok_or_else(short)?;
        at += depth[k];
        let points = depth[k].saturating_sub(skip);
        if points < 2 {
            continue;
        }
        let step = 1e-6 * (word(head + 18)? as i32 as f64) * float(head + 10)? / 25.0;
        let position = i16::from_le_bytes([b[head + 22], b[head + 23]]) as f64;
        let v: Vec<f32> = block[..points].iter().map(|code| ((125.0 - *code as f64 - position) * step) as f32).collect();
        if step == 0.0 || v.iter().any(|x| !x.is_finite()) {
            return Err("the RIGOL file has no usable vertical scale".into());
        }
        out.push(WaveTrace { t0: middle - depth[k] as f64 / rate / 2.0, dt: 1.0 / rate, v, label: format!("CH{}", k + 1) });
    }
    if out.is_empty() {
        return Err("that file holds no waveform".into());
    }
    Ok(out)
}

#[cfg(test)]
mod rigol_wfm_file_tests {
    use super::*;

    /// "scale 0 and offset 0; scale 7 and offset 5; ..." forty times over: 893 bytes that compress well.
    fn text() -> Vec<u8> {
        (0..40).map(|i| format!("scale {} and offset {}; ", i * 7 % 13, i * 5 % 11)).collect::<String>().into_bytes()
    }

    /// A zlib stream of one stored block.
    fn stored(data: &[u8]) -> Vec<u8> {
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend_from_slice(&(data.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(data.len() as u16)).to_le_bytes());
        z.extend_from_slice(data);
        z.extend_from_slice(&adler32(data).to_be_bytes());
        z
    }

    #[test]
    fn the_three_kinds_of_deflate_block_are_inflated() {
        // fixed codes, with a match that runs over itself: "abc" four times, as zlib level 9 writes it
        let fixed = [120, 218, 75, 76, 74, 78, 132, 33, 0, 29, 224, 4, 153];
        assert_eq!(zlib_inflate(&fixed, 64).unwrap(), b"abcabcabcabc");
        // dynamic codes, as zlib level 9 writes the text
        let dynamic = [
            120, 218, 117, 210, 77, 10, 2, 49, 12, 64, 225, 171, 244, 8, 105, 250, 143, 167, 25, 116, 102, 53, 232, 162, 222, 31, 65, 72, 201, 200,
            115, 251, 8, 52, 31, 205, 188, 111, 231, 30, 36, 108, 207, 71, 120, 29, 199, 220, 223, 65, 110, 97, 126, 107, 243, 181, 88, 141, 190, 198,
            53, 220, 125, 206, 86, 213, 215, 97, 117, 248, 154, 172, 38, 95, 251, 122, 238, 178, 155, 90, 206, 190, 182, 53, 124, 93, 206, 114, 241,
            181, 174, 97, 69, 117, 69, 181, 176, 186, 161, 58, 162, 186, 163, 90, 81, 61, 16, 157, 24, 45, 136, 206, 140, 142, 136, 46, 252, 213, 202,
            234, 138, 106, 65, 117, 67, 117, 68, 117, 71, 181, 162, 122, 32, 58, 49, 154, 239, 59, 51, 250, 207, 129, 23, 254, 106, 190, 240, 138, 106,
            249, 81, 127, 0, 152, 61, 27, 73,
        ];
        assert_eq!(zlib_inflate(&dynamic, 4096).unwrap(), text());
        // stored
        assert_eq!(zlib_inflate(&stored(&text()), 4096).unwrap(), text());
    }

    #[test]
    fn a_stream_that_is_damaged_cut_short_or_too_long_is_none() {
        let good = stored(&text());
        assert!(zlib_inflate(&good, 892).is_none(), "longer than the limit");
        assert!(zlib_inflate(&good[..good.len() - 5], 4096).is_none(), "cut short");
        let mut sum = good.clone();
        *sum.last_mut().unwrap() ^= 1;
        assert!(zlib_inflate(&sum, 4096).is_none(), "wrong check sum");
        let mut length = good.clone();
        length[5] ^= 1;
        assert!(zlib_inflate(&length, 4096).is_none(), "length and its complement disagree");
        // a match that reaches back before the start, a block of the kind that does not exist, no stream at all
        assert!(inflate(&[0x03, 0x02, 0x00], 64).is_none());
        assert!(inflate(&[0x07], 64).is_none());
        assert!(zlib_inflate(&[], 64).is_none());
        assert!(zlib_inflate(b"plain settings, not a stream", 64).is_none());
    }

    fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
        b[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// The settings of one channel: 0.1 mV a code, and the volts the middle code stands for.
    fn channel_settings(dho800: bool, centre: f64) -> Vec<u8> {
        let mut p = vec![0u8; 64];
        if dho800 {
            put(&mut p, 1, &750_000_000i64.to_le_bytes());
            put(&mut p, 38, &((-centre * 1e9) as i32).to_le_bytes());
        } else {
            put(&mut p, 1, &75_000_000i64.to_le_bytes());
            put(&mut p, 38, &((-centre * 1e8) as i64).to_le_bytes());
        }
        p
    }

    /// A DHO file: its blocks, some zeros, the frame header and the samples of `channels` channels taking turns.
    fn dho(blocks: &[(u16, u16, Vec<u8>, bool)], channels: u64, ticks: u32, codes: &[u16]) -> Vec<u8> {
        let mut b = vec![0u8; 24];
        b[0] = 2;
        for (id, kind, payload, compress) in blocks {
            let content = if *compress { stored(payload) } else { payload.clone() };
            let taken = content.len() + 3; // the file pads a block
            for word in [*id, *kind, payload.len() as u16, content.len() as u16, taken as u16, 0] {
                b.extend_from_slice(&word.to_le_bytes());
            }
            b.extend_from_slice(&content);
            b.extend_from_slice(&[0; 3]);
        }
        b.extend_from_slice(&[0; 12 + 24]); // the block that ends the list, and the zeros before the frame
        let points = codes.len() as u64 / channels;
        let mut frame = vec![0u8; 40];
        put(&mut frame, 0, &(points * channels).to_le_bytes());
        put(&mut frame, 16, &ticks.to_le_bytes());
        put(&mut frame, 20, &0x4Du32.to_le_bytes());
        put(&mut frame, 24, &(points as u32).to_le_bytes());
        put(&mut frame, 28, &(points as u32).to_le_bytes());
        frame[32] = 1;
        b.extend_from_slice(&frame);
        b.extend(codes.iter().flat_map(|c| c.to_le_bytes()));
        b
    }

    fn close(seen: &[f32], wanted: &[f32]) -> bool {
        seen.len() == wanted.len() && seen.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn a_dho800_file_gives_its_channels_from_samples_that_take_turns() {
        // CH1 around 0.5 V and CH2 around -1 V, 500 ticks of 0.8 ns between samples, compressed settings
        let blocks = [(1, 5, channel_settings(true, 0.5), true), (300, 6, vec![7; 40], true), (2, 5, channel_settings(true, -1.0), false)];
        let b = dho(&blocks, 2, 500, &[32768, 32768, 42768, 22768, 22768, 42768, 32768, 32768]);
        let traces = parse_wave(&b, "dho.wfm").unwrap();
        assert_eq!(traces.len(), 2);
        assert!(close(&traces[0].v, &[0.5, 1.5, -0.5, 0.5]) && close(&traces[1].v, &[-1.0, -2.0, 0.0, -1.0]), "{:?}", (&traces[0].v, &traces[1].v));
        assert_eq!((traces[0].label.as_str(), traces[1].label.as_str()), ("CH1", "CH2"));
        assert!(traces.iter().all(|t| (t.dt - 4e-7).abs() < 1e-18 && (t.t0 + 8e-7).abs() < 1e-18));
    }

    #[test]
    fn a_dho1000_file_counts_ten_nanoseconds_and_a_few_samples_too_many() {
        let mut b = dho(&[(1, 9, channel_settings(false, 0.5), true)], 1, 3, &[32768, 42768, 22768, 32768]);
        // the count of all channels runs over the samples of the one channel
        let frame = b.len() - 8 - 40;
        put(&mut b, frame, &5u64.to_le_bytes());
        let t = &parse_wave(&b, "dho.wfm").unwrap()[0];
        assert!(close(&t.v, &[0.5, 1.5, -0.5, 0.5]), "{:?}", t.v);
        assert!((t.dt - 3e-8).abs() < 1e-20 && (t.t0 + 6e-8).abs() < 1e-20);
    }

    #[test]
    fn a_dho_file_whose_count_is_a_multiple_of_256_is_read_whole() {
        // 256 samples of one channel: the count begins with a zero byte, like the zeros before it
        let ramp: Vec<u16> = (0..256).map(|i| 32768 + 100 * i).collect();
        let b = dho(&[(1, 5, channel_settings(true, 0.0), true)], 1, 500, &ramp);
        let one = parse_wave(&b, "dho.wfm").unwrap();
        assert_eq!((one.len(), one[0].v.len()), (1, 256));
        assert!((one[0].v[255] - 2.55).abs() < 1e-5, "{}", one[0].v[255]);
        assert!((one[0].dt - 4e-7).abs() < 1e-18 && (one[0].t0 + 128.0 * 4e-7).abs() < 1e-15);
        // cut short it is refused, not read as the 256th part of itself
        assert!(parse_wave(&b[..b.len() - 1], "dho.wfm").is_err());
        // 65536 samples of two channels: two zero bytes, and the count of one channel begins with one as well
        let turns: Vec<u16> = (0..65536).map(|i| if i % 2 == 0 { 42768 } else { 22768 }).collect();
        let blocks = [(1, 5, channel_settings(true, 0.0), true), (2, 5, channel_settings(true, 0.0), true)];
        let two = parse_wave(&dho(&blocks, 2, 500, &turns), "dho.wfm").unwrap();
        assert_eq!(two.iter().map(|t| t.v.len()).collect::<Vec<_>>(), [32768, 32768]);
        assert!(two[0].v.iter().all(|v| (v - 1.0).abs() < 1e-5) && two[1].v.iter().all(|v| (v + 1.0).abs() < 1e-5));
    }

    #[test]
    fn a_dho_file_with_settings_that_are_too_large_or_do_not_decompress_is_refused() {
        let codes = [32768, 42768, 22768, 32768];
        // 70 channel blocks of 65 000 bytes that hold no scale: more than 4 MiB before a scale is found
        let mut many = vec![(1u16, 5u16, vec![0u8; 65_000], true); 70];
        let refused = parse_wave(&dho(&many, 1, 500, &codes), "dho.wfm").unwrap_err();
        assert!(refused.contains("4 MiB"), "{refused}");
        // 64 of them stay under it, and the scale after them is read
        many.truncate(64);
        many.push((1, 5, channel_settings(true, 0.5), true));
        assert!(close(&parse_wave(&dho(&many, 1, 500, &codes), "dho.wfm").unwrap()[0].v, &[0.5, 1.5, -0.5, 0.5]));
        // one bit of a compressed block changed: its bytes are not read as the settings
        let mut damaged = dho(&[(1, 5, channel_settings(true, 0.5), true)], 1, 500, &codes);
        assert!(parse_wave(&damaged, "dho.wfm").is_ok());
        damaged[24 + 12 + 10] ^= 1;
        let refused = parse_wave(&damaged, "dho.wfm").unwrap_err();
        assert!(refused.contains("do not decompress"), "{refused}");
        // a block that is used by nothing is not decompressed: a damaged one does not take the file with it
        let mut other = dho(&[(300, 6, vec![7; 40], true), (1, 5, channel_settings(true, 0.5), true)], 1, 500, &codes);
        other[24 + 12 + 10] ^= 1;
        assert!(parse_wave(&other, "dho.wfm").is_ok());
    }

    #[test]
    fn a_dho_file_without_a_sample_interval_is_refused() {
        // no ticks between samples, or more than the layout knows as an interval: no interval is made up
        for ticks in [0, 1_000_000_001] {
            let b = dho(&[(1, 5, channel_settings(true, 0.5), true)], 1, ticks, &[32768, 42768, 22768, 32768]);
            let refused = parse_wave(&b, "dho.wfm").unwrap_err();
            assert!(refused.contains("sample interval"), "{refused}");
        }
    }

    #[test]
    fn a_dho_file_without_its_scale_or_its_samples_is_refused() {
        let good = dho(&[(1, 5, channel_settings(true, 0.5), true)], 1, 500, &[32768, 42768, 22768, 32768]);
        assert!(parse_wave(&good, "dho.wfm").is_ok());
        assert!(parse_wave(&good[..good.len() - 1], "dho.wfm").unwrap_err().contains("truncated"));
        // a channel block that holds no scale
        let empty = dho(&[(1, 5, vec![0; 64], true)], 1, 500, &[32768, 42768, 22768, 32768]);
        assert!(parse_wave(&empty, "dho.wfm").unwrap_err().contains("vertical scale"));
        // no channel block at all: not taken for this kind of file
        assert!(rigol_wfm_kind(&dho(&[(300, 6, vec![7; 40], true)], 1, 500, &[1, 2])).is_none());
    }

    /// A DS1000E file: 1 V per division at the input of a 10x probe, 25 codes of position, 1 kSa/s, the middle
    /// of the record at 2 ms.
    fn e(on: [bool; 2], depth: u32, roll_stop: u32, data: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 276];
        put(&mut b, 0, &[0xA5, 0xA5, 0, 0]);
        put(&mut b, 20, &roll_stop.to_le_bytes());
        put(&mut b, 28, &depth.to_le_bytes());
        for k in 0..2 {
            let head = 34 + 24 * k;
            put(&mut b, head + 10, &10.0f32.to_le_bytes());
            b[head + 15] = on[k] as u8;
            put(&mut b, head + 18, &100_000i32.to_le_bytes());
            put(&mut b, head + 22, &(25i16 * (1 - 2 * k as i16)).to_le_bytes());
        }
        put(&mut b, 100, &1000.0f32.to_le_bytes());
        put(&mut b, 112, &2_000_000_000i64.to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn a_ds1000e_file_gives_its_channels_one_after_the_other() {
        // a higher code is a lower voltage; CH1 sits a division up, CH2 a division down
        let traces = parse_wave(&e([true, true], 4, 0, &[100, 75, 125, 150, 150, 125, 175, 200]), "e.wfm").unwrap();
        assert_eq!(traces.len(), 2);
        assert!(close(&traces[0].v, &[0.0, 1.0, -1.0, -2.0]) && close(&traces[1].v, &[0.0, 1.0, -1.0, -2.0]), "{:?}", (&traces[0].v, &traces[1].v));
        assert!(traces.iter().all(|t| t.dt == 1e-3 && (t.t0 - 0.0).abs() < 1e-12));
        // CH2 alone, and the bytes after a roll stop left out
        let alone = parse_wave(&e([false, true], 6, 1, &[150, 125, 175, 0, 0, 0]), "e.wfm").unwrap();
        assert_eq!((alone.len(), alone[0].label.as_str()), (1, "CH2"));
        assert!(close(&alone[0].v, &[0.0, 1.0, -1.0]) && (alone[0].t0 + 1e-3).abs() < 1e-12);
    }

    #[test]
    fn rigol_files_that_are_not_read_are_refused_by_family() {
        assert!(parse_wave(&e([true, false], 4, 0, &[1, 2, 3]), "e.wfm").unwrap_err().contains("truncated"));
        assert!(parse_wave(&e([false, false], 4, 0, &[]), "e.wfm").unwrap_err().contains("no waveform"));
        // the shorter header of a DS1000C: 256 bytes, then the 20 samples
        assert!(parse_wave(&[&e([true, false], 20, 0, &[])[..256], &[0u8; 20][..]].concat(), "c.wfm").unwrap_err().contains("DS1000C"));
        for (first, family) in [([1u8, 0xFF, 0xFF, 0xFF, 0xA5, 0xA5], "DS1000Z"), ([0xA5, 0xA5, 0x38, 0, 0, 0], "DS2000"), ([0xA5, 0xA5, 0xA4, 1, 0, 0], "DS1000B"), ([0xA1, 0xA5, 0, 0, 0, 0], "DS1000C")] {
            let refused = parse_wave(&[&first[..], &[0u8; 64][..]].concat(), "r.wfm").unwrap_err();
            assert!(refused.contains(family) && refused.contains("not read"), "{refused}");
        }
    }
}

#[cfg(test)]
mod rigol_csv_file_tests {
    use super::*;

    fn close(seen: f64, wanted: f64) -> bool {
        (seen - wanted).abs() <= wanted.abs() * 1e-9
    }

    /// The lines of each test are the text of files saved by real scopes, from the RigolWFM project's captures.
    fn read(lines: &[&str], end: &str) -> Result<Vec<WaveTrace>, String> {
        let text: String = lines.iter().map(|line| format!("{line}{end}")).collect();
        parse_wave(text.as_bytes(), "rigol.csv")
    }

    #[test]
    fn a_ds1000z_file_counts_samples_and_gives_their_time_in_its_header() {
        // CH1 and CH4 of a DS1104Z, lines ended by a line feed
        let traces = read(&["X,CH1,CH4,Start,Increment,", "Sequence,Volt,Volt,-1.200000e-03,4.000000e-08", "0,2.86e+00,3.08e+00,", "1,2.86e+00,3.08e+00,", "2,2.84e+00,3.10e+00,"], "\n").unwrap();
        let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
        assert_eq!(seen, [("CH1", &[2.86f32, 2.86, 2.84][..]), ("CH4", &[3.08f32, 3.08, 3.10][..])]);
        assert!(traces.iter().all(|t| close(t.t0, -1.2e-3) && close(t.dt, 4e-8)), "{:?}", (traces[0].t0, traces[0].dt));
        // four channels of a DS1054Z, lines ended by a carriage return and a line feed
        let four = read(&["X,CH1,CH2,CH3,CH4,Start,Increment,", "Sequence,Volt,Volt,Volt,Volt,-3.000000e-07,5.000000e-10", "0,3.28e+00,9.60e-01,-1.60e-01,0.00e+00,", "1,3.28e+00,9.60e-01,-1.60e-01,0.00e+00,", "2,3.20e+00,9.60e-01,-8.00e-02,0.00e+00,"], "\r\n").unwrap();
        assert_eq!(four.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH2", "CH3", "CH4"]);
        assert_eq!(four[2].v, [-0.16, -0.16, -0.08]);
        assert!(four.iter().all(|t| close(t.t0, -3e-7) && close(t.dt, 5e-10)));
    }

    #[test]
    fn a_ds2000_and_a_ds4000_file_are_read_the_same_way() {
        // one channel of a DS2072A: its second line ends with a comma
        let one = read(&["X,CH1,Start,Increment,", "Sequence,Volt,-3.500000e-03,5.000000e-06,", "0,2.400000e-02,", "1,8.000000e-03,", "2,2.400000e-02,"], "\r\n").unwrap();
        assert_eq!((one.len(), one[0].label.as_str(), one[0].v.as_slice()), (1, "CH1", &[0.024f32, 0.008, 0.024][..]));
        assert!(close(one[0].t0, -3.5e-3) && close(one[0].dt, 5e-6));
        // a DS4024 starts its count at 22: the first sample is 22 steps after the start
        let late = read(&["X,CH1,CH2,Start,Increment,", "Sequence,Volt,Volt,-1.400000e-03,2.000000e-06,", "22,3.125000e-02,6.250000e-03,", "23,-6.250000e-02,-6.250000e-03,", "24,3.125000e-02,6.250000e-03,"], "\r\n").unwrap();
        assert!(close(late[0].t0, -1.4e-3 + 22.0 * 2e-6) && close(late[0].dt, 2e-6), "{:?}", (late[0].t0, late[0].dt));
        assert_eq!(late[1].v, [0.00625, -0.00625, 0.00625]);
    }

    #[test]
    fn a_file_with_a_time_column_keeps_its_times_and_takes_the_channels_of_its_header() {
        // DS1102E: seconds in the first column
        let e = read(&["X,CH1,", "Second,Volt,", "-5.9999998e-06,4.40000e+00,", "-5.9800000e-06,4.32000e+00,", "-5.9600002e-06,4.24000e+00,"], "\r\n").unwrap();
        assert_eq!((e.len(), e[0].label.as_str(), e[0].v.as_slice()), (1, "CH1", &[4.4f32, 4.32, 4.24][..]));
        assert!(close(e[0].t0, -5.9999998e-06) && (e[0].dt - 2e-8).abs() < 1e-12);
        // DS1204B: no name over the first column, CH2 and CH4 on, a comma and a space at the end of a row
        let b = read(&[",CH2,CH4", "Second,Volt,Volt", "1.63720e-02,1.48e+01,1.36e+01, ", "1.63760e-02,1.48e+01,1.36e+01, ", "1.63800e-02,2.08e+01,1.36e+01, "], "\r\n").unwrap();
        assert_eq!(b.iter().map(|t| (t.label.as_str(), t.v.len())).collect::<Vec<_>>(), [("CH2", 3), ("CH4", 3)]);
        // DS1102D: the unit in the name of a column, no second header line
        let d = read(&["X,CH 1 (V),CH 2 (V)", "-0.00468800,8.08,8.40", "-0.00467799,4.88,8.40", "-0.00466798,4.88,8.40"], "\n").unwrap();
        assert_eq!(d.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH2"]);
        assert_eq!(d[0].v, [8.08, 4.88, 4.88]);
    }

    #[test]
    fn a_file_that_counts_samples_without_saying_their_time_is_refused() {
        let refused = read(&["X,CH1,Start,Increment,", "Sequence,Volt,", "0,2.400000e-02,", "1,8.000000e-03,"], "\n").unwrap_err();
        assert!(refused.contains("Start and Increment"), "{refused}");
        assert!(read(&["X,CH1,Start,Increment,", "Sequence,Volt,-3.5e-03,0", "0,1,", "1,2,"], "\n").is_err());
        // another program's file with the same first cell is read as before
        let other = read(&["X,Y", "0,1", "1,2", "2,3"], "\n").unwrap();
        assert_eq!((other.len(), other[0].label.as_str(), other[0].dt), (1, "rigol.csv", 1.0));
    }

    #[test]
    fn start_and_increment_are_found_under_their_names() {
        // made for this test, not the text of a scope's file: a column of another trace between the channels and
        // Start. The time of a sample is the header's, not a second a sample
        let header = ["X,CH1,MATH,Start,Increment,", "Sequence,Volt,Volt,-1.200000e-03,4.000000e-08"];
        let rows = ["0,2.86e+00,1.00e+00,", "1,2.86e+00,1.00e+00,", "2,2.84e+00,1.00e+00,"];
        let traces = read(&[&header[..], &rows[..]].concat(), "\n").unwrap();
        assert_eq!((traces.len(), traces[1].v.as_slice()), (2, &[1.0f32, 1.0, 1.0][..]));
        assert!(traces.iter().all(|t| close(t.t0, -1.2e-3) && close(t.dt, 4e-8)), "{:?}", (traces[0].t0, traces[0].dt));
        // one of the two names, or a first column that says it counts samples, without both values: refused
        let half = ["X,CH1,Start,", "Sequence,Volt,-3.5e-03,", "0,1,", "1,2,"];
        let counted = ["X,CH1,", "Sequence,Volt,", "0,1,", "1,2,"];
        for lines in [half, counted] {
            let refused = read(&lines, "\n").unwrap_err();
            assert!(refused.contains("Start and Increment"), "{refused}");
        }
    }

    #[test]
    fn a_first_line_that_is_too_long_or_too_wide_is_no_rigol_header() {
        // one line of a million cells is not taken apart
        assert!(rigol_csv_header(&format!("X{}", ",CH1".repeat(1_000_000))).unwrap().is_none());
        // 4096 bytes are a line, one more is not
        let padded = |spaces: usize| format!("X,CH1,{}\n0,1\n1,2\n", " ".repeat(spaces));
        let channels = rigol_csv_header(&padded(RIGOL_CSV_LINE - 6)).unwrap().map(|header| header.channels);
        assert_eq!(channels, Some(vec!["CH1".to_string()]));
        assert!(rigol_csv_header(&padded(RIGOL_CSV_LINE - 5)).unwrap().is_none());
        // 64 cells are a header, one more is not
        let wide = |cells: usize| format!("X,CH1{}\n0,1\n1,2\n", ",".repeat(cells - 2));
        assert!(rigol_csv_header(&wide(RIGOL_CSV_CELLS)).unwrap().is_some());
        assert!(rigol_csv_header(&wide(RIGOL_CSV_CELLS + 1)).unwrap().is_none());
        // such a file is read as any other text file
        assert_eq!(parse_wave(wide(RIGOL_CSV_CELLS + 1).as_bytes(), "wide.csv").unwrap()[0].label, "wide.csv");
    }

    #[test]
    fn a_ds1000z_waveform_file_is_refused_with_the_way_round_it() {
        let refused = parse_wave(&[&[1u8, 0xFF, 0xFF, 0xFF, 0xA5, 0xA5][..], &[0u8; 64][..]].concat(), "z.wfm").unwrap_err();
        assert!(refused.contains("DS1000Z") && refused.contains("vertical offset") && refused.contains("save the waveform as CSV"), "{refused}");
    }
}

#[cfg(test)]
mod csv_time_step_tests {
    use super::*;

    #[test]
    fn the_time_between_samples_is_taken_over_the_whole_column() {
        // 3600 samples over 500 us, the times written with five digits, as a capture saved by Probewright has them:
        // the first step alone reads 140 ns, the true one is 138.9 ns
        let mut text = String::from("time_s,voltage_v\n");
        let step = 5e-4 / 3600.0;
        for i in 0..3600 {
            text += &format!("{:.4e},{}\n", -2.5e-4 + i as f64 * step, i % 7);
        }
        let t = &parse_wave(text.as_bytes(), "capture.csv").unwrap()[0];
        assert_eq!(t.v.len(), 3600);
        assert!((t.dt / step - 1.0).abs() < 2e-5, "{} against {step}", t.dt);
        assert_eq!(t.t0, -2.5e-4);
    }

    #[test]
    fn two_first_lines_at_the_same_rounded_time_are_still_a_waveform() {
        // a RIGOL DS1042C writes three digits: its first lines repeat a time
        let text = "X,CH1\nSecond,Volt,\n-5.11e-03,2.00e-02\n-5.11e-03,2.00e-02\n-5.10e-03,4.00e-02\n-5.10e-03,4.00e-02\n-5.09e-03,2.00e-02\n";
        let t = &parse_wave(text.as_bytes(), "c.csv").unwrap()[0];
        assert_eq!(t.v.len(), 5);
        assert!((t.dt - 5e-6).abs() < 1e-12, "{}", t.dt);
        // one line, or times that do not advance, are not a waveform
        assert!(parse_wave(b"0,1\n", "one.csv").is_err());
        assert!(parse_wave(b"1,1\n1,2\n1,3\n", "flat.csv").is_err());
        assert!(parse_wave(b"3,1\n2,2\n1,3\n", "back.csv").is_err());
    }

    #[test]
    fn a_file_of_two_sweeps_takes_its_step_from_one_of_them() {
        // two sweeps of 100 lines a microsecond apart, each from -50 us: the last time of the file less its first
        // is the length of one sweep, over the lines of two
        let mut text = String::from("time_s,voltage_v\n");
        for i in 0..200 {
            text += &format!("{:.6e},{}\n", -5e-5 + (i % 100) as f64 * 1e-6, i % 7);
        }
        let t = &parse_wave(text.as_bytes(), "sweeps.csv").unwrap()[0];
        assert_eq!((t.v.len(), t.t0), (200, -5e-5));
        assert!((t.dt / 1e-6 - 1.0).abs() < 1e-9, "{}", t.dt);
        // a sweep of 11 lines before one of 3600, the times written with five digits: the longer one says the
        // step, the short one alone is off by eight parts in ten thousand
        let step = 5e-4 / 3600.0;
        let mut text = String::new();
        for i in (0..11).chain(0..3600) {
            text += &format!("{:.4e},{}\n", -2.5e-4 + i as f64 * step, i % 7);
        }
        let t = &parse_wave(text.as_bytes(), "sweeps.csv").unwrap()[0];
        assert_eq!((t.v.len(), t.t0), (3611, -2.5e-4));
        assert!((t.dt / step - 1.0).abs() < 2e-5, "{} against {step}", t.dt);
        // a first sweep whose lines all have one time, then one that advances
        let t = &parse_wave(b"5,1\n5,2\n5,3\n0,1\n2,2\n", "flat.csv").unwrap()[0];
        assert_eq!((t.v.len(), t.t0, t.dt), (5, 5.0, 2.0));
    }
}
