// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The live picture of the web remote (`GET /screen.png`, `/screen.jpg`): the frame loop hands the screen over, a
//! thread of its own makes PNG or JPEG (VPU) of a changed screen, and the viewers waiting for it get it. Base: it needs
//! the canvas, the encoders and the senders of `net`, nothing of the screen's state; the web server (oos-remote's
//! http.rs) hands it the requests through `site`.

use crate::gfx::Canvas;
use std::io::Write;
use std::net::TcpStream;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// The fastest the live picture is made: 20 a second.
const SHOT_MIN: Duration = Duration::from_millis(50);
/// The encoder's share of its core at most (half of a slow core, which is idle otherwise): the time between two copies
/// is at least this many times the last encode.
const SHOT_DUTY: u32 = 2;
/// While the screen stands still, it is looked at again only this often.
const SHOT_STILL: Duration = Duration::from_millis(200);
/// After a request, pictures are made ahead for this long (a viewer that left costs no more than this).
const SHOT_AHEAD: Duration = Duration::from_secs(1);
/// A viewer that has the newest picture waits this long for the next one (long poll), then gets 304.
pub(crate) const LONG_POLL: Duration = if cfg!(test) { Duration::from_millis(1000) } else { Duration::from_secs(2) };
/// A picture from a copy at most this old goes out at once to a viewer that does not have it; else a fresh copy first.
const SHOT_FRESH: Duration = Duration::from_millis(300);
/// Connections waiting for a picture at most (more are answered 503).
const SHOT_WAITING: usize = 8;
/// After the VPU failed, JPEG is tried again this much later (PNG meanwhile).
const VPU_RETRY: Duration = Duration::from_secs(60);

/// The live picture of the web remote.
///
/// A viewer asks with the ETag of the picture it has (If-None-Match). Has it an older one, or none, it gets the newest
/// at once. Has it the newest, the request waits (long poll) until the screen has changed and a new picture is made,
/// at most `LONG_POLL`, then 304. So every new picture reaches the page without delay, and nothing is asked for in
/// vain.
///
/// The frame loop only copies the screen (2.4 MB), and only while somebody waits, and not more often than the encoder
/// can keep up with on a third of a slow core (`SHOT_DUTY`), at most 20 times a second. The encoder thread compares the
/// copy with the last picture, makes a PNG only of a changed screen and hands it to the waiting viewers; it and the
/// senders get only the time the scope leaves over (`system::lower_priority`). The PNG used to be made in the frame
/// loop: one viewer at 15 pictures a second took the scope's screen from 27 to 16 frames a second.
pub struct Shots {
    st: Mutex<ShotState>,
    wake: Condvar,
}

#[derive(Default)]
pub(crate) struct ShotState {
    /// a copy of the screen for the encoder: width, height, pixels
    next: Option<(u32, u32, Vec<u32>)>,
    /// the encoder works on a copy
    busy: bool,
    /// connections waiting for a picture newer than the ETag they have, since when, and in which format (JPEG; PNG
    /// once the VPU failed)
    pub(crate) waiting: Vec<(TcpStream, Option<String>, Instant, bool)>,
    /// the last picture: its pixels (taken out by the encoder while it compares), size, PNG, ETag
    px: Vec<u32>,
    size: (u32, u32),
    png: Option<Arc<[u8]>>,
    tag: String,
    /// when the last copy was taken, and the time to wait after it before the next
    at: Option<Instant>,
    /// the last request for the live picture: while it is recent, the next picture is made ahead of the next request
    asked: Option<Instant>,
    gap: Duration,
    /// pixel buffer for the next copy
    spare: Vec<u32>,
    /// PNGs made so far
    pub(crate) encodes: u64,
    /// the live picture as JPEG from the VPU (`/screen.jpg`, `vpu.rs`) rather than PNG: as the last request asked;
    /// the format of `png`; the VPU failed (PNG until `no_jpeg_at` + `VPU_RETRY`)
    jpeg: bool,
    pic_jpeg: bool,
    no_jpeg: bool,
    /// when the VPU failed: it is tried again after `VPU_RETRY` (one timeout must not end JPEG for the session)
    no_jpeg_at: Option<Instant>,
    /// tests: a long poll other than `LONG_POLL`
    #[cfg(test)]
    pub(crate) long_poll: Option<Duration>,
    /// the frame loop hands its screens over (`hand_over`) instead of `tick` copying them
    frames: bool,
    thread: bool,
    quit: bool,
}

