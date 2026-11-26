// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Clock floors (cpufreq and devfreq minimum clocks). The governors raise a clock only after they saw the load, and a
//! record's work comes in short bursts: the first frames after RUN, and each burst after a quiet spell, ran at the
//! lowest steps. While the hardware acquisition runs, the big cores' `scaling_min_freq` is at least `CPU_RUN_KHZ` and
//! the GPU's devfreq `min_freq` at least `GPU_RUN_HZ`; on STOP and when oos-ui ends, each minimum goes back to the
//! value it had before. `perf.clocks = off` in system.conf (or OOS_CLOCKS=0) leaves them alone; on by default, verified
//! on the unit (DHO924S, 2026-11-25; ADR 0010). While the GPU helper runs the GPU's floor is `GPU_HELPER_HZ` (`gpu.rs`), with or
//! without the switch. Each floor is the lowest step of the device's table at or above it (the highest step where the
//! table has none), never below the minimum already set. A minimum found at one of these floors without this run
//! having set it was left by an oos-ui that ended while running (a crash): it is taken as the table's lowest step.
//! Thermal limits still win: the kernel caps a minimum above the maximum the cooling leaves.

use std::path::{Path, PathBuf};

/// The big cores' lowest clock while the acquisition runs, kHz (RK3399: the A72 step at 1.416 GHz).
pub const CPU_RUN_KHZ: u64 = 1_416_000;
/// The GPU's lowest clock while the acquisition runs, Hz.
pub const GPU_RUN_HZ: u64 = 600_000_000;
/// The GPU's lowest clock while the helper runs, Hz: the load governor sees only the short work of each frame and
/// stays at the lowest step (200 MHz, an 8 ms frame); 400 MHz runs at the same voltage (rk3399-opp.dtsi), in 4 ms.
pub const GPU_HELPER_HZ: u64 = 400_000_000;

/// What the floors are for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Want {
    /// the hardware acquisition runs and `perf.clocks` is not off
    pub running: bool,
    /// the GPU helper runs
    pub gpu_helper: bool,
}

/// The minimums this module raised, with the values they had before.
#[derive(Debug, Default)]
pub struct Floors {
    saved: Vec<(PathBuf, u64)>,
}

/// One clock: its minimum file, its table of steps, the floor wanted.
struct Clock {
    min: PathBuf,
    steps: Vec<u64>,
    floor: Option<u64>,
    /// every floor this module sets on it
    own: &'static [u64],
}

fn number(p: &Path) -> Option<u64> {
    std::fs::read_to_string(p).ok()?.trim().parse().ok()
}

/// The clocks under `root` (`/sys`) with the floors of `want`: the cpufreq policy of the first big core (two kinds of
/// core, `cpu_capacity`), every devfreq device whose name ends in `.gpu`.
fn clocks(root: &Path, want: Want) -> Vec<Clock> {
    let mut out = Vec::new();
    let cpu = root.join("devices/system/cpu");
    let cap = crate::placement::capacities(&cpu);
    let max = cap.iter().map(|x| x.1).max();
    if let Some(&(big, _)) =
        cap.iter().filter(|x| Some(x.1) == max).min().filter(|_| cap.iter().any(|x| Some(x.1) != max))
    {
        let d = cpu.join(format!("cpu{}/cpufreq", big));
        let steps = std::fs::read_to_string(d.join("scaling_available_frequencies")).unwrap_or_default();
        let floor = want.running.then_some(CPU_RUN_KHZ);
        out.push(Clock { min: d.join("scaling_min_freq"), steps: parse(&steps), floor, own: &[CPU_RUN_KHZ] });
    }
    let mut gpus: Vec<PathBuf> = std::fs::read_dir(root.join("class/devfreq"))
        .map(|d| d.flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".gpu")).map(|e| e.path()).collect())
        .unwrap_or_default();
    gpus.sort();
    for d in gpus {
        let steps = std::fs::read_to_string(d.join("available_frequencies")).unwrap_or_default();
        let floor =
            [want.running.then_some(GPU_RUN_HZ), want.gpu_helper.then_some(GPU_HELPER_HZ)].into_iter().flatten().max();
        out.push(Clock { min: d.join("min_freq"), steps: parse(&steps), floor, own: &[GPU_RUN_HZ, GPU_HELPER_HZ] });
    }
    out
}

fn parse(steps: &str) -> Vec<u64> {
    steps.split_whitespace().filter_map(|v| v.parse().ok()).collect()
}

/// The lowest step of `steps` at or above `floor`, else the highest.
fn step(steps: &[u64], floor: u64) -> Option<u64> {
    steps.iter().copied().filter(|&s| s >= floor).min().or(steps.iter().copied().max())
}

