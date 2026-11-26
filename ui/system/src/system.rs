// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Safe wrappers for the OS calls of oos-ui that need `unsafe`: thread placement (`sched_setaffinity`, `SCHED_IDLE`
//! with a nice fallback for the threads of the web remote, `SCHED_FIFO` for the acquisition thread), file-system
//! statistics (`statvfs`), setting the clock (`clock_settime`) and the socket options std has no method for
//! (`setsockopt`). Each wrapper holds its `unsafe` once, so its callers stay safe; each makes the same call with the
//! same arguments the callers made before and leaves the choice to ignore, log or pass on an error to them. A new OS
//! call goes here as a function of its own, behind the `cfg` its callers need, with a test where the host can run it.

use std::io;
#[cfg(unix)]
use std::os::fd::AsRawFd;

/// The calling thread, and the threads and processes it starts after this, onto `cores`; nothing when it is empty.
#[cfg(target_os = "linux")]
pub fn pin_current_thread(cores: &[usize]) -> io::Result<()> {
    pin_thread(0, cores)
}

/// Thread `tid` (0: the calling thread) onto `cores`; nothing when it is empty.
#[cfg(target_os = "linux")]
pub fn pin_thread(tid: libc::pid_t, cores: &[usize]) -> io::Result<()> {
    if cores.is_empty() {
        return Ok(());
    }
    // SAFETY: zero is a valid empty CPU set, CPU_SET only writes into it (an index beyond it panics on the slice), and
    // the kernel reads the local set during the call with its exact size.
    let rc = unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        for &c in cores {
            libc::CPU_SET(c, &mut set);
        }
        libc::sched_setaffinity(tid, std::mem::size_of::<libc::cpu_set_t>(), &set)
    };
    status(rc)
}

/// The calling thread to scheduling class `SCHED_IDLE`, or to nice value `nice` where that fails; returns the thread's
/// id (for `pin_thread`).
#[cfg(target_os = "linux")]
pub fn idle_current_thread(nice: i32) -> libc::pid_t {
    sched_current_thread(libc::SCHED_IDLE, 0, Some(nice)).0
}

/// The calling thread to `SCHED_FIFO` at `priority` (1: the lowest real-time priority, above every normal thread).
/// The threads it starts after this run at normal priority again (`SCHED_RESET_ON_FORK`).
#[cfg(target_os = "linux")]
pub fn fifo_current_thread(priority: i32) -> io::Result<()> {
    sched_current_thread(libc::SCHED_FIFO | libc::SCHED_RESET_ON_FORK, priority, None).1
}

/// The calling thread to scheduling class `policy` at `priority`, else (with `nice`) to that nice value: its id and
/// whether the class was taken.
#[cfg(target_os = "linux")]
fn sched_current_thread(policy: libc::c_int, priority: i32, nice: Option<i32>) -> (libc::pid_t, io::Result<()>) {
    // SAFETY: gettid takes no pointer; zero fills every field of sched_param (musl's struct has more fields) before the
    // priority is set; the kernel reads the local parameter during the call and checks policy and priority.
    unsafe {
        let tid = libc::syscall(libc::SYS_gettid) as libc::pid_t;
        let mut p: libc::sched_param = std::mem::zeroed();
        p.sched_priority = priority;
        // the system call itself: musl's sched_setscheduler() is a stub that always fails
        let rc = libc::syscall(libc::SYS_sched_setscheduler, tid, policy, &p as *const libc::sched_param);
        let taken = status(rc as libc::c_int);
        if let (Err(_), Some(n)) = (&taken, nice) {
            libc::setpriority(libc::PRIO_PROCESS, tid as libc::id_t, n);
        }
        (tid, taken)
    }
}

/// (available, total) bytes of the file system holding `path`: none without `statvfs` (the oos-plus Windows desktop
/// build).
#[cfg(not(unix))]
pub fn fs_stats(_path: &str) -> io::Result<(u64, u64)> {
    Err(io::Error::new(io::ErrorKind::Unsupported, "no statvfs on this host"))
}

