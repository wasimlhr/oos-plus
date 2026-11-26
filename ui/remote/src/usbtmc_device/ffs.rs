// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The files of the FunctionFS mount (`ep0` .. `ep3`, Linux `Documentation/usb/functionfs.rst`) and the threads that
//! wait on them; the protocol is `super::Device`. Plain file reads and writes, no ioctl.
//!
//! - The service thread waits for the mount `oos-gadget` makes, writes the descriptors and strings to `ep0` (the
//!   function is then ready, and `bind` asks the script to bind the gadget), opens `ep1` (Bulk-OUT), `ep2` (Bulk-IN)
//!   and `ep3` (Interrupt-IN), and reads the events of `ep0`: ENABLE, DISABLE, UNBIND and the setup packets, which
//!   `Device::control` answers (a write of the answer, or a read that stalls). `ep0` is read without waiting (every
//!   10 ms when quiet), so the thread sees the switch go off.
//! - One thread per endpoint: Bulk-OUT reads what `Device::out_want` says and hands it to `Device::bulk_out`;
//!   Bulk-IN writes what `Device::next_in` gives, in pieces of 16 KiB, and ends an aborted transfer with a short
//!   packet; Interrupt-IN writes the status byte notifications. A read or write of a transfer blocks until the host
//!   takes part in it; when the host is gone or the gadget is unbound the kernel ends it with an error. The endpoint
//!   files are opened non-blocking, so a read or write while no host has configured the interface returns at once.
//!
//! Closing every file of the function lets the kernel unbind the gadget (FunctionFS, also on a crash of oos-ui); the
//! next session writes the descriptors again and binds it again.

use super::{events, Control, Event, Setup, Shared, State, EVENT, FS_PACKET};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};
use std::sync::Arc;
use std::time::Duration;

/// How often `ep0` is read while nothing happens, and the mount looked at while there is no session.
const EP0_POLL: Duration = Duration::from_millis(10);
const MOUNT_POLL: Duration = Duration::from_millis(250);
/// Bytes per write of a Bulk-IN transfer: whole packets at both speeds, so only the transfer's last piece is short.
const IN_PIECE: usize = 16 << 10;
/// How long a thread waits for the device before it looks at its session again, in ms.
const WAIT_MS: u64 = 100;

fn open(p: &Path) -> io::Result<File> {
    OpenOptions::new().read(true).write(true).custom_flags(libc::O_NONBLOCK).open(p)
}

/// A read or write of an endpoint that no host has enabled (EAGAIN with O_NONBLOCK), or whose host went (ESHUTDOWN).
fn idle(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(libc::ESHUTDOWN)
}

/// A write to `ep0` that the kernel may refuse for a moment (its lock taken, O_NONBLOCK).
fn write_ep0(f: &mut File, b: &[u8]) -> io::Result<usize> {
    for _ in 0..100 {
        match f.write(b) {
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => std::thread::sleep(EP0_POLL),
            r => return r,
        }
    }
    Err(io::ErrorKind::WouldBlock.into())
}

/// The service thread: a session whenever the mount is there and nobody else holds it, while the switch is on.
pub fn service(shared: &Arc<Shared>, mount: &Path, bind: fn()) {
    let mut said = String::new();
    loop {
        serve(shared, mount, bind, &mut said);
        shared.dev().reset();
        shared.set_state(State::Off);
        if shared.may_end() {
            return;
        }
    }
}

fn serve(shared: &Arc<Shared>, mount: &Path, bind: fn(), said: &mut String) {
    while shared.wanted() {
        shared.set_state(State::Starting);
        // ep1 appears once descriptors are written: another program holds the function then
        if mount.join("ep0").exists() && !mount.join("ep1").exists() {
            let r = session(shared, mount, bind);
            let text = r.err().unwrap_or_default();
            if !text.is_empty() && text != *said {
                eprintln!("oos-ui: USBTMC: {}", text);
            }
            *said = text;
        }
        std::thread::sleep(MOUNT_POLL);
    }
}

