// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What a view of Display > Views (ui/views.rs) keeps of a setup: its keys, and the check of a view as a setup
//! keeps it (`view1` ...).

/// Keys of a setup that belong to a view, besides the keys of the Ui (`UI_KEYS`).
pub const KEYS: [&str; 44] = [
    "windows",
    "density",
    "split_pane",
    "ch_window",
    "math_window",
    "la_window",
    "ref_window",
    "meas",
    "meas2",
    "cat",
    "row_cfg",
    "meas_slots",
    "meas_stat",
    "meas_full_record",
    "dvm",
    "counter",
    "cursor",
    "cur_a",
    "cur_b",
    "cur_c",
    "cur_d",
    "cur_snap",
    "cur_b_source",
    "pane",
    "meas_labels",
    "trend_sel",
    "strip",
    "studio_ov",
    "studio_meas_view",
    "studio_badges",
    "zoom",
    "zoom_tdiv",
    "zdelay",
    "hist",
    "view",
    "spec",
    "spec_peaks",
    "spec_acpr",
    "eye",
    "eye_place",
    "eye_more",
    "eye_pll",
    "math_hidden",
    "gen_hidden",
];

/// Keys of a view that are fields of the Ui, not of the model: the XY and full-screen state, the Studio view, its
/// docking and state, the lanes and the math definitions shown (oos-plus).
pub const UI_KEYS: [&str; 7] = ["xy", "full", "studio_view", "studio_dock", "studio_state", "lanes", "math_def"];

/// Whether `k` is a key of a view's model part (not one of `UI_KEYS`).
pub fn is_view_key(k: &str) -> bool {
    KEYS.contains(&k)
}

fn known(k: &str) -> bool {
    KEYS.contains(&k) || UI_KEYS.contains(&k)
}

/// The pairs of a view as kept, `key=value; key=value ...`, key and value trimmed; `Err(part)` for a part without `=`.
pub fn pairs(v: &str) -> impl Iterator<Item = Result<(&str, &str), &str>> {
    let parts = v.split(';').map(str::trim).filter(|p| !p.is_empty());
    parts.map(|p| p.split_once('=').map(|(k, v)| (k.trim(), v.trim())).ok_or(p))
}

/// A view as kept in a setup: `key=value; key=value ...` of its keys only.
pub fn slot_checked(v: &str) -> Result<String, String> {
    for p in pairs(v) {
        let (k, _) = p.map_err(|part| format!("not key=value: {}", part))?;
        if !known(k) {
            return Err(format!("not part of a view: {}", k));
        }
    }
    Ok(v.trim().to_string())
}
