// Copyright (C) 2026 dekajoule
// Copyright (C) 2026 OOS contributors
// SPDX-License-Identifier: GPL-2.0-or-later

//! An external generator (ADR 0028): the models as a table of command templates, the connection (a socket on the LAN,
//! or USBTMC on the USB-A port, `crate::usbtmc`) and the thread that talks to it. The page is ui/extgen.rs; the Bode
//! sweep (ui/bode.rs) and whatever sweeps later send their batches through the same thread.
//!
//! A model that names an error query is asked after every command, so a setting the generator refuses ends the sweep
//! with what the generator said instead of a curve of the old setting. `*ESR?` is never sent: not every generator
//! answers it.

use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

// oos-plus: the table of models lives in the model (`crate::model::bode`), where upstream keeps the Bode presets
// a setup names by index; this module re-exports it.
pub use crate::model::bode::{external, BUILTIN, FIRST_EXTERNAL, FREE, GENERATORS};

/// The external models, in the order of the table.
pub fn externals() -> impl Iterator<Item = usize> {
    (0..GENERATORS.len()).filter(|&i| external(i))
}

/// The model whose `*IDN?` answer starts like `idn`.
pub fn for_idn(idn: &str) -> Option<usize> {
    let up = idn.trim().to_ascii_uppercase();
    externals().find(|&i| !GENERATORS[i].idn.is_empty() && up.starts_with(GENERATORS[i].idn))
}

/// Maker and model of an `*IDN?` answer: its first two fields. The serial number and the firmware stay off the
/// screen (a screenshot is passed on).
pub fn idn_short(idn: &str) -> String {
    let mut f = idn.split(',').map(str::trim).filter(|s| !s.is_empty());
    match (f.next(), f.next()) {
        (Some(a), Some(b)) => format!("{} {}", a, b),
        (Some(a), None) => a.to_string(),
        _ => String::new(),
    }
}

/// What a template is filled with.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vars {
    /// Hz
    pub f: f64,
    /// Vpp
    pub a: f64,
    /// channel of the generator, 0 = the first
    pub ch: usize,
    /// the generator drives a high impedance (the input of the scope), not 50 ohms
    pub hiz: bool,
}


/// The command lines of a template.
pub fn lines(tpl: &str, v: &Vars) -> Vec<String> {
    let fmt = oos_dsp::bode::fmt_hz;
    tpl.split('\n')
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.replace("{f}", &fmt(v.f)).replace("{a}", &fmt(v.a)).replace("{c}", &(v.ch + 1).to_string()).replace("{z}", if v.hiz { "INFinity" } else { "50" }))
        .collect()
}

/// Whether an answer of the error queue says "no error": `0`, `+0` or `0,"No error"`.
pub fn no_error(answer: &str) -> bool {
    let t = answer.trim().trim_start_matches('+');
    t == "0" || t.starts_with("0,")
}

/// The state of the connection.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Conn {
    #[default]
    Off,
    Connecting,
    /// connected: what to (`generator USB 5345:1235`, `generator 192.168.1.20:5555`)
    On(String),
    /// why not
    Failed(String),
}

/// What the thread tells the screen.
#[derive(Default)]
pub struct Shared {
    pub conn: Conn,
    /// the `*IDN?` answer, when the generator was asked and gave one
    pub idn: Option<String>,
    /// why the connection ended, or what the generator refused; whoever shows it takes it
    pub err: Option<String>,
    /// a message that ends nothing
    pub note: Option<String>,
    /// batches the thread is done with: a sweep waits for its batch before it measures
    pub done: u64,
}

/// Commands for the generator, sent in order.
pub struct Batch {
    pub lines: Vec<String>,
    /// the error query of the model (`Gen::err`)
    pub check: &'static str,
    pub pace_ms: u64,
}

/// Where the generator is.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    /// the first USBTMC instrument on the USB-A port, instead of `addr`
    pub usb: bool,
    /// `ip` or `ip:port` (port 5555 if none)
    pub addr: String,
    /// ask `*IDN?` on the LAN too (on USB it is always asked)
    pub identify: bool,
}

/// How long an answer may take.
const ANSWER: Duration = Duration::from_secs(2);
/// Entries of the error queue read at most to empty it.
const QUEUE: usize = 16;

