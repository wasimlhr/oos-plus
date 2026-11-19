// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Storage: a file by its name (ADR 0029). A row that saves asks for the name of its file, a row that loads asks which
//! file. On the scope the name comes from the on-screen keyboard, with the next free name already in it, and the file
//! is picked from a list of the folder, a picker that scrolls when it is long (ADR 0006). The desktop build asks with the dialogs of its system
//! (`desktop/picker.rs`): the question is left in `file_request` and the window loop answers it. A path given by SCPI,
//! a command without one and the Quick key never ask.

use super::*;
use std::path::{Path, PathBuf};

/// Longest name the keyboard takes (without the suffix).
pub(super) const NAME_MAX: usize = 48;
/// Longest list of a folder: the newest files.
const LIST_MAX: usize = 500;

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

    /// The word for it in a title or a message.
    pub fn what(self) -> &'static str {
        match self {
            FileKind::Setup => "setup",
            FileKind::SetupScpi => "SCPI setup",
            FileKind::Session => "session",
            FileKind::WaveCsv | FileKind::WaveBin => "waveform",
            FileKind::Shot => "screenshot",
            FileKind::FastFrame => "fast-frame",
        }
    }

    /// How a file of this kind starts, where the suffix does not say it: a mask is a `.txt` too, a waveform a `.bin`.
    fn magic(self) -> &'static [u8] {
        match self {
            FileKind::Setup => b"# OOS setup",
            FileKind::Session => b"OOSS",
            FileKind::FastFrame => b"OOSF",
            _ => b"",
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

/// The file name of what was typed: the suffix is added unless it was typed too. A name is one file of the folder, so
/// nothing that names another place, and nothing a FAT stick refuses.
pub(super) fn typed_name(text: &str, ext: &str) -> Result<String, String> {
    let t = text.trim();
    let cut = t.len().saturating_sub(ext.len() + 1);
    let stem = if t.is_char_boundary(cut) && t[cut..].eq_ignore_ascii_case(&format!(".{ext}")) { t[..cut].trim_end() } else { t };
    if stem.is_empty() {
        return Err("Enter a name for the file".into());
    }
    if stem.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']) || stem.chars().any(char::is_control) {
        return Err("A file name has none of / \\ : * ? \" < > |".into());
    }
    if stem.starts_with('.') || stem.ends_with('.') {
        return Err("A file name does not start or end with a dot".into());
    }
    Ok(format!("{stem}.{ext}"))
}