impl Shots {
    pub(crate) fn new() -> Arc<Shots> {
        Arc::new(Shots { st: Default::default(), wake: Default::default() })
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, ShotState> {
        self.st.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The answer to one `GET /screen.png` (`have`: the ETag of If-None-Match); the frame loop never encodes here.
    fn request(self: &Arc<Self>, s: &TcpStream, have: Option<String>, cv: &Canvas, jpeg: bool) {
        let mut st = self.lock();
        st.asked = Some(Instant::now());
        if st.no_jpeg && st.no_jpeg_at.is_some_and(|t| t.elapsed() >= VPU_RETRY) {
            st.no_jpeg = false;
        }
        st.jpeg = jpeg && !st.no_jpeg;
        // an older picture or none: the newest at once, when it is fresh (a screenshot must not be an old one) and in
        // the format asked for
        let fresh =
            st.at.is_some_and(|t| t.elapsed() < SHOT_FRESH) && !st.busy && st.next.is_none() && st.pic_jpeg == st.jpeg;
        if let Some(png) = st
            .png
            .clone()
            .filter(|_| fresh && have.as_deref() != Some(st.tag.as_str()) && st.size == (cv.w as u32, cv.h as u32))
        {
            let tag = st.tag.clone();
            drop(st);
            return answer_shot(s, have.as_deref(), png, &tag);
        }
        if st.waiting.len() >= SHOT_WAITING {
            drop(st);
            return crate::net::busy_answer(s);
        }
        if !st.thread {
            let me = self.clone();
            if std::thread::Builder::new().name("web-png".into()).spawn(move || me.encoder()).is_err() {
                // no thread: the old way, in the frame loop
                drop(st);
                return answer_shot(s, None, crate::gfx::png::png_of(cv).into(), "");
            }
            st.thread = true;
        }
        let Ok(c) = s.try_clone() else { return };
        let want = st.jpeg;
        st.waiting.push((c, have, Instant::now(), want));
        drop(st);
        self.tick(cv);
    }

    /// Every frame: a copy for the encoder when the time has come and somebody waits, or asked within `SHOT_AHEAD` (the
    /// picture is then ready when the viewer's next request comes); 304 to whoever waited too long.
    fn tick(&self, cv: &Canvas) {
        let mut st = self.lock();
        let watched = st.asked.is_some_and(|t| t.elapsed() < SHOT_AHEAD);
        if st.waiting.is_empty() && !watched {
            return;
        }
        let now = Instant::now();
        #[cfg(test)]
        let long_poll = st.long_poll.unwrap_or(LONG_POLL);
        #[cfg(not(test))]
        let long_poll = LONG_POLL;
        let (old, keep): (Vec<_>, Vec<_>) =
            std::mem::take(&mut st.waiting).into_iter().partition(|w| now - w.2 >= long_poll);
        st.waiting = keep;
        if !old.is_empty() {
            let (png, tag) = (st.png.clone().unwrap_or_else(|| Arc::from(&[][..])), st.tag.clone());
            let fit = |w: bool| Self::fits(w, st.pic_jpeg, st.no_jpeg);
            let fits: Vec<bool> = old.iter().map(|w| fit(w.3)).collect();
            drop(st);
            for ((c, have, _, _), ok) in old.into_iter().zip(fits) {
                // the last picture in the format asked for, else "busy" (the page asks again)
                if ok {
                    answer_shot(&c, have.as_deref(), png.clone(), &tag);
                } else {
                    crate::net::busy_answer(&c);
                }
            }
            st = self.lock();
        }
        if !st.frames && Self::wanted(&st, now, watched) {
            Self::give_copy(&mut st, cv, now);
            self.wake.notify_one();
        }
    }

    /// A copy of the screen for the encoder (the caller wakes it).
    fn give_copy(st: &mut ShotState, cv: &Canvas, now: Instant) {
        let mut px = std::mem::take(&mut st.spare);
        px.clear();
        px.extend_from_slice(&cv.px);
        st.next = Some((cv.w as u32, cv.h as u32, px));
        st.at = Some(now);
    }

    /// A picture in JPEG (`pic_jpeg`) or PNG answers a request for `want_jpeg`: the same format, or PNG for JPEG once
    /// the VPU failed. A screenshot asked for as PNG must never get JPEG bytes (it is saved as .png).
    pub(crate) fn fits(want_jpeg: bool, pic_jpeg: bool, no_jpeg: bool) -> bool {
        want_jpeg == pic_jpeg || (want_jpeg && no_jpeg && !pic_jpeg)
    }

    /// Whether the encoder should get the screen now.
    fn wanted(st: &ShotState, now: Instant, watched: bool) -> bool {
        (!st.waiting.is_empty() || watched) && !st.busy && st.next.is_none() && st.at.is_none_or(|t| now - t >= st.gap)
    }

    /// Right before the frame loop renders the next screen (which clears and draws all of it): the shown screen goes to
    /// the encoder by swapping buffers, no copy. A copy of 2.4 MB in the frame loop pushed everything else of the frame
    /// out of the fast core's cache and made the whole frame about 10 % slower. `whole`: the next render draws every
    /// pixel (not the start-up screen); else nothing is swapped and `tick` copies.
    fn hand_over(&self, cv: &mut Canvas, whole: bool) {
        let mut st = self.lock();
        st.frames = whole;
        let now = Instant::now();
        let watched = st.asked.is_some_and(|t| t.elapsed() < SHOT_AHEAD);
        if !whole || !Self::wanted(&st, now, watched) {
            return;
        }
        let mut px = std::mem::take(&mut st.spare);
        px.resize(cv.px.len(), 0);
        std::mem::swap(&mut px, &mut cv.px);
        st.next = Some((cv.w as u32, cv.h as u32, px));
        st.at = Some(now);
        self.wake.notify_one();
    }

    fn encoder(&self) {
        crate::system::lower_priority();
        let mut vpu: Option<crate::vpu::JpegEnc> = None;
        let mut vpu_size = (0, 0);
        let boot = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64 & 0xffff_ffff);
        loop {
            let mut st = self.lock();
            while st.next.is_none() && !st.quit {
                st = self.wake.wait(st).unwrap_or_else(|e| e.into_inner());
            }
            if st.quit {
                return;
            }
            let Some((w, h, px)) = st.next.take() else { continue };
            st.busy = true;
            let last = std::mem::take(&mut st.px);
            // the format of the oldest request waiting (one viewer may ask for JPEG, another for a PNG screenshot)
            let jpeg = st.waiting.first().map_or(st.jpeg, |w| w.3) && !st.no_jpeg;
            let known = st.png.is_some() && st.size == (w, h) && st.pic_jpeg == jpeg;
            drop(st);
            // the comparison and the encoding without the lock: the frame loop may ask meanwhile
            let t0 = Instant::now();
            let mut vpu_failed = false;
            let png: Option<Arc<[u8]>> = (!known || !same_pixels(&last, &px)).then(|| {
                if jpeg {
                    if vpu_size != (w, h) {
                        vpu = None;
                    }
                    if vpu.is_none() {
                        match crate::vpu::JpegEnc::open(w as usize, h as usize) {
                            Ok(e) => {
                                eprintln!("oos-ui: JPEG encoder: VPU, {} in", e.input());
                                (vpu, vpu_size) = (Some(e), (w, h));
                            }
                            Err(e) => eprintln!("oos-ui: JPEG encoder: {}", e),
                        }
                    }
                    match vpu.as_mut().map(|e| e.encode(&px)) {
                        Some(Ok(j)) => return j.into(),
                        Some(Err(e)) => eprintln!("oos-ui: JPEG encoder: {}", e),
                        None => {}
                    }
                    (vpu, vpu_failed) = (None, true);
                }
                crate::gfx::png::png(w, h, &px).into()
            });
            let took = t0.elapsed();
            let mut st = self.lock();
            st.busy = false;
            if vpu_failed {
                (st.no_jpeg, st.jpeg, st.no_jpeg_at) = (true, false, Some(Instant::now()));
            }
            let Some(png) = png else {
                // the same screen: who has an older picture gets this one, who has it waits on; the next look later
                st.gap = SHOT_STILL.max(took * SHOT_DUTY);
                st.px = last;
                st.spare = px;
                let tag = st.tag.clone();
                let (pj, nj) = (st.pic_jpeg, st.no_jpeg);
                let (lack, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut st.waiting)
                    .into_iter()
                    .partition(|w| w.1.as_deref() != Some(tag.as_str()) && Self::fits(w.3, pj, nj));
                st.waiting = keep;
                let png = st.png.clone().unwrap_or_else(|| Arc::from(&[][..]));
                drop(st);
                for (c, have, _, _) in lack {
                    answer_shot(&c, have.as_deref(), png.clone(), &tag);
                }
                continue;
            };
            st.gap = SHOT_MIN.max(took * SHOT_DUTY);
            st.encodes += 1;
            st.tag = format!("\"{:x}-{}\"", boot, st.encodes);
            st.pic_jpeg = png.starts_with(&[0xff, 0xd8]);
            st.png = Some(png.clone());
            st.size = (w, h);
            st.px = px;
            st.spare = last;
            // who asked for the other format waits for the next picture, made in that format
            let (pj, nj) = (st.pic_jpeg, st.no_jpeg);
            let (waiting, other): (Vec<_>, Vec<_>) =
                std::mem::take(&mut st.waiting).into_iter().partition(|w| Self::fits(w.3, pj, nj));
            st.waiting = other;
            let tag = st.tag.clone();
            drop(st);
            for (c, have, _, _) in waiting {
                answer_shot(&c, have.as_deref(), png.clone(), &tag);
            }
        }
    }
}

/// Whether two screens are the same, eight bytes at a step. `==` on the slices ends in musl's memcmp, which goes byte
/// by byte: 12 ms for a screen on an A53, against about 1 ms here.
pub fn same_pixels(a: &[u32], b: &[u32]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let ((ca, ra), (cb, rb)) = (a.as_chunks::<8>(), b.as_chunks::<8>());
    // 32 bytes at a time with an OR of the differences: no early exit per word, which lets the compiler use vectors
    ca.iter().zip(cb).all(|(x, y)| x.iter().zip(y).fold(0u32, |d, (p, q)| d | (p ^ q)) == 0) && ra == rb
}

/// 304 when the viewer has this picture, else the PNG through a sender thread (503 when all are busy).
fn answer_shot(s: &TcpStream, have: Option<&str>, png: Arc<[u8]>, tag: &str) {
    let etag = if tag.is_empty() { String::new() } else { format!("ETag: {}\r\n", tag) };
    if !tag.is_empty() && have == Some(tag) {
        let _ = s.set_write_timeout(Some(Duration::from_secs(1)));
        let _ = (&*s).write_all(
            format!("HTTP/1.1 304 Not Modified\r\n{}Cache-Control: no-store\r\nConnection: close\r\n\r\n", etag)
                .as_bytes(),
        );
        return;
    }
    let ctype = if png.starts_with(&[0xff, 0xd8]) { "image/jpeg" } else { "image/png" };
    let head = format!("HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}Cache-Control: no-store\r\nConnection: close\r\n\r\n", ctype, png.len(), etag);
    if !crate::net::send_in_background(s, head, png) {
        crate::net::busy_answer(s);
    }
}

/// After a request for the monitor's picture the frame loop makes one for this long.
const MONITOR_ASK: Duration = Duration::from_secs(3);
/// The web server waits this long for the monitor's picture (the frame loop makes one at the next frame of the
/// monitor: 15 a second on the CPU, half the frames on the GPU).
pub(crate) const MONITOR_WAIT: Duration = Duration::from_secs(5);

/// The picture a monitor on the HDMI port gets, for `GET /monitor.png`. The web server asks for it and waits on a
/// thread of its own; the frame loop, which makes that picture for the monitor, copies it when it was asked for
/// (`wanted`, `give`) and a thread encodes the copy, so the frame loop never waits for the PNG.
#[derive(Default)]
pub struct MonitorShot {
    st: Mutex<MonitorState>,
    done: Condvar,
}

#[derive(Default)]
struct MonitorState {
    /// the last request
    asked: Option<Instant>,
    /// the newest picture, as PNG, and when it was copied
    png: Option<(Instant, Arc<Vec<u8>>)>,
    /// a copy is being encoded
    encoding: bool,
}

impl MonitorShot {
    /// A request: the frame loop makes the next picture of the monitor for it. Returns the time to wait from.
    pub fn ask(&self) -> Instant {
        let now = Instant::now();
        self.st.lock().unwrap_or_else(|e| e.into_inner()).asked = Some(now);
        now
    }