/// (available, total) bytes of the file system holding `path`.
#[cfg(unix)]
pub fn fs_stats(path: &str) -> io::Result<(u64, u64)> {
    let c = std::ffi::CString::new(path)?;
    // SAFETY: statvfs is a C output record for which zero is valid; the path is NUL terminated and both live through
    // the call.
    let s = unsafe {
        let mut s: libc::statvfs = std::mem::zeroed();
        status(libc::statvfs(c.as_ptr(), &mut s))?;
        s
    };
    let fr = s.f_frsize as u64;
    Ok((s.f_bavail as u64 * fr, s.f_blocks as u64 * fr))
}

/// Sets `CLOCK_REALTIME` to `unix` seconds, with their fraction as nanoseconds.
#[cfg(target_os = "linux")]
pub fn set_realtime_clock(unix: f64) -> io::Result<()> {
    let ts = realtime_spec(unix);
    // SAFETY: ts is an initialised timespec that lives through the call; the kernel checks its values.
    status(unsafe { libc::clock_settime(libc::CLOCK_REALTIME, &ts) })
}

#[cfg(any(target_os = "linux", test))]
fn realtime_spec(unix: f64) -> libc::timespec {
    libc::timespec { tv_sec: unix.floor() as _, tv_nsec: ((unix - unix.floor()) * 1e9) as _ }
}

/// Socket option `name` of `level` to the int `value` (setsockopt with a pointer to it and its size). An option that
/// takes an `in_addr` (IP_MULTICAST_IF) takes its four bytes as `i32::from_ne_bytes(address.octets())`.
#[cfg(unix)]
pub fn set_socket_option(s: &impl AsRawFd, level: libc::c_int, name: libc::c_int, value: i32) -> io::Result<()> {
    // SAFETY: a pointer to an int of the size given, for the duration of the call
    status(unsafe {
        libc::setsockopt(
            s.as_raw_fd(),
            level,
            name,
            &value as *const i32 as *const libc::c_void,
            std::mem::size_of::<i32>() as libc::socklen_t,
        )
    })
}

