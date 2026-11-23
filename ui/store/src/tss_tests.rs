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
    assert!(
        refused(&with(list + 42, &[0xF0, 0xFF, 0, 0]), 4096).contains("cut short"),
        "an entry that begins past the list"
    );
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
    // a reference that is no waveform file; one that is a text of values, which is read; a record that is cut
    let junk = refuse(&tss(SETUP, &[(1, "a.wfm", b"not a waveform".to_vec())], &[]));
    assert_eq!(junk, "the session holds no waveform that is read (not read: 1 other waveform)");
    let text = b"Model,X\nTime,Volts\n0,1\n1e-3,2\n2e-3,3\n".to_vec();
    assert_eq!(parse_wave(&tss(SETUP, &[(1, "saved.csv", text)], &[]), "x.tss").unwrap()[0].v, [1.0, 2.0, 3.0]);
    let short = wfm([0.0; 4]);
    assert_eq!(refuse(&tss(SETUP, &[], &[("ch1.wfm", short[..400].to_vec())])), "ch1.wfm: truncated WFM");
}

/// A record as a session gives it.
struct Rec {
    t0: f64,
    dt: f64,
    ch: [Vec<f32>; NCH],
}

impl Samples for Rec {
    fn t0(&self) -> f64 {
        self.t0
    }
    fn dt(&self) -> f64 {
        self.dt
    }
    fn ch(&self) -> &[Vec<f32>; NCH] {
        &self.ch
    }
    fn from_file(t0: f64, dt: f64, ch: [Vec<f32>; NCH]) -> Self {
        Rec { t0, dt, ch }
    }
}

/// The value of `key` in a setup text of OOS, as a number (`true` is 1); None when the key is not there.
fn set(session: &Session<Rec>, key: &str) -> Option<f64> {
    let line = session.setup.lines().find_map(|l| l.strip_prefix(key).and_then(|rest| rest.strip_prefix(" = ")))?;
    Some(line.parse().unwrap_or(if line == "true" { 1.0 } else { 0.0 }))
}

/// The value of `key` is `want`, to the six digits a setup text keeps of a 32-bit number.
fn near(session: &Session<Rec>, key: &str, want: f64) {
    let got = set(session, key).unwrap_or(f64::NAN);
    assert!((got - want).abs() <= 1e-6 * want.abs().max(1e-12), "{key} = {got}, not {want}\n{}", session.setup);
}