    /// The frame loop should copy the monitor's next picture: someone asked a moment ago, and no picture made since
    /// is there yet or on its way.
    pub fn wanted(&self) -> bool {
        let st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        !st.encoding
            && st.asked.is_some_and(|a| a.elapsed() < MONITOR_ASK && st.png.as_ref().is_none_or(|(t, _)| *t < a))
    }

    /// The monitor's picture of this frame: copied here, encoded on a thread of its own.
    pub fn give(self: &Arc<Self>, cv: &Canvas) {
        let copied = Instant::now();
        {
            let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
            if st.encoding {
                return;
            }
            st.encoding = true;
        }
        let mut copy = Canvas::new(cv.w, cv.h);
        copy.px.copy_from_slice(&cv.px);
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new().name("monitor-png".into()).spawn(move || {
            crate::system::lower_priority();
            let png = Arc::new(crate::gfx::png::png_of(&copy));
            let mut st = me.st.lock().unwrap_or_else(|e| e.into_inner());
            st.png = Some((copied, png));
            st.encoding = false;
            me.done.notify_all();
        });
        if spawned.is_err() {
            self.st.lock().unwrap_or_else(|e| e.into_inner()).encoding = false;
        }
    }

    /// The first picture copied after `since`, as PNG; None when there is none within `timeout` (the monitor went
    /// away, or the frame loop stopped).
    pub fn wait(&self, since: Instant, timeout: Duration) -> Option<Arc<Vec<u8>>> {
        let end = Instant::now() + timeout;
        let mut st = self.st.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some((_, png)) = st.png.as_ref().filter(|(t, _)| *t >= since) {
                return Some(Arc::clone(png));
            }
            let left = end.checked_duration_since(Instant::now())?;
            st = self.done.wait_timeout(st, left).unwrap_or_else(|e| e.into_inner()).0;
        }
    }
}