/// 0: Ok; otherwise the error the call left in errno.
fn status(rc: libc::c_int) -> io::Result<()> {
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Nice value of the threads of the web remote (Linux; a thread's own value), where `SCHED_IDLE` is not to be had.
pub const WEB_NICE: i32 = 10;

/// The threads of the web remote (encoder, senders) run only on what the scope leaves over: scheduling class
/// `SCHED_IDLE` (they get a core only while nothing else wants it), and only on the slower cores of a big.LITTLE SoC.
/// Which cores those are, the kernel says at run time (`cpu_capacity`, `placement::little_cores`); no number of cores
/// is built in. On the RK3399 they are the four A53, and the two A72 stay for the frame loop: assumed from the SoC's
/// documentation, not checked here. Nice 10 where `SCHED_IDLE` fails.
pub fn lower_priority() {
    // not in the tests (of oos-ui too, which turns on `testkit`): with every core busy with tests in parallel (CI), a
    // thread at SCHED_IDLE got no time for seconds, and a viewer waiting for a picture got 304 or 503 instead
    #[cfg(target_os = "linux")]
    if !cfg!(any(test, feature = "testkit")) {
        let tid = crate::system::idle_current_thread(WEB_NICE);
        let _ = crate::system::pin_thread(tid, &crate::placement::little_cores());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The policy and real-time priority of the calling thread, from /proc (fields 41 and 40 of its stat).
    #[cfg(target_os = "linux")]
    fn policy_of_this_thread() -> (u32, u32) {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap();
        let f: Vec<&str> = stat[stat.rfind(')').unwrap() + 2..].split_whitespace().collect();
        (f[38].parse().unwrap(), f[37].parse().unwrap())
    }

    // Break it catches: the policy or priority not set as asked, the threads it starts left real-time, an error not
    // passed on. An unprivileged host may refuse: then the thread stays as it was.
    #[cfg(target_os = "linux")]
    #[test]
    fn fifo_takes_the_thread_and_not_the_ones_it_starts() {
        std::thread::spawn(|| match fifo_current_thread(1) {
            Ok(()) => {
                assert_eq!(policy_of_this_thread(), (libc::SCHED_FIFO as u32, 1));
                let child = std::thread::spawn(policy_of_this_thread).join().unwrap();
                assert_eq!(child, (libc::SCHED_OTHER as u32, 0), "SCHED_RESET_ON_FORK: started threads are normal");
            }
            Err(e) => {
                assert_eq!(e.kind(), io::ErrorKind::PermissionDenied, "{e}");
                assert_eq!(policy_of_this_thread(), (libc::SCHED_OTHER as u32, 0));
            }
        })
        .join()
        .unwrap();
    }

    #[test]
    fn realtime_seconds_and_fraction_keep_the_conversion() {
        for (unix, seconds, nanos) in [
            (0.0, 0, 0),
            (1_794_398_400.25, 1_794_398_400, 250_000_000),
            (42.0, 42, 0),
            (-0.25, -1, 750_000_000),
            (1.999_999_999_9, 1, 999_999_999),
        ] {
            let ts = realtime_spec(unix);
            assert_eq!((ts.tv_sec, ts.tv_nsec), (seconds, nanos));
        }
    }

    #[test]
    fn filesystem_stats_report_available_bytes_and_errors() {
        let root = env!("CARGO_MANIFEST_DIR");
        let (available, total) = fs_stats(root).unwrap();
        assert!(total > 0);
        assert!(available <= total);
        assert_eq!(fs_stats(".\0").unwrap_err().kind(), io::ErrorKind::InvalidInput);
        assert!(fs_stats(&format!("{}/Cargo.toml/child", root)).is_err());
    }

    // Break it catches: a value or size the kernel does not get as given, an error not passed on.
    #[cfg(unix)]
    #[test]
    fn socket_options_reach_the_socket_and_errors_come_back() {
        let s = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        set_socket_option(&s, libc::IPPROTO_IP, libc::IP_MULTICAST_TTL, 255).unwrap();
        assert_eq!(s.multicast_ttl_v4().unwrap(), 255);
        set_socket_option(&s, libc::IPPROTO_IP, libc::IP_MULTICAST_IF, i32::from_ne_bytes([127, 0, 0, 1])).unwrap();
        assert!(set_socket_option(&s, libc::IPPROTO_IP, -1, 1).is_err());
    }

    /// The CPUs the calling thread may run on, as the kernel lists them (`0-3,5`).
    #[cfg(target_os = "linux")]
    fn affinity() -> String {
        let s = std::fs::read_to_string("/proc/thread-self/status").unwrap();
        s.lines().find_map(|l| l.strip_prefix("Cpus_allowed_list:")).unwrap().trim().to_string()
    }

    // Break it catches: an empty set that pins to nothing (or to CPU 0), the set of another thread changed, a child
    // that does not inherit the set.
    #[test]
    #[cfg(target_os = "linux")]
    fn affinity_pins_only_the_calling_thread_and_is_inherited() {
        let outside = affinity();
        std::thread::spawn(|| {
            let before = affinity();
            pin_current_thread(&[]).unwrap();
            assert_eq!(affinity(), before);
            let cpu: usize = before.split(|c: char| !c.is_ascii_digit()).next().unwrap().parse().unwrap();
            pin_current_thread(&[cpu]).unwrap();
            assert_eq!(affinity(), cpu.to_string());
            assert_eq!(std::thread::spawn(affinity).join().unwrap(), cpu.to_string());
        })
        .join()
        .unwrap();
        assert_eq!(affinity(), outside);
    }
}
