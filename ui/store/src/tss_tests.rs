// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The tests of the zip reader (`zip.rs`) and of the session archive of another instrument (`tss.rs`): every archive
//! is built here (`testkit::zip`, `testkit::tss`), none is a file of an instrument. The last test walks a folder of
//! real archives when one is named, and is skipped without.

use super::testkit::*;
use super::*;
use crate::zip::Zip;

/// 600 bytes that are not all alike.
fn bytes() -> Vec<u8> {
    (0..600u32).map(|k| (k * k / 7 + k) as u8).collect()
}

/// Where the list of entries of an archive begins: at its first entry.
fn list_at(zip: &[u8]) -> usize {
    zip.windows(4).position(|w| w == b"PK\x01\x02").unwrap()
}

fn refused(zip: &[u8], limit: usize) -> String {
    match Zip::open(zip) {
        Ok(z) => z.entries.iter().map(|e| z.read(e, limit)).find_map(Result::err).expect("the archive is read"),
        Err(e) => e,
    }
}

#[test]
fn an_archive_is_read_stored_deflated_with_late_sizes_and_under_a_folder() {
    let data = bytes();
    let late = ZipFile { late: true, ..ZipFile::stored("late.bin", &data) };
    let early = ZipFile { late: false, ..ZipFile::deflated("early.bin", &data) };
    let files = [ZipFile::stored("plain.txt", b"as it is"), ZipFile::deflated("folder/packed.bin", &data), late, early];
    let archive = zip(&files);
    let z = Zip::open(&archive).unwrap();
    let names: Vec<&str> = z.entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["plain.txt", "folder/packed.bin", "late.bin", "early.bin"]);
    assert_eq!(z.read(&z.entries[0], 64).unwrap(), b"as it is");
    for e in &z.entries[1..] {
        assert_eq!(z.read(e, data.len()).unwrap(), data, "{}", e.name);
    }
    // a comment after the end record, and bytes in front of the archive that its offsets do not count
    let mut comment = archive.clone();
    let n = comment.len();
    comment[n - 2] = 5;
    comment.extend_from_slice(b"hello");
    assert_eq!(Zip::open(&comment).unwrap().entries.len(), 4);
    // an archive in the archive opens the same way, from the bytes of its entry
    let nested = zip(&[ZipFile::deflated("inner.zip", &archive)]);
    let outer = Zip::open(&nested).unwrap();
    let inner = outer.read(&outer.entries[0], archive.len()).unwrap();
    assert_eq!(Zip::open(&inner).unwrap().entries.len(), 4);
}

#[test]
fn a_damaged_or_unusual_archive_is_refused_with_its_reason() {
    let data = bytes();
    let good = zip(&[ZipFile::stored("a.bin", &data), ZipFile::deflated("b.bin", &data)]);
    let list = list_at(&good);
    let end = good.len() - 22;
    let with = |at: usize, patch: &[u8]| {
        let mut b = good.clone();
        b[at..at + patch.len()].copy_from_slice(patch);
        b
    };
    // cut anywhere: an error, never a panic and never the entries of a shorter archive
    for cut in 0..good.len() {
        assert!(Zip::open(&good[..cut]).is_err(), "cut at {cut}");
    }
    assert!(refused(&with(40, &[0xFF]), 4096).contains("check sum"), "a byte of a stored entry changed");
    assert!(refused(&with(list - 30, &[0xFF, 0xFF]), 4096).contains("damaged"), "the deflate stream changed");
    assert!(refused(&good, 599).contains("import limit"), "an entry of 600 bytes and a limit of 599");
    assert_eq!(refused(&with(end + 10, &[0xFF, 0xFF]), 4096), "zip64 archives are not read");
    assert_eq!(refused(&with(list + 24, &[0xFF; 4]), 4096), "zip64 archives are not read", "a size of all ones");
    assert!(refused(&with(list + 8, &[1]), 4096).contains("encrypted"));
    assert!(refused(&with(list + 10, &[12]), 4096).contains("method 12"));
    assert!(refused(&with(end + 4, &[1]), 4096).contains("several parts"));
    assert!(refused(&with(list + 42, &[0xF0, 0xFF, 0, 0]), 4096).contains("cut short"), "an entry that begins past the list");
    assert!(refused(&with(list + 20, &[0xFF, 0xFF, 0, 0]), 4096).contains("cut short"), "more bytes than there are");
    assert!(refused(&with(list + 24, &[0x59, 2, 0, 0]), 4096).contains("cut short"), "stored, with two sizes");
    // sizes that ask for more than the stream gives, or less
    let second = list + 46 + 5;
    assert!(refused(&with(second + 24, &[0x59, 2, 0, 0]), 4096).contains("does not unpack to its size"));
    assert!(refused(&with(second + 24, &[0x57, 2, 0, 0]), 4096).contains("does not unpack to its size"));
    for name in ["../up.bin", "/root.bin", "\\root.bin", "c:\\x.bin", "a/../../b.bin", "nul\0.bin"] {
        let odd = zip(&[ZipFile::stored(name, &data)]);
        assert!(refused(&odd, 4096).contains("leaves the archive"), "{name:?}");
    }
    let many: Vec<ZipFile> = (0..257).map(|_| ZipFile::stored("x", b"")).collect();
    assert!(refused(&zip(&many), 4096).contains("257 entries"));
    assert_eq!(Zip::open(&zip(&many[..256])).unwrap().entries.len(), 256);
    assert!(Zip::open(b"PK\x03\x04 and no more").is_err());
}

