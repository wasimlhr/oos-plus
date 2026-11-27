// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Build script of oos-ui: the board family this build is for, `cfg(oos_board = "<name>")` from `OOS_BOARD`
//! (ADR 0067, hal/board_cfg.rs). oos-plus: also the commit the binary was built from, for System > About (`OOS_GIT`):
//! "which build is this" has one answer on the scope and on a desktop. Without git, or outside a checkout, it is left
//! out and About shows the version alone, as it is with `OOS_GIT_OFF` set: tools/compare-base.sh sets it, so the
//! pictures of two commits are not different by the commit About names.

use std::process::Command;

include!("../hal/board_cfg.rs");

fn main() {
    board_cfg(std::path::Path::new(env!("CARGO_MANIFEST_DIR")), "../hal/board_cfg.rs");
    println!("cargo:rerun-if-changed=../.git/HEAD");
    println!("cargo:rerun-if-changed=../.git/logs/HEAD");
    println!("cargo:rerun-if-env-changed=OOS_GIT_OFF");
    if std::env::var_os("OOS_GIT_OFF").is_some() {
        return;
    }
    let git = |args: &[&str]| Command::new("git").args(args).output().ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
    if let Some(hash) = git(&["rev-parse", "--short=7", "HEAD"]).filter(|h| !h.is_empty()) {
        let date = git(&["log", "-1", "--format=%cd", "--date=format:%Y-%m-%d %H:%M"]).unwrap_or_default();
        println!("cargo:rustc-env=OOS_GIT={hash} {date}");
    }
}
