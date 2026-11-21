// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What the web remote answers from the screen (ADR 0049, the command layer, beside `cmd` and `scpi`): its state, the
//! last record as a file, an update package, a waveform file for a reference slot, the live picture, and MCP (its
//! tools in site/mcp.rs). The transport is `oos_remote::http` (ADR 0064): it reads the requests, keeps the limits and
//! sends the answers, and asks the screen through `oos_remote::http::Site`, which `Ctx` is for one pass of the frame
//! loop.

use crate::gfx::Canvas;
use crate::scpi::Scpi;
use crate::shots::Live;
use crate::ui::Ui;
use oos_remote::http::Body;
use std::net::TcpStream;

/// What an answer of the web server may use: the screen, its last picture, the SCPI interpreter and the live picture,
/// for one pass of the frame loop.
pub struct Ctx<'a> {
    ui: &'a mut Ui,
    cv: &'a Canvas,
    scpi: &'a mut Scpi,
    live: Option<&'a Live>,
}

impl<'a> Ctx<'a> {
    pub fn new(ui: &'a mut Ui, cv: &'a Canvas, scpi: &'a mut Scpi) -> Ctx<'a> {
        Ctx { ui, cv, scpi, live: None }
    }

    /// With the live picture of the frame loop: without it `/screen.png` and `/screen.jpg` are answered 503 (tests
    /// that ask for no picture).
    pub fn with_live(mut self, live: &'a Live) -> Ctx<'a> {
        self.live = Some(live);
        self
    }
}

/// The waveform files `POST /waveform` takes, read on the web server's thread.
pub type Upload = Vec<crate::store::WaveTrace>;

/// The web server of oos-ui: the transport of oos-remote with the screen's uploads.
pub type Web = oos_remote::http::Http<Upload>;

impl oos_remote::http::Site for Ctx<'_> {
    type Upload = Upload;
    const MAX_WAVEFORM: usize = crate::store::MAX_WAVEFORM_BYTES;

    /// The live picture's turn (`Live::tick`).
    fn pass(&mut self) {
        if let Some(live) = self.live {
            live.tick(self.cv);
        }
        mcp::expire_files(self.scpi);
    }

    /// The live picture answers it; 503 without one (tests that ask for no picture).
    fn picture(&mut self, s: &TcpStream, have: Option<String>, jpeg: bool) {
        match self.live {
            Some(live) => live.request(s, have, self.cv, jpeg),
            None => crate::net::busy_answer(s),
        }
    }

    /// Keys, knobs and taps (`cmd::remote_ok`).
    fn remote_ok(&self, line: &str) -> bool {
        crate::cmd::remote_ok(line)
    }

    /// /ui-state, /stamp, /wave.csv, /la.csv, /wave.bin, POST /update, /screen.bmp, /mcp and /mcp/waveform/<file>.
    fn answer(&mut self, route: &str, post: bool, body: &[u8]) -> Option<(&'static str, Body, bool)> {
        let ok = |ctype: &'static str, b: Vec<u8>| Some((ctype, Body::Now(b), false));
        let file = |ctype: &'static str, r: Result<Vec<u8>, String>| match r {
            Ok(b) => Some((ctype, Body::Now(b), false)),
            Err(e) => Some(("text/plain", Body::Now(e.into_bytes()), true)),
        };
        let ui = &mut *self.ui;
        match route {
            "/ui-state" if !post => ok("application/json", ui.workflow_state().into_bytes()),
            // the scope's local time for the names of the page's downloads; empty while its clock is not set
            "/stamp" => ok("text/plain", ui.file_stamp().unwrap_or_default().into_bytes()),
            "/wave.csv" => file("text/csv; charset=utf-8", ui.web_wave_csv().map(String::into_bytes)),
            "/la.csv" => file("text/csv; charset=utf-8", ui.web_la_csv().map(String::into_bytes)),
            "/wave.bin" => file("application/octet-stream", ui.web_wave_bin()),
            "/update" if post => match ui.update_upload(body) {
                Ok(m) => ok("text/plain", m.into_bytes()),
                // refused: the body says why (409, #205)
                Err(e) => Some(("text/plain", Body::Now(format!("REFUSED: {}", e).into_bytes()), true)),
            },
            // the screen copied here, encoded on a sender thread
            "/screen.bmp" => {
                let shot = snapshot(self.cv);
                Some(("image/bmp", Body::Later(Box::new(move || crate::scpi::bmp(&shot))), false))
            }
            _ if !post && route.starts_with("/mcp/waveform/") => {
                let name = &route["/mcp/waveform/".len()..];
                file("text/csv; charset=utf-8", waveform_csv(ui, self.scpi, name))
            }
            "/mcp" => match mcp(&String::from_utf8_lossy(body), self) {
                Some(b) => Some(("application/json", b, false)),
                None => ok("", Vec::new()),
            },
            _ => None,
        }
    }

    /// `Ui::update_status` (run by the server on a thread of its own), and the result of the last upload.
    fn update_state(&mut self) -> (fn() -> String, String) {
        (Ui::update_status, self.ui.update_last().to_string())
    }

    /// `Ui::waveform_epoch`.
    fn waveform_epoch(&self) -> u64 {
        self.ui.waveform_epoch()
    }

    fn parse_upload(file: &[u8], name: &str) -> Result<Upload, String> {
        crate::store::parse_wave(file, name)
    }

    fn take_upload(&mut self, parsed: Result<Upload, String>, name: &str, slot: usize, epoch: u64) -> (bool, String) {
        let ui = &mut *self.ui;
        let previous = ui.ref_slot();
        let result = if ui.waveform_epoch() != epoch {
            Err("The session changed while the file was read; load it again.".to_string())
        } else {
            parsed.and_then(|traces| ui.commit_waveform_traces_at(traces, name, slot))
        };
        match result {
            Ok(()) => {
                let hint = if ui.record_mode() {
                    "Use Run/Stop or Single to acquire from this record; Return to live restores the live source."
                } else {
                    "It is on with the live channels. Fit shows its full record; Take over enters stored acquisition."
                };
                (true, format!("Loaded {} into R{}. {}", name, ui.ref_slot() + 1, hint))
            }
            Err(error) => {
                ui.ref_slot_set(previous);
                (false, error)
            }
        }
    }
}

/// A copy of the screen's pixels, to be encoded off the frame loop.
fn snapshot(cv: &Canvas) -> Canvas {
    let mut shot = Canvas::new(cv.w, cv.h);
    shot.px.copy_from_slice(&cv.px);
    shot
}

mod mcp;
pub use mcp::{mcp, waveform_csv};

#[cfg(test)]
mod tests;
