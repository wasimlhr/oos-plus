// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the set-up file of the desktop build (`SetupFile`). The fork's readers of other scopes' waveform files
//! are in the crate oos-store (`ui/store/src/readers.rs`).

use super::*;
use std::path::{Path, PathBuf};

/// The setup the desktop build keeps between runs, as the unit keeps it in /tmp/oos-ui.setup: the same text
/// (`setup_text`), so the layout and the Studio view come back after a restart. It lives in the storage directory
/// (`Ui::store_dir`: the user's Documents\OOS on Windows, or `--store-dir`).
#[cfg_attr(not(test), allow(dead_code))] // the Windows desktop and the tests use it
pub struct SetupFile {
    path: PathBuf,
    last: String,
    t: std::time::Instant,
}

#[cfg_attr(not(test), allow(dead_code))] // the Windows desktop and the tests use it
impl SetupFile {
    pub const NAME: &'static str = "oos-ui.setup";

    pub fn new(dir: &Path) -> SetupFile {
        SetupFile { path: dir.join(Self::NAME), last: String::new(), t: std::time::Instant::now() }
    }

    /// Reads the file, if there is one, into the interface. A missing file is not an error; a bad one is reported and
    /// left as it is.
    pub fn restore(&mut self, ui: &mut crate::ui::Ui) -> Result<bool, String> {
        let result = match std::fs::read_to_string(&self.path) {
            Ok(text) => match parse_setup(&text) {
                Ok(m) => {
                    ui.m = m;
                    ui.sync_full();
                    ui.restore_view_state();
                    Ok(true)
                }
                Err(e) => Err(format!("{}: {}", self.path.display(), e)),
            },
            Err(_) => Ok(false),
        };
        self.last = setup_text(&ui.m);
        result
    }

    /// Writes the setup when it changed, at most every two seconds (`force`: now). The file is replaced whole.
    pub fn save(&mut self, ui: &crate::ui::Ui, force: bool) {
        if !force && self.t.elapsed().as_secs() < 2 {
            return;
        }
        self.t = std::time::Instant::now();
        let text = setup_text(&ui.m);
        if text == self.last {
            return;
        }
        if let Some(dir) = self.path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = self.path.with_extension("setup.new");
        if std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, &self.path)).is_ok() {
            self.last = text;
        }
    }
}
