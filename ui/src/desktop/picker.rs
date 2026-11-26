// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The native Open and Save As dialogs. Canceling never changes the session and writes nothing.

use std::io::Read;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, Sender};
use windows_sys::Win32::UI::Controls::Dialogs::*;

/// The traces of a file, its name, and what of it was not read (`store::parse_wave_noted`).
type Parsed = Result<(Vec<crate::store::WaveTrace>, String, String), String>;

struct Job {
    id: u64,
    epoch: u64,
    slot: usize,
    path: PathBuf,
}
struct Worker {
    requests: Sender<Job>,
    results: Receiver<(Job, Parsed)>,
}

/// One reader/parser worker, with at most one pending choice after the active
/// file. Superseded/canceled results never replace a working view.
#[derive(Default)]
pub struct Loader {
    worker: Option<Worker>,
    next: u64,
    active: Option<u64>,
}

impl Loader {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn busy(&self) -> bool {
        self.active.is_some()
    }
    pub fn cancel(&mut self) {
        self.active = None;
    }

    pub fn submit(&mut self, path: PathBuf, epoch: u64, slot: usize) -> Result<String, String> {
        if slot >= crate::model::NREF {
            return Err("Choose a reference slot from R1 to R10".into());
        }
        if self.worker.is_none() {
            let (requests, input) = mpsc::channel::<Job>();
            let (output, results) = mpsc::channel();
            std::thread::Builder::new()
                .name("oos-wave-load".into())
                .spawn(move || {
                    while let Ok(mut job) = input.recv() {
                        while let Ok(newer) = input.try_recv() {
                            job = newer;
                        }
                        let parsed = read_waveform(&job.path);
                        if output.send((job, parsed)).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|e| format!("Could not start waveform reader: {e}"))?;
            self.worker = Some(Worker { requests, results });
        }
        let name = file_name(&path);
        self.next = self.next.wrapping_add(1);
        self.active = Some(self.next);
        if self.worker.as_ref().unwrap().requests.send(Job { id: self.next, epoch, slot, path }).is_err() {
            self.active = None;
            self.worker = None;
            return Err("Waveform reader stopped; choose the file again".into());
        }
        Ok(name)
    }

    pub fn poll(&mut self, epoch: u64) -> Option<(usize, Parsed)> {
        loop {
            let worker = self.worker.as_ref()?;
            match worker.results.try_recv() {
                Ok((job, result)) => {
                    if self.active != Some(job.id) {
                        continue;
                    }
                    self.active = None;
                    if job.epoch != epoch {
                        continue;
                    }
                    return Some((job.slot, result));
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.worker = None;
                    return self
                        .active
                        .take()
                        .map(|_| (0, Err("Waveform reader stopped; choose the file again".into())));
                }
            }
        }
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "waveform".into())
}

/// Bounded reading also handles a file growing after metadata was checked.
fn read_waveform(path: &std::path::Path) -> Parsed {
    let name = file_name(path);
    let file = std::fs::File::open(path).map_err(|e| format!("Cannot read {name}: {e}"))?;
    let size = file.metadata().map_err(|e| format!("Cannot read {name}: {e}"))?.len();
    let maximum = crate::store::MAX_WAVEFORM_BYTES;
    if size > maximum as u64 {
        return Err(format!("{name} exceeds the {} MiB waveform limit", maximum >> 20));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes).map_err(|e| format!("Cannot read {name}: {e}"))?;
    if bytes.len() > maximum {
        return Err(format!("{name} exceeds the {} MiB waveform limit", maximum >> 20));
    }
    let (traces, left_out) = crate::store::parse_wave_at(&bytes, path)?;
    Ok((traces, name, left_out))
}

/// One question of a file dialog: Open, or Save As.
struct Ask<'a> {
    title: String,
    /// pairs of a description and its patterns, each ended by NUL (`"Sessions (*.oss)\0*.oss\0"`)
    filter: String,
    /// the folder the dialog opens in; None: where the system last was
    dir: Option<&'a std::path::Path>,
    /// the name a save proposes
    name: &'a str,
    /// the suffix a save adds to a name typed without one
    ext: &'a str,
    save: bool,
}

/// NUL-ended UTF-16, as the dialog takes its texts.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The buffer the dialog writes the chosen path into, with the proposed name in it.
fn name_buffer(name: &str) -> Vec<u16> {
    let mut buffer = vec![0u16; 32_768];
    for (slot, unit) in buffer.iter_mut().zip(name.encode_utf16().take(32_000)) {
        *slot = unit;
    }
    buffer
}

/// The filter of a dialog: `kinds` as pairs of description and patterns, then All files.
fn filter(kinds: &[(&str, &str)]) -> String {
    let mut text: String = kinds.iter().map(|(what, patterns)| format!("{what} ({patterns})\0{patterns}\0")).collect();
    text.push_str("All files (*.*)\0*.*\0");
    text
}

/// Opens the dialog and waits for it. None: closed without a file.
fn dialog(owner: *mut std::ffi::c_void, ask: &Ask) -> Result<Option<PathBuf>, String> {
    let (filter, title, ext) = (wide(&ask.filter), wide(&ask.title), wide(ask.ext));
    // a folder that is not there yet (nothing was saved so far) would make the dialog fail: it opens where it last was
    let dir = ask.dir.filter(|d| d.is_dir()).map(|d| wide(&d.to_string_lossy()));
    let mut name = name_buffer(ask.name);
    let flags = if ask.save { OFN_OVERWRITEPROMPT | OFN_HIDEREADONLY } else { OFN_FILEMUSTEXIST };
    let mut options = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: filter.as_ptr(),
        nFilterIndex: 1,
        lpstrFile: name.as_mut_ptr(),
        nMaxFile: name.len() as u32,
        lpstrInitialDir: dir.as_ref().map_or(std::ptr::null(), |d| d.as_ptr()),
        lpstrTitle: title.as_ptr(),
        lpstrDefExt: if ask.ext.is_empty() { std::ptr::null() } else { ext.as_ptr() },
        Flags: OFN_EXPLORER | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR | flags,
        ..unsafe { std::mem::zeroed() }
    };
    let chosen = unsafe {
        if ask.save {
            GetSaveFileNameW(&mut options)
        } else {
            GetOpenFileNameW(&mut options)
        }
    };
    if chosen == 0 {
        let error = unsafe { CommDlgExtendedError() };
        return if error == 0 { Ok(None) } else { Err(format!("The file dialog could not open (error {error:#x}).")) };
    }
    let end = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    Ok(Some(std::ffi::OsString::from_wide(&name[..end]).into()))
}

