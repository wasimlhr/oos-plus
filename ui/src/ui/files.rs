// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Storage: a file by its name. A row that saves asks for the name of its file, a row that loads asks which file. The
//! desktop build asks with the dialogs of its system (`desktop/picker.rs`): the question is left in `file_request`
//! and the window loop answers it. The scope has no such dialog and takes the next free name and the newest file. A
//! path given by SCPI, a command without one and the Quick key never ask.

use super::*;
use std::path::{Path, PathBuf};

/// What a Storage row saves or loads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    Setup,
    /// the set-up as SCPI commands
    SetupScpi,
    Session,
    WaveCsv,
    WaveBin,
    Shot,
    FastFrame,
}

impl FileKind {
    /// The start of the names OOS gives (`wave-001.csv`).
    pub fn prefix(self) -> &'static str {
        match self {
            FileKind::Setup | FileKind::SetupScpi => "setup",
            FileKind::Session => "session",
            FileKind::WaveCsv | FileKind::WaveBin => "wave",
            FileKind::Shot => "shot",
            FileKind::FastFrame => "frame",
        }
    }

    pub fn ext(self) -> &'static str {
        match self {
            FileKind::Setup => "txt",
            FileKind::SetupScpi => "scpi",
            FileKind::Session => "oss",
            FileKind::WaveCsv => "csv",
            FileKind::WaveBin | FileKind::FastFrame => "bin",
            FileKind::Shot => "png",
        }
    }
}

/// A question for the file dialog of the desktop build: save under which name, or open which file.
#[derive(Clone, Debug, PartialEq)]
pub struct FileAsk {
    pub kind: FileKind,
    pub save: bool,
    /// the folder the dialog opens in
    pub dir: PathBuf,
    /// the name it proposes for a save
    pub name: String,
}

impl Ui {
    /// The desktop build asks with the dialogs of its system; the scope has none.
    fn native_files(&self) -> bool {
        self.desktop_panel.is_some()
    }

    /// The folder a file of `kind` is saved to and looked for in: the save folder (Storage > Save to). A setup follows
    /// it to the stick; locally it lies beside the slots, in the folder that is kept over a power cycle, where the
    /// save folder of the scope is in memory.
    pub(super) fn file_dir(&self, kind: FileKind) -> PathBuf {
        let save = self.save_dir();
        if kind != FileKind::Setup || save == Path::new(crate::store::USB_DIR) {
            return save;
        }
        self.setup_path().parent().map(Path::to_path_buf).unwrap_or(save)
    }

    /// The next free name of `kind` in its folder: what a save without a name takes, and what the dialog proposes. A
    /// session and a fast-frame file keep their running number, which the load without a name finds.
    fn file_next(&self, kind: FileKind) -> PathBuf {
        let dir = self.file_dir(kind);
        match kind {
            FileKind::Session | FileKind::FastFrame => crate::store::next_name(&dir, kind.prefix(), kind.ext()),
            _ => self.new_file(&dir, kind.prefix(), kind.ext()),
        }
    }

