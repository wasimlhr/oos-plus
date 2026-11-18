// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Storage: setup files (`key = value` text of the Model), waveform CSV, PNG screenshots (own encoder: one fixed-Huffman
//! deflate block with matches against the pixel to the left and above, CRC32, Adler32), file names in the storage directory.

use crate::gfx::Canvas;
use crate::math::{MATH_OPS, WINDOWS};
use crate::model::*;
use crate::wave::Record;
use oos_dsp::decode::Parity;
use std::fmt::Debug;
use std::path::{Path, PathBuf};

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

/// A stick is foreign data and this program runs as root: a link or a device node on it, under a name a save would use,
/// could lead straight to the card. So only a regular file or a new one, and on the stick no link on the way there.
pub fn check_target(path: &Path) -> std::io::Result<()> {
    let refuse = |what: &str| Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, format!("{}: {}", path.display(), what)));
    // the vendor's data is never written (ADR 0009), whatever path a command was given; `..` is followed as written
    let mut plain = std::path::PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => { plain.pop(); }
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

// ---------------------------------------------------------------------------------------------- setup files

const COUPLINGS: [Coupling; 4] = [Coupling::Dc, Coupling::Ac, Coupling::Gnd, Coupling::Dc50];
const SLOPES: [Slope; 3] = [Slope::Rise, Slope::Fall, Slope::Either];
const TRIG_MODES: [TrigMode; 3] = [TrigMode::Auto, TrigMode::Normal, TrigMode::Single];
const ACQ_MODES: [AcqMode; 4] = [AcqMode::Normal, AcqMode::Peak, AcqMode::Average, AcqMode::HiRes];
const WHENS: [WhenKind; 5] = [WhenKind::Off, WhenKind::Gt, WhenKind::Lt, WhenKind::In, WhenKind::Out];
const WIN_MODES: [WinMode; 3] = [WinMode::Enter, WinMode::Exit, WinMode::Inside];
const VSYNCS: [VSync; 5] = [VSync::AllLines, VSync::Line, VSync::Odd, VSync::Even, VSync::AllFields];
const HEXPANDS: [HExpand; 4] = [HExpand::Center, HExpand::Left, HExpand::Right, HExpand::Trigger];
const CURSORS: [CursorMode; 5] = [CursorMode::Off, CursorMode::Time, CursorMode::Volt, CursorMode::Track, CursorMode::Xy];
const GRIDS: [Grid; 3] = [Grid::Full, Grid::Light, Grid::None];
const VIDEO_STDS: [VideoStd; 2] = [VideoStd::Pal, VideoStd::Ntsc];
const BITS: [Bit; 5] = [Bit::H, Bit::L, Bit::X, Bit::R, Bit::F];
const POLS: [Pol; 2] = [Pol::Pos, Pol::Neg];
const PARITIES: [Parity; 3] = [Parity::None, Parity::Even, Parity::Odd];

fn name<T: Debug>(x: T) -> String {
    format!("{:?}", x)
}

fn list<T: ToString>(v: &[T]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(" ")
}

/// The setup as `key = value` lines: every Model field except the acquisition state (run / trigger status).
/// Enums by variant name, indices into the value ladders as numbers, floats in Rust's shortest exact form.
/// The keys of one decoder.
fn dec_lines(m: &Model, kv: &mut dyn FnMut(&str, String)) {
    kv("dec_on", m.dec_on.to_string());
    kv("dec_src", m.dec_src.to_string());
    kv("dec_baud", m.dec_baud.to_string());
    kv("dec_canrate", m.dec_canrate.to_string());
    kv("dec_canh", m.dec_canh.to_string());
    kv("dec_fdrate", m.dec_fdrate.to_string());
    kv("dec_arate", m.dec_arate.to_string());
    kv("dec_tick", m.dec_tick.to_string());
    kv("dec_nibbles", m.dec_nibbles.to_string());
    kv("dec_uspeed", m.dec_uspeed.to_string());
    kv("dec_cphy_rate", m.dec_cphy_rate.to_string());
    kv("dec_src4", m.dec_src4.map_or("none".into(), |c| (c + 1).to_string()));
    kv("dec_diff", m.dec_diff.map_or("none".into(), |c| (c + 1).to_string()));
    kv("trig_bus", m.trig_bus.name().to_string());
    kv("trig_bus_val", m.trig_bus_val.to_string());
    let s = &m.ser;
    kv("ser.rel", name(s.rel));
    kv("ser.max", s.max.to_string());
    kv("ser.mask", s.mask.to_string());
    kv("ser.uart_inv", s.uart_inv.to_string());
    kv("ser.uart_stop", name(s.uart_stop));
    kv("ser.uart_err", name(s.uart_err));
    kv("ser.spi_cs_high", s.spi_cs_high.to_string());
    kv("ser.spi_timeout", s.spi_timeout.to_string());
    kv("ser.addr_rel", name(s.addr_rel));
    kv("ser.addr_max", s.addr_max.to_string());
    kv("ser.dir", name(s.dir));
    kv("ser.ten_bit", s.ten_bit.to_string());
    kv("ser.bytes", s.bytes.to_string());
    kv("dec_thr", m.dec_thr.to_string());
    kv("dec_ascii", m.dec_ascii.to_string());
    kv("dec_proto", m.dec_proto.key());
    kv("dec_opt", list(&m.dec_opt));
    kv("dec_src2", m.dec_src2.to_string());
    kv("dec_cs", m.dec_cs.map_or("none".into(), |c| c.to_string()));
    kv("dec_mode", m.dec_mode.to_string());
    kv("dec_bits", m.dec_bits.to_string());
    kv("dec_parity", name(m.dec_parity));
}

/// The one bus on the plot.
fn bus_lines(m: &Model, kv: &mut dyn FnMut(&str, String)) {
    let b = &m.bus;
    kv("bus_on", b.on.to_string());
    kv("bus_label", b.label.clone());
    kv("bus_pos", b.pos.to_string());
    kv("bus_kind", name(b.kind));
    kv("bus_clk", b.clk.token());
    kv("bus_data", b.data.token());
    kv("bus_clk_thr", b.clk_thr.to_string());
    kv("bus_data_thr", b.data_thr.to_string());
    kv("bus_rw", b.addr_rw.to_string());
}

pub fn setup_text(m: &Model) -> String {
    let mut s = String::from("# OOS setup v1: key = value (oos-ui)\n");
    let mut kv = |k: &str, v: String| {
        s.push_str(k);
        s.push_str(" = ");
        s.push_str(&v);
        s.push('\n');
    };
    for (i, &seconds) in m.deskew_ch.iter().enumerate() {
        if seconds != 0.0 { kv(&format!("deskew_ch{}", i + 1), seconds.to_string()); }
    }
    for (i, &seconds) in m.deskew_ref.iter().enumerate() {
        if seconds != 0.0 { kv(&format!("deskew_ref{}", i + 1), seconds.to_string()); }
    }
    for (c, ch) in m.ch.iter().enumerate() {
        let p = |k: &str| format!("ch{}.{}", c + 1, k);
        kv(&p("on"), ch.on.to_string());
        kv(&p("vdiv"), VSCALES[ch.vscale].to_string());
        if ch.vfine != 1.0 {
            kv(&p("vfine"), ch.vfine.to_string());
        }
        if ch.unit != AmpUnit::V {
            kv(&p("unit"), ch.unit.name().to_string());
        }
        if ch.bias != 0.0 {
            kv(&p("bias"), ch.bias.to_string());
        }
        if ch.skew != 0.0 {
            kv(&p("skew"), ch.skew.to_string());
        }
        kv(&p("offset"), ch.offset.to_string());
        kv(&p("coupling"), name(ch.coupling));
        kv(&p("bw"), BWS[ch.bw].0.to_string());
        kv(&p("probe"), ch.probe.to_string());
        kv(&p("invert"), ch.invert.to_string());
    }
    kv("active", m.active.to_string());
    if let Some(source) = m.cur_b_source {
        kv("cur_b_source", source.token());
    }
    kv("tdiv", tscales()[m.tscale].to_string());
    kv("delay", m.delay.to_string());
    kv("trig_src", m.trig_src.to_string());
    kv("trig_ref", m.trig_ref.map_or("none".into(), |i| i.to_string()));
    kv("trig_la", m.trig_la.map_or("none".to_string(), |n| n.to_string()));
    kv("pat_la", m.pat_la.iter().map(|&b| match b { Bit::H => 'H', Bit::L => 'L', _ => 'X' }).collect());
    kv("trig_level", m.trig_level.to_string());
    kv("slope", name(m.slope));
    kv("trig_mode", name(m.trig_mode));
    kv("holdoff", m.holdoff.to_string());
    kv("trig_type", name(m.trig_type));
    kv("trig_level2", m.trig_level2.to_string());
    kv("trig_nrej", m.trig_nrej.to_string());
    kv("auto_nrej", m.auto_nrej.to_string());
    kv("trig_coup", name(m.trig_coup));
    kv("trig_src2", m.trig_src2.to_string());
    kv("trig_ref2", m.trig_ref2.map_or("none".into(), |i| i.to_string()));
    kv("trig_slope2", name(m.trig_slope2));
    kv("trig_pol", name(m.trig_pol));
    kv("trig_when", name(m.trig_when));
    kv("trig_t1", m.trig_t1.to_string());
    kv("trig_t2", m.trig_t2.to_string());
    kv("video_std", name(m.video_std));
    kv("video_sync", name(m.video_sync));
    kv("video_line", m.video_line.to_string());
    kv("pat", m.pat.iter().map(|&b| name(b)).collect::<Vec<_>>().join(" "));
    kv("pat_levels", list(&m.pat_levels));
    kv("win_mode", name(m.win_mode));
    kv("nth_idle", m.nth_idle.to_string());
    kv("nth_n", m.nth_n.to_string());
    kv("setup", m.setup.to_string());
    kv("hold", m.hold.to_string());
    kv("acq", name(m.acq));
    kv("cores", m.cores.to_string());
    kv("avg_count", m.avg_count.to_string());
    kv("avg_hw", m.avg_hw.to_string());
    kv("segments", m.segments.to_string());
    kv("seg_hw", m.seg_hw.to_string());
    kv("seg_interval", m.seg_ivl.to_string());
    kv("mdepth", m.mdepth.to_string());
    kv("persist", m.persist.to_string());
    kv("intensity", m.intensity.to_string());
    kv("grid", name(m.grid));
    kv("grid_bright", m.grid_bright.to_string());
    kv("grid_frame", m.grid_frame.to_string());
    kv("skin", name(m.skin));
    kv("windows", name(m.win_layout));
    kv("density", name(m.density));
    kv("split_pane", m.split_pane.to_string());
    kv("ch_window", m.ch_win.iter().map(|w| (w + 1).to_string()).collect::<Vec<_>>().join(" "));
    kv("math_window", m.math_win.iter().map(|w| (w + 1).to_string()).collect::<Vec<_>>().join(" "));
    kv("la_window", (m.la_win + 1).to_string());
    kv("ref_window", m.ref_win.iter().map(|w| (w + 1).to_string()).collect::<Vec<_>>().join(" "));
    kv("meas", m.meas.iter().map(|&(k, id)| format!("{:?}:{}", k, id.token())).collect::<Vec<_>>().join(" "));
    kv("meas2", m.meas2.iter().map(|&(k, a, b)| format!("{}:{}:{}", k, a.token(), b.token())).collect::<Vec<_>>().join(" "));
    if !m.cat.is_empty() {
        kv("cat", m.cat.iter().map(|&(k, a, b, c, d)| format!("{}:{}:{}:{}:{}", k, a.token(), b.token(), c.token(), d.token())).collect::<Vec<_>>().join(" "));
    }
    {
        // per-row settings of the rows that are still selected
        let keys = m.row_keys();
        let rows: Vec<String> = m.row_cfg.iter().filter(|(k, c)| keys.contains(k) && !c.is_default()).map(|(k, c)| format!("{}|{}", k.text(), c.text())).collect();
        if !rows.is_empty() { kv("row_cfg", rows.join(" ")); }
    }
    kv("dvm", format!("{} {} {}", m.dvm_on, m.dvm_src, m.dvm_mode));
    kv("counter", format!("{} {} {} {}", m.cnt_on, m.cnt_src, m.cnt_mode, m.cnt_digits));
    for c in 0..NCH {
        if !m.ch_label[c].is_empty() {
            kv(&format!("ch{}.label", c + 1), m.ch_label[c].clone());
        }
    }
    kv("cursor", name(m.cursor));
    kv("cur_a", m.cur_a.to_string());
    kv("cur_b", m.cur_b.to_string());
    kv("cur_c", m.cur_c.to_string());
    kv("cur_d", m.cur_d.to_string());
    kv("touch_lock", m.touch_lock.to_string());
    kv("aux_out", name(m.aux_out));
    kv("fine_v", m.fine_v.to_string());
    kv("fine_t", m.fine_t.to_string());
    if m.tfine != 1.0 {
        kv("tfine", m.tfine.to_string());
    }
    kv("v_expand_gnd", m.v_expand_gnd.to_string());
    kv("h_expand", format!("{:?}", m.h_expand));
    kv("math", name(m.math));
    kv("math_a", m.math_a.to_string());
    kv("math_b", m.math_b.to_string());
    kv("math_scale", m.math_scale.to_string());
    kv("math_fc", m.math_fc.to_string());
    kv("math_lin", format!("{} {}", m.math_lin.0, m.math_lin.1));
    for (k, e) in m.math_expr.iter().enumerate().filter(|(_, e)| !e.is_empty()) {
        kv(&format!("math_expr{}", k + 1), e.clone());
    }
    kv("log_ivl", m.log_ivl.to_string());
    kv("spec_margin", m.spec_margin.to_string());
    kv("math_offset", m.math_offset.to_string());
    kv("fft_win", name(m.fft_win));
    kv("math_bw", m.math_bw.to_string());
    // the "math*" keys are the selected math channel; the others as mN.* (N = 1..8)
    kv("math_sel", m.math_sel.to_string());
    for (k, c) in m.maths.iter().enumerate().filter(|(k, _)| *k != m.math_sel) {
        let p = |f: &str| format!("m{}.{}", k + 1, f);
        kv(&p("op"), name(c.op));
        kv(&p("a"), c.a.to_string());
        kv(&p("b"), c.b.to_string());
        kv(&p("scale"), c.scale.to_string());
        kv(&p("fc"), c.fc.to_string());
        kv(&p("bw"), c.bw.to_string());
        kv(&p("offset"), c.offset.to_string());
        kv(&p("win"), name(c.win));
        kv(&p("lin"), format!("{} {}", c.lin.0, c.lin.1));
    }
    kv("dec_sel", m.dec_sel.to_string());
    dec_lines(m, &mut kv);
    bus_lines(m, &mut kv);
    for k in (0..DECODERS).filter(|&k| k != m.dec_sel && m.decs[k].is_some()) {
        dec_lines(&m.dec_view(k), &mut |n: &str, v: String| kv(&format!("d{}.{}", k + 1, n), v));
    }
    kv("pane", m.pane.to_string());
    kv("meas_labels", m.meas_labels.to_string());
    kv("strip", name(m.strip_dock));
    kv("trend_sel", format!("{} {}", m.trend_sel[0], m.trend_sel[1]));
    kv("zoom", m.zoom.to_string());
    kv("auto_roll", m.auto_roll.to_string());
    kv("zoom_tdiv", tscales()[m.zscale].to_string());
    kv("zdelay", m.zdelay.to_string());
    kv("color_grade", m.color_grade.to_string());
    kv("dots", m.dots.to_string());
    kv("layout", m.layout.name().to_ascii_lowercase());
    kv("studio_view", m.studio_view.clone());
    if !m.studio_dock.is_empty() { kv("studio_dock", m.studio_dock.clone()); }
    if m.studio_ov != 0 { kv("studio_ov", m.studio_ov.to_string()); }
    if !m.studio_detail { kv("studio_meas_view", "list".to_string()); }
    if m.studio_overlay { kv("studio_traces", "overlay".to_string()); }
    if !m.studio_zoom_tab { kv("studio_zoom_controls", "titlebar".to_string()); }
    if m.studio_classic { kv("studio_badges", "classic".to_string()); }
    kv("bar", if m.bar_compact { "compact" } else { "cards" }.to_string());
    kv("draw_path", if m.draw_batch { "batch" } else { "standard" }.to_string());
    kv("hdmi", name(m.hdmi));
    kv("hdmi_view", m.hdmi_view.to_string());
    kv("view", name(m.view));
    // Display > Views: the count and the one shown, then what each of the others keeps (ui/views.rs)
    kv("views", format!("{} {}", m.views, m.view_cur + 1));
    kv("studio_state", m.studio_state.clone());
    if m.math_hidden.iter().any(|&h| h) { kv("math_hidden", m.math_hidden.iter().enumerate().filter(|(_, &h)| h).map(|(k, _)| (k + 1).to_string()).collect::<Vec<_>>().join(" ")); }
    if m.gen_hidden { kv("gen_hidden", "true".to_string()); }
    // The generator's wave and modulation. Not whether its output is on: a recalled set-up or session never switches
    // an output on (it is off after a recall). An arbitrary wave has its samples elsewhere and is not written.
    if m.afg_wave < AFG_WAVES.len() - 1 {
        kv("gen", format!("{} {} {} {} {} {}", AFG_WAVES[m.afg_wave], m.afg_freq, m.afg_ampl, m.afg_offset, m.afg_duty, m.afg_symmetry));
        kv("gen_mod", format!("{} {} {} {} {} {}", AFG_MODS[m.afg_mod.min(AFG_MODS.len() - 1)], m.afg_mod_rate, m.afg_mod_depth, m.afg_fm_dev, m.afg_pm_dev,
            AFG_MOD_SHAPES[m.afg_mod_shape.min(AFG_MOD_SHAPES.len() - 1)].replace(' ', "_")));
    }
    for (i, v) in m.view_slots.iter().enumerate().filter(|(_, v)| !v.is_empty()) {
        kv(&format!("view{}", i + 1), v.clone());
    }
    // the generator index of an external generator is the model of the Ext generator page
    let ext = if crate::extgen::external(m.ext_model) { m.ext_model } else { crate::extgen::FIRST_EXTERNAL };
    let bode_gen = if crate::extgen::external(m.bode_gen) { ext } else { m.bode_gen };
    kv("bode", format!("{} {} {} {} {} {} {} {}", m.bode_start, m.bode_stop, m.bode_points, m.bode_amp, m.bode_in.token(), m.bode_out.token(), bode_gen, m.bode_addr));
    // the generator's connection: lan (the address above) or usb; a line of its own, so older versions skip it
    kv("bode_link", if m.bode_usb { "usb" } else { "lan" }.to_string());
    // the external generator: model index, channel (1-based), load (hiz or 50), frequency Hz and amplitude Vpp of the page's output
    kv("extgen", format!("{} {} {} {} {}", ext, m.ext_ch.min(1) + 1, if m.ext_hiz { "hiz" } else { "50" }, m.ext_freq, m.ext_ampl));
    // free sweep: time, log, sync channel, edge, level, input (0 = none), reference dBV
    kv("bode_sweep", format!("{} {} {} {} {} {} {}", m.bode_sw_time, m.bode_sw_log, m.bode_sync + 1, name(m.bode_sync_slope), m.bode_sync_level, m.bode_sw_in, m.bode_ref));
    kv("spec", format!("{} {} {} {} {} {} {} {} {}", m.spec_src.token(), m.spec_center, m.spec_span, m.spec_rbw, name(m.spec_win), m.spec_trace, m.spec_ref, m.spec_wf, m.spec_dist));
    kv("spec_peaks", format!("{} {}", m.spec_sort, m.spec_harm));
    kv("eye", format!("{} {} {}", m.eye_src.token(), m.eye_rate, if m.eye_from_record { 1 } else { 0 }));
    kv("eye_place", name(m.eye_place));
    kv("power", format!("{} {} {}", m.pw_v.token(), m.pw_i.token(), name(m.pw_mode)));
    let opt = |id: Option<WaveId>| id.map(|w| w.token()).unwrap_or_else(|| "-".into());
    kv("meas_cfg", format!("{} {:?} {} {} {} {} {} {}", m.meas_idle, m.meas_out, m.meas_n, if m.meas_data { 1 } else { 0 }, m.meas_lpct, m.meas_hpct, m.meas_tsetup, m.meas_thold));
    if m.meas_pop_current || m.meas_show != [true; 3] {
        kv("meas_stat", format!("{} {} {} {}", m.meas_pop_current as u8, m.meas_show[0] as u8, m.meas_show[1] as u8, m.meas_show[2] as u8));
    }
    // the other shared reference levels: volts mode, custom, mid, hysteresis, volts, volts hysteresis, separate fall, fall, fall volts, top/base
    let l = &m.meas_lv;
    kv("meas_levels", format!("{} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {:?}", l.absolute as u8, l.custom as u8, l.mid, l.hyst, l.volts[0], l.volts[1], l.volts[2], l.vhyst,
        l.sep_fall as u8, l.fall[0], l.fall[1], l.fall[2], l.fall_volts[0], l.fall_volts[1], l.fall_volts[2], l.base_top));
    kv("meas_full_record", m.meas_full_record.to_string());
    kv("spec_acpr", format!("{} {}", m.spec_acpr_w, m.spec_acpr_s));
    kv("bode_mode", name(m.bode_mode));
    kv("eye_more", format!("{} {} {} {} {} {} {} {}", m.eye_ber, m.eye_pn_lo, m.eye_pn_hi, if m.eye_npj { 1 } else { 0 }, opt(m.eye_rail), m.eye_notch, opt(m.eye_b), if m.eye_hp { 1 } else { 0 }));
    if m.eye_pll_bw > 0.0 { kv("eye_pll", m.eye_pll_bw.to_string()); }
    kv("power_more", format!("{} {} {} {} {}", m.pw_fline, m.pw_fsw, opt(m.pw_vin), m.pw_ref.map(|v| v.to_string()).unwrap_or_else(|| "-".into()), opt(m.pw_clk)));
    kv("magnetic", format!("{} {} {} {} {} {} {:?} {}", m.mag_v.token(), m.mag_i.token(), m.mag_n, m.mag_ae, m.mag_le, m.mag_rcu, m.mag_plot, m.mag_cur));
    let opt = |c: Option<WaveId>| c.map_or("off".into(), |c| c.token());
    let line = |c: Option<u8>| c.map_or("off".into(), |c| c.to_string());
    kv("inverter", format!("{:?} {} {} {} {:?} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {} {}", m.inv_page, m.inv_a.token(), m.inv_b.token(), m.inv_c.token(), m.inv_ang, m.inv_poles, m.inv_an.token(), m.inv_volt, opt(m.inv_tq), opt(m.inv_ev), opt(m.inv_ei), m.inv_scale, m.inv_degv, m.inv_off, m.inv_elec, m.inv_qa, m.inv_qb, line(m.inv_qi), m.inv_cpr, m.inv_swap, m.inv_ha, m.inv_hb, m.inv_hc));
    kv("dpt", format!("{} {} {} {:?} {} {} {:?}", m.dpt_v.token(), m.dpt_i.token(), m.dpt_g.token(), m.dpt_4th, m.dpt_4ch.token(), m.dpt_pct, m.dpt_zoom));
    kv("sim", format!("{} {} {} {} {} {} {} {} {} {} {} {} {} {} {}", m.sim_preset, m.sim_slot, m.sim_rate, m.sim_prbs, m.sim_vhigh, m.sim_vlow, m.sim_rise, m.sim_noise, m.sim_bw, m.sim_deemph, if m.sim_ssc { 1 } else { 0 }, m.sim_rj, m.sim_pj, m.sim_pjhz, 0));
    kv("sim_extra", format!("{} {} {} {} {} {}", m.sim_ssc_center, m.sim_ssc_deviation, m.sim_ssc_hz, m.sim_dcd_pp, m.sim_corr_rj, m.sim_corr_hz));
    // zones: "on src inside t:v t:v ..."
    for (k, z) in m.zones.iter().enumerate() {
        let pts: Vec<String> = z.pts.iter().map(|(t, v)| format!("{}:{}", t, v)).collect();
        kv(&format!("zone{}", k + 1), format!("{} {} {} {}", z.on, z.src + 1, z.inside, pts.join(" ")).trim().to_string());
    }
    kv("z_src", m.z_src.map_or("off".to_string(), |c| (c + 1).to_string()));
    kv("hist", name(m.hist));
    kv("mask_on", m.mask_on.to_string());
    kv("mask_src", m.mask_src.to_string());
    kv("mask_x", m.mask_x.to_string());
    kv("mask_y", m.mask_y.to_string());
    kv("mask_stop", m.mask_stop.to_string());
    kv("mask_inv", m.mask_inv.to_string());
    kv("quick", name(m.quick));
    kv("text_size", name(m.text_size));
    kv("la", m.la.to_text());
    kv("dec_la", format!("{} {}", m.dec_la_in as u8, m.dec_la.map(|n| n.map_or("-".to_string(), |x| x.to_string())).join(" ")));
    s
}

fn p_bool(v: &str) -> Result<bool, String> {
    match v {
        "true" | "on" | "1" => Ok(true),
        "false" | "off" | "0" => Ok(false),
        _ => Err(format!("not a boolean: {}", v)),
    }
}

fn p_f32(v: &str) -> Result<f32, String> {
    v.parse::<f32>().ok().filter(|x| x.is_finite()).ok_or_else(|| format!("not a number: {}", v))
}

fn p_u32(v: &str) -> Result<u32, String> {
    v.parse::<u32>().map_err(|_| format!("not an integer: {}", v))
}

/// Old setup files stored an index into the 5 ns .. 1000 s ladder.
fn legacy_tscale(v: &str, ntime: usize) -> Result<usize, String> {
    let five = tscales().iter().position(|&t| t >= 4.9e-9).unwrap_or(0);
    Ok(five.saturating_add(p_idx(v, ntime.saturating_sub(five).max(1))?))
}

/// Index below n.
/// A time/div value as an index into `tscales()` (exact step, 0.1 % tolerance).
fn tdiv_index(v: &str) -> Result<usize, String> {
    let x = p_f32(v)?;
    tscales().iter().position(|&s| (s - x).abs() <= s * 1e-3).ok_or_else(|| format!("no time/div step {}", v))
}

fn p_idx(v: &str, n: usize) -> Result<usize, String> {
    let i = v.parse::<usize>().map_err(|_| format!("not an index: {}", v))?;
    if i < n {
        Ok(i)
    } else {
        Err(format!("{} out of range (< {})", i, n))
    }
}

fn p_decode_source(v: &str) -> Result<usize, String> {
    let source = if v.trim().starts_with(['R', 'r']) {
        match WaveId::parse(v.trim())? { WaveId::Ref(i) => DEC_REF_BASE + i, _ => unreachable!() }
    } else { p_idx(v, DEC_SOURCE_COUNT)? };
    decode_wave(source).map(|_| source).ok_or_else(|| "decoder source must be CH or R; LA uses dec_la".into())
}

fn p_decode_optional_one_based(v: &str) -> Result<Option<usize>, String> {
    if v.trim() == "none" { return Ok(None); }
    if v.trim().starts_with(['R', 'r']) { return p_decode_source(v).map(Some); }
    let source = p_idx(v, DEC_SOURCE_COUNT + 1)?.max(1) - 1;
    decode_wave(source).map(|_| Some(source)).ok_or_else(|| "invalid decoder source".into())
}

fn p_enum<T: Debug + Copy>(v: &str, all: &[T]) -> Result<T, String> {
    all.iter().copied().find(|x| format!("{:?}", x) == v).ok_or_else(|| format!("unknown value: {}", v))
}

/// A measurement kind by its name in a setup file.
pub fn parse_meas(v: &str) -> Result<Meas, String> {
    p_enum(v, &ALL_MEAS)
}

fn p_list<T>(v: &str, n: usize, f: impl Fn(&str) -> Result<T, String>) -> Result<Vec<T>, String> {
    let r: Vec<T> = v.split_whitespace().map(f).collect::<Result<_, _>>()?;
    if r.len() == n {
        Ok(r)
    } else {
        Err(format!("{} values, want {}", r.len(), n))
    }
}

pub fn set(m: &mut Model, k: &str, v: &str) -> Result<(), String> {
    if let Some((reference, suffix)) = k.strip_prefix("deskew_ch").map(|s| (false, s))
        .or_else(|| k.strip_prefix("deskew_ref").map(|s| (true, s))) {
        let values: &mut [f64] = if reference { &mut m.deskew_ref } else { &mut m.deskew_ch };
        let index = suffix.parse::<usize>().ok().and_then(|i| i.checked_sub(1)).filter(|&i| i < values.len()).ok_or("invalid deskew source")?;
        let value = v.parse::<f64>().ok().filter(|x| x.is_finite() && x.abs() <= 1000.0).ok_or("invalid deskew seconds")?;
        values[index] = value;
        return Ok(());
    }
    // the other decoders: dec_sel comes first in the file
    if let Some((n, field)) = k.strip_prefix('d').and_then(|r| r.split_once('.')) {
        if let Ok(n) = n.parse::<usize>() {
            let n = n.checked_sub(1).filter(|&n| n < DECODERS).ok_or("unknown decoder")?;
            let mut t = m.dec_view(n);
            set(&mut t, field, v)?;
            m.dec_store(n, t.dec_cfg());
            return Ok(());
        }
    }
    if let Some((n, field)) = k.strip_prefix('m').and_then(|r| r.split_once('.')) {
        if let Ok(n) = n.parse::<usize>() {
            let c = m.maths.get_mut(n.wrapping_sub(1)).ok_or("unknown math channel")?;
            match field {
                "op" => c.op = p_enum(v, &MATH_OPS)?,
                "a" => c.a = p_idx(v, crate::math::MATH_SOURCES)?,
                "b" => c.b = p_idx(v, crate::math::MATH_SOURCES)?,
                "scale" => c.scale = p_idx(v, crate::math::math_scales().len())?,
                "fc" => c.fc = p_f32(v)?,
                "bw" => c.bw = p_f32(v)?,
                "offset" => c.offset = p_f32(v)?,
                "win" => c.win = p_enum(v, &WINDOWS)?,
                "lin" => {
                    let (a, b) = v.split_once(' ').ok_or("lin: two numbers")?;
                    c.lin = (p_f32(a)?, p_f32(b)?);
                }
                _ => return Err("unknown key".into()),
            }
            return Ok(());
        }
    }
    if let Some((c, field)) = k.strip_prefix("ch").and_then(|r| r.split_once('.')) {
        let c = c.parse::<usize>().ok().filter(|c| (1..=NCH).contains(c)).ok_or("unknown channel")? - 1;
        let ch = &mut m.ch[c];
        match field {
            "on" => ch.on = p_bool(v)?,
            "vfine" => ch.vfine = p_f32(v)?.clamp(1.0, 2.5),
            "unit" => ch.unit = *AmpUnit::ALL.iter().find(|u| u.name() == v.trim()).ok_or_else(|| format!("no unit {}", v))?,
            "bias" => ch.bias = p_f32(v)?,
            "skew" => ch.skew = p_f32(v)?.clamp(-SKEW_MAX, SKEW_MAX),
            // the keyboard takes ten characters; a hand-edited file gets no more
            "label" => m.ch_label[c] = v.trim().chars().take(10).collect(),
            "vdiv" => {
                let x = p_f32(v)?;
                // 20 V/div was once on the ladder, the hardware ends at 10
                let x = x.min(VSCALES[VSCALES.len() - 1]);
                ch.vscale = VSCALES.iter().position(|&s| (s - x).abs() <= s * 1e-3).ok_or_else(|| format!("no V/div step {}", v))?;
            }
            // setups before the 100/200 uV steps: index into the old ladder (500 uV .. 20 V)
            "vscale" => ch.vscale = (p_idx(v, VSCALES.len() - 1)? + 2).min(VSCALES.len() - 1),
            "offset" => ch.offset = p_f32(v)?,
            "coupling" => ch.coupling = p_enum(v, &COUPLINGS)?,
            "bw" => ch.bw = BWS.iter().position(|b| b.0 == v.trim()).ok_or_else(|| format!("unknown bandwidth {}", v))?,
            // setups before 2026-11-03
            "bw20" => ch.bw = if p_bool(v)? { BW_20M } else { BW_FULL },
            "probe" => ch.probe = p_f32(v)?,
            "invert" => ch.invert = p_bool(v)?,
            _ => {} // unknown keys of newer versions are ignored
        }
        return Ok(());
    }
    let ntime = tscales().len();
    match k {
        "active" => m.active = p_idx(v, NCH)?,
        "cur_b_ch" => m.cur_b_source = Some(WaveId::Ch(p_idx(v, NCH)?)),
        "cur_b_source" => m.cur_b_source = if v == "none" { None } else { Some(WaveId::parse(v)?) },
        "tdiv" => m.tscale = tdiv_index(v)?,
        // setups before the 1/2 ns steps: index into the old ladder (5 ns .. 1000 s)
        "tscale" => m.tscale = legacy_tscale(v, ntime)?,
        "delay" => m.delay = p_f32(v)?,
        "trig_src" => m.trig_src = p_idx(v, NCH)?,
        "trig_ref" => m.trig_ref = if v == "none" { None } else { Some(p_idx(v, NREF)?) },
"pat_la" => {
            if v.trim().len() != 16 {
                return Err(format!("pat_la: {:?}", v));
            }
            for (b, c) in m.pat_la.iter_mut().zip(v.trim().chars()) {
                *b = match c { 'H' => Bit::H, 'L' => Bit::L, _ => Bit::X };
            }
        }
        "trig_la" => m.trig_la = if v.trim() == "none" { None } else { Some(p_idx(v, 16)? as u8) },
        "trig_level" => m.trig_level = p_f32(v)?,
        "slope" => m.slope = p_enum(v, &SLOPES)?,
        "trig_mode" => m.trig_mode = p_enum(v, &TRIG_MODES)?,
        "holdoff" => m.holdoff = p_f32(v)?,
        "trig_type" => m.trig_type = p_enum(v, &TRIG_TYPES)?,
        "trig_nrej" => m.trig_nrej = p_bool(v)?,
        "auto_nrej" => m.auto_nrej = p_bool(v)?,
        "trig_coup" => m.trig_coup = p_enum(v, &TRIG_COUPS)?,
        "trig_level2" => m.trig_level2 = p_f32(v)?,
        "trig_src2" => m.trig_src2 = p_idx(v, NCH)?,
        "trig_ref2" => m.trig_ref2 = if v == "none" { None } else { Some(p_idx(v, NREF)?) },
        "trig_slope2" => m.trig_slope2 = p_enum(v, &SLOPES)?,
        "trig_pol" => m.trig_pol = p_enum(v, &POLS)?,
        "trig_when" => m.trig_when = p_enum(v, &WHENS)?,
        "trig_t1" => m.trig_t1 = p_f32(v)?,
        "trig_t2" => m.trig_t2 = p_f32(v)?,
        "video_std" => m.video_std = p_enum(v, &VIDEO_STDS)?,
        "video_sync" => m.video_sync = p_enum(v, &VSYNCS)?,
        "video_line" => m.video_line = p_u32(v)?,
        "pat" => {
            let b = p_list(v, NCH, |x| p_enum(x, &BITS))?;
            m.pat.copy_from_slice(&b);
        }
        "pat_levels" => {
            let l = p_list(v, NCH, p_f32)?;
            m.pat_levels.copy_from_slice(&l);
        }
        "win_mode" => m.win_mode = p_enum(v, &WIN_MODES)?,
        "nth_idle" => m.nth_idle = p_f32(v)?,
        "nth_n" => m.nth_n = p_u32(v)?,
        "setup" => m.setup = p_f32(v)?,
        "hold" => m.hold = p_f32(v)?,
        "acq" => m.acq = p_enum(v, &ACQ_MODES)?,
        "cores" => m.cores = p_bool(v)?,
        "avg_count" => m.avg_count = p_u32(v)?.max(1),
        "avg_hw" => m.avg_hw = p_bool(v)?,
        "segments" => m.segments = (p_u32(v)? as usize).min(crate::ui::SEG_HW_MAX),
        "seg_hw" => m.seg_hw = p_bool(v)?,
        "seg_interval" => m.seg_ivl = p_f32(v)?.clamp(0.0, crate::ui::SEG_IVL_MAX),
        "mdepth" => m.mdepth = p_idx(v, mdepths().len())?,
        "persist" => m.persist = p_idx(v, PERSIST.len())?,
        "intensity" => m.intensity = p_u32(v)?.clamp(10, 100),
        "grid" => m.grid = p_enum(v, &GRIDS)?,
        "grid_bright" => m.grid_bright = p_u32(v)?.clamp(10, 100),
        "grid_frame" => m.grid_frame = p_bool(v)?,
        "skin" => m.skin = p_enum(v, &crate::ui::skin::SKINS)?,
        "windows" => m.win_layout = p_enum(v, &WinLayout::ALL)?,
        "density" => m.density = p_enum(v, &Density::ALL)?,
        "split_pane" => m.split_pane = p_idx(v, crate::ui::PANES_N)?,
        "ch_window" => {
            let w = p_list(v, NCH, |x| p_idx(x, 5)?.checked_sub(1).ok_or_else(|| "window 0".to_string()))?;
            m.ch_win.copy_from_slice(&w);
        }
        "la_window" => m.la_win = p_idx(v, 5)?.checked_sub(1).ok_or("window 0")?,
        "ref_window" => {
            let w = p_list(v, crate::model::NREF, |x| p_idx(x, 5)?.checked_sub(1).ok_or_else(|| "window 0".to_string()))?;
            m.ref_win.copy_from_slice(&w);
        }
        "math_window" => {
            let w = p_list(v, crate::math::NMATH, |x| p_idx(x, 5)?.checked_sub(1).ok_or_else(|| "window 0".to_string()))?;
            m.math_win.copy_from_slice(&w);
        }
        "meas" => {
            let mut out = Vec::new();
            for e in v.split_whitespace() {
                let (k, c) = e.split_once(':').ok_or_else(|| format!("not item:channel: {}", e))?;
                out.push((p_enum(k, &ALL_MEAS)?, WaveId::parse(c)?));
            }
            if out.len() + m.meas2.len() + m.cat.len() > MEAS_SELECTION_MAX { return Err("too many selected measurements".into()); }
            m.meas = out;
        }
        "meas2" => {
            let mut out = Vec::new();
            for e in v.split_whitespace() {
                let f: Vec<&str> = e.split(':').collect();
                let [k, a, b] = f[..] else { return Err(format!("not item:a:b: {}", e)) };
                out.push((p_idx(k, crate::ui::PAIRS_N)?, WaveId::parse(a)?, WaveId::parse(b)?));
            }
            if m.meas.len() + out.len() + m.cat.len() > MEAS_SELECTION_MAX { return Err("too many selected measurements".into()); }
            m.meas2 = out;
        }
        "cat" => {
            let mut out = Vec::new();
            for e in v.split_whitespace() {
                let f: Vec<&str> = e.split(':').collect();
                let [k, a, b, c, d] = f[..] else { return Err(format!("not item:a:b:c:d: {}", e)) };
                let id = k.parse::<u16>().map_err(|_| format!("not a catalog id: {}", k))?;
                if !catalog_id_valid(id) { return Err(format!("unknown or non-Wave catalog id: {id}")); }
                if m.meas.len() + m.meas2.len() + out.len() >= MEAS_SELECTION_MAX { return Err("too many selected measurements".into()); }
                let row = (id, WaveId::parse(a)?, WaveId::parse(b)?, WaveId::parse(c)?, WaveId::parse(d)?);
                if out.contains(&row) { return Err("duplicate catalog row".into()); }
                out.push(row);
            }
            m.cat = out;
        }
        "row_cfg" => {
            let mut out = Vec::new();
            for e in v.split_whitespace() {
                let (k, c) = e.split_once('|').ok_or_else(|| format!("not row|settings: {}", e))?;
                out.push((crate::model::RowKey::parse(k)?, crate::model::RowCfg::parse(c)?));
            }
            m.row_cfg = out;
        }
        "dvm" => {
            let f: Vec<&str> = v.split_whitespace().collect();
            let [on, c, mode] = f[..] else { return Err("dvm: on source mode".into()) };
            (m.dvm_on, m.dvm_src, m.dvm_mode) = (p_bool(on)?, p_idx(c, NCH)?, p_idx(mode, 3)?);
        }
        "counter" => {
            let f: Vec<&str> = v.split_whitespace().collect();
            let [on, c, mode, d] = f[..] else { return Err("counter: on source mode digits".into()) };
            (m.cnt_on, m.cnt_src, m.cnt_mode) = (p_bool(on)?, p_idx(c, NCH)?, p_idx(mode, 3)?);
            m.cnt_digits = p_idx(d, 8)?.max(3) as u32;
        }
        "dec_sel" => m.dec_sel = p_idx(v, DECODERS)?,
        "pane" => m.pane = p_idx(v, crate::ui::PANES_N)?,
        "meas_labels" => m.meas_labels = p_bool(v)?,
        "strip" => m.strip_dock = p_enum(v, &StripDock::ALL)?,
        // before the strip setting was shared with Display > Windows the key was `strip_dock` and Below was called Under
        "strip_dock" => m.strip_dock = p_enum(if v == "Under" { "Below" } else { v }, &StripDock::ALL)?,
        "trend_sel" => {
            let f: Vec<&str> = v.split_whitespace().collect();
            let [a, b] = f[..] else { return Err("trend_sel: two values".into()) };
            m.trend_sel = [p_idx(a, crate::ui::TREND_SEL_N)?, p_idx(b, crate::ui::TREND_SEL_N)?];
        }
        "spec_peaks" => {
            let f: Vec<&str> = v.split_whitespace().collect();
            let [sort, harm] = f[..] else { return Err("spec_peaks: order harmonics".into()) };
            (m.spec_sort, m.spec_harm) = (p_idx(sort, 2)?, p_idx(harm, crate::ui::PANE_HARMS.len())?);
        }
        "cursor" => m.cursor = p_enum(v, &CURSORS)?,
        "cur_a" => m.cur_a = p_f32(v)?,
        "cur_b" => m.cur_b = p_f32(v)?,
        "cur_c" => m.cur_c = p_f32(v)?,
        "cur_d" => m.cur_d = p_f32(v)?,
        "touch_lock" => m.touch_lock = p_bool(v)?,
        "aux_out" => m.aux_out = p_enum(v, &AUX_OUTS)?,
        "fine_v" => m.fine_v = p_bool(v)?,
        "fine_t" => m.fine_t = p_bool(v)?,
        "tfine" => m.tfine = p_f32(v)?.clamp(0.1, 1.0),
        "v_expand_gnd" => m.v_expand_gnd = p_bool(v)?,
        "h_expand" => m.h_expand = p_enum(v, &HEXPANDS)?,
        "math" => m.math = p_enum(v, &MATH_OPS)?,
        "math_a" => m.math_a = p_idx(v, crate::math::MATH_SOURCES)?,
        "math_b" => m.math_b = p_idx(v, crate::math::MATH_SOURCES)?,
        "math_bw" => m.math_bw = p_f32(v)?,
        "math_sel" => m.math_sel = p_idx(v, crate::math::NMATH)?,
        "math_scale" => m.math_scale = p_idx(v, crate::math::math_scales().len())?,
        "math_fc" => m.math_fc = p_f32(v)?,
        "log_ivl" => m.log_ivl = p_idx(v, crate::ui::LOG_STEPS_N)?,
        "spec_margin" => m.spec_margin = p_f32(v)?.clamp(0.0, 60.0),
        k if k.starts_with("math_expr") => {
            let n = k[9..].parse::<usize>().map_err(|_| "math_expr: no number")?;
            // a formula that does not parse stays as it is and draws nothing; only the length is checked here
            let e = m.math_expr.get_mut(n.wrapping_sub(1)).ok_or("unknown math channel")?;
            *e = v.trim().chars().take(oos_dsp::expr::EXPR_MAX).collect();
        }
        "math_lin" => {
            let (a, b) = v.split_once(' ').ok_or("math_lin: two numbers")?;
            m.math_lin = (p_f32(a)?, p_f32(b)?);
        }
        "math_offset" => m.math_offset = p_f32(v)?,
        "fft_win" => m.fft_win = p_enum(v, &WINDOWS)?,
        "dec_on" => m.dec_on = p_bool(v)?,
        "dec_src" => m.dec_src = p_decode_source(v)?,
        "dec_baud" => m.dec_baud = p_idx(v, crate::decode::BAUDS.len())?,
        "dec_canrate" => m.dec_canrate = p_idx(v, crate::decode::CAN_RATES.len())?,
        "dec_canh" => m.dec_canh = p_bool(v)?,
        "dec_fdrate" => m.dec_fdrate = p_idx(v, crate::decode::FD_RATES.len())?,
        "dec_arate" => m.dec_arate = p_idx(v, crate::decode::ARINC_RATES.len())?,
        "dec_tick" => m.dec_tick = p_idx(v, crate::decode::SENT_TICKS.len())?,
        "dec_nibbles" => m.dec_nibbles = p_u32(v)?.clamp(1, 6),
        "dec_uspeed" => m.dec_uspeed = p_idx(v, crate::decode::USB_SPEEDS.len())?,
        "dec_cphy_rate" => m.dec_cphy_rate = crate::decode::parse_cphy_rate(v)?,
        "dec_src4" => m.dec_src4 = p_decode_optional_one_based(v)?,
        "dec_diff" => m.dec_diff = p_decode_optional_one_based(v)?,
        "trig_bus" => m.trig_bus = *BUS_WHATS.iter().find(|b| b.name() == v.trim()).ok_or_else(|| format!("unknown bus match {}", v))?,
        "trig_bus_val" => m.trig_bus_val = v.trim().parse().map_err(|e| format!("{}", e))?,
        "ser.rel" => m.ser.rel = p_enum(v, &RELS)?,
        "ser.max" => m.ser.max = v.trim().parse().map_err(|e| format!("ser.max: {}", e))?,
        "ser.mask" => m.ser.mask = v.trim().parse().map_err(|e| format!("ser.mask: {}", e))?,
        "ser.uart_inv" => m.ser.uart_inv = p_bool(v)?,
        "ser.uart_stop" => m.ser.uart_stop = p_enum(v, &STOP_BITS)?,
        "ser.uart_err" => m.ser.uart_err = p_enum(v, &UART_ERRS)?,
        "ser.spi_cs_high" => m.ser.spi_cs_high = p_bool(v)?,
        "ser.spi_timeout" => m.ser.spi_timeout = p_f32(v)?.max(0.0),
        "ser.addr_rel" => m.ser.addr_rel = p_enum(v, &RELS)?,
        "ser.addr_max" => m.ser.addr_max = p_u32(v)?.min(0x3ff),
        "ser.dir" => m.ser.dir = p_enum(v, &I2C_DIRS)?,
        "ser.ten_bit" => m.ser.ten_bit = p_bool(v)?,
        "ser.bytes" => m.ser.bytes = p_u32(v)?.min(5) as u8,
        "dec_thr" => m.dec_thr = p_f32(v)?,
        "dec_ascii" => m.dec_ascii = p_bool(v)?,
        "dec_proto" => m.dec_proto = Proto::from_key(v.trim()).ok_or_else(|| format!("unknown protocol: {}", v.trim()))?,
        // a module's option indices; brought into range where they are used (`protocols::opt_fit`)
        "dec_opt" => m.dec_opt = p_list(v, crate::protocols::OPTS, |x| x.parse::<u8>().map_err(|e| e.to_string()))?.try_into().unwrap(),
        "dec_src2" => m.dec_src2 = p_decode_source(v)?,
        "dec_cs" => m.dec_cs = if v == "none" { None } else { Some(p_decode_source(v)?) },
        "dec_mode" => m.dec_mode = p_idx(v, 4)? as u8,
        "dec_bits" => m.dec_bits = p_u32(v)?.clamp(4, 32),
        "dec_parity" => m.dec_parity = p_enum(v, &PARITIES)?,
        "zoom" => m.zoom = p_bool(v)?,
        "auto_roll" => m.auto_roll = p_bool(v)?,
        "zoom_tdiv" => m.zscale = tdiv_index(v)?,
        "zscale" => m.zscale = legacy_tscale(v, ntime)?,
        "zdelay" => m.zdelay = p_f32(v)?,
        "color_grade" => m.color_grade = p_bool(v)?,
        "dots" => m.dots = p_bool(v)?,
        "layout" => m.layout = Layout::parse(v).ok_or_else(|| format!("bad layout {v}"))?,
        "studio_view" => m.studio_view = if ["waveform", "docked", "floating", "only"].contains(&v) { v.to_string() } else { return Err(format!("bad studio view {v}")) },
        "studio_dock" => m.studio_dock = v.to_string(),
        "studio_zoom_controls" => m.studio_zoom_tab = match v { "tab" => true, "titlebar" => false, _ => return Err(format!("bad studio_zoom_controls {v}")) },
        "studio_state" => m.studio_state = v.to_string(),
        "math_hidden" => {
            let mut h = [false; crate::math::NMATH];
            for e in v.split_whitespace() {
                h[p_idx(e, crate::math::NMATH + 1)?.checked_sub(1).ok_or_else(|| "math 0".to_string())?] = true;
            }
            m.math_hidden = h;
        }
        "gen_hidden" => m.gen_hidden = p_bool(v)?,
        "gen" | "gen_mod" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 6 {
                return Err(format!("{k}: want 6 values"));
            }
            let f = |s: &str| s.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(|| format!("{k}: bad number {s}"));
            let named = |all: &[&str], s: &str| all.iter().position(|n| n.replace(' ', "_").eq_ignore_ascii_case(s)).ok_or_else(|| format!("{k}: unknown {s}"));
            if k == "gen" {
                // every wave but the arbitrary one, whose samples a set-up does not hold
                let wave = named(&AFG_WAVES[..AFG_WAVES.len() - 1], w[0])?;
                (m.afg_wave, m.afg_freq, m.afg_ampl, m.afg_offset, m.afg_duty, m.afg_symmetry) = (wave, f(w[1])?, f(w[2])?, f(w[3])?, f(w[4])?.clamp(0.0, 100.0), f(w[5])?.clamp(0.0, 100.0));
            } else {
                let (kind, shape) = (named(&AFG_MODS, w[0])?, named(&AFG_MOD_SHAPES, w[5])?);
                (m.afg_mod, m.afg_mod_rate, m.afg_mod_depth, m.afg_fm_dev, m.afg_pm_dev, m.afg_mod_shape) = (kind, f(w[1])?, f(w[2])?, f(w[3])?, f(w[4])?, shape);
            }
        }
        "studio_traces" => m.studio_overlay = match v { "overlay" => true, "stacked" => false, _ => return Err(format!("bad studio_traces {v}")) },
        "studio_meas_view" => m.studio_detail = match v { "detailed" => true, "list" => false, _ => return Err(format!("bad studio_meas_view {v}")) },
        "studio_badges" => m.studio_classic = match v { "classic" => true, "studio" => false, _ => return Err(format!("bad studio_badges {v}")) },
        "studio_ov" => m.studio_ov = v.parse::<i32>().ok().filter(|h| (0..=2000).contains(h)).ok_or_else(|| format!("bad studio_ov {v}"))?,
        "draw_path" => {
            m.draw_batch = match v.trim() {
                "batch" => true,
                "standard" => false,
                _ => return Err(format!("{:?}: standard or batch", v)),
            }
        }
        "bar" => {
            m.bar_compact = match v.trim() {
                "compact" => true,
                "cards" => false,
                _ => return Err(format!("{:?}: cards or compact", v)),
            }
        }
        "hdmi" => m.hdmi = p_enum(v, &HDMI_MODES)?,
        "hdmi_view" => {
            let n = v.trim().parse::<usize>().map_err(|e| e.to_string())?;
            if n > crate::ui::VIEWS_MAX {
                return Err(format!("{:?}: 0 (the panel's) to {}", v, crate::ui::VIEWS_MAX));
            }
            m.hdmi_view = n;
        }
        "views" => {
            let n = p_list(v, 2, |x| x.parse::<usize>().map_err(|e| e.to_string()))?;
            if !(1..=crate::ui::VIEWS_MAX).contains(&n[0]) || !(1..=n[0]).contains(&n[1]) {
                return Err(format!("{:?}: count 1 to {}, then the one shown", v, crate::ui::VIEWS_MAX));
            }
            (m.views, m.view_cur) = (n[0], n[1] - 1);
        }
        k if k.strip_prefix("view").and_then(|i| i.parse::<usize>().ok()).is_some_and(|i| (1..=crate::ui::VIEWS_MAX).contains(&i)) => {
            let i: usize = k[4..].parse().unwrap_or(1);
            m.view_slots[i - 1] = crate::ui::views::slot_checked(v)?;
        }
        // setups before the Layout mode: true = Waveform, false = Mirror
        "hdmi_wave" => m.hdmi = if p_bool(v)? { HdmiMode::Waveform } else { HdmiMode::Mirror },
        "view" => m.view = p_enum(v, &crate::ui::stored_views())?,
        "bode" => {
            // the address is the rest of the line: it may be empty
            let w: Vec<&str> = v.trim().splitn(8, char::is_whitespace).collect();
            if w.len() < 7 {
                return Err("bode: want 8 values".into());
            }
            let f = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
            (m.bode_start, m.bode_stop, m.bode_amp) = (f(w[0])?, f(w[1])?, f(w[3])?);
            m.bode_points = p_u32(w[2])?.clamp(2, 1000) as usize;
            m.bode_in = WaveId::parse(w[4])?;
            m.bode_out = WaveId::parse(w[5])?;
            m.bode_gen = p_idx(w[6], crate::ui::bode::GENERATORS.len())?;
            // a file from before the Ext generator page names the external generator here only
            if crate::extgen::external(m.bode_gen) {
                m.ext_model = m.bode_gen;
            }
            m.bode_addr = w.get(7).map_or("", |a| a.trim()).to_string();
        }
        "extgen" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 5 {
                return Err("extgen: want 5 values".into());
            }
            let model = p_idx(w[0], crate::extgen::GENERATORS.len())?;
            if !crate::extgen::external(model) {
                return Err(format!("extgen: {} is not an external generator", model));
            }
            m.ext_model = model;
            if crate::extgen::external(m.bode_gen) {
                m.bode_gen = model;
            }
            m.ext_ch = p_idx(w[1], 3)?.max(1) - 1;
            m.ext_hiz = match w[2] {
                "hiz" => true,
                "50" => false,
                _ => return Err(format!("extgen: load is hiz or 50, not {}", w[2])),
            };
            let f = |s: &str| s.parse::<f64>().ok().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| format!("extgen: not a positive number: {}", s));
            (m.ext_freq, m.ext_ampl) = (f(w[3])?, f(w[4])?);
        }
        "bode_link" => {
            m.bode_usb = match v.trim() {
                "usb" => true,
                "lan" => false,
                _ => return Err(format!("not lan or usb: {}", v)),
            }
        }
        "bode_sweep" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 7 {
                return Err("bode_sweep: want 7 values".into());
            }
            let f = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
            m.bode_sw_time = f(w[0])?;
            if !m.bode_sw_time.is_finite() || m.bode_sw_time <= 0.0 {
                return Err("bode_sweep: the sweep time must be above 0".into());
            }
            m.bode_sw_log = p_bool(w[1])?;
            m.bode_sync = p_idx(w[2], NCH + 1)?.max(1) - 1;
            m.bode_sync_slope = p_enum(w[3], &SLOPES[..2])?;
            m.bode_sync_level = p_f32(w[4])?;
            m.bode_sw_in = p_idx(w[5], NCH + 1)?;
            m.bode_ref = f(w[6])?;
        }
        "spec" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 9 {
                return Err("spec: want 9 values".into());
            }
            let f = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
            m.spec_src = WaveId::parse(w[0])?;
            (m.spec_center, m.spec_span, m.spec_rbw) = (f(w[1])?, f(w[2])?, f(w[3])?);
            m.spec_win = p_enum(w[4], &WINDOWS)?;
            m.spec_trace = p_idx(w[5], 4)?;
            m.spec_ref = p_f32(w[6])?;
            (m.spec_wf, m.spec_dist) = (p_bool(w[7])?, p_bool(w[8])?);
        }
        "eye" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            m.eye_src = WaveId::parse(w.first().ok_or("eye: source")?)?;
            m.eye_rate = w.get(1).ok_or("eye: rate")?.parse::<f64>().map_err(|e| e.to_string())?;
            // older files have no third field: the rate was entered, not measured
            m.eye_from_record = w.get(2).is_some_and(|s| *s != "0");
        }
        "eye_pll" => m.eye_pll_bw = v.trim().parse::<f64>().ok().filter(|x| x.is_finite() && *x >= 0.0).ok_or("eye_pll: Hz")?.min(crate::model::EYE_PLL_MAX),
        "eye_place" => m.eye_place = p_enum(v, &[EyePlace::Full, EyePlace::Side, EyePlace::Under])?,
        "power" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 3 {
                return Err("power: want 3 values".into());
            }
            m.pw_v = WaveId::parse(w[0])?;
            m.pw_i = WaveId::parse(w[1])?;
            m.pw_mode = p_enum(w[2], &[PowerMode::Quality, PowerMode::Harmonics, PowerMode::Switching, PowerMode::Ripple, PowerMode::Capacitance, PowerMode::Rail])?;
        }
        "magnetic" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 8 {
                return Err("magnetic: want 8 values".into());
            }
            let f = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
            m.mag_v = WaveId::parse(w[0])?;
            m.mag_i = WaveId::parse(w[1])?;
            m.mag_n = f(w[2])?.clamp(1.0, 10_000.0);
            m.mag_ae = f(w[3])?.max(0.0);
            m.mag_le = f(w[4])?.max(0.0);
            m.mag_rcu = f(w[5])?.max(0.0);
            m.mag_plot = p_enum(w[6], &[MagPlot::IPsi, MagPlot::BH])?;
            m.mag_cur = w[7].parse::<usize>().map_err(|e| e.to_string())?;
        }
        "inverter" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 23 {
                return Err("inverter: want 23 values".into());
            }
            let f = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
            let ch = |s: &str| WaveId::parse(s);
            let opt = |s: &str| -> Result<Option<WaveId>, String> { if s == "off" { Ok(None) } else { Ok(Some(ch(s)?)) } };
            let bit = |s: &str| -> Result<u8, String> {
                let n = s.parse::<u8>().map_err(|e| e.to_string())?;
                if n < 16 { Ok(n) } else { Err(format!("logic line {} out of range", n)) }
            };
            m.inv_page = p_enum(w[0], &[InvPage::Dq0, InvPage::Mech])?;
            (m.inv_a, m.inv_b, m.inv_c) = (ch(w[1])?, ch(w[2])?, ch(w[3])?);
            m.inv_ang = p_enum(w[4], &[InvAngle::Off, InvAngle::Phase, InvAngle::Analog, InvAngle::Quad, InvAngle::Hall])?;
            m.inv_poles = p_u32(w[5])?.clamp(1, 32);
            m.inv_an = ch(w[6])?;
            m.inv_volt = p_bool(w[7])?;
            (m.inv_tq, m.inv_ev, m.inv_ei) = (opt(w[8])?, opt(w[9])?, opt(w[10])?);
            (m.inv_scale, m.inv_degv, m.inv_off) = (f(w[11])?, f(w[12])?, f(w[13])?);
            m.inv_elec = p_bool(w[14])?;
            (m.inv_qa, m.inv_qb) = (bit(w[15])?, bit(w[16])?);
            m.inv_qi = if w[17] == "off" { None } else { Some(bit(w[17])?) };
            m.inv_cpr = f(w[18])?.clamp(1.0, 100_000.0);
            m.inv_swap = p_bool(w[19])?;
            (m.inv_ha, m.inv_hb, m.inv_hc) = (bit(w[20])?, bit(w[21])?, bit(w[22])?);
        }
        "dpt" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 7 {
                return Err("dpt: want 7 values".into());
            }
            m.dpt_v = WaveId::parse(w[0])?;
            m.dpt_i = WaveId::parse(w[1])?;
            m.dpt_g = WaveId::parse(w[2])?;
            m.dpt_4th = p_enum(w[3], &[DptFourth::Off, DptFourth::Gate, DptFourth::DiodeV, DptFourth::DiodeI])?;
            m.dpt_4ch = WaveId::parse(w[4])?;
            m.dpt_pct = w[5].parse::<f64>().map_err(|e| e.to_string())?.clamp(1.0, 49.0);
            m.dpt_zoom = p_enum(w[6], &[DptZoom::Both, DptZoom::TurnOn, DptZoom::TurnOff])?;
        }
        "zone1" | "zone2" => {
            let z = &mut m.zones[if k == "zone1" { 0 } else { 1 }];
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() < 3 {
                return Err("zone: want on src inside points".into());
            }
            z.on = p_bool(w[0])?;
            z.src = p_idx(w[1], NCH + 1)?.max(1) - 1;
            z.inside = p_bool(w[2])?;
            z.pts = w[3..]
                .iter()
                .map(|p| {
                    let (t, v) = p.split_once(':').ok_or("zone point: want t:v")?;
                    Ok((t.parse::<f64>().map_err(|e| e.to_string())?, v.parse::<f64>().map_err(|e| e.to_string())?))
                })
                .collect::<Result<_, String>>()?;
        }
        "z_src" => m.z_src = if v.trim() == "off" { None } else { Some(p_idx(v, NCH + 1)?.max(1) - 1) },
        "hist" => m.hist = p_enum(v, &[Hist::Off, Hist::Vertical, Hist::Horizontal])?,
        "mask_on" => m.mask_on = p_bool(v)?,
        "mask_src" => m.mask_src = p_idx(v, NCH)?,
        "mask_x" => m.mask_x = p_f32(v)?,
        "mask_y" => m.mask_y = p_f32(v)?,
        "mask_stop" => m.mask_stop = p_bool(v)?,
        "mask_inv" => m.mask_inv = p_bool(v)?,
        "quick" => m.quick = p_enum(v, &QUICKS)?,
        "text_size" => m.text_size = p_enum(v, &TEXT_SIZES)?,
        "la" => m.la = crate::ui::la::LaSet::parse(v)?,
        "dec_la" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 5 {
                return Err(format!("dec_la: {:?}", v));
            }
            m.dec_la_in = w[0] == "1";
            for (r, s) in m.dec_la.iter_mut().zip(&w[1..]) {
                *r = if *s == "-" { None } else { Some(s.parse::<u8>().map_err(|_| format!("dec_la: {:?}", v))?.min(15)) };
            }
        }
        "meas_levels" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 16 {
                return Err("meas_levels: 16 values".into());
            }
            let f = |i: usize| w[i].parse::<f64>().map_err(|e| e.to_string());
            let l = &mut m.meas_lv;
            l.absolute = p_bool(w[0])?;
            l.custom = p_bool(w[1])?;
            (l.mid, l.hyst) = (f(2)?, f(3)?);
            l.volts = [f(4)?, f(5)?, f(6)?];
            l.vhyst = f(7)?;
            l.sep_fall = p_bool(w[8])?;
            l.fall = [f(9)?, f(10)?, f(11)?];
            l.fall_volts = [f(12)?, f(13)?, f(14)?];
            l.base_top = p_enum(w[15], &oos_dsp::measure::BaseTop::ALL)?;
        }
        "meas_full_record" => m.meas_full_record = p_bool(v)?,
        "meas_stat" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            let [pop, sd, min, max] = w[..] else { return Err("meas_stat: population sd min max".into()) };
            m.meas_pop_current = p_bool(pop)?;
            m.meas_show = [p_bool(sd)?, p_bool(min)?, p_bool(max)?];
        }
        "meas_cfg" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() != 4 && w.len() != 8 {
                return Err("meas_cfg: idle outside n data [lpct hpct tsetup thold]".into());
            }
            m.meas_idle = w[0].parse::<f64>().map_err(|e| e.to_string())?;
            m.meas_out = p_enum(w[1], &[oos_dsp::measure::Outside::Above, oos_dsp::measure::Outside::Below, oos_dsp::measure::Outside::Both])?;
            m.meas_n = p_u32(w[2])?.clamp(1, 32);
            m.meas_data = p_bool(w[3])?;
            if w.len() == 8 {
                m.meas_lpct = w[4].parse::<f64>().map_err(|e| e.to_string())?;
                m.meas_hpct = w[5].parse::<f64>().map_err(|e| e.to_string())?;
                m.meas_tsetup = w[6].parse::<f64>().map_err(|e| e.to_string())?;
                m.meas_thold = w[7].parse::<f64>().map_err(|e| e.to_string())?;
            }
        }
        "spec_acpr" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            let [width, space] = w[..] else { return Err("spec_acpr: width spacing".into()) };
            m.spec_acpr_w = width.parse::<f64>().map_err(|e| e.to_string())?;
            m.spec_acpr_s = space.parse::<f64>().map_err(|e| e.to_string())?;
        }
        "bode_mode" => m.bode_mode = p_enum(v, &[oos_dsp::bode::Mode::Loop, oos_dsp::bode::Mode::Psrr, oos_dsp::bode::Mode::Z])?,
        "sim" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            if w.len() < 2 {
                return Err("sim: preset slot".into());
            }
            m.sim_preset = p_idx(w[0], oos_dsp::serial::LANES.len())?;
            m.sim_slot = p_idx(w[1], NREF)?;
            m.apply_sim_preset();
            if w.len() >= 14 {
                let f = |s: &str| s.parse::<f64>().map_err(|e| e.to_string());
                m.sim_rate = f(w[2])?;
                m.sim_prbs = w[3].parse::<u8>().map_err(|e| e.to_string())?;
                m.sim_vhigh = f(w[4])?;
                m.sim_vlow = f(w[5])?;
                m.sim_rise = f(w[6])?;
                m.sim_noise = f(w[7])?;
                m.sim_bw = f(w[8])?;
                m.sim_deemph = f(w[9])?;
                m.sim_ssc = p_bool(w[10])?;
                m.sim_rj = f(w[11])?;
                m.sim_pj = f(w[12])?;
                m.sim_pjhz = f(w[13])?;
            }
        }
        "sim_extra" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            let [center, deviation, hz, dcd, corr, corner] = w[..] else {
                return Err("sim_extra: center deviation frequency DCD correlated-RJ corner".into());
            };
            let bounded = |s: &str, low: f64, high: f64| -> Result<f64, String> {
                s.parse::<f64>().ok().filter(|value| value.is_finite() && *value >= low && *value <= high)
                    .ok_or_else(|| format!("{s} must be between {low} and {high}"))
            };
            m.sim_ssc_center = p_bool(center)?;
            m.sim_ssc_deviation = bounded(deviation, 0.0, 0.05)?;
            m.sim_ssc_hz = bounded(hz, 1e3, 1e6)?;
            m.sim_dcd_pp = bounded(dcd, 0.0, 100e-12)?;
            m.sim_corr_rj = bounded(corr, 0.0, 50e-12)?;
            m.sim_corr_hz = bounded(corner, 1e3, 100e6)?;
        }
        "eye_more" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            let [ber, lo, hi, npj, rail, notch, b, hp] = w[..] else { return Err("eye_more".into()) };
            let opt = |s: &str| if s == "-" { Ok(None) } else { WaveId::parse(s).map(Some) };
            m.eye_ber = ber.parse::<f64>().map_err(|e| e.to_string())?;
            m.eye_pn_lo = lo.parse::<f64>().map_err(|e| e.to_string())?;
            m.eye_pn_hi = hi.parse::<f64>().map_err(|e| e.to_string())?;
            m.eye_npj = p_bool(npj)?;
            m.eye_rail = opt(rail)?;
            m.eye_notch = notch.parse::<f64>().map_err(|e| e.to_string())?;
            m.eye_b = opt(b)?;
            m.eye_hp = p_bool(hp)?;
        }
        "bus_on" => m.bus.on = p_bool(v)?,
        "bus_label" => m.bus.label = crate::model::bus_label(v),
        "bus_pos" => m.bus.pos = p_f32(v)?.clamp(BUS_POS_MIN, BUS_POS_MAX),
        "bus_kind" => m.bus.kind = p_enum(v, &[BusKind::I2c])?,
        "bus_clk" => m.bus.clk = WaveId::parse(v)?,
        "bus_data" => m.bus.data = WaveId::parse(v)?,
        "bus_clk_thr" => m.bus.clk_thr = p_f32(v)?.clamp(-40.0, 40.0),
        "bus_data_thr" => m.bus.data_thr = p_f32(v)?.clamp(-40.0, 40.0),
        "bus_rw" => m.bus.addr_rw = p_bool(v)?,
        "power_more" => {
            let w: Vec<&str> = v.split_whitespace().collect();
            let [fline, fsw, vin, reference, clk] = w[..] else { return Err("power_more".into()) };
            let opt = |s: &str| if s == "-" { Ok(None) } else { WaveId::parse(s).map(Some) };
            m.pw_fline = fline.parse::<f64>().map_err(|e| e.to_string())?;
            m.pw_fsw = fsw.parse::<f64>().map_err(|e| e.to_string())?;
            m.pw_vin = opt(vin)?;
            m.pw_ref = if reference == "-" { None } else { Some(reference.parse::<f64>().map_err(|e| e.to_string())?) };
            m.pw_clk = opt(clk)?;
        }
        _ => {} // unknown keys of newer versions are ignored
    }
    Ok(())
}

/// Parses a setup file on top of the default set-up (keys missing in the file keep their defaults). Errors name the
/// line and the key; unknown keys are ignored.
#[cfg_attr(not(test), allow(dead_code))] // the strict form, for the tests
pub fn parse_setup(text: &str) -> Result<Model, String> {
    let (m, bad) = parse_setup_lenient(text);
    match bad.into_iter().next() {
        Some(e) => Err(e),
        None => Ok(m),
    }
}

/// `parse_setup` that loads what it can: a line it cannot take (a key of a newer or another build, a protocol not in
/// this one, a value out of range) is left out and named in the list, the rest of the setup stands. A decoder whose
/// protocol is not in this build is switched off, its other settings kept. For recalling a setup and the restart.
pub fn parse_setup_lenient(text: &str) -> (Model, Vec<String>) {
    let mut bad = Vec::new();
    let m = parse_lines(text, &mut bad);
    (m, bad)
}

fn parse_lines(text: &str, bad: &mut Vec<String>) -> Model {
    let mut m = Model::default();
    // a setup from before the gate setting measured the visible window; keep it that way
    m.meas_full_record = false;
    let mut proto_missing = false;
    for (ln, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            bad.push(format!("line {}: no '='", ln + 1));
            continue;
        };
        let k = k.trim();
        if let Err(e) = set(&mut m, k, v.trim()) {
            proto_missing |= k == "dec_proto";
            bad.push(format!("line {}: {}: {}", ln + 1, k, e));
        }
    }
    if proto_missing {
        m.dec_on = false;
    }
    // a protocol that became a module since the setup was written: its options from the shared decoder rows
    if let (Proto::Module(i), false) = (m.dec_proto, text.lines().any(|l| l.split('=').next().is_some_and(|k| k.trim() == "dec_opt"))) {
        let old = crate::protocols::Legacy { baud: m.dec_baud, bits: m.dec_bits, parity: m.dec_parity, canrate: m.dec_canrate, canh: m.dec_canh, fdrate: m.dec_fdrate, arate: m.dec_arate, tick: m.dec_tick, nibbles: m.dec_nibbles, uspeed: m.dec_uspeed };
        if let Some(o) = crate::protocols::get(i).legacy(&old) {
            m.dec_opt = o.map(|x| x as u8);
            m.dec_opt = crate::protocols::opt_fit(i, m.dec_opt).map(|x| x as u8);
        }
    }
    // a setup from before the density row: the windows and the pane already say which of the four it was
    if !text.lines().any(|l| l.split('=').next().is_some_and(|k| k.trim() == "density")) {
        m.density = match m.win_layout {
            WinLayout::Quad => Density::Four,
            WinLayout::Cols | WinLayout::Rows => Density::Split,
            WinLayout::One if m.pane != 0 => Density::Split,
            WinLayout::One => Density::Scope,
        };
        if m.density == Density::Split {
            m.split_pane = m.pane;
        }
    }
    if m.zscale > m.tscale {
        m.zscale = m.tscale;
    }
    m
}

/// Display state of a ref slot, kept in a setup file as `ref<N>.<key>` lines. The samples are not part of a setup:
/// a recall restores how the slot looks, and applies it to the slot when that holds a waveform.
#[derive(Clone, Default, Debug, PartialEq)]
pub struct RefLook {
    pub on: Option<bool>,
    pub vdiv: Option<f32>,
    pub offset: Option<f32>,
    pub color: Option<usize>,
    pub label: Option<String>,
}

/// `ref<N>.*` lines for the loaded slots (`None` = empty slot, nothing is written).
pub fn ref_setup_text(looks: &[Option<RefLook>]) -> String {
    let mut s = String::new();
    for (i, l) in looks.iter().enumerate() {
        let Some(l) = l else { continue };
        let mut kv = |k: &str, v: String| s.push_str(&format!("ref{}.{} = {}\n", i + 1, k, v));
        if let Some(v) = l.on { kv("on", v.to_string()); }
        if let Some(v) = l.vdiv { kv("vdiv", v.to_string()); }
        if let Some(v) = l.offset { kv("offset", v.to_string()); }
        if let Some(v) = l.color { kv("color", v.to_string()); }
        if let Some(v) = &l.label { kv("name", v.replace(['\r', '\n'], " ")); }
    }
    s
}

/// The `ref<N>.*` lines of a setup file; a setup from before they existed gives all `None`.
pub fn parse_ref_looks(text: &str, n: usize) -> Vec<Option<RefLook>> {
    let mut out: Vec<Option<RefLook>> = vec![None; n];
    for line in text.lines() {
        let Some((k, v)) = line.trim().split_once('=') else { continue };
        let Some(rest) = k.trim().strip_prefix("ref") else { continue };
        let Some((idx, key)) = rest.split_once('.') else { continue };
        let Some(i) = idx.parse::<usize>().ok().and_then(|i| i.checked_sub(1)).filter(|&i| i < n) else { continue };
        let v = v.trim();
        let l = out[i].get_or_insert_with(RefLook::default);
        match key {
            "on" => l.on = v.parse().ok(),
            "vdiv" => l.vdiv = v.parse().ok().filter(|x: &f32| x.is_finite() && *x > 0.0),
            "offset" => l.offset = v.parse().ok().filter(|x: &f32| x.is_finite()),
            "color" => l.color = v.parse().ok(),
            "name" => l.label = Some(v.to_string()),
            _ => {}
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------- waveform CSV

/// CSV of the enabled channels of a record: `Time (s),CH1 (V),...`, one row per sample, time relative to the
/// trigger; samples without data (NaN) are empty fields.
pub fn wave_csv(rec: &Record, m: &Model) -> String {
    let chans: Vec<(usize, &[f32])> = (0..NCH).filter(|&c| m.ch[c].on && !rec.ch[c].is_empty()).map(|c| (c, rec.ch[c].as_slice())).collect();
    wave_csv_of(rec.t0, rec.dt, &chans)
}

/// The CSV of `wave_csv` for any set of channels (number from 0, samples) on one time axis: the acquisition memory.
pub fn wave_csv_of(t0: f64, dt: f64, chans: &[(usize, &[f32])]) -> String {
    let n = chans.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
    let mut s = String::with_capacity(n * (16 + 12 * chans.len()) + 64);
    s.push_str("Time (s)");
    for (c, _) in chans {
        s.push_str(&format!(",CH{} (V)", c + 1));
    }
    s.push('\n');
    for k in 0..n {
        s.push_str(&format!("{:.9e}", t0 + k as f64 * dt));
        for (_, samples) in chans {
            s.push(',');
            if let Some(v) = samples.get(k).filter(|v| v.is_finite()) {
                s.push_str(&v.to_string());
            }
        }
        s.push('\n');
    }
    s
}

/// Logic analyser record as CSV: the lines shown, one row at the record start and one for every change of them, so a
/// record of millions of samples stays small and nothing is lost (the levels hold until the next row). The last row is
/// the end of the record.
pub fn la_csv(la: &crate::ui::la::LaRec, lines: &[usize]) -> String {
    let mask = lines.iter().fold(0u16, |m, &n| m | 1 << n);
    let mut s = String::from("Time (s)");
    for n in lines {
        s.push_str(&format!(",D{}", n));
    }
    s.push('\n');
    let row = |s: &mut String, k: usize, w: u16| {
        s.push_str(&format!("{:.9e}", la.t0 + k as f64 * la.dt));
        for &n in lines {
            s.push_str(if w >> n & 1 == 1 { ",1" } else { ",0" });
        }
        s.push('\n');
    };
    let mut prev = None;
    for (k, &w) in la.words.iter().enumerate() {
        if prev != Some(w & mask) {
            row(&mut s, k, w);
            prev = Some(w & mask);
        }
    }
    if let (Some(&w), true) = (la.words.last(), la.words.len() > 1) {
        row(&mut s, la.words.len() - 1, w);
    }
    s
}

/// The record in a plain binary form of OOS, little endian:
/// "OOSW", u32 version 1, u32 number of channels, u64 samples per channel, f64 dt, f64 t0, then per channel its number
/// (u32, 1..4) and the samples as f32 volts.
pub fn wave_bin(rec: &Record, m: &Model) -> Vec<u8> {
    let chans: Vec<(usize, &[f32])> = (0..NCH).filter(|&c| m.ch[c].on && !rec.ch[c].is_empty()).map(|c| (c, rec.ch[c].as_slice())).collect();
    wave_bin_of(rec.t0, rec.dt, &chans)
}

/// The binary form of `wave_bin` for any set of channels (number from 0, samples) on one time axis.
pub fn wave_bin_of(t0: f64, dt: f64, chans: &[(usize, &[f32])]) -> Vec<u8> {
    let n = chans.iter().map(|(_, v)| v.len()).max().unwrap_or(0);
    let mut b = Vec::with_capacity(36 + chans.len() * (4 + 4 * n));
    b.extend_from_slice(b"OOSW");
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&(chans.len() as u32).to_le_bytes());
    b.extend_from_slice(&(n as u64).to_le_bytes());
    b.extend_from_slice(&dt.to_le_bytes());
    b.extend_from_slice(&t0.to_le_bytes());
    for (c, samples) in chans {
        b.extend_from_slice(&(*c as u32 + 1).to_le_bytes());
        for i in 0..n {
            b.extend_from_slice(&samples.get(i).copied().unwrap_or(f32::NAN).to_le_bytes());
        }
    }
    b
}

/// One trace taken from a file: volts, the sample step, and the time of the first sample.
#[derive(Clone, Debug)]
pub struct WaveTrace {
    pub t0: f64,
    pub dt: f64,
    pub v: Vec<f32>,
    pub label: String,
}

/// Import bounds shared by native readers and remote uploads. They include the
/// supplied 80 MB / 20-million-point captures without unbounded allocations.
pub const MAX_WAVEFORM_BYTES: usize = 128 << 20;
pub const MAX_WAVEFORM_SAMPLES: usize = 32_000_000;

/// File systems of the kernel: their files can be regular and still endless (`/proc/kmsg`), or a device.
const NOT_STORAGE: [&str; 3] = ["/proc", "/sys", "/dev"];

/// The bytes of a waveform file named by a remote client (SCPI `:REFerence:LOAD`, also over VXI-11 and MCP) or found
/// beside the setups, read on the frame loop. Refused: a file of `NOT_STORAGE` (by the path it resolves to), and one
/// larger than the import limit, from its size before anything is read; the read stops one byte past the limit should
/// the file grow meanwhile. A path that is not there or not a regular file is `missing <name>`.
pub fn read_waveform_file(p: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let name = file_name(p);
    let missing = || format!("missing {name}");
    let real = p.canonicalize().map_err(|_| missing())?;
    if NOT_STORAGE.iter().any(|d| real.starts_with(d)) {
        return Err(format!("{name}: files under /proc, /sys and /dev are not read"));
    }
    let too_large = || format!("{name} exceeds the {} MiB import limit", MAX_WAVEFORM_BYTES >> 20);
    // before the open: opening a FIFO would wait for a writer
    let meta = std::fs::metadata(&real).map_err(|_| missing())?;
    if !meta.is_file() {
        return Err(missing());
    }
    if meta.len() > MAX_WAVEFORM_BYTES as u64 {
        return Err(too_large());
    }
    let file = std::fs::File::open(&real).map_err(|_| missing())?;
    let mut bytes = Vec::with_capacity(meta.len() as usize);
    file.take(MAX_WAVEFORM_BYTES as u64 + 1).read_to_end(&mut bytes).map_err(|e| format!("{name}: {e}"))?;
    if bytes.len() > MAX_WAVEFORM_BYTES {
        return Err(too_large());
    }
    Ok(bytes)
}

/// Extensions shown by native and browser waveform import. The header chooses the parser.
/// `.txt` is not one of them though a Keysight Y-values file has it: the setups are `.txt` too, and a list of
/// waveforms would show them. Such a file is opened by name, from All files, or by dropping it.
pub const WAVE_EXTS: &[&str] = &["wfm", "bin", "csv", "trc", "isf", "tr0", "wfmx", "pwwf", "tkwf"];

/// A stored waveform, recognized by its header:
/// `OOSW`; `PWWF0001` or `TKWF0001`; `:WFM#001`, `:WFM#002` or `:WFM#003`; `:WFMPRE:`; `WAVEDESC`; cookie `AG` or `RG`;
/// a `$&%#` block (`9601`, `9007`, or `2001`) or a `Title:` / `Plotname:` transient (`Binary:` or `Values:`);
/// XML `WaveformInternal` or `WaveformPatternInternal`; a Siglent header of version 2, 3, 4 or 6; the first bytes of a RIGOL
/// DHO800, DHO900, DHO1000 or DS1000E waveform file; or a CSV time column.
/// Anything else is refused.
pub fn parse_wave(bytes: &[u8], name: &str) -> Result<Vec<WaveTrace>, String> {
    if bytes.len() > MAX_WAVEFORM_BYTES { return Err("waveform exceeds the 128 MiB import limit".into()); }
    if bytes.starts_with(b"OOSW") {
        return parse_oosw(bytes);
    }
    if looks_pwwf(bytes) {
        return parse_pwwf(bytes).map(|t| vec![t]);
    }
    if looks_wfm(bytes) {
        return parse_wfm(bytes).map(|t| vec![t]);
    }
    if looks_isf(bytes) {
        return parse_isf(bytes);
    }
    if looks_wavedesc(bytes) {
        return parse_wavedesc(bytes);
    }
    if looks_ag(bytes) {
        return parse_ag(bytes);
    }
    if looks_tr0(bytes) {
        return parse_tr0(bytes);
    }
    if let Some(kind) = rigol_wfm_kind(bytes) {
        return parse_rigol_wfm(bytes, kind);
    }
    if let Some(version) = siglent_version(bytes) {
        return parse_siglent(bytes, version);
    }
    // one half of a waveform kept in two files: without where it lies, the other half cannot be found
    if looks_rs_description(bytes) {
        return Err("this describes a Rohde & Schwarz waveform; open it from its folder, where its .Wfm.bin file is".into());
    }
    if looks_wfmx(bytes) {
        return parse_wfmx(bytes);
    }
    // containers this build does not open: said by name, with the way round them
    if bytes.starts_with(&[0x89, b'H', b'D', b'F', 0x0d, 0x0a, 0x1a, 0x0a]) {
        return Err("HDF5 (.h5) waveform files are not read; save the waveform as BIN (.bin) or CSV".into());
    }
    if bytes.starts_with(b"MATLAB ") {
        return Err("MATLAB (.mat) waveform files are not read; save the waveform as BIN (.bin) or CSV".into());
    }
    if let Ok(text) = std::str::from_utf8(bytes) {
        if keysight_text_kind(text) == Some("WaveformYValues") {
            return parse_keysight_y(text, name).map(|t| vec![t]);
        }
        let rigol = rigol_csv_header(text)?;
        let mut traces = parse_wave_csv(text, name)?;
        if let Some(header) = rigol {
            // the first column counted samples: the header has the time of sample 0 and of one step
            if let Some((start, step)) = header.clock {
                for t in &mut traces {
                    (t.t0, t.dt) = (start + t.t0 * step, t.dt * step);
                }
            }
            if header.channels.len() == traces.len() {
                for (t, channel) in traces.iter_mut().zip(header.channels) {
                    t.label = channel;
                }
            }
        }
        // an XY-values file names its source in its header; with one trace, that name is the trace's
        if let ([only], Some("WaveformXYValues")) = (traces.as_mut_slice(), keysight_text_kind(text)) {
            if let Some(source) = keysight_header(text, "Source Name") {
                only.label = source.chars().take(10).collect();
            }
        }
        return Ok(traces);
    }
    Err("that file is not a waveform".into())
}

/// What the first lines of a RIGOL CSV file say: the channel of each column and, in a file whose first column
/// counts samples, the time of sample 0 and the time between samples.
struct RigolCsv {
    channels: Vec<String>,
    clock: Option<(f64, f64)>,
}

/// The header of a RIGOL CSV file, None for another file. The first line names the columns: `X,CH1,CH2,`, on a
/// DS1000B `,CH2,CH4`, on a DS1000D `X,CH 1 (V),CH 2 (V)`. A DS1000Z, DS2000 or DS4000 goes on
/// `,Start,Increment,`; its second line has the two values under those names, and the first column of its rows
/// counts samples, not seconds:
/// `X,CH1,CH4,Start,Increment,` / `Sequence,Volt,Volt,-1.200000e-03,4.000000e-08` / `0,2.86e+00,3.08e+00,`.
/// Such a file without its two values is refused: read as it stands it would be a second a sample.
fn rigol_csv_header(text: &str) -> Result<Option<RigolCsv>, String> {
    let mut lines = without_mark(text).lines();
    let cells = |line: &str| line.split(',').map(|c| c.trim().to_string()).collect::<Vec<String>>();
    let first = cells(lines.next().unwrap_or(""));
    if !(first[0].is_empty() || first[0] == "X") {
        return Ok(None);
    }
    let mut channels = Vec::new();
    while let Some(cell) = first.get(1 + channels.len()) {
        let plain: String = cell.chars().filter(|c| !c.is_whitespace()).collect();
        let Some(number) = plain.strip_prefix("CH").and_then(|rest| rest.chars().next()).filter(|c| c.is_ascii_digit()) else { break };
        channels.push(format!("CH{number}"));
    }
    let named = |k: usize, name: &str| first.get(k).is_some_and(|cell| cell == name);
    let at = 1 + channels.len();
    if channels.is_empty() {
        return Ok(None);
    }
    if !(named(at, "Start") && named(at + 1, "Increment")) {
        return Ok(Some(RigolCsv { channels, clock: None }));
    }
    let second = cells(lines.next().unwrap_or(""));
    let value = |k: usize| second.get(k).and_then(|cell| cell.parse::<f64>().ok()).filter(|x| x.is_finite());
    match (value(at), value(at + 1)) {
        (Some(start), Some(step)) if step > 0.0 => Ok(Some(RigolCsv { channels, clock: Some((start, step)) })),
        _ => Err("the RIGOL CSV file has no usable Start and Increment".into()),
    }
}

/// A text file without the byte-order mark some programs put first.
fn without_mark(text: &str) -> &str {
    text.trim_start_matches(|c: char| c as u32 == 0xfeff)
}

/// The header of a Keysight text waveform file, up to its `Data,` line: lines of `Key, value`.
fn keysight_header_lines(text: &str) -> impl Iterator<Item = (&str, &str)> {
    without_mark(text).lines().take(64)
        .take_while(|line| !line.trim_start().starts_with("Data"))
        .filter_map(|line| line.split_once(',')).map(|(key, value)| (key.trim(), value.trim()))
}

/// The value of one header line of a Keysight text waveform file.
fn keysight_header<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    keysight_header_lines(text).find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v).filter(|v| !v.is_empty())
}

/// What a Keysight text waveform file says it is in its first line (`File Format, WaveformYValues`).
fn keysight_text_kind(text: &str) -> Option<&str> {
    let (key, value) = without_mark(text).lines().next()?.split_once(',')?;
    key.trim().eq_ignore_ascii_case("File Format").then(|| value.trim())
}

/// A Keysight Y-values file (`.txt`): a header of `Key, value` lines, a `Data,` line, in format version 2 a line
/// naming the number type, then one amplitude per line. The time of the first point and the interval are the
/// header's `XOrg` and `XInc`. A file saved without its header has no time scale and is not a waveform here.
fn parse_keysight_y(text: &str, name: &str) -> Result<WaveTrace, String> {
    let number = |key: &str| keysight_header(text, key).and_then(|v| v.parse::<f64>().ok()).filter(|x| x.is_finite());
    let (Some(t0), Some(dt)) = (number("XOrg"), number("XInc")) else {
        return Err("the Y-values file has no XOrg and XInc in its header".into());
    };
    if dt <= 0.0 {
        return Err("that file holds no waveform".into());
    }
    let mut v = Vec::new();
    let data = text.lines().skip_while(|line| !line.trim_start().starts_with("Data")).skip(1);
    for line in data {
        // the number-type line of version 2, a blank line, or an ellipsis: not a sample
        let Ok(x) = line.trim().trim_end_matches(',').parse::<f32>() else { continue };
        if !x.is_finite() { continue; }
        if v.len() >= MAX_WAVEFORM_SAMPLES { return Err("waveform exceeds the 32-million-sample import limit".into()); }
        v.push(x);
    }
    if v.len() < 2 {
        return Err("that file holds no waveform".into());
    }
    let label = keysight_header(text, "Source Name").unwrap_or(name).chars().take(10).collect();
    Ok(WaveTrace { t0, dt, v, label })
}

/// A waveform record (`.pwwf`, and the same layout under its older `.tkwf` name): one channel of a capture.
fn looks_pwwf(bytes: &[u8]) -> bool {
    bytes.len() >= 12 && (bytes.starts_with(b"PWWF0001") || bytes.starts_with(b"TKWF0001"))
}

/// The record: the 8-byte magic, a little-endian u32 header length, a JSON header, then little-endian samples.
/// The header gives `sampleType` (`int8`, `int16`, `uint16` or `float32`), `nPoints`, the vertical scaling
/// `volts = (raw - yoff) * ymult + yzero`, and the time base `t[i] = xzero + xincr * (i - ptOff)`.
fn parse_pwwf(b: &[u8]) -> Result<WaveTrace, String> {
    let unreadable = || "the waveform record header is not readable".to_string();
    let length = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
    let end = 12usize.checked_add(length).filter(|&end| end <= b.len()).ok_or("truncated waveform record")?;
    let members = std::str::from_utf8(&b[12..end]).ok().and_then(json_members).ok_or_else(unreadable)?;
    let get = |key: &str| members.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let number = |key: &str, default: f64| match get(key) {
        None | Some("null") => Ok(default),
        Some(v) => v.parse::<f64>().ok().filter(|x| x.is_finite()).ok_or_else(unreadable),
    };
    let n = get("nPoints").and_then(|v| v.parse::<usize>().ok()).ok_or_else(unreadable)?;
    if n > MAX_WAVEFORM_SAMPLES { return Err("waveform exceeds the 32-million-sample import limit".into()); }
    let dt = number("xincr", 0.0)?;
    let t0 = number("xzero", 0.0)? - number("ptOff", 0.0)? * dt;
    if n < 2 || dt <= 0.0 || !t0.is_finite() {
        return Err("that file holds no waveform".into());
    }
    let (yoff, ymult, yzero) = (number("yoff", 0.0)? as f32, number("ymult", 1.0)? as f32, number("yzero", 0.0)? as f32);
    let kind = get("sampleType").unwrap_or("");
    let width = match kind {
        "int8" => 1,
        "int16" | "uint16" => 2,
        "float32" => 4,
        _ => return Err(format!("waveform record sample type {kind:?} is not read")),
    };
    let data = &b[end..];
    if data.len() / width < n {
        return Err("truncated waveform record".into());
    }
    let scale = |raw: f32| (raw - yoff) * ymult + yzero;
    let v: Vec<f32> = match kind {
        "int8" => data[..n].iter().map(|&x| scale(x as i8 as f32)).collect(),
        "int16" => data[..2 * n].chunks_exact(2).map(|c| scale(i16::from_le_bytes([c[0], c[1]]) as f32)).collect(),
        "uint16" => data[..2 * n].chunks_exact(2).map(|c| scale(u16::from_le_bytes([c[0], c[1]]) as f32)).collect(),
        _ => data[..4 * n].chunks_exact(4).map(|c| scale(f32::from_le_bytes([c[0], c[1], c[2], c[3]]))).collect(),
    };
    if v.iter().any(|x| !x.is_finite()) {
        return Err("the waveform record holds samples that are not numbers".into());
    }
    let label = get("channel").map(str::trim).filter(|s| !s.is_empty()).unwrap_or("Record").chars().take(10).collect();
    Ok(WaveTrace { t0, dt, v, label })
}

/// The members of a JSON object, in order: each key with its value as text. A string value is unescaped; a
/// number, `true`, `false` or `null` is as written; a nested object or array is kept verbatim, so a key inside it
/// is never taken for a member. `None` when the text is not one object.
fn json_members(text: &str) -> Option<Vec<(String, String)>> {
    let b = text.as_bytes();
    let space = |mut i: usize| { while b.get(i).is_some_and(|c| c.is_ascii_whitespace()) { i += 1; } i };
    let mut i = space(0);
    if b.get(i) != Some(&b'{') { return None; }
    i += 1;
    let mut out = Vec::new();
    loop {
        i = space(i);
        match *b.get(i)? {
            b'}' => return space(i + 1).eq(&b.len()).then_some(out),
            b',' if !out.is_empty() => { i += 1; continue; }
            b'"' => {}
            _ => return None,
        }
        let (key, after) = json_string(text, i)?;
        i = space(after);
        if b.get(i) != Some(&b':') { return None; }
        i = space(i + 1);
        let value = if b.get(i) == Some(&b'"') {
            let (s, after) = json_string(text, i)?;
            i = after;
            s
        } else {
            let (start, mut depth) = (i, 0usize);
            loop {
                match *b.get(i)? {
                    b'"' => { i = json_string(text, i)?.1; continue; }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' if depth > 0 => depth -= 1,
                    b'}' | b',' if depth == 0 => break,
                    b']' if depth == 0 => return None,
                    _ => {}
                }
                i += 1;
            }
            let raw = text[start..i].trim();
            if raw.is_empty() { return None; }
            raw.to_string()
        };
        out.push((key, value));
    }
}

/// A JSON string starting at the quote at `at`: its text and the index after its closing quote.
fn json_string(text: &str, at: usize) -> Option<(String, usize)> {
    let mut out = String::new();
    let mut chars = text.get(at + 1..)?.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Some((out, at + 1 + i + 1)),
            '\\' => match chars.next()?.1 {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                'r' => out.push('\r'),
                'b' => out.push(char::from(8u8)),
                'f' => out.push(char::from(12u8)),
                'u' => {
                    let hex: String = (0..4).map(|_| chars.next().map(|(_, h)| h)).collect::<Option<String>>()?;
                    out.push(char::from_u32(u32::from_str_radix(&hex, 16).ok()?).unwrap_or(char::REPLACEMENT_CHARACTER));
                }
                other @ ('"' | '\\' | '/') => out.push(other),
                _ => return None,
            },
            c => out.push(c),
        }
    }
    None
}

fn looks_wfm(bytes: &[u8]) -> bool {
    bytes.len() >= 10 && (bytes[2..].starts_with(b":WFM#") || bytes[2..].starts_with(b"WFM#") || bytes.starts_with(b":WFM#"))
}

fn parse_oosw(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    if b.len() < 36 || &b[..4] != b"OOSW" {
        return Err("not an OOS waveform".into());
    }
    let ver = u32::from_le_bytes(b[4..8].try_into().unwrap());
    if ver != 1 {
        return Err(format!("OOS waveform version {ver}"));
    }
    let nch = u32::from_le_bytes(b[8..12].try_into().unwrap()) as usize;
    let n = usize::try_from(u64::from_le_bytes(b[12..20].try_into().unwrap())).map_err(|_| "OOS waveform is too large".to_string())?;
    let dt = f64::from_le_bytes(b[20..28].try_into().unwrap());
    let t0 = f64::from_le_bytes(b[28..36].try_into().unwrap());
    if n > MAX_WAVEFORM_SAMPLES { return Err("waveform exceeds the 32-million-sample import limit".into()); }
    if nch == 0 || nch > 8 || n < 2 || !dt.is_finite() || dt <= 0.0 || !t0.is_finite() {
        return Err("that file holds no waveform".into());
    }
    let need = n.checked_mul(4).and_then(|bytes| bytes.checked_add(4)).and_then(|bytes| bytes.checked_mul(nch)).and_then(|bytes| bytes.checked_add(36)).ok_or("OOS waveform is too large")?;
    if b.len() < need {
        return Err("truncated OOS waveform".into());
    }
    let mut out = Vec::with_capacity(nch);
    let mut p = 36;
    for _ in 0..nch {
        let ch = u32::from_le_bytes(b[p..p + 4].try_into().unwrap());
        p += 4;
        let mut v = Vec::with_capacity(n);
        for _ in 0..n {
            v.push(f32::from_le_bytes(b[p..p + 4].try_into().unwrap()));
            p += 4;
        }
        out.push(WaveTrace { t0, dt, v, label: format!("CH{ch}") });
    }
    Ok(out)
}

/// A time column, then one trace per further column. The columns are kept as the samples they become, and a line wider
/// than the reference slots ends the parse: a single line of many columns no longer makes as many traces.
fn parse_wave_csv(text: &str, name: &str) -> Result<Vec<WaveTrace>, String> {
    let mut columns: Vec<Vec<f32>> = Vec::new();
    // the first time and the last: the time between samples is their distance over the steps between them
    let mut times: [f64; 2] = [f64::NAN; 2];
    let mut rows = 0usize;
    for line in text.lines() {
        // a line that ends with its separator ("x, y,") has no column after it
        let mut cols = line.trim_end().trim_end_matches(',').split(',').map(|s| s.trim().parse::<f64>().ok());
        let Some(Some(t)) = cols.next() else { continue };
        let mut rest = cols.map(|x| x.unwrap_or(f64::NAN)).peekable();
        if rest.peek().is_none() {
            continue;
        }
        if rows >= MAX_WAVEFORM_SAMPLES { return Err("waveform exceeds the 32-million-sample import limit".into()); }
        let mut k = 0;
        for x in rest {
            if k >= NREF {
                return Err("The file has more traces than the remaining reference slots".into());
            }
            if k == columns.len() {
                // a column that starts on a later line: the lines before it have no sample there
                columns.push(vec![f32::NAN; rows]);
            }
            columns[k].push(x as f32);
            k += 1;
        }
        for c in &mut columns[k..] {
            c.push(f32::NAN);
        }
        times[(rows > 0) as usize] = t;
        rows += 1;
    }
    // not the first step alone: a file that rounds its times to a few digits has a first step that is off by
    // a percent, or no step at all between its first two lines
    let dt = match times {
        [a, b] if rows >= 2 && b > a => (b - a) / (rows - 1) as f64,
        _ => return Err("that file holds no waveform".into()),
    };
    let ntrace = columns.len();
    Ok(columns.into_iter().enumerate().map(|(k, v)| {
        let label = if ntrace == 1 { name.into() } else { format!("CH{}", k + 1) };
        WaveTrace { t0: times[0], dt, v, label }
    }).collect())
}

/// Tek reference waveform version 001/002/003, YT vector, first frame.
/// Version 001 uses an 88-byte reference block, 156-byte explicit dimensions,
/// and 132-byte implicit dimensions. Later versions use 90/160/136 bytes.
/// See Tektronix Reference Waveform File Format (077-0220-10/11).
fn parse_wfm(b: &[u8]) -> Result<WaveTrace, String> {
    if b.len() < 78 {
        return Err("truncated WFM".into());
    }
    let le = match u16::from_le_bytes(b[0..2].try_into().unwrap()) {
        0x0F0F => true,
        0xF0F0 => false,
        _ => return Err("header is not a Tek WFM version 001, 002 or 003".into()),
    };
    let get_u32 = |o| {
        let v = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        if le { v } else { v.swap_bytes() }
    };
    let get_i32 = |o| get_u32(o) as i32;
    let get_f64 = |o| {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(&b[o..o + 8]);
        if !le {
            raw.reverse();
        }
        f64::from_le_bytes(raw)
    };
    let ver = &b[2..10];
    if ver != b":WFM#001" && ver != b":WFM#002" && ver != b":WFM#003" {
        return Err("header is not a Tek WFM version 001, 002 or 003".into());
    }
    let bpp = b[15] as usize;
    if bpp == 0 || bpp > 8 {
        return Err("WFM sample size".into());
    }
    let curve = usize::try_from(get_i32(16)).map_err(|_| "WFM curve offset is negative")?;
    let label = b[40..72].iter().take_while(|&&c| c != 0).map(|&c| c as char).collect::<String>().trim().to_string();
    let (reference_size, explicit_size, implicit_size) = if ver == b":WFM#001" { (88, 156, 132) } else { (90, 160, 136) };
    // Packed header follows the common 78-byte static block.
    let mut i = 78usize;
    let need = |n: usize| -> Result<(), String> { if b.len() < n { Err("truncated WFM".into()) } else { Ok(()) } };
    need(i + reference_size)?;
    i += reference_size;
    need(i + explicit_size)?;
    let vscale = get_f64(i);
    let voff = get_f64(i + 8);
    let fmt = get_u32(i + 72); // after scale, offset, size, units, 4 doubles
    i += explicit_size;
    need(i + explicit_size)?;
    i += explicit_size;
    need(i + implicit_size)?;
    let dt = get_f64(i);
    let t0 = get_f64(i + 8);
    i += implicit_size;
    need(i + implicit_size + 12 + 12 + 24 + 30)?;
    i += implicit_size + 12 + 12 + 24;
    let data_start = get_u32(i + 14) as usize;
    let post_start = get_u32(i + 18) as usize;
    if !dt.is_finite() || dt <= 0.0 || !t0.is_finite() || !vscale.is_finite() || !voff.is_finite() {
        return Err("WFM time and voltage scaling must be finite with a positive time step".into());
    }
    if curve < i + 30 || data_start > post_start {
        return Err("WFM curve is empty".into());
    }
    let expected_bpp = match fmt { 0 => 2, 1 | 2 | 4 => 4, 3 | 5 => 8, 6 | 7 => 1, _ => return Err("WFM sample format".into()) };
    if bpp != expected_bpp { return Err("WFM sample format does not match bytes per point".into()); }
    let start = curve.checked_add(data_start).ok_or("WFM curve offset overflow")?;
    let stop = curve.checked_add(post_start).ok_or("WFM curve offset overflow")?;
    if stop > b.len().saturating_sub(8) { return Err("truncated WFM curve".into()); }
    if stop <= start || (stop - start) % bpp != 0 {
        return Err("WFM curve is empty".into());
    }
    let n = (stop - start) / bpp;
    if n < 2 {
        return Err("WFM curve is empty".into());
    }
    if n > MAX_WAVEFORM_SAMPLES { return Err("waveform exceeds the 32-million-sample import limit".into()); }
    let mut v = Vec::with_capacity(n);
    for k in 0..n {
        let o = start + k * bpp;
        let code = match fmt {
            0 if bpp >= 2 => {
                let s = i16::from_le_bytes(b[o..o + 2].try_into().unwrap());
                (if le { s } else { s.swap_bytes() }) as f64
            }
            1 if bpp >= 4 => {
                let s = i32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                (if le { s } else { s.swap_bytes() }) as f64
            }
            2 => get_u32(o) as f64,
            3 => {
                let mut raw: [u8; 8] = b[o..o + 8].try_into().unwrap();
                if !le { raw.reverse(); }
                u64::from_le_bytes(raw) as f64
            }
            4 if bpp >= 4 => {
                let mut raw = [0u8; 4];
                raw.copy_from_slice(&b[o..o + 4]);
                if !le {
                    raw.reverse();
                }
                f32::from_le_bytes(raw) as f64
            }
            5 if bpp >= 8 => get_f64(o),
            6 if bpp >= 1 => b[o] as f64,
            7 if bpp >= 1 => b[o] as i8 as f64,
            _ => return Err("WFM sample format".into()),
        };
        v.push((code * vscale + voff) as f32);
    }
    Ok(WaveTrace { t0, dt, v, label })
}

fn looks_isf(b: &[u8]) -> bool {
    let s = if b.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    b.get(s..).is_some_and(|x| x.starts_with(b":WFMPRE:") || x.starts_with(b"WFMPRE:"))
}

/// Preamble `:WFMPRE:` and a `:CURVE` block. `Y = (code - YOFF) * YMULT + YZERO`, `t = XZERO + (i - PT_OFF) * XINCR`.
fn parse_isf(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let s = if b.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let curve = b[s..].windows(6).position(|w| w == b":CURVE").map(|p| s + p).ok_or("that file holds no waveform")?;
    let mut kv = std::collections::HashMap::<String, String>::new();
    let pre = String::from_utf8_lossy(&b[s..curve]);
    for part in pre.split(|c| c == ';' || c == '\n') {
        let part = part.trim().trim_start_matches(':');
        let part = part.split_once(':').map(|(_, r)| r.trim()).unwrap_or(part);
        let mut it = part.split_whitespace();
        let Some(key) = it.next() else { continue };
        let val = it.next().unwrap_or("").trim_matches('"').to_string();
        kv.insert(key.to_ascii_uppercase(), val);
    }
    let num = |k: &str| kv.get(k).and_then(|s| s.parse::<f64>().ok());
    let numeric = |k: &str, default: f64| -> Result<f64, String> {
        match kv.get(k) {
            Some(value) => value.parse().map_err(|_| format!("invalid waveform {k}")),
            None => Ok(default),
        }
    };
    let bpp = numeric("BYT_NR", 1.0)?;
    if !matches!(bpp, 1.0 | 2.0 | 4.0) {
        return Err("that file holds no waveform".into());
    }
    let bpp = bpp as usize;
    let dt = num("XINCR").filter(|x| x.is_finite() && *x > 0.0).ok_or("that file holds no waveform")?;
    let xzero = numeric("XZERO", 0.0)?;
    let pt_off = numeric("PT_OFF", 0.0)?;
    let ymult = numeric("YMULT", 1.0)?;
    let yoff = numeric("YOFF", 0.0)?;
    let yzero = numeric("YZERO", 0.0)?;
    if ![ymult, yoff, yzero, xzero, pt_off].iter().all(|x| x.is_finite()) {
        return Err("that file holds no waveform".into());
    }
    let nr = match kv.get("NR_PT") {
        None => 0,
        Some(value) => value.parse::<usize>().ok().filter(|n| (2..=MAX_WAVEFORM_SAMPLES).contains(n))
            .ok_or("invalid waveform point count")?,
    };
    let msb = kv.get("BYT_OR").is_some_and(|s| s.eq_ignore_ascii_case("MSB"));
    let signed = !kv.get("BN_FMT").is_some_and(|s| s.eq_ignore_ascii_case("RP"));
    let ascii = kv.get("ENCDG").is_some_and(|s| s.eq_ignore_ascii_case("ASC"));
    let mut i = curve + 6;
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let (raw, vals) = if b.get(i) == Some(&b'#') {
        i += 1;
        let n = b.get(i).filter(|c| c.is_ascii_digit()).map(|c| (c - b'0') as usize)
            .ok_or("invalid waveform block length")?;
        if !(1..=9).contains(&n) || i + 1 + n > b.len() {
            return Err("truncated waveform".into());
        }
        let len: usize = std::str::from_utf8(&b[i + 1..i + 1 + n]).ok().and_then(|s| s.parse().ok()).ok_or("truncated waveform")?;
        let start = i + 1 + n;
        if len > MAX_WAVEFORM_BYTES || start.checked_add(len).is_none_or(|end| end > b.len()) {
            return Err(if len > MAX_WAVEFORM_BYTES { "waveform is too large" } else { "truncated waveform" }.into());
        }
        (start, len)
    } else {
        (i, b.len() - i)
    };
    let body = &b[raw..raw + vals];
    let mut codes = Vec::new();
    if ascii {
        let text = String::from_utf8_lossy(body);
        for tok in text.split(|c: char| c == ',' || c.is_whitespace()) {
            if let Ok(x) = tok.parse::<f64>() {
                codes.push(x);
            }
        }
    } else {
        if vals % bpp != 0 {
            return Err("truncated waveform".into());
        }
        let n = vals / bpp;
        if n > MAX_WAVEFORM_SAMPLES {
            return Err("waveform is too large".into());
        }
        for k in 0..n {
            let o = raw + k * bpp;
            let code = match bpp {
                1 => if signed { b[o] as i8 as f64 } else { b[o] as f64 },
                2 => {
                    let u = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
                    let u = if msb { u.swap_bytes() } else { u };
                    if signed { u as i16 as f64 } else { u as f64 }
                }
                _ => {
                    let u = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
                    let u = if msb { u.swap_bytes() } else { u };
                    if signed { u as i32 as f64 } else { u as f64 }
                }
            };
            codes.push(code);
        }
    }
    let env = kv.get("PT_FMT").is_some_and(|s| s.eq_ignore_ascii_case("ENV"));
    if env && nr == 0 { return Err("an envelope needs a waveform point count".into()); }
    let needed = if env { nr * 2 } else { nr };
    if codes.len() < needed { return Err("truncated waveform".into()); }
    let t0 = xzero - pt_off * dt;
    let scale = |code: f64| ((code - yoff) * ymult + yzero) as f32;
    if env {
        let min: Vec<f32> = codes.iter().take(nr * 2).step_by(2).copied().map(scale).collect();
        let max: Vec<f32> = codes.iter().skip(1).take(nr * 2).step_by(2).copied().map(scale).collect();
        return Ok(vec![WaveTrace { t0, dt, v: min, label: "min".into() }, WaveTrace { t0, dt, v: max, label: "max".into() }]);
    }
    if nr >= 2 && codes.len() > nr {
        codes.truncate(nr);
    }
    if codes.len() < 2 {
        return Err("that file holds no waveform".into());
    }
    Ok(vec![WaveTrace { t0, dt, v: codes.into_iter().map(scale).collect(), label: kv.get("WFID").cloned().unwrap_or_default() }])
}

fn looks_wavedesc(b: &[u8]) -> bool {
    wavedesc_at(b).is_some_and(|i| b.len() >= i + 188 && matches!((b[i + 34], b[i + 35]), (1, 0) | (0, 0)))
}

fn wavedesc_at(b: &[u8]) -> Option<usize> {
    let n = b.len().min(64);
    b[..n].windows(8).position(|w| w == b"WAVEDESC")
}

fn endian_u16(b: &[u8], o: usize, le: bool) -> u16 {
    let u = u16::from_le_bytes(b[o..o + 2].try_into().unwrap());
    if le { u } else { u.swap_bytes() }
}

fn endian_u32(b: &[u8], o: usize, le: bool) -> u32 {
    let u = u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
    if le { u } else { u.swap_bytes() }
}

fn endian_f32(b: &[u8], o: usize, le: bool) -> f32 {
    let mut raw = [0u8; 4];
    raw.copy_from_slice(&b[o..o + 4]);
    if !le {
        raw.reverse();
    }
    f32::from_le_bytes(raw)
}

fn endian_f64(b: &[u8], o: usize, le: bool) -> f64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&b[o..o + 8]);
    if !le {
        raw.reverse();
    }
    f64::from_le_bytes(raw)
}

/// `WAVEDESC` block. `volts = code * gain - offset`, time of point 0 is the double at offset 180.
fn parse_wavedesc(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let i = wavedesc_at(b).ok_or("that file is not a waveform")?;
    if b.len() < i + 188 {
        return Err("truncated waveform".into());
    }
    let le = match (b[i + 34], b[i + 35]) {
        (1, 0) => true,
        (0, 0) => false,
        _ => return Err("that file is not a waveform".into()),
    };
    let desc = endian_u32(b, i + 36, le) as usize;
    let user = endian_u32(b, i + 40, le) as usize;
    let trig = endian_u32(b, i + 48, le) as usize;
    let ris = endian_u32(b, i + 52, le) as usize;
    let nbytes = endian_u32(b, i + 60, le) as usize;
    let count = endian_u32(b, i + 116, le) as usize;
    if desc < 188 || desc > 8192 || count < 2 || count > MAX_WAVEFORM_SAMPLES {
        return Err("that file holds no waveform".into());
    }
    let bpp = if count > 0 && nbytes % count == 0 && matches!(nbytes / count, 1 | 2 | 4) {
        nbytes / count
    } else {
        match endian_u16(b, i + 32, le) {
            0 => 1,
            1 => 2,
            _ => return Err("that file holds no waveform".into()),
        }
    };
    let gain = endian_f32(b, i + 156, le) as f64;
    let off = endian_f32(b, i + 160, le) as f64;
    let dt = endian_f32(b, i + 176, le) as f64;
    let t0 = endian_f64(b, i + 180, le);
    if !(dt.is_finite() && dt > 0.0 && gain.is_finite() && off.is_finite() && t0.is_finite()) {
        return Err("that file holds no waveform".into());
    }
    let start = i.checked_add(desc).and_then(|x| x.checked_add(user)).and_then(|x| x.checked_add(trig)).and_then(|x| x.checked_add(ris)).ok_or("waveform is too large")?;
    let need = count.checked_mul(bpp).ok_or("waveform is too large")?;
    if start.checked_add(need).is_none_or(|end| end > b.len()) {
        return Err("truncated waveform".into());
    }
    let label = cstr(&b[i + 96..i + 112]);
    let mut v = Vec::with_capacity(count);
    for k in 0..count {
        let o = start + k * bpp;
        let code = match bpp {
            1 => b[o] as i8 as f64,
            2 => endian_u16(b, o, le) as i16 as f64,
            _ => endian_f32(b, o, le) as f64,
        };
        v.push((code * gain - off) as f32);
    }
    Ok(vec![WaveTrace { t0, dt, v, label }])
}

/// The two makers of the binary waveform file: the same waveform header, and sizes of two widths.
/// `AG`: a 12-byte file header (cookie, version, u32 file size, u32 waveforms) and a 12-byte buffer header.
/// `RG`: a 16-byte file header (u64 file size) and a 16-byte buffer header (u64 buffer size).
#[derive(Clone, Copy, PartialEq)]
enum BinMaker { Ag, Rg }

impl BinMaker {
    fn of(b: &[u8]) -> Option<Self> {
        match b.get(..2)? {
            b"AG" => Some(Self::Ag),
            b"RG" => Some(Self::Rg),
            _ => None,
        }
    }

    /// Length of the file header; the waveform count is its last four bytes.
    fn file_header(self) -> usize {
        if self == Self::Rg { 16 } else { 12 }
    }
}

fn looks_ag(b: &[u8]) -> bool {
    let Some(maker) = BinMaker::of(b) else { return false };
    let first = maker.file_header();
    if b.len() < first + 140 || !b[2].is_ascii_graphic() || !b[3].is_ascii_graphic() {
        return false;
    }
    let nwave = u32::from_le_bytes(b[first - 4..first].try_into().unwrap());
    let hsize = u32::from_le_bytes(b[first..first + 4].try_into().unwrap());
    (1..=64).contains(&nwave) && (140..8192).contains(&hsize)
}

/// Cookie `AG` or `RG`, then one header per waveform and a buffer of float32 samples. Little endian.
/// A logic waveform (type 6, or a buffer of 8-bit digital samples) is not a trace and is left out.
///
/// The start of an `RG` record: its manual calls X Origin the x value of the first point, as in an `AG` file,
/// but files saved by the instruments hold the distance before the trigger there, a positive number (the
/// RigolWFM project found this on DHO800 captures). So the first point of an `RG` record is at `-X Origin`.
fn parse_ag(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    if !looks_ag(b) {
        return Err("that file is not a waveform".into());
    }
    let maker = BinMaker::of(b).unwrap();
    let wide = maker == BinMaker::Rg;
    let mut p = maker.file_header();
    let nwave = u32::from_le_bytes(b[p - 4..p].try_into().unwrap()) as usize;
    let mut logic = 0usize;
    let mut out = Vec::with_capacity(nwave);
    for wi in 0..nwave {
        if p + 140 > b.len() {
            return Err("truncated waveform".into());
        }
        let hsize = u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize;
        if !(140..8192).contains(&hsize) || p + hsize > b.len() {
            return Err("truncated waveform".into());
        }
        let wave_type = u32::from_le_bytes(b[p + 4..p + 8].try_into().unwrap());
        let nbuf = u32::from_le_bytes(b[p + 8..p + 12].try_into().unwrap()) as usize;
        let npts = u32::from_le_bytes(b[p + 12..p + 16].try_into().unwrap()) as usize;
        let dt = f64::from_le_bytes(b[p + 32..p + 40].try_into().unwrap());
        let origin = f64::from_le_bytes(b[p + 40..p + 48].try_into().unwrap());
        let t0 = if wide { -origin } else { origin };
        let label = cstr(&b[p + 112..p + 128]);
        if !(1..=32).contains(&nbuf) || !(dt.is_finite() && dt > 0.0 && t0.is_finite()) {
            return Err("that file holds no waveform".into());
        }
        p += hsize;
        let mut normal: Option<Vec<f32>> = None;
        let mut other: Option<Vec<f32>> = None;
        let mut digital = wave_type == 6;
        let buffer_header = if wide { 16 } else { 12 };
        for _ in 0..nbuf {
            if p + buffer_header > b.len() {
                return Err("truncated waveform".into());
            }
            let dsize = u32::from_le_bytes(b[p..p + 4].try_into().unwrap()) as usize;
            let kind = u16::from_le_bytes(b[p + 4..p + 6].try_into().unwrap());
            let bpp = u16::from_le_bytes(b[p + 6..p + 8].try_into().unwrap()) as usize;
            let bsize = if wide {
                usize::try_from(u64::from_le_bytes(b[p + 8..p + 16].try_into().unwrap())).unwrap_or(usize::MAX)
            } else {
                u32::from_le_bytes(b[p + 8..p + 12].try_into().unwrap()) as usize
            };
            if dsize < buffer_header || p.checked_add(dsize).and_then(|data| data.checked_add(bsize)).is_none_or(|end| end > b.len()) {
                return Err("truncated waveform".into());
            }
            // 8-bit digital samples: buffer type 6 in an AG file, 5 in an RG file (an AG file has counts there)
            digital |= kind == if wide { 5 } else { 6 };
            let data = p + dsize;
            if matches!(kind, 1 | 2 | 3) && bpp == 4 && bsize >= 8 && bsize % 4 == 0 {
                let n = (bsize / 4).min(if npts >= 2 { npts } else { bsize / 4 });
                if n >= 2 && n <= MAX_WAVEFORM_SAMPLES {
                    let v: Vec<f32> = (0..n).map(|k| f32::from_le_bytes(b[data + k * 4..data + k * 4 + 4].try_into().unwrap())).collect();
                    if kind == 1 {
                        normal = Some(v);
                    } else if other.is_none() {
                        other = Some(v);
                    }
                }
            }
            p = data + bsize;
        }
        let Some(v) = normal.or(other) else {
            if digital {
                logic += 1;
                continue;
            }
            return Err("that file holds no waveform".into());
        };
        let label = if label.is_empty() { format!("CH{}", wi + 1) } else { label };
        out.push(WaveTrace { t0, dt, v, label });
    }
    if out.is_empty() {
        return Err(if logic > 0 { "that file holds logic channels only, and they are not read" } else { "that file holds no waveform" }.into());
    }
    Ok(out)
}

fn block_le(head: &[u8]) -> Option<bool> {
    if head.len() < 16 {
        return None;
    }
    let mark = |o: usize| match &head[o..o + 4] {
        b"\x04\x00\x00\x00" => Some(true),
        b"\x00\x00\x00\x04" => Some(false),
        _ => None,
    };
    mark(0).or_else(|| mark(8))
}

fn looks_raw(b: &[u8]) -> bool {
    b.starts_with(b"Title:") && (b.windows(7).any(|w| w == b"Binary:") || b.windows(7).any(|w| w == b"Values:")) && b.windows(9).any(|w| w.eq_ignore_ascii_case(b"Plotname:"))
}

fn looks_tr0(b: &[u8]) -> bool {
    if looks_raw(b) {
        return true;
    }
    let head = &b[..b.len().min(8192)];
    let marked = head.windows(4).any(|w| w == b"$&%#");
    let version = head.windows(4).any(|w| matches!(w, b"9601" | b"2001" | b"9007"));
    if !(marked && version) {
        return false;
    }
    block_le(b).is_some() || b.first().is_some_and(u8::is_ascii_digit)
}

/// `$&%#` post file (`9601` / `9007` float32, `2001` float64) or a `Title:` transient with `Binary:` or `Values:`.
fn parse_tr0(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    if block_le(b).is_some() {
        return parse_tr0_blocks(b);
    }
    if looks_raw(b) {
        return parse_raw(b);
    }
    parse_tr0_ascii(b)
}

fn post_width(text: &str) -> Result<usize, String> {
    if text.contains("2001") {
        Ok(8)
    } else if text.contains("9601") || text.contains("9007") {
        Ok(4)
    } else {
        Err("that file is not a waveform".into())
    }
}

fn post_vars(text: &str) -> Result<(usize, Vec<String>), String> {
    let end = text.find("$&%#").ok_or("that file is not a waveform")?;
    // Name fields are 16 bytes and may end in spaces, so only a trailing newline is dropped.
    let head = text[..end].trim_end_matches(['\r', '\n', '\0']);
    let digits: String = head.chars().skip_while(|c| c.is_whitespace()).take(4).collect();
    if digits.len() != 4 || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err("that file is not a waveform".into());
    }
    let nvars: usize = digits.parse().unwrap_or(0);
    if !(2..=64).contains(&nvars) {
        return Err("that file holds no waveform".into());
    }
    let names = if head.len() >= nvars * 16 {
        head.as_bytes()[head.len() - nvars * 16..].chunks(16).map(|c| String::from_utf8_lossy(c).trim().to_string()).collect()
    } else {
        (0..nvars).map(|k| if k == 0 { "time".into() } else { format!("CH{k}") }).collect()
    };
    Ok((nvars, names))
}

fn traces_from_columns(cols: &[f64], nvars: usize, names: &[String], time_f32: bool) -> Result<Vec<WaveTrace>, String> {
    if !(2..=64).contains(&nvars) { return Err("invalid waveform column count".into()); }
    let mut n = cols.len();
    if n > 0 && cols[n - 1].abs() >= 1e29 {
        n -= 1;
    }
    let body = if n >= nvars && n % nvars == 0 {
        &cols[..n]
    } else if n > nvars && (n - 1) % nvars == 0 {
        &cols[1..n]
    } else {
        return Err("that file holds no waveform".into());
    };
    let rows = body.len() / nvars;
    if rows < 2 || rows > MAX_WAVEFORM_SAMPLES {
        return Err(if rows < 2 { "that file holds no waveform" } else { "waveform is too large" }.into());
    }
    let t0 = body[0];
    let dt = if time_f32 { (body[(rows - 1) * nvars] - t0) / (rows - 1) as f64 } else { body[nvars] - t0 };
    if !(t0.is_finite() && dt.is_finite() && dt > 0.0) {
        return Err("that file holds no waveform".into());
    }
    let valid_time = |r: usize| {
        let t = body[r * nvars];
        if time_f32 {
            let expected = t0 + r as f64 * dt;
            let tolerance = (dt.abs() * 1e-5).max(t0.abs().max(expected.abs()) * f32::EPSILON as f64 * 2.0).max(1e-45);
            t.is_finite() && expected.is_finite() && (t - expected).abs() <= tolerance
        } else { uniform_time(t, t0, dt, r) }
    };
    if !(0..rows).all(valid_time) {
        return Err("nonuniform waveform time axis is not supported".into());
    }
    let mut out = Vec::with_capacity(nvars - 1);
    for k in 1..nvars {
        let v: Vec<f32> = (0..rows).map(|r| body[r * nvars + k] as f32).collect();
        let label = names.get(k).cloned().filter(|s| !s.is_empty()).unwrap_or_else(|| format!("CH{k}"));
        out.push(WaveTrace { t0, dt, v, label });
    }
    Ok(out)
}

fn parse_tr0_blocks(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let mut pos = 0usize;
    let mut header = String::new();
    let mut data = Vec::new();
    let mut first = true;
    while pos + 16 <= b.len() {
        let le = block_le(&b[pos..pos + 16]).ok_or("that file is not a waveform")?;
        let n = endian_u32(b, pos + 12, le) as usize;
        if n > MAX_WAVEFORM_BYTES || pos + 16 + n > b.len() {
            return Err(if n > MAX_WAVEFORM_BYTES { "waveform is too large" } else { "truncated waveform" }.into());
        }
        let body = &b[pos + 16..pos + 16 + n];
        pos += 16 + n;
        if pos + 4 <= b.len() {
            pos += 4;
        }
        if first {
            header = String::from_utf8_lossy(body).into_owned();
            first = false;
        } else {
            data.extend_from_slice(body);
        }
    }
    if header.find("$&%#").is_none() {
        return Err("that file is not a waveform".into());
    }
    let width = post_width(&header)?;
    let (nvars, names) = post_vars(&header)?;
    if data.len() % width != 0 {
        return Err("truncated waveform".into());
    }
    let mut cols = Vec::with_capacity(data.len() / width);
    for chunk in data.chunks(width) {
        let x = if width == 8 {
            f64::from_le_bytes(chunk.try_into().unwrap())
        } else {
            f32::from_le_bytes(chunk.try_into().unwrap()) as f64
        };
        if x.abs() >= 1e29 {
            break;
        }
        cols.push(x);
    }
    // The block endian applies to the floats as well. Re-read when the header was big endian.
    if block_le(b) == Some(false) {
        cols.clear();
        for chunk in data.chunks(width) {
            let x = if width == 8 { endian_f64(chunk, 0, false) } else { endian_f32(chunk, 0, false) as f64 };
            if x.abs() >= 1e29 {
                break;
            }
            cols.push(x);
        }
    }
    traces_from_columns(&cols, nvars, &names, width == 4)
}

fn parse_tr0_ascii(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let text = std::str::from_utf8(b).map_err(|_| "that file is not a waveform".to_string())?;
    let end = text.find("$&%#").ok_or("that file is not a waveform")?;
    let width_bytes = post_width(text)?;
    let (nvars, names) = post_vars(text)?;
    let rest = text[end + 4..].trim_start();
    let field = if width_bytes == 8 { 13 } else { 11 };
    let packed = rest.as_bytes().get(..field).and_then(|b| std::str::from_utf8(b).ok())
        .is_some_and(|s| !s.bytes().any(|b| b.is_ascii_whitespace()) && s.trim().parse::<f64>().is_ok());
    let mut cols = Vec::new();
    if packed {
        let bytes = rest.as_bytes();
        let mut i = 0;
        while i + field <= bytes.len() {
            let s = std::str::from_utf8(&bytes[i..i + field]).unwrap_or("").trim();
            if s.is_empty() {
                break;
            }
            let Ok(x) = s.parse::<f64>() else { break };
            if x.abs() >= 1e29 {
                break;
            }
            cols.push(x);
            i += field;
        }
    }
    if cols.len() < nvars * 2 {
        cols.clear();
        for tok in rest.split_whitespace() {
            let Ok(x) = tok.parse::<f64>() else { break };
            if x.abs() >= 1e29 {
                break;
            }
            cols.push(x);
        }
    }
    traces_from_columns(&cols, nvars, &names, false)
}

fn parse_raw(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let mark = b.windows(7).position(|w| w == b"Binary:" || w == b"Values:").ok_or("that file is not a waveform")?;
    let header = std::str::from_utf8(&b[..mark]).map_err(|_| "that file is not a waveform".to_string())?;
    let nvars = raw_field(header, "No. Variables:").ok_or("that file holds no waveform")?;
    let npts = raw_field(header, "No. Points:").unwrap_or(0);
    if !(2..=64).contains(&nvars) {
        return Err("that file holds no waveform".into());
    }
    let complex = header.to_ascii_lowercase().contains("flags: complex") || header.to_ascii_lowercase().contains("flags:complex");
    let names = raw_names(header, nvars);
    let binary = b[mark..].starts_with(b"Binary:");
    let mut cols = Vec::new();
    if binary {
        let mut i = mark + 7;
        if b.get(i) == Some(&b'\r') {
            i += 1;
        }
        if b.get(i) == Some(&b'\n') {
            i += 1;
        }
        let points = if npts >= 2 { npts } else { (b.len() - i) / (nvars * 8) };
        if points < 2 || points > MAX_WAVEFORM_SAMPLES {
            return Err(if points < 2 { "that file holds no waveform" } else { "waveform is too large" }.into());
        }
        if i + points * nvars * 8 > b.len() {
            return Err("truncated waveform".into());
        }
        for p in 0..points {
            for k in 0..nvars {
                let o = i + (p * nvars + k) * 8;
                let x = if complex { endian_f32(b, o, true) as f64 } else { endian_f64(b, o, true) };
                cols.push(x);
            }
        }
    } else {
        let text = std::str::from_utf8(&b[mark + 7..]).map_err(|_| "that file is not a waveform".to_string())?;
        let nums: Vec<f64> = text.split(|c: char| !(c.is_ascii_digit() || c == '-' || c == '+' || c == '.' || c == 'e' || c == 'E')).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect();
        let stride = nvars + 1;
        let indexed = nums.len() >= stride * 2 && nums[0].abs() < 1e-9 && (nums[stride] - 1.0).abs() < 1e-6;
        if indexed {
            let rows = nums.len() / stride;
            for r in 0..rows {
                cols.extend_from_slice(&nums[r * stride + 1..r * stride + 1 + nvars]);
            }
        } else if nums.len() >= nvars * 2 && nums.len() % nvars == 0 {
            cols = nums;
        } else {
            return Err("that file holds no waveform".into());
        }
    }
    traces_from_columns(&cols, nvars, &names, false)
}

fn raw_field(header: &str, key: &str) -> Option<usize> {
    let lower = header.to_ascii_lowercase();
    let i = lower.find(&key.to_ascii_lowercase())? + key.len();
    let rest = header.get(i..)?.trim_start();
    let n: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    n.parse().ok()
}

fn raw_names(header: &str, nvars: usize) -> Vec<String> {
    let mut names = vec![String::new(); nvars];
    let Some(at) = header.to_ascii_lowercase().find("variables:") else { return names };
    for line in header[at + 9..].lines() {
        let mut it = line.split_whitespace();
        let Some(idx) = it.next().and_then(|s| s.parse::<usize>().ok()) else { continue };
        if let Some(name) = it.next() {
            if idx < nvars {
                names[idx] = name.to_string();
            }
        }
    }
    names
}

fn looks_wfmx(b: &[u8]) -> bool {
    let s = if b.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let rest = b.get(s..).unwrap_or(b);
    if !rest.starts_with(b"<?xml") {
        return false;
    }
    let n = rest.len().min(8192);
    let t = &rest[..n];
    let has = |needle: &[u8]| t.windows(needle.len()).any(|w| w == needle);
    has(b"WaveformPatternInternal") || has(b"WaveformInternal")
}

fn xml_text<'a>(hay: &'a str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let i = hay.find(&open)? + open.len();
    let j = hay[i..].find(&close)? + i;
    Some(hay[i..j].trim())
}

/// XML header `WaveformInternal` or `WaveformPatternInternal`, then float samples. `XOrg` is the first time, `XInc` the step.
fn parse_wfmx(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let s = if b.starts_with(&[0xEF, 0xBB, 0xBF]) { 3 } else { 0 };
    let head_end = b[s..].windows(7).position(|w| w == b"</Data>").map(|p| s + p + 7).unwrap_or(b.len().min(s + 8192));
    let head = String::from_utf8_lossy(&b[s..head_end.min(b.len())]);
    if !(head.contains("WaveformInternal") || head.contains("WaveformPatternInternal")) {
        return Err("that file is not a waveform".into());
    }
    let points: usize = xml_text(&head, "Points").and_then(|s| s.parse().ok()).ok_or("that file holds no waveform")?;
    let t0: f64 = xml_text(&head, "XOrg").and_then(|s| s.parse().ok()).ok_or("that file holds no waveform")?;
    let dt: f64 = xml_text(&head, "XInc").and_then(|s| s.parse().ok()).ok_or("that file holds no waveform")?;
    if points < 2 || points > MAX_WAVEFORM_SAMPLES || !(dt.is_finite() && dt > 0.0 && t0.is_finite()) {
        return Err("that file holds no waveform".into());
    }
    let payload = wfmx_payload(b, points)?;
    let v = wfmx_volts(payload, points, t0, dt)?;
    let label = xml_text(&head, "SourceName").unwrap_or("").to_string();
    Ok(vec![WaveTrace { t0, dt, v, label }])
}

fn wfmx_payload(b: &[u8], points: usize) -> Result<&[u8], String> {
    let lengths = [4usize, 8, 16].map(|width| points.checked_mul(width).ok_or("waveform is too large"));
    let [four, eight, sixteen] = lengths;
    let lengths = [four?, eight?, sixteen?];
    let fits = |p: &[u8]| lengths.contains(&p.len());
    if let Some(start) = b.windows(5).enumerate().find_map(|(i, w)|
        (w == b"<Data" && b.get(i + 5).is_some_and(|c| *c == b'>' || c.is_ascii_whitespace())).then_some(i)) {
        if let Some(gt) = b[start..].iter().position(|&c| c == b'>') {
            let from = start + gt + 1;
            for length in lengths {
                if let Some(end) = from.checked_add(length) {
                    if b.get(end..).is_some_and(|tail| tail.starts_with(b"</Data>")) {
                        return Ok(&b[from..end]);
                    }
                }
            }
        }
    }
    // Samples follow the header. An empty `<Data>` element means they start after
    // the closing root tag. Consume only the XML separator required by the exact
    // payload size; leading/trailing float bytes can themselves be whitespace.
    if let Some(i) = b.windows(7).position(|w| w == b"</Data>") {
        let mut p = i + 7;
        let mut root = p;
        while b.get(root).is_some_and(u8::is_ascii_whitespace) {
            root += 1;
        }
        if b.get(root) == Some(&b'<') && b.get(root + 1) == Some(&b'/') {
            if let Some(gt) = b[root..].iter().position(|&c| c == b'>') {
                p = root + gt + 1;
            }
        }
        let tail = &b[p..];
        // A single XML line ending is supported after the root. Never trim the
        // binary tail or skip an unbounded run of its possible first bytes. A
        // present separator is structural, never a replacement for missing data.
        for separator in [b"\r\n".as_slice(), b"\n".as_slice()] {
            if let Some(payload) = tail.strip_prefix(separator) {
                return if fits(payload) { Ok(payload) } else { Err("truncated or unsupported waveform payload".into()) };
            }
        }
        if fits(tail) {
            return Ok(tail);
        }
    }
    Err("that file holds no waveform".into())
}

fn wfmx_volts(payload: &[u8], points: usize, t0: f64, dt: f64) -> Result<Vec<f32>, String> {
    let le_f32 = |p: &[u8], be: bool| -> Vec<f32> { (0..p.len() / 4).map(|k| endian_f32(p, k * 4, !be)).collect() };
    let as_y = |ys: &[f32]| ys.iter().take(points).copied().collect::<Vec<_>>();
    if payload.len() == points * 4 {
        let v = le_f32(payload, false);
        if v.iter().filter(|x| x.is_finite()).count() * 2 >= v.len() {
            return Ok(v);
        }
        return Ok(le_f32(payload, true));
    }
    if payload.len() == points * 8 {
        let f32s = le_f32(payload, false);
        let xs: Vec<f64> = f32s[..points].iter().map(|x| *x as f64).collect();
        if x_matches_f32(&xs, t0, dt) {
            return Ok(as_y(&f32s[points..]));
        }
        let inter: Vec<f64> = f32s.iter().step_by(2).copied().map(|x| x as f64).collect();
        if x_matches_f32(&inter, t0, dt) {
            return Ok(f32s.iter().skip(1).step_by(2).copied().collect());
        }
        // Eight bytes per point could also be an implicit f64 Y array. Without
        // a declared layout it cannot be distinguished safely from malformed
        // f32 X/Y data. Never reinterpret a failed explicit time axis as volts.
        return Err("unsupported or ambiguous WFMX sample layout".into());
    }
    if payload.len() == points * 16 {
        let xs: Vec<f64> = (0..points).map(|k| endian_f64(payload, k * 8, true)).collect();
        if x_matches(&xs, t0, dt) {
            return Ok((0..points).map(|k| endian_f64(payload, (points + k) * 8, true) as f32).collect());
        }
        let inter: Vec<f64> = (0..points).map(|k| endian_f64(payload, k * 16, true)).collect();
        if x_matches(&inter, t0, dt) {
            return Ok((0..points).map(|k| endian_f64(payload, k * 16 + 8, true) as f32).collect());
        }
    }
    Err("that file holds no waveform".into())
}

fn x_matches_f32(xs: &[f64], t0: f64, dt: f64) -> bool {
    xs.len() >= 2 && xs.iter().enumerate().all(|(i, &x)| {
        let expected = t0 + i as f64 * dt;
        let tolerance = (dt.abs() * 1e-5).max(expected.abs() * f32::EPSILON as f64).max(1e-45);
        x.is_finite() && expected.is_finite() && (x - expected).abs() <= tolerance
    })
}

fn x_matches(xs: &[f64], t0: f64, dt: f64) -> bool {
    xs.len() >= 2 && xs.iter().enumerate().all(|(i, &x)| uniform_time(x, t0, dt, i))
}

/// A WaveTrace has one interval; adaptive/invalid time axes cannot be relabeled
/// uniform without moving their samples. Tolerance covers source float rounding.
fn uniform_time(time: f64, t0: f64, dt: f64, index: usize) -> bool {
    let expected = t0 + index as f64 * dt;
    let tolerance = (dt.abs() * 1e-5).max(expected.abs() * f64::EPSILON * 8.0).max(1e-18);
    time.is_finite() && expected.is_finite() && (time - expected).abs() <= tolerance
}

fn cstr(b: &[u8]) -> String {
    b.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect::<String>().trim().to_string()
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)?.try_into().ok().map(u32::from_le_bytes)
}

fn u64_le(b: &[u8], at: usize) -> Option<u64> {
    b.get(at..at + 8)?.try_into().ok().map(u64::from_le_bytes)
}

fn f64_le(b: &[u8], at: usize) -> Option<f64> {
    b.get(at..at + 8)?.try_into().ok().map(f64::from_le_bytes)
}

fn f32_le(b: &[u8], at: usize) -> Option<f32> {
    b.get(at..at + 4)?.try_into().ok().map(f32::from_le_bytes)
}

/// A reference slot as a session keeps it: the samples and their time, not the look (the setup text has that).
#[derive(Clone, Debug, PartialEq)]
pub struct SessionRef {
    pub slot: usize,
    /// the channel it was taken from (0-based)
    pub src: usize,
    pub t0: f64,
    pub dt: f64,
    pub v: Vec<f32>,
}

/// What a session file holds (ADR 0027): the setup as text, the record on the screen, the loaded reference slots.
#[derive(Clone)]
pub struct Session {
    pub setup: String,
    pub record: Option<Record>,
    pub refs: Vec<SessionRef>,
    /// the reference slot that filled the screen when the session was saved in the offline view (`session_view`)
    pub offline_ref: Option<usize>,
}

/// The end of a session saved in the offline view, after the reference slots: "VIEW", u32 1 (the offline view) and
/// u32 the slot shown (0-based). A session saved with the live channels has no such end, and a build from before it
/// reads the file without it.
pub fn session_view(offline_ref: usize) -> [u8; 12] {
    let mut b = [0u8; 12];
    b[..4].copy_from_slice(b"VIEW");
    b[4..8].copy_from_slice(&1u32.to_le_bytes());
    b[8..].copy_from_slice(&(offline_ref as u32).to_le_bytes());
    b
}

/// A session as one file, little endian: "OOSS", u32 version 1, u32 length of the setup text and the text (UTF-8),
/// u32 1 or 0 and then an OOSW blob (`wave_bin`) when 1, u32 number of reference slots, and per slot u32 slot (0-based),
/// u32 source channel, f64 t0, f64 dt, u64 samples and the samples as f32 volts. The blob is `memory` when given: the
/// samples the acquisition kept (`wave_bin_of`), where `rec` has the points of the screen.
pub fn session_bin(setup: &str, rec: Option<&Record>, memory: Option<&[u8]>, m: &Model, refs: &[SessionRef]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"OOSS");
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&(setup.len() as u32).to_le_bytes());
    b.extend_from_slice(setup.as_bytes());
    match (memory, rec) {
        (Some(blob), _) => {
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(blob);
        }
        // a record with no channel on (the offline view, every input off) is no record: a blob of no channels
        // is one the reader refuses, and the file would not load
        (None, Some(r)) if (0..NCH).any(|c| m.ch[c].on && !r.ch[c].is_empty()) => {
            b.extend_from_slice(&1u32.to_le_bytes());
            b.extend_from_slice(&wave_bin(r, m));
        }
        (None, _) => b.extend_from_slice(&0u32.to_le_bytes()),
    }
    b.extend_from_slice(&(refs.len() as u32).to_le_bytes());
    for r in refs {
        b.extend_from_slice(&(r.slot as u32).to_le_bytes());
        b.extend_from_slice(&(r.src as u32).to_le_bytes());
        b.extend_from_slice(&r.t0.to_le_bytes());
        b.extend_from_slice(&r.dt.to_le_bytes());
        b.extend_from_slice(&(r.v.len() as u64).to_le_bytes());
        for x in &r.v {
            b.extend_from_slice(&x.to_le_bytes());
        }
    }
    b
}

/// Whether these bytes are a session file (`session_bin`), by its mark: the command line opens one as a session.
pub fn looks_session(b: &[u8]) -> bool {
    b.starts_with(b"OOSS")
}

/// Reads a session file. Every length is checked against the bytes that are there and against the import limits before
/// anything is allocated for it, so a cut or made-up file is an error, not a crash or a huge allocation.
pub fn parse_session(b: &[u8], slots: usize) -> Result<Session, String> {
    let cut = || "the session file is cut short".to_string();
    let u32_at = |at: usize| b.get(at..at + 4).map(|x| u32::from_le_bytes(x.try_into().unwrap())).ok_or_else(cut);
    let u64_at = |at: usize| b.get(at..at + 8).map(|x| u64::from_le_bytes(x.try_into().unwrap())).ok_or_else(cut);
    let f64_at = |at: usize| b.get(at..at + 8).map(|x| f64::from_le_bytes(x.try_into().unwrap())).ok_or_else(cut);
    if b.len() < 12 || &b[..4] != b"OOSS" {
        return Err("not an OOS session file".into());
    }
    let version = u32_at(4)?;
    if version != 1 {
        return Err(format!("session version {version}, this build reads 1"));
    }
    let n = u32_at(8)? as usize;
    let mut at: usize = 12;
    let setup = b.get(at..at.checked_add(n).ok_or_else(cut)?).ok_or_else(cut)?;
    let setup = std::str::from_utf8(setup).map_err(|_| "the setup of the session is not text".to_string())?.to_string();
    at += n;
    let record = match u32_at(at)? {
        0 => {
            at += 4;
            None
        }
        1 => {
            at += 4;
            let (rec, used) = parse_wave_bin(&b[at..])?;
            at += used;
            Some(rec)
        }
        x => return Err(format!("session record marker {x}")),
    };
    let count = u32_at(at)? as usize;
    at += 4;
    if count > slots {
        return Err(format!("{count} reference slots, this scope has {slots}"));
    }
    let mut refs = Vec::with_capacity(count);
    for _ in 0..count {
        let (slot, src) = (u32_at(at)? as usize, u32_at(at + 4)? as usize);
        let (t0, dt) = (f64_at(at + 8)?, f64_at(at + 16)?);
        let len = u64_at(at + 24)? as usize;
        at += 32;
        if slot >= slots || src >= NCH || !(dt > 0.0) || !dt.is_finite() || !t0.is_finite() {
            return Err("a reference slot of the session is not valid".into());
        }
        if len > MAX_WAVEFORM_SAMPLES {
            return Err("a reference of the session is larger than the import limit".into());
        }
        let bytes = b.get(at..at.checked_add(len.checked_mul(4).ok_or_else(cut)?).ok_or_else(cut)?).ok_or_else(cut)?;
        let v = bytes.chunks_exact(4).map(|x| f32::from_le_bytes(x.try_into().unwrap())).collect();
        at += len * 4;
        refs.push(SessionRef { slot, src, t0, dt, v });
    }
    // the view the session was saved in, when the file says so and names a slot it holds
    let offline_ref = b.get(at..at + 12).filter(|end| &end[..4] == b"VIEW" && end[4..8] == 1u32.to_le_bytes())
        .map(|end| u32::from_le_bytes(end[8..12].try_into().unwrap()) as usize)
        .filter(|slot| refs.iter().any(|r| r.slot == *slot));
    Ok(Session { setup, record, refs, offline_ref })
}

/// One OOSW blob: the record and the number of bytes consumed.
pub fn parse_wave_bin(b: &[u8]) -> Result<(Record, usize), String> {
    if b.len() < 36 || &b[..4] != b"OOSW" {
        return Err("not an OOS waveform file".into());
    }
    let ver = u32_le(b, 4).unwrap_or(0);
    if ver != 1 {
        return Err(format!("waveform version {}", ver));
    }
    let nch = u32_le(b, 8).unwrap_or(0) as usize;
    // the sample count comes from the file: bounded before it sizes anything, so a damaged or hostile count is an
    // error and not an overflow or an allocation the size of the count
    let n = usize::try_from(u64_le(b, 12).unwrap_or(0)).unwrap_or(usize::MAX);
    let dt = f64_le(b, 20).unwrap_or(0.0);
    let t0 = f64_le(b, 28).unwrap_or(0.0);
    if nch == 0 || nch > NCH || dt <= 0.0 {
        return Err("waveform header".into());
    }
    if n > MAX_WAVEFORM_SAMPLES {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    let mut off = 36;
    let mut rec = Record { t0, dt, ch: Default::default(), triggered: true, partial: false, raw: None, la: None, pairs: false };
    for _ in 0..nch {
        let c = u32_le(b, off).ok_or("truncated waveform")? as usize;
        off += 4;
        if c < 1 || c > NCH {
            return Err(format!("channel {}", c));
        }
        if b.len().saturating_sub(off) < 4 * n {
            return Err("truncated waveform".into());
        }
        let mut v = vec![0f32; n];
        for i in 0..n {
            v[i] = f32_le(b, off + 4 * i).unwrap_or(f32::NAN);
        }
        rec.ch[c - 1] = v;
        off += 4 * n;
    }
    Ok((rec, off))
}

/// Fast-frame file: "OOSF", u32 version 1, u32 frame count, then that many OOSW blobs.
pub fn ff_bin(recs: &[Record], m: &Model) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(b"OOSF");
    b.extend_from_slice(&1u32.to_le_bytes());
    b.extend_from_slice(&(recs.len() as u32).to_le_bytes());
    for rec in recs {
        b.extend_from_slice(&wave_bin(rec, m));
    }
    b
}

pub fn parse_ff(b: &[u8]) -> Result<Vec<Record>, String> {
    if b.len() >= 4 && &b[..4] == b"OOSW" {
        return parse_wave_bin(b).map(|(r, _)| vec![r]);
    }
    if b.len() < 12 || &b[..4] != b"OOSF" {
        return Err("not a fast-frame file".into());
    }
    let ver = u32_le(b, 4).unwrap_or(0);
    if ver != 1 {
        return Err(format!("fast-frame version {}", ver));
    }
    let n = u32_le(b, 8).unwrap_or(0) as usize;
    // every frame is at least a header and one channel number: a count the bytes cannot hold is refused before it
    // sizes the list
    if n > (b.len() - 12) / 40 {
        return Err("truncated fast-frame file".into());
    }
    let mut off = 12;
    let mut recs = Vec::with_capacity(n);
    for _ in 0..n {
        let (r, used) = parse_wave_bin(b.get(off..).unwrap_or(&[]))?;
        recs.push(r);
        off += used;
    }
    Ok(recs)
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
                if best.as_ref().map_or(true, |(b, _)| n >= *b) {
                    best = Some((n, e.path()));
                }
            }
        }
    }
    best.map(|x| x.1)
}

/// Mask file: optional header (`# OOS mask` then `key=value` lines and `---`) then one volt per line.
pub struct MaskFile {
    pub src: Option<usize>,
    pub x: Option<f32>,
    pub y: Option<f32>,
    pub inv: Option<bool>,
    pub v: Vec<f32>,
}

pub fn mask_text(src: usize, x: f32, y: f32, inv: bool, v: &[f32]) -> String {
    let mut s = String::from("# OOS mask\n");
    s.push_str(&format!("src={}\n", src + 1));
    s.push_str(&format!("x={}\n", x));
    s.push_str(&format!("y={}\n", y));
    s.push_str(&format!("inv={}\n", if inv { 1 } else { 0 }));
    s.push_str("---\n");
    for x in v {
        s.push_str(&format!("{:.9e}\n", x));
    }
    s
}

pub fn parse_mask(text: &str) -> Result<MaskFile, String> {
    let mut lines = text.lines().peekable();
    let mut src = None;
    let mut x = None;
    let mut y = None;
    let mut inv = None;
    if lines.peek().map_or(false, |l| l.starts_with("# OOS mask")) {
        lines.next();
        for line in lines.by_ref() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "---" {
                break;
            }
            let Some((k, v)) = line.split_once('=') else {
                return Err(format!("mask header: {}", line));
            };
            match k.trim() {
                "src" => {
                    let n: usize = v.trim().parse().map_err(|e: std::num::ParseIntError| e.to_string())?;
                    if n < 1 || n > NCH {
                        return Err(format!("mask src {}", n));
                    }
                    src = Some(n - 1);
                }
                "x" => x = Some(v.trim().parse().map_err(|e: std::num::ParseFloatError| e.to_string())?),
                "y" => y = Some(v.trim().parse().map_err(|e: std::num::ParseFloatError| e.to_string())?),
                "inv" => inv = Some(matches!(v.trim(), "1" | "true" | "on")),
                _ => {}
            }
        }
    }
    let mut v = Vec::new();
    for line in lines {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        v.push(line.parse::<f32>().map_err(|e| e.to_string())?);
    }
    if v.is_empty() {
        return Err("empty mask".into());
    }
    Ok(MaskFile { src, x, y, inv, v })
}

// ---------------------------------------------------------------------------------------------- PNG

/// CRC-32 tables for eight bytes per step ("slicing by 8"): `T[0]` is the usual byte table, `T[k][n]` the CRC of byte
/// `n` followed by `k` zero bytes.
const CRC_T: [[u32; 256]; 8] = {
    let mut t = [[0u32; 256]; 8];
    let mut n = 0;
    while n < 256 {
        let mut c = n as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
            k += 1;
        }
        t[0][n] = c;
        n += 1;
    }
    let mut k = 1;
    while k < 8 {
        let mut n = 0;
        while n < 256 {
            let c = t[k - 1][n];
            t[k][n] = t[0][(c & 0xff) as usize] ^ (c >> 8);
            n += 1;
        }
        k += 1;
    }
    t
};

/// CRC-32 (ISO 3309 / PNG / zip).
pub fn crc32(data: &[u8]) -> u32 {
    let t = &CRC_T;
    let mut c = !0u32;
    let mut it = data.chunks_exact(8);
    for b in &mut it {
        let lo = c ^ u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
        c = t[7][(lo & 0xff) as usize]
            ^ t[6][(lo >> 8 & 0xff) as usize]
            ^ t[5][(lo >> 16 & 0xff) as usize]
            ^ t[4][(lo >> 24) as usize]
            ^ t[3][b[4] as usize]
            ^ t[2][b[5] as usize]
            ^ t[1][b[6] as usize]
            ^ t[0][b[7] as usize];
    }
    !it.remainder().iter().fold(c, |c, &b| t[0][((c ^ b as u32) & 0xff) as usize] ^ (c >> 8))
}

/// Adler-32 (zlib). Sixteen lanes side by side (the compiler makes vector code of the lane loop): lane `k` sums the
/// bytes at `16 j + k` (`sa`) and those sums again (`sb`), and per chunk of 5552 bytes `b` grows by `len * a`,
/// `16 * sum(sb)` less `sum(k * sa[k])` (each byte counted `len - position` times), the same as byte by byte.
pub fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u64, 0u64);
    for chunk in data.chunks(5552) {
        let (mut sa, mut sb) = ([0u32; 16], [0u32; 16]);
        let mut blocks = chunk.chunks_exact(16);
        for blk in &mut blocks {
            for k in 0..16 {
                sa[k] += blk[k] as u32;
                sb[k] += sa[k];
            }
        }
        let n = (chunk.len() - blocks.remainder().len()) as u64;
        let sum_a: u64 = sa.iter().map(|&x| x as u64).sum();
        let sum_b: u64 = sb.iter().map(|&x| x as u64).sum();
        let weighted: u64 = sa.iter().enumerate().map(|(k, &x)| k as u64 * x as u64).sum();
        b += n * a + 16 * sum_b - weighted;
        a += sum_a;
        for &x in blocks.remainder() {
            a += x as u64;
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    ((b << 16) | a) as u32
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let start = out.len();
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let crc = crc32(&out[start..]);
    out.extend_from_slice(&crc.to_be_bytes());
}

/// Deflate length codes 257..285 (RFC 1951 3.2.5): base length and extra bits.
const LEN_BASE: [(u16, u8); 29] = [
    (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (9, 0), (10, 0), (11, 1), (13, 1), (15, 1), (17, 1), (19, 2), (23, 2), (27, 2),
    (31, 2), (35, 3), (43, 3), (51, 3), (59, 3), (67, 4), (83, 4), (99, 4), (115, 4), (131, 5), (163, 5), (195, 5), (227, 5), (258, 0),
];
/// Deflate distance codes 0..29: base distance and extra bits.
const DIST_BASE: [(u16, u8); 30] = [
    (1, 0), (2, 0), (3, 0), (4, 0), (5, 1), (7, 1), (9, 2), (13, 2), (17, 3), (25, 3), (33, 4), (49, 4), (65, 5), (97, 5), (129, 6),
    (193, 6), (257, 7), (385, 7), (513, 8), (769, 8), (1025, 9), (1537, 9), (2049, 10), (3073, 10), (4097, 11), (6145, 11),
    (8193, 12), (12289, 12), (16385, 13), (24577, 13),
];

/// Bit writer of a deflate stream: values LSB first, Huffman codes MSB first.
struct BitOut {
    out: Vec<u8>,
    acc: u64,
    n: u32,
}

impl BitOut {
    /// At most 32 bits at a time; whole bytes go out once 32 have gathered.
    fn bits(&mut self, v: u32, n: u32) {
        self.acc |= (v as u64) << self.n;
        self.n += n;
        if self.n >= 32 {
            self.out.extend_from_slice(&(self.acc as u32).to_le_bytes());
            self.acc >>= 32;
            self.n -= 32;
        }
    }
    /// A Huffman code of `n` bits, first bit = most significant.
    fn code(&mut self, c: u32, n: u32) {
        self.bits(c.reverse_bits() >> (32 - n), n);
    }
    /// Literal/length symbol of the fixed code (RFC 1951 3.2.6).
    fn symbol(&mut self, s: u32) {
        match s {
            0..=143 => self.code(0x30 + s, 8),
            144..=255 => self.code(0x190 + s - 144, 9),
            256..=279 => self.code(s - 256, 7),
            _ => self.code(0xc0 + s - 280, 8),
        }
    }
    /// Length symbol and extra bits of a match of 3..=258 bytes, then the 5-bit distance code and its extra bits.
    fn matched(&mut self, len: usize, dist_code: usize, dist_extra: u32) {
        let i = LEN_CODE[len] as usize;
        self.symbol(257 + i as u32);
        self.bits((len - LEN_BASE[i].0 as usize) as u32, LEN_BASE[i].1 as u32);
        self.code(dist_code as u32, 5);
        self.bits(dist_extra, DIST_BASE[dist_code].1 as u32);
    }
    fn finish(mut self) -> Vec<u8> {
        while self.n > 0 {
            self.out.push(self.acc as u8);
            self.acc >>= 8;
            self.n = self.n.saturating_sub(8);
        }
        self.out
    }
}

/// Index into `LEN_BASE` for each match length 0..=258 (0..2 unused).
const LEN_CODE: [u8; 259] = {
    let mut t = [0u8; 259];
    let mut len = 3;
    while len <= 258 {
        let mut i = 28;
        while LEN_BASE[i].0 as usize > len {
            i -= 1;
        }
        t[len] = i as u8;
        len += 1;
    }
    t
};

/// Number of equal bytes at `i` and `i - d`, at most 258 (the longest deflate match); eight bytes per step.
#[inline]
fn match_len(raw: &[u8], i: usize, d: usize) -> usize {
    let max = (raw.len() - i).min(258);
    let (a, b) = (&raw[i..i + max], &raw[i - d..i - d + max]);
    let mut n = 0;
    while n + 8 <= max {
        let x = u64::from_le_bytes(a[n..n + 8].try_into().unwrap()) ^ u64::from_le_bytes(b[n..n + 8].try_into().unwrap());
        if x != 0 {
            return n + (x.trailing_zeros() / 8) as usize;
        }
        n += 8;
    }
    while n < max && a[n] == b[n] {
        n += 1;
    }
    n
}

/// Deflate (one block, fixed Huffman codes) of picture rows: besides literals only matches against the pixel to the left
/// (distance 3) and the pixel above (distance `stride`). A scope screen is flat areas, grid lines and a few traces, so
/// that is most of what a full LZ77 finds, in one pass, with no hash table and little time on the scope's CPU. The
/// left match costs 10 bits less than the one above (no extra distance bits), so it is looked for first and the one
/// above only when the left one is not the longest possible.
fn deflate_rows(raw: &[u8], stride: usize) -> Vec<u8> {
    let (stride_code, stride_extra) = {
        let c = DIST_BASE.iter().rposition(|&(b, _)| b as usize <= stride).unwrap_or(0);
        (c, stride.saturating_sub(DIST_BASE[c].0 as usize) as u32)
    };
    let stride_ok = stride > 3 && stride <= 32768;
    let mut o = BitOut { out: Vec::with_capacity(raw.len() / 8 + 64), acc: 0, n: 0 };
    o.bits(1, 1); // BFINAL
    o.bits(1, 2); // BTYPE 01 = fixed Huffman
    let mut i = 0;
    while i < raw.len() {
        let left = if i >= 3 { match_len(raw, i, 3) } else { 0 };
        let above = if left < 258 && stride_ok && i >= stride { match_len(raw, i, stride) } else { 0 };
        if above > left && above >= 3 {
            o.matched(above, stride_code, stride_extra);
            i += above;
        } else if left >= 3 {
            o.matched(left, 2, 0);
            i += left;
        } else {
            o.symbol(raw[i] as u32);
            i += 1;
        }
    }
    o.symbol(256);
    o.finish()
}

/// PNG (8-bit RGB) of XRGB8888 pixels: filter 0 per row, zlib stream of one fixed-Huffman deflate block
/// (`deflate_rows`), so a screen of flat colour and thin lines takes a few percent of its bitmap.
pub fn png(w: u32, h: u32, px: &[u32]) -> Vec<u8> {
    let stride = w as usize * 3 + 1;
    let mut raw = vec![0u8; stride * h as usize];
    for (out, row) in raw.chunks_exact_mut(stride).zip(px.chunks(w as usize)) {
        // out[0] = 0: filter type None
        for (o, &p) in out[1..].chunks_exact_mut(3).zip(row) {
            o.copy_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }
    let mut z = vec![0x78, 0x01]; // deflate, 32 K window, no dictionary, (0x7801 % 31 == 0)
    z.extend_from_slice(&deflate_rows(&raw, stride));
    z.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]); // 8 bit, RGB, deflate, filter set 0, no interlace
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

/// PNG of the screen canvas.
/// Draws a binary PPM (P6, maxval 255) centred into the canvas (black around it); returns its size.
pub fn ppm_into(b: &[u8], cv: &mut Canvas) -> Option<(i32, i32)> {
    // header: "P6" w h maxval, separated by whitespace, comments with '#'
    let mut fields = Vec::new();
    let mut p = 2;
    if !b.starts_with(b"P6") {
        return None;
    }
    while fields.len() < 3 {
        while p < b.len() && (b[p].is_ascii_whitespace() || b[p] == b'#') {
            if b[p] == b'#' {
                while p < b.len() && b[p] != b'\n' {
                    p += 1;
                }
            }
            p += 1;
        }
        let s = p;
        while p < b.len() && b[p].is_ascii_digit() {
            p += 1;
        }
        fields.push(std::str::from_utf8(&b[s..p]).ok()?.parse::<i32>().ok()?);
    }
    let (w, h, max) = (fields[0], fields[1], fields[2]);
    p += 1;
    if max != 255 || w <= 0 || h <= 0 || b.len() < p + (w * h * 3) as usize {
        return None;
    }
    cv.fill(crate::gfx::Rect::new(0, 0, cv.w, cv.h), 0);
    let (ox, oy) = ((cv.w - w) / 2, (cv.h - h) / 2);
    for y in 0..h {
        for x in 0..w {
            let (cx, cy) = (ox + x, oy + y);
            if (0..cv.w).contains(&cx) && (0..cv.h).contains(&cy) {
                let k = p + ((y * w + x) * 3) as usize;
                cv.px[(cy * cv.w + cx) as usize] = crate::gfx::rgb(b[k] as u32, b[k + 1] as u32, b[k + 2] as u32);
            }
        }
    }
    Some((w, h))
}

pub fn png_of(cv: &Canvas) -> Vec<u8> {
    png(cv.w as u32, cv.h as u32, &cv.px)
}

/// Writes everything cached to the disks (a stick can be pulled, the scope switched off at the mains). Nothing to do
/// on hosts without `sync(2)`; their files reach the disk on their own.
pub fn sync_disks() {
    #[cfg(unix)]
    unsafe {
        libc::sync()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // Break it catches: a save command given a path into the vendor's data wrote there (ADR 0009).
    #[test]
    fn no_save_writes_the_vendors_data() {
        for p in ["/rigol/data/x.bin", "/rigol/x", "/tmp/../rigol/data/x.bin", "/rigol/./data/../data/x"] {
            assert!(check_target(Path::new(p)).is_err(), "{p}");
        }
        assert!(check_target(Path::new("/tmp/oos/rigol.bin")).is_ok());
    }
    use crate::gfx::rgb;
    use crate::math::{MathOp, Window};

    #[test]
    #[cfg(unix)]
    fn a_save_does_not_follow_links_or_write_to_devices() {
        let d = std::env::temp_dir().join(format!("oos-link-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let (victim, link) = (d.join("victim"), d.join("shot-001.png"));
        std::fs::write(&victim, b"keep").unwrap();
        std::os::unix::fs::symlink(&victim, &link).unwrap();
        assert!(write(&link, b"x").is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"keep");
        // taken, also when it dangles
        std::fs::remove_file(&victim).unwrap();
        assert_eq!(next_name(&d, "shot", "png"), d.join("shot-002.png"));
        assert!(write(&link, b"x").is_err());
        assert!(!victim.exists());
        assert!(write(Path::new("/dev/null"), b"x").is_err());
        write(&d.join("ok.txt"), b"1").unwrap();
        write(&d.join("ok.txt"), b"2").unwrap();
        assert_eq!(std::fs::read(d.join("ok.txt")).unwrap(), b"2");
        let _ = std::fs::remove_dir_all(&d);
    }

    // Break it catches: the hardware average switch lost in a setup or on by default, the count not stored with it.
    #[test]
    fn average_settings_round_trip() {
        let mut m = Model::default();
        assert!(!m.avg_hw);
        (m.acq, m.avg_count, m.avg_hw) = (AcqMode::Average, 64, true);
        let back = parse_setup(&setup_text(&m)).unwrap();
        assert_eq!((back.acq, back.avg_count, back.avg_hw), (AcqMode::Average, 64, true));
        // older setups without the key: software average
        assert!(!parse_setup("acq = Average\navg_count = 8\n").unwrap().avg_hw);
        assert!(parse_setup("avg_hw = maybe\n").is_err());
    }

    // Break it catches: hardware segment settings lost in a setup, a count clamped to the software limit, a negative
    // or absurd interval reaching 0x4068.
    #[test]
    fn segment_settings_round_trip() {
        let mut m = Model::default();
        (m.segments, m.seg_hw, m.seg_ivl) = (20_000, true, 1e-3);
        let back = parse_setup(&setup_text(&m)).unwrap();
        assert_eq!((back.segments, back.seg_hw, back.seg_ivl), (20_000, true, 1e-3));
        let d = parse_setup("segments = 900000\nseg_interval = -1\n").unwrap();
        assert_eq!((d.segments, d.seg_hw, d.seg_ivl), (crate::ui::SEG_HW_MAX, false, 0.0));
        assert_eq!(parse_setup("seg_interval = 1e9\n").unwrap().seg_ivl, crate::ui::SEG_IVL_MAX);
    }

    #[test]
    fn tdiv_by_value_and_legacy_index() {
        let m = parse_setup("tdiv = 1e-9\nzoom_tdiv = 1e-9\n").unwrap();
        assert_eq!((tscales()[m.tscale], tscales()[m.zscale]), (1e-9, 1e-9));
        assert!(setup_text(&m).contains("tdiv = 0.000000001"));
        // old ladder started at 5 ns: index 0 was 5 ns/div, 3 was 50 ns
        let m = parse_setup("tscale = 3\nzscale = 0\n").unwrap();
        assert_eq!((tscales()[m.tscale], tscales()[m.zscale]), (50e-9, 5e-9));
        assert!(parse_setup("tdiv = 3e-9").is_err());
    }

    #[test]
    fn vdiv_by_value_and_legacy_index() {
        let m = parse_setup("ch1.vdiv = 0.0001\nch2.vscale = 10\n").unwrap();
        assert_eq!(VSCALES[m.ch[0].vscale], 100e-6);
        // old ladder started at 500 uV: index 10 was 1 V/div
        assert_eq!(VSCALES[m.ch[1].vscale], 1.0);
        assert_eq!(VSCALES[Model::default().ch[0].vscale], 1.0);
        assert!(setup_text(&m).contains("ch1.vdiv = 0.0001"));
    }

    /// A non-default set-up in every group of fields.
    fn custom() -> Model {
        let mut m = Model::default();
        (m.bode_sw_time, m.bode_sw_log, m.bode_sync, m.bode_sync_slope, m.bode_sync_level, m.bode_sw_in, m.bode_ref) = (12.5, false, 2, Slope::Fall, -0.75, 0, -6.02);
        m.ch[1].on = true;
        m.ch[1].vscale = 1; // 200 uV/div
        m.ch[1].offset = -0.123_456_79;
        m.ch[2].coupling = Coupling::Ac;
        m.ch[2].bw = BW_20M;
        m.ch[1].bw = 17;
        m.ch[3].probe = 10.0;
        m.ch[3].invert = true;
        m.active = 2;
        m.tscale = 5;
        m.delay = 1.5e-7;
        m.trig_src = 3;
        m.trig_level = 0.1;
        m.slope = Slope::Either;
        m.trig_mode = TrigMode::Normal;
        m.trig_type = TrigType::SetupHold;
        m.trig_level2 = -2.25;
        m.trig_src2 = 1;
        m.trig_slope2 = Slope::Fall;
        m.trig_pol = Pol::Neg;
        m.trig_when = WhenKind::Out;
        m.trig_t1 = 3e-9;
        m.trig_t2 = 0.7;
        m.video_std = VideoStd::Ntsc;
        m.video_sync = VSync::Line;
        m.video_line = 123;
        m.pat = [Bit::R, Bit::F, Bit::L, Bit::X];
        m.pat_levels = [0.1, -0.2, 3.3, 1e-3];
        m.win_mode = WinMode::Inside;
        m.nth_idle = 2e-3;
        m.nth_n = 7;
        m.setup = 5e-9;
        m.hold = 6e-9;
        m.acq = AcqMode::Average;
        m.avg_count = 256;
        m.mdepth = 4;
        m.persist = 6;
        m.intensity = 85;
        m.grid = Grid::Light;
        m.meas = vec![(Meas::Rise, WaveId::Ch(3)), (Meas::Vrms, WaveId::Ch(0)), (Meas::PDuty, WaveId::Ch(1))];
        m.strip_dock = StripDock::Right;
        m.cursor = CursorMode::Volt;
        m.cur_a = 0.125;
        m.cur_b = 0.875;
        m.touch_lock = true;
        m.aux_out = AuxOut::Off;
        m.ser = SerialTrig { rel: Rel::Out, max: 0x1_2345_6789, mask: 0xf0f0, uart_inv: true, uart_stop: StopBits::Two, uart_err: UartErr::Parity, spi_cs_high: true, spi_timeout: 2e-6, addr_rel: Rel::In, addr_max: 0x3a0, dir: I2cDir::Read, ten_bit: true, bytes: 4 };
        m.trig_bus_val = 0xab_cdef_0123;
        m.maths[3] = crate::math::MathCh { op: MathOp::Bpf, a: 4, b: 0, scale: 7, fc: 1234.0, bw: 56.0, offset: -1.5, win: Window::Blackman, lin: (1.0, 0.0) };
        m.math_bw = 777.0;
        m.zones[1] = crate::zone::Zone { on: true, src: 2, inside: false, pts: vec![(-1e-4, 0.5), (2.5e-5, 1.25), (3e-5, -0.75)] };
        m.dots = true;
        m.z_src = Some(3);
        m.hist = Hist::Horizontal;
        (m.view, m.spec_src, m.spec_center, m.spec_span, m.spec_rbw, m.spec_trace, m.spec_ref, m.spec_wf, m.spec_dist) = (AView::Spectrum, WaveId::Ch(2), 1.5e6, 2e5, 1e3, 2, -12.5, true, true);
        (m.spec_sort, m.spec_harm, m.trend_sel) = (1, 4, [3, 1]);
        (m.eye_src, m.eye_rate, m.pw_v, m.pw_i, m.pw_mode) = (WaveId::Ch(3), 1.25e9, WaveId::Ch(2), WaveId::Ch(0), PowerMode::Switching);
        m.math = MathOp::Fft;
        m.math_a = 2;
        m.math_b = 3;
        m.math_scale = 2;
        m.math_fc = 12345.0;
        m.math_offset = -1.3;
        m.fft_win = Window::Flattop;
        m.dec_on = true;
        m.dec_src = 1;
        m.dec_baud = 3;
        m.dec_thr = 0.8;
        m.dec_ascii = false;
        m.dec_proto = Proto::SPI;
        (m.dec_arate, m.dec_tick, m.dec_nibbles, m.dec_uspeed) = (2, 3, 4, 2);
        m.dec_src2 = 0;
        m.dec_cs = None;
        m.dec_mode = 3;
        m.dec_bits = 16;
        m.dec_parity = Parity::Odd;
        m.zoom = true;
        m.zscale = 2;
        m.zdelay = -3e-8;
        m.color_grade = true;
        m.mask_on = true;
        m.mask_src = 3;
        m.mask_x = 0.5;
        m.mask_y = 1.25;
        m.mask_stop = true;
        m.quick = Quick::SaveCsv;
        m.la = crate::ui::la::LaSet { on: true, show: 0x0f0f, preset: [2, crate::ui::la::USER], user: [0.5, -0.25], height: 2 };
        m
    }

    #[test]
    fn a_ref_in_a_setup_stays_a_ref_and_a_channel_number_stays_a_channel() {
        let mut m = Model::default();
        m.mag_v = WaveId::Ref(1);
        m.mag_i = WaveId::Ch(0);
        m.dpt_v = WaveId::Ref(0);
        m.inv_a = WaveId::Ref(4);
        m.inv_tq = Some(WaveId::Ref(9));
        m.spec_src = WaveId::Ref(0);
        m.eye_src = WaveId::Ref(2);
        m.pw_v = WaveId::Ref(1);
        m.pw_i = WaveId::Ch(0);
        m.bode_in = WaveId::Ref(3);
        m.bode_out = WaveId::Ch(1);
        m.meas = vec![(Meas::Vpp, WaveId::Ref(0))];
        m.meas2 = vec![(0, WaveId::Ref(0), WaveId::Ch(1))];
        let text = setup_text(&m);
        assert!(text.contains("magnetic = R2 1 "));
        assert!(text.contains("dpt = R1 "));
        assert!(text.contains("spec = R1 "));
        assert!(text.contains("eye = R3 "));
        assert!(text.contains("power = R2 1 "));
        assert!(text.contains("bode = "));
        assert!(text.contains(" R4 2 "));
        assert!(text.contains("meas = Vpp:R1"));
        assert!(text.contains("meas2 = 0:R1:2"));
        let back = parse_setup(&text).unwrap();
        assert_eq!((back.mag_v, back.mag_i, back.dpt_v, back.inv_a, back.inv_tq), (m.mag_v, m.mag_i, m.dpt_v, m.inv_a, m.inv_tq));
        assert_eq!((back.spec_src, back.eye_src, back.pw_v, back.pw_i, back.bode_in, back.bode_out), (m.spec_src, m.eye_src, m.pw_v, m.pw_i, m.bode_in, m.bode_out));
        assert_eq!((back.meas, back.meas2), (m.meas, m.meas2));
        // a setup written before refs were sources
        let old = parse_setup("magnetic = 1 2 1 0 0 0 IPsi 0\ninverter = Dq0 1 2 3 Off 1 4 true off off off 1 1 0 false 0 1 off 1024 false 0 1 2\ndpt = 1 2 3 Off 4 10 Both\n").unwrap();
        assert_eq!(old.mag_v, WaveId::Ch(0));
        assert_eq!(old.mag_i, WaveId::Ch(1));
        assert_eq!(old.inv_a, WaveId::Ch(0));
        assert_eq!(old.dpt_g, WaveId::Ch(2));
        assert_eq!(old.inv_tq, None);
    }

    #[test]
    fn catalog_setup_validates_ids_sources_duplicates_and_shared_selection_limit() {
        let mut model=Model::default();
        set(&mut model,"cat","90:R1:R2:3:4 170:R3:R4:R5:4").unwrap();
        let text=setup_text(&model);let restored=parse_setup(&text).unwrap();assert_eq!(restored.cat,model.cat);
        for invalid in ["999:R1:R2:3:4","1:R1:R2:3:4","90:R99:R2:3:4","90:R1:R2:3","90:R1:R2:3:4 90:R1:R2:3:4"] {
            let before=model.cat.clone();assert!(set(&mut model,"cat",invalid).is_err(),"{invalid}");assert_eq!(model.cat,before);
        }
        model.meas=vec![(Meas::Vpp,WaveId::Ch(0));MEAS_SELECTION_MAX-1];model.meas2.clear();model.cat.clear();
        assert!(set(&mut model,"cat","90:R1:R2:3:4 170:R3:R4:R5:4").is_err());assert!(model.cat.is_empty());
        set(&mut model,"cat","90:R1:R2:3:4").unwrap();
        assert!(set(&mut model,"meas2","0:1:2").is_err());assert!(model.meas2.is_empty());
        set(&mut model,"cat","").unwrap();assert!(model.cat.is_empty());
    }

    #[test]
    fn setup_round_trip() {
        let m = custom();
        assert_ne!(m, Model::default());
        let text = setup_text(&m);
        let back = parse_setup(&text).unwrap();
        assert_eq!(back, m);
        // and the default set-up
        assert_eq!(parse_setup(&setup_text(&Model::default())).unwrap(), Model::default());
    }

    #[test]
    fn setup_sim_advanced_round_trip_and_legacy_defaults() {
        let mut model = Model::default();
        model.sim_prbs = 13;
        model.sim_ssc_center = true;
        model.sim_ssc_deviation = 0.01;
        model.sim_ssc_hz = 40e3;
        model.sim_dcd_pp = 8e-12;
        model.sim_corr_rj = 2e-12;
        model.sim_corr_hz = 2e6;
        let setup = setup_text(&model);
        assert_eq!(parse_setup(&setup).unwrap(), model);
        let legacy = setup.lines().filter(|line| !line.starts_with("sim_extra =")).collect::<Vec<_>>().join("\n");
        let old = parse_setup(&legacy).unwrap();
        assert_eq!(old.sim_prbs, 13);
        assert!(!old.sim_ssc_center);
        assert_eq!(old.sim_ssc_deviation, 0.005);
        assert_eq!(old.sim_ssc_hz, 33e3);
        assert_eq!(old.sim_dcd_pp, 0.0);
        assert_eq!(old.sim_corr_rj, 0.0);
        assert_eq!(old.sim_corr_hz, 1e6);
        for invalid in ["true NaN 33000 0 0 1e6", "false .005 0 0 0 1e6",
                        "true .005 33000 -1 0 1e6", "false .005 33000 0 1 1e6"] {
            assert!(parse_setup(&format!("sim_extra = {invalid}\n")).is_err());
        }
    }

    #[test]
    fn setup_errors_and_unknown_keys() {
        let m = parse_setup("# comment\n\nnew_key_of_a_later_version = 3\nch2.on = true\n").unwrap();
        assert!(m.ch[1].on);
        assert!(parse_setup("tscale = 9999").unwrap_err().contains("line 1: tscale"));
        assert!(parse_setup("ch1.coupling = XY").is_err());
        assert!(parse_setup("ch9.on = true").is_err());
        assert!(parse_setup("pat = H L").is_err());
        assert!(parse_setup("delay").is_err());
    }

    /// AUX OUT: written by name, missing key = trigger out (the FPGA's power-up state).
    #[test]
    fn setup_aux_out() {
        assert_eq!(parse_setup("ch2.on = true").unwrap().aux_out, AuxOut::TrigOut);
        assert!(setup_text(&Model::default()).contains("aux_out = TrigOut\n"));
        assert_eq!(parse_setup("aux_out = Off").unwrap().aux_out, AuxOut::Off);
        assert!(parse_setup("aux_out = Blink").is_err());
        assert_eq!(parse_setup("aux_out = PassFail").unwrap().aux_out, AuxOut::PassFail);
    }

    /// The Quick key action: written by name, read back, unknown names rejected, missing key = Screenshot.
    #[test]
    fn setup_quick_key() {
        for &q in QUICKS.iter() {
            let mut m = Model::default();
            m.quick = q;
            let text = setup_text(&m);
            assert!(text.contains(&format!("quick = {:?}\n", q)));
            assert_eq!(parse_setup(&text).unwrap().quick, q);
        }
        assert_eq!(parse_setup("quick = Autoset\nquick_v2 = Whatever\n").unwrap().quick, Quick::Autoset);
        assert_eq!(parse_setup("ch1.on = true\n").unwrap().quick, Quick::Screenshot);
        assert!(parse_setup("quick = Coffee").unwrap_err().contains("line 1: quick"));
    }

    /// Every protocol is stored by name; the USB decoder with its D- source and speed.
    #[test]
    #[cfg_attr(not(feature = "decoders"), ignore = "needs every decoder")]
    fn setup_protocols_by_name() {
        for &p in PROTOS.iter() {
            let mut m = Model::default();
            m.dec_proto = p;
            let text = setup_text(&m);
            assert!(text.contains(&format!("dec_proto = {}\n", p.key())));
            assert_eq!(parse_setup(&text).unwrap().dec_proto, p);
        }
        // the protocols that became modules keep the names of setups saved before
        for (old, name) in [("Dmx512", "DMX512"), ("Spdif", "S/PDIF"), ("UsbPd", "USB PD")] {
            assert_eq!(parse_setup(&format!("dec_proto = {}\n", old)).unwrap().dec_proto.name(), name);
        }
        // a module's options go with it
        let mut m = Model::default();
        m.dec_proto = Proto::from_key("Example").unwrap();
        m.dec_opt = [2, 1, 0, 0];
        let text = setup_text(&m);
        assert!(text.contains("dec_proto = Example\n") && text.contains("dec_opt = 2 1 0 0\n"));
        assert_eq!(parse_setup(&text).unwrap(), m);
        assert!(parse_setup("dec_proto = NoSuchBus").is_err());
        let mut m = Model::default();
        (m.dec_proto, m.dec_src, m.dec_cs, m.dec_uspeed) = (Proto::from_key("Usb").unwrap(), 2, Some(3), 1);
        let text = setup_text(&m);
        assert!(text.contains("dec_proto = Usb\n") && text.contains("dec_uspeed = 1\n"));
        assert_eq!(parse_setup(&text).unwrap(), m);
        assert!(parse_setup("dec_uspeed = 3").is_err());
    }

    #[test]
    fn csv_of_a_record() {
        let mut m = Model::default();
        m.ch[2].on = true;
        let mut rec = Record { t0: -1e-3, dt: 1e-6, ch: Default::default(), triggered: true, partial: false, raw: None, la: None, pairs: false };
        rec.ch[0] = vec![0.5, 1.25, f32::NAN];
        rec.ch[2] = vec![-1.0, 0.0, 3.3];
        let csv = wave_csv(&rec, &m);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Time (s),CH1 (V),CH3 (V)");
        assert_eq!(lines[1], "-1.000000000e-3,0.5,-1");
        assert_eq!(lines[3], "-9.980000000e-4,,3.3");
        assert_eq!(lines.len(), 4);
        let back = parse_wave(csv.as_bytes(), "wave.csv").unwrap();
        assert_eq!(back.len(), 2);
        assert!((back[0].dt - 1e-6).abs() < 1e-18);
        assert_eq!(back[0].v[0], 0.5);
        assert_eq!(back[1].v[2], 3.3);
    }

    /// A line wider than the reference slots ends the parse: before, one line of a million columns made a million
    /// traces (40 MB of text took 1.9 GB) before the slots refused them.
    #[test]
    fn csv_wider_than_the_reference_slots_is_refused_while_parsing() {
        let wide = format!("0{}\n1,0\n", ",0".repeat(NREF + 1));
        assert!(parse_wave(wide.as_bytes(), "wide.csv").unwrap_err().contains("more traces"));
        let full = format!("0{}\n1{}\n", ",1".repeat(NREF), ",2".repeat(NREF));
        assert_eq!(parse_wave(full.as_bytes(), "full.csv").unwrap().len(), NREF);
        // ragged lines: a column missing on a line, or starting late, is NaN there
        let back = parse_wave(b"Time,A\n0,1\n1,2,7\n2,,8\n3\n", "r.csv").unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!((back[0].t0, back[0].dt, back[0].label.as_str()), (0.0, 1.0, "CH1"));
        assert_eq!(&back[0].v[..2], [1.0, 2.0]);
        assert!(back[0].v[2].is_nan() && back[0].v.len() == 3);
        assert!(back[1].v[0].is_nan());
        assert_eq!(&back[1].v[1..], [7.0, 8.0]);
    }

    #[test]
    fn oos_bin_and_wfm_round_trip() {
        let mut m = Model::default();
        m.ch[0].on = true;
        let rec = Record { t0: -2e-6, dt: 5e-9, ch: { let mut c: [Vec<f32>; NCH] = Default::default(); c[0] = vec![0.1, 0.2, -0.3, 0.4]; c }, triggered: true, partial: false, raw: None, la: None, pairs: false };
        let bin = wave_bin(&rec, &m);
        let t = &parse_wave(&bin, "wave.bin").unwrap()[0];
        assert_eq!(t.v, rec.ch[0]);
        assert!((t.dt - rec.dt).abs() < 1e-18);
        assert!((t.t0 - rec.t0).abs() < 1e-18);

        let v = [0.0_f32, 0.5, 1.0, -0.25];
        let wfm = wfm_f32(b":WFM#003", -1e-6, 1e-9, &v, "lane");
        let g = parse_wave(&wfm, "x.wfm").unwrap();
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].label, "lane");
        assert!((g[0].dt - 1e-9).abs() < 1e-18);
        assert!((g[0].t0 + 1e-6).abs() < 1e-18);
        for (a, b) in g[0].v.iter().zip(v) {
            assert!((a - b).abs() < 1e-6, "{a} {b}");
        }
        let old = wfm_f32(b":WFM#001", -1e-6, 1e-9, &v, "lane");
        assert_eq!(parse_wave(&old, "old.wfm").unwrap()[0].v, v);
        let two = wfm_f32(b":WFM#002", 0.0, 2e-9, &v, "two");
        assert_eq!(parse_wave(&two, "two.wfm").unwrap()[0].label, "two");
    }

    /// A short Tek WFM curve of f32 samples, little endian.
    fn wfm_f32(magic: &[u8; 8], t0: f64, dt: f64, v: &[f32], label: &str) -> Vec<u8> {
        let (explicit, implicit, specification, curve) = if magic == b":WFM#001" { (166, 478, 790, 820) } else { (168, 488, 808, 838) };
        let mut b = vec![0u8; curve + v.len() * 4 + 8];
        b[0..2].copy_from_slice(&0x0F0Fu16.to_le_bytes());
        b[2..10].copy_from_slice(magic);
        b[15] = 4;
        b[16..20].copy_from_slice(&(curve as i32).to_le_bytes());
        let lab = label.as_bytes();
        b[40..40 + lab.len()].copy_from_slice(lab);
        let put_f64 = |b: &mut [u8], o: usize, x: f64| b[o..o + 8].copy_from_slice(&x.to_le_bytes());
        let put_u32 = |b: &mut [u8], o: usize, x: u32| b[o..o + 4].copy_from_slice(&x.to_le_bytes());
        put_f64(&mut b, explicit, 1.0);
        put_f64(&mut b, explicit + 8, 0.0);
        put_u32(&mut b, explicit + 72, 4);
        put_f64(&mut b, implicit, dt);
        put_f64(&mut b, implicit + 8, t0);
        put_u32(&mut b, specification + 14, 0);
        put_u32(&mut b, specification + 18, (v.len() * 4) as u32);
        for (k, &x) in v.iter().enumerate() {
            b[curve + k * 4..curve + k * 4 + 4].copy_from_slice(&x.to_le_bytes());
        }
        b
    }

    #[test]
    fn all_tek_wfm_versions_keep_voltage_calibration_time_origin_and_byte_order() {
        for magic in [b":WFM#001", b":WFM#002", b":WFM#003"] {
            let (explicit, implicit, specification, curve) = if magic == b":WFM#001" { (166, 478, 790, 820) } else { (168, 488, 808, 838) };
            for big_endian in [false, true] {
                let codes = [-2.0, 0.0, 2.0, 4.0];
                let mut bytes = wfm_f32(magic, -9.5e-7, 20e-12, &codes, "captured");
                bytes[explicit..explicit + 8].copy_from_slice(&0.125f64.to_le_bytes());
                bytes[explicit + 8..explicit + 16].copy_from_slice(&0.75f64.to_le_bytes());
                if big_endian {
                    bytes[0..2].copy_from_slice(&[0xf0, 0xf0]);
                    for offset in [16, explicit + 72, specification + 14, specification + 18] { bytes[offset..offset + 4].reverse(); }
                    for offset in [explicit, explicit + 8, implicit, implicit + 8] { bytes[offset..offset + 8].reverse(); }
                    for offset in (curve..curve + codes.len() * 4).step_by(4) { bytes[offset..offset + 4].reverse(); }
                }
                let parsed = parse_wave(&bytes, "capture.wfm").unwrap();
                assert_eq!(parsed[0].v, [0.5, 0.75, 1.0, 1.25]);
                assert_eq!(parsed[0].label, "captured");
                assert_eq!(parsed[0].dt, 20e-12);
                assert_eq!(parsed[0].t0, -9.5e-7);
            }
        }
    }

    #[test]
    fn tek_wfm_rejects_truncated_misaligned_or_invalid_scaling_before_decoding() {
        let original = wfm_f32(b":WFM#001", 0.0, 20e-12, &[-1.0, 1.0], "capture");
        let mut truncated = original.clone();
        truncated.remove(truncated.len() - 9);
        assert!(parse_wave(&truncated, "capture.wfm").unwrap_err().contains("truncated"));
        let mut wrong_size = original.clone();
        wrong_size[15] = 2;
        assert!(parse_wave(&wrong_size, "capture.wfm").unwrap_err().contains("bytes per point"));
        let mut negative = original.clone();
        negative[16..20].copy_from_slice(&(-1i32).to_le_bytes());
        assert!(parse_wave(&negative, "capture.wfm").unwrap_err().contains("negative"));
        let mut bad_time = original.clone();
        bad_time[478..486].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(parse_wave(&bad_time, "capture.wfm").unwrap_err().contains("finite"));
        let mut bad_voltage = original;
        bad_voltage[166..174].copy_from_slice(&f64::INFINITY.to_le_bytes());
        assert!(parse_wave(&bad_voltage, "capture.wfm").unwrap_err().contains("finite"));
    }

    #[test]
    fn wave_and_fast_frame_round_trip() {
        let mut m = Model::default();
        m.ch[0].on = true;
        m.ch[2].on = true;
        let mut rec = Record { t0: -1e-3, dt: 1e-6, ch: Default::default(), triggered: true, partial: false, raw: None, la: None, pairs: false };
        rec.ch[0] = vec![0.5, 1.25];
        rec.ch[2] = vec![-1.0, 0.0];
        let b = wave_bin(&rec, &m);
        let (back, n) = parse_wave_bin(&b).unwrap();
        assert_eq!(n, b.len());
        assert_eq!(back.dt, rec.dt);
        assert_eq!(back.ch[0], rec.ch[0]);
        assert_eq!(back.ch[2], rec.ch[2]);
        let ff = ff_bin(&[rec.clone(), rec.clone()], &m);
        let recs = parse_ff(&ff).unwrap();
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[1].ch[0], rec.ch[0]);
        assert_eq!(parse_ff(&b).unwrap().len(), 1);
        let t = mask_text(2, 0.2, 0.4, true, &[1.0, 2.5]);
        let mf = parse_mask(&t).unwrap();
        assert_eq!((mf.src, mf.inv, mf.v), (Some(2), Some(true), vec![1.0, 2.5]));
        let old = parse_mask("1.0\n2.0\n").unwrap();
        assert_eq!(old.v, vec![1.0, 2.0]);
        assert!(old.src.is_none());
    }

    /// Counts in a frame file come from the file. One that does not fit the bytes is an error: before, a sample count
    /// near 2^62 overflowed the offset (a panic, and with panic=abort the end of oos-ui) and a frame count of 2^32 - 1
    /// asked for a list of that many records.
    #[test]
    fn frame_file_counts_beyond_the_bytes_are_refused() {
        let mut huge = b"OOSW".to_vec();
        for x in [1u32, 1] {
            huge.extend_from_slice(&x.to_le_bytes());
        }
        huge.extend_from_slice(&((1u64 << 62) + 1).to_le_bytes());
        huge.extend_from_slice(&1e-9f64.to_le_bytes());
        huge.extend_from_slice(&0f64.to_le_bytes());
        huge.extend_from_slice(&1u32.to_le_bytes());
        huge.extend_from_slice(&[0u8; 16]);
        assert!(parse_wave_bin(&huge).err().unwrap().contains("limit"));
        let over = (MAX_WAVEFORM_SAMPLES as u64 + 1).to_le_bytes();
        huge[12..20].copy_from_slice(&over);
        assert!(parse_ff(&huge).err().unwrap().contains("limit"));
        huge[12..20].copy_from_slice(&5u64.to_le_bytes());
        assert!(parse_wave_bin(&huge).err().unwrap().contains("truncated"));
        huge[12..20].copy_from_slice(&4u64.to_le_bytes());
        assert_eq!(parse_wave_bin(&huge).unwrap().1, huge.len());

        let mut ff = b"OOSF".to_vec();
        ff.extend_from_slice(&1u32.to_le_bytes());
        ff.extend_from_slice(&u32::MAX.to_le_bytes());
        ff.extend_from_slice(&huge);
        assert!(parse_ff(&ff).err().unwrap().contains("truncated"));
        ff[8..12].copy_from_slice(&1u32.to_le_bytes());
        assert_eq!(parse_ff(&ff).unwrap().len(), 1);
    }

    #[test]
    fn checksums() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
        // the eight-byte steps against the plain bitwise definitions, all lengths around the step sizes
        let data: Vec<u8> = (0..20000u32).map(|i| (i.wrapping_mul(2654435761) >> 13) as u8).collect();
        for n in (0..70).chain([5551, 5552, 5553, 11104, 20000]) {
            let d = &data[..n];
            let crc = !d.iter().fold(!0u32, |mut c, &x| {
                c ^= x as u32;
                for _ in 0..8 {
                    c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
                }
                c
            });
            assert_eq!(crc32(d), crc, "CRC of {} bytes", n);
            let (a, b) = d.iter().fold((1u32, 0u32), |(a, b), &x| ((a + x as u32) % 65521, (b + (a + x as u32) % 65521) % 65521));
            assert_eq!(adler32(d), b << 16 | a, "Adler of {} bytes", n);
        }
        assert_eq!(adler32(&[0xff; 100_000]), {
            let (a, b) = (0..100_000).fold((1u64, 0u64), |(a, b), _| ((a + 255) % 65521, (b + (a + 255) % 65521) % 65521));
            (b << 16 | a) as u32
        });
        assert_eq!(crc32(b"IEND"), 0xae42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11e6_0398);
        assert_eq!(adler32(&[]), 1);
    }

    /// Reads a deflate stream bit by bit (RFC 1951 3.1.1): values LSB first, Huffman codes MSB first.
    struct Bits<'a> {
        d: &'a [u8],
        p: usize,
    }
    impl Bits<'_> {
        fn bit(&mut self) -> u32 {
            let b = (self.d[self.p >> 3] >> (self.p & 7)) & 1;
            self.p += 1;
            b as u32
        }
        fn bits(&mut self, n: u32) -> u32 {
            (0..n).fold(0, |v, i| v | self.bit() << i)
        }
    }

    /// One symbol of the fixed Huffman code (RFC 1951 3.2.6): 7 bits 256..279, 8 bits 0..143 and 280..287, 9 bits 144..255.
    fn fixed_symbol(b: &mut Bits) -> u32 {
        let mut c = (0..7).fold(0, |c, _| c << 1 | b.bit());
        if c <= 0b001_0111 {
            return 256 + c;
        }
        c = c << 1 | b.bit();
        if (0b0011_0000..=0b1011_1111).contains(&c) {
            return c - 0b0011_0000;
        }
        if (0b1100_0000..=0b1100_0111).contains(&c) {
            return 280 + c - 0b1100_0000;
        }
        c = c << 1 | b.bit();
        144 + c - 0b1_1001_0000
    }

    /// Independent inflate for what the encoder may write (stored and fixed Huffman blocks, not dynamic ones); returns the
    /// data and the number of bytes the stream took.
    fn inflate(z: &[u8]) -> (Vec<u8>, usize) {
        const LBASE: [u32; 29] = [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
        const LEXTRA: [u32; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
        const DBASE: [u32; 30] = [1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577];
        const DEXTRA: [u32; 30] = [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];
        let mut b = Bits { d: z, p: 0 };
        let mut out: Vec<u8> = Vec::new();
        loop {
            let last = b.bit();
            match b.bits(2) {
                0 => {
                    b.p = (b.p + 7) & !7;
                    let (n, nn) = (b.bits(16), b.bits(16));
                    assert_eq!(n, !nn & 0xffff, "stored block length check");
                    for _ in 0..n {
                        out.push(b.bits(8) as u8);
                    }
                }
                1 => loop {
                    let s = fixed_symbol(&mut b);
                    if s < 256 {
                        out.push(s as u8);
                    } else if s == 256 {
                        break;
                    } else {
                        assert!(s <= 285, "length symbol {}", s);
                        let i = (s - 257) as usize;
                        let len = LBASE[i] + b.bits(LEXTRA[i]);
                        let dc = (0..5).fold(0, |c, _| c << 1 | b.bit()) as usize;
                        assert!(dc < 30, "distance code {}", dc);
                        let dist = (DBASE[dc] + b.bits(DEXTRA[dc])) as usize;
                        assert!(dist <= out.len(), "distance {} reaches before the start ({} bytes so far)", dist, out.len());
                        for _ in 0..len {
                            out.push(out[out.len() - dist]);
                        }
                    }
                },
                t => panic!("block type {} is not written by the encoder", t),
            }
            if last == 1 {
                break;
            }
        }
        (out, (b.p + 7) / 8)
    }

    #[test]
    fn inflate_reference_streams() {
        // Python zlib, Z_FIXED, raw deflate of b"hello hello hello hello": "hello " as literals, then one match (distance 6)
        let z = [0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01];
        let (out, used) = inflate(&z);
        assert_eq!(out, b"hello hello hello hello");
        assert_eq!(used, z.len());
        // a stored block
        let (out, used) = inflate(&[1, 3, 0, 0xfc, 0xff, b'a', b'b', b'c']);
        assert_eq!((out.as_slice(), used), (&b"abc"[..], 8));
    }

    /// Parses the PNG structure: signature, chunk lengths and CRCs, IHDR, the zlib stream (inflated, Adler32) and the
    /// pixels.
    fn decode(png: &[u8]) -> (u32, u32, Vec<u8>) {
        assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
        let mut i = 8;
        let (mut w, mut h, mut z) = (0, 0, Vec::new());
        let mut kinds = Vec::new();
        while i < png.len() {
            let n = u32::from_be_bytes(png[i..i + 4].try_into().unwrap()) as usize;
            let kind = &png[i + 4..i + 8];
            let data = &png[i + 8..i + 8 + n];
            let crc = u32::from_be_bytes(png[i + 8 + n..i + 12 + n].try_into().unwrap());
            assert_eq!(crc, crc32(&png[i + 4..i + 8 + n]), "CRC of {:?}", std::str::from_utf8(kind));
            match kind {
                b"IHDR" => {
                    w = u32::from_be_bytes(data[0..4].try_into().unwrap());
                    h = u32::from_be_bytes(data[4..8].try_into().unwrap());
                    assert_eq!(&data[8..], &[8, 2, 0, 0, 0]);
                }
                b"IDAT" => z.extend_from_slice(data),
                _ => {}
            }
            kinds.push(String::from_utf8_lossy(kind).into_owned());
            i += 12 + n;
        }
        assert_eq!(kinds, ["IHDR", "IDAT", "IEND"]);
        assert_eq!(i, png.len());
        // zlib header, deflate data, Adler32
        assert_eq!(((z[0] as u32) << 8 | z[1] as u32) % 31, 0);
        assert_eq!(z[0] & 0x0f, 8);
        assert_eq!(z[1] & 0x20, 0, "no preset dictionary");
        let (raw, used) = inflate(&z[2..]);
        let p = 2 + used;
        assert_eq!(u32::from_be_bytes(z[p..p + 4].try_into().unwrap()), adler32(&raw));
        assert_eq!(p + 4, z.len());
        // rows: filter 0 + RGB
        let mut pix = Vec::new();
        for row in raw.chunks(w as usize * 3 + 1) {
            assert_eq!(row[0], 0);
            pix.extend_from_slice(&row[1..]);
        }
        assert_eq!(pix.len(), (w * h * 3) as usize);
        (w, h, pix)
    }

    #[test]
    fn ppm_power_on_picture() {
        let mut b = b"P6\n# OOS\n2 1\n255\n".to_vec();
        b.extend_from_slice(&[255, 0, 0, 0, 0, 255]);
        let mut cv = Canvas::new(10, 4);
        assert_eq!(ppm_into(&b, &mut cv), Some((2, 1)));
        assert_eq!((cv.px[(1 * 10 + 4) as usize], cv.px[(1 * 10 + 5) as usize]), (crate::gfx::rgb(255, 0, 0), crate::gfx::rgb(0, 0, 255)));
        assert!(ppm_into(b"P5\n1 1\n255\n\0", &mut cv).is_none());
    }

    #[test]
    fn png_structure_and_pixels() {
        // larger than one stored block (65535 bytes)
        let (w, h) = (301u32, 97u32);
        let px: Vec<u32> = (0..w * h).map(|i| i.wrapping_mul(2654435761u32) & 0xff_ffff).collect();
        let data = png(w, h, &px);
        let (dw, dh, pix) = decode(&data);
        assert_eq!((dw, dh), (w, h));
        for (k, &p) in px.iter().enumerate() {
            assert_eq!(&pix[k * 3..k * 3 + 3], &[(p >> 16) as u8, (p >> 8) as u8, p as u8]);
        }
    }

    /// A picture like the scope screen: black, a grid, filled boxes, a trace, some speckle.
    fn screen_like(w: u32, h: u32) -> Vec<u32> {
        let mut px = vec![0u32; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let mut c = 0;
                if x % 100 == 0 || y % 75 == 0 {
                    c = rgb(60, 60, 60);
                }
                if (40..200).contains(&x) && (30..70).contains(&y) {
                    c = rgb(20, 90, 160);
                }
                let trace = (h as f32 / 2.0 + (x as f32 / 20.0).sin() * h as f32 / 4.0) as u32;
                if y == trace || y == trace + 1 {
                    c = rgb(255, 220, 0);
                }
                if (x * 7 + y * 13) % 997 == 0 {
                    c = rgb(200, 200, 200);
                }
                px[(y * w + x) as usize] = c;
            }
        }
        px
    }

    fn assert_decodes_to(data: &[u8], w: u32, h: u32, px: &[u32]) {
        let (dw, dh, pix) = decode(data);
        assert_eq!((dw, dh), (w, h));
        for (k, &p) in px.iter().enumerate() {
            assert_eq!(&pix[k * 3..k * 3 + 3], &[(p >> 16) as u8, (p >> 8) as u8, p as u8], "pixel {}", k);
        }
    }

    #[test]
    fn png_of_a_screen_round_trips() {
        // runs longer than the 258 a match can take, rows equal to the row above, matches across the end of a row
        for (w, h) in [(1024u32, 600u32), (301, 97), (1, 1), (1, 300), (2, 3), (700, 1)] {
            let px = screen_like(w, h);
            assert_decodes_to(&png(w, h, &px), w, h, &px);
        }
        let flat = vec![rgb(1, 2, 3); 50 * 40];
        assert_decodes_to(&png(50, 40, &flat), 50, 40, &flat);
    }

    #[test]
    fn png_is_much_smaller_than_the_bitmap() {
        let (w, h) = (1024u32, 600u32);
        let raw = (w * 3 + 1) * h;
        let black = png(w, h, &vec![0u32; (w * h) as usize]);
        assert!(black.len() < raw as usize / 100, "black screen: {} bytes", black.len());
        let screen = png(w, h, &screen_like(w, h));
        assert!(screen.len() < raw as usize / 20, "screen-like picture: {} bytes of {}", screen.len(), raw);
    }

    /// Cross-check with an independent decoder when the host has Python with Pillow (skipped otherwise).
    #[test]
    fn png_opens_in_pillow() {
        let has_pillow = std::process::Command::new("python3").args(["-c", "import PIL"]).output().map_or(false, |o| o.status.success());
        if !has_pillow {
            return eprintln!("python3 with Pillow not found: check skipped");
        }
        let (w, h) = (64u32, 40u32);
        let px: Vec<u32> = (0..w * h).map(|i| rgb((i % w) * 4, (i / w) * 6, 0x80)).collect();
        let dir = std::env::temp_dir().join(format!("oos-ui-png-{}", std::process::id()));
        let path = dir.join("t.png");
        write(&path, &png(w, h, &px)).unwrap();
        // a full screen with long runs and repeated rows, compared byte for byte with what Pillow decodes
        let (sw, sh) = (1024u32, 600u32);
        let screen = screen_like(sw, sh);
        let big = dir.join("big.png");
        write(&big, &png(sw, sh, &screen)).unwrap();
        let rgb_bytes: Vec<u8> = screen.iter().flat_map(|&p| [(p >> 16) as u8, (p >> 8) as u8, p as u8]).collect();
        write(&dir.join("big.rgb"), &rgb_bytes).unwrap();
        let script = "import sys\nfrom PIL import Image\nim = Image.open(sys.argv[1]); im.load()\nprint(im.size[0], im.size[1], im.mode, *im.getpixel((5, 7)))\nbig = Image.open(sys.argv[2]); big.load()\nprint(big.tobytes() == open(sys.argv[3], 'rb').read())";
        let out = std::process::Command::new("python3").arg("-c").arg(script).arg(&path).arg(&big).arg(dir.join("big.rgb")).output().unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out.status.success(), "Pillow: {}", String::from_utf8_lossy(&out.stderr));
        let s = String::from_utf8_lossy(&out.stdout);
        assert_eq!(s.trim(), format!("64 40 RGB {} {} 128\nTrue", 5 * 4, 7 * 6));
    }

    /// A real screen (the demo, four channels and the menus of the UI) decodes back to its pixels.
    #[test]
    fn png_of_the_demo_screen_round_trips() {
        let mut ui = crate::ui::Ui::new(crate::wave::Source::Demo { t: 0.0 });
        for c in ["key ch2", "key ch3", "key ch4", "key measure"] {
            crate::cmd::run(&mut ui, c);
        }
        let mut cv = Canvas::new(crate::ui::PANEL_W, crate::ui::PANEL_H);
        for _ in 0..3 {
            ui.step(0.04);
            ui.render(&mut cv);
        }
        assert_decodes_to(&png_of(&cv), cv.w as u32, cv.h as u32, &cv.px);
    }

    /// Time of one PNG of a real screen (the demo source, 4 channels on, as the web remote sends it) and of the
    /// screen-like test picture: `cargo test --release png_encode_time -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn png_encode_time() {
        let mut ui = crate::ui::Ui::new(crate::wave::Source::Demo { t: 0.0 });
        for c in ["key ch2", "key ch3", "key ch4"] {
            crate::cmd::run(&mut ui, c);
        }
        let mut cv = Canvas::new(crate::ui::PANEL_W, crate::ui::PANEL_H);
        for _ in 0..10 {
            ui.step(0.04);
            ui.render(&mut cv);
        }
        let synthetic = screen_like(cv.w as u32, cv.h as u32);
        for (name, px) in [("demo screen", &cv.px), ("screen-like", &synthetic)] {
            let n = 40;
            let mut size = 0;
            let t = std::time::Instant::now();
            for _ in 0..n {
                size = std::hint::black_box(png(cv.w as u32, cv.h as u32, px)).len();
            }
            let per = t.elapsed().as_secs_f64() / n as f64;
            println!("{}: {} x {}, {} bytes, {:.2} ms per PNG", name, cv.w, cv.h, size, per * 1e3);
            assert_decodes_to(&png(cv.w as u32, cv.h as u32, px), cv.w as u32, cv.h as u32, px);
        }
        // what the frame loop does per picture of the web remote now (http.rs `Shots`): one copy of the screen; and
        // what the encoder thread does before it encodes: compare it with the last copy
        let n = 200;
        let mut copy: Vec<u32> = Vec::with_capacity(cv.px.len());
        let t = std::time::Instant::now();
        for _ in 0..n {
            copy.clear();
            copy.extend_from_slice(std::hint::black_box(&cv.px));
        }
        let per_copy = t.elapsed().as_secs_f64() / n as f64;
        let t = std::time::Instant::now();
        let mut same = false;
        for _ in 0..n {
            same = crate::http::same_pixels(std::hint::black_box(&copy), std::hint::black_box(&cv.px));
        }
        let per_cmp = t.elapsed().as_secs_f64() / n as f64;
        assert!(same);
        println!("frame loop: copy of the screen {:.3} ms; encoder thread: comparison {:.3} ms", per_copy * 1e3, per_cmp * 1e3);
    }

    #[test]
    fn next_free_name() {
        let dir = std::env::temp_dir().join(format!("oos-ui-names-{}", std::process::id()));
        assert_eq!(file_name(&next_name(&dir, "shot", "png")), "shot-001.png");
        write(&dir.join("shot-001.png"), b"x").unwrap();
        write(&dir.join("shot-002.png"), b"x").unwrap();
        assert_eq!(file_name(&next_name(&dir, "shot", "png")), "shot-003.png");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod header_import_tests {
    use super::*;
    #[test]
    fn other_waveform_headers_load_and_unknown_bytes_are_refused() {
        let samples = [0.0_f32, 1.0, -0.5, 0.25];
        let isf = isf_i16(0.0, 1e-9, 1e-3, &[0, 1000, -500, 250]);
        let got = parse_wave(&isf, "a.isf").unwrap();
        assert_close(&got[0], 0.0, 1e-9, &samples);

        let trc = wavedesc_i16(0.0, 1e-9, 1e-3, 0.0, &[0, 1000, -500, 250]);
        assert_close(&parse_wave(&trc, "a.trc").unwrap()[0], 0.0, 1e-9, &samples);

        let ag = ag_f32(-1e-6, 2e-9, &samples, "lane");
        let got = parse_wave(&ag, "a.bin").unwrap();
        assert_eq!(got[0].label, "lane");
        assert_close(&got[0], -1e-6, 2e-9, &samples);

        let tr0 = tr0_f32(0.0, 1e-9, &samples);
        assert_close(&parse_wave(&tr0, "a.tr0").unwrap()[0], 0.0, 1e-9, &samples);
        let mut ascii = String::from("00020000000000009601*x ");
        for name in ["TIME", "v(1)"] {
            ascii.push_str(&format!("{name:<16}"));
        }
        ascii.push_str("$&%#");
        for (k, x) in samples.iter().enumerate() {
            ascii.push_str(&format!("{:11.4e}{:11.4e}", k as f64 * 1e-9, x));
        }
        ascii.push_str(&format!("{:11.4e}", 1.0e30));
        assert_close(&parse_wave(ascii.as_bytes(), "c.tr0").unwrap()[0], 0.0, 1e-9, &samples);
        let raw = raw_f64(0.0, 1e-9, &samples);
        assert_close(&parse_wave(&raw, "b.tr0").unwrap()[0], 0.0, 1e-9, &samples);

        let wfmx = wfmx_f32(-1e-6, 1e-9, &samples);
        assert_close(&parse_wave(&wfmx, "a.wfmx").unwrap()[0], -1e-6, 1e-9, &samples);

        assert!(parse_wave(b"\x00\x01\x02\x03\xff", "no.bin").is_err());
        assert!(parse_wave(b"not a waveform\n", "no.csv").unwrap_err().contains("no waveform"));
        assert!(parse_wave(b"AGxx,1,2\n3,4,5\n", "no.bin").is_err());
    }

    #[test]
    fn header_import_isf_block_digits_and_point_counts_are_checked() {
        for byte in 0..=u8::MAX {
            let mut b = b":WFMPRE:BYT_NR 1;NR_PT 2;XINCR 1e-9;:CURVE #".to_vec();
            b.push(byte);
            b.extend_from_slice(b"000000000");
            assert!(parse_wave(&b, "bad.isf").is_err(), "block byte {byte}");
        }
        for field in ["NR_PT -1", "NR_PT 2.5", "NR_PT 18446744073709551615", "BYT_NR 1.5", "YMULT wrong"] {
            let b = format!(":WFMPRE:BYT_NR 1;XINCR 1e-9;{field};:CURVE #12\0\x01");
            assert!(parse_wave(b.as_bytes(), "bad.isf").is_err(), "{field}");
        }
        assert!(parse_wave(b":WFMPRE:BYT_NR 1;NR_PT 3;XINCR 1;:CURVE #12\0\x01", "short.isf")
            .unwrap_err().contains("truncated"));
    }

    #[test]
    fn header_import_isf_envelope_preserves_pairs_and_requires_complete_data() {
        let header = b":WFMPRE:BYT_NR 1;NR_PT 2;PT_FMT ENV;XINCR 1;:CURVE #14";
        let mut b = header.to_vec();
        b.extend([-2i8 as u8, 2, -1i8 as u8, 1]);
        let traces = parse_wave(&b, "envelope.isf").unwrap();
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].v, [-2.0, -1.0]);
        assert_eq!(traces[1].v, [2.0, 1.0]);
        let short = b":WFMPRE:BYT_NR 1;NR_PT 2;PT_FMT ENV;XINCR 1;:CURVE #13\0\x01\x02";
        assert!(parse_wave(short, "short.isf").unwrap_err().contains("truncated"));
    }

    #[test]
    fn header_import_fixed_width_unicode_is_safe_and_invalid_data_is_refused() {
        let text = format!("0002{}x$&%#", "é".repeat(16));
        let (nvars, names) = post_vars(&text).unwrap();
        assert_eq!((nvars, names.len()), (2, 2));
        let text = format!("00020000000000009601*x $&%#{}", "é".repeat(16));
        assert!(parse_wave(text.as_bytes(), "bad.tr0").is_err());
    }

    #[test]
    fn header_import_transient_requires_every_time_sample_to_be_uniform() {
        for time in [3.0, 1.0, f64::NAN, f64::INFINITY] {
            let mut b = raw_f64(0.0, 1.0, &[10.0, 20.0, 30.0, 40.0]);
            let start = b.len() - 4 * 16;
            b[start + 2 * 16..start + 2 * 16 + 8].copy_from_slice(&time.to_le_bytes());
            assert!(parse_wave(&b, "adaptive.tr0").unwrap_err().contains("nonuniform"));
        }
        let values = [10.0, 20.0, 30.0, 40.0];
        let b = raw_f64(-2e-6, 2e-9, &values);
        assert_close(&parse_wave(&b, "regular.tr0").unwrap()[0], -2e-6, 2e-9, &values);
    }

    #[test]
    fn header_import_wfmx_preserves_whitespace_bytes_at_binary_boundaries() {
        for first in [0x3f800020, 0x3f80000a, 0x3f80000d] {
            for last in [0x0a800000, 0x0d800000] {
                let values = [f32::from_bits(first), f32::from_bits(last)];
                let b = wfmx_f32(0.0, 1.0, &values);
                let trace = parse_wave(&b, "bytes.wfmx").unwrap().remove(0);
                assert_eq!(trace.v.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), [first, last]);
                let mut crlf = b.clone();
                let separator = crlf.len() - 8 - 1;
                crlf.insert(separator, b'\r');
                assert_eq!(parse_wave(&crlf, "crlf.wfmx").unwrap()[0].v, values);
            }
        }
    }

    #[test]
    fn header_import_wfmx_embedded_payload_uses_exact_length_not_binary_tag_bytes() {
        let values = [f32::from_bits(0x3f800020), 2.0];
        let mut b = b"<?xml?><DataFile><FileType>WaveformInternal</FileType><Points>2</Points><XOrg>0</XOrg><XInc>1</XInc><Data>".to_vec();
        for value in values { b.extend(value.to_le_bytes()); }
        b.extend_from_slice(b"</Data></DataFile>");
        assert_eq!(parse_wave(&b, "embedded.wfmx").unwrap()[0].v, values);
        let mut b = b"<Data>".to_vec();
        let payload = b"</Data>123456789";
        assert_eq!(payload.len(), 16);
        b.extend_from_slice(payload);
        b.extend_from_slice(b"</Data>");
        assert_eq!(wfmx_payload(&b, 4).unwrap(), payload);
        assert!(wfmx_payload(&b, usize::MAX).is_err());
    }

    #[test]
    fn header_import_wfmx_separator_cannot_replace_a_missing_payload_byte() {
        let mut b = wfmx_f32(0.0, 1.0, &[1.0, 2.0]);
        b.pop();
        assert!(parse_wave(&b, "short.wfmx").unwrap_err().contains("truncated"));
    }

    #[test]
    fn header_import_wfmx_checks_the_whole_explicit_time_axis() {
        let mut payload = Vec::new();
        for value in [0.0f64, 1.0, 3.0, 10.0, 20.0, 30.0] { payload.extend(value.to_le_bytes()); }
        assert!(wfmx_volts(&payload, 3, 0.0, 1.0).is_err());
        payload[16..24].copy_from_slice(&2.0f64.to_le_bytes());
        assert_eq!(wfmx_volts(&payload, 3, 0.0, 1.0).unwrap(), [10.0, 20.0, 30.0]);
    }

    #[test]
    fn header_import_wfmx_f32_axis_accepts_quantization_without_guessing_other_layouts() {
        let n = 100_000;
        let t0 = -37e-6;
        let dt = 1e-9;
        let xs: Vec<f32> = (0..n).map(|i| (t0 + i as f64 * dt) as f32).collect();
        let ys: Vec<f32> = (0..n).map(|i| ((i % 37) as f32 - 18.0) / 19.0).collect();
        for interleaved in [false, true] {
            let mut payload = Vec::with_capacity(n * 8);
            if interleaved {
                for i in 0..n { payload.extend(xs[i].to_le_bytes()); payload.extend(ys[i].to_le_bytes()); }
            } else {
                for x in &xs { payload.extend(x.to_le_bytes()); }
                for y in &ys { payload.extend(y.to_le_bytes()); }
            }
            assert_eq!(wfmx_volts(&payload, n, t0, dt).unwrap(), ys);
            let at = if interleaved { (n - 1) * 8 } else { (n - 1) * 4 };
            payload[at..at + 4].copy_from_slice(&(xs[n - 1] + 10.0 * dt as f32).to_le_bytes());
            assert!(wfmx_volts(&payload, n, t0, dt).unwrap_err().contains("ambiguous"));
        }
        let implicit_f64: Vec<u8> = [1.0f64, 2.0, 3.0].into_iter().flat_map(f64::to_le_bytes).collect();
        assert!(wfmx_volts(&implicit_f64, 3, 0.0, 1.0).is_err());
    }

    #[test]
    fn header_import_long_tr0_f32_times_keep_source_precision_and_reject_adaptive_axis() {
        let values: Vec<f32> = (0..100_000).map(|i| (i % 7) as f32).collect();
        let b = tr0_f32(-37e-6, 1e-9, &values);
        let got = parse_wave(&b, "long.tr0").unwrap();
        assert_eq!(got[0].v, values);
        assert!((got[0].dt - 1e-9).abs() < 1e-16);
        let mut cols = Vec::new();
        for i in 0..1000 { cols.extend([(i as f64 * 1e-9) as f32 as f64, i as f64]); }
        cols[998 * 2] += 5e-9;
        assert!(traces_from_columns(&cols, 2, &[], true).unwrap_err().contains("nonuniform"));
    }

    #[test]
    fn header_import_known_formats_tolerate_all_truncation_boundaries_without_panicking() {
        let values = [0.0, 1.0, -0.5, 0.25];
        let fixtures = [
            isf_i16(0.0, 1e-9, 1e-3, &[0, 1000, -500, 250]),
            wavedesc_i16(0.0, 1e-9, 1e-3, 0.0, &[0, 1000, -500, 250]),
            ag_f32(0.0, 1e-9, &values, "lane"),
            tr0_f32(0.0, 1e-9, &values), raw_f64(0.0, 1e-9, &values), wfmx_f32(0.0, 1e-9, &values),
        ];
        for b in fixtures {
            for end in 0..b.len() { let _ = parse_wave(&b[..end], "header.bin"); }
            assert_eq!(parse_wave(&b, "header.bin").unwrap()[0].v.len(), 4);
        }
    }

    fn assert_close(t: &WaveTrace, t0: f64, dt: f64, v: &[f32]) {
        let tol = dt.abs() * 1e-5 + 1e-15;
        assert!((t.t0 - t0).abs() <= tol, "t0 {} {}", t.t0, t0);
        assert!((t.dt - dt).abs() <= tol, "dt {} {}", t.dt, dt);
        assert_eq!(t.v.len(), v.len());
        for (a, b) in t.v.iter().zip(v) {
            assert!((a - b).abs() < 1e-5, "{a} {b}");
        }
    }

    fn isf_i16(t0: f64, dt: f64, ymult: f64, codes: &[i16]) -> Vec<u8> {
        let mut curve = Vec::new();
        for c in codes {
            curve.extend_from_slice(&c.to_le_bytes());
        }
        let len = curve.len().to_string();
        let mut b = format!(":WFMPRE:BYT_NR 2;ENCDG BIN;BN_FMT RI;BYT_OR LSB;NR_PT {};PT_FMT Y;XINCR {dt:.9e};XZERO {t0:.9e};PT_OFF 0;YMULT {ymult:.9e};YOFF 0;YZERO 0;WFID \"lane\";:CURVE #{}{len}", codes.len(), len.len()).into_bytes();
        b.extend(curve);
        b
    }

    fn wavedesc_i16(t0: f64, dt: f64, gain: f32, off: f32, codes: &[i16]) -> Vec<u8> {
        let desc = 346usize;
        let mut b = vec![0u8; desc + codes.len() * 2];
        b[..8].copy_from_slice(b"WAVEDESC");
        b[32] = 1;
        b[34] = 1;
        b[36..40].copy_from_slice(&(desc as u32).to_le_bytes());
        b[60..64].copy_from_slice(&((codes.len() * 2) as u32).to_le_bytes());
        b[116..120].copy_from_slice(&(codes.len() as u32).to_le_bytes());
        b[96..98].copy_from_slice(b"C1");
        b[156..160].copy_from_slice(&gain.to_le_bytes());
        b[160..164].copy_from_slice(&off.to_le_bytes());
        b[176..180].copy_from_slice(&(dt as f32).to_le_bytes());
        b[180..188].copy_from_slice(&t0.to_le_bytes());
        for (k, c) in codes.iter().enumerate() {
            b[desc + k * 2..desc + k * 2 + 2].copy_from_slice(&c.to_le_bytes());
        }
        b
    }

    fn ag_f32(t0: f64, dt: f64, v: &[f32], label: &str) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"AG10");
        let size_at = b.len();
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut h = vec![0u8; 140];
        h[..4].copy_from_slice(&140u32.to_le_bytes());
        h[4..8].copy_from_slice(&1u32.to_le_bytes());
        h[8..12].copy_from_slice(&1u32.to_le_bytes());
        h[12..16].copy_from_slice(&(v.len() as u32).to_le_bytes());
        h[32..40].copy_from_slice(&dt.to_le_bytes());
        h[40..48].copy_from_slice(&t0.to_le_bytes());
        h[112..112 + label.len()].copy_from_slice(label.as_bytes());
        b.extend(h);
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&((v.len() * 4) as u32).to_le_bytes());
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
        let n = b.len() as u32;
        b[size_at..size_at + 4].copy_from_slice(&n.to_le_bytes());
        b
    }

    fn tr0_block(payload: &[u8]) -> Vec<u8> {
        let mut b = Vec::new();
        b.extend_from_slice(b"\x04\x00\x00\x00\x00\x00\x00\x00\x04\x00\x00\x00");
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b.extend_from_slice(payload);
        b.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        b
    }

    fn tr0_f32(t0: f64, dt: f64, v: &[f32]) -> Vec<u8> {
        let mut header = String::from("00020000000000009601*x ");
        for name in ["TIME", "v(1)"] {
            header.push_str(&format!("{name:<16}"));
        }
        header.push_str("$&%#");
        let mut data = Vec::new();
        for (k, x) in v.iter().enumerate() {
            let t = (t0 + k as f64 * dt) as f32;
            data.extend_from_slice(&t.to_le_bytes());
            data.extend_from_slice(&x.to_le_bytes());
        }
        data.extend_from_slice(&1.0e30_f32.to_le_bytes());
        let mut b = tr0_block(header.as_bytes());
        b.extend(tr0_block(&data));
        b
    }

    fn raw_f64(t0: f64, dt: f64, v: &[f32]) -> Vec<u8> {
        let mut b = format!("Title: t\nDate: today\nPlotname: Transient Analysis\nFlags: real\nNo. Variables: 2\nNo. Points: {}\nVariables:\n\t0\ttime\ttime\n\t1\tv(1)\tvoltage\nBinary:\n", v.len()).into_bytes();
        for (k, x) in v.iter().enumerate() {
            b.extend_from_slice(&(t0 + k as f64 * dt).to_le_bytes());
            b.extend_from_slice(&(*x as f64).to_le_bytes());
        }
        b
    }

    fn wfmx_f32(t0: f64, dt: f64, v: &[f32]) -> Vec<u8> {
        let mut b = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<DcaFile><Header><FileType>WaveformInternal</FileType></Header><Points>{}</Points><XOrg>{t0}</XOrg><XInc>{dt}</XInc><Data Lengths=\"{}, 1\"></Data></DcaFile>\n", v.len(), v.len()).into_bytes();
        for x in v {
            b.extend_from_slice(&x.to_le_bytes());
        }
        b
    }


    // Break it catches (rdtsc in the forum): a setup naming a protocol this build does not have, or a key of a newer
    // build, failing to load as a whole
    #[test]
    fn a_setup_loads_what_it_can() {
        let mut m = Model::default();
        m.dec_on = true;
        m.tscale = 5;
        // unknown keys were always passed over; a protocol this build lacks and a value out of range were not
        let text = setup_text(&m).replace("dec_proto = ", "dec_proto = NoSuchDecoder # was ") + "avg_hw = maybe\n";
        assert!(parse_setup(&text).is_err(), "strict parse names the first bad line");
        let (back, bad) = parse_setup_lenient(&text);
        assert_eq!(back.tscale, 5, "the rest of the setup stands");
        assert!(!back.dec_on, "the decoder without its protocol is off");
        assert!(bad.len() == 2 && bad[0].contains("dec_proto") && bad[1].contains("avg_hw"), "{:?}", bad);
        assert_eq!(parse_setup_lenient(&setup_text(&m)).1, Vec::<String>::new());
    }
}

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
        if !force && self.t.elapsed().as_secs() < 2 { return; }
        self.t = std::time::Instant::now();
        let text = setup_text(&ui.m);
        if text == self.last { return; }
        if let Some(dir) = self.path.parent() { let _ = std::fs::create_dir_all(dir); }
        let tmp = self.path.with_extension("setup.new");
        if std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, &self.path)).is_ok() { self.last = text; }
    }
}

#[cfg(test)]
mod waveform_record_tests {
    use super::*;

    fn record(magic: &[u8; 8], header: &str, samples: &[u8]) -> Vec<u8> {
        let mut b = magic.to_vec();
        b.extend_from_slice(&(header.len() as u32).to_le_bytes());
        b.extend_from_slice(header.as_bytes());
        b.extend_from_slice(samples);
        b
    }

    fn words(counts: &[u16]) -> Vec<u8> {
        counts.iter().flat_map(|c| c.to_le_bytes()).collect()
    }

    #[test]
    fn a_word_record_scales_counts_and_places_the_trigger() {
        // the nested preamble repeats the scaling keys with other values: only the members of the header count
        let header = r#"{"version":1,"sampleType":"uint16","nPoints":4,"xincr":2e-9,"xzero":-1e-6,"ptOff":10,"yoff":32768,"ymult":0.001,"yzero":0.5,"yunit":"V","channel":"CH3","preamble":{"xincr":9,"nPoints":99,"yoff":[1,2,{"ymult":7}],"note":"a } and a , in \"text\""},"nativeByteOrder":"little"}"#;
        let bytes = record(b"PWWF0001", header, &words(&[32768, 33768, 31768, 32769, 7]));
        let traces = parse_wave(&bytes, "ch3.pwwf").unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!(t.label, "CH3");
        assert_eq!(t.v.len(), 4, "samples past nPoints are not read");
        for (got, want) in t.v.iter().zip([0.5_f32, 1.5, -0.5, 0.501]) {
            assert!((got - want).abs() < 1e-6, "{got} vs {want}");
        }
        assert!((t.dt - 2e-9).abs() < 1e-21);
        assert!((t.t0 - (-1e-6 - 10.0 * 2e-9)).abs() < 1e-18);
    }

    #[test]
    fn every_sample_type_and_the_older_magic_are_read() {
        let int8 = record(b"TKWF0001", r#"{"sampleType":"int8","nPoints":3,"xincr":1e-6,"ymult":0.5}"#, &[0xfe, 0, 4]);
        assert_eq!(parse_wave(&int8, "a.tkwf").unwrap()[0].v, [-1.0, 0.0, 2.0]);
        let int16 = record(b"PWWF0001", r#"{"sampleType":"int16","nPoints":2,"xincr":1e-6,"ymult":2}"#, &words(&[0xffff, 3]));
        assert_eq!(parse_wave(&int16, "a.pwwf").unwrap()[0].v, [-2.0, 6.0]);
        let floats: Vec<u8> = [0.25_f32, -1.5].iter().flat_map(|x| x.to_le_bytes()).collect();
        let float32 = record(b"PWWF0001", r#"{ "sampleType" : "float32" , "nPoints" : 2 , "xincr" : 1e-3 , "ptOff" : null }"#, &floats);
        let t = &parse_wave(&float32, "a.pwwf").unwrap()[0];
        assert_eq!(t.v, [0.25, -1.5]);
        assert_eq!((t.t0, t.label.as_str()), (0.0, "Record"));
    }

    #[test]
    fn a_damaged_record_is_refused_with_its_reason() {
        let good = r#"{"sampleType":"uint16","nPoints":3,"xincr":1e-6}"#;
        let refused = |bytes: Vec<u8>| parse_wave(&bytes, "x.pwwf").map(|_| ()).unwrap_err();
        assert_eq!(refused(record(b"PWWF0001", good, &words(&[1, 2]))), "truncated waveform record");
        let mut cut = record(b"PWWF0001", good, &[]);
        cut.truncate(20);
        assert_eq!(refused(cut), "truncated waveform record");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","nPoints":3"#, &words(&[1, 2, 3]))), "the waveform record header is not readable");
        assert_eq!(refused(record(b"PWWF0001", r#"["sampleType","uint16"]"#, &words(&[1, 2, 3]))), "the waveform record header is not readable");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","xincr":1e-6}"#, &words(&[1, 2, 3]))), "the waveform record header is not readable");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","nPoints":3,"xincr":0}"#, &words(&[1, 2, 3]))), "that file holds no waveform");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"uint16","nPoints":1,"xincr":1e-6}"#, &words(&[1]))), "that file holds no waveform");
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"float64","nPoints":3,"xincr":1e-6}"#, &[0; 24])), "waveform record sample type \"float64\" is not read");
        let big = format!(r#"{{"sampleType":"int8","nPoints":{},"xincr":1e-6}}"#, MAX_WAVEFORM_SAMPLES + 1);
        assert_eq!(refused(record(b"PWWF0001", &big, &[0; 8])), "waveform exceeds the 32-million-sample import limit");
        let nan: Vec<u8> = [f32::NAN, 0.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(refused(record(b"PWWF0001", r#"{"sampleType":"float32","nPoints":2,"xincr":1e-6}"#, &nan)), "the waveform record holds samples that are not numbers");
    }

    #[test]
    fn json_members_keeps_nested_values_whole_and_unescapes_strings() {
        let members = json_members(r#" { "a" : "x\n\"y\" é" , "b" : -1.5e3 , "c" : { "a" : [ 1 , "}" ] } , "d" : [ ] , "e" : true } "#).unwrap();
        let get = |key: &str| members.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).unwrap();
        assert_eq!(get("a"), "x\n\"y\" \u{e9}");
        assert_eq!(get("b"), "-1.5e3");
        assert_eq!(get("c"), r#"{ "a" : [ 1 , "}" ] }"#);
        assert_eq!((get("d"), get("e")), ("[ ]", "true"));
        assert_eq!(json_members("{}").unwrap().len(), 0);
        for bad in ["", "[]", "{", r#"{"a"}"#, r#"{"a":}"#, r#"{,"a":1}"#, r#"{"a":1} x"#, r#"{"a":"unterminated}"#, r#"{"a":1]}"#] {
            assert!(json_members(bad).is_none(), "{bad}");
        }
    }
}

#[cfg(test)]
mod rigol_bin_tests {
    use super::*;

    /// One waveform of an `RG` file: the 140-byte waveform header, the 16-byte buffer header and the buffer.
    fn waveform(wave_type: u32, buffer_type: u16, bytes_per_point: u16, dt: f64, origin: f64, label: &str, data: &[u8]) -> Vec<u8> {
        let mut h = vec![0u8; 140];
        h[..4].copy_from_slice(&140u32.to_le_bytes());
        h[4..8].copy_from_slice(&wave_type.to_le_bytes());
        h[8..12].copy_from_slice(&1u32.to_le_bytes());
        h[12..16].copy_from_slice(&((data.len() / bytes_per_point as usize) as u32).to_le_bytes());
        h[32..40].copy_from_slice(&dt.to_le_bytes());
        h[40..48].copy_from_slice(&origin.to_le_bytes());
        h[48..52].copy_from_slice(&2u32.to_le_bytes());
        h[52..56].copy_from_slice(&1u32.to_le_bytes());
        h[88..88 + 12].copy_from_slice(b"DHO924S:DHO9");
        h[112..112 + label.len()].copy_from_slice(label.as_bytes());
        h.extend_from_slice(&16u32.to_le_bytes());
        h.extend_from_slice(&buffer_type.to_le_bytes());
        h.extend_from_slice(&bytes_per_point.to_le_bytes());
        h.extend_from_slice(&(data.len() as u64).to_le_bytes());
        h.extend_from_slice(data);
        h
    }

    fn file(waveforms: &[Vec<u8>]) -> Vec<u8> {
        let mut b = b"RG01".to_vec();
        let size = 16 + waveforms.iter().map(Vec::len).sum::<usize>();
        b.extend_from_slice(&(size as u64).to_le_bytes());
        b.extend_from_slice(&(waveforms.len() as u32).to_le_bytes());
        for w in waveforms {
            b.extend_from_slice(w);
        }
        b
    }

    fn floats(v: &[f32]) -> Vec<u8> {
        v.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn every_analog_channel_is_a_trace_and_the_record_starts_before_the_trigger() {
        let a = [0.0_f32, 0.5, 1.0, -0.25];
        let b = [3.3_f32, 3.2, 3.1, 3.0];
        let bytes = file(&[
            waveform(1, 1, 4, 4e-7, 2e-3, "CH1", &floats(&a)),
            waveform(6, 5, 1, 4e-7, 2e-3, "D0", &[0, 1, 1, 0]),
            waveform(1, 1, 4, 4e-7, 2e-3, "CH3", &floats(&b)),
        ]);
        let traces = parse_wave(&bytes, "capture.bin").unwrap();
        assert_eq!(traces.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH3"]);
        assert_eq!((traces[0].v.as_slice(), traces[1].v.as_slice()), (&a[..], &b[..]));
        for t in &traces {
            assert!((t.dt - 4e-7).abs() < 1e-18);
            // X Origin holds the distance before the trigger: the first point is at its negative
            assert!((t.t0 + 2e-3).abs() < 1e-15, "{}", t.t0);
        }
    }

    #[test]
    fn a_file_of_logic_channels_or_a_cut_file_is_refused_with_its_reason() {
        let logic = file(&[waveform(6, 5, 1, 1e-6, 0.0, "D0", &[0, 1, 1, 0])]);
        assert_eq!(parse_wave(&logic, "la.bin").map(|_| ()).unwrap_err(), "that file holds logic channels only, and they are not read");
        let mut cut = file(&[waveform(1, 1, 4, 1e-6, 0.0, "CH1", &floats(&[1.0, 2.0, 3.0]))]);
        cut.truncate(cut.len() - 5);
        assert_eq!(parse_wave(&cut, "cut.bin").map(|_| ()).unwrap_err(), "truncated waveform");
        // a buffer size past the end of the file, in the upper half of its eight bytes too
        let mut huge = file(&[waveform(1, 1, 4, 1e-6, 0.0, "CH1", &floats(&[1.0, 2.0]))]);
        let at = 16 + 140 + 8;
        huge[at..at + 8].copy_from_slice(&u64::MAX.to_le_bytes());
        assert_eq!(parse_wave(&huge, "huge.bin").map(|_| ()).unwrap_err(), "truncated waveform");
        huge[at..at + 8].copy_from_slice(&(1u64 << 40).to_le_bytes());
        assert_eq!(parse_wave(&huge, "huge.bin").map(|_| ()).unwrap_err(), "truncated waveform");
    }

    #[test]
    fn the_other_maker_of_the_format_still_starts_at_its_x_origin() {
        // cookie AG: a 12-byte file header and a 12-byte buffer header, and X Origin is the first point
        let v = [1.0_f32, 2.0, 3.0];
        let mut b = b"AG10".to_vec();
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut h = waveform(1, 1, 4, 1e-9, -5e-6, "lane", &[]);
        h.truncate(140);
        h[12..16].copy_from_slice(&3u32.to_le_bytes());
        b.extend(h);
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&4u16.to_le_bytes());
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend(floats(&v));
        let t = &parse_wave(&b, "k.bin").unwrap()[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&v[..], "lane"));
        assert!((t.t0 + 5e-6).abs() < 1e-18);
    }
}

#[cfg(test)]
mod keysight_file_tests {
    use super::*;

    /// The header of the example in Keysight's description of the Y-values file, with these amplitudes.
    fn y_values(extra: &str, values: &str) -> String {
        format!("File Format, WaveformYValues\r\nFormat Version, 2\r\nInstrument, D9300A\r\nSwVersion, P.26.01.214\r\nSerialNumber, MXL3441NC2\r\nDate, 3/30/2026 13:33:17 GMT-06:00\r\n\r\nPoints, 4\r\nSignal Type, Unspecified\r\nSource Name, Channel 1\r\nChannel Noise, 0.002\r\nIntrinsic Jitter, 1E-14\r\nInterpolation Factor, 4\r\n{extra}Bandwidth, 128000000000\r\nX Units, Second\r\nY Units, Volt\r\nData,\r\nfloat\r\n{values}")
    }

    #[test]
    fn a_y_values_file_takes_its_time_scale_from_the_header() {
        let text = y_values("XOrg, -5.0000000000000004E-08\r\nXInc, 9.765625E-13\r\n", "0.248322\r\n0.25\r\n-0.125\r\n1.5E-01\r\n");
        let traces = parse_wave(text.as_bytes(), "ch1.txt").unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!(t.v, [0.248322, 0.25, -0.125, 0.15]);
        assert_eq!((t.t0, t.dt, t.label.as_str()), (-5.0000000000000004E-08, 9.765625E-13, "Channel 1"));
        // version 1 has no number-type line; a byte-order mark before the first line is not part of it
        let v1 = format!("{}{}", char::from_u32(0xfeff).unwrap(), text.replace("float\r\n", "").replace("Format Version, 2", "Format Version, 1"));
        assert_eq!(parse_wave(v1.as_bytes(), "ch1.txt").unwrap()[0].v, [0.248322, 0.25, -0.125, 0.15]);
    }

    #[test]
    fn a_y_values_file_without_a_time_scale_is_refused_with_its_reason() {
        let refused = |text: String| parse_wave(text.as_bytes(), "y.txt").map(|_| ()).unwrap_err();
        assert_eq!(refused(y_values("", "1\r\n2\r\n")), "the Y-values file has no XOrg and XInc in its header");
        assert_eq!(refused(y_values("XOrg, 0\r\nXInc, 0\r\n", "1\r\n2\r\n")), "that file holds no waveform");
        assert_eq!(refused(y_values("XOrg, 0\r\nXInc, 1E-9\r\n", "1\r\n")), "that file holds no waveform");
        // amplitudes alone, the file saved without its header: one column, no time
        assert_eq!(parse_wave(b"0.1\r\n0.2\r\n0.3\r\n", "y.txt").map(|_| ()).unwrap_err(), "that file holds no waveform");
    }

    #[test]
    fn an_xy_values_file_is_read_with_the_name_of_its_source() {
        let text = "File Format, WaveformXYValues\r\nFormat Version, 2\r\nInstrument, D9300A\r\nDate, 3/30/2026 13:33:48 GMT-06:00\r\n\r\nPoints, 3\r\nSource Name, Channel 2\r\nInterpolation Factor, 0\r\nBandwidth, 128000000000\r\nX Units, Second\r\nY Units, Volt\r\nData,\r\ndouble, float\r\n-5.0E-08, 0.248322,\r\n-4.9E-08, 0.25,\r\n-4.8E-08, -0.125,\r\n";
        let traces = parse_wave(text.as_bytes(), "wave.csv").unwrap();
        assert_eq!(traces.len(), 1, "a line that ends with its separator has no column after it");
        let t = &traces[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&[0.248322f32, 0.25, -0.125][..], "Channel 2"));
        assert!((t.t0 + 5.0e-8).abs() < 1e-20 && (t.dt - 1.0e-9).abs() < 1e-18, "{} {}", t.t0, t.dt);
        // a plain time,volts table keeps the name of its file
        assert_eq!(parse_wave(b"0,1\n1,2\n2,3\n", "plain.csv").unwrap()[0].label, "plain.csv");
    }

    #[test]
    fn containers_that_are_not_read_say_so_and_how_to_get_round_it() {
        let hdf5 = [0x89, b'H', b'D', b'F', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];
        assert_eq!(parse_wave(&hdf5, "w.h5").map(|_| ()).unwrap_err(), "HDF5 (.h5) waveform files are not read; save the waveform as BIN (.bin) or CSV");
        let mat = b"MATLAB 5.0 MAT-file, Platform: PCWIN64";
        assert_eq!(parse_wave(mat, "w.mat").map(|_| ()).unwrap_err(), "MATLAB (.mat) waveform files are not read; save the waveform as BIN (.bin) or CSV");
    }

    /// An `AG` file of one waveform with one buffer of this type.
    fn ag(wave_type: u32, buffer_type: u16, bytes_per_point: u16, data: &[u8]) -> Vec<u8> {
        let mut b = b"AG10".to_vec();
        b.extend_from_slice(&0u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        let mut h = vec![0u8; 140];
        h[..4].copy_from_slice(&140u32.to_le_bytes());
        h[4..8].copy_from_slice(&wave_type.to_le_bytes());
        h[8..12].copy_from_slice(&1u32.to_le_bytes());
        h[12..16].copy_from_slice(&((data.len() / bytes_per_point as usize) as u32).to_le_bytes());
        h[32..40].copy_from_slice(&1e-9f64.to_le_bytes());
        h[40..48].copy_from_slice(&(-2e-9f64).to_le_bytes());
        b.extend(h);
        b.extend_from_slice(&12u32.to_le_bytes());
        b.extend_from_slice(&buffer_type.to_le_bytes());
        b.extend_from_slice(&bytes_per_point.to_le_bytes());
        b.extend_from_slice(&(data.len() as u32).to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn the_digital_buffer_of_an_ag_file_is_type_six_and_its_counts_are_not_logic() {
        let refused = |bytes: Vec<u8>| parse_wave(&bytes, "k.bin").map(|_| ()).unwrap_err();
        assert_eq!(refused(ag(6, 6, 1, &[0, 1, 1, 0])), "that file holds logic channels only, and they are not read");
        // type 5 in an AG file is the counts of a histogram: no trace, and not called a logic channel
        let counts: Vec<u8> = [3.0f32, 9.0, 4.0].iter().flat_map(|x| x.to_le_bytes()).collect();
        assert_eq!(refused(ag(5, 5, 4, &counts)), "that file holds no waveform");
        let volts: Vec<u8> = [0.5f32, -0.5, 0.25].iter().flat_map(|x| x.to_le_bytes()).collect();
        let t = &parse_wave(&ag(1, 1, 4, &volts), "k.bin").unwrap()[0];
        assert_eq!((t.v.as_slice(), t.t0, t.dt), (&[0.5f32, -0.5, 0.25][..], -2e-9, 1e-9));
    }
}

// ── Rohde & Schwarz RTO / RTE / RTP: a description file and a data file ──────────────────────────────────────

/// The elements of a description file that carry a `Name`, each with its attributes; the first of a name is kept.
struct RsTags(Vec<(String, Vec<(String, String)>)>);

impl RsTags {
    fn read(xml: &str) -> Self {
        let mut tags: Vec<(String, Vec<(String, String)>)> = Vec::new();
        let mut rest = xml;
        while let Some(open) = rest.find('<') {
            rest = &rest[open + 1..];
            let Some(close) = rest.find('>') else { break };
            let tag = &rest[..close];
            rest = &rest[close + 1..];
            if tag.starts_with('?') || tag.starts_with('!') || tag.starts_with('/') {
                continue;
            }
            let attributes = rs_attributes(tag);
            let Some(name) = attributes.iter().find(|(key, _)| key == "Name").map(|(_, value)| value.clone()) else { continue };
            if !tags.iter().any(|(known, _)| *known == name) {
                tags.push((name, attributes));
            }
        }
        Self(tags)
    }

    fn attribute(&self, name: &str, key: &str) -> Option<&str> {
        let (_, attributes) = self.0.iter().find(|(known, _)| known == name)?;
        attributes.iter().find(|(k, _)| k == key).map(|(_, value)| value.as_str())
    }

    fn value(&self, name: &str) -> Option<&str> {
        self.attribute(name, "Value")
    }

    fn number(&self, name: &str, key: &str) -> Result<f64, String> {
        self.attribute(name, key).and_then(|v| v.trim().parse::<f64>().ok()).filter(|x| x.is_finite())
            .ok_or_else(|| format!("the Rohde & Schwarz description has no usable {name}"))
    }

    fn count(&self, name: &str) -> Result<usize, String> {
        let x = self.number(name, "Value")?;
        if x < 0.0 || x.fract() != 0.0 || x > u32::MAX as f64 {
            return Err(format!("the Rohde & Schwarz description has no usable {name}"));
        }
        Ok(x as usize)
    }
}

/// The `key="value"` pairs of one tag.
fn rs_attributes(tag: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut rest = tag;
    while let Some(equals) = rest.find('=') {
        let key = rest[..equals].split_whitespace().last().unwrap_or("").to_string();
        let after = rest[equals + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            rest = &rest[equals + 1..];
            continue;
        };
        let body = &after[1..];
        let Some(end) = body.find(quote) else { break };
        if !key.is_empty() {
            out.push((key, body[..end].to_string()));
        }
        rest = &body[end + 1..];
    }
    out
}

fn bytes_hold(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|window| window == needle)
}

/// The description file of a Rohde & Schwarz RTO/RTE/RTP waveform: XML that names a signal format and a record
/// length. It has the extension `.bin`, and the samples are in `<name>.Wfm.bin` beside it.
fn looks_rs_description(b: &[u8]) -> bool {
    let head = &b[..b.len().min(1 << 20)];
    let start = head.iter().position(|c| !c.is_ascii_whitespace() && ![0xEFu8, 0xBB, 0xBF].contains(c));
    start.is_some_and(|at| head[at] == b'<') && bytes_hold(head, b"eRS_SIGNAL_FORMAT_") && bytes_hold(head, b"RecordLength")
}

/// The data file of such a waveform, by its name and its first word: the format code.
fn looks_rs_data(b: &[u8], name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".wfm.bin") && b.len() >= 8 && [0u32, 1, 4, 6].contains(&u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// The name a source of such a file is shown by: `eRS_SIGNAL_SOURCE_CH1_TR1` and `C1W1` are CH1; another source
/// keeps its own name.
fn rs_source_label(source: &str) -> String {
    let plain = source.trim().trim_start_matches("eRS_SIGNAL_SOURCE_");
    let digit_after = |mark: &str| plain.find(mark).and_then(|at| plain[at + mark.len()..].chars().next()).filter(|c| ('1'..='8').contains(c));
    match digit_after("CH").or_else(|| if plain.starts_with('C') { digit_after("C") } else { None }) {
        Some(n) => format!("CH{n}"),
        None if plain.is_empty() || plain == "NONE" => "CH1".into(),
        None => plain.chars().take(10).collect(),
    }
}

/// A Rohde & Schwarz RTO/RTE/RTP waveform from its two files. The description gives, by `Name`: `SignalFormat`
/// (INT8BIT, INT16BIT, FLOAT or XYDOUBLEFLOAT), `RecordLength`, the `SignalHardwareRecordLength` rows the data
/// file holds and the `LeadingSettlingSamples` before the record in them, `XStart` and `XStop`, and for integer
/// samples the scaling: volts = raw * StepFactor * VerticalScale / NofQuantisationLevels + VerticalOffset -
/// VerticalPosition * VerticalScale. With `MultiChannelExport` on, the channels switched on in
/// `MultiChannelExportState` (`I_0` to `I_3`) are interleaved row by row, each with its own scale, position and
/// offset. The data file starts with a u32 format code (0, 1, 4 or 6) and a u32 row count, little endian; a row
/// of XYDOUBLEFLOAT starts with its time as an f64.
/// Layout after the RigolWFM project's reader of these files; one acquisition, a normal or averaged trace.
fn parse_rs(xml: &str, data: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let tags = RsTags::read(xml);
    let Some(format) = tags.value("SignalFormat").map(|f| f.trim_start_matches("eRS_SIGNAL_FORMAT_")) else {
        return Err("the Rohde & Schwarz description has no SignalFormat".into());
    };
    let (code, sample, xy): (u32, usize, bool) = match format {
        "INT8BIT" => (0, 1, false),
        "INT16BIT" => (1, 2, false),
        "FLOAT" => (4, 4, false),
        "XYDOUBLEFLOAT" => (6, 4, true),
        other => return Err(format!("Rohde & Schwarz signal format {other} is not read")),
    };
    if let Some(trace) = tags.value("TraceType").map(|t| t.trim_start_matches("eRS_TRACE_TYPE_")) {
        if trace != "NORMAL" && trace != "AVERAGE" {
            return Err(format!("a Rohde & Schwarz {} trace is not read; save a normal trace", trace.to_ascii_lowercase()));
        }
    }
    if tags.value("NumberOfAcquisitions").is_some() && tags.count("NumberOfAcquisitions")? != 1 {
        return Err("the Rohde & Schwarz file holds several acquisitions; save one acquisition".into());
    }
    let record = tags.count("RecordLength")?;
    let leading = if tags.value("LeadingSettlingSamples").is_some() { tags.count("LeadingSettlingSamples")? } else { 0 };
    let rows = if tags.value("SignalHardwareRecordLength").is_some() { tags.count("SignalHardwareRecordLength")? } else { leading + record };
    if record > MAX_WAVEFORM_SAMPLES {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    if record < 2 || leading.checked_add(record).is_none_or(|end| end > rows) {
        return Err("that file holds no waveform".into());
    }
    let (start, stop) = (tags.number("XStart", "Value")?, tags.number("XStop", "Value")?);

    // the columns of a row: one, or the channels of a multi-channel export that are on
    let multi = tags.value("MultiChannelExport").is_some_and(|v| v.ends_with("_ON"));
    let columns: Vec<(Option<&str>, String)> = if multi {
        ["I_0", "I_1", "I_2", "I_3"].iter().copied()
            .filter(|key| tags.attribute("MultiChannelExportState", key).is_some_and(|v| v.ends_with("_ON")))
            .map(|key| (Some(key), rs_source_label(tags.attribute("MultiChannelSource", key).unwrap_or("")))).collect()
    } else {
        vec![(None, rs_source_label(tags.value("Source").unwrap_or("")))]
    };
    if columns.is_empty() {
        return Err("the Rohde & Schwarz description switches no channel on".into());
    }
    let scale = |column: Option<&str>| -> Result<(f32, f32), String> {
        if sample == 4 {
            return Ok((1.0, 0.0)); // float samples are volts
        }
        let levels = tags.number("NofQuantisationLevels", "Value")?;
        let (scale, position, offset, step) = match column {
            Some(key) => (tags.number("MultiChannelVerticalScale", key)?, tags.number("MultiChannelVerticalPosition", key)?,
                tags.number("MultiChannelVerticalOffset", key)?, tags.number("MultiChannelVerticalScale", "StepFactor")?),
            None => (tags.number("VerticalScale", "Value")?, tags.number("VerticalPosition", "Value")?,
                tags.number("VerticalOffset", "Value")?, tags.number("VerticalScale", "StepFactor")?),
        };
        if levels <= 0.0 {
            return Err("the Rohde & Schwarz description has no usable NofQuantisationLevels".into());
        }
        Ok(((step * scale / levels) as f32, (offset - position * scale) as f32))
    };
    let mut scales = Vec::with_capacity(columns.len());
    for (key, _) in &columns {
        scales.push(scale(*key)?);
    }

    if data.len() < 8 {
        return Err("truncated Rohde & Schwarz waveform data".into());
    }
    let found = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    let held = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if found != code || held != rows {
        return Err("the Rohde & Schwarz data file does not belong to this description".into());
    }
    let lead = if xy { 8 } else { 0 };
    let row = lead + columns.len() * sample;
    let body = &data[8..];
    if rows.checked_mul(row).is_none_or(|bytes| bytes > body.len()) {
        return Err("truncated Rohde & Schwarz waveform data".into());
    }
    let window = &body[leading * row..(leading + record) * row];
    let value = |cell: &[u8]| match sample {
        1 => cell[0] as i8 as f32,
        2 => i16::from_le_bytes([cell[0], cell[1]]) as f32,
        _ => f32::from_le_bytes([cell[0], cell[1], cell[2], cell[3]]),
    };
    let (t0, dt) = if xy {
        // each row carries its own time
        let time = |r: usize| {
            let mut at = [0u8; 8];
            at.copy_from_slice(&window[r * row..r * row + 8]);
            f64::from_le_bytes(at)
        };
        (time(0), time(1) - time(0))
    } else {
        (start, (stop - start) / record as f64)
    };
    if !(dt > 0.0) || !dt.is_finite() || !t0.is_finite() {
        return Err("that file holds no waveform".into());
    }
    let mut out = Vec::with_capacity(columns.len());
    for (k, ((_, label), (factor, offset))) in columns.into_iter().zip(scales).enumerate() {
        let at = lead + k * sample;
        let v: Vec<f32> = window.chunks_exact(row).map(|r| value(&r[at..at + sample]) * factor + offset).collect();
        if v.iter().any(|x| !x.is_finite()) {
            return Err("the Rohde & Schwarz data file holds samples that are not numbers".into());
        }
        out.push(WaveTrace { t0, dt, v, label });
    }
    Ok(out)
}

/// A waveform file read where it lies, so that a format of two files finds its other file beside it. A
/// Rohde & Schwarz description (`name.bin`) is read with `name.Wfm.bin`; that data file opened by itself is read
/// with its description. Every other file is `parse_wave` of its bytes.
pub fn parse_wave_at(bytes: &[u8], path: &Path) -> Result<Vec<WaveTrace>, String> {
    let name = file_name(path);
    if looks_rs_description(bytes) {
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let data = ["Wfm.bin", "wfm.bin", "WFM.BIN", "Wfm.BIN"].iter().map(|end| path.with_file_name(format!("{stem}.{end}"))).find(|p| p.is_file())
            .ok_or_else(|| format!("{name} describes a Rohde & Schwarz waveform; its data file {stem}.Wfm.bin is not beside it"))?;
        return parse_rs(&String::from_utf8_lossy(bytes), &read_waveform_file(&data)?);
    }
    if looks_rs_data(bytes, &name) {
        // the name ends with the eight ASCII characters of .wfm.bin, whatever their case
        let stem = &name[..name.len() - 8];
        let description = path.with_file_name(format!("{stem}.bin"));
        return match read_waveform_file(&description) {
            Ok(xml) if looks_rs_description(&xml) => parse_rs(&String::from_utf8_lossy(&xml), bytes),
            _ => Err(format!("{name} is the data of a Rohde & Schwarz waveform; its description {stem}.bin is not beside it")),
        };
    }
    parse_wave(bytes, &name)
}

#[cfg(test)]
mod rohde_schwarz_file_tests {
    use super::*;

    fn prop(name: &str, value: &str) -> String {
        format!("  <Prop Name=\"{name}\" Value=\"{value}\"/>\n")
    }

    /// A description as the scope writes it: a database of named properties.
    fn description(format: &str, record: usize, leading: usize, rows: usize, more: &str) -> String {
        let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<Database FWVersion=\"4.60.1.6\">\n <Group Name=\"WaveformData\">\n");
        xml += &prop("SignalFormat", &format!("eRS_SIGNAL_FORMAT_{format}"));
        xml += &prop("RecordLength", &record.to_string());
        xml += &prop("LeadingSettlingSamples", &leading.to_string());
        xml += &prop("SignalHardwareRecordLength", &rows.to_string());
        xml += &prop("XStart", "-2e-3");
        xml += &prop("XStop", "2e-3");
        xml += &prop("NofQuantisationLevels", "128");
        xml += more;
        xml + " </Group>\n</Database>\n"
    }

    /// One channel: 0.5 V/div with a step factor of 8 over 128 levels is 31.25 mV a step; position 1 div and
    /// offset 0.25 V put a raw 0 at -0.25 V.
    fn one_channel(source: &str) -> String {
        format!("  <Prop Name=\"VerticalScale\" Value=\"0.5\" StepFactor=\"8\"/>\n{}{}{}", prop("VerticalPosition", "1"), prop("VerticalOffset", "0.25"),
            prop("Source", source))
    }

    fn data(code: u32, rows: usize, body: &[u8]) -> Vec<u8> {
        let mut out = code.to_le_bytes().to_vec();
        out.extend_from_slice(&(rows as u32).to_le_bytes());
        out.extend_from_slice(body);
        out
    }

    fn words(raw: &[i16]) -> Vec<u8> {
        raw.iter().flat_map(|x| x.to_le_bytes()).collect()
    }

    #[test]
    fn a_sixteen_bit_record_is_scaled_and_keeps_only_the_rows_of_the_record() {
        // two settling rows before the record and two after it
        let xml = description("INT16BIT", 4, 2, 8, &one_channel("eRS_SIGNAL_SOURCE_CH2_TR1"));
        let traces = parse_rs(&xml, &data(1, 8, &words(&[999, 999, 32, -32, 0, 64, 999, 999]))).unwrap();
        assert_eq!(traces.len(), 1);
        let t = &traces[0];
        assert_eq!((t.v.as_slice(), t.t0, t.dt, t.label.as_str()), (&[0.75f32, -1.25, -0.25, 1.75][..], -2e-3, 1e-3, "CH2"));
    }

    #[test]
    fn an_eight_bit_record_is_scaled_the_same_way() {
        let xml = description("INT8BIT", 3, 0, 3, &one_channel("C1W1"));
        let t = &parse_rs(&xml, &data(0, 3, &[32, 0xE0, 0])).unwrap()[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&[0.75f32, -1.25, -0.25][..], "CH1"));
    }

    #[test]
    fn float_samples_are_volts_as_they_stand() {
        let xml = description("FLOAT", 3, 1, 4, &prop("Source", "eRS_SIGNAL_SOURCE_CH4_TR1"));
        let body: Vec<u8> = [9.0f32, 0.5, -0.5, 0.25].iter().flat_map(|x| x.to_le_bytes()).collect();
        let t = &parse_rs(&xml, &data(4, 4, &body)).unwrap()[0];
        assert_eq!((t.v.as_slice(), t.label.as_str()), (&[0.5f32, -0.5, 0.25][..], "CH4"));
    }

    #[test]
    fn a_file_of_time_and_value_rows_takes_its_time_from_the_rows() {
        let xml = description("XYDOUBLEFLOAT", 3, 0, 3, &prop("Source", "eRS_SIGNAL_SOURCE_CH1_TR1"));
        let mut body = Vec::new();
        for (time, volts) in [(0.5f64, 1.0f32), (0.75, 2.0), (1.0, 3.0)] {
            body.extend_from_slice(&time.to_le_bytes());
            body.extend_from_slice(&volts.to_le_bytes());
        }
        let t = &parse_rs(&xml, &data(6, 3, &body)).unwrap()[0];
        assert_eq!((t.v.as_slice(), t.t0, t.dt), (&[1.0f32, 2.0, 3.0][..], 0.5, 0.25));
    }

    /// Channels 1 and 3 of a multi-channel export, channel 2 off: two columns, each with its own scaling.
    fn two_of_three() -> String {
        let per = |name: &str, a: &str, b: &str, c: &str, more: &str| format!("  <Prop Name=\"{name}\" I_0=\"{a}\" I_1=\"{b}\" I_2=\"{c}\"{more}/>\n");
        description("INT16BIT", 2, 0, 2, &[
            prop("MultiChannelExport", "eRS_ONOFF_ON"),
            per("MultiChannelExportState", "eRS_ONOFF_ON", "eRS_ONOFF_OFF", "eRS_ONOFF_ON", ""),
            per("MultiChannelSource", "eRS_SIGNAL_SOURCE_CH1_TR1", "eRS_SIGNAL_SOURCE_CH2_TR1", "eRS_SIGNAL_SOURCE_CH3_TR1", ""),
            per("MultiChannelVerticalScale", "0.5", "1", "1", " StepFactor=\"8\""),
            per("MultiChannelVerticalPosition", "0", "0", "0", ""),
            per("MultiChannelVerticalOffset", "0", "0", "1", ""),
        ].concat())
    }

    #[test]
    fn the_channels_of_a_multi_channel_export_are_taken_apart_row_by_row() {
        // rows: (CH1, CH3), (CH1, CH3)
        let traces = parse_rs(&two_of_three(), &data(1, 2, &words(&[32, 16, -32, -16]))).unwrap();
        let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
        assert_eq!(seen, [("CH1", &[1.0f32, -1.0][..]), ("CH3", &[2.0f32, 0.0][..])]);
        assert!(traces.iter().all(|t| t.t0 == -2e-3 && t.dt == 2e-3));
    }

    #[test]
    fn what_is_not_one_plain_record_is_refused_by_name() {
        let plain = one_channel("eRS_SIGNAL_SOURCE_CH1_TR1");
        let body = data(1, 3, &words(&[1, 2, 3]));
        let refused = |more: &str| parse_rs(&description("INT16BIT", 3, 0, 3, &format!("{plain}{more}")), &body).unwrap_err();
        assert!(refused(&prop("TraceType", "eRS_TRACE_TYPE_ENVELOPE")).contains("envelope trace is not read"));
        assert!(refused(&prop("NumberOfAcquisitions", "5")).contains("several acquisitions"));
        assert!(parse_rs(&description("INT16BIT", 3, 0, 3, ""), &body).unwrap_err().contains("no usable VerticalScale"));
        assert!(parse_rs(&description("CUSTOM", 3, 0, 3, &plain), &body).unwrap_err().contains("CUSTOM is not read"));
        // a data file of another format, of another length, or cut short
        let good = description("INT16BIT", 3, 0, 3, &plain);
        assert!(parse_rs(&good, &data(0, 3, &words(&[1, 2, 3]))).unwrap_err().contains("does not belong"));
        assert!(parse_rs(&good, &data(1, 4, &words(&[1, 2, 3, 4]))).unwrap_err().contains("does not belong"));
        assert!(parse_rs(&good, &data(1, 3, &words(&[1, 2]))).unwrap_err().contains("truncated"));
        assert!(parse_rs(&good, &[1, 0, 0]).unwrap_err().contains("truncated"));
        // more settling rows than the file has
        assert!(parse_rs(&description("INT16BIT", 3, 2, 3, &plain), &body).unwrap_err().contains("no waveform"));
        assert!(parse_rs(&good, &body).is_ok());
    }

    #[test]
    fn the_first_property_of_a_name_wins_and_either_quote_is_read() {
        let tags = RsTags::read("<a Name='X' Value='1'/><b Name=\"X\" Value=\"2\"/><c Value=\"3\"/><!-- Name=\"Y\" --><d Name = \"Z\" Value = \"a=b\">");
        assert_eq!((tags.value("X"), tags.value("Y"), tags.value("Z"), tags.0.len()), (Some("1"), None, Some("a=b"), 2));
    }

    #[test]
    fn a_source_is_shown_by_its_channel() {
        let shown: Vec<String> = ["eRS_SIGNAL_SOURCE_CH3_TR2", "C2W1", "", "eRS_SIGNAL_SOURCE_NONE", "eRS_SIGNAL_SOURCE_MATH1"].iter().map(|s| rs_source_label(s)).collect();
        assert_eq!(shown, ["CH3", "CH2", "CH1", "CH1", "MATH1"]);
    }

    #[test]
    fn either_file_of_the_pair_opens_the_waveform_and_a_lone_one_says_what_is_missing() {
        let dir = std::env::temp_dir().join(format!("oos-rs-pair-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let xml = description("INT16BIT", 4, 2, 8, &one_channel("eRS_SIGNAL_SOURCE_CH2_TR1"));
        let body = data(1, 8, &words(&[999, 999, 32, -32, 0, 64, 999, 999]));
        let (first, second) = (dir.join("capture.bin"), dir.join("capture.Wfm.bin"));
        std::fs::write(&first, &xml).unwrap();

        // the description by itself: named as one half, with the name of the other
        let alone = parse_wave_at(xml.as_bytes(), &first).unwrap_err();
        assert!(alone.contains("capture.Wfm.bin is not beside it"), "{alone}");
        assert!(parse_wave(xml.as_bytes(), "capture.bin").unwrap_err().contains("open it from its folder"));

        std::fs::write(&second, &body).unwrap();
        for (bytes, path) in [(xml.as_bytes(), &first), (body.as_slice(), &second)] {
            let traces = parse_wave_at(bytes, path).unwrap();
            assert_eq!((traces.len(), traces[0].v.as_slice(), traces[0].label.as_str()), (1, &[0.75f32, -1.25, -0.25, 1.75][..], "CH2"));
        }

        // the data by itself
        std::fs::remove_file(&first).unwrap();
        let alone = parse_wave_at(&body, &second).unwrap_err();
        assert!(alone.contains("its description capture.bin is not beside it"), "{alone}");

        // every other file is read as before
        let csv = dir.join("plain.csv");
        std::fs::write(&csv, "t,v\n0,1\n1e-3,2\n2e-3,3\n").unwrap();
        assert_eq!(parse_wave_at(&std::fs::read(&csv).unwrap(), &csv).unwrap()[0].v, [1.0, 2.0, 3.0]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

// ── Siglent: binary waveform files ───────────────────────────────────────────────────────────────────────────

fn sg_u32(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at.checked_add(4)?).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn sg_f64(b: &[u8], at: usize) -> Option<f64> {
    b.get(at..at.checked_add(8)?).map(|s| {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(s);
        f64::from_le_bytes(raw)
    })
}

/// A value with its unit, 40 bytes: an f64, the index of its prefix (8 is none, 7 milli, 9 kilo, in steps of a
/// thousand from yocto at 0 to yotta at 16) and seven words that describe the unit. Returned without the prefix.
fn sg_value(b: &[u8], at: usize) -> Option<f64> {
    let (value, prefix, unit) = (sg_f64(b, at)?, sg_u32(b, at + 8)?, sg_u32(b, at + 12)?);
    if !value.is_finite() || prefix > 16 || unit > 12 {
        return None;
    }
    let thousands = prefix as i32 - 8;
    // a division by a whole power of ten is exact to the last bit; a product with its inverse is not
    Some(if thousands >= 0 { value * 10f64.powi(3 * thousands) } else { value / 10f64.powi(-3 * thousands) })
}

/// The version of a Siglent binary waveform file: its first word, and a header that holds what that version
/// puts there. Versions 2, 3 and 4 are one fixed header and the channels after it; version 6 is a list of
/// records. The files of older firmware, which start without a version, are not recognized.
fn siglent_version(b: &[u8]) -> Option<u32> {
    let version = sg_u32(b, 0)?;
    let flags = |at: usize| (0..4).all(|k| sg_u32(b, at + 4 * k).is_some_and(|f| f <= 1));
    let flag_byte = |at: usize| b.get(at).is_some_and(|x| *x <= 1);
    let known = match version {
        2 | 3 => b.len() >= 0x800 && flags(0x04) && sg_value(b, 0x198).is_some() && sg_value(b, 0x1EC).is_some() && flag_byte(0x260),
        4 => sg_u32(b, 4).is_some_and(|data| (0x1000..=b.len()).contains(&(data as usize))) && flags(0x08)
            && sg_value(b, 0x19C).is_some() && sg_value(b, 0x1F0).is_some() && flag_byte(0x264) && flag_byte(0x265),
        6 => b.len() >= 108 && u16::from_le_bytes([b[4], b[5]]) >= 108 && sg_u32(b, 0x68).is_some_and(|n| n > 0) && b[8].is_ascii_graphic(),
        _ => false,
    };
    known.then_some(version)
}

/// The unsigned samples of a block, `width` bytes each.
fn sg_codes(block: &[u8], width: usize, big: bool) -> impl Iterator<Item = f64> + '_ {
    block.chunks_exact(width).map(move |c| match (width, big) {
        (1, _) => c[0] as f64,
        (2, false) => u16::from_le_bytes([c[0], c[1]]) as f64,
        (2, true) => u16::from_be_bytes([c[0], c[1]]) as f64,
        _ => u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as f64,
    })
}

/// A Siglent binary waveform file (SDS800X HD to SDS7000A, SDS2000X Plus, SDS5000X, SDS6000). Addresses are
/// those of versions 2 and 3; version 4 has the address of its data at 0x04 and every later field 4 bytes on.
/// 0x04 four channels on; 0x14 volts per division and 0xB4 vertical offset, a 40-byte value each; 0x198 time per
/// division; 0x1C0 delay; 0x1E8 the samples of a channel; 0x1EC sample rate; 0x240 probe factors (f64);
/// 0x260 one byte, 1 for 16-bit samples; 0x261 one byte, 1 for the high byte first; 0x268 horizontal divisions;
/// 0x26C codes per division. The channels that are on follow from 0x800, one after the other, unsigned:
/// volts = ((code - centre) * volts per division / codes per division - offset) * probe, the centre 128 or 32768,
/// and the first sample is at -(time per division * divisions / 2) - delay.
/// Layout from Siglent's "How to Extract Data from the Binary File"; the sign of the offset and the place of the
/// probe factor from the RigolWFM project, which found the document's own formula wrong on SDS800X HD files.
/// Checked on that project's SDS814X HD files: channels with a 1x and a 10x probe, a math trace and a zoom save.
/// Digital data, which follows the math traces, and channels 5 to 8 are not read.
fn parse_siglent(b: &[u8], version: u32) -> Result<Vec<WaveTrace>, String> {
    if version == 6 {
        return parse_siglent_records(b);
    }
    let short = || "truncated Siglent waveform file".to_string();
    let shift = if version == 4 { 4 } else { 0 };
    let word = |at: usize| sg_u32(b, at + shift).ok_or_else(short);
    let value = |at: usize, what: &str| sg_value(b, at + shift).ok_or_else(|| format!("the Siglent file has no usable {what}"));
    let data = if version == 4 { sg_u32(b, 4).ok_or_else(short)? as usize } else { 0x800 };

    let points = word(0x1E8)? as usize;
    if points > MAX_WAVEFORM_SAMPLES {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    let rate = value(0x1EC, "sample rate")?;
    // a save of a math trace alone has no channel on, and need not say how long a channel is
    let channels = (0..4).any(|k| sg_u32(b, 0x04 + shift + 4 * k) == Some(1));
    if channels && (points < 2 || !(rate > 0.0)) {
        return Err("that file holds no waveform".into());
    }
    let flag_byte = |at: usize| b.get(at + shift).copied().ok_or_else(short);
    let (width, big) = (if flag_byte(0x260)? == 1 { 2 } else { 1 }, flag_byte(0x261)? == 1);
    let centre = if width == 2 { 32768.0 } else { 128.0 };
    // a header of version 2 that leaves these two empty is of a scope with 14 divisions and 25 codes in each
    let divisions = match word(0x268)? {
        n @ 1..=20 => n as f64,
        _ if version == 2 => 14.0,
        _ => 10.0,
    };
    let mut t0 = -(value(0x198, "time per division")? * divisions / 2.0) - value(0x1C0, "delay")?;
    if version == 4 && data >= 0xB48 && sg_u32(b, 0xAF4) == Some(1) {
        // a zoom window saved by itself has its own time base, its delay counted the other way
        if let (Some(per), Some(delay)) = (sg_value(b, 0xAF8), sg_value(b, 0xB20)) {
            t0 = delay - per * divisions / 2.0;
        }
    }
    if !t0.is_finite() {
        return Err("the Siglent file has no usable time per division".into());
    }

    let mut out = Vec::new();
    let mut at = data;
    for k in 0..4 {
        if word(0x04 + 4 * k)? != 1 {
            continue;
        }
        let per = value(0x14 + 0x28 * k, "vertical scale")?;
        let offset = value(0xB4 + 0x28 * k, "vertical offset")?;
        let probe = sg_f64(b, 0x240 + shift + 8 * k).filter(|p| p.is_finite() && *p > 0.0).unwrap_or(1.0);
        let codes = match word(0x26C + 4 * k)? as i32 {
            n if n > 0 => n as f64,
            _ if version == 2 => 25.0,
            _ => return Err("the Siglent file has no usable codes per division".into()),
        };
        let block = at.checked_add(points * width).and_then(|end| b.get(at..end)).ok_or_else(short)?;
        at += points * width;
        let step = per / codes;
        let v: Vec<f32> = sg_codes(block, width, big).map(|code| (((code - centre) * step - offset) * probe) as f32).collect();
        if v.iter().any(|x| !x.is_finite()) {
            return Err("the Siglent file has no usable vertical scale".into());
        }
        out.push(WaveTrace { t0, dt: 1.0 / rate, v, label: format!("CH{}", k + 1) });
    }
    // the math traces that are on follow the channels: 0x27C four on; 0x28C volts per division and 0x32C
    // position, a 40-byte value each; 0x3CC their samples; 0x3DC the time between samples (f64); 0x3FC the codes
    // per division of them all. Scaled as a channel is, without a probe factor. A math trace whose header or
    // samples are not all there is left out; the channels before it stand.
    for k in 0..4 {
        if version < 3 || sg_u32(b, 0x27C + shift + 4 * k) != Some(1) {
            continue;
        }
        let (Some(per), Some(position)) = (sg_value(b, 0x28C + shift + 0x28 * k), sg_value(b, 0x32C + shift + 0x28 * k)) else { break };
        let (Some(points), Some(dt), Some(codes)) = (sg_u32(b, 0x3CC + shift + 4 * k), sg_f64(b, 0x3DC + shift + 8 * k), sg_u32(b, 0x3FC + shift)) else { break };
        let (points, codes) = (points as usize, codes as i32);
        if points > MAX_WAVEFORM_SAMPLES || codes <= 0 || !(dt > 0.0) || !dt.is_finite() {
            break;
        }
        let Some(block) = at.checked_add(points * width).and_then(|end| b.get(at..end)) else { break };
        at += points * width;
        if points < 2 {
            continue;
        }
        let step = per / codes as f64;
        let v: Vec<f32> = sg_codes(block, width, big).map(|code| ((code - centre) * step - position) as f32).collect();
        if v.iter().all(|x| x.is_finite()) {
            out.push(WaveTrace { t0, dt, v, label: format!("F{}", k + 1) });
        }
    }
    if out.is_empty() {
        return Err("the Siglent file holds no channel and no math trace".into());
    }
    Ok(out)
}

/// A Siglent binary waveform file of version 6: a header of `header_bytes` (u16 at 0x04) with the number of
/// records (u32 at 0x68), then each record: a header whose size is its u32 at 0x04, the extra bytes counted by
/// its u32 at 0xF0, and the samples. In a record header: 0x0E channel index (u16); 0x10 label; 0x60 the sample
/// at time zero and 0x68 the time between samples; 0xB0 the code of zero volts and 0xB8 the volts of one code
/// (f64 each); 0xF8 the number of samples and 0x100 their bytes (u64 each). The samples are unsigned.
/// Layout from Siglent's document, the reading of the samples from the RigolWFM project.
fn parse_siglent_records(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let short = || "truncated Siglent waveform file".to_string();
    let wide = |at: usize| sg_f64(b, at).map(f64::to_bits).ok_or_else(short);
    let records = sg_u32(b, 0x68).ok_or_else(short)?;
    let mut at = u16::from_le_bytes([b[4], b[5]]) as usize;
    let mut out = Vec::new();
    for _ in 0..records {
        let head = sg_u32(b, at + 4).ok_or_else(short)? as usize;
        if head < 264 {
            return Err("that Siglent waveform record is not read".into());
        }
        let extra = sg_u32(b, at + 0xF0).ok_or_else(short)? as usize;
        let (number, bytes) = (wide(at + 0xF8)?, wide(at + 0x100)?);
        if number > MAX_WAVEFORM_SAMPLES as u64 {
            return Err("waveform exceeds the 32-million-sample import limit".into());
        }
        let (number, bytes) = (number as usize, usize::try_from(bytes).map_err(|_| short())?);
        let start = at + head + extra;
        let block = start.checked_add(bytes).and_then(|end| b.get(start..end)).ok_or_else(short)?;
        let record = at;
        at = start + bytes;
        if number < 2 {
            continue;
        }
        let width = bytes / number;
        if bytes % number != 0 || ![1, 2, 4].contains(&width) {
            return Err("that Siglent sample size is not read".into());
        }
        let field = |offset: usize| sg_f64(b, record + offset).filter(|x| x.is_finite()).ok_or_else(|| "the Siglent file has no usable scale".to_string());
        let (zero_at, dt, zero_code, step) = (field(0x60)?, field(0x68)?, field(0xB0)?, field(0xB8)?);
        if !(dt > 0.0) || step == 0.0 {
            return Err("the Siglent file has no usable scale".into());
        }
        let named: String = b[record + 0x10..record + 0x20].iter().take_while(|c| **c != 0).filter(|c| c.is_ascii_graphic()).map(|c| *c as char).take(10).collect();
        let label = if named.is_empty() { format!("CH{}", u16::from_le_bytes([b[record + 0x0E], b[record + 0x0F]]).max(1)) } else { named };
        let v: Vec<f32> = sg_codes(block, width, false).map(|code| ((code - zero_code) * step) as f32).collect();
        out.push(WaveTrace { t0: -zero_at * dt, dt, v, label });
    }
    if out.is_empty() {
        return Err("that file holds no waveform".into());
    }
    Ok(out)
}

#[cfg(test)]
mod siglent_file_tests {
    use super::*;

    fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
        b[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// A value in volts with a prefix: the unit words say "V to the power 1".
    fn put_value(b: &mut [u8], at: usize, value: f64, prefix: u32) {
        put(b, at, &value.to_le_bytes());
        for (k, word) in [prefix, 0, 1, 1, 0, 1, 0, 1].iter().enumerate() {
            put(b, at + 8 + 4 * k, &word.to_le_bytes());
        }
    }

    /// A header of version 2, 3 or 4: 500 mV per division, 0.25 V offset, 32 codes per division, 2 s per division
    /// over 10 divisions, 0.5 s delay and 1 kSa/s. Then the data.
    fn file(version: u32, on: [u32; 4], points: u32, body: &[u8]) -> Vec<u8> {
        let (shift, data) = if version == 4 { (4, 0x1000) } else { (0, 0x800) };
        let mut b = vec![0u8; data];
        put(&mut b, 0, &version.to_le_bytes());
        if version == 4 {
            put(&mut b, 4, &(data as u32).to_le_bytes());
        }
        for k in 0..4 {
            put(&mut b, shift + 0x04 + 4 * k, &on[k].to_le_bytes());
            put_value(&mut b, shift + 0x14 + 0x28 * k, 500.0, 7);
            put_value(&mut b, shift + 0xB4 + 0x28 * k, 0.25, 8);
            put(&mut b, shift + 0x240 + 8 * k, &1.0f64.to_le_bytes());
            put(&mut b, shift + 0x26C + 4 * k, &32u32.to_le_bytes());
        }
        put_value(&mut b, shift + 0x198, 2.0, 8);
        put_value(&mut b, shift + 0x1C0, 500.0, 7);
        put(&mut b, shift + 0x1E8, &points.to_le_bytes());
        put_value(&mut b, shift + 0x1EC, 1.0, 9);
        put(&mut b, shift + 0x268, &10u32.to_le_bytes());
        b.extend_from_slice(body);
        b
    }

    #[test]
    fn the_channels_that_are_on_follow_one_another_and_are_scaled() {
        // CH2 and CH4 on: three samples of the one, then three of the other
        for version in [2, 3, 4] {
            let traces = parse_wave(&file(version, [0, 1, 0, 1], 3, &[160, 128, 96, 128, 192, 64]), "s.bin").unwrap();
            let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
            assert_eq!(seen, [("CH2", &[0.25f32, -0.25, -0.75][..]), ("CH4", &[-0.25f32, 0.75, -1.25][..])], "version {version}");
            assert!(traces.iter().all(|t| t.t0 == -10.5 && t.dt == 1e-3), "version {version}");
        }
    }

    #[test]
    fn sixteen_bit_samples_are_read_in_the_byte_order_the_header_names() {
        for (big, shift, version) in [(false, 4, 4), (true, 4, 4), (true, 0, 3)] {
            let codes = [32800u16, 32768, 32736];
            let body: Vec<u8> = codes.iter().flat_map(|c| if big { c.to_be_bytes() } else { c.to_le_bytes() }).collect();
            let mut b = file(version, [1, 0, 0, 0], 3, &body);
            b[shift + 0x260] = 1;
            b[shift + 0x261] = big as u8;
            assert_eq!(parse_wave(&b, "s.bin").unwrap()[0].v, [0.25, -0.25, -0.75], "version {version}, high byte first {big}");
        }
    }

    #[test]
    fn the_probe_factor_multiplies_what_the_header_scales_at_the_input() {
        let mut b = file(4, [1, 0, 0, 0], 3, &[160, 128, 96]);
        put(&mut b, 4 + 0x240, &10.0f64.to_le_bytes());
        assert_eq!(parse_wave(&b, "s.bin").unwrap()[0].v, [2.5, -2.5, -7.5]);
    }

    #[test]
    fn an_old_header_without_its_counts_is_of_a_scope_with_25_codes_and_14_divisions() {
        let mut b = file(2, [1, 0, 0, 0], 3, &[153, 128, 103]);
        put(&mut b, 0x268, &[0; 4]);
        put(&mut b, 0x26C, &[0; 4]);
        let t = &parse_wave(&b, "s.bin").unwrap()[0];
        assert_eq!((t.v.as_slice(), t.t0), (&[0.25f32, -0.25, -0.75][..], -14.5));
        // a later version has to say them
        put(&mut b, 0, &3u32.to_le_bytes());
        assert!(parse_wave(&b, "s.bin").unwrap_err().contains("codes per division"));
    }

    #[test]
    fn a_zoom_window_saved_by_itself_is_placed_by_its_own_time_base() {
        let mut b = file(4, [1, 0, 0, 0], 3, &[160, 128, 96]);
        put(&mut b, 0xAF4, &1u32.to_le_bytes());
        put_value(&mut b, 0xAF8, 1.0, 8);
        put_value(&mut b, 0xB20, 3.0, 8);
        assert_eq!(parse_wave(&b, "s.bin").unwrap()[0].t0, -2.0);
    }

    #[test]
    fn a_file_without_its_samples_or_without_a_channel_is_refused() {
        assert!(parse_wave(&file(4, [1, 1, 0, 0], 3, &[160, 128, 96, 1, 2]), "s.bin").unwrap_err().contains("truncated"));
        assert!(parse_wave(&file(4, [0, 0, 0, 0], 3, &[160, 128, 96]), "s.bin").unwrap_err().contains("no channel and no math trace"));
        assert!(parse_wave(&file(3, [1, 0, 0, 0], 1, &[160]), "s.bin").unwrap_err().contains("no waveform"));
        // a first word that is a version, over a header that is not one of that version
        let mut b = file(4, [1, 0, 0, 0], 3, &[160, 128, 96]);
        put(&mut b, 0x08, &7u32.to_le_bytes());
        assert_eq!(siglent_version(&b), None);
        assert_eq!(siglent_version(&file(3, [1, 0, 0, 0], 3, &[1, 2, 3])), Some(3));
        assert_eq!(siglent_version(&5u32.to_le_bytes()), None);
    }

    /// Switches math trace F2 on: 2 V per division, position -1 V, four samples 0.5 s apart, 32 codes per division.
    fn with_math(mut b: Vec<u8>, shift: usize, samples: &[u8]) -> Vec<u8> {
        put(&mut b, shift + 0x27C + 4, &1u32.to_le_bytes());
        put_value(&mut b, shift + 0x28C + 0x28, 2.0, 8);
        put_value(&mut b, shift + 0x32C + 0x28, -1.0, 8);
        put(&mut b, shift + 0x3CC + 4, &4u32.to_le_bytes());
        put(&mut b, shift + 0x3DC + 8, &0.5f64.to_le_bytes());
        put(&mut b, shift + 0x3FC, &32u32.to_le_bytes());
        b.extend_from_slice(samples);
        b
    }

    #[test]
    fn a_math_trace_follows_the_channels_and_is_read_alone_too() {
        // after one channel
        for (version, shift) in [(3, 0), (4, 4)] {
            let traces = parse_wave(&with_math(file(version, [1, 0, 0, 0], 3, &[160, 128, 96]), shift, &[144, 128, 112, 96]), "s.bin").unwrap();
            let seen: Vec<(&str, &[f32], f64)> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice(), t.dt)).collect();
            assert_eq!(seen, [("CH1", &[0.25f32, -0.25, -0.75][..], 1e-3), ("F2", &[2.0f32, 1.0, 0.0, -1.0][..], 0.5)], "version {version}");
            assert!(traces.iter().all(|t| t.t0 == -10.5));
        }
        // saved by itself: no channel is on
        let alone = parse_wave(&with_math(file(4, [0, 0, 0, 0], 0, &[]), 4, &[144, 128, 112, 96]), "s.bin").unwrap();
        assert_eq!((alone.len(), alone[0].label.as_str(), alone[0].v.as_slice()), (1, "F2", &[2.0f32, 1.0, 0.0, -1.0][..]));
        // cut short, the math trace is left out and the channel stands; a version 2 file has none
        let cut = parse_wave(&with_math(file(4, [1, 0, 0, 0], 3, &[160, 128, 96]), 4, &[144, 128]), "s.bin").unwrap();
        assert_eq!((cut.len(), cut[0].label.as_str()), (1, "CH1"));
        assert_eq!(parse_wave(&with_math(file(2, [1, 0, 0, 0], 3, &[160, 128, 96]), 0, &[144, 128, 112, 96]), "s.bin").unwrap().len(), 1);
    }

    /// One record of a version 6 file: 264 header bytes, four extra, then the samples.
    fn record(label: &str, index: u16, width: usize, codes: &[u32]) -> Vec<u8> {
        let mut r = vec![0u8; 264 + 4];
        put(&mut r, 0x04, &264u32.to_le_bytes());
        put(&mut r, 0x0E, &index.to_le_bytes());
        put(&mut r, 0x10, label.as_bytes());
        put(&mut r, 0x60, &2.0f64.to_le_bytes());
        put(&mut r, 0x68, &0.5f64.to_le_bytes());
        put(&mut r, 0xB0, &128.0f64.to_le_bytes());
        put(&mut r, 0xB8, &0.25f64.to_le_bytes());
        put(&mut r, 0xF0, &4u32.to_le_bytes());
        put(&mut r, 0xF8, &(codes.len() as u64).to_le_bytes());
        put(&mut r, 0x100, &((codes.len() * width) as u64).to_le_bytes());
        for code in codes {
            r.extend_from_slice(&code.to_le_bytes()[..width]);
        }
        r
    }

    fn records(list: &[Vec<u8>]) -> Vec<u8> {
        let mut b = vec![0u8; 108];
        put(&mut b, 0, &6u32.to_le_bytes());
        put(&mut b, 4, &108u16.to_le_bytes());
        put(&mut b, 8, b"SDS1202X-E");
        put(&mut b, 0x68, &(list.len() as u32).to_le_bytes());
        for r in list {
            b.extend_from_slice(r);
        }
        b
    }

    #[test]
    fn a_file_of_records_gives_a_trace_for_each_record() {
        let b = records(&[record("C1", 1, 1, &[130, 128, 126]), record("", 2, 2, &[132, 128, 124])]);
        let traces = parse_wave(&b, "s.bin").unwrap();
        let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
        assert_eq!(seen, [("C1", &[0.5f32, 0.0, -0.5][..]), ("CH2", &[1.0f32, 0.0, -1.0][..])]);
        assert!(traces.iter().all(|t| t.t0 == -1.0 && t.dt == 0.5));
        // cut short inside the second record, and a sample size that is none
        assert!(parse_wave(&b[..b.len() - 1], "s.bin").unwrap_err().contains("truncated"));
        let mut odd = record("C1", 1, 1, &[130, 128, 126]);
        put(&mut odd, 0xF8, &2u64.to_le_bytes());
        assert!(parse_wave(&records(&[odd]), "s.bin").unwrap_err().contains("sample size"));
    }

    #[test]
    fn the_csv_file_of_a_siglent_scope_is_read_past_its_settings() {
        let text = "Record Length,Analog:14000\nSample Rate,100000000.0\nVertical Scale,CH1: 0.05\nVertical Offset,CH1:0.00000\nHorizontal Scale, 0.000010000\nSecond,Volt\n-0.00007000000,-0.19400\n-0.00006999000,-0.19200,\n-0.00006998000,-0.19000,\n";
        let t = &parse_wave(text.as_bytes(), "siglent.csv").unwrap()[0];
        assert_eq!(t.v, [-0.194, -0.192, -0.19]);
        assert!((t.t0 + 7e-5).abs() < 1e-18 && (t.dt - 1e-8).abs() < 1e-14);
    }
}

// ── Inflate (RFC 1951): the compressed blocks of a Rigol DHO waveform file ───────────────────────────────────

/// Bit reader of a deflate stream: values LSB first.
struct BitIn<'a> {
    data: &'a [u8],
    at: usize,
    hold: u32,
    held: u32,
}

impl BitIn<'_> {
    /// The next `n` bits, at most 16.
    fn bits(&mut self, n: u32) -> Option<u32> {
        while self.held < n {
            self.hold |= (*self.data.get(self.at)? as u32) << self.held;
            self.at += 1;
            self.held += 8;
        }
        let value = self.hold & ((1u32 << n) - 1);
        self.hold >>= n;
        self.held -= n;
        Some(value)
    }
}

/// A canonical Huffman code: how many codes each length has, and the symbols in the order of their codes.
struct Huffman {
    count: [u16; 16],
    symbol: Vec<u16>,
}

impl Huffman {
    /// From the code length of each symbol, 0 for a symbol without a code. None when the lengths ask for more
    /// codes than there are.
    fn new(lengths: &[u8]) -> Option<Self> {
        let mut count = [0u16; 16];
        for &length in lengths {
            count[(length & 15) as usize] += 1;
        }
        count[0] = 0;
        let mut left = 1i32;
        for &n in &count[1..] {
            left = (left << 1) - n as i32;
            if left < 0 {
                return None;
            }
        }
        let mut next = [0u16; 16];
        for length in 1..15 {
            next[length + 1] = next[length] + count[length];
        }
        let mut symbol = vec![0u16; lengths.len()];
        for (s, &length) in lengths.iter().enumerate() {
            if length != 0 {
                let at = &mut next[(length & 15) as usize];
                symbol[*at as usize] = s as u16;
                *at += 1;
            }
        }
        Some(Self { count, symbol })
    }

    /// The symbol of the code that comes next, read a bit at a time, the first bit the highest.
    fn decode(&self, bits: &mut BitIn) -> Option<usize> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for &n in &self.count[1..] {
            code |= bits.bits(1)? as i32;
            let n = n as i32;
            if code - n < first {
                return self.symbol.get((index + code - first) as usize).map(|s| *s as usize);
            }
            index += n;
            first = (first + n) << 1;
            code <<= 1;
        }
        None
    }
}

/// The two codes of a block with dynamic Huffman codes (RFC 1951 3.2.7): for literals and lengths, and for distances.
fn inflate_codes(bits: &mut BitIn) -> Option<(Huffman, Huffman)> {
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let (literals, distances, coded) = (bits.bits(5)? as usize + 257, bits.bits(5)? as usize + 1, bits.bits(4)? as usize + 4);
    if literals > 286 || distances > 30 {
        return None;
    }
    let mut of_lengths = [0u8; 19];
    for &at in &ORDER[..coded] {
        of_lengths[at] = bits.bits(3)? as u8;
    }
    let of_lengths = Huffman::new(&of_lengths)?;
    let mut lengths = [0u8; 286 + 30];
    let mut n = 0;
    while n < literals + distances {
        let (length, times) = match of_lengths.decode(bits)? {
            symbol @ 0..=15 => (symbol as u8, 1),
            16 => (*lengths.get(n.checked_sub(1)?)?, 3 + bits.bits(2)? as usize),
            17 => (0, 3 + bits.bits(3)? as usize),
            _ => (0, 11 + bits.bits(7)? as usize),
        };
        if n + times > literals + distances {
            return None;
        }
        lengths[n..n + times].fill(length);
        n += times;
    }
    if lengths[256] == 0 {
        return None; // no code for the end of the block
    }
    Some((Huffman::new(&lengths[..literals])?, Huffman::new(&lengths[literals..literals + distances])?))
}

/// The bytes a deflate stream stands for, at most `limit` of them, and how many bytes of `data` the stream took.
/// None for a stream that is damaged, cut short or longer than the limit.
fn inflate(data: &[u8], limit: usize) -> Option<(Vec<u8>, usize)> {
    let mut bits = BitIn { data, at: 0, hold: 0, held: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = bits.bits(1)? == 1;
        match bits.bits(2)? {
            0 => {
                // stored: from the next whole byte, a length, its complement and the bytes
                (bits.hold, bits.held) = (0, 0);
                let head = data.get(bits.at..bits.at.checked_add(4)?)?;
                let (length, check) = (u16::from_le_bytes([head[0], head[1]]), u16::from_le_bytes([head[2], head[3]]));
                let block = data.get(bits.at + 4..bits.at + 4 + length as usize)?;
                if length != !check || out.len() + block.len() > limit {
                    return None;
                }
                out.extend_from_slice(block);
                bits.at += 4 + length as usize;
            }
            kind @ (1 | 2) => {
                let (literals, distances) = if kind == 2 {
                    inflate_codes(&mut bits)?
                } else {
                    let mut fixed = [8u8; 288];
                    fixed[144..256].fill(9);
                    fixed[256..280].fill(7);
                    (Huffman::new(&fixed)?, Huffman::new(&[5u8; 30])?)
                };
                loop {
                    let symbol = literals.decode(&mut bits)?;
                    if symbol == 256 {
                        break;
                    }
                    if symbol < 256 {
                        if out.len() >= limit {
                            return None;
                        }
                        out.push(symbol as u8);
                        continue;
                    }
                    let (base, extra) = *LEN_BASE.get(symbol - 257)?;
                    let length = base as usize + bits.bits(extra as u32)? as usize;
                    let (base, extra) = *DIST_BASE.get(distances.decode(&mut bits)?)?;
                    let distance = base as usize + bits.bits(extra as u32)? as usize;
                    if distance > out.len() || out.len() + length > limit {
                        return None;
                    }
                    for _ in 0..length {
                        out.push(out[out.len() - distance]);
                    }
                }
            }
            _ => return None,
        }
        if last {
            return Some((out, bits.at));
        }
    }
}

/// The bytes of a zlib stream (RFC 1950): two header bytes, a deflate stream and the Adler-32 of what it holds.
fn zlib_inflate(data: &[u8], limit: usize) -> Option<Vec<u8>> {
    if data.len() < 6 || data[0] & 0x0F != 8 || u16::from_be_bytes([data[0], data[1]]) % 31 != 0 || data[1] & 0x20 != 0 {
        return None;
    }
    let (out, taken) = inflate(&data[2..], limit)?;
    let sum = data.get(2 + taken..2 + taken + 4)?;
    (u32::from_be_bytes([sum[0], sum[1], sum[2], sum[3]]) == adler32(&out)).then_some(out)
}

// ── Rigol: waveform files of the scope's own format (.wfm) ───────────────────────────────────────────────────

/// What a Rigol `.wfm` file is, by its first four bytes.
enum RigolWfm {
    /// DHO800, DHO900 and DHO1000: settings in compressed blocks, 16-bit samples.
    Dho,
    /// DS1000E: one fixed header, 8-bit samples.
    E,
    /// A family whose file is known by its first bytes and not read.
    Unread(&'static str),
}

fn rigol_wfm_kind(b: &[u8]) -> Option<RigolWfm> {
    match b.get(..4)? {
        [2, 0, 0, 0] if rigol_dho_blocks(b).is_some_and(|(blocks, _)| blocks.iter().any(|block| matches!(block.kind, 5 | 9))) => Some(RigolWfm::Dho),
        [0xA5, 0xA5, 0, 0] if b.len() >= 276 => Some(RigolWfm::E),
        [1, 0xFF, 0xFF, 0xFF] if b.get(4) == Some(&0xA5) => Some(RigolWfm::Unread("DS1000Z and MSO1000Z")),
        [0xA5, 0xA5, 0x38, 0] => Some(RigolWfm::Unread("DS2000, DS4000 and DS6000")),
        [0xA5, 0xA5, 0xA4, 1] => Some(RigolWfm::Unread("DS1000B")),
        [0xA1, 0xA5, 0, 0] => Some(RigolWfm::Unread("DS1000C")),
        _ => None,
    }
}

fn parse_rigol_wfm(b: &[u8], kind: RigolWfm) -> Result<Vec<WaveTrace>, String> {
    match kind {
        RigolWfm::Dho => parse_rigol_dho(b),
        RigolWfm::E => parse_rigol_e(b),
        // a DS1000Z file is left unread on purpose: no known reading of it gives the vertical offset the scope
        // showed, and the project this layout comes from says to check its result against a measurement
        RigolWfm::Unread(family) if family.starts_with("DS1000Z") => {
            Err(format!("RIGOL {family} waveform files (.wfm) are not read: the vertical offset cannot be told from the file; save the waveform as CSV"))
        }
        RigolWfm::Unread(family) => Err(format!("RIGOL {family} waveform files (.wfm) are not read; save the waveform as CSV")),
    }
}

/// A settings block of a DHO waveform file: its number, its kind and what it holds, decompressed.
struct DhoBlock {
    id: u16,
    kind: u16,
    payload: Vec<u8>,
}

/// The settings blocks of a DHO waveform file, and where they end. They start at byte 24, each with a header of
/// six u16: number, kind, size decompressed, size compressed, bytes taken in the file, zero. The first
/// `compressed` bytes are a zlib stream, or the settings themselves; a block that takes no bytes ends the list.
fn rigol_dho_blocks(b: &[u8]) -> Option<(Vec<DhoBlock>, usize)> {
    let mut blocks = Vec::new();
    let mut at = 24usize;
    loop {
        let head = b.get(at..at + 12)?;
        let word = |k: usize| u16::from_le_bytes([head[2 * k], head[2 * k + 1]]);
        let (compressed, taken) = (word(3) as usize, word(4) as usize);
        if compressed == 0 && taken == 0 {
            return Some((blocks, at));
        }
        if compressed > taken || blocks.len() >= 4096 {
            return None;
        }
        let content = b.get(at + 12..at + 12 + taken)?;
        let payload = zlib_inflate(&content[..compressed], 1 << 20).unwrap_or_else(|| content[..compressed].to_vec());
        blocks.push(DhoBlock { id: word(0), kind: word(1), payload });
        at += 12 + taken;
    }
}

/// A waveform file of a RIGOL DHO800, DHO900 or DHO1000. RIGOL does not publish this layout; it is the one the
/// RigolWFM project worked out and checked against the scope's own BIN export of the same captures.
/// The settings blocks (`rigol_dho_blocks`) hold the vertical scale: in the block numbered as its channel, of kind
/// 5 (DHO800) or 9 (DHO1000), an i64 at byte 1 and the centre at byte 38 (i32 or i64):
/// DHO800: volts of one code = i64 / 7.5e12, centre = -i32 / 1e9, volts = code * volts of one code + centre - 32768 codes;
/// DHO1000: volts of one code = i64 / 7.5e11, centre = i64 / 1e8, volts = code * volts of one code - centre - 32768 codes.
/// After the blocks and the zeros that follow them: u64 samples of all channels; at +16 the time between samples
/// as a u32 count of 0.8 ns (DHO800) or 10 ns (DHO1000); at +24 the samples of one channel; at +40 the samples,
/// u16, the channels taking turns. The channels are taken to be CH1 onwards, and the middle sample to be the
/// trigger: the file says neither where the sources looked.
fn parse_rigol_dho(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let short = || "truncated RIGOL waveform file".to_string();
    let (blocks, mut at) = rigol_dho_blocks(b).ok_or_else(short)?;
    let dho800 = blocks.iter().any(|block| block.kind == 5 && (1..=4).contains(&block.id));
    let scales: Vec<(u16, f64, f64)> = blocks.iter().filter(|block| block.kind == if dho800 { 5 } else { 9 } && (1..=4).contains(&block.id)).filter_map(|block| {
        let number = sg_f64(&block.payload, 1)?.to_bits() as i64 as f64;
        let (step, centre) = if dho800 {
            (number / 7.5e12, -(sg_u32(&block.payload, 38)? as i32 as f64) / 1e9)
        } else {
            (number / 7.5e11, -(sg_f64(&block.payload, 38)?.to_bits() as i64 as f64) / 1e8)
        };
        (step != 0.0).then_some((block.id, step, centre - step * 32768.0))
    }).collect();
    let Some(&(_, first_step, first_zero)) = scales.first() else {
        return Err("the RIGOL file has no usable vertical scale".into());
    };
    while b.get(at) == Some(&0) {
        at += 1;
    }
    let total = sg_f64(b, at).ok_or_else(short)?.to_bits();
    let (ticks, hint) = (sg_u32(b, at + 16).ok_or_else(short)?, sg_u32(b, at + 24).ok_or_else(short)? as u64);
    if total == 0 || total > 4 * MAX_WAVEFORM_SAMPLES as u64 {
        return Err("that file holds no waveform".into());
    }
    // the count of all channels can run a few samples over: the channels are the nearest whole number
    let (points, channels) = if hint > 0 { (hint, (total + hint / 2) / hint) } else { (total, 1) };
    if points < 2 || !(1..=4).contains(&channels) {
        return Err("that file holds no waveform".into());
    }
    if points > MAX_WAVEFORM_SAMPLES as u64 {
        return Err("waveform exceeds the 32-million-sample import limit".into());
    }
    let (points, channels) = (points as usize, channels as usize);
    let ticks = if ticks == 0 || ticks > 1_000_000_000 { 1 } else { ticks };
    let dt = ticks as f64 * if dho800 { 8e-10 } else { 1e-8 };
    let t0 = -(points as f64 / 2.0) * dt;
    let samples = b.get(at + 40..).filter(|s| s.len() >= points * channels * 2).ok_or_else(short)?;
    Ok((0..channels).map(|k| {
        let (step, zero) = scales.iter().find(|(id, _, _)| *id as usize == k + 1).map_or((first_step, first_zero), |&(_, step, zero)| (step, zero));
        let v = samples.chunks_exact(2 * channels).take(points).map(|row| (u16::from_le_bytes([row[2 * k], row[2 * k + 1]]) as f64 * step + zero) as f32).collect();
        WaveTrace { t0, dt, v, label: format!("CH{}", k + 1) }
    }).collect())
}

/// A waveform file of a RIGOL DS1000E. One header of 276 bytes, little endian: 20 roll stop (u32); 28 the samples
/// of CH1 (u32); a channel header at 34 (CH1) and 58 (CH2), in it +10 probe factor (f32), +15 on (u8), +18
/// microvolts per division at the input (i32) and +22 position in codes (i16); 100 sample rate (f32, Hz);
/// 112 time of the middle of the record (i64, ps); 232 the samples of CH2 (u32), 0 for as many as CH1.
/// The samples follow, CH1 then CH2, 8-bit, 25 codes a division, a higher code a lower voltage:
/// volts = (125 - code - position) * volts per division * probe / 25. In roll mode the last `roll stop + 2`
/// bytes of a channel are not samples. Layout from the RigolWFM project. A DS1000D file has the same bytes and
/// its scale already holds the probe factor; it is read as a DS1000E file. The logic channels, and the second
/// time base of an alternate trigger, are not read.
fn parse_rigol_e(b: &[u8]) -> Result<Vec<WaveTrace>, String> {
    let short = || "truncated RIGOL waveform file".to_string();
    let word = |at: usize| sg_u32(b, at).ok_or_else(short);
    let float = |at: usize| word(at).map(|bits| f32::from_bits(bits) as f64);
    let depth = [word(28)? as usize, match word(232)? { 0 => word(28)?, n => n } as usize];
    let on = [b[34 + 15] != 0, b[58 + 15] != 0];
    let held: usize = (0..2).filter(|k| on[*k]).map(|k| depth[k]).sum();
    // a DS1000C file has the same first bytes and a shorter header
    if b.len() == 256 + held || b.len() == 272 + held {
        return Err("RIGOL DS1000C waveform files (.wfm) are not read".into());
    }
    let rate = float(100)?;
    if !(rate > 0.0) || !rate.is_finite() {
        return Err("the RIGOL file has no usable sample rate".into());
    }
    let middle = sg_f64(b, 112).ok_or_else(short)?.to_bits() as i64 as f64 * 1e-12;
    let skip = match word(20)? as usize {
        0 => 0,
        stop => stop + 2,
    };
    let mut out = Vec::new();
    let mut at = 276usize;
    for k in 0..2 {
        if !on[k] {
            continue;
        }
        if depth[k] > MAX_WAVEFORM_SAMPLES {
            return Err("waveform exceeds the 32-million-sample import limit".into());
        }
        let head = 34 + 24 * k;
        let block = at.checked_add(depth[k]).and_then(|end| b.get(at..end)).ok_or_else(short)?;
        at += depth[k];
        let points = depth[k].saturating_sub(skip);
        if points < 2 {
            continue;
        }
        let step = 1e-6 * (word(head + 18)? as i32 as f64) * float(head + 10)? / 25.0;
        let position = i16::from_le_bytes([b[head + 22], b[head + 23]]) as f64;
        let v: Vec<f32> = block[..points].iter().map(|code| ((125.0 - *code as f64 - position) * step) as f32).collect();
        if step == 0.0 || v.iter().any(|x| !x.is_finite()) {
            return Err("the RIGOL file has no usable vertical scale".into());
        }
        out.push(WaveTrace { t0: middle - depth[k] as f64 / rate / 2.0, dt: 1.0 / rate, v, label: format!("CH{}", k + 1) });
    }
    if out.is_empty() {
        return Err("that file holds no waveform".into());
    }
    Ok(out)
}

#[cfg(test)]
mod rigol_wfm_file_tests {
    use super::*;

    /// "scale 0 and offset 0; scale 7 and offset 5; ..." forty times over: 893 bytes that compress well.
    fn text() -> Vec<u8> {
        (0..40).map(|i| format!("scale {} and offset {}; ", i * 7 % 13, i * 5 % 11)).collect::<String>().into_bytes()
    }

    /// A zlib stream of one stored block.
    fn stored(data: &[u8]) -> Vec<u8> {
        let mut z = vec![0x78, 0x01, 0x01];
        z.extend_from_slice(&(data.len() as u16).to_le_bytes());
        z.extend_from_slice(&(!(data.len() as u16)).to_le_bytes());
        z.extend_from_slice(data);
        z.extend_from_slice(&adler32(data).to_be_bytes());
        z
    }

    #[test]
    fn the_three_kinds_of_deflate_block_are_inflated() {
        // fixed codes, with a match that runs over itself: "abc" four times, as zlib level 9 writes it
        let fixed = [120, 218, 75, 76, 74, 78, 132, 33, 0, 29, 224, 4, 153];
        assert_eq!(zlib_inflate(&fixed, 64).unwrap(), b"abcabcabcabc");
        // dynamic codes, as zlib level 9 writes the text
        let dynamic = [
            120, 218, 117, 210, 77, 10, 2, 49, 12, 64, 225, 171, 244, 8, 105, 250, 143, 167, 25, 116, 102, 53, 232, 162, 222, 31, 65, 72, 201, 200,
            115, 251, 8, 52, 31, 205, 188, 111, 231, 30, 36, 108, 207, 71, 120, 29, 199, 220, 223, 65, 110, 97, 126, 107, 243, 181, 88, 141, 190, 198,
            53, 220, 125, 206, 86, 213, 215, 97, 117, 248, 154, 172, 38, 95, 251, 122, 238, 178, 155, 90, 206, 190, 182, 53, 124, 93, 206, 114, 241,
            181, 174, 97, 69, 117, 69, 181, 176, 186, 161, 58, 162, 186, 163, 90, 81, 61, 16, 157, 24, 45, 136, 206, 140, 142, 136, 46, 252, 213, 202,
            234, 138, 106, 65, 117, 67, 117, 68, 117, 71, 181, 162, 122, 32, 58, 49, 154, 239, 59, 51, 250, 207, 129, 23, 254, 106, 190, 240, 138, 106,
            249, 81, 127, 0, 152, 61, 27, 73,
        ];
        assert_eq!(zlib_inflate(&dynamic, 4096).unwrap(), text());
        // stored
        assert_eq!(zlib_inflate(&stored(&text()), 4096).unwrap(), text());
        // what this file's own deflate writes
        let rows: Vec<u8> = (0..600).map(|i| (i / 7 % 5) as u8).collect();
        assert_eq!(inflate(&deflate_rows(&rows, 30), 4096).unwrap().0, rows);
    }

    #[test]
    fn a_stream_that_is_damaged_cut_short_or_too_long_is_none() {
        let good = stored(&text());
        assert!(zlib_inflate(&good, 892).is_none(), "longer than the limit");
        assert!(zlib_inflate(&good[..good.len() - 5], 4096).is_none(), "cut short");
        let mut sum = good.clone();
        *sum.last_mut().unwrap() ^= 1;
        assert!(zlib_inflate(&sum, 4096).is_none(), "wrong check sum");
        let mut length = good.clone();
        length[5] ^= 1;
        assert!(zlib_inflate(&length, 4096).is_none(), "length and its complement disagree");
        // a match that reaches back before the start, a block of the kind that does not exist, no stream at all
        assert!(inflate(&[0x03, 0x02, 0x00], 64).is_none());
        assert!(inflate(&[0x07], 64).is_none());
        assert!(zlib_inflate(&[], 64).is_none());
        assert!(zlib_inflate(b"plain settings, not a stream", 64).is_none());
    }

    fn put(b: &mut [u8], at: usize, bytes: &[u8]) {
        b[at..at + bytes.len()].copy_from_slice(bytes);
    }

    /// The settings of one channel: 0.1 mV a code, and the volts the middle code stands for.
    fn channel_settings(dho800: bool, centre: f64) -> Vec<u8> {
        let mut p = vec![0u8; 64];
        if dho800 {
            put(&mut p, 1, &750_000_000i64.to_le_bytes());
            put(&mut p, 38, &((-centre * 1e9) as i32).to_le_bytes());
        } else {
            put(&mut p, 1, &75_000_000i64.to_le_bytes());
            put(&mut p, 38, &((-centre * 1e8) as i64).to_le_bytes());
        }
        p
    }

    /// A DHO file: its blocks, some zeros, the frame header and the samples of `channels` channels taking turns.
    fn dho(blocks: &[(u16, u16, Vec<u8>, bool)], channels: u64, ticks: u32, codes: &[u16]) -> Vec<u8> {
        let mut b = vec![0u8; 24];
        b[0] = 2;
        for (id, kind, payload, compress) in blocks {
            let content = if *compress { stored(payload) } else { payload.clone() };
            let taken = content.len() + 3; // the file pads a block
            for word in [*id, *kind, payload.len() as u16, content.len() as u16, taken as u16, 0] {
                b.extend_from_slice(&word.to_le_bytes());
            }
            b.extend_from_slice(&content);
            b.extend_from_slice(&[0; 3]);
        }
        b.extend_from_slice(&[0; 12 + 24]); // the block that ends the list, and the zeros before the frame
        let points = codes.len() as u64 / channels;
        let mut frame = vec![0u8; 40];
        put(&mut frame, 0, &(points * channels).to_le_bytes());
        put(&mut frame, 16, &ticks.to_le_bytes());
        put(&mut frame, 20, &0x4Du32.to_le_bytes());
        put(&mut frame, 24, &(points as u32).to_le_bytes());
        put(&mut frame, 28, &(points as u32).to_le_bytes());
        frame[32] = 1;
        b.extend_from_slice(&frame);
        b.extend(codes.iter().flat_map(|c| c.to_le_bytes()));
        b
    }

    fn close(seen: &[f32], wanted: &[f32]) -> bool {
        seen.len() == wanted.len() && seen.iter().zip(wanted).all(|(a, b)| (a - b).abs() < 1e-5)
    }

    #[test]
    fn a_dho800_file_gives_its_channels_from_samples_that_take_turns() {
        // CH1 around 0.5 V and CH2 around -1 V, 500 ticks of 0.8 ns between samples, compressed settings
        let blocks = [(1, 5, channel_settings(true, 0.5), true), (300, 6, vec![7; 40], true), (2, 5, channel_settings(true, -1.0), false)];
        let b = dho(&blocks, 2, 500, &[32768, 32768, 42768, 22768, 22768, 42768, 32768, 32768]);
        let traces = parse_wave(&b, "dho.wfm").unwrap();
        assert_eq!(traces.len(), 2);
        assert!(close(&traces[0].v, &[0.5, 1.5, -0.5, 0.5]) && close(&traces[1].v, &[-1.0, -2.0, 0.0, -1.0]), "{:?}", (&traces[0].v, &traces[1].v));
        assert_eq!((traces[0].label.as_str(), traces[1].label.as_str()), ("CH1", "CH2"));
        assert!(traces.iter().all(|t| (t.dt - 4e-7).abs() < 1e-18 && (t.t0 + 8e-7).abs() < 1e-18));
    }

    #[test]
    fn a_dho1000_file_counts_ten_nanoseconds_and_a_few_samples_too_many() {
        let mut b = dho(&[(1, 9, channel_settings(false, 0.5), true)], 1, 3, &[32768, 42768, 22768, 32768]);
        // the count of all channels runs over the samples of the one channel
        let frame = b.len() - 8 - 40;
        put(&mut b, frame, &5u64.to_le_bytes());
        let t = &parse_wave(&b, "dho.wfm").unwrap()[0];
        assert!(close(&t.v, &[0.5, 1.5, -0.5, 0.5]), "{:?}", t.v);
        assert!((t.dt - 3e-8).abs() < 1e-20 && (t.t0 + 6e-8).abs() < 1e-20);
    }

    #[test]
    fn a_dho_file_without_its_scale_or_its_samples_is_refused() {
        let good = dho(&[(1, 5, channel_settings(true, 0.5), true)], 1, 500, &[32768, 42768, 22768, 32768]);
        assert!(parse_wave(&good, "dho.wfm").is_ok());
        assert!(parse_wave(&good[..good.len() - 1], "dho.wfm").unwrap_err().contains("truncated"));
        // a channel block that holds no scale
        let empty = dho(&[(1, 5, vec![0; 64], true)], 1, 500, &[32768, 42768, 22768, 32768]);
        assert!(parse_wave(&empty, "dho.wfm").unwrap_err().contains("vertical scale"));
        // no channel block at all: not taken for this kind of file
        assert!(rigol_wfm_kind(&dho(&[(300, 6, vec![7; 40], true)], 1, 500, &[1, 2])).is_none());
    }

    /// A DS1000E file: 1 V per division at the input of a 10x probe, 25 codes of position, 1 kSa/s, the middle
    /// of the record at 2 ms.
    fn e(on: [bool; 2], depth: u32, roll_stop: u32, data: &[u8]) -> Vec<u8> {
        let mut b = vec![0u8; 276];
        put(&mut b, 0, &[0xA5, 0xA5, 0, 0]);
        put(&mut b, 20, &roll_stop.to_le_bytes());
        put(&mut b, 28, &depth.to_le_bytes());
        for k in 0..2 {
            let head = 34 + 24 * k;
            put(&mut b, head + 10, &10.0f32.to_le_bytes());
            b[head + 15] = on[k] as u8;
            put(&mut b, head + 18, &100_000i32.to_le_bytes());
            put(&mut b, head + 22, &(25i16 * (1 - 2 * k as i16)).to_le_bytes());
        }
        put(&mut b, 100, &1000.0f32.to_le_bytes());
        put(&mut b, 112, &2_000_000_000i64.to_le_bytes());
        b.extend_from_slice(data);
        b
    }

    #[test]
    fn a_ds1000e_file_gives_its_channels_one_after_the_other() {
        // a higher code is a lower voltage; CH1 sits a division up, CH2 a division down
        let traces = parse_wave(&e([true, true], 4, 0, &[100, 75, 125, 150, 150, 125, 175, 200]), "e.wfm").unwrap();
        assert_eq!(traces.len(), 2);
        assert!(close(&traces[0].v, &[0.0, 1.0, -1.0, -2.0]) && close(&traces[1].v, &[0.0, 1.0, -1.0, -2.0]), "{:?}", (&traces[0].v, &traces[1].v));
        assert!(traces.iter().all(|t| t.dt == 1e-3 && (t.t0 - 0.0).abs() < 1e-12));
        // CH2 alone, and the bytes after a roll stop left out
        let alone = parse_wave(&e([false, true], 6, 1, &[150, 125, 175, 0, 0, 0]), "e.wfm").unwrap();
        assert_eq!((alone.len(), alone[0].label.as_str()), (1, "CH2"));
        assert!(close(&alone[0].v, &[0.0, 1.0, -1.0]) && (alone[0].t0 + 1e-3).abs() < 1e-12);
    }

    #[test]
    fn rigol_files_that_are_not_read_are_refused_by_family() {
        assert!(parse_wave(&e([true, false], 4, 0, &[1, 2, 3]), "e.wfm").unwrap_err().contains("truncated"));
        assert!(parse_wave(&e([false, false], 4, 0, &[]), "e.wfm").unwrap_err().contains("no waveform"));
        // the shorter header of a DS1000C: 256 bytes, then the 20 samples
        assert!(parse_wave(&[&e([true, false], 20, 0, &[])[..256], &[0u8; 20][..]].concat(), "c.wfm").unwrap_err().contains("DS1000C"));
        for (first, family) in [([1u8, 0xFF, 0xFF, 0xFF, 0xA5, 0xA5], "DS1000Z"), ([0xA5, 0xA5, 0x38, 0, 0, 0], "DS2000"), ([0xA5, 0xA5, 0xA4, 1, 0, 0], "DS1000B"), ([0xA1, 0xA5, 0, 0, 0, 0], "DS1000C")] {
            let refused = parse_wave(&[&first[..], &[0u8; 64][..]].concat(), "r.wfm").unwrap_err();
            assert!(refused.contains(family) && refused.contains("not read"), "{refused}");
        }
    }
}

#[cfg(test)]
mod rigol_csv_file_tests {
    use super::*;

    fn close(seen: f64, wanted: f64) -> bool {
        (seen - wanted).abs() <= wanted.abs() * 1e-9
    }

    /// The lines of each test are the text of files saved by real scopes, from the RigolWFM project's captures.
    fn read(lines: &[&str], end: &str) -> Result<Vec<WaveTrace>, String> {
        let text: String = lines.iter().map(|line| format!("{line}{end}")).collect();
        parse_wave(text.as_bytes(), "rigol.csv")
    }

    #[test]
    fn a_ds1000z_file_counts_samples_and_gives_their_time_in_its_header() {
        // CH1 and CH4 of a DS1104Z, lines ended by a line feed
        let traces = read(&["X,CH1,CH4,Start,Increment,", "Sequence,Volt,Volt,-1.200000e-03,4.000000e-08", "0,2.86e+00,3.08e+00,", "1,2.86e+00,3.08e+00,", "2,2.84e+00,3.10e+00,"], "\n").unwrap();
        let seen: Vec<(&str, &[f32])> = traces.iter().map(|t| (t.label.as_str(), t.v.as_slice())).collect();
        assert_eq!(seen, [("CH1", &[2.86f32, 2.86, 2.84][..]), ("CH4", &[3.08f32, 3.08, 3.10][..])]);
        assert!(traces.iter().all(|t| close(t.t0, -1.2e-3) && close(t.dt, 4e-8)), "{:?}", (traces[0].t0, traces[0].dt));
        // four channels of a DS1054Z, lines ended by a carriage return and a line feed
        let four = read(&["X,CH1,CH2,CH3,CH4,Start,Increment,", "Sequence,Volt,Volt,Volt,Volt,-3.000000e-07,5.000000e-10", "0,3.28e+00,9.60e-01,-1.60e-01,0.00e+00,", "1,3.28e+00,9.60e-01,-1.60e-01,0.00e+00,", "2,3.20e+00,9.60e-01,-8.00e-02,0.00e+00,"], "\r\n").unwrap();
        assert_eq!(four.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH2", "CH3", "CH4"]);
        assert_eq!(four[2].v, [-0.16, -0.16, -0.08]);
        assert!(four.iter().all(|t| close(t.t0, -3e-7) && close(t.dt, 5e-10)));
    }

    #[test]
    fn a_ds2000_and_a_ds4000_file_are_read_the_same_way() {
        // one channel of a DS2072A: its second line ends with a comma
        let one = read(&["X,CH1,Start,Increment,", "Sequence,Volt,-3.500000e-03,5.000000e-06,", "0,2.400000e-02,", "1,8.000000e-03,", "2,2.400000e-02,"], "\r\n").unwrap();
        assert_eq!((one.len(), one[0].label.as_str(), one[0].v.as_slice()), (1, "CH1", &[0.024f32, 0.008, 0.024][..]));
        assert!(close(one[0].t0, -3.5e-3) && close(one[0].dt, 5e-6));
        // a DS4024 starts its count at 22: the first sample is 22 steps after the start
        let late = read(&["X,CH1,CH2,Start,Increment,", "Sequence,Volt,Volt,-1.400000e-03,2.000000e-06,", "22,3.125000e-02,6.250000e-03,", "23,-6.250000e-02,-6.250000e-03,", "24,3.125000e-02,6.250000e-03,"], "\r\n").unwrap();
        assert!(close(late[0].t0, -1.4e-3 + 22.0 * 2e-6) && close(late[0].dt, 2e-6), "{:?}", (late[0].t0, late[0].dt));
        assert_eq!(late[1].v, [0.00625, -0.00625, 0.00625]);
    }

    #[test]
    fn a_file_with_a_time_column_keeps_its_times_and_takes_the_channels_of_its_header() {
        // DS1102E: seconds in the first column
        let e = read(&["X,CH1,", "Second,Volt,", "-5.9999998e-06,4.40000e+00,", "-5.9800000e-06,4.32000e+00,", "-5.9600002e-06,4.24000e+00,"], "\r\n").unwrap();
        assert_eq!((e.len(), e[0].label.as_str(), e[0].v.as_slice()), (1, "CH1", &[4.4f32, 4.32, 4.24][..]));
        assert!(close(e[0].t0, -5.9999998e-06) && (e[0].dt - 2e-8).abs() < 1e-12);
        // DS1204B: no name over the first column, CH2 and CH4 on, a comma and a space at the end of a row
        let b = read(&[",CH2,CH4", "Second,Volt,Volt", "1.63720e-02,1.48e+01,1.36e+01, ", "1.63760e-02,1.48e+01,1.36e+01, ", "1.63800e-02,2.08e+01,1.36e+01, "], "\r\n").unwrap();
        assert_eq!(b.iter().map(|t| (t.label.as_str(), t.v.len())).collect::<Vec<_>>(), [("CH2", 3), ("CH4", 3)]);
        // DS1102D: the unit in the name of a column, no second header line
        let d = read(&["X,CH 1 (V),CH 2 (V)", "-0.00468800,8.08,8.40", "-0.00467799,4.88,8.40", "-0.00466798,4.88,8.40"], "\n").unwrap();
        assert_eq!(d.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), ["CH1", "CH2"]);
        assert_eq!(d[0].v, [8.08, 4.88, 4.88]);
    }

    #[test]
    fn a_file_that_counts_samples_without_saying_their_time_is_refused() {
        let refused = read(&["X,CH1,Start,Increment,", "Sequence,Volt,", "0,2.400000e-02,", "1,8.000000e-03,"], "\n").unwrap_err();
        assert!(refused.contains("Start and Increment"), "{refused}");
        assert!(read(&["X,CH1,Start,Increment,", "Sequence,Volt,-3.5e-03,0", "0,1,", "1,2,"], "\n").is_err());
        // another program's file with the same first cell is read as before
        let other = read(&["X,Y", "0,1", "1,2", "2,3"], "\n").unwrap();
        assert_eq!((other.len(), other[0].label.as_str(), other[0].dt), (1, "rigol.csv", 1.0));
    }

    #[test]
    fn a_ds1000z_waveform_file_is_refused_with_the_way_round_it() {
        let refused = parse_wave(&[&[1u8, 0xFF, 0xFF, 0xFF, 0xA5, 0xA5][..], &[0u8; 64][..]].concat(), "z.wfm").unwrap_err();
        assert!(refused.contains("DS1000Z") && refused.contains("vertical offset") && refused.contains("save the waveform as CSV"), "{refused}");
    }
}

#[cfg(test)]
mod csv_time_step_tests {
    use super::*;

    #[test]
    fn the_time_between_samples_is_taken_over_the_whole_column() {
        // 3600 samples over 500 us, the times written with five digits, as a capture saved by Probewright has them:
        // the first step alone reads 140 ns, the true one is 138.9 ns
        let mut text = String::from("time_s,voltage_v\n");
        let step = 5e-4 / 3600.0;
        for i in 0..3600 {
            text += &format!("{:.4e},{}\n", -2.5e-4 + i as f64 * step, i % 7);
        }
        let t = &parse_wave(text.as_bytes(), "capture.csv").unwrap()[0];
        assert_eq!(t.v.len(), 3600);
        assert!((t.dt / step - 1.0).abs() < 2e-5, "{} against {step}", t.dt);
        assert_eq!(t.t0, -2.5e-4);
    }

    #[test]
    fn two_first_lines_at_the_same_rounded_time_are_still_a_waveform() {
        // a RIGOL DS1042C writes three digits: its first lines repeat a time
        let text = "X,CH1\nSecond,Volt,\n-5.11e-03,2.00e-02\n-5.11e-03,2.00e-02\n-5.10e-03,4.00e-02\n-5.10e-03,4.00e-02\n-5.09e-03,2.00e-02\n";
        let t = &parse_wave(text.as_bytes(), "c.csv").unwrap()[0];
        assert_eq!(t.v.len(), 5);
        assert!((t.dt - 5e-6).abs() < 1e-12, "{}", t.dt);
        // one line, or times that do not advance, are not a waveform
        assert!(parse_wave(b"0,1\n", "one.csv").is_err());
        assert!(parse_wave(b"1,1\n1,2\n1,3\n", "flat.csv").is_err());
        assert!(parse_wave(b"3,1\n2,2\n1,3\n", "back.csv").is_err());
    }
}