#[test]
fn a_session_archive_becomes_a_session_with_the_settings_that_are_mapped() {
    // CH1 with its settings; CH2 with none, shown off; CH3 on another time axis; a channel and a reference OOS has
    // no place for
    let setup = ":CH1:SCALE 320.0000E-3\n:CH1:POSITION -3.7400\n:CH1:OFFSET 500.0000E-3\n:CH1:LABEL:NAME \"a%20long%20name%20here\"\n\
        :DISPLAY:GLOBAL:CH1:STATE 1\n:DISPLAY:GLOBAL:CH2:STATE 0\n:SELECT:CH2 1\n:CH5:SCALE 1.0\n\
        :HORIZONTAL:SCALE 200.0000E-12\n:HORIZONTAL:POSITION 25.0000\n:HORIZONTAL:DELAY:MODE 0\n:HORIZONTAL:DELAY:TIME 1.0\n\
        :DISPLAY:WAVEVIEW1:REF:REF2:VERTICAL:SCALE 6.0000\n:DISPLAY:WAVEVIEW1:REF:REF2:VERTICAL:POSITION -1.5000\n\
        :DISPLAY:GLOBAL:REF2:STATE 0\n:REF:REF2:LABEL:NAME \"Hall = A\"\n";
    let other_axis = wfm_f32(b":WFM#003", -4e-9, 1e-9, &[0.0; 4], "");
    let beside = [
        ("ch1.wfm", wfm([0.1; 4])),
        ("ch2.wfm", wfm([-1.0, 5.0, 2.0, 2.0])),
        ("ch3.wfm", other_axis),
        ("ch5.wfm", wfm([0.5; 4])),
    ];
    let refs =
        [(2, "hall.wfm", wfm([3.0; 4])), (7, "fit.wfm", wfm([0.0, 0.3, 0.3, 0.0])), (11, "far.wfm", wfm([0.0; 4]))];
    let s: Session<Rec> = parse_tss(&tss(setup, &refs, &beside), 10).unwrap();
    let rec = s.record.as_ref().unwrap();
    assert_eq!((rec.t0, rec.dt), (-4e-9, 2e-9));
    assert_eq!(rec.ch.each_ref().map(Vec::len), [4, 4, 0, 0], "CH1 and CH2 at their length, CH3 not on their axis");
    assert_eq!(rec.ch[1], [-1.0, 5.0, 2.0, 2.0]);
    let ending = "; not read: 1 differently timed and 2 surplus waveforms";
    assert!(
        s.note.starts_with("waveforms, scales, time base and filters, without") && s.note.ends_with(ending),
        "{}",
        s.note
    );
    // 320 mV a division is the step of 200 mV times 1.6; the trace 3.74 divisions down after 0.5 V are taken off
    for (key, want) in
        [("ch1.on", 1.0), ("ch1.vdiv", 0.2), ("ch1.vfine", 1.6), ("ch1.offset", -3.74 * 0.32 - 0.5), ("ch1.probe", 1.0)]
    {
        near(&s, key, want);
    }
    assert!(s.setup.contains("\nch1.label = a long nam\n"), "ten characters of the name");
    // no scale in the settings: 6 V of samples in six divisions of 1 V, around 2 V; shown off as the settings say
    for (key, want) in [("ch2.on", 0.0), ("ch2.vdiv", 1.0), ("ch2.offset", -2.0), ("ch3.on", 0.0), ("ch4.on", 0.0)] {
        near(&s, key, want);
    }
    assert_eq!(set(&s, "ch2.vfine"), None);
    // the window of the settings lies in the record: 200 ps a division, the trigger a quarter in
    for (key, want) in [("tdiv", 2e-10), ("delay", 5e-10), ("active", 0.0), ("trig_src", 0.0)] {
        near(&s, key, want);
    }
    // the references: in the slots of their numbers, with the look of the settings or one fitted to the samples
    let slots: Vec<(usize, usize, f64)> = s.refs.iter().map(|r| (r.slot, r.v.len(), r.dt)).collect();
    assert_eq!((slots, s.offline_ref), (vec![(1, 4, 2e-9), (6, 4, 2e-9)], None));
    for (key, want) in [
        ("ref2.on", 0.0),
        ("ref2.vdiv", 6.0),
        ("ref2.offset", -9.0),
        ("ref7.on", 1.0),
        ("ref7.vdiv", 0.05),
        ("ref7.offset", -0.15),
    ] {
        near(&s, key, want);
    }
    assert!(s.setup.contains("\nref2.name = Hall = A\n") && s.setup.contains("\nref7.name = fit\n"));

    // a window that is not in the record (the settings of another acquisition), and none: the whole record
    for horizontal in [":HORIZONTAL:SCALE 4.0000E-3\n:HORIZONTAL:POSITION 50.0000\n", ""] {
        let s: Session<Rec> = parse_tss(&tss(horizontal, &[], &[("ch4.wfm", wfm([0.0, 1.0, 0.0, 1.0]))]), 10).unwrap();
        // 8 ns in ten divisions: the step of 1 ns times 0.8
        for (key, want) in [("tdiv", 1e-9), ("tfine", 0.8), ("delay", 0.0)] {
            near(&s, key, want);
        }
        assert_eq!(
            (set(&s, "ch4.on"), set(&s, "ch1.on"), set(&s, "active"), s.note.contains("not read")),
            (Some(1.0), Some(0.0), Some(3.0), false)
        );
    }
    // the delay switched on: the middle of the screen is that long after the trigger
    let delayed = ":HORIZONTAL:SCALE 200.0000E-12\n:HORIZONTAL:POSITION 10.0\n:HORIZONTAL:DELAY:MODE ON\n:HORIZONTAL:DELAY:TIME -1.0E-9\n";
    let s: Session<Rec> = parse_tss(&tss(delayed, &[], &[("ch1.wfm", wfm([0.0; 4]))]), 10).unwrap();
    for (key, want) in [("tdiv", 2e-10), ("delay", -1e-9)] {
        near(&s, key, want);
    }
}

