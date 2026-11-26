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
    let stem = if t.is_char_boundary(cut) && t[cut..].eq_ignore_ascii_case(&format!(".{ext}")) {
        t[..cut].trim_end()
    } else {
        t
    };
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

/// The suffix of the session archive of another instrument (docs/tss.md), which Load session lists and loads beside
/// the sessions of OOS. Save session writes OOS's own file only.
const FOREIGN_SESSION: &str = "tss";

/// Whether `path` is such an archive, for a list of sessions: by its suffix and its first bytes, those of a zip.
fn foreign_session(kind: FileKind, path: &Path) -> bool {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    kind == FileKind::Session && ext.eq_ignore_ascii_case(FOREIGN_SESSION) && starts_with(path, b"PK\x03\x04")
}

impl Ui {
    /// The desktop build asks with the dialogs of its system; the scope has none.
    fn native_files(&self) -> bool {
        self.workspace.desktop_panel.is_some()
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
            FileKind::FastFrame if self.history.recs.is_empty() && self.last[0].is_none() => Some("No frames to save"),
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
            self.file_request =
                Some(FileAsk { kind, save: true, dir: self.file_dir(kind), name: crate::store::file_name(&next) });
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
                let la = path
                    .file_stem()
                    .filter(|_| csv)
                    .map(|s| path.with_file_name(format!("{}-la.csv", s.to_string_lossy())));
                self.store_wave_to(csv, path, la);
            }
            FileKind::Shot => {
                self.rec_action(":SAVE:IMAGe");
                // without the dialog on it: the screen is saved after the next frame
                self.close_menu();
                self.asks.shot = Some(path.to_string_lossy().into_owned());
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
        let mut v: Vec<(std::time::SystemTime, PathBuf)> = rd
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
                // a setup of the stock naming (`:SAVE:SETup "name.stp"`) is a setup too
                let fits = ext == kind.ext() || (kind == FileKind::Setup && ext == "stp");
                let meta = e.metadata().ok().filter(|m| m.is_file())?;
                (fits && starts_with(&p, kind.magic()) || foreign_session(kind, &p))
                    .then(|| (meta.modified().unwrap_or(std::time::UNIX_EPOCH), p))
            })
            .collect();
        // on a stick, the session archives of another instrument wherever they lie, as its waveform files are found
        // (`ref_wave_candidates`): they were copied there on a PC, not saved into the folder of OOS
        if let (FileKind::Session, true, Some(stick)) =
            (kind, self.stick.mounted, Path::new(crate::store::USB_DIR).parent())
        {
            for p in crate::store::files_under(stick, &[FOREIGN_SESSION], 3, crate::store::STICK_FILES) {
                if foreign_session(kind, &p) && !v.iter().any(|(_, q)| *q == p) {
                    v.push((p.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p));
                }
            }
        }
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
        if n <= room {
            text
        } else {
            format!("\u{2026}{}", text.chars().skip(n + 1 - room).collect::<String>())
        }
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
        u.files.dir = dir.clone();
        u.files.dest = crate::store::StoreDest::Local;
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
        for (it, kind) in [
            (Item::StoreSetupSave, FileKind::Setup),
            (Item::StoreSessSave, FileKind::Session),
            (Item::StoreCsv, FileKind::WaveCsv),
            (Item::StoreBin, FileKind::WaveBin),
            (Item::StoreFfSave, FileKind::FastFrame),
        ] {
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
        assert_eq!(u.asks.shot.take(), Some(dir.join("front.png").to_string_lossy().into_owned()));
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
        u.files.slot = 4;
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
        u.files.dest = crate::store::StoreDest::Stick;
        assert_eq!(u.file_dir(FileKind::Setup), Path::new(crate::store::USB_DIR));
        assert_eq!(u.setup_path().parent(), Some(dir.as_path()), "the slots stay where they are kept");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The desktop build: the row leaves a question for the dialog and opens no keyboard and no list; the answer saves
    /// or loads, and a dialog closed without a file changes nothing.
    #[test]
    fn the_desktop_build_asks_with_its_dialog() {
        let (mut u, dir) = bench("desktop");
        u.workspace.desktop_panel = Some(false);
        u.activate(Item::StoreSessSave);
        assert!(u.wlan.osk.is_none());
        let ask = u.take_file_request().expect("a question for the dialog");
        assert_eq!(
            (ask.kind, ask.save, ask.dir.as_path(), ask.name.as_str()),
            (FileKind::Session, true, dir.as_path(), "session-001.oss")
        );
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
            u.workspace.desktop_panel = native.then_some(false);
            u.m.quick = Quick::SaveCsv;
            u.quick();
            u.file_save_auto(FileKind::FastFrame);
            u.file_save_auto(FileKind::Shot);
            assert!(u.wlan.osk.is_none() && u.take_file_request().is_none());
            assert!(u.asks.shot.take().is_some_and(|p| p.ends_with(".png")));
        }
        let names: Vec<String> =
            std::fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert_eq!(names.iter().filter(|n| n.starts_with("wave-") && n.ends_with(".csv")).count(), 2, "{names:?}");
        assert!(
            names.contains(&"frame-001.bin".to_string()) && names.contains(&"frame-002.bin".to_string()),
            "{names:?}"
        );
        u.history.recs.clear();
        u.file_load_latest(FileKind::FastFrame);
        assert!(!u.history.recs.is_empty(), "{:?}", u.toast);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A ramp of `n` samples, 1 ns apart from `t0` on, as a `:WFM#003` file.
    fn ramp(n: usize, t0: f64, top: f32) -> Vec<u8> {
        let v: Vec<f32> = (0..n).map(|k| top * k as f32 / n as f32).collect();
        oos_store::testkit::wfm_f32(b":WFM#003", t0, 1e-9, &v, "")
    }

    /// The settings of a session of another instrument (docs/tss.md): two channels and two references.
    const FOREIGN: &str = "*RST;:PARAMBATCHING 0;:ACQUIRE:STATE 0\n:CH1:SCALE 200.0000E-3\n:CH1:POSITION -2.0000\n\
        :CH1:OFFSET 0.0E+0\n:CH1:LABEL:NAME \"Rail%203V3\"\n:CH3:SCALE 25.0000E-3\n:CH3:POSITION 0.0E+0\n\
        :DISPLAY:GLOBAL:CH1:STATE 1\n:DISPLAY:GLOBAL:CH3:STATE 1\n:HORIZONTAL:SCALE 1.0000E-6\n\
        :HORIZONTAL:POSITION 50.0000\n:HORIZONTAL:DELAY:MODE 0\n:TRIGGER:A:EDGE:SOURCE CH3\n:BUS:B1:TYPE I2C\n\
        :DISPLAY:WAVEVIEW1:REF:REF2:VERTICAL:SCALE 2.0000\n:DISPLAY:WAVEVIEW1:REF:REF2:VERTICAL:POSITION 1.5000\n\
        :DISPLAY:GLOBAL:REF2:STATE 1\n:REF:REF2:LABEL:NAME \"Hall A\"\n:DISPLAY:GLOBAL:REF5:STATE 0\n";

    /// The session archive of another instrument loads as a session, down the path of OOS's own: its channel records
    /// are the record, at full length and with their time step, the references are in the slots of their numbers
    /// under their names, the scope is stopped on the loaded record, and the message says what was taken and what
    /// not. Load session lists the file beside the sessions of OOS, in the dialog of Default and on the sheet of
    /// Studio, and `:LOAD:SESSion` and a file named at the start take it too.
    #[test]
    fn a_session_of_another_instrument_loads_as_a_session() {
        use oos_store::testkit::tss;
        let (mut u, dir) = bench("tss");
        let mut bits = ramp(100, 0.0, 1.0);
        bits[122] = 6;
        let refs =
            [(2, "hall.wfm", ramp(4000, -2e-6, 3.0)), (5, "edge.wfm", ramp(300, 0.0, 1.0)), (6, "bits.wfm", bits)];
        let beside = [("ch1.wfm", ramp(10_000, -5e-6, 0.8)), ("ch3.wfm", ramp(10_000, -5e-6, 0.1))];
        let file = dir.join("faster scope.tss");
        std::fs::write(&file, tss(FOREIGN, &refs, &beside)).unwrap();
        std::fs::write(dir.join("notes.tss"), b"not an archive").unwrap();
        u.file_save(FileKind::Session, dir.join("own.oss"));
        u.refs[8] = Some(RefWave {
            src: 0,
            color: 0,
            label: "old".into(),
            t0: 0.0,
            dt: 1e-9,
            v: vec![0.0; 16].into(),
            vpd: 1.0,
            offset: 0.0,
            on: true,
        });

        u.open_menu(MenuId::Storage);
        u.activate(Item::StoreSessLoad);
        let mut names = u.options(Item::StoreFile);
        names.sort();
        assert_eq!(names, ["faster scope.tss", "own.oss"], "the sessions of both kinds, by what they hold");
        let k = u.options(Item::StoreFile).iter().position(|n| n == "faster scope.tss").unwrap();
        u.activate(Item::PickOpt(k));
        let said = u.toast.as_ref().unwrap().0.clone();
        assert_eq!(said, "Loaded faster scope.tss: waveforms, scales, time base and filters, without its trigger, buses, measurements, other maths; not read: 1 digital waveform");
        assert!(u.fonts().mb.width(&said) + 44 <= W, "the message fits the screen");

        // the record: both channels, every sample, on the time axis of the files
        for (c, top) in [(0, 0.8f32), (2, 0.1)] {
            let (v, dt, t0) = u.channel_view(c, true).unwrap();
            assert_eq!((v.len(), dt, t0), (10_000, 1e-9, -5e-6), "CH{}", c + 1);
            assert_eq!(v[5000], top * 0.5);
        }
        assert!(u.channel_view(1, true).is_none() && u.channel_view(3, true).is_none());
        let on: Vec<bool> = u.m.ch.iter().map(|ch| ch.on).collect();
        assert_eq!(on, [true, false, true, false]);
        assert_eq!((u.m.ch[0].volts_per_div(), u.m.ch[0].offset, u.m.ch_label[0].as_str()), (0.2, -0.4, "Rail 3V3"));
        assert_eq!((VSCALES[u.m.ch[2].vscale], u.m.ch[2].vfine, u.m.ch[2].offset), (0.02, 1.25, 0.0));
        // the window of the settings: 1 us a division around the trigger, which is the whole record
        assert_eq!((u.m.time_per_div(), u.m.delay), (1e-6, 0.0));
        // stopped on the loaded record, as after a session of OOS; the references beside it, not in its place
        assert!(u.m.run == RunState::Stop && u.loaded_record && !u.record_mode());
        let hall = u.refs[1].as_ref().unwrap();
        assert_eq!(
            (hall.label.as_str(), hall.v.len(), hall.dt, hall.t0, hall.vpd, hall.offset, hall.on),
            ("Hall A", 4000, 1e-9, -2e-6, 2.0, 3.0, true)
        );
        let edge = u.refs[4].as_ref().unwrap();
        assert_eq!((edge.label.as_str(), edge.v.len(), edge.on), ("edge", 300, false));
        assert!(edge.vpd > 0.0 && u.refs[5].is_none() && u.refs[8].is_none(), "a session is the whole bench");
        // what is not mapped is the default, not a guess: the trigger of the settings was on CH3
        assert_eq!((u.m.trig_src, u.m.trig_type, u.m.dec_on), (0, Model::default().trig_type, Model::default().dec_on));

        // by SCPI, and named at the start of the program
        u.m.ch[0].on = false;
        u.session_load(file.to_str().unwrap()).unwrap();
        assert!(u.m.ch[0].on && u.refs[1].is_some());
        let mut other = Ui::new(Source::Demo { t: 0.0 });
        crate::open_file(&mut other, file.to_str().unwrap()).unwrap();
        assert_eq!(other.channel_view(2, true).unwrap().0.len(), 10_000);
        assert!(other.refs[1].is_some() && other.loaded_record);
        // Save session writes OOS's own file, which holds what was loaded
        other.files.dir = dir.clone();
        let saved = other.session_save("").unwrap();
        assert!(saved.extension().unwrap() == "oss" && crate::store::looks_session(&std::fs::read(&saved).unwrap()));

        // the sheet of Studio lists it too
        u.m.layout = Layout::Studio;
        u.studio.enter = true;
        for _ in 0..8 {
            u.step(0.05);
        }
        u.open_menu(MenuId::Storage);
        u.activate(Item::StoreSessLoad);
        assert!(u.sheet_active());
        assert_eq!(u.sheet_pick_cells().0.len(), 3, "own.oss, the one just saved and the archive");
        assert!(u.file_list.contains(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A session archive with references only fills the screen with its first reference, in the offline view, on the
    /// time base of that record; one without a waveform, a damaged one and one that is no session leave the bench as
    /// it is and say why.
    #[test]
    fn a_session_of_another_instrument_with_references_only_and_one_that_cannot_be_read() {
        use oos_store::testkit::{tss, zip, ZipFile};
        let (mut u, dir) = bench("tss-refs");
        let file = dir.join("refs.tss");
        let refs = [(3, "late.wfm", ramp(500, 0.0, 1.0)), (2, "first.wfm", ramp(20_000, -4e-6, 2.5))];
        std::fs::write(&file, tss(":HORIZONTAL:SCALE 4.0000E-3\n:HORIZONTAL:POSITION 50.0000\n", &refs, &[])).unwrap();
        u.session_read(file.clone()).unwrap();
        assert!(u.record_mode() && u.offline.on && u.offline.slot == 1, "the offline view on R2");
        assert!(u.m.ch.iter().all(|ch| !ch.on) && u.m.run == RunState::Stop);
        let first = u.refs[1].as_ref().unwrap();
        assert_eq!((first.label.as_str(), first.v.len(), first.on), ("first", 20_000, true));
        assert_eq!((first.vpd, first.offset), (0.5, -1.0), "fitted to the samples: no scale in the settings");
        assert_eq!(u.refs[2].as_ref().unwrap().v.len(), 500);
        // the record of R2 on the screen (20 us from -4 us), not the 40 ms of the settings, which are the acquisition's
        assert_eq!((u.m.time_per_div(), u.m.delay), (2e-6, 6e-6));

        let before = (u.m.clone(), u.refs[1].as_ref().map(|r| r.v.len()));
        let mut refused = |name: &str, bytes: Vec<u8>| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            let e = u.session_read(p).unwrap_err();
            assert_eq!(u.toast.as_ref().unwrap().0, format!("Load failed: {e}"));
            assert_eq!((u.m.clone(), u.refs[1].as_ref().map(|r| r.v.len())), before, "{name}: the bench is as it was");
            e
        };
        assert_eq!(refused("empty.tss", tss("", &[], &[])), "the session holds no waveform");
        let good = tss("", &refs, &[]);
        assert_eq!(refused("cut.tss", good[..good.len() - 30].to_vec()), "the archive is cut short or damaged");
        assert!(refused("plain.tss", zip(&[ZipFile::stored("a.txt", b"hello")])).contains("holds no session"));
        assert_eq!(refused("text.tss", b"OOSW and no more".to_vec()), "not an OOS session file");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The local check: every session archive under the folder `OOS_TSS_DIR` loads as a session or is refused with a
    /// reason, and the counts are printed (`cargo test local_folder_load_as -- --nocapture`). Skipped when the
    /// variable is not set: the archives of an instrument are not part of the tree.
    #[test]
    fn the_session_archives_of_a_local_folder_load_as_sessions_or_say_why() {
        let Ok(folder) = std::env::var("OOS_TSS_DIR") else { return };
        let files = crate::store::files_under(Path::new(&folder), &[FOREIGN_SESSION], 4, 10_000);
        let (mut with_record, mut references_only, mut left_out, mut longest, mut slowest) = (0, 0, 0, 0, (0.0, 0, 0));
        let mut refused: std::collections::BTreeMap<String, usize> = Default::default();
        for (k, p) in files.iter().enumerate() {
            let mut u = Ui::new(Source::Demo { t: 0.0 });
            let start = std::time::Instant::now();
            match u.session_read(p.clone()) {
                Ok(()) => {
                    let took = start.elapsed().as_secs_f64();
                    let record = (0..NCH).filter_map(|c| u.channel_view(c, true)).map(|(v, _, _)| v.len()).max();
                    let slots = u.refs.iter().flatten().count();
                    assert!(u.m.run == RunState::Stop && u.loaded_record, "file {k}");
                    assert!(record.is_some() != u.offline.on && (record.is_some() || slots > 0), "file {k}");
                    assert!(u.m.time_per_div() > 0.0 && u.m.delay.is_finite(), "file {k}");
                    with_record += record.is_some() as usize;
                    references_only += record.is_none() as usize;
                    left_out += u.toast.as_ref().is_some_and(|t| t.0.contains("not read:")) as usize;
                    longest = longest.max(record.unwrap_or(0));
                    if took > slowest.0 {
                        slowest = (took, std::fs::metadata(p).map_or(0, |m| m.len()), record.unwrap_or(0));
                    }
                }
                Err(e) => {
                    assert!(
                        !e.is_empty() && u.toast.as_ref().is_some_and(|t| t.0.starts_with("Load failed: ")),
                        "file {k}"
                    );
                    *refused.entry(e).or_default() += 1;
                }
            }
        }
        let loaded = with_record + references_only;
        println!("{} archives: {loaded} load as a session, {} are refused", files.len(), files.len() - loaded);
        println!("{with_record} with a record of channels, {references_only} with references only, {left_out} with waveforms left out");
        println!("the longest record has {longest} samples a channel");
        println!(
            "the slowest load took {:.2} s: {} bytes, a record of {} samples a channel",
            slowest.0, slowest.1, slowest.2
        );
        for (reason, n) in &refused {
            println!("  {n} x {reason}");
        }
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