/// Where the thread finds USB devices (none in the tests: they must never talk to a real instrument).
fn usb_root() -> &'static std::path::Path {
    std::path::Path::new(if cfg!(test) { "/nonexistent/oos-test" } else { crate::usbtmc::SYSFS })
}

/// The connection to an external generator.
enum Link {
    Tcp(std::net::TcpStream),
    Usb(crate::usbtmc::Usbtmc<crate::usbtmc::UsbFs>),
}

impl Link {
    /// Opens the connection: the first USBTMC device (`usb`), else `addr` (port 5555 if none). Returns it with its
    /// name for messages.
    fn open(usb: bool, addr: &str) -> Result<(Link, String), String> {
        if usb {
            let (t, d) = crate::usbtmc::connect(usb_root()).map_err(|e| format!("generator USB: {}", e))?;
            return Ok((Link::Usb(t), format!("generator USB {}", d.id)));
        }
        let target = if addr.contains(':') { addr.to_string() } else { format!("{}:5555", addr) };
        let what = format!("generator {}", target);
        let sock = target.parse::<std::net::SocketAddr>().map_err(|e| e.to_string()).and_then(|a| std::net::TcpStream::connect_timeout(&a, ANSWER).map_err(|e| e.to_string()));
        match sock {
            Ok(s) => {
                // an answer that does not come must not hold the thread for ever; small commands go out at once
                let _ = s.set_read_timeout(Some(ANSWER));
                let _ = s.set_nodelay(true);
                Ok((Link::Tcp(s), what))
            }
            Err(e) => Err(format!("{}: {}", what, e)),
        }
    }

    /// Sends one command line.
    fn send(&mut self, line: &str) -> Result<(), String> {
        use std::io::Write;
        match self {
            Link::Tcp(s) => s.write_all(format!("{}\n", line).as_bytes()).map_err(|e| e.to_string()),
            Link::Usb(t) => t.command(line),
        }
    }

    /// Sends a query and returns its answer without the line end.
    fn query(&mut self, line: &str) -> Result<String, String> {
        use std::io::{Read, Write};
        match self {
            Link::Usb(t) => t.query(line),
            Link::Tcp(s) => {
                // what an earlier query left behind (an answer that came after its time) is not this answer
                let mut old = [0u8; 256];
                while matches!(crate::net::read_now(s, &mut old), Ok(n) if n > 0) {}
                s.write_all(format!("{}\n", line).as_bytes()).map_err(|e| e.to_string())?;
                let mut a = Vec::new();
                let mut b = [0u8; 256];
                while !a.contains(&b'\n') {
                    match s.read(&mut b) {
                        Ok(0) => return Err("connection closed".into()),
                        Ok(n) => a.extend_from_slice(&b[..n]),
                        Err(e) if crate::net::later(&e) => return Err("no answer (timeout)".into()),
                        Err(e) => return Err(e.to_string()),
                    }
                }
                let end = a.iter().position(|&c| c == b'\n').unwrap_or(a.len());
                Ok(String::from_utf8_lossy(&a[..end]).trim_end_matches('\r').to_string())
            }
        }
    }
}

/// Why a batch did not go through.
enum Fail {
    /// the connection is gone
    Link(String),
    /// the generator answered with an error; the connection stays
    Refused(String),
}

/// Reads the error queue until it is empty.
fn empty_queue(link: &mut Link, check: &str) -> Result<(), String> {
    for _ in 0..QUEUE {
        if no_error(&link.query(check)?) {
            break;
        }
    }
    Ok(())
}

/// One batch: every line, and after each the error query when the model has one.
fn run(link: &mut Link, b: &Batch, fresh: &mut bool) -> Result<(), Fail> {
    let asked = !b.check.is_empty();
    // what the queue held before the first command is not from these commands
    if asked && *fresh {
        empty_queue(link, b.check).map_err(Fail::Link)?;
        *fresh = false;
    }
    for line in &b.lines {
        link.send(line).map_err(Fail::Link)?;
        if b.pace_ms > 0 {
            std::thread::sleep(Duration::from_millis(b.pace_ms));
        }
        if asked {
            let a = link.query(b.check).map_err(Fail::Link)?;
            if !no_error(&a) {
                let _ = empty_queue(link, b.check);
                return Err(Fail::Refused(format!("The generator refused {}: {}", line, a)));
            }
        }
    }
    Ok(())
}