/// Four samples at 2 ns from -4 ns on, as a `:WFM#003` file.
fn wfm(v: [f32; 4]) -> Vec<u8> {
    wfm_f32(b":WFM#003", -4e-9, 2e-9, &v, "")
}

/// The same file marked as a logic waveform.
fn digital() -> Vec<u8> {
    let mut b = wfm([0.0, 1.0, 1.0, 0.0]);
    b[122] = 6;
    b
}

const SETUP: &str = "*RST;:PARAMBATCHING 0;:CH1:BANDWIDTH 500.0000E+6\n:CH2:LABEL:NAME \"Rail%203V3\"\n\
    :MEASUREMENT:MEAS1:CUSTOM \"a;b\nc\"\n:REF:REF1:LABEL:NAME \"\"\n:REF:REF3:LABEL:NAME \"Hall \"\"A\"\"\"\n";

#[test]
fn the_waveforms_of_a_session_archive_come_channels_first_then_the_references_by_number() {
    let refs = [(3, "hall.wfm", wfm([3.0; 4])), (1, "Edge.WFM", wfm([1.0; 4])), (2, "bits.wfm", digital())];
    let iq = ("ch1_iq.wfm", wfm([9.0; 4]));
    let beside = [("ch2.wfm", wfm([0.2; 4])), ("ch1.wfm", wfm([0.1; 4])), iq, ("ch3.wfm", digital())];
    let (traces, left_out) = parse_wave_noted(&tss(SETUP, &refs, &beside), "bench.tss").unwrap();
    let got: Vec<(&str, f32)> = traces.iter().map(|t| (t.label.as_str(), t.v[0])).collect();
    assert_eq!(got, [("ch1", 0.1), ("Rail 3V3", 0.2), ("Edge", 1.0), ("Hall \"A\"", 3.0)]);
    assert!(traces.iter().all(|t| t.v.len() == 4 && t.dt == 2e-9 && t.t0 == -4e-9));
    assert_eq!(left_out, "not read: 2 digital and 1 IQ waveforms");
    assert_eq!(parse_wave(&tss(SETUP, &refs, &beside), "bench.tss").unwrap().len(), 4);

    // under one folder, in the archive and in the archive inside it
    let ini = "recallSubsystem\\refMgr\\ref1\\sourceFileName=edge.wfm\n";
    let (edge, ch1) = (wfm([1.0; 4]), wfm([0.1; 4]));
    let inner = zip(&[
        ZipFile::stored("demo/", b""),
        ZipFile::deflated("demo/x.set", SETUP.as_bytes()),
        ZipFile::stored("demo/r.ini", ini.as_bytes()),
        ZipFile::deflated("demo/edge.wfm", &edge),
    ]);
    let outer = zip(&[
        ZipFile::stored("demo/", b""),
        ZipFile::deflated("demo/y.set", &inner),
        ZipFile::deflated("demo/CH1.wfm", &ch1),
    ]);
    let (traces, left_out) = parse_wave_noted(&outer, "demo.tss").unwrap();
    let labels: Vec<&str> = traces.iter().map(|t| t.label.as_str()).collect();
    assert_eq!((labels, left_out.as_str()), (vec!["CH1", "edge"], ""));
}