#[test]
fn a_session_archive_of_references_only_shows_the_first_and_one_without_a_waveform_is_refused() {
    let long = wfm_f32(b":WFM#003", -1e-3, 1e-6, &[0.5; 4000], "");
    let refs = [(4, "b.wfm", wfm([0.0; 4])), (3, "a.wfm", long)];
    let s: Session<Rec> =
        parse_tss(&tss(":HORIZONTAL:SCALE 1.0E-9\n:HORIZONTAL:POSITION 50.0\n", &refs, &[]), 10).unwrap();
    assert!(s.record.is_none());
    assert_eq!((s.offline_ref, s.refs.len(), s.refs[0].slot, s.refs[0].v.len()), (Some(2), 2, 2, 4000));
    // the time base of that reference: 4 ms in ten divisions is the step of 500 us times 0.8, around 1 ms
    for (key, want) in [("tdiv", 5e-4), ("tfine", 0.8), ("delay", 1e-3)] {
        near(&s, key, want);
    }
    assert!((1..=NCH).all(|c| set(&s, &format!("ch{c}.on")) == Some(0.0)));

    let refuse = |b: &[u8]| parse_tss::<Rec>(b, 10).err().unwrap();
    assert_eq!(refuse(&tss(SETUP, &[], &[])), "the session holds no waveform");
    assert_eq!(
        refuse(&tss(SETUP, &[], &[("ch1_iq.wfm", wfm([0.0; 4]))])),
        "the session holds no waveform that is read (not read: 1 IQ waveform)"
    );
    assert_eq!(
        refuse(&tss(SETUP, &[(12, "far.wfm", wfm([0.0; 4]))], &[])),
        "the session holds no waveform that is read (not read: 1 surplus waveform)"
    );
    assert!(refuse(&zip(&[ZipFile::stored("notes.txt", b"hello")])).contains("holds no session"));
    assert!(refuse(b"PK\x03\x04").contains("cut short"));
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
                    assert!(
                        t.v.len() >= 2 && t.dt > 0.0 && t.dt.is_finite() && t.t0.is_finite() && !t.label.is_empty(),
                        "file {k}"
                    );
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
    println!(
        "the longest trace has {longest} samples; the slowest load took {:.2} s for {} bytes",
        slowest.0, slowest.1
    );
    for (reason, n) in &refused {
        println!("  {n} x {reason}");
    }
}