/// The live picture as the frame loop owns it; the encoder thread ends with it.
pub struct Live {
    pub(crate) shots: Arc<Shots>,
    /// the monitor's picture (`GET /monitor.png`)
    pub(crate) monitor: Arc<MonitorShot>,
}

impl Default for Live {
    fn default() -> Live {
        Live { shots: Shots::new(), monitor: Arc::default() }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.shots.lock().quit = true;
        self.shots.wake.notify_all();
    }
}

impl Live {
    /// The answer to one `GET /screen.png` or `/screen.jpg` (`have`: the ETag of If-None-Match); it never encodes in
    /// the frame loop.
    pub fn request(&self, s: &TcpStream, have: Option<String>, cv: &Canvas, jpeg: bool) {
        self.shots.request(s, have, cv, jpeg);
    }

    /// Every pass of the frame loop, before the requests: a copy for the encoder when it is time, 304 to who waited
    /// too long (`Shots::tick`).
    pub fn tick(&self, cv: &Canvas) {
        self.shots.tick(cv);
    }

    /// Call right before rendering the next screen: hands the shown one to the live picture (`Shots::hand_over`).
    /// `whole`: the render draws every pixel.
    pub fn before_render(&self, cv: &mut Canvas, whole: bool) {
        self.shots.hand_over(cv, whole);
    }

