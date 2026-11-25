// Copyright (C) 2026 Julian Decker
// SPDX-License-Identifier: GPL-2.0-or-later

//! VNC for the web remote (prototype): the screen as an RFB server over a WebSocket on the web port
//! (`GET /websockify`), for the noVNC page at `/vnc` (`http.rs`).
//!
//! What a viewer costs the scope:
//!   - one thread (`web-vnc`), on what the scope leaves over (`http::lower_priority`), as the PNG encoder of the live
//!     picture; the frame loop only hands a screen over when a viewer has asked for one, at most 20 times a second;
//!   - three screens of memory for the first viewer (the newest, the one the viewer has, a spare: 2.4 MB each at
//!     1024x600) and one more for the second; all of it is given back when the last viewer leaves;
//!   - only the tiles that changed are sent, as PNG (`store::png`, the encoder of the live picture) or as one colour.
//!
//! Protocol: RFB 3.3, 3.7 and 3.8, security type None (as the web remote: bench LAN only). Encodings: Tight with PNG
//! (-260: fill and PNG rectangles) for a viewer that takes it and has 8 bits a colour, else Raw in the viewer's pixel
//! format; DesktopSize (-223) when the screen changes its size. The left mouse button is a finger on the screen
//! (`down`, `move`, `up`, `cmd.rs`), its wheel the wheel of a mouse on the scope (`wheel`), and the keys are those of
//! a keyboard on the scope (`keysym`, `ui/pointer.rs`). The right button is the
//! touch-hold context menu (`context`).
//!
//! Desktop viewers connect to a port of its own (`PORT`; Remote services > VNC, off by default): the same protocol
//! without the WebSocket around it.

use crate::gfx::Canvas;
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU16, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Viewers at once (more are answered 503).
pub const MAX_VIEWERS: usize = 2;
/// The fastest a screen is handed over: 20 a second.
const MIN_GAP: Duration = Duration::from_millis(50);
/// The time between two screens is at least this many times the last comparison and encoding.
const DUTY: u32 = 2;
/// While the screen stands still, it is looked at again only this often.
const STILL: Duration = Duration::from_millis(200);
/// Side of the squares the screen is compared in.
const TILE: usize = 32;
/// Most bytes of one WebSocket frame or one RFB message from a viewer (pasted text is the only long one).
const MAX_IN: usize = 1 << 20;
/// How long a viewer's thread sleeps at most before it looks at its connection again (the delay of a mouse event).
const WAIT: Duration = Duration::from_millis(10);
/// A ping this often; a viewer that sent nothing for `SILENT` is gone.
const PING: Duration = Duration::from_secs(20);
const SILENT: Duration = Duration::from_secs(60);
const SEND_TIMEOUT: Duration = Duration::from_secs(30);
/// Commands waiting for the frame loop at most (`queue`).
const MAX_CMDS: usize = 64;
/// The port for desktop viewers: the one registered for the protocol; `--vnc-port` gives another on a computer.
pub const PORT: u16 = 5900;
static CONFIGURED: AtomicU16 = AtomicU16::new(PORT);

/// The port Remote services > VNC opens (`Vnc::serve`).
pub fn configured_port() -> u16 {
    CONFIGURED.load(Ordering::Relaxed)
}

pub fn set_configured_port(port: u16) {
    CONFIGURED.store(port, Ordering::Relaxed);
}

/// Keys a viewer holds at once at most; a key beyond is not taken.
const MAX_KEYS: usize = 16;
const WS_GUID: &[u8] = b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

/// One screen handed over by the frame loop.
pub struct Frame {
    w: usize,
    h: usize,
    px: Vec<u32>,
    seq: u64,
}

#[derive(Default)]
struct State {
    viewers: usize,
    /// a viewer has asked for an update and has seen the newest screen
    need: bool,
    frame: Option<Arc<Frame>>,
    seq: u64,
    /// when the last screen was handed over, and the time to wait after it
    at: Option<Instant>,
    gap: Duration,
    /// pixel buffer for the next screen
    spare: Vec<u32>,
    /// the frame loop hands its screens over (`hand_over`, `read_back`) instead of `tick` copying them
    frames: bool,
    /// touch commands for the frame loop
    cmds: Vec<String>,
    /// raised to end the viewers of before: [those of the port, those of the web page] (`Session::ws`)
    epoch: [u64; 2],
    /// the port for desktop viewers while Remote services > VNC is on, and when opening it was last tried
    listener: Option<TcpListener>,
    tried: Option<Instant>,
}

struct Shared {
    st: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// The frame loop's side: hands screens over and takes the viewers' touch commands.
pub struct Vnc {
    sh: Arc<Shared>,
}

impl Vnc {
    pub fn new() -> Vnc {
        Vnc { sh: Arc::new(Shared { st: Default::default(), wake: Default::default() }) }
    }

    fn wanted(st: &State, now: Instant) -> bool {
        st.viewers > 0 && st.need && st.at.map_or(true, |t| now.saturating_duration_since(t) >= st.gap)
    }

    fn give(st: &mut State, w: usize, h: usize, px: Vec<u32>, now: Instant) {
        st.seq += 1;
        let old = st.frame.replace(Arc::new(Frame { w, h, px, seq: st.seq }));
        // the screen no viewer holds any more is the buffer of the next one
        if let Some(Ok(f)) = old.map(Arc::try_unwrap) {
            if st.spare.capacity() == 0 {
                st.spare = f.px;
            }
        }
        st.at = Some(now);
        st.need = false;
    }

    fn copy(&self, st: &mut State, cv: &Canvas, now: Instant) {
        let mut px = std::mem::take(&mut st.spare);
        px.clear();
        px.extend_from_slice(&cv.px);
        Self::give(st, cv.w as usize, cv.h as usize, px, now);
        self.sh.wake.notify_all();
    }

    /// Every frame: a copy of the screen when a viewer waits for one and the frame loop hands none over, and the
    /// desktop viewers that connected to the port.
    pub fn tick(&self, cv: &Canvas) {
        let mut st = self.sh.lock();
        let now = Instant::now();
        if !st.frames && Self::wanted(&st, now) {
            self.copy(&mut st, cv, now);
        }
        let mut new = Vec::new();
        if let Some(l) = &st.listener {
            while let Ok((s, _)) = l.accept() {
                new.push(s);
            }
        }
        drop(st);
        for s in new {
            // the server speaks first: nothing is read here. A viewer too many is dropped, which closes it
            let _ = s.set_nonblocking(false);
            self.start(s, None, &[], (cv.w as usize, cv.h as usize));
        }
    }

    /// Right before the next render, as `http::Shots::hand_over`: the shown screen goes to the viewers by swapping
    /// buffers. `taken`: the live picture took this screen already (the next one is ours).
    pub fn hand_over(&self, cv: &mut Canvas, whole: bool, taken: bool) {
        let mut st = self.sh.lock();
        st.frames = whole;
        let now = Instant::now();
        if !whole || taken || !Self::wanted(&st, now) {
            return;
        }
        let mut px = std::mem::take(&mut st.spare);
        px.resize(cv.px.len(), 0);
        std::mem::swap(&mut px, &mut cv.px);
        Self::give(&mut st, cv.w as usize, cv.h as usize, px, now);
        self.sh.wake.notify_all();
    }

    /// System > Graphics = GPU: the frame loop hands over what it reads back (`read_back`).
    pub fn gpu_frames(&self) {
        self.sh.lock().frames = true;
    }

    /// System > Graphics = GPU: `cv` was just read back whole.
    pub fn read_back(&self, cv: &Canvas) {
        let mut st = self.sh.lock();
        let now = Instant::now();
        if Self::wanted(&st, now) {
            self.copy(&mut st, cv, now);
        }
    }

