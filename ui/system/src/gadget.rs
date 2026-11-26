// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The rear USB port as a USB device for one computer (docs/usb-device.md): `/bin/oos-gadget` builds the USB gadget,
//! with the network over the cable (ADR 0079: RNDIS and ECM, the scope's fixed address, the computer's by DHCP) and
//! the USBTMC instrument (ADR 0080: a FunctionFS function, served by `oos_remote::usbtmc_device`). oos-ui asks the
//! script to build or take down the gadget when `usb.net` or `usb.tmc` changes, to bind it once the USBTMC
//! descriptors are written (`bind`), shows its state, and lets the computer on the cable in like loopback
//! (`on_usb_link`, used by `netacl`).

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

/// The script that owns the gadget.
pub const SCRIPT: &str = "/bin/oos-gadget";
pub const LOG: &str = "/tmp/oos-gadget.log";
/// One line of state the script leaves: `on: 192.168.235.1`, `off`, `off: no USB device port`, ...
const STATE: &str = "/tmp/oos-gadget.state";
/// The USB prefix in force (`192.168.235.0/29`), only while the network over the cable is up.
const NET_FILE: &str = "/tmp/oos-gadget.net";

/// The script's state line; "not started" before it ran.
pub fn state() -> String {
    std::fs::read_to_string(STATE).map(|s| s.trim().to_string()).unwrap_or_else(|_| "not started".into())
}

/// The scope's address on the cable while the network is up (`on: <address>`), else None.
pub fn address(state: &str) -> Option<&str> {
    state.strip_prefix("on: ").map(str::trim).filter(|a| a.parse::<std::net::Ipv4Addr>().is_ok())
}

/// Whether `ip` is in the prefix of the USB link (`text`: what the script left in `NET_FILE`, None without it). Only
/// an IPv4 prefix of the script's form counts; anything else in the file lets nobody in.
pub fn in_usb_prefix(text: Option<&str>, ip: IpAddr) -> bool {
    let Some((net, bits)) = text.and_then(|t| t.trim().split_once('/')) else { return false };
    let (Ok(net), Ok(bits)) = (net.parse::<std::net::Ipv4Addr>(), bits.parse::<u32>()) else { return false };
    let ip = match ip {
        IpAddr::V4(a) => a,
        IpAddr::V6(a) => match a.to_ipv4_mapped() {
            Some(a) => a,
            None => return false,
        },
    };
    let mask = if (24..=32).contains(&bits) { u32::MAX << (32 - bits) } else { return false };
    u32::from(ip) & mask == u32::from(net) & mask
}

/// The computer on the rear USB port: an address of the USB prefix while the network over the cable is up. The
/// firewall lets it in on the gadget's interfaces only (`oos-firewall`); here it is the address alone, which other
/// interfaces cannot bring past the firewall.
pub fn on_usb_link(ip: IpAddr) -> bool {
    in_usb_prefix(std::fs::read_to_string(NET_FILE).ok().as_deref(), ip)
}

/// Builds the gadget again with the current settings or takes it down (`oos-gadget apply`), in a thread of its own:
/// the script waits for the device controller, the frame loop does not. Only on the scope, where the script is.
/// Applies run one after the other, and those asked for while one runs become one more run: the script reads
/// `usb.net` when it starts, so the last run follows the last change (an `on` still waiting for the controller can
/// not bind the gadget again after a later `off`).
pub fn apply() {
    if !std::path::Path::new(SCRIPT).exists() {
        return;
    }
    APPLY.request(|| {
        let step = crate::remote::Step { prog: SCRIPT, arg: "apply", log: LOG };
        if let Err(e) = crate::remote::script_runner()(&step) {
            eprintln!("oos-ui: {}", e);
        }
    });
}

static APPLY: Serial = Serial::new();

/// Binds the gadget once the USBTMC function's descriptors are written (`oos-gadget bind`): a FunctionFS function can
/// only be bound when its program is ready, so the script builds the gadget and leaves the bind to it. In a thread of
/// its own, one at a time; the script itself waits while an apply runs (its lock).
pub fn bind() {
    if !std::path::Path::new(SCRIPT).exists() {
        return;
    }
    BIND.request(|| {
        let step = crate::remote::Step { prog: SCRIPT, arg: "bind", log: LOG };
        if let Err(e) = crate::remote::script_runner()(&step) {
            eprintln!("oos-ui: {}", e);
        }
    });
}