/// The generator thread: connects, then sends every batch it receives. It ends when the sender is dropped or the
/// connection fails. `shared.conn` is `Connecting` from the caller on.
pub fn spawn(target: Target, shared: Arc<Mutex<Shared>>) -> mpsc::Sender<Batch> {
    let (tx, rx) = mpsc::channel::<Batch>();
    shared.lock().unwrap().conn = Conn::Connecting;
    std::thread::spawn(move || {
        let (mut link, what) = match Link::open(target.usb, &target.addr) {
            Ok(l) => l,
            Err(e) => {
                let mut s = shared.lock().unwrap();
                (s.conn, s.err) = (Conn::Failed(e.clone()), Some(e));
                return;
            }
        };
        if target.usb || target.identify {
            let at = if target.usb { "USB".to_string() } else { target.addr.clone() };
            let (idn, note) = match link.query("*IDN?") {
                Ok(idn) if !idn.is_empty() => (Some(idn.clone()), format!("Generator on {}: {}", at, idn_short(&idn))),
                Ok(_) => (None, format!("Generator on {}: empty answer to *IDN?", at)),
                Err(e) => (None, format!("Generator on {}: *IDN? failed ({}), sending anyway", at, e)),
            };
            let mut s = shared.lock().unwrap();
            (s.idn, s.note) = (idn, Some(note));
        }
        shared.lock().unwrap().conn = Conn::On(what.clone());
        let mut fresh = true;
        for b in rx {
            let r = run(&mut link, &b, &mut fresh);
            let mut s = shared.lock().unwrap();
            s.done += 1;
            match r {
                Ok(()) => {}
                Err(Fail::Refused(e)) => s.err = Some(e),
                Err(Fail::Link(e)) => {
                    let e = format!("{}: {}", what, e);
                    (s.conn, s.err) = (Conn::Failed(e.clone()), Some(e));
                    return;
                }
            }
        }
        shared.lock().unwrap().conn = Conn::Off;
    });
    tx
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};

    /// A generator on a local socket: it keeps every line it gets and answers the queries. `*IDN?` gets `idn`,
    /// `SYSTem:ERRor?` the entry a line left that holds `refuse` (-222), else no error. Returns its address and the
    /// lines; `CLOSED` follows the last one when the connection has ended.
    pub const CLOSED: &str = "<closed>";

    pub fn fake_generator(idn: &'static str, refuse: &'static str) -> (String, Arc<Mutex<Vec<String>>>) {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = l.local_addr().unwrap().to_string();
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        std::thread::spawn(move || {
            let Ok((s, _)) = l.accept() else { return };
            let mut w = s.try_clone().unwrap();
            let mut queue: Vec<String> = Vec::new();
            for line in BufReader::new(s).lines() {
                let Ok(line) = line else { break };
                g.lock().unwrap().push(line.clone());
                let answer = match line.as_str() {
                    "*IDN?" => Some(idn.to_string()),
                    "SYSTem:ERRor?" => Some(if queue.is_empty() { "0,\"No error\"".to_string() } else { queue.remove(0) }),
                    l if !refuse.is_empty() && l.contains(refuse) => {
                        queue.push("-222,\"Data out of range\"".into());
                        None
                    }
                    _ => None,
                };
                if let Some(a) = answer {
                    if w.write_all(format!("{}\n", a).as_bytes()).is_err() {
                        break;
                    }
                }
            }
            g.lock().unwrap().push(CLOSED.into());
        });
        (addr, got)
    }

    /// Waits until `f` holds of the shared state (at most 5 s).
    pub fn wait(shared: &Arc<Mutex<Shared>>, f: impl Fn(&Shared) -> bool) -> bool {
        for _ in 0..500 {
            if f(&shared.lock().unwrap()) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        false
    }

    const V: Vars = Vars { f: 12500.0, a: 2.0, ch: 0, hiz: true };

    #[test]
    fn the_table_keeps_its_indexes() {
        // the setup file keeps the index: the older models keep theirs, new ones follow
        let names: Vec<&str> = GENERATORS.iter().map(|g| g.name).collect();
        assert_eq!(names, ["Built-in", "APPL", "BSWV", "SCPI", "Free-running sweep", "CHANnel", "OWON DGE"]);
        assert_eq!((GENERATORS[BUILTIN].name, GENERATORS[FREE].name), ("Built-in", "Free-running sweep"));
        assert_eq!(externals().collect::<Vec<_>>(), [1, 2, 3, 5, 6]);
        assert!(external(FIRST_EXTERNAL) && !external(BUILTIN) && !external(FREE) && !external(GENERATORS.len()));
    }

    #[test]
    fn older_models_send_what_they_sent() {
        assert_eq!(lines(GENERATORS[1].step, &Vars { f: 1e3, a: 1.0, ..V }), [":SOUR1:APPL:SIN 1000,1,0"]);
        assert_eq!(lines(GENERATORS[1].on, &V), [":OUTP1 ON"]);
        assert!(lines(GENERATORS[1].setup, &V).is_empty());
        assert_eq!(lines(GENERATORS[2].step, &Vars { f: 1e3, a: 1.0, ..V }), ["C1:BSWV WVTP,SINE,FRQ,1000,AMP,1,OFST,0"]);
        assert_eq!(lines(GENERATORS[3].step, &Vars { f: 1e3, a: 1.0, ..V }), ["FREQ 1000;VOLT 1"]);
        let g = &GENERATORS[5];
        assert_eq!(lines(g.setup, &Vars { f: 100.0, ..V }), [":CHANnel1:MODe CONTINUE", ":CHANnel1:BASE:WAVe SINe", ":CHANnel1:AMPLitude:UNIT VPP", ":CHANnel1:BASE:AMPLitude 2", ":CHANnel1:BASE:OFFSet 0"]);
        assert_eq!(lines(g.step, &V), [":CHANnel1:BASE:FREQuency 12500"]);
        assert_eq!(lines(g.on, &V), [":CHANnel1:OUTPut ON"]);
        // the second channel
        assert_eq!(lines(GENERATORS[1].on, &Vars { ch: 1, ..V }), [":OUTP2 ON"]);
        assert_eq!(lines(GENERATORS[2].off, &Vars { ch: 1, ..V }), ["C2:OUTP OFF"]);
        assert!(GENERATORS[1].has("{c}") && !GENERATORS[3].has("{c}") && !GENERATORS[1].has("{z}"));
    }

    #[test]
    fn owon_dge_commands() {
        let g = &GENERATORS[for_idn("OWON,DGE2070,12345678,SCPI:99.0 FV:V1.0.4.7.0").unwrap()];
        assert_eq!(g.name, "OWON DGE");
        assert_eq!(
            lines(g.setup, &V),
            ["OUTPut1:IMPedance INFinity", "SOURce1:FUNCtion:SHAPe SIN", "SOURce1:VOLTage:LEVel:IMMediate:OFFSet 0mV", "SOURce1:VOLTage:LEVel:IMMediate:AMPLitude 2Vpp"]
        );
        assert_eq!(lines(g.setup, &Vars { ch: 1, hiz: false, a: 0.05, ..V })[0], "OUTPut2:IMPedance 50");
        assert_eq!(lines(g.setup, &Vars { a: 0.05, ..V })[3], "SOURce1:VOLTage:LEVel:IMMediate:AMPLitude 0.05Vpp");
        assert_eq!(lines(g.step, &V), ["SOURce1:FREQuency:FIXed 12500Hz"]);
        assert_eq!(lines(g.step, &Vars { f: 70e6, ..V }), ["SOURce1:FREQuency:FIXed 70000000Hz"]);
        assert_eq!((lines(g.on, &V), lines(g.off, &V)), (vec!["OUTPut1:STATe ON".to_string()], vec!["OUTPut1:STATe OFF".to_string()]));
        assert_eq!(g.err, "SYSTem:ERRor?");
        assert!(g.has("{c}") && g.has("{z}"));
        // no model for what is not known
        assert_eq!(for_idn("ACME,X1,1,1"), None);
        assert_eq!(for_idn(" owon,dge3062,1,1 "), Some(6));
    }

    #[test]
    fn answers_of_the_error_queue() {
        assert!(no_error("0,\"No error\"") && no_error("+0,\"No error\"") && no_error("0") && no_error(" 0,No error\r"));
        assert!(!no_error("-201,\"Invalid while in local\"") && !no_error("-102,\"Syntax error\"") && !no_error("") && !no_error("01"));
    }

    #[test]
    fn idn_without_serial_and_firmware() {
        assert_eq!(idn_short("OWON,DGE2070,12345678,SCPI:99.0 FV:V1.0.4.7.0"), "OWON DGE2070");
        assert_eq!(idn_short("ACME"), "ACME");
        assert_eq!(idn_short(""), "");
    }

    #[test]
    fn a_refused_command_is_reported_and_the_link_stays() {
        let (addr, got) = fake_generator("OWON,DGE2070,1,x", "80000000Hz");
        let shared = Arc::new(Mutex::new(Shared::default()));
        let tx = spawn(Target { usb: false, addr, identify: true }, shared.clone());
        assert_eq!(shared.lock().unwrap().conn, Conn::Connecting);
        let g = &GENERATORS[6];
        let batch = |l: Vec<String>| Batch { lines: l, check: g.err, pace_ms: 0 };
        tx.send(batch(lines(g.step, &Vars { f: 1e3, ..V }))).unwrap();
        assert!(wait(&shared, |s| s.done == 1));
        {
            let s = shared.lock().unwrap();
            assert!(matches!(&s.conn, Conn::On(w) if w.starts_with("generator 127.0.0.1:")), "{:?}", s.conn);
            assert_eq!(s.idn.as_deref(), Some("OWON,DGE2070,1,x"));
            assert!(s.note.as_deref().is_some_and(|n| n.ends_with(": OWON DGE2070")), "{:?}", s.note);
            assert_eq!(s.err, None);
        }
        // above the range: the generator keeps the old frequency and says so
        tx.send(batch(lines(g.step, &Vars { f: 80e6, ..V }))).unwrap();
        assert!(wait(&shared, |s| s.done == 2));
        let e = shared.lock().unwrap().err.take().unwrap();
        assert_eq!(e, "The generator refused SOURce1:FREQuency:FIXed 80000000Hz: -222,\"Data out of range\"");
        // the next command goes through, with a clean queue
        tx.send(batch(lines(g.on, &V))).unwrap();
        assert!(wait(&shared, |s| s.done == 3));
        assert_eq!(shared.lock().unwrap().err, None);
        assert!(matches!(shared.lock().unwrap().conn, Conn::On(_)));
        // the queue was emptied before the first command, each command was followed by the query
        let l = got.lock().unwrap().clone();
        assert_eq!(l[..4], ["*IDN?", "SYSTem:ERRor?", "SOURce1:FREQuency:FIXed 1000Hz", "SYSTem:ERRor?"]);
        assert!(!l.iter().any(|x| x.contains("ESR")), "{:?}", l);
        drop(tx);
        assert!(wait(&shared, |s| s.conn == Conn::Off));
    }

    #[test]
    fn a_model_without_error_query_is_not_asked() {
        let (addr, got) = fake_generator("x", "");
        let shared = Arc::new(Mutex::new(Shared::default()));
        let tx = spawn(Target { usb: false, addr, identify: false }, shared.clone());
        tx.send(Batch { lines: lines(GENERATORS[1].step, &Vars { f: 1e3, a: 1.0, ..V }), check: "", pace_ms: 0 }).unwrap();
        assert!(wait(&shared, |s| s.done == 1));
        drop(tx);
        assert!(wait(&shared, |s| s.conn == Conn::Off));
        // all of it, once the generator has seen the end of the connection: the one command, no query
        assert!(wait(&shared, |_| got.lock().unwrap().last().is_some_and(|l| l == CLOSED)));
        assert_eq!(*got.lock().unwrap(), [":SOUR1:APPL:SIN 1000,1,0", CLOSED]);
        assert_eq!(shared.lock().unwrap().idn, None);
    }

    #[test]
    fn nothing_there() {
        // no USB device in the tests, and a port nobody listens on
        let shared = Arc::new(Mutex::new(Shared::default()));
        let _tx = spawn(Target { usb: true, addr: String::new(), identify: true }, shared.clone());
        assert!(wait(&shared, |s| matches!(&s.conn, Conn::Failed(e) if e.starts_with("generator USB: no USB device found"))), "{:?}", shared.lock().unwrap().conn);
        assert!(shared.lock().unwrap().err.is_some());
        let shared = Arc::new(Mutex::new(Shared::default()));
        let _tx = spawn(Target { usb: false, addr: "not an address".into(), identify: true }, shared.clone());
        assert!(wait(&shared, |s| matches!(&s.conn, Conn::Failed(e) if e.starts_with("generator not an address"))));
    }
}