    /// System > Graphics = GPU: the frame loop hands over the pictures that come back from the GPU
    /// (`picture_read_back`), `tick` copies none and nothing is swapped. `cv` holds the last picture read back, not
    /// one drawn every frame: a buffer swapped out of it would stay in it, an older picture, until the next read.
    pub fn before_gpu_frame(&self) {
        self.shots.lock().frames = true;
    }

    /// System > Graphics = GPU: `cv` was just read back whole; a copy goes to the encoder when it wants one.
    pub fn picture_read_back(&self, cv: &Canvas) {
        let mut st = self.shots.lock();
        let watched = st.asked.is_some_and(|t| t.elapsed() < SHOT_AHEAD);
        let now = Instant::now();
        if Shots::wanted(&st, now, watched) {
            Shots::give_copy(&mut st, cv, now);
            self.shots.wake.notify_one();
        }
    }

    /// The monitor's picture is asked for (`MonitorShot::wanted`): the frame loop reads it back or copies it.
    pub fn monitor_wanted(&self) -> bool {
        self.monitor.wanted()
    }

    /// The monitor's picture of this frame, for the one who asked (`MonitorShot::give`).
    pub fn monitor_give(&self, cv: &Canvas) {
        self.monitor.give(cv);
    }

    /// The live picture would take the next screen (System > Graphics = GPU: read it back for it).
    pub fn picture_wanted(&self) -> bool {
        let st = self.shots.lock();
        let watched = st.asked.is_some_and(|t| t.elapsed() < SHOT_AHEAD);
        Shots::wanted(&st, Instant::now(), watched)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A request for the monitor's picture makes the frame loop copy one; it is encoded after the copy and handed to
    /// the one that waits, and a picture made before the request is not the answer.
    #[test]
    fn the_monitor_picture_comes_after_the_request() {
        let m = Arc::new(MonitorShot::default());
        assert!(!m.wanted(), "nobody asked");
        let t = m.ask();
        assert!(m.wanted());
        let mut cv = Canvas::new(8, 4);
        cv.px.fill(0xff12_3456);
        m.give(&cv);
        let png = m.wait(t, Duration::from_secs(5)).expect("a picture");
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(!m.wanted(), "the picture is there");
        // a later request wants a newer one, and the old one is not its answer
        let t2 = m.ask();
        assert!(m.wanted());
        assert!(m.wait(t2, Duration::from_millis(30)).is_none());
    }

    /// A screenshot asked for as PNG never gets JPEG bytes, a JPEG viewer gets PNG only once the VPU failed.
    #[test]
    fn a_picture_answers_only_its_own_format() {
        assert!(Shots::fits(false, false, false) && Shots::fits(true, true, false));
        assert!(!Shots::fits(false, true, false) && !Shots::fits(false, true, true), "PNG asked, JPEG made");
        assert!(!Shots::fits(true, false, false), "JPEG asked, PNG made while the VPU works: wait for the JPEG");
        assert!(Shots::fits(true, false, true), "the VPU failed: PNG for everyone");
    }
}
