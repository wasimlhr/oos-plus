// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Where stored files go and how they are written: the storage directory and the stick, free and latest file names,
//! the check that a save follows no link and writes no device, and the sync before a stick is pulled. A new place to
//! store to, or a new rule for a name, goes here.

use super::*;

/// Default storage directory on the unit (created when missing).
pub const DEFAULT_DIR: &str = "/tmp/oos";
/// Folder on a USB stick (`/bin/oos-usb` mounts it at /mnt/usb).
pub const USB_DIR: &str = "/mnt/usb/OOS";

/// Where CSV, PNG, mask and fast-frame files go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StoreDest {
    /// USB stick when mounted, otherwise the storage directory
    Auto,
    /// USB stick (`/mnt/usb/OOS`)
    Stick,
    /// The local storage directory, also when a stick is mounted (on the unit `/tmp/oos`, in RAM)
    Local,
}

impl StoreDest {
    pub const ALL: [StoreDest; 3] = [StoreDest::Auto, StoreDest::Stick, StoreDest::Local];

    pub fn name(self) -> &'static str {
        match self {
            StoreDest::Auto => "Auto",
            StoreDest::Stick => "USB",
            StoreDest::Local => "Local",
        }
    }
}

/// A stick is mounted, by the text of /proc/mounts.
pub fn usb_mounted(mounts: &str) -> bool {
    mounts.lines().any(|l| l.split(' ').nth(1) == Some("/mnt/usb"))
}

/// First free `<prefix>-NNN.<ext>` in `dir` (001 .. 999, then the last one is overwritten).
pub fn next_name(dir: &Path, prefix: &str, ext: &str) -> PathBuf {
    let mut p = dir.join(format!("{}-999.{}", prefix, ext));
    for i in 1..1000 {
        let c = dir.join(format!("{}-{:03}.{}", prefix, i, ext));
        // a link counts as taken, also one that points nowhere
        if c.symlink_metadata().is_err() {
            p = c;
            break;
        }
    }
    p
}

/// `<prefix>-<stamp>.<ext>` (`<prefix>-<stamp>-2.<ext>` and on when taken) with a time stamp, else `next_name`.
pub fn next_name_at(dir: &Path, prefix: &str, ext: &str, stamp: Option<&str>) -> PathBuf {
    let Some(st) = stamp else { return next_name(dir, prefix, ext) };
    for i in 1..100 {
        let n = if i == 1 { format!("{}-{}.{}", prefix, st, ext) } else { format!("{}-{}-{}.{}", prefix, st, i, ext) };
        if dir.join(&n).symlink_metadata().is_err() {
            return dir.join(n);
        }
    }
    next_name(dir, prefix, ext)
}

/// File name of a path for toasts ("shot-001.png").
pub fn file_name(p: &Path) -> String {
    p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string())
}

/// How many waveform files of a stick the lists of files to open take (`wave_files_under`).
pub const STICK_FILES: usize = 40;

/// The waveform files (`WAVE_EXTS`) under `root`: `files_under`.
pub fn wave_files_under(root: &Path, depth: usize, max: usize) -> Vec<PathBuf> {
    files_under(root, WAVE_EXTS, depth, max)
}

/// The files with a suffix of `exts` under `root`, down to `depth` folders below it, at most `max`: the files of a
/// folder in the order of their names, then its folders. For a stick, whose files lie where their owner put them.
/// A link is not followed and a name that begins with a dot is left out (a stick is foreign data, `check_target`).
pub fn files_under(root: &Path, exts: &[&str], depth: usize, max: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut dirs = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, level)) = dirs.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = rd.filter_map(|e| e.ok()).collect();
        entries.sort_by_key(|e| e.file_name());
        let mut below = Vec::new();
        for e in entries {
            if e.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            // (the type of the entry itself: a link is neither a file nor a folder here)
            let Ok(kind) = e.file_type() else { continue };
            let path = e.path();
            if kind.is_file() {
                let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
                if exts.contains(&ext.as_str()) {
                    out.push(path);
                    if out.len() >= max {
                        return out;
                    }
                }
            } else if kind.is_dir() && level < depth {
                below.push((path, level + 1));
            }
        }
        // (a stack: the folders in the order of their names come off it last one first)
        dirs.extend(below.into_iter().rev());
    }
    out
}

/// The name of a file in a list of files to open: its name, and for a file in another folder of the stick than the
/// one the scope saves to, its path on the stick ("captures/a.csv"), so two of one name can be told apart.
pub fn list_name(p: &Path) -> String {
    let stick = Path::new(USB_DIR).parent().unwrap_or(Path::new("/"));
    match p.strip_prefix(stick) {
        Ok(rel) if rel.components().count() > 1 && !p.starts_with(USB_DIR) => rel.to_string_lossy().replace('\\', "/"),
        _ => file_name(p),
    }
}

/// A stick is foreign data and this program runs as root: a link or a device node on it, under a name a save would use,
/// could lead straight to the card. So only a regular file or a new one, and on the stick no link on the way there.
pub fn check_target(path: &Path) -> std::io::Result<()> {
    let refuse = |what: &str| {
        Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{}: {}", path.display(), what)))
    };
    // the vendor's data is never written (ADR 0009), whatever path a command was given; `..` is followed as written
    let mut plain = std::path::PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                plain.pop();
            }
            std::path::Component::CurDir => {}
            other => plain.push(other),
        }
    }
    if plain.starts_with("/rigol") {
        return refuse("the vendor's data is not written");
    }
    if path.starts_with("/mnt/usb") {
        for a in path.ancestors().skip(1).take_while(|a| a.starts_with("/mnt/usb") && *a != Path::new("/mnt/usb")) {
            if a.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
                return refuse("a link on the way there");
            }
        }
    }
    match path.symlink_metadata() {
        Ok(m) if !m.is_file() => refuse("not a regular file"),
        _ => Ok(()),
    }
}

/// Writes `data` to `path`, creating the directory first.
pub fn write(path: &Path, data: &[u8]) -> std::io::Result<()> {
    check_target(path)?;
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(path, data)
}

/// Highest numbered `<prefix>-NNN.<ext>` in `dir`, if any.
pub fn latest_name(dir: &Path, prefix: &str, ext: &str) -> Option<PathBuf> {
    let mut best: Option<(u32, PathBuf)> = None;
    let rd = std::fs::read_dir(dir).ok()?;
    for e in rd.flatten() {
        let s = e.file_name().to_string_lossy().into_owned();
        let want = format!("{}-", prefix);
        if let Some(rest) = s.strip_prefix(&want).and_then(|r| r.strip_suffix(&format!(".{}", ext))) {
            if let Ok(n) = rest.parse::<u32>() {
                if best.as_ref().is_none_or(|(b, _)| n >= *b) {
                    best = Some((n, e.path()));
                }
            }
        }
    }
    best.map(|x| x.1)
}

/// Writes everything cached to the disks (a stick can be pulled, the scope switched off at the mains).
pub fn sync_disks() {
    #[cfg(unix)]
    unsafe {
        libc::sync()
    };
}