static BIND: Serial = Serial::new();

/// Runs a job in a thread of its own, never two at once; requests while it runs are merged into one more run.
struct Serial {
    pending: AtomicBool,
    running: AtomicBool,
}

impl Serial {
    const fn new() -> Serial {
        Serial { pending: AtomicBool::new(false), running: AtomicBool::new(false) }
    }

    fn request(&'static self, job: fn()) {
        self.pending.store(true, SeqCst);
        if self.running.swap(true, SeqCst) {
            return;
        }
        std::thread::spawn(move || loop {
            while self.pending.swap(false, SeqCst) {
                job();
            }
            self.running.store(false, SeqCst);
            // a request between the last look and the store above: run it here, unless a new thread took it
            if !self.pending.load(SeqCst) || self.running.swap(true, SeqCst) {
                break;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    // Break it catches: the computer on the cable shut out by `net.allow` (no address list of its own), or the USB
    // prefix letting in addresses beside it, or a broken file letting in everyone
    #[test]
    fn the_usb_prefix_lets_in_the_computer_on_the_cable_only() {
        let f = Some("10.9.9.8/29\n");
        for a in ["10.9.9.8", "10.9.9.9", "::ffff:10.9.9.9"] {
            assert!(in_usb_prefix(f, ip(a)), "{}", a);
        }
        for a in ["10.9.9.1", "10.0.0.9", "10.1.2.3", "fe80::1"] {
            assert!(!in_usb_prefix(f, ip(a)), "{}", a);
        }
        // the network over the cable is down, or the file is not the script's: nobody
        for t in [None, Some(""), Some("garbage"), Some("0.0.0.0/0"), Some("10.9.9.8/x"), Some("fd00::/8")] {
            assert!(!in_usb_prefix(t, ip("10.9.9.9")), "{:?}", t);
        }
    }

    // Break it catches: two applies at once (an `on` waiting for the controller binding the gadget again after a
    // later `off`), or a change asked for while one runs left without a run after it
    #[test]
    fn applies_run_one_at_a_time_and_the_last_request_gets_a_run() {
        use std::sync::atomic::AtomicUsize;
        use std::time::{Duration, Instant};
        static S: Serial = Serial::new();
        static GO: AtomicBool = AtomicBool::new(false);
        static NOW: AtomicUsize = AtomicUsize::new(0);
        static MOST: AtomicUsize = AtomicUsize::new(0);
        static RUNS: AtomicUsize = AtomicUsize::new(0);
        static ASKED: AtomicUsize = AtomicUsize::new(0);
        static SEEN: AtomicUsize = AtomicUsize::new(0);
        fn job() {
            let n = NOW.fetch_add(1, SeqCst) + 1;
            MOST.fetch_max(n, SeqCst);
            SEEN.store(ASKED.load(SeqCst), SeqCst);
            // the first run holds until every other request is made, whatever the speed of the computer
            let end = Instant::now() + Duration::from_secs(10);
            while !GO.load(SeqCst) && Instant::now() < end {
                std::thread::yield_now();
            }
            RUNS.fetch_add(1, SeqCst);
            NOW.fetch_sub(1, SeqCst);
        }
        for i in 1..=10 {
            ASKED.store(i, SeqCst);
            S.request(job);
            if i == 1 {
                // the first run has started before the others are asked for
                let end = Instant::now() + Duration::from_secs(10);
                while SEEN.load(SeqCst) != 1 && Instant::now() < end {
                    std::thread::yield_now();
                }
            }
        }
        GO.store(true, SeqCst);
        let end = Instant::now() + Duration::from_secs(10);
        while (S.running.load(SeqCst) || RUNS.load(SeqCst) < 2) && Instant::now() < end {
            std::thread::yield_now();
        }
        assert_eq!(MOST.load(SeqCst), 1, "two runs at once");
        assert_eq!(SEEN.load(SeqCst), 10, "the last request had no run after it");
        assert_eq!(RUNS.load(SeqCst), 2, "the nine requests during the first run are one more run");
    }

    #[test]
    fn the_address_of_the_state_line() {
        assert_eq!(address("on: 192.168.235.1"), Some("192.168.235.1"));
        for s in ["off", "off: no USB device port", "not started", "on: x"] {
            assert_eq!(address(s), None, "{}", s);
        }
    }
}