/// The I/Q record of a spectrum view (`ch<n>_iq.wfm`) with a centre frequency in the settings is read as the real signal
/// around that frequency: a tone 1 MHz above the centre is a sine at 99 MHz, at the rate that holds it; without a
/// centre it is counted as not read, as before.
#[test]
fn the_iq_record_of_a_spectrum_view_is_read_around_its_centre() {
    let (dt, pairs) = (40e-9, 3000usize);
    // one step of the file is half a pair: I, Q, I, Q ...
    let tone: Vec<f32> = (0..pairs)
        .flat_map(|k| {
            let ph = std::f64::consts::TAU * 1e6 * (2 * k) as f64 * dt;
            [ph.cos() as f32, ph.sin() as f32]
        })
        .collect();
    let iq = wfm_f32(b":WFM#003", -1.395e-3, dt, &tone, "");
    let real = wfm([0.5; 4]);
    let setup = "*RST\n:CH1:SV:CENTERFREQUENCY 98.0000E+6\n";
    let beside = [("ch1.wfm", real.clone()), ("ch1_iq.wfm", iq.clone())];
    let (traces, left_out) = parse_wave_noted(&tss(setup, &[], &beside), "bench.tss").unwrap();
    assert_eq!(left_out, "", "nothing is left out");
    assert_eq!(traces.len(), 2);
    let signal = &traces[1];
    assert_eq!(signal.label, "ch1 I/Q 98 MHz");
    assert_eq!(signal.t0, -1.395e-3);
    // the rate holds 98 MHz plus half the I/Q rate (104.25 MHz) 2.2 times over: 19 steps a pair of 80 ns
    assert!(
        (signal.dt - 80e-9 / 19.0).abs() < 1e-15 && signal.v.len() == (pairs - 1) * 19 + 1,
        "{} {}",
        signal.dt,
        signal.v.len()
    );
    // zero crossings in the middle of the record: 99 MHz is 198 a microsecond
    let (from, to) = (signal.v.len() / 4, 3 * signal.v.len() / 4);
    let crossings = (from..to).filter(|&k| signal.v[k - 1] < 0.0 && signal.v[k] >= 0.0).count() as f64;
    let seconds = (to - from) as f64 * signal.dt;
    let mhz = crossings / seconds / 1e6;
    assert!((mhz - 99.0).abs() < 0.2, "the tone is at {mhz} MHz");
    // without a centre in the settings the record is left out and counted
    let (traces, left_out) = parse_wave_noted(&tss("*RST\n", &[], &beside), "bench.tss").unwrap();
    assert_eq!((traces.len(), left_out.as_str()), (1, "not read: 1 IQ waveform"));
}

/// The filters of the math channels become math channels of OOS: the response, order, cut-off or band, the source, the
/// scale and position of the lane, and whether it is shown; a custom one stands in as a Butterworth and is said so.
#[test]
fn the_filters_of_the_math_channels_are_mapped_to_the_designer() {
    let mut setup = String::new();
    let filters = [
        // (type, response, extra lines)
        (
            "LPASS",
            "BUTTERWORTH",
            ":MATH:MATH1:FILTER:CFREQ 156.2500E+3
:MATH:MATH1:FILTER:ORDER 5
",
        ),
        (
            "LPASS",
            "BESSEL",
            ":MATH:MATH2:FILTER:CFREQ 450.0000E+3
:MATH:MATH2:FILTER:ORDER 5
",
        ),
        (
            "LPASS",
            "GAUSSIAN",
            ":MATH:MATH3:FILTER:CFREQ 250.0000E+3
",
        ),
        (
            "APASS",
            "BUTTERWORTH",
            ":MATH:MATH4:FILTER:DELAY 1.6000E-3
",
        ),
        (
            "RC",
            "BUTTERWORTH",
            ":MATH:MATH5:FILTER:SDURATION 5.0000E-6
:MATH:MATH5:FILTER:ROFACTOR 300.0000E-3
",
        ),
        (
            "BPASS",
            "CUSTOM",
            ":MATH:MATH6:FILTER:LCFREQ 80.0000E+3
:MATH:MATH6:FILTER:HCFREQ 400.0000E+3
:MATH:MATH6:FILTER:ORDER 5087
",
        ),
    ];
    for (k, (kind, response, more)) in filters.iter().enumerate() {
        let n = k + 1;
        setup += &format!(
            ":MATH:MATH{n}:TYPE FILTER
:MATH:MATH{n}:FILTER:TYPE {kind}
:MATH:MATH{n}:FILTER:RESPONSE {response}
             :MATH:MATH{n}:FILTER:SOURCE REF1
{more}:DISPLAY:WAVEVIEW1:MATH:MATH{n}:STATE {}
",
            (n != 2) as u8
        );
    }
    setup += ":DISPLAY:WAVEVIEW1:MATH:MATH1:VERTICAL:SCALE 80.0000E-3
:DISPLAY:WAVEVIEW1:MATH:MATH1:VERTICAL:POSITION 20.0000E-3
              :DISPLAY:SELECT:MATH MATH5
:MATH:MATH7:TYPE FILTER
:MATH:MATH7:FILTER:TYPE NOTCHY
";
    let refs = [(1, "demo.wfm", wfm([0.0, 1.0, 0.0, -1.0]))];
    let s: Session<Rec> = parse_tss(&tss(&setup, &refs, &[]), 10).unwrap();
    let lines: Vec<&str> = s.setup.lines().collect();
    let has = |l: &str| lines.contains(&l);
    // the selected channel (M5) is at the main keys, the others under their numbers
    assert!(has("math_sel = 4") && has("filt_resp = RaisedCosine"), "{}", s.setup);
    assert!(has("m1.op = Lpf") && has("m2.op = Lpf") && has("m4.op = Lpf") && has("m6.op = Bpf"), "{}", s.setup);
    // the source is R1, which is the 17th source (4 channels, 8 math channels)
    assert!(has("m1.a = 12") && has("m6.a = 12"), "{}", s.setup);
    assert!(lines.iter().any(|l| l.starts_with("m1.filt = Butterworth 5 ")), "{}", s.setup);
    assert!(lines.iter().any(|l| l.starts_with("m2.filt = Bessel 5 ")), "{}", s.setup);
    assert!(lines.iter().any(|l| l.starts_with("m3.filt = Gaussian ")), "{}", s.setup);
    assert!(lines.iter().any(|l| l.starts_with("m4.filt = Delay ") && l.ends_with(" 0.0016")), "{}", s.setup);
    assert!(lines.iter().any(|l| l.starts_with("filt_symbol = 0.000005")), "{}", s.setup);
    assert!(has("filt_rolloff = 0.3"), "{}", s.setup);
    // a band of 80 to 400 kHz: its centre and width
    near(&s, "m6.fc", 240e3);
    near(&s, "m6.bw", 320e3);
    near(&s, "m1.fc", 156.25e3);
    assert!(
        s.note.contains("1 custom filter file not read") && s.note.contains("1 filter of a kind not mapped"),
        "{}",
        s.note
    );
}