/// One session: descriptors written, the endpoints served until the gadget is unbound, `ep0` fails or the switch goes
/// off. The endpoint threads end once their file returns (the unbind ends a transfer that waits for the host).
fn session(shared: &Arc<Shared>, mount: &Path, bind: fn()) -> Result<(), String> {
    let at = |e: io::Error, what: &str| format!("{}: {}", what, e);
    let mut ep0 = open(&mount.join("ep0")).map_err(|e| at(e, "ep0"))?;
    write_ep0(&mut ep0, &super::descriptors()).map_err(|e| at(e, "descriptors"))?;
    write_ep0(&mut ep0, &super::strings()).map_err(|e| at(e, "strings"))?;
    let out = open(&mount.join("ep1")).map_err(|e| at(e, "ep1"))?;
    let inp = open(&mount.join("ep2")).map_err(|e| at(e, "ep2"))?;
    let intr = open(&mount.join("ep3")).map_err(|e| at(e, "ep3"))?;
    shared.dev().reset();
    shared.set_state(State::NoComputer);
    let alive = Arc::new(AtomicBool::new(true));
    let spawn = |name: &str, run: fn(&Shared, &AtomicBool, File), f: File| {
        let (shared, alive) = (shared.clone(), alive.clone());
        std::thread::Builder::new().name(name.into()).spawn(move || run(&shared, &alive, f))
    };
    let threads =
        [spawn("usbtmc-out", bulk_out, out), spawn("usbtmc-in", bulk_in, inp), spawn("usbtmc-intr", interrupt, intr)];
    // the function is ready: the gadget can be bound (oos-gadget waits for this when it is built)
    bind();
    let r = events_loop(shared, &mut ep0);
    alive.store(false, SeqCst);
    shared.dev().reset();
    shared.wake();
    drop(ep0);
    for t in threads.into_iter().flatten() {
        let _ = t.join();
    }
    r
}

fn events_loop(shared: &Shared, ep0: &mut File) -> Result<(), String> {
    let mut buf = [0u8; 4 * EVENT];
    // errors in a row: a setup the host cancelled (EIDRM) passes, one that stays (the function reset, EBADFD) ends
    // the session after about a second
    let mut errors = 0;
    while shared.wanted() {
        let n = match ep0.read(&mut buf) {
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                std::thread::sleep(EP0_POLL);
                continue;
            }
            Err(e) => {
                errors += 1;
                if errors > 100 {
                    return Err(format!("ep0: {}", e));
                }
                std::thread::sleep(EP0_POLL);
                continue;
            }
        };
        errors = 0;
        for ev in events(&buf[..n]) {
            match ev {
                Event::Enable => {
                    shared.dev().reset();
                    shared.set_state(State::Connected);
                }
                Event::Disable => {
                    shared.dev().reset();
                    shared.set_state(State::NoComputer);
                }
                Event::Unbind => {
                    shared.dev().reset();
                    return Ok(());
                }
                Event::Setup(s) => {
                    let c = shared.dev().control(&s);
                    answer(ep0, &s, c);
                }
                _ => {}
            }
            shared.wake();
        }
    }
    Ok(())
}

/// The data stage of a setup packet: the answer written, or a stall (FunctionFS stalls on a read of `ep0` when the
/// request goes to the host, on a write when it comes from the host). A failure is the host's timeout.
fn answer(ep0: &mut File, s: &Setup, c: Control) {
    match c {
        Control::Answer(a) if s.to_host() => {
            if let Err(e) = write_ep0(ep0, &a[..a.len().min(s.length as usize)]) {
                eprintln!("oos-ui: USBTMC: request {:#04x}: {}", s.request, e);
            }
        }
        // the error of these is the stall itself (EL2HLT)
        Control::Stall if s.to_host() => {
            let _ = ep0.read(&mut [0u8; 1]);
        }
        _ => {
            let _ = write_ep0(ep0, &[0]);
        }
    }
}