/// The file of SCPI commands to run (Storage > Run SCPI file).
pub fn commands(owner: *mut std::ffi::c_void) -> Result<Option<PathBuf>, String> {
    let kinds = [("SCPI commands", "*.scpi;*.txt")];
    dialog(
        owner,
        &Ask { title: "Run SCPI file".into(), filter: filter(&kinds), dir: None, name: "", ext: "", save: false },
    )
}

pub fn waveform(owner: *mut std::ffi::c_void, slot: usize) -> Result<Option<PathBuf>, String> {
    let patterns = "*.csv;*.bin;*.wfm;*.trc;*.isf;*.tr0;*.wfmx;*.pwwf;*.tkwf;*.tss";
    dialog(
        owner,
        &Ask {
            title: format!("Open waveform into R{}", slot + 1),
            filter: filter(&[("Waveforms", patterns)]),
            dir: None,
            name: "",
            ext: "",
            save: false,
        },
    )
}

/// The title and the filter of the dialog for a Storage row.
fn storage_texts(kind: crate::ui::FileKind, save: bool) -> (String, String) {
    use crate::ui::FileKind::*;
    let (what, kinds): (&str, &[(&str, &str)]) = match kind {
        Setup => ("setup", &[("Setups", "*.txt;*.stp")]),
        SetupScpi => ("setup as SCPI", &[("SCPI commands", "*.scpi;*.txt")]),
        // a load also takes the session archive of another instrument (docs/tss.md); a save writes OOS's own
        Session if !save => ("session", &[("Sessions", "*.oss;*.tss")]),
        Session => ("session", &[("Sessions", "*.oss")]),
        WaveCsv => ("waveform (CSV)", &[("CSV", "*.csv")]),
        WaveBin => ("waveform (binary)", &[("OOS waveforms", "*.bin")]),
        Shot => ("screenshot", &[("PNG", "*.png")]),
        FastFrame => ("fast-frame", &[("Fast-frame files", "*.bin")]),
    };
    (format!("{} {what}", if save { "Save" } else { "Load" }), filter(kinds))
}