/// A filter file (the taps of a filter and its sample rate) is a waveform at the step of the filter, and a custom filter
/// of a session archive takes its taps from the file beside the settings: they come as a reference, Source B.
#[test]
fn the_taps_of_a_custom_filter_are_a_reference_and_its_source_b() {
    let flt = "// Do not edit. Generated by a filter editor.
// {filter_type: lowPass}

5000000.0;0.25,0.25,0.25,0.25
";
    let t = parse_wave(flt.as_bytes(), "my taps.flt").unwrap();
    assert_eq!((t.len(), t[0].v.clone(), t[0].dt, t[0].label.as_str()), (1, vec![0.25; 4], 2e-7, "my taps"));
    assert!(parse_wave(b"// only a comment
@1000.0;1.0
", "d.flt").is_err(), "one tap is no filter");
    let setup = ":MATH:MATH1:TYPE FILTER
:MATH:MATH1:FILTER:TYPE BPASS
:MATH:MATH1:FILTER:RESPONSE CUSTOM
        :MATH:MATH1:FILTER:SOURCE CH1
:MATH:MATH1:FILTER:LCFREQ 1.0
:MATH:MATH1:FILTER:HCFREQ 2.0
";
    let inner = zip(&[ZipFile::deflated("bench.set", setup.as_bytes()), ZipFile::deflated("filterCoeff1_math1.flt", flt.as_bytes())]);
    let outer = zip(&[ZipFile::stored("shot.png", b"a picture"), ZipFile::deflated("bench.set", &inner), ZipFile::stored("ch1.wfm", &wfm([0.0, 1.0, 0.0, -1.0]))]);
    let s: Session<Rec> = parse_tss(&outer, 10).unwrap();
    let lines: Vec<&str> = s.setup.lines().collect();
    assert!(lines.iter().any(|l| l.starts_with("filt_resp = Custom")), "{}", s.setup);
    // the taps are the first free slot after the references: R1; Source B of M1 is the source after CH1-4 and M1-8
    assert_eq!(s.refs.len(), 1);
    assert!(lines.contains(&"math_b = 12"), "{}", s.setup);
    assert!(!s.note.contains("custom filter file"), "{}", s.note);
}