impl Floors {
    /// The writes (minimum file, value, the value before any raise) that bring the clocks under `root` to the floors
    /// of `want`: a raised minimum without a floor goes back to its value before; nothing for a clock already where it
    /// should be, or without a table or a readable minimum. Reads only.
    pub fn plan(&self, root: &Path, want: Want) -> Vec<(PathBuf, u64, u64)> {
        let mut out = Vec::new();
        for c in clocks(root, want) {
            let (Some(now), Some(&lowest)) = (number(&c.min), c.steps.iter().min()) else { continue };
            let saved = self.saved.iter().find(|s| s.0 == c.min).map(|s| s.1);
            let left = c.own.iter().any(|&f| step(&c.steps, f) == Some(now)) && now != lowest;
            let base = saved.unwrap_or(if left { lowest } else { now });
            let target = c.floor.and_then(|f| step(&c.steps, f)).map_or(base, |s| s.max(base));
            if target != now {
                out.push((c.min, target, base));
            }
        }
        out
    }

    /// Writes `plan`, keeping the value each minimum had before its first raise; the errors, one line each.
    pub fn apply(&mut self, root: &Path, want: Want) -> Vec<String> {
        let mut errors = Vec::new();
        for (path, v, base) in self.plan(root, want) {
            if let Err(e) = std::fs::write(&path, v.to_string()) {
                errors.push(format!("{}: {}", path.display(), e));
                continue;
            }
            let at = self.saved.iter().position(|s| s.0 == path);
            match at {
                // back at the value before: no longer raised
                Some(i) if v == base => drop(self.saved.remove(i)),
                None if v != base => self.saved.push((path, base)),
                _ => {}
            }
        }
        errors
    }
}

/// `perf.clocks = off` in system.conf, or OOS_CLOCKS=0: no floors while the acquisition runs.
const OFF: crate::sysconf::Flag = crate::sysconf::Flag { key: "perf.clocks", env: Some("OOS_CLOCKS"), on: "off" };

/// The floors of the scope (`/sys`) and what they are for now.
#[cfg(target_os = "linux")]
static STATE: std::sync::Mutex<(Option<Want>, Floors, bool, Option<std::time::Instant>)> =
    std::sync::Mutex::new((None, Floors { saved: Vec::new() }, false, None));