    /// Why there is nothing to save, said before a name is asked for.
    fn nothing_to_save(&self, kind: FileKind) -> Option<&'static str> {
        match kind {
            FileKind::WaveCsv | FileKind::WaveBin if self.last[0].is_none() => Some("No waveform to save"),
            FileKind::FastFrame if self.history.is_empty() && self.last[0].is_none() => Some("No frames to save"),
            _ => None,
        }
    }

    /// A Storage row that saves: asks for the name of the file, then saves (`file_save`).
    pub(super) fn file_save_ask(&mut self, kind: FileKind) {
        if let Some(why) = self.nothing_to_save(kind) {
            return self.toast(why);
        }
        if !self.native_files() {
            return self.file_save_auto(kind);
        }
        let next = self.file_next(kind);
        self.file_request = Some(FileAsk { kind, save: true, dir: self.file_dir(kind), name: crate::store::file_name(&next) });
    }

    /// Saves `kind` to the file the user named in the dialog.
    pub fn file_save(&mut self, kind: FileKind, path: PathBuf) {
        match kind {
            FileKind::Setup => {
                let r = crate::store::write(&path, self.setup_text_full().as_bytes());
                self.toast_saved(&path, r);
            }
            FileKind::SetupScpi => self.setup_scpi_request = Some(path),
            FileKind::Session => {
                let _ = self.session_write(path);
            }
            FileKind::WaveCsv | FileKind::WaveBin => {
                // the LA lines of a CSV go beside it, under its name
                let csv = kind == FileKind::WaveCsv;
                let la = path.file_stem().filter(|_| csv).map(|s| path.with_file_name(format!("{}-la.csv", s.to_string_lossy())));
                self.store_wave_to(csv, path, la);
            }
            FileKind::Shot => {
                // without the dialog on it: the screen is saved after the next frame
                self.close_menu();
                self.shot_request = Some(path.to_string_lossy().into_owned());
            }
            FileKind::FastFrame => self.ff_save_to(Some(path)),
        }
    }

    /// The save under the next free name, without a question: SCPI without a path and the Quick key.
    pub(crate) fn file_save_auto(&mut self, kind: FileKind) {
        match kind {
            FileKind::WaveCsv | FileKind::WaveBin => self.store_wave(kind == FileKind::WaveCsv, None),
            FileKind::FastFrame => self.ff_save_to(None),
            FileKind::Session => {
                let _ = self.session_save("");
            }
            _ => {
                let next = self.file_next(kind);
                self.file_save(kind, next);
            }
        }
    }

    /// A Storage row that loads: asks which file, then loads it (`file_load`).
    pub(super) fn file_load_ask(&mut self, kind: FileKind) {
        if !self.native_files() {
            return self.file_load_latest(kind);
        }
        self.file_request = Some(FileAsk { kind, save: false, dir: self.file_dir(kind), name: String::new() });
    }

    /// Loads the file the user chose in the dialog.
    pub fn file_load(&mut self, kind: FileKind, path: &Path) {
        match kind {
            FileKind::Setup => self.recall_setup_from(path),
            FileKind::Session => {
                let _ = self.session_read(path.to_path_buf());
            }
            FileKind::FastFrame => self.ff_load_from(path),
            FileKind::SetupScpi | FileKind::WaveCsv | FileKind::WaveBin | FileKind::Shot => {}
        }
    }

    /// The newest file of `kind` with a running number, without a question: `:LOAD:FFRame`.
    pub(crate) fn file_load_latest(&mut self, kind: FileKind) {
        match crate::store::latest_name(&self.file_dir(kind), kind.prefix(), kind.ext()) {
            Some(p) => self.file_load(kind, &p),
            None => self.toast(&format!("No {}-NNN.{} in the save folder", kind.prefix(), kind.ext())),
        }
    }

    /// The question the window loop of the desktop build answers with its dialog.
    #[cfg_attr(target_os = "linux", allow(dead_code))] // used by the desktop build and the tests
    pub fn take_file_request(&mut self) -> Option<FileAsk> {
        self.file_request.take()
    }

    /// The answer of the dialog: the file, or None when it was closed without one.
    #[cfg_attr(target_os = "linux", allow(dead_code))] // used by the desktop build and the tests
    pub fn file_chosen(&mut self, ask: &FileAsk, path: Option<PathBuf>) {
        match (path, ask.save) {
            (Some(p), true) => self.file_save(ask.kind, p),
            (Some(p), false) => self.file_load(ask.kind, &p),
            (None, true) => self.toast("Save canceled; nothing was written"),
            (None, false) => self.toast("Open canceled; nothing was loaded"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bench(tag: &str) -> (Ui, PathBuf) {
        let dir = std::env::temp_dir().join(format!("oos-files-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut u = Ui::new(Source::Demo { t: 0.0 });
        u.store_dir = dir.clone();
        u.store_dest = crate::store::StoreDest::Local;
        u.step(0.05);
        (u, dir)
    }

    /// Save to names the place of a setup by name too: the stick when files go there, else the folder of the slots.
    #[test]
    fn save_to_holds_for_a_setup_by_name() {
        let (mut u, dir) = bench("dest");
        assert_eq!(u.file_dir(FileKind::Setup), dir);
        assert_eq!(u.file_dir(FileKind::Session), dir);
        u.store_dest = crate::store::StoreDest::Stick;
        assert_eq!(u.file_dir(FileKind::Setup), Path::new(crate::store::USB_DIR));
        assert_eq!(u.setup_path().parent(), Some(dir.as_path()), "the slots stay where they are kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The desktop build: the row leaves a question for the dialog; the answer saves or loads, and a dialog closed
    /// without a file changes nothing. The slots do not ask.
    #[test]
    fn the_desktop_build_asks_with_its_dialog() {
        let (mut u, dir) = bench("desktop");
        u.desktop_panel = Some(false);
        u.activate(Item::StoreSaveSetup);
        assert!(u.take_file_request().is_none() && dir.join("setup1.txt").is_file(), "a slot is saved without a question");
        std::fs::remove_file(dir.join("setup1.txt")).unwrap();
        u.activate(Item::StoreSessSave);
        let ask = u.take_file_request().expect("a question for the dialog");
        assert_eq!((ask.kind, ask.save, ask.dir.as_path(), ask.name.as_str()), (FileKind::Session, true, dir.as_path(), "session-001.oss"));
        u.file_chosen(&ask, None);
        assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "canceled: nothing written");
        let elsewhere = dir.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        u.m.tscale = 4;
        u.file_chosen(&ask, Some(elsewhere.join("bench.oss")));
        assert!(elsewhere.join("bench.oss").is_file(), "the folder of the dialog, not the save folder");
        u.m.tscale = 7;
        u.activate(Item::StoreSessLoad);
        let ask = u.take_file_request().expect("a question for the dialog");
        assert_eq!((ask.kind, ask.save), (FileKind::Session, false));
        u.file_chosen(&ask, None);
        assert_eq!(u.m.tscale, 7, "canceled: nothing loaded");
        u.file_chosen(&ask, Some(elsewhere.join("bench.oss")));
        assert_eq!(u.m.tscale, 4);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// SCPI and the Quick key never ask: a command without a path takes the next free name, as before.
    #[test]
    fn scpi_and_the_quick_key_never_ask() {
        let (mut u, dir) = bench("auto");
        for native in [false, true] {
            u.desktop_panel = native.then_some(false);
            u.m.quick = Quick::SaveCsv;
            u.quick();
            u.file_save_auto(FileKind::FastFrame);
            u.file_save_auto(FileKind::Shot);
            assert!(u.take_file_request().is_none());
            assert!(u.shot_request.take().is_some_and(|p| p.ends_with(".png")));
        }
        let names: Vec<String> = std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.iter().filter(|n| n.starts_with("wave-") && n.ends_with(".csv")).count(), 2, "{names:?}");
        assert!(names.contains(&"frame-001.bin".to_string()) && names.contains(&"frame-002.bin".to_string()), "{names:?}");
        u.history.clear();
        u.file_load_latest(FileKind::FastFrame);
        assert!(!u.history.is_empty(), "{:?}", u.toast);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
