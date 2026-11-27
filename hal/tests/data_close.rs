// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The card's clean state before the power goes or another kernel starts (`oos_hal::fsutil::DATA_CLOSE`): the helper
//! unmounts the USB stick before it makes /oos read only, and every way out of the running system uses it.
//! Break it catches: a power-off, restart, kexec or shutdown path that only syncs, or a helper that makes /oos read
//! only while the stick still waits for its writes.

use std::path::{Path, PathBuf};

fn top() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().to_path_buf()
}

fn read(rel: &str) -> String {
    std::fs::read_to_string(top().join(rel)).unwrap_or_else(|e| panic!("{}: {}", rel, e))
}

#[test]
fn the_stick_goes_before_the_data_partition_is_made_read_only() {
    let rel = format!("rootfs/initramfs{}", oos_hal::fsutil::DATA_CLOSE);
    let s = read(&rel);
    let at = |what: &str| s.find(what).unwrap_or_else(|| panic!("{}: no {:?}", rel, what));
    assert!(at("umount $USB") < at("remount,ro $DATA"), "{}: the stick is unmounted first", rel);
    assert!(at("sync") < at("remount,ro $DATA"), "{}: synced before read only", rel);
    for step in ["timeout 1 umount", "timeout 3 sync", "timeout 3 mount -o remount,ro", "timeout 3 mount -o remount,rw"]
    {
        assert!(s.contains(step), "{}: every step has a time limit ({:?})", rel, step);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(top().join(&rel)).unwrap().permissions().mode();
        assert!(mode & 0o111 != 0, "{}: executable", rel);
    }
}

#[test]
fn every_way_out_of_the_system_closes_the_data_partition() {
    let shutdown = read("rootfs/initramfs/etc/inittab");
    let line = format!("::shutdown:{} close", oos_hal::fsutil::DATA_CLOSE);
    assert!(shutdown.lines().any(|l| l == line), "inittab: {:?}", line);
    for (rel, before) in [("ui/src/main.rs", "PanelCmd::PowerOff"), ("fpgad/src/bin/oos-update.rs", "kexec::start()")] {
        let s = read(rel);
        let close = s.find("data_close(\"close\")").unwrap_or_else(|| panic!("{}: no data_close(\"close\")", rel));
        let after = s[close..].find(before).unwrap_or_else(|| panic!("{}: {:?} after the close", rel, before));
        assert!(after > 0, "{}", rel);
        assert!(s.contains("data_close(\"reopen\")"), "{}: the way back when the power or the kexec did not come", rel);
    }
}