    /// A viewer would take the next screen.
    pub fn picture_wanted(&self) -> bool {
        Self::wanted(&self.sh.lock(), Instant::now())
    }

    /// The viewers' touch commands since the last call (`cmd::run` syntax).
    pub fn take_cmds(&self) -> Vec<String> {
        std::mem::take(&mut self.sh.lock().cmds)
    }

    #[cfg(test)]
    pub fn viewers(&self) -> usize {
        self.sh.lock().viewers
    }

    /// The web remote was switched off: the viewers of its page go.
    pub fn close(&self) {
        self.sh.lock().epoch[1] += 1;
        self.sh.wake.notify_all();
    }

    /// A connection that asked for the WebSocket (`key`: its Sec-WebSocket-Key; `binary`: it offered that
    /// sub-protocol; `early`: what it sent after the request head). The answer and all the rest come from a thread of
    /// its own. False when there are `MAX_VIEWERS` already or no thread could be started (nothing was answered).
    pub fn adopt(&self, s: TcpStream, key: &str, binary: bool, early: &[u8], size: (usize, usize)) -> bool {
        let head = format!(
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {}\r\n{}\r\n",
            ws_accept(key),
            if binary { "Sec-WebSocket-Protocol: binary\r\n" } else { "" }
        );
        self.start(s, Some(head), early, size)
    }

    /// A viewer on `s`, on a thread of its own. `head`: the answer to its WebSocket request, and the protocol is in
    /// WebSocket frames from there on; none for a desktop viewer, whose connection carries the protocol as it is.
    /// False when there are `MAX_VIEWERS` already or no thread could be started (nothing was answered).
    fn start(&self, s: TcpStream, head: Option<String>, early: &[u8], size: (usize, usize)) -> bool {
        let mut st = self.sh.lock();
        if st.viewers >= MAX_VIEWERS {
            return false;
        }
        st.viewers += 1;
        let now = Instant::now();
        let ws = head.is_some();
        let sess = Session {
            sh: self.sh.clone(),
            s,
            epoch: st.epoch[ws as usize],
            ws,
            ws_in: early.to_vec(),
            rfb: Rfb::new(size),
            shadow: None,
            seen: 0,
            scratch: Vec::new(),
            last_rx: now,
            last_ping: now,
            updates: 0,
            bytes: 0,
            work: Duration::ZERO,
            started: false,
        };
        eprintln!("oos-ui: VNC viewer {} of {}", st.viewers, MAX_VIEWERS);
        drop(st);
        // a thread that could not be started drops the session, which takes the viewer off the count again
        std::thread::Builder::new().name("web-vnc".into()).spawn(move || sess.run(head.unwrap_or_default())).is_ok()
    }

    /// Remote services > VNC: the port desktop viewers connect to, open on `port` or closed, which ends its viewers.
    /// A port that is taken is tried again a second later, as the other listeners are (#212).
    pub fn serve(&self, port: Option<u16>) {
        let mut st = self.sh.lock();
        match port {
            None => {
                st.tried = None;
                if st.listener.take().is_some() {
                    st.epoch[0] += 1;
                    drop(st);
                    self.sh.wake.notify_all();
                    eprintln!("oos-ui: VNC port off");
                }
            }
            Some(p) if st.listener.is_none() && st.tried.is_none_or(|t| t.elapsed() >= Duration::from_secs(1)) => {
                st.tried = Some(Instant::now());
                st.listener = crate::net6::tcp_service(p, "VNC");
            }
            Some(_) => {}
        }
    }

    /// The port desktop viewers connect to, while it is open.
    #[cfg(test)]
    pub fn port(&self) -> Option<u16> {
        self.sh.lock().listener.as_ref().and_then(|l| l.local_addr().ok()).map(|a| a.port())
    }
}

impl Drop for Vnc {
    fn drop(&mut self) {
        let mut st = self.sh.lock();
        st.listener = None;
        st.epoch = [st.epoch[0] + 1, st.epoch[1] + 1];
        drop(st);
        self.sh.wake.notify_all();
    }
}

/// Sec-WebSocket-Accept for a Sec-WebSocket-Key (RFC 6455, 4.2.2).
pub fn ws_accept(key: &str) -> String {
    crate::base64::encode(&oos_hal::update::hash::sha1(&[key.trim().as_bytes(), WS_GUID]))
}

/// The head of a WebSocket frame from the server (final, not masked) for `len` bytes of payload.
fn ws_head(op: u8, len: usize) -> Vec<u8> {
    let mut h = vec![0x80 | op];
    match len {
        0..=125 => h.push(len as u8),
        126..=65535 => {
            h.push(126);
            h.extend_from_slice(&(len as u16).to_be_bytes());
        }
        _ => {
            h.push(127);
            h.extend_from_slice(&(len as u64).to_be_bytes());
        }
    }
    h
}

/// The next whole frame of a viewer out of `buf`: its opcode and its payload, unmasked. `None`: not all there yet.
/// `Err`: not a frame a browser sends (no mask, reserved bits, longer than `MAX_IN`).
fn ws_take(buf: &mut Vec<u8>) -> Result<Option<(u8, Vec<u8>)>, ()> {
    if buf.len() < 2 {
        return Ok(None);
    }
    if buf[0] & 0x70 != 0 || buf[1] & 0x80 == 0 {
        return Err(());
    }
    let op = buf[0] & 0x0f;
    let (len, at) = match buf[1] & 0x7f {
        126 => {
            if buf.len() < 4 {
                return Ok(None);
            }
            (u16::from_be_bytes([buf[2], buf[3]]) as u64, 4)
        }
        127 => {
            if buf.len() < 10 {
                return Ok(None);
            }
            (u64::from_be_bytes([buf[2], buf[3], buf[4], buf[5], buf[6], buf[7], buf[8], buf[9]]), 10)
        }
        n => (n as u64, 2),
    };
    if len > MAX_IN as u64 {
        return Err(());
    }
    let len = len as usize;
    if buf.len() < at + 4 + len {
        return Ok(None);
    }
    let key = [buf[at], buf[at + 1], buf[at + 2], buf[at + 3]];
    let data = buf[at + 4..at + 4 + len].iter().enumerate().map(|(i, b)| b ^ key[i & 3]).collect();
    buf.drain(..at + 4 + len);
    Ok(Some((op, data)))
}

/// A viewer's pixel format (SetPixelFormat), true colour only.
#[derive(Clone, Copy, PartialEq, Debug)]
struct PixFmt {
    bytes: usize,
    depth: u8,
    big: bool,
    max: [u32; 3],
    shift: [u32; 3],
}

impl PixFmt {
    /// The screen as it is: XRGB8888.
    const SERVER: PixFmt = PixFmt { bytes: 4, depth: 24, big: false, max: [255; 3], shift: [16, 8, 0] };

    /// The 16 bytes of the protocol's PIXEL_FORMAT.
    fn wire(&self) -> [u8; 16] {
        let m = |i: usize| (self.max[i] as u16).to_be_bytes();
        [
            (self.bytes * 8) as u8,
            self.depth,
            self.big as u8,
            1,
            m(0)[0],
            m(0)[1],
            m(1)[0],
            m(1)[1],
            m(2)[0],
            m(2)[1],
            self.shift[0] as u8,
            self.shift[1] as u8,
            self.shift[2] as u8,
            0,
            0,
            0,
        ]
    }

    fn parse(b: &[u8]) -> Option<PixFmt> {
        let bpp = b[0] as u32;
        if b[3] == 0 || !matches!(bpp, 8 | 16 | 32) {
            return None; // a colour map: no viewer of today asks for one
        }
        let max = [
            u16::from_be_bytes([b[4], b[5]]) as u32,
            u16::from_be_bytes([b[6], b[7]]) as u32,
            u16::from_be_bytes([b[8], b[9]]) as u32,
        ];
        let shift = [b[10] as u32, b[11] as u32, b[12] as u32];
        if shift.iter().any(|&s| s >= bpp) {
            return None;
        }
        Some(PixFmt { bytes: bpp as usize / 8, depth: b[1], big: b[2] != 0, max, shift })
    }