#[test]
fn a_session_archive_without_a_waveform_to_read_or_without_a_session_says_so() {
    let refuse = |b: &[u8]| parse_wave(b, "x.tss").unwrap_err();
    assert_eq!(refuse(&tss(SETUP, &[], &[])), "the session holds no waveform");
    let only = refuse(&tss(SETUP, &[(1, "bits.wfm", digital())], &[("ch4_iq.wfm", wfm([0.0; 4]))]));
    assert_eq!(only, "the session holds no waveform that is read (not read: 1 digital and 1 IQ waveforms)");
    // a reference the list names without its file, and a waveform file that is no reference and no channel
    let list = b"recallSubsystem\\refMgr\\ref2\\sourceFileName=gone.wfm\n";
    let lost = zip(&[ZipFile::stored("x.set", SETUP.as_bytes()), ZipFile::stored("r.ini", list)]);
    let stray = wfm([0.0; 4]);
    let odd = zip(&[ZipFile::stored("x.set", &lost), ZipFile::stored("stray.wfm", &stray)]);
    assert!(refuse(&odd).ends_with("(not read: 1 other and 1 missing waveforms)"), "{}", refuse(&odd));
    // an archive that is not a session; one whose .set is a text; one that is three archives deep
    let plain = zip(&[ZipFile::stored("notes.txt", b"hello")]);
    assert!(refuse(&plain).contains("holds no session"));
    assert!(refuse(&zip(&[ZipFile::stored("x.set", SETUP.as_bytes())])).contains("holds no session"));
    let deep = zip(&[ZipFile::stored("x.set", &zip(&[ZipFile::stored("x.set", &plain)]))]);
    assert!(refuse(&deep).contains("an archive in an archive in an archive"));
    // a reference that is no waveform file, and one that is cut
    let junk = refuse(&tss(SETUP, &[(1, "a.wfm", b"not a waveform".to_vec())], &[]));
    assert_eq!(junk, "the session holds no waveform that is read (not read: 1 other waveform)");
    let text = b"Model,X
Time,Volts
0,1
1e-3,2
2e-3,3
".to_vec();
    assert_eq!(parse_wave(&tss(SETUP, &[(1, "saved.csv", text)], &[]), "x.tss").unwrap()[0].v, [1.0, 2.0, 3.0]);
    let short = wfm([0.0; 4]);
    assert_eq!(refuse(&tss(SETUP, &[], &[("ch1.wfm", short[..400].to_vec())])), "ch1.wfm: truncated WFM");
}

/// The local check: every session archive under the folder `OOS_TSS_DIR` loads or is refused with a reason, and the
/// counts are printed (`cargo test -p oos-store local_folder -- --nocapture`). Skipped when the variable is not
/// set: the archives of an instrument are not part of the tree.
#[test]
fn the_session_archives_of_a_local_folder_load_or_say_why() {
    let Ok(dir) = std::env::var("OOS_TSS_DIR") else { return };
    let mut files: Vec<std::path::PathBuf> = Vec::new();
    let mut dirs = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).unwrap().filter_map(|e| e.ok()) {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("tss")) {
                files.push(p);
            }
        }
    }
    files.sort();
    let (mut loaded, mut noted, mut longest, mut slowest) = (0, 0, 0, (0.0, 0));
    let mut refused: std::collections::BTreeMap<String, usize> = Default::default();
    for (k, p) in files.iter().enumerate() {
        let bytes = read_waveform_file(p).unwrap();
        let start = std::time::Instant::now();
        match parse_wave_noted(&bytes, "local.tss") {
            Ok((traces, left_out)) => {
                assert!(!traces.is_empty(), "file {k}");
                for t in &traces {
                    assert!(t.v.len() >= 2 && t.dt > 0.0 && t.dt.is_finite() && t.t0.is_finite() && !t.label.is_empty(), "file {k}");
                }
                loaded += 1;
                noted += !left_out.is_empty() as usize;
                longest = longest.max(traces.iter().map(|t| t.v.len()).max().unwrap_or(0));
                let took = start.elapsed().as_secs_f64();
                if took > slowest.0 {
                    slowest = (took, bytes.len());
                }
            }
            Err(e) => {
                assert!(!e.is_empty(), "file {k}");
                *refused.entry(e).or_default() += 1;
            }
        }
    }
    println!("{} archives: {loaded} load ({noted} with parts left out), {} refused", files.len(), files.len() - loaded);
    println!("the longest trace has {longest} samples; the slowest load took {:.2} s for {} bytes", slowest.0, slowest.1);
    for (reason, n) in &refused {
        println!("  {n} x {reason}");
    }
}