/// Does the file start with `magic`? An empty one is not asked for.
fn starts_with(path: &Path, magic: &[u8]) -> bool {
    use std::io::Read;
    if magic.is_empty() {
        return true;
    }
    let mut head = [0u8; 16];
    let n = magic.len().min(head.len());
    std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head[..n])).is_ok() && head[..n] == magic[..n]
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

    /// The next free name of `kind` in its folder: what a save without a name takes, and what the keyboard and the
    /// dialog propose. A session and a fast-frame file keep their running number, which the load without a name finds.
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
        let next = self.file_next(kind);
        self.file_replace = None;
        if self.native_files() {
            self.file_request = Some(FileAsk { kind, save: true, dir: self.file_dir(kind), name: crate::store::file_name(&next) });
        } else {
            let stem = next.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            self.wlan.osk = Some(wlan::Osk::proposing(wlan::OskFor::File(kind), &stem));
        }
    }

    /// OK of the keyboard for a file name; false keeps the keyboard, with the reason in a toast. A name that is taken
    /// is replaced by a second OK, not by the first.
    pub(super) fn file_name_done(&mut self, kind: FileKind, text: &str) -> bool {
        let name = match typed_name(text, kind.ext()) {
            Ok(n) => n,
            Err(e) => {
                self.toast(&e);
                return false;
            }
        };
        let path = self.file_dir(kind).join(&name);
        if path.symlink_metadata().is_ok() && self.file_replace.as_deref() != Some(path.as_path()) {
            self.toast(&format!("{name} is there: OK once more replaces it"));
            self.file_replace = Some(path);
            return false;
        }
        self.file_replace = None;
        self.file_save(kind, path);
        true
    }

    /// Saves `kind` to the file the user named, on the keyboard or in the dialog.
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
                // for the SCPI recorder: the command saves under the next free name of the scope that runs it
                if self.last[0].is_some() {
                    self.rec_action(if csv { ":SAVE:WAVeform" } else { ":SAVE:WAVeform ,BIN" });
                }
                let la = path.file_stem().filter(|_| csv).map(|s| path.with_file_name(format!("{}-la.csv", s.to_string_lossy())));
                self.store_wave_to(csv, path, la);
            }
            FileKind::Shot => {
                self.rec_action(":SAVE:IMAGe");
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

    /// The files of `kind` in its folder, the newest first.
    fn files_of(&self, kind: FileKind) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(self.file_dir(kind)) else { return Vec::new() };
        let mut v: Vec<(std::time::SystemTime, PathBuf)> = rd.flatten().filter_map(|e| {
            let p = e.path();
            let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
            // a setup of the stock naming (`:SAVE:SETup "name.stp"`) is a setup too
            let fits = ext == kind.ext() || (kind == FileKind::Setup && ext == "stp");
            let meta = e.metadata().ok().filter(|m| m.is_file())?;
            (fits && starts_with(&p, kind.magic())).then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), p))
        }).collect();
        // files of the same second (a stick keeps two seconds): the later name first
        v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
        v.truncate(LIST_MAX);
        v.into_iter().map(|x| x.1).collect()
    }

    /// A Storage row that loads: asks which file, then loads it (`file_load`).
    pub(super) fn file_load_ask(&mut self, kind: FileKind) {
        let dir = self.file_dir(kind);
        if self.native_files() {
            self.file_request = Some(FileAsk { kind, save: false, dir, name: String::new() });
            return;
        }
        self.file_list = self.files_of(kind);
        self.file_kind = kind;
        if self.file_list.is_empty() {
            return self.toast(&format!("No {} file (.{}) in {}", kind.what(), kind.ext(), dir.display()));
        }
        self.open_picker(Item::StoreFile);
    }

    /// The file tapped in the list.
    pub(super) fn file_pick(&mut self, k: usize) {
        let p = self.file_list.get(k).cloned();
        self.close_picker();
        if let Some(p) = p {
            self.file_load(self.file_kind, &p);
        }
    }

    /// Loads the file the user chose, in the list or in the dialog.
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

    // ------------------------------------------------------------------------------------------- the list

    /// The folder of `kind` for a title: the stick by its name, a long path by its end.
    fn file_place(&self, kind: FileKind) -> String {
        let dir = self.file_dir(kind);
        if dir == Path::new(crate::store::USB_DIR) {
            return "USB stick".into();
        }
        let (text, room) = (dir.display().to_string(), 30);
        let n = text.chars().count();
        if n <= room { text } else { format!("\u{2026}{}", text.chars().skip(n + 1 - room).collect::<String>()) }
    }

    /// Title of the keyboard that asks for a file name.
    pub(super) fn file_name_title(&self, kind: FileKind) -> String {
        format!("Name of the {} file (.{}) \u{b7} {}", kind.what(), kind.ext(), self.file_place(kind))
    }

    /// Title of the list.
    pub(super) fn file_list_title(&self) -> String {
        format!("Load {} \u{b7} {}", self.file_kind.what(), self.file_place(self.file_kind))
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

    fn keyboard(u: &Ui) -> String {
        u.wlan.osk.as_ref().map(|o| o.text.clone()).expect("the keyboard is open")
    }

    #[test]
    fn a_typed_name_is_one_file_of_the_folder() {
        assert_eq!(typed_name(" ripple 5V ", "csv").unwrap(), "ripple 5V.csv");
        assert_eq!(typed_name("ripple.CSV", "csv").unwrap(), "ripple.csv");
        assert_eq!(typed_name("v1.2", "oss").unwrap(), "v1.2.oss");
        for bad in ["", "  ", ".csv", "../up", "a/b", "a\\b", "C:x", "what?", ".hidden", "dot.", "tab\tname"] {
            assert!(typed_name(bad, "csv").is_err(), "{bad:?}");
        }
    }

    /// Every row that saves asks for the name first, with the next free name in the keyboard; OK writes that file.
    #[test]
    fn a_save_asks_for_the_name_on_the_scope() {
        let (mut u, dir) = bench("ask");
        for (it, kind) in [(Item::StoreSetupSave, FileKind::Setup), (Item::StoreSessSave, FileKind::Session), (Item::StoreCsv, FileKind::WaveCsv),
            (Item::StoreBin, FileKind::WaveBin), (Item::StoreFfSave, FileKind::FastFrame)] {
            u.open_menu(MenuId::Storage);
            u.activate(it);
            assert!(u.take_file_request().is_none(), "no dialog of a system on the scope");
            let proposed = keyboard(&u);
            assert!(proposed.starts_with(kind.prefix()), "{it:?} proposes {proposed}");
            assert!(std::fs::read_dir(&dir).unwrap().next().is_none(), "{it:?}: nothing is written before OK");
            u.wlan.osk.as_mut().unwrap().text = format!("bench {}", kind.prefix());
            u.osk_ok();
            assert!(u.wlan.osk.is_none(), "{it:?}: {:?}", u.toast);
            let file = dir.join(format!("bench {}.{}", kind.prefix(), kind.ext()));
            assert!(file.is_file(), "{it:?} wrote {}", file.display());
            std::fs::remove_file(file).unwrap();
        }
        // the two files that are written later, by the main loop and by the interpreter: the request holds the name
        u.open_menu(MenuId::Storage);
        u.activate(Item::StorePng);
        u.wlan.osk.as_mut().unwrap().text = "front".into();
        u.osk_ok();
        assert_eq!(u.shot_request.take(), Some(dir.join("front.png").to_string_lossy().into_owned()));
        assert!(!u.menu_open, "the screenshot is taken without the menu on it");
        u.open_menu(MenuId::Storage);
        u.activate(Item::StoreSaveScpi);
        u.wlan.osk.as_mut().unwrap().text = "bench.scpi".into();
        u.osk_ok();
        assert_eq!(u.setup_scpi_request.take(), Some(dir.join("bench.scpi")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The name in the keyboard is a proposal: the first character typed takes its place, so a name of one's own
    /// needs no twenty taps on Delete; an arrow keeps it, to add to it.
    #[test]
    fn typing_replaces_the_proposed_name() {
        let (mut u, dir) = bench("propose");
        u.activate(Item::StoreSessSave);
        assert_eq!(keyboard(&u), "session-001");
        for c in "amp".chars() {
            u.osk_type(c);
        }
        assert_eq!(keyboard(&u), "amp");
        u.wlan.osk = None;
        u.activate(Item::StoreSessSave);
        u.osk_right();
        for c in "-b".chars() {
            u.osk_type(c);
        }
        assert_eq!(keyboard(&u), "session-001-b");
        u.osk_ok();
        assert!(dir.join("session-001-b.oss").is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A name that is no file name keeps the keyboard, and so does one that is taken, until OK is given once more.
    #[test]
    fn a_bad_name_and_a_taken_name_keep_the_keyboard() {
        let (mut u, dir) = bench("taken");
        u.activate(Item::StoreSessSave);
        u.wlan.osk.as_mut().unwrap().text = "a/b".into();
        u.osk_ok();
        assert!(u.wlan.osk.is_some(), "a path is not a name");
        u.wlan.osk.as_mut().unwrap().text = "first".into();
        u.osk_ok();
        assert!(u.wlan.osk.is_none());
        let file = dir.join("first.oss");
        let before = std::fs::read(&file).unwrap();
        u.m.ch[1].on = !u.m.ch[1].on;
        u.activate(Item::StoreSessSave);
        u.wlan.osk.as_mut().unwrap().text = "first".into();
        u.osk_ok();
        assert!(u.wlan.osk.is_some(), "the first OK only says that the name is taken");
        assert_eq!(std::fs::read(&file).unwrap(), before);
        u.osk_ok();
        assert!(u.wlan.osk.is_none());
        assert_ne!(std::fs::read(&file).unwrap(), before, "the second OK replaced it");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Load session lists the sessions of the folder, the newest first, and loads the one that is tapped: not only the
    /// last one.
    #[test]
    fn a_load_lists_the_files_and_loads_the_one_chosen() {
        let (mut u, dir) = bench("list");
        std::fs::write(dir.join("notes.oss"), b"not a session").unwrap();
        std::fs::write(dir.join("wave-001.bin"), crate::store::wave_bin(u.last[0].as_ref().unwrap(), &u.m)).unwrap();
        u.m.tscale = 3;
        u.file_save(FileKind::Session, dir.join("slow.oss"));
        u.m.tscale = 9;
        u.file_save(FileKind::Session, dir.join("fast.oss"));
        u.m.tscale = 5;
        u.open_menu(MenuId::Storage);
        u.activate(Item::StoreSessLoad);
        assert_eq!(u.menu, MenuId::Pick);
        let mut names = u.options(Item::StoreFile);
        names.sort();
        assert_eq!(names, ["fast.oss", "slow.oss"], "sessions only, by what they hold");
        let k = u.options(Item::StoreFile).iter().position(|n| n == "slow.oss").unwrap();
        u.activate(Item::PickOpt(k));
        assert_eq!(u.m.tscale, 3, "{:?}", u.toast);
        assert_eq!(u.menu, MenuId::Storage, "back in the menu the list was opened from");
        // nothing to list: a message, no empty list
        for f in ["slow.oss", "fast.oss"] {
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        u.activate(Item::StoreSessLoad);
        assert_eq!(u.menu, MenuId::Storage);
        assert!(u.toast.as_ref().is_some_and(|t| t.0.starts_with("No session file")), "{:?}", u.toast);
        // fast-frame files are told from waveforms, which are .bin too
        u.file_save(FileKind::FastFrame, dir.join("burst.bin"));
        u.activate(Item::StoreFfLoad);
        assert_eq!(u.options(Item::StoreFile), ["burst.bin"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The slots stay the quick way: Save setup and Recall setup never ask. A setup by name is a file beside them.
    #[test]
    fn slots_stay_and_a_setup_by_name_is_a_file_beside_them() {
        let (mut u, dir) = bench("setup");
        u.store_slot = 4;
        u.activate(Item::StoreSaveSetup);
        assert!(u.wlan.osk.is_none() && dir.join("setup4.txt").is_file());
        u.m.tscale = 2;
        u.activate(Item::StoreSetupSave);
        assert!(keyboard(&u).starts_with("setup-"), "not a slot's name: {}", keyboard(&u));
        u.wlan.osk.as_mut().unwrap().text = "ripple".into();
        u.osk_ok();
        assert!(dir.join("ripple.txt").is_file());
        std::fs::write(dir.join("mask-001.txt"), "# OOS mask v1\n").unwrap();
        u.m.tscale = 8;
        u.activate(Item::StoreSetupLoad);
        let names = u.options(Item::StoreFile);
        assert!(names.contains(&"ripple.txt".to_string()) && names.contains(&"setup4.txt".to_string()), "{names:?}");
        assert!(!names.contains(&"mask-001.txt".to_string()), "a mask is no setup");
        u.activate(Item::PickOpt(names.iter().position(|n| n == "ripple.txt").unwrap()));
        assert_eq!(u.m.tscale, 2);
        u.m.tscale = 8;
        u.activate(Item::StoreRecall);
        assert_ne!(u.m.tscale, 8, "the slot is recalled without a question");
        assert_eq!(u.menu_open && u.menu == MenuId::Pick, false);
        let _ = std::fs::remove_dir_all(&dir);
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

    /// The desktop build: the row leaves a question for the dialog and opens no keyboard and no list; the answer saves
    /// or loads, and a dialog closed without a file changes nothing.
    #[test]
    fn the_desktop_build_asks_with_its_dialog() {
        let (mut u, dir) = bench("desktop");
        u.desktop_panel = Some(false);
        u.activate(Item::StoreSessSave);
        assert!(u.wlan.osk.is_none());
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
        assert!(!(u.menu_open && u.menu == MenuId::Pick), "no list where the system has a dialog");
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
            assert!(u.wlan.osk.is_none() && u.take_file_request().is_none());
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

    /// Studio and Minimal show a menu as a sheet: the keyboard is the same one, and the list is the sheet's own list
    /// (in pages), not the dialog of the Default layout.
    #[test]
    fn the_sheets_ask_with_the_keyboard_and_their_own_list() {
        let (mut u, dir) = bench("sheet");
        u.m.layout = Layout::Studio;
        u.studio.enter = true;
        for _ in 0..8 {
            u.step(0.05);
        }
        u.open_menu(MenuId::Storage);
        assert!(u.sheet_active());
        u.m.tscale = 6;
        u.activate(Item::StoreSessSave);
        u.wlan.osk.as_mut().unwrap().text = "studio".into();
        u.osk_ok();
        assert!(dir.join("studio.oss").is_file(), "{:?}", u.toast);
        u.m.tscale = 9;
        u.activate(Item::StoreSessLoad);
        assert!(u.sheet_active());
        let (cells, _, _) = u.sheet_pick_cells();
        assert_eq!(cells.len(), 1);
        u.tap(cells[0].1.x + 10, cells[0].1.y + 10);
        assert_eq!(u.m.tscale, 6, "{:?}", u.toast);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A folder with more files than the screen has rows: the list is a long picker, which scrolls by its bar and by
    /// the finger (ADR 0006), and a tap on a file loads it.
    #[test]
    fn a_long_list_scrolls() {
        let (mut u, dir) = bench("scroll");
        let session = {
            u.file_save(FileKind::Session, dir.join("s000.oss"));
            std::fs::read(dir.join("s000.oss")).unwrap()
        };
        // the picker shows its options in four columns: 200 files are more lines than the screen has
        for i in 1..200 {
            std::fs::write(dir.join(format!("s{i:03}.oss")), &session).unwrap();
        }
        u.open_menu(MenuId::Storage);
        u.activate(Item::StoreSessLoad);
        let d = u.dlg_layout().unwrap();
        let (bar, _) = d.bar.expect("a list longer than the screen has a scroll bar");
        let shown = d.rows.iter().filter(|r| r.ctrl.w > 0).count();
        assert!(bar.needed() && shown < 200 && d.r.bottom() <= u.frame().bottom.y, "{shown} rows in {:?}", d.r);
        // the thumb to the end
        let th = bar.thumb();
        u.handle(Ev::Touch { id: 0, phase: Phase::Down, x: th.x + 5, y: th.y + 5 });
        u.handle(Ev::Touch { id: 0, phase: Phase::Move, x: th.x + 5, y: bar.track.bottom() + 40 });
        u.handle(Ev::Touch { id: 0, phase: Phase::Up, x: th.x + 5, y: bar.track.bottom() + 40 });
        assert_eq!(u.menu, MenuId::Pick, "a finger that moved the list picked nothing");
        let d = u.dlg_layout().unwrap();
        let last = d.rows.iter().filter(|r| r.ctrl.w > 0).last().map(|r| (r.it, r.ctrl)).unwrap();
        assert_eq!(last.0, Item::PickOpt(199));
        // a press that does not move is a tap on the file under it
        u.tap(last.1.x + 20, last.1.y + 20);
        assert_eq!(u.menu, MenuId::Storage);
        assert!(u.toast.as_ref().is_some_and(|t| t.0.starts_with("Loaded ")), "{:?} for {:?}", u.toast, last.0);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