    /// Tight sends such a pixel as three bytes, red, green, blue.
    fn tpixel(&self) -> bool {
        self.bytes == 4 && self.depth == 24 && self.max == [255; 3] && self.shift.iter().all(|s| s % 8 == 0)
    }

    /// One pixel of the screen (XRGB8888) in this format.
    fn put(&self, px: u32, out: &mut Vec<u8>) {
        let c = [px >> 16 & 255, px >> 8 & 255, px & 255];
        let mut v = 0u32;
        for i in 0..3 {
            let s = if self.max[i] == 255 { c[i] } else { (c[i] * self.max[i] + 127) / 255 };
            v |= s << self.shift[i];
        }
        match (self.bytes, self.big) {
            (1, _) => out.push(v as u8),
            (2, false) => out.extend_from_slice(&(v as u16).to_le_bytes()),
            (2, true) => out.extend_from_slice(&(v as u16).to_be_bytes()),
            (_, false) => out.extend_from_slice(&v.to_le_bytes()),
            (_, true) => out.extend_from_slice(&v.to_be_bytes()),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Version,
    Security,
    Init,
    Run,
}

/// The protocol of one viewer, without its connection: bytes in, answers and touch commands out.
struct Rfb {
    stage: Stage,
    minor: u8,
    inb: Vec<u8>,
    /// the size the viewer knows
    size: (usize, usize),
    pf: PixFmt,
    tight_png: bool,
    desktop_size: bool,
    /// the viewer asked for an update that it has not got yet; `full`: of the whole screen, not only what changed
    requested: bool,
    full: bool,
    buttons: u8,
    pos: (i32, i32),
    /// the keys the viewer holds, by keysym: a second down of one is the key repeating
    keys: Vec<u32>,
}

/// A command for the frame loop. A move replaces a move that has not run yet. A full queue takes only what ends
/// something (a finger or a key up), and four times as full nothing: a viewer cannot fill the memory with commands.
fn queue(q: &mut Vec<String>, c: String) {
    let ends = c.starts_with("up ") || c.starts_with("keysym up ");
    if c.starts_with("move ") && q.last().is_some_and(|l| l.starts_with("move ")) {
        q.pop();
    } else if q.len() >= if ends { 4 * MAX_CMDS } else { MAX_CMDS } {
        return;
    }
    q.push(c);
}

impl Rfb {
    fn new(size: (usize, usize)) -> Rfb {
        Rfb {
            stage: Stage::Version,
            minor: 8,
            inb: Vec::new(),
            size,
            pf: PixFmt::SERVER,
            tight_png: false,
            desktop_size: false,
            requested: false,
            full: true,
            buttons: 0,
            pos: (0, 0),
            keys: Vec::new(),
        }
    }

    /// What the server says first.
    const HELLO: &'static [u8] = b"RFB 003.008\n";

    /// Bytes from the viewer. `out`: what to answer; `cmds`: touch commands. False: not the protocol, close.
    fn feed(&mut self, data: &[u8], out: &mut Vec<u8>, cmds: &mut Vec<String>) -> bool {
        self.inb.extend_from_slice(data);
        loop {
            let used = match self.stage {
                Stage::Version => {
                    if self.inb.len() < 12 {
                        break;
                    }
                    if &self.inb[..4] != b"RFB " || self.inb[11] != b'\n' {
                        return false;
                    }
                    let minor =
                        std::str::from_utf8(&self.inb[8..11]).ok().and_then(|s| s.parse::<u32>().ok()).unwrap_or(3);
                    self.minor = match minor {
                        0..=6 => 3,
                        7 => 7,
                        _ => 8,
                    };
                    if self.minor == 3 {
                        // 3.3: the server names the one security type (None), no answer to it
                        out.extend_from_slice(&1u32.to_be_bytes());
                        self.stage = Stage::Init;
                    } else {
                        out.extend_from_slice(&[1, 1]);
                        self.stage = Stage::Security;
                    }
                    12
                }
                Stage::Security => {
                    if self.inb.is_empty() {
                        break;
                    }
                    if self.inb[0] != 1 {
                        return false;
                    }
                    if self.minor >= 8 {
                        out.extend_from_slice(&0u32.to_be_bytes());
                    }
                    self.stage = Stage::Init;
                    1
                }
                Stage::Init => {
                    if self.inb.is_empty() {
                        break;
                    }
                    // ClientInit (shared or not: the screen is shared anyway), then ServerInit
                    out.extend_from_slice(&(self.size.0 as u16).to_be_bytes());
                    out.extend_from_slice(&(self.size.1 as u16).to_be_bytes());
                    out.extend_from_slice(&PixFmt::SERVER.wire());
                    let name = b"OOS";
                    out.extend_from_slice(&(name.len() as u32).to_be_bytes());
                    out.extend_from_slice(name);
                    self.stage = Stage::Run;
                    1
                }
                Stage::Run => match self.message(cmds) {
                    Ok(0) => break,
                    Ok(n) => n,
                    Err(()) => return false,
                },
            };
            self.inb.drain(..used);
        }
        self.inb.len() <= MAX_IN
    }

    /// One message of the running protocol at the start of `inb`: its length, 0 when it is not all there yet.
    fn message(&mut self, cmds: &mut Vec<String>) -> Result<usize, ()> {
        let n = self.inb.len();
        let Some(&kind) = self.inb.first() else { return Ok(0) };
        let be16 = |b: &[u8], i: usize| u16::from_be_bytes([b[i], b[i + 1]]) as usize;
        match kind {
            // SetPixelFormat
            0 => {
                if n < 20 {
                    return Ok(0);
                }
                self.pf = PixFmt::parse(&self.inb[4..20]).ok_or(())?;
                Ok(20)
            }
            // SetEncodings
            2 => {
                if n < 4 {
                    return Ok(0);
                }
                let len = 4 + 4 * be16(&self.inb, 2);
                if n < len {
                    return Ok(0);
                }
                let has = |e: i32| {
                    self.inb[4..len].chunks_exact(4).any(|c| i32::from_be_bytes([c[0], c[1], c[2], c[3]]) == e)
                };
                (self.tight_png, self.desktop_size) = (has(-260), has(-223));
                Ok(len)
            }
            // FramebufferUpdateRequest
            3 => {
                if n < 10 {
                    return Ok(0);
                }
                self.requested = true;
                self.full |= self.inb[1] == 0;
                Ok(10)
            }
            // KeyEvent
            4 => {
                if n < 8 {
                    return Ok(0);
                }
                let sym = u32::from_be_bytes([self.inb[4], self.inb[5], self.inb[6], self.inb[7]]);
                self.key(self.inb[1] != 0, sym, cmds);
                Ok(8)
            }
            // PointerEvent
            5 => {
                if n < 6 {
                    return Ok(0);
                }
                let (mask, x, y) = (self.inb[1], be16(&self.inb, 2) as i32, be16(&self.inb, 4) as i32);
                self.pointer(mask, x, y, cmds);
                Ok(6)
            }
            // ClientCutText: nothing to paste into
            6 => {
                if n < 8 {
                    return Ok(0);
                }
                let len = u32::from_be_bytes([self.inb[4], self.inb[5], self.inb[6], self.inb[7]]) as usize;
                if len > MAX_IN {
                    return Err(());
                }
                Ok(if n < 8 + len { 0 } else { 8 + len })
            }
            _ => Err(()),
        }
    }

    /// A key, by its keysym: down, down again while it is held (the key repeats), or up.
    fn key(&mut self, down: bool, sym: u32, cmds: &mut Vec<String>) {
        let phase = match (down, self.keys.contains(&sym)) {
            (true, true) => "repeat",
            (true, false) if self.keys.len() < MAX_KEYS => {
                self.keys.push(sym);
                "down"
            }
            (false, true) => {
                self.keys.retain(|k| *k != sym);
                "up"
            }
            _ => return,
        };
        queue(cmds, format!("keysym {} {:x}", phase, sym));
    }

    /// Ctrl, Shift and Alt as held, as the `wheel` command names them (`c`, `s`, `a`; `-` for none).
    fn modifiers(&self) -> String {
        let held = |m: &(u32, u32, char)| self.keys.iter().any(|k| *k == m.0 || *k == m.1);
        let m: String = [(0xffe3, 0xffe4, 'c'), (0xffe1, 0xffe2, 's'), (0xffe9, 0xffea, 'a')]
            .iter()
            .filter(|m| held(m))
            .map(|m| m.2)
            .collect();
        if m.is_empty() {
            "-".into()
        } else {
            m
        }
    }

    /// The left button is a finger on the screen; a step of the wheel is a press of button 4 (away from the hand) or
    /// 5, with the modifier keys held; the right button, while no finger is down, is the touch-hold context menu.
    fn pointer(&mut self, mask: u8, x: i32, y: i32, cmds: &mut Vec<String>) {
        let (x, y) = (x.clamp(0, self.size.0 as i32 - 1), y.clamp(0, self.size.1 as i32 - 1));
        for (bit, steps) in [(8u8, 1), (16, -1)] {
            if mask & bit != 0 && self.buttons & bit == 0 {
                queue(cmds, format!("wheel {} {} {} {}", x, y, steps, self.modifiers()));
            }
        }
        if mask & 4 != 0 && self.buttons & 4 == 0 && (mask | self.buttons) & 1 == 0 {
            queue(cmds, format!("context {} {}", x, y));
        }
        match (self.buttons & 1 != 0, mask & 1 != 0) {
            (false, true) => queue(cmds, format!("down {} {}", x, y)),
            (true, true) if (x, y) != self.pos => queue(cmds, format!("move {} {}", x, y)),
            (true, false) => queue(cmds, format!("up {} {}", x, y)),
            _ => {}
        }
        (self.buttons, self.pos) = (mask, (x, y));
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}

/// What changed from `prev` to `cur`, in rectangles of whole tiles: runs of changed tiles in a row of tiles, joined
/// with the same run right above. All of the screen without a `prev` of the same size.
fn dirty(prev: Option<&Frame>, cur: &Frame) -> Vec<Rect> {
    let (w, h) = (cur.w, cur.h);
    let Some(prev) = prev.filter(|p| p.w == w && p.h == h && p.px.len() == cur.px.len()) else {
        return vec![Rect { x: 0, y: 0, w, h }];
    };
    let mut out: Vec<Rect> = Vec::new();
    let mut y = 0;
    while y < h {
        let th = TILE.min(h - y);
        let mut run: Option<usize> = None;
        // one tile past the end closes a run that reaches the right edge
        for tx in 0..=w.div_ceil(TILE) {
            let x = (tx * TILE).min(w);
            let tw = TILE.min(w - x);
            let changed = tw > 0
                && (y..y + th).any(|r| {
                    !crate::shots::same_pixels(&prev.px[r * w + x..r * w + x + tw], &cur.px[r * w + x..r * w + x + tw])
                });
            match (run, changed) {
                (None, true) => run = Some(x),
                (Some(x0), false) => {
                    match out.iter_mut().rev().find(|o| o.x == x0 && o.w == x - x0 && o.y + o.h == y) {
                        Some(o) => o.h += th,
                        None => out.push(Rect { x: x0, y, w: x - x0, h: th }),
                    }
                    run = None;
                }
                _ => {}
            }
        }
        y += th;
    }
    out
}

/// Tight's length of what follows: 7 bits a byte, the lowest first, three bytes at most.
fn compact_len(o: &mut Vec<u8>, n: usize) {
    let (a, b, c) = ((n & 0x7f) as u8, (n >> 7 & 0x7f) as u8, (n >> 14) as u8);
    if n < 1 << 7 {
        o.push(a);
    } else if n < 1 << 14 {
        o.extend_from_slice(&[a | 0x80, b]);
    } else {
        o.extend_from_slice(&[a | 0x80, b | 0x80, c]);
    }
}

/// A FramebufferUpdate with `rects` of `f`. `tight_png`: as fill and PNG rectangles, else Raw in the format `pf`.
/// `resized`: the new size first (DesktopSize). `scratch`: the pixels of one rectangle, kept between calls.
fn encode_update(
    f: &Frame,
    rects: &[Rect],
    pf: &PixFmt,
    tight_png: bool,
    resized: bool,
    scratch: &mut Vec<u32>,
) -> Vec<u8> {
    let mut o = Vec::with_capacity(64 << 10);
    o.extend_from_slice(&[0, 0]);
    o.extend_from_slice(&((rects.len() + resized as usize) as u16).to_be_bytes());
    let head = |o: &mut Vec<u8>, r: &Rect, enc: i32| {
        for v in [r.x, r.y, r.w, r.h] {
            o.extend_from_slice(&(v as u16).to_be_bytes());
        }
        o.extend_from_slice(&enc.to_be_bytes());
    };
    if resized {
        head(&mut o, &Rect { x: 0, y: 0, w: f.w, h: f.h }, -223);
    }
    for r in rects {
        let rows = || (r.y..r.y + r.h).map(|y| &f.px[y * f.w + r.x..y * f.w + r.x + r.w]);
        if tight_png && pf.tpixel() {
            let first = f.px[r.y * f.w + r.x] & 0xff_ffff;
            if rows().all(|row| row.iter().fold(0, |d, &p| d | (p ^ first)) & 0xff_ffff == 0) {
                head(&mut o, r, -260);
                o.extend_from_slice(&[0x80, (first >> 16) as u8, (first >> 8) as u8, first as u8]);
                continue;
            }
            scratch.clear();
            rows().for_each(|row| scratch.extend_from_slice(row));
            let png = crate::gfx::png::png(r.w as u32, r.h as u32, scratch);
            // Tight's length has 22 bits; a PNG beyond (noise on a large desktop window) goes as Raw
            if png.len() < 1 << 22 {
                head(&mut o, r, -260);
                o.push(0xa0);
                compact_len(&mut o, png.len());
                o.extend_from_slice(&png);
                continue;
            }
        }
        head(&mut o, r, 0);
        o.reserve(r.w * r.h * pf.bytes);
        rows().for_each(|row| row.iter().for_each(|&p| pf.put(p, &mut o)));
    }
    o
}

/// One viewer: its connection and its thread.
struct Session {
    sh: Arc<Shared>,
    s: TcpStream,
    epoch: u64,
    /// the protocol comes in WebSocket frames (the web page); else the connection carries it as it is (the port)
    ws: bool,
    /// from the connection, not yet a whole WebSocket frame
    ws_in: Vec<u8>,
    rfb: Rfb,
    /// the screen the viewer has
    shadow: Option<Arc<Frame>>,
    /// the newest screen looked at
    seen: u64,
    scratch: Vec<u32>,
    last_rx: Instant,
    last_ping: Instant,
    updates: u64,
    bytes: u64,
    work: Duration,
    started: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut st = self.sh.lock();
        st.viewers = st.viewers.saturating_sub(1);
        if self.rfb.buttons & 1 != 0 {
            // the finger does not stay on the screen
            queue(&mut st.cmds, format!("up {} {}", self.rfb.pos.0, self.rfb.pos.1));
        }
        // nor do its keys stay down: the screen keeps Shift, Ctrl and Alt as held
        for k in std::mem::take(&mut self.rfb.keys) {
            queue(&mut st.cmds, format!("keysym up {:x}", k));
        }
        if st.viewers == 0 {
            (st.frame, st.spare, st.need) = (None, Vec::new(), false);
        }
        if self.started {
            let ms = if self.updates > 0 { self.work.as_secs_f64() * 1e3 / self.updates as f64 } else { 0.0 };
            eprintln!(
                "oos-ui: VNC viewer left: {} updates, {} KB, {:.1} ms each to compare and encode",
                self.updates,
                self.bytes / 1024,
                ms
            );
        }
    }
}

impl Session {
    /// Bytes of the protocol to the viewer: one binary WebSocket message, or as they are.
    fn send(&mut self, payload: &[u8]) -> std::io::Result<()> {
        if !self.ws {
            self.bytes += payload.len() as u64;
            return self.s.write_all(payload);
        }
        let mut m = ws_head(2, payload.len());
        self.bytes += (m.len() + payload.len()) as u64;
        if payload.len() <= 4096 {
            m.extend_from_slice(payload);
            self.s.write_all(&m)
        } else {
            self.s.write_all(&m)?;
            self.s.write_all(payload)
        }
    }

    /// The whole frames that have arrived: protocol, answers, touch commands. False: the viewer is gone or wrong.
    fn pump(&mut self) -> bool {
        if !self.ws {
            let data = std::mem::take(&mut self.ws_in);
            return data.is_empty() || self.feed(&data);
        }
        loop {
            let (op, data) = match ws_take(&mut self.ws_in) {
                Ok(Some(f)) => f,
                Ok(None) => return self.ws_in.len() <= MAX_IN + 14,
                Err(()) => return false,
            };
            match op {
                // RFB is a stream of bytes: the bounds of the frames and their kind mean nothing
                0..=2 => {
                    if !self.feed(&data) {
                        return false;
                    }
                }
                8 => {
                    let _ = self.s.write_all(&ws_head(8, 0));
                    return false;
                }
                9 => {
                    let mut m = ws_head(10, data.len());
                    m.extend_from_slice(&data);
                    if self.s.write_all(&m).is_err() {
                        return false;
                    }
                }
                _ => {}
            }
        }
    }

    /// Bytes of the protocol from the viewer: the answers go back, the commands to the frame loop. False: not the
    /// protocol, or the viewer is gone.
    fn feed(&mut self, data: &[u8]) -> bool {
        let (mut out, mut cmds) = (Vec::new(), Vec::new());
        if !self.rfb.feed(data, &mut out, &mut cmds) || (!out.is_empty() && self.send(&out).is_err()) {
            return false;
        }
        if !cmds.is_empty() {
            let mut st = self.sh.lock();
            cmds.into_iter().for_each(|c| queue(&mut st.cmds, c));
        }
        true
    }

    /// A screen newer than the viewer's: what changed goes out. `Err`: the viewer is gone.
    fn update(&mut self, f: Arc<Frame>) -> Result<(), ()> {
        let t0 = Instant::now();
        self.seen = f.seq;
        let resized = (f.w, f.h) != self.rfb.size;
        if resized && !self.rfb.desktop_size {
            return Err(()); // it cannot follow the new size: it connects again
        }
        let prev = if self.rfb.full || resized { None } else { self.shadow.as_deref() };
        let rects = dirty(prev, &f);
        if rects.is_empty() {
            // the same screen: the request stays open, the next look comes later
            self.sh.lock().gap = STILL.max(t0.elapsed() * DUTY);
            return Ok(());
        }
        let msg = encode_update(&f, &rects, &self.rfb.pf, self.rfb.tight_png, resized, &mut self.scratch);
        let took = t0.elapsed();
        self.send(&msg).map_err(|_| ())?;
        self.updates += 1;
        self.work += took;
        (self.rfb.requested, self.rfb.full, self.rfb.size) = (false, false, (f.w, f.h));
        let old = self.shadow.replace(f);
        let mut st = self.sh.lock();
        st.gap = MIN_GAP.max(took * DUTY);
        if let Some(Ok(o)) = old.map(Arc::try_unwrap) {
            if st.spare.capacity() == 0 {
                st.spare = o.px;
            }
        }
        Ok(())
    }

    fn run(mut self, head: String) {
        crate::system::lower_priority();
        let _ = self.s.set_write_timeout(Some(SEND_TIMEOUT));
        let _ = self.s.set_nodelay(true);
        if self.s.write_all(head.as_bytes()).is_err() || self.send(Rfb::HELLO).is_err() {
            return;
        }
        self.started = true;
        let mut buf = vec![0u8; 16 << 10];
        loop {
            loop {
                match crate::net::read_now(&self.s, &mut buf) {
                    Ok(0) => return,
                    Ok(n) => {
                        self.ws_in.extend_from_slice(&buf[..n]);
                        self.last_rx = Instant::now();
                    }
                    Err(e) if crate::net::later(&e) => break,
                    Err(_) => return,
                }
            }
            if !self.pump() || self.last_rx.elapsed() > SILENT {
                return;
            }
            if self.last_ping.elapsed() > PING {
                self.last_ping = Instant::now();
                // is the viewer still there? The web page answers a ping. The protocol itself has none: a desktop
                // viewer whose request is open gets an update of nothing, which it answers with its next request
                let sent = if self.ws {
                    self.s.write_all(&ws_head(9, 0))
                } else if std::mem::take(&mut self.rfb.requested) {
                    self.send(&[0, 0, 0, 0])
                } else {
                    Ok(())
                };
                if sent.is_err() {
                    return;
                }
            }
            let mut st = self.sh.lock();
            if st.epoch[self.ws as usize] != self.epoch {
                return;
            }
            let newer = |st: &State, seen: u64| st.frame.clone().filter(|f| f.seq > seen);
            let mut next = if self.rfb.requested { newer(&st, self.seen) } else { None };
            if next.is_none() {
                st.need |= self.rfb.requested;
                st = self.sh.wake.wait_timeout(st, WAIT).unwrap_or_else(|e| e.into_inner()).0;
                if self.rfb.requested {
                    next = newer(&st, self.seen);
                }
            }
            drop(st);
            if next.is_some_and(|f| self.update(f).is_err()) {
                return;
            }
        }
    }
}

/// noVNC 1.7.0 (MPL-2.0), `core/rfb.js` and what it imports as one minified module, gzip: `ui/web/novnc/README.md`.
pub const NOVNC_GZ: &[u8] = include_bytes!("../web/novnc/novnc.min.js.gz");

/// The screen through noVNC (`/novnc.js`) and the VNC server on `/websockify` (`vnc.rs`): only what changed comes over,
/// and the mouse is a finger on the screen, held and moved as on the scope.
pub const PAGE: &str = r#"<!DOCTYPE html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>OOS remote (VNC)</title>
<link rel="stylesheet" href="/style.css">
<style>#v{width:100%;aspect-ratio:1024/600;background:#000;border:1px solid #3a424d;border-radius:6px;overflow:hidden;touch-action:none}</style></head>
<body><header><b>OOS remote</b><span id="st">connecting ...</span><nav><a href="/">Classic</a><a href="/update">Update</a></nav></header>
<div id="v"></div>
<script type="module">
import RFB from '/novnc.js';
const st=document.getElementById('st'),v=document.getElementById('v');let tries=0;
function say(m,bad){st.textContent=m;st.className=bad?'bad':'';}
function go(){
const rfb=new RFB(v,(location.protocol=='https:'?'wss://':'ws://')+location.host+'/websockify');
rfb.scaleViewport=true;rfb.showDotCursor=true;
rfb.addEventListener('connect',()=>{tries=0;say('connected');rfb.focus();});
rfb.addEventListener('disconnect',e=>{say(e.detail.clean?'disconnected':'no connection',true);setTimeout(go,Math.min(5000,500<<tries++));});
}
go();
</script></body></html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn frame(w: usize, h: usize, fill: u32) -> Frame {
        Frame { w, h, px: vec![fill; w * h], seq: 1 }
    }

    /// A frame as a browser sends it: masked.
    fn masked(op: u8, payload: &[u8]) -> Vec<u8> {
        let mut m = ws_head(op, payload.len());
        m[1] |= 0x80;
        let key = [0x12, 0x34, 0x56, 0x78];
        m.extend_from_slice(&key);
        m.extend(payload.iter().enumerate().map(|(i, b)| b ^ key[i & 3]));
        m
    }

    #[test]
    fn websocket_accept_is_the_rfc_example() {
        assert_eq!(ws_accept("dGhlIHNhbXBsZSBub25jZQ=="), "s3pPLMBiTxaQ9kYGzzhZRbK+xOo=");
    }

    #[test]
    fn websocket_frames_come_out_whole_and_unmasked() {
        let long = vec![7u8; 300];
        let mut buf = masked(2, b"RFB");
        buf.extend(masked(2, &long));
        let all = buf.clone();
        // every cut of the first frame: nothing yet, nothing taken
        for cut in 0..masked(2, b"RFB").len() {
            let mut part = all[..cut].to_vec();
            assert_eq!(ws_take(&mut part), Ok(None), "cut at {}", cut);
            assert_eq!(part.len(), cut);
        }
        assert_eq!(ws_take(&mut buf), Ok(Some((2, b"RFB".to_vec()))));
        assert_eq!(ws_take(&mut buf), Ok(Some((2, long))));
        assert_eq!(ws_take(&mut buf), Ok(None));
        assert!(buf.is_empty());
        // not masked, as only a server sends; reserved bits; too long
        assert_eq!(ws_take(&mut vec![0x82, 3, 1, 2, 3]), Err(()));
        assert_eq!(ws_take(&mut vec![0xc2, 0x80, 0, 0, 0, 0]), Err(()));
        let mut huge = vec![0x82, 0xff];
        huge.extend_from_slice(&(MAX_IN as u64 + 1).to_be_bytes());
        assert_eq!(ws_take(&mut huge), Err(()));
        assert_eq!(ws_head(2, 70000)[..2], [0x82, 127]);
    }

    /// The messages noVNC sends after the handshake.
    fn novnc_start() -> Vec<u8> {
        let mut m = vec![0, 0, 0, 0, 32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 0, 8, 16, 0, 0, 0];
        m.extend_from_slice(&[2, 0, 0, 3]);
        for e in [-260i32, 0, -223] {
            m.extend_from_slice(&e.to_be_bytes());
        }
        m.extend_from_slice(&[3, 0, 0, 0, 0, 0, 4, 0, 2, 88]);
        m
    }

    #[test]
    fn handshake_and_first_request() {
        let mut r = Rfb::new((1024, 600));
        let (mut out, mut cmds) = (Vec::new(), Vec::new());
        assert!(r.feed(b"RFB 003.0", &mut out, &mut cmds) && out.is_empty());
        assert!(r.feed(b"08\n", &mut out, &mut cmds));
        assert_eq!(std::mem::take(&mut out), [1, 1], "one security type: None");
        assert!(r.feed(&[1], &mut out, &mut cmds));
        assert_eq!(std::mem::take(&mut out), [0, 0, 0, 0], "SecurityResult: ok");
        assert!(r.feed(&[1], &mut out, &mut cmds));
        let init = std::mem::take(&mut out);
        assert_eq!(init[..4], [4, 0, 2, 88], "1024 x 600");
        assert_eq!(init[4..20], [32, 24, 0, 1, 0, 255, 0, 255, 0, 255, 16, 8, 0, 0, 0, 0]);
        assert_eq!(&init[20..], [0, 0, 0, 3, b'O', b'O', b'S']);
        assert!(!r.requested);
        // in two parts, cut inside a message
        let m = novnc_start();
        assert!(r.feed(&m[..27], &mut out, &mut cmds) && r.feed(&m[27..], &mut out, &mut cmds));
        assert!(out.is_empty() && cmds.is_empty());
        assert!(r.requested && r.full && r.tight_png && r.desktop_size);
        assert_eq!(r.pf, PixFmt { bytes: 4, depth: 24, big: false, max: [255; 3], shift: [0, 8, 16] });
        assert!(r.pf.tpixel());
        // keys and pasted text are read over
        assert!(r.feed(&[4, 1, 0, 0, 0, 0, 0, 0x41, 6, 0, 0, 0, 0, 0, 0, 2, b'h', b'i'], &mut out, &mut cmds));
        assert!(r.inb.is_empty());
        // a message this server never agreed to
        assert!(!r.feed(&[150, 0], &mut out, &mut cmds));
        // an older viewer: 3.3 gets the security type as a word and no result
        let mut r = Rfb::new((1024, 600));
        assert!(r.feed(b"RFB 003.003\n", &mut out, &mut cmds));
        assert_eq!(std::mem::take(&mut out), [0, 0, 0, 1]);
        assert!(r.feed(&[0], &mut out, &mut cmds) && out.len() == 27);
        assert!(!Rfb::new((1, 1)).feed(b"GET / HTTP/1", &mut out, &mut cmds));
    }

    #[test]
    fn the_left_button_is_a_finger() {
        let mut r = Rfb::new((1024, 600));
        let (mut out, mut cmds) = (Vec::new(), Vec::new());
        assert!(r.feed(b"RFB 003.008\n\x01\x01", &mut out, &mut cmds));
        let ptr = |mask: u8, x: u16, y: u16| [5, mask, (x >> 8) as u8, x as u8, (y >> 8) as u8, y as u8];
        let mut m = Vec::new();
        // moves without the button, down, three moves, up beyond the screen, a wheel step
        for p in [
            ptr(0, 10, 10),
            ptr(1, 100, 200),
            ptr(1, 110, 200),
            ptr(1, 120, 200),
            ptr(1, 130, 210),
            ptr(0, 2000, 900),
            ptr(8, 5, 5),
        ] {
            m.extend_from_slice(&p);
        }
        assert!(r.feed(&m, &mut out, &mut cmds));
        assert_eq!(cmds, ["down 100 200", "move 130 210", "up 1023 599", "wheel 5 5 1 -"]);
        assert!(cmds.iter().all(|c| crate::cmd::remote_ok(c)));
        // a queue that is full drops moves, never the up
        let mut q: Vec<String> = (0..MAX_CMDS).map(|i| format!("down {} 0", i)).collect();
        queue(&mut q, "move 1 1".into());
        assert_eq!(q.len(), MAX_CMDS);
        queue(&mut q, "up 1 1".into());
        assert_eq!(q.len(), MAX_CMDS + 1);
    }

    /// Keys go on by their keysym, a key held repeats, and the wheel carries the modifiers held with it.
    #[test]
    fn keys_repeat_and_the_wheel_takes_the_modifiers() {
        let mut r = Rfb::new((1024, 600));
        let (mut out, mut cmds) = (Vec::new(), Vec::new());
        assert!(r.feed(b"RFB 003.008\n\x01\x01", &mut out, &mut cmds));
        let key = |down: u8, sym: u32| [&[4, down, 0, 0][..], &sym.to_be_bytes()[..]].concat();
        let ptr = |mask: u8| vec![5, mask, 0, 50, 0, 60];
        // a, held and repeating, up; an up of a key that never went down; Ctrl and Shift held for two wheel steps
        let m = [
            key(1, 0x61),
            key(1, 0x61),
            key(0, 0x61),
            key(0, 0x62),
            key(1, 0xffe3),
            key(1, 0xffe2),
            ptr(16),
            ptr(0),
            ptr(16),
            key(0, 0xffe3),
            ptr(0),
            ptr(8),
        ]
        .concat();
        assert!(r.feed(&m, &mut out, &mut cmds));
        let want = [
            "keysym down 61",
            "keysym repeat 61",
            "keysym up 61",
            "keysym down ffe3",
            "keysym down ffe2",
            "wheel 50 60 -1 cs",
            "wheel 50 60 -1 cs",
            "keysym up ffe3",
            "wheel 50 60 1 s",
        ];
        assert_eq!(cmds, want);
        assert!(cmds.iter().all(|c| crate::cmd::remote_ok(c)));
        assert_eq!(r.keys, [0xffe2]);
        // no more keys held than MAX_KEYS, and a full queue takes ups only
        let mut cmds = Vec::new();
        for sym in 0x100..0x100 + 2 * MAX_KEYS as u32 {
            r.key(true, sym, &mut cmds);
        }
        assert_eq!(r.keys.len(), MAX_KEYS);
        let mut q: Vec<String> = (0..MAX_CMDS).map(|i| format!("down {} 0", i)).collect();
        queue(&mut q, "keysym repeat 61".into());
        queue(&mut q, "wheel 1 1 1 -".into());
        assert_eq!(q.len(), MAX_CMDS);
        queue(&mut q, "keysym up 61".into());
        assert_eq!(q.len(), MAX_CMDS + 1);
    }

    #[test]
    fn only_changed_tiles_are_named() {
        let a = frame(1000, 600, 0x101010);
        assert_eq!(dirty(None, &a), [Rect { x: 0, y: 0, w: 1000, h: 600 }]);
        assert_eq!(
            dirty(Some(&frame(1024, 600, 0x101010)), &a),
            [Rect { x: 0, y: 0, w: 1000, h: 600 }],
            "another size"
        );
        assert!(dirty(Some(&a), &frame(1000, 600, 0x101010)).is_empty());
        let mut b = frame(1000, 600, 0x101010);
        b.px[40 * 1000 + 70] = 0xffffff; // tile (2, 1)
        assert_eq!(dirty(Some(&a), &b), [Rect { x: 64, y: 32, w: 32, h: 32 }]);
        b.px[70 * 1000 + 70] = 0xffffff; // the tile below: one rectangle
        b.px[70 * 1000 + 100] = 0xffffff; // and its neighbour: a wider run, a rectangle of its own
        assert_eq!(dirty(Some(&a), &b), [Rect { x: 64, y: 32, w: 32, h: 32 }, Rect { x: 64, y: 64, w: 64, h: 32 }]);
        let mut c = frame(1000, 600, 0x101010);
        c.px[599 * 1000 + 999] = 0; // the last pixel: the short tile at the right and lower edge
        assert_eq!(dirty(Some(&a), &c), [Rect { x: 992, y: 576, w: 8, h: 24 }]);
        let mut d = frame(1000, 600, 0x101010);
        for y in [10, 50, 90] {
            d.px[y * 1000 + 5] = 1;
        }
        assert_eq!(dirty(Some(&a), &d), [Rect { x: 0, y: 0, w: 32, h: 96 }], "the same run in three rows of tiles");
    }

    #[test]
    fn rectangles_as_fill_png_or_raw() {
        let mut f = frame(64, 32, 0x00336699);
        f.px[5] = 0x00ff0000;
        let novnc = PixFmt { bytes: 4, depth: 24, big: false, max: [255; 3], shift: [0, 8, 16] };
        let rects = [Rect { x: 0, y: 0, w: 32, h: 32 }, Rect { x: 32, y: 0, w: 32, h: 32 }];
        let mut scratch = Vec::new();
        let m = encode_update(&f, &rects, &novnc, true, false, &mut scratch);
        assert_eq!(m[..4], [0, 0, 0, 2]);
        // the first holds the red pixel: PNG
        assert_eq!(m[4..16], [0, 0, 0, 0, 0, 32, 0, 32, 0xff, 0xff, 0xfe, 0xfc]);
        assert_eq!(m[16], 0xa0);
        let png = crate::gfx::png::png(32, 32, &f.px.chunks(64).flat_map(|r| r[..32].to_vec()).collect::<Vec<u32>>());
        let mut len = Vec::new();
        compact_len(&mut len, png.len());
        assert_eq!(m[17..17 + len.len()], len[..]);
        let at = 17 + len.len();
        assert_eq!(m[at..at + png.len()], png[..]);
        assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
        // the second is one colour: fill, red green blue
        assert_eq!(m[at + png.len()..], [0, 32, 0, 0, 0, 32, 0, 32, 0xff, 0xff, 0xfe, 0xfc, 0x80, 0x33, 0x66, 0x99]);
        // Raw for a viewer without Tight PNG, in its order of bytes; the new size first
        let m = encode_update(&f, &rects[1..], &novnc, false, true, &mut scratch);
        assert_eq!(m[..4], [0, 0, 0, 2]);
        assert_eq!(m[4..16], [0, 0, 0, 0, 0, 64, 0, 32, 0xff, 0xff, 0xff, 0x21], "DesktopSize");
        assert_eq!(m[16..28], [0, 32, 0, 0, 0, 32, 0, 32, 0, 0, 0, 0]);
        assert_eq!(m.len(), 28 + 32 * 32 * 4);
        assert_eq!(m[28..32], [0x33, 0x66, 0x99, 0]);
        // 16 bits, 5-6-5, big end first
        let rgb565 = PixFmt::parse(&[16, 16, 1, 1, 0, 31, 0, 63, 0, 31, 11, 5, 0, 0, 0, 0]).unwrap();
        let mut o = Vec::new();
        rgb565.put(0x00ff0000, &mut o);
        rgb565.put(0x0000ff00, &mut o);
        rgb565.put(0x00ffffff, &mut o);
        assert_eq!(o, [0xf8, 0x00, 0x07, 0xe0, 0xff, 0xff]);
        assert!(!rgb565.tpixel());
        assert_eq!(PixFmt::parse(&[8, 8, 0, 0, 0, 7, 0, 7, 0, 3, 0, 3, 6, 0, 0, 0]), None, "a colour map");
        assert_eq!(
            PixFmt::parse(&[16, 16, 0, 1, 0, 31, 0, 63, 0, 31, 16, 5, 0, 0, 0, 0]),
            None,
            "a shift out of the pixel"
        );
        for (n, bytes) in [(5usize, vec![5u8]), (200, vec![0xc8, 1]), (20000, vec![0xa0, 0x9c, 1])] {
            let mut o = Vec::new();
            compact_len(&mut o, n);
            assert_eq!(o, bytes, "{}", n);
        }
    }

    /// One server message of the test's connection: the payload of the next binary frame (pings are passed over).
    fn read_msg(c: &mut TcpStream) -> Vec<u8> {
        loop {
            let mut h = [0u8; 2];
            c.read_exact(&mut h).unwrap();
            let len = match h[1] {
                126 => {
                    let mut l = [0u8; 2];
                    c.read_exact(&mut l).unwrap();
                    u16::from_be_bytes(l) as usize
                }
                127 => {
                    let mut l = [0u8; 8];
                    c.read_exact(&mut l).unwrap();
                    u64::from_be_bytes(l) as usize
                }
                n => n as usize,
            };
            let mut d = vec![0u8; len];
            c.read_exact(&mut d).unwrap();
            if h[0] == 0x82 {
                return d;
            }
        }
    }

    /// A desktop viewer on the port: the protocol without the WebSocket, the first screen, the right button, an
    /// update of nothing while the screen is still, and the port switched off ends the viewer.
    #[test]
    fn a_desktop_viewer_on_the_port() {
        let vnc = Vnc::new();
        let cv = Canvas::new(64, 32);
        vnc.serve(Some(0));
        let port = vnc.port().expect("the port is open");
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let until = |what: &str, done: &mut dyn FnMut() -> bool| {
            let t = Instant::now();
            while !done() {
                assert!(t.elapsed() < Duration::from_secs(10), "{}", what);
                vnc.tick(&cv);
                std::thread::sleep(Duration::from_millis(2));
            }
        };
        until("the frame loop takes the connection", &mut || vnc.viewers() == 1);
        let take = |c: &mut TcpStream, n: usize| {
            let mut b = vec![0u8; n];
            c.read_exact(&mut b).unwrap();
            b
        };
        assert_eq!(take(&mut c, 12), b"RFB 003.008\n");
        c.write_all(b"RFB 003.008\n").unwrap();
        assert_eq!(take(&mut c, 2), [1, 1]);
        c.write_all(&[1]).unwrap();
        assert_eq!(take(&mut c, 4), [0, 0, 0, 0]);
        c.write_all(&[1]).unwrap();
        let init = take(&mut c, 24);
        assert_eq!(init[..4], [0, 64, 0, 32]);
        take(&mut c, u32::from_be_bytes([init[20], init[21], init[22], init[23]]) as usize);
        // no encodings named: the first screen comes Raw in the server's format; the right button with it
        c.write_all(&[3, 0, 0, 0, 0, 0, 0, 64, 0, 32]).unwrap();
        c.write_all(&[5, 4, 0, 10, 0, 12, 5, 0, 0, 10, 0, 12]).unwrap();
        c.set_nonblocking(true).unwrap();
        until("the first screen", &mut || c.peek(&mut [0u8; 1]).is_ok());
        c.set_nonblocking(false).unwrap();
        let m = take(&mut c, 16 + 64 * 32 * 4);
        assert_eq!(m[..16], [0, 0, 0, 1, 0, 0, 0, 0, 0, 64, 0, 32, 0, 0, 0, 0], "all of the screen, Raw");
        let mut cmds = Vec::new();
        until("the right button", &mut || {
            cmds.extend(vnc.take_cmds());
            !cmds.is_empty()
        });
        assert_eq!(cmds, ["context 10 12"]);
        assert!(crate::cmd::remote_ok(&cmds[0]));
        // Remote services > VNC off: the port closes and its viewer ends
        vnc.serve(None);
        assert!(vnc.port().is_none());
        until("the viewer goes", &mut || vnc.viewers() == 0);
    }

    /// Over a real connection: the WebSocket answer, the handshake, the first screen whole, then only what changed,
    /// a touch for the frame loop, and the memory given back when the viewer leaves.
    #[test]
    fn a_viewer_from_request_to_goodbye() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let mut c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
        c.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        let (s, _) = l.accept().unwrap();
        let vnc = Vnc::new();
        let mut cv = Canvas::new(96, 64);
        cv.px.fill(0x202020);
        vnc.tick(&cv);
        assert!(vnc.sh.lock().frame.is_none(), "no viewer, no copy");
        assert!(vnc.adopt(s, "dGhlIHNhbXBsZSBub25jZQ==", true, &[], (96, 64)));
        assert_eq!(vnc.viewers(), 1);
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            let mut b = [0u8; 1];
            c.read_exact(&mut b).unwrap();
            head.push(b[0]);
        }
        let head = String::from_utf8(head).unwrap();
        assert!(
            head.starts_with("HTTP/1.1 101 ")
                && head.contains("Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\n")
                && head.contains("Sec-WebSocket-Protocol: binary\r\n"),
            "{}",
            head
        );
        assert_eq!(read_msg(&mut c), b"RFB 003.008\n");
        c.write_all(&masked(2, b"RFB 003.008\n")).unwrap();
        assert_eq!(read_msg(&mut c), [1, 1]);
        c.write_all(&masked(2, &[1])).unwrap();
        assert_eq!(read_msg(&mut c), [0, 0, 0, 0]);
        c.write_all(&masked(2, &[1])).unwrap();
        assert_eq!(read_msg(&mut c)[..4], [0, 96, 0, 64]);
        c.write_all(&masked(2, &novnc_start())).unwrap();
        // the frame loop, until the viewer has its update
        let pump = |vnc: &Vnc, cv: &Canvas, c: &mut TcpStream| {
            c.set_nonblocking(true).unwrap();
            let t = Instant::now();
            let mut b = [0u8; 1];
            while c.peek(&mut b).is_err() {
                assert!(t.elapsed() < Duration::from_secs(10), "no update");
                vnc.tick(cv);
                std::thread::sleep(Duration::from_millis(2));
            }
            c.set_nonblocking(false).unwrap();
            read_msg(c)
        };
        let m = pump(&vnc, &cv, &mut c);
        assert_eq!(m[..16], [0, 0, 0, 1, 0, 0, 0, 0, 0, 96, 0, 64, 0xff, 0xff, 0xfe, 0xfc], "all of the first screen");
        assert_eq!(m[16..], [0x80, 0x20, 0x20, 0x20], "one colour");
        // a touch, and the next request; the screen changes in one tile
        let mut m = vec![5, 1, 0, 40, 0, 20, 5, 0, 0, 40, 0, 20];
        m.extend_from_slice(&[3, 1, 0, 0, 0, 0, 0, 96, 0, 64]);
        c.write_all(&masked(2, &m)).unwrap();
        cv.px[40 * 96 + 70] = 0xffffff;
        let m = pump(&vnc, &cv, &mut c);
        assert_eq!(m[..16], [0, 0, 0, 1, 0, 64, 0, 32, 0, 32, 0, 32, 0xff, 0xff, 0xfe, 0xfc], "the changed tile only");
        assert_eq!(m[16], 0xa0);
        assert_eq!(vnc.take_cmds(), ["down 40 20", "up 40 20"]);
        // a still screen: the open request is not answered
        c.write_all(&masked(2, &[3, 1, 0, 0, 0, 0, 0, 96, 0, 64])).unwrap();
        for _ in 0..40 {
            vnc.tick(&cv);
            std::thread::sleep(Duration::from_millis(5));
        }
        c.set_nonblocking(true).unwrap();
        assert!(c.peek(&mut [0u8; 1]).is_err(), "an update of nothing");
        c.set_nonblocking(false).unwrap();
        // a finger down when the viewer goes is lifted
        c.write_all(&masked(2, &[5, 1, 0, 9, 0, 9])).unwrap();
        c.write_all(&masked(8, &[])).unwrap();
        let t = Instant::now();
        while vnc.viewers() > 0 {
            assert!(t.elapsed() < Duration::from_secs(10), "the viewer stays");
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(vnc.take_cmds(), ["down 9 9", "up 9 9"]);
        let st = vnc.sh.lock();
        assert!(st.frame.is_none() && st.spare.capacity() == 0, "the screens are given back");
    }

    #[test]
    fn no_more_than_the_viewers_allowed_and_close_ends_them() {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let vnc = Vnc::new();
        let mut clients = Vec::new();
        for i in 0..=MAX_VIEWERS {
            let c = TcpStream::connect(l.local_addr().unwrap()).unwrap();
            let (s, _) = l.accept().unwrap();
            assert_eq!(vnc.adopt(s, "k", false, &[], (96, 64)), i < MAX_VIEWERS, "viewer {}", i);
            clients.push(c);
        }
        assert_eq!(vnc.viewers(), MAX_VIEWERS);
        vnc.close();
        let t = Instant::now();
        while vnc.viewers() > 0 {
            assert!(t.elapsed() < Duration::from_secs(10), "viewers stay after close");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}