/// `change` on what the floors are for; writes the minimums of `/sys` at the first call and when that changed. A write
/// that failed is tried again a second later (a floor left raised by a failed restore is put back once it can be); the
/// errors are said once. Not in the tests of oos-ui: they use `Floors` with a directory of their own.
fn update(change: impl FnOnce(&mut Want)) {
    #[cfg(target_os = "linux")]
    if !cfg!(any(test, feature = "testkit")) {
        let mut g = STATE.lock().unwrap_or_else(|e| e.into_inner());
        let (want, floors, said, retry) = &mut *g;
        let mut new = want.unwrap_or_default();
        change(&mut new);
        let due = retry.is_none_or(|t| t.elapsed() >= std::time::Duration::from_secs(1));
        if *want != Some(new) && due {
            let errors = floors.apply(Path::new("/sys"), new);
            if errors.is_empty() {
                (*want, *retry) = (Some(new), None);
            } else {
                // `want` stays as it was, so the next call writes again
                *retry = Some(std::time::Instant::now());
                if !*said {
                    eprintln!("oos-ui: clock floor: {}", errors.join(", "));
                    *said = true;
                }
            }
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = change;
}

/// The hardware acquisition runs (RUN) or not (STOP, and when oos-ui ends).
pub fn running(on: bool) {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let on = on && *ENABLED.get_or_init(|| !OFF.get());
    update(|w| w.running = on);
}

/// The GPU helper runs or not (`gpu.rs`).
pub fn gpu_helper(on: bool) {
    update(|w| w.gpu_helper = on);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_reads_both_files() {
        OFF.check_both_files();
    }

    /// An RK3399 under a scratch directory: four A53 and two A72, the A72 policy's minimum `cpu_min`, a GPU at
    /// `gpu_min` and another devfreq device.
    fn rk3399(cpu_min: u64, gpu_min: u64) -> PathBuf {
        static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("oos-clockfloor-{}-{}", std::process::id(), n));
        let put = |p: &str, v: &str| {
            let p = root.join(p);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, v).unwrap();
        };
        for c in 0..6 {
            put(&format!("devices/system/cpu/cpu{}/cpu_capacity", c), if c < 4 { "485\n" } else { "1024\n" });
        }
        put("devices/system/cpu/cpu4/cpufreq/scaling_available_frequencies", "408000 816000 1416000 1800000 \n");
        put("devices/system/cpu/cpu4/cpufreq/scaling_min_freq", &format!("{}\n", cpu_min));
        put("class/devfreq/ff9a0000.gpu/available_frequencies", "200000000 400000000 600000000 800000000\n");
        put("class/devfreq/ff9a0000.gpu/min_freq", &format!("{}\n", gpu_min));
        put("class/devfreq/dmc/available_frequencies", "200000000 800000000\n");
        put("class/devfreq/dmc/min_freq", "200000000\n");
        root
    }

    fn mins(root: &Path) -> (u64, u64, u64) {
        let r = |p: &str| number(&root.join(p)).unwrap();
        (
            r("devices/system/cpu/cpu4/cpufreq/scaling_min_freq"),
            r("class/devfreq/ff9a0000.gpu/min_freq"),
            r("class/devfreq/dmc/min_freq"),
        )
    }

    const RUN: Want = Want { running: true, gpu_helper: false };
    const HELPER: Want = Want { running: false, gpu_helper: true };
    const BOTH: Want = Want { running: true, gpu_helper: true };

    // Break it catches: a floor on the little cores or another devfreq device, STOP not restoring what was there
    // before, a minimum lowered by a floor, the helper's floor lost on STOP, a write each frame, a floor of a crashed
    // run kept for good.
    #[test]
    fn run_raises_the_a72_and_gpu_minimums_and_stop_restores_them() {
        let root = rk3399(816000, 200000000);
        let mut f = Floors::default();
        assert!(f.apply(&root, RUN).is_empty());
        assert_eq!(mins(&root), (1416000, 600000000, 200000000));
        assert!(f.plan(&root, RUN).is_empty(), "nothing to write while it stays");
        assert!(f.apply(&root, Want::default()).is_empty());
        assert_eq!(mins(&root), (816000, 200000000, 200000000));
        assert!(f.saved.is_empty());
        // with the GPU helper: 400 MHz, 600 while running, 400 again on STOP, the old minimum when it ends
        f.apply(&root, HELPER);
        assert_eq!(mins(&root).1, 400000000);
        f.apply(&root, BOTH);
        assert_eq!(mins(&root), (1416000, 600000000, 200000000));
        f.apply(&root, HELPER);
        assert_eq!(mins(&root), (816000, 400000000, 200000000));
        f.apply(&root, Want::default());
        assert_eq!(mins(&root), (816000, 200000000, 200000000));
        std::fs::remove_dir_all(&root).unwrap();
        // a minimum already above the floor stays
        let root = rk3399(1800000, 800000000);
        let mut f = Floors::default();
        assert!(f.plan(&root, BOTH).is_empty());
        f.apply(&root, BOTH);
        f.apply(&root, Want::default());
        assert_eq!(mins(&root), (1800000, 800000000, 200000000));
        std::fs::remove_dir_all(&root).unwrap();
        // floors left by an oos-ui that ended while running: the lowest steps once it stops, 400 MHz with the helper
        let root = rk3399(1416000, 600000000);
        let mut f = Floors::default();
        f.apply(&root, HELPER);
        assert_eq!(mins(&root), (408000, 400000000, 200000000));
        f.apply(&root, Want::default());
        assert_eq!(mins(&root), (408000, 200000000, 200000000));
        std::fs::remove_dir_all(&root).unwrap();
    }

    // Break it catches: a value outside the table written, a write on a SoC whose cores are all alike, a panic or a
    // write without the files.
    #[test]
    fn the_floor_is_a_step_of_the_table_and_missing_files_are_skipped() {
        let root = rk3399(408000, 200000000);
        std::fs::write(root.join("devices/system/cpu/cpu4/cpufreq/scaling_available_frequencies"), "408000 1200000")
            .unwrap();
        std::fs::write(root.join("class/devfreq/ff9a0000.gpu/available_frequencies"), "200000000 700000000").unwrap();
        let p = Floors::default().plan(&root, RUN);
        assert_eq!(p.iter().map(|w| w.1).collect::<Vec<_>>(), [1200000, 700000000]);
        for c in 4..6 {
            std::fs::write(root.join(format!("devices/system/cpu/cpu{}/cpu_capacity", c)), "485").unwrap();
        }
        assert_eq!(Floors::default().plan(&root, RUN).len(), 1, "all cores alike: the GPU only");
        std::fs::remove_dir_all(root.join("class")).unwrap();
        assert!(Floors::default().plan(&root, RUN).is_empty());
        std::fs::remove_dir_all(&root).unwrap();
        assert!(Floors::default().plan(&root, RUN).is_empty());
    }
}