/// Bulk-OUT: the stream of transfers into the device; a transfer it does not take halts the endpoint (a write to an
/// OUT endpoint does that in FunctionFS) until the host clears it.
fn bulk_out(shared: &Shared, alive: &AtomicBool, mut f: File) {
    let mut buf = vec![0u8; super::MAX_READ];
    while alive.load(SeqCst) {
        let (want, epoch) = {
            let d = shared.dev();
            (d.out_want(), d.out_epoch())
        };
        if want == 0 {
            drop(shared.wait(shared.dev(), WAIT_MS));
            continue;
        }
        match f.read(&mut buf[..want]) {
            Ok(0) => {}
            Ok(n) => {
                let r = shared.dev().bulk_out(epoch, &buf[..n]);
                shared.wake();
                if let Err(super::Halt(why)) = r {
                    eprintln!("oos-ui: USBTMC: Bulk-OUT halted: {}", why);
                    let _ = f.write(&[0]);
                }
            }
            Err(e) if idle(&e) => {
                drop(shared.wait(shared.dev(), WAIT_MS));
            }
            Err(e) => {
                eprintln!("oos-ui: USBTMC: Bulk-OUT: {}", e);
                std::thread::sleep(Duration::from_millis(WAIT_MS));
            }
        }
    }
}

/// Bulk-IN: each transfer in pieces of whole packets, its last piece short; an aborted one is ended with a short
/// packet of four zeros (the host reads up to a short packet after the abort and drops what it read).
fn bulk_in(shared: &Shared, alive: &AtomicBool, mut f: File) {
    while alive.load(SeqCst) {
        let t = {
            let mut d = shared.dev();
            loop {
                if !alive.load(SeqCst) {
                    return;
                }
                if let Some(t) = d.next_in() {
                    break t;
                }
                d = shared.wait(d, WAIT_MS);
            }
        };
        let (mut ok, mut short) = (true, false);
        for piece in t.chunks(IN_PIECE) {
            if shared.dev().in_aborted() {
                break;
            }
            if let Err(e) = write_piece(shared, alive, &mut f, piece) {
                if !idle(&e) {
                    eprintln!("oos-ui: USBTMC: Bulk-IN: {}", e);
                }
                ok = false;
                break;
            }
            shared.dev().in_written(piece.len());
            short = !piece.len().is_multiple_of(FS_PACKET);
        }
        if ok && !short && shared.dev().in_aborted() {
            let _ = f.write(&[0; 4]);
        }
        shared.dev().in_done();
        shared.wake();
    }
}

/// One piece of a Bulk-IN transfer, all of it: `WouldBlock` (the file is O_NONBLOCK, a slow host has not taken the
/// last packets yet) waits and tries again, until the piece is out, the transfer is aborted, the session ends or the
/// host is gone (ESHUTDOWN). Err only for those last cases and real errors.
fn write_piece(shared: &Shared, alive: &AtomicBool, f: &mut File, mut piece: &[u8]) -> io::Result<()> {
    while !piece.is_empty() {
        match f.write(piece) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => piece = &piece[n..],
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                if !alive.load(SeqCst) || shared.dev().in_aborted() {
                    return Err(e);
                }
                drop(shared.wait(shared.dev(), WAIT_MS));
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Interrupt-IN: the status byte with its tag, after READ_STATUS_BYTE.
fn interrupt(shared: &Shared, alive: &AtomicBool, mut f: File) {
    while alive.load(SeqCst) {
        let n = {
            let mut d = shared.dev();
            loop {
                if !alive.load(SeqCst) {
                    return;
                }
                if let Some(n) = d.take_notification() {
                    break n;
                }
                d = shared.wait(d, WAIT_MS);
            }
        };
        let _ = f.write(&n);
    }
}