/// Save As or Open for a Storage row (`ui/files.rs`): the file the user chose, None when the dialog was closed.
pub fn storage(owner: *mut std::ffi::c_void, ask: &crate::ui::FileAsk) -> Result<Option<PathBuf>, String> {
    let (title, filter) = storage_texts(ask.kind, ask.save);
    dialog(owner, &Ask { title, filter, dir: Some(&ask.dir), name: &ask.name, ext: ask.kind.ext(), save: ask.save })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn fixture() -> PathBuf {
        let directory = std::env::temp_dir().join(format!("oos-native-load-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("clock.csv");
        std::fs::write(&path, b"Time (s),Volts\n0,-1\n1e-9,1\n2e-9,-1\n").unwrap();
        path
    }

    #[test]
    fn background_reader_commits_only_the_latest_choice_in_the_same_workflow() {
        let path = fixture();
        let mut loader = Loader::new();
        loader.submit(path.clone(), 1, 0).unwrap();
        loader.submit(path.clone(), 2, 8).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        let (traces, name, _) = loop {
            if let Some((slot, result)) = loader.poll(2) {
                assert_eq!(slot, 8);
                break result.unwrap();
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        };
        assert_eq!(name, "clock.csv");
        assert_eq!(traces[0].v, [-1.0, 1.0, -1.0]);
        assert!(!loader.busy());
        loader.submit(path.clone(), 2, 1).unwrap();
        while loader.busy() {
            assert!(loader.poll(3).is_none(), "a changed source workflow must reject the late load");
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(2));
        }
        loader.submit(path.clone(), 3, 2).unwrap();
        loader.cancel();
        assert!(!loader.busy());
        assert!(loader.poll(3).is_none());
        let _ = std::fs::remove_file(path);
    }

    /// What the dialog is given: texts that end with NUL, the proposed name at the start of its buffer, a filter
    /// that names the kind first and ends with All files.
    #[test]
    fn the_texts_of_a_storage_dialog() {
        assert_eq!(wide("a\u{e9}"), [0x61, 0xe9, 0]);
        let buffer = name_buffer("session-001.oss");
        assert_eq!(String::from_utf16_lossy(&buffer[..15]), "session-001.oss");
        assert!(buffer.len() == 32_768 && buffer[15..].iter().all(|&unit| unit == 0));
        for save in [true, false] {
            for kind in [
                crate::ui::FileKind::Setup,
                crate::ui::FileKind::SetupScpi,
                crate::ui::FileKind::Session,
                crate::ui::FileKind::WaveCsv,
                crate::ui::FileKind::WaveBin,
                crate::ui::FileKind::Shot,
                crate::ui::FileKind::FastFrame,
            ] {
                let (title, filter) = storage_texts(kind, save);
                assert!(title.starts_with(if save { "Save " } else { "Load " }), "{title}");
                let parts: Vec<&str> = filter.split('\0').collect();
                assert!(parts.len() == 5 && parts[4].is_empty(), "{filter:?}");
                assert!(
                    parts[1].split(';').any(|pattern| pattern == format!("*.{}", kind.ext())),
                    "{kind:?}: {filter:?}"
                );
                assert_eq!((parts[2], parts[3]), ("All files (*.*)", "*.*"));
            }
        }
    }

    /// Explicit local audit, never part of ordinary CI. The original captures
    /// are read only; pass paths separated by semicolons in OOS_NATIVE_WAVEFORMS.
    #[test]
    #[ignore = "requires explicitly supplied local waveform paths"]
    fn actual_native_waveform_files_parse_at_their_recorded_intervals() {
        let paths = std::env::var("OOS_NATIVE_WAVEFORMS")
            .expect("set OOS_NATIVE_WAVEFORMS to semicolon-separated capture paths");
        for path in paths.split(';') {
            let start = Instant::now();
            let (traces, name, _) = read_waveform(std::path::Path::new(path)).unwrap();
            assert!(!traces.is_empty());
            for trace in traces {
                assert!(trace.v.len() >= 2 && trace.dt.is_finite() && trace.dt > 0.0 && trace.t0.is_finite());
                assert!(trace.v.iter().any(|value| value.is_finite()));
                eprintln!(
                    "{name}: {} points dt={}s t0={}s label={} read+parse={:?}",
                    trace.v.len(),
                    trace.dt,
                    trace.t0,
                    trace.label,
                    start.elapsed()
                );
            }
        }
    }
}
