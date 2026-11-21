// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Saving and recalling: the toast after a save, the USB stick (where files go, the
//! flush before it is pulled, noticing it come and go) and the set-up file with the looks of the references, and
//! recalling it. A new kind of file to save goes next to `Ui::save_dir`.

use super::*;

impl Ui {
    /// The file of a session: `path` as given (relative: beside the setups), or the next `session-NNN.oss` of the save
    /// folder for an empty one.
    pub(super) fn session_file(&self, path: &str, new: bool) -> Result<std::path::PathBuf, String> {
        let raw = path.trim().trim_matches('"');
        if raw.is_empty() {
            return if new {
                Ok(crate::store::next_name(&self.save_dir(), "session", "oss"))
            } else {
                crate::store::latest_name(&self.save_dir(), "session", "oss").ok_or_else(|| "missing session-NNN.oss in the save folder".to_string())
            };
        }
        let named = std::path::Path::new(raw);
        Ok(if named.is_absolute() { named.to_path_buf() } else { self.setup_path().parent().map(|d| d.join(named)).unwrap_or_else(|| named.to_path_buf()) })
    }

    /// Save session: the setup, the record on the screen and every loaded reference slot with its samples, as one file
    /// (ADR 0027). Storage > Save session and `:SAVE:SESSion`.
    /// The record of a session when the acquisition kept more than the screen shows: every sample of the channels
    /// that are on, as an OOSW blob. None when the screen record is all there is (min/max pairs, roll, a short record),
    /// or when the channels do not share one time axis.
    pub(super) fn session_memory(&self) -> Option<Vec<u8>> {
        let rec = self.last[0].as_ref()?;
        let on: Vec<usize> = (0..NCH).filter(|&c| self.m.ch[c].on && !rec.ch[c].is_empty()).collect();
        let views: Vec<(usize, &[f32], f64, f64)> = on.iter().filter_map(|&c| self.channel_view(c, true).map(|(v, dt, t0)| (c, v, dt, t0))).collect();
        let &(_, first, dt, t0) = views.first()?;
        let one_axis = views.len() == on.len() && views.iter().all(|&(_, v, d, t)| v.len() == first.len() && d == dt && t == t0);
        if !one_axis || first.len() <= SESSION_SCREEN_MAX {
            return None;
        }
        let chans: Vec<(usize, &[f32])> = views.iter().map(|&(c, v, _, _)| (c, v)).collect();
        Some(crate::store::wave_bin_of(t0, dt, &chans))
    }

    /// The record of a loaded session with more points than a screen has is an acquisition: it becomes the frame
    /// behind the record, and the screen is drawn from it for every view, as after a capture (`refresh_capture_view`).
    pub(super) fn session_record(&mut self, rec: Record) -> Record {
        let n = rec.ch.iter().map(Vec::len).max().unwrap_or(0);
        if n <= SESSION_SCREEN_MAX || !(rec.dt > 0.0) || !rec.t0.is_finite() {
            return rec;
        }
        let Record { t0, dt, ch, triggered, .. } = rec;
        let basic = std::array::from_fn(|c| crate::board::BasicStats::of(&ch[c]));
        let pre = (-t0 / dt).round().clamp(0.0, (n - 1) as f64) as usize;
        let frame = std::sync::Arc::new(crate::board::Frame { seq: 1, dt, pre, ch, acq: (1.0 / dt, n), basic, ..Default::default() });
        let raw = Raw { frame, i0: 0, i1: n, t0, conditioned: true };
        let whole = Record { t0, dt, ch: Default::default(), triggered, partial: false, raw: Some(raw), la: None, pairs: false };
        let view = (self.m.time_per_div(), self.m.delay);
        self.captured_record_view(&whole, view).map(|r| r.as_ref().clone()).unwrap_or(whole)
    }

    pub(crate) fn session_save(&mut self, path: &str) -> Result<std::path::PathBuf, String> {
        let p = self.session_file(path, true)?;
        self.session_write(p)
    }

    /// Save session to the file `p`.
    pub(super) fn session_write(&mut self, p: std::path::PathBuf) -> Result<std::path::PathBuf, String> {
        let setup = self.setup_text_full();
        let refs: Vec<crate::store::SessionRef> = self.refs.iter().enumerate().filter_map(|(slot, r)| {
            r.as_ref().map(|r| crate::store::SessionRef { slot, src: r.src, t0: r.t0, dt: r.dt, v: r.v.to_vec() })
        }).collect();
        let memory = self.session_memory();
        let mut bytes = crate::store::session_bin(&setup, self.last[0].as_ref(), memory.as_deref(), &self.m, &refs);
        // saved in the offline view, a loaded capture fills the screen: the session comes back that way
        if self.offline && !self.session_is_sim && self.sim_refs.is_empty() && self.refs.get(self.offline_ref).is_some_and(Option::is_some) {
            bytes.extend_from_slice(&crate::store::session_view(self.offline_ref));
        }
        let r = crate::store::write(&p, &bytes);
        let out = r.as_ref().map(|_| p.clone()).map_err(|e| e.to_string());
        self.toast_saved(&p, r);
        out
    }

    /// Load session: the scope stops and shows what the file holds. The setup is applied as Recall setup does, the
    /// record becomes the one on the screen, and the reference slots are replaced by those of the file (a slot the file
    /// does not have is emptied: the session is the whole bench, not an addition to it).
    pub(crate) fn session_load(&mut self, path: &str) -> Result<(), String> {
        match self.session_file(path, false) {
            Ok(p) => self.session_read(p),
            Err(e) => {
                self.toast(&format!("Load failed: {e}"));
                Err(e)
            }
        }
    }

    /// Load session from the file `p`.
    pub(super) fn session_read(&mut self, p: std::path::PathBuf) -> Result<(), String> {
        let done = (|| {
            let data = crate::store::read_waveform_file(&p)?;
            let session = crate::store::parse_session(&data, NREF)?;
            // checked before anything is changed: a file with a setup this build cannot read leaves the bench as it is
            crate::store::parse_setup(&session.setup)?;
            let offline_ref = session.offline_ref;
            if self.record_mode() {
                self.leave_session();
            }
            for r in self.refs.iter_mut() {
                *r = None;
            }
            for r in session.refs {
                self.refs[r.slot] = Some(RefWave { src: r.src, color: r.slot, label: String::new(), t0: r.t0, dt: r.dt, v: r.v.into(), vpd: 1.0, offset: 0.0, on: true });
            }
            // the look of the slots (scale, offset, colour, name, shown) is in the setup text and is applied with it
            self.apply_setup_text(&session.setup)?;
            self.m.run = RunState::Stop;
            self.history.clear();
            self.hist_pos = None;
            if let Some(rec) = session.record {
                self.last[0] = Some(self.session_record(rec));
                self.cleared = false;
            }
            // whatever the hardware still holds is not this record
            self.stop.ti_off = None;
            (self.stop.raw, self.stop.pic) = (None, None);
            self.loaded_record = true;
            self.invalidate_reference_eye();
            if let Some(slot) = offline_ref {
                self.restore_offline_view(slot);
            }
            self.measure_last();
            self.measure_refs();
            Ok(p)
        })();
        match done {
            Ok(p) => {
                self.toast(&format!("Loaded {}", crate::store::file_name(&p)));
                Ok(())
            }
            Err(e) => {
                self.toast(&format!("Load failed: {e}"));
                Err(e)
            }
        }
    }

    /// Storage > Save waveform (CSV or binary): the newest record to the save folder, or to a file of the caller
    /// (SCPI `:SAVE:WAVeform "path"`).
    pub(crate) fn store_wave(&mut self, csv: bool, path: Option<std::path::PathBuf>) {
        // for the SCPI recorder: the row of the Storage menu, when there is a waveform to save
        if path.is_none() && self.last[0].is_some() {
            self.rec_action(if csv { ":SAVE:WAVeform" } else { ":SAVE:WAVeform ,BIN" });
        }
        if self.last[0].is_none() {
            return self.toast("No waveform to save");
        }
        // the LA lines of a file OOS names go to the next la-NNN.csv; a file of the caller is that one file
        let la = (csv && path.is_none()).then(|| self.new_file(&self.save_dir(), "la", "csv"));
        let p = path.unwrap_or_else(|| self.new_file(&self.save_dir(), "wave", if csv { "csv" } else { "bin" }));
        self.store_wave_to(csv, p, la);
    }

    /// The newest record to `p`; the LA lines of a CSV to `la`, when there is one and the analyser is on.
    pub(super) fn store_wave_to(&mut self, csv: bool, p: std::path::PathBuf, la: Option<std::path::PathBuf>) {
        let Some(rec) = &self.last[0] else { return self.toast("No waveform to save") };
        let mut r = if csv {
            crate::store::write(&p, crate::store::wave_csv(rec, &self.m).as_bytes())
        } else {
            crate::store::write(&p, &crate::store::wave_bin(rec, &self.m))
        };
        // the LA lines go to their own file: another sample rate, and only the changes
        if let (true, Ok(()), Some(lines), true, Some(q)) = (csv, &r, &rec.la, self.m.la_on(), &la) {
            r = crate::store::write(q, crate::store::la_csv(lines, &self.m.la.lines()).as_bytes());
        }
        self.toast_saved(&p, r);
    }

    /// Save of the acquisition memory (`:SAVE:MEMory:WAVeform`): every sample the acquisition kept of the channels
    /// that are on, where `store_wave` writes the points of the screen. Peak detect, roll and a record above 1 M
    /// points keep no single samples; there it is the screen record again.
    pub(crate) fn store_wave_memory(&mut self, csv: bool, path: Option<std::path::PathBuf>) -> Result<(), String> {
        const CSV_MAX: usize = 1_000_000;
        let data = {
            let views: Vec<(usize, &[f32], f64, f64)> = (0..NCH)
                .filter(|&c| self.m.ch[c].on)
                .filter_map(|c| self.channel_view(c, true).map(|(v, dt, t0)| (c, v, dt, t0)))
                .collect();
            let Some(&(_, _, dt, t0)) = views.first() else {
                return Err("-221,\"Settings conflict\" no waveform to save".into());
            };
            if csv && views.iter().any(|(_, v, _, _)| v.len() > CSV_MAX) {
                return Err("-221,\"Settings conflict\" more than 1 M points: save as BIN".into());
            }
            let chans: Vec<(usize, &[f32])> = views.iter().map(|&(c, v, _, _)| (c, v)).collect();
            if csv {
                crate::store::wave_csv_of(t0, dt, &chans).into_bytes()
            } else {
                crate::store::wave_bin_of(t0, dt, &chans)
            }
        };
        let p = path.unwrap_or_else(|| self.new_file(&self.save_dir(), "wave", if csv { "csv" } else { "bin" }));
        let r = crate::store::write(&p, &data);
        let failed = r.as_ref().err().map(|e| format!("-250,\"Mass storage error\" {e}"));
        self.toast_saved(&p, r);
        failed.map_or(Ok(()), Err)
    }

    pub(super) fn toast_saved(&mut self, p: &std::path::Path, r: std::io::Result<()>) {
        match r {
            Ok(()) => {
                self.flush_stick();
                self.toast(&format!("Saved {}{}", crate::store::file_name(p), self.saved_where()));
            }
            Err(e) => self.toast(&format!("Save failed: {}", e)),
        }
    }

    /// A USB stick is mounted and the storage directory was not set by hand: files go to it.
    pub fn usb_used(&self) -> bool {
        self.usb && self.store_dir == std::path::Path::new(crate::store::DEFAULT_DIR)
    }

    /// Where screenshots, CSV, mask copies and fast-frame files go now.
    pub fn save_dir(&self) -> std::path::PathBuf {
        match self.store_dest {
            crate::store::StoreDest::Stick => std::path::PathBuf::from(crate::store::USB_DIR),
            crate::store::StoreDest::Local => self.store_dir.clone(),
            crate::store::StoreDest::Auto => {
                if self.usb_used() {
                    std::path::PathBuf::from(crate::store::USB_DIR)
                } else {
                    self.store_dir.clone()
                }
            }
        }
    }

    /// For the message after saving.
    pub fn saved_where(&self) -> &'static str {
        match self.store_dest {
            crate::store::StoreDest::Local if self.usb => " locally, not on the stick",
            crate::store::StoreDest::Stick => " on the USB stick",
            crate::store::StoreDest::Auto if self.usb_used() => " on the USB stick",
            _ => "",
        }
    }

    /// After a file: on the stick for real, so that it can be pulled. Not in the frame loop, a stick can take its time.
    pub fn flush_stick(&self) {
        if self.usb_used() {
            let _ = std::thread::Builder::new().name("sync".into()).spawn(crate::store::sync_disks);
        }
    }

    /// From `step`: is a stick mounted (`/bin/oos-usb`); says so when that changes.
    pub(super) fn usb_poll(&mut self) {
        if cfg!(test) || self.now < self.usb_check {
            return;
        }
        self.usb_check = self.now + 2.0;
        self.net.wlan_stick = wlan::stick_there();
        self.bt.stick = bt::stick_there();
        self.bt.inputs = bt::inputs_connected(&bt::inputs());
        let on = std::fs::read_to_string("/proc/mounts").is_ok_and(|t| crate::store::usb_mounted(&t));
        if on != self.usb {
            self.usb = on;
            if self.store_dir == std::path::Path::new(crate::store::DEFAULT_DIR) {
                self.toast(if on { "USB stick ready: files are saved to it" } else { "USB stick removed" });
            }
        }
    }

    /// File of the current setup slot: a setup is small and has to survive a power cycle, so the default storage directory
    /// in RAM is not used for it when OOS's data partition is there.
    pub(super) fn setup_path(&self) -> std::path::PathBuf {
        let keep = std::path::Path::new("/oos/setups");
        let dir = if self.store_dir == std::path::Path::new(crate::store::DEFAULT_DIR) && keep.is_dir() {
            keep
        } else {
            self.store_dir.as_path()
        };
        dir.join(format!("setup{}.txt", self.store_slot))
    }

    /// The setup file the Storage menu uses. An empty name is the current slot (`setupN.txt`). A number 1 to 9 selects
    /// that slot. A `.stp` or `.txt` name is that file, beside the setups when the path is relative. `slot` and
    /// `suffix` are the refusals.
    #[cfg_attr(target_os = "linux", allow(dead_code))] // used by the desktop build and the tests
    pub(crate) fn setup_file(&mut self, path: &str) -> Result<std::path::PathBuf, String> {
        let raw = path.trim().trim_matches('"');
        if raw.is_empty() {
            return Ok(self.setup_path());
        }
        if let Ok(slot) = raw.parse::<u32>() {
            if !(1..=9).contains(&slot) {
                return Err("slot".into());
            }
            self.store_slot = slot;
            return Ok(self.setup_path());
        }
        let named = std::path::Path::new(raw);
        let ext = named.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext != "stp" && ext != "txt" {
            return Err("suffix".into());
        }
        Ok(if named.is_absolute() {
            named.to_path_buf()
        } else {
            self.setup_path().parent().map(|d| d.join(named)).unwrap_or_else(|| named.to_path_buf())
        })
    }

    /// The `ref<N>.*` lines of a setup: how each loaded ref slot looks (on/off, scale, offset, colour, name).
    pub(super) fn ref_setup_text(&self) -> String {
        let looks: Vec<_> = self
            .refs
            .iter()
            .map(|r| {
                r.as_ref().map(|r| crate::store::RefLook {
                    on: Some(r.on),
                    vdiv: Some(r.vpd),
                    offset: Some(r.offset),
                    color: Some(r.color),
                    label: Some(r.label.clone()),
                })
            })
            .collect();
        crate::store::ref_setup_text(&looks)
    }

    /// Applies the saved look to the ref slots that hold a waveform now (the samples are not in a setup).
    fn apply_ref_looks(&mut self, looks: &[Option<crate::store::RefLook>]) {
        for (r, l) in self.refs.iter_mut().zip(looks) {
            let (Some(r), Some(l)) = (r.as_mut(), l) else { continue };
            if let Some(v) = l.on {
                r.on = v;
            }
            if let Some(v) = l.vdiv {
                r.vpd = v;
            }
            if let Some(v) = l.offset {
                r.offset = v;
            }
            if let Some(v) = l.color {
                r.color = v.min(REF_COLORS.len() - 1);
            }
            if let Some(v) = &l.label {
                r.label = v.clone();
            }
        }
    }

    /// Loads setup<slot>.txt of the storage directory; the acquisition state (run / stop) stays as it is. What this
    /// build cannot take (a decoder it does not have, a key of a newer build) is left out and named.
    pub(super) fn recall_setup(&mut self) {
        let p = self.setup_path();
        self.recall_setup_from(&p);
    }

    /// Recall setup from the file `p`: the slot's, or one the user chose (Storage > Load setup from file).
    pub(super) fn recall_setup_from(&mut self, p: &std::path::Path) {
        let text = match std::fs::read_to_string(p) {
            Ok(t) => t,
            Err(e) => return self.toast(&format!("Recall failed: {}", e)),
        };
        let (m, bad) = crate::store::parse_setup_lenient(&text);
        for e in &bad {
            eprintln!("oos-ui: recall {}: left out: {}", p.display(), e);
        }
        // a set-up is a live one, as with the Default key: a reference or SIM session ends first, so a Live afterwards
        // does not bring back the set-up from before it over the recalled one
        if self.record_mode() {
            self.leave_session();
        }
        self.apply_setup_model(m, &text);
        match bad.first() {
            None => self.toast(&format!("Recalled {}", crate::store::file_name(p))),
            Some(e) => self.toast(&format!(
                "Recalled {}, {} line{} left out ({})",
                crate::store::file_name(p),
                bad.len(),
                if bad.len() == 1 { "" } else { "s" },
                e
            )),
        }
    }

    /// The text of the whole setup (the keys of the setup file and the look of the ref slots): what Save setup writes
    /// and `:SYSTem:SETup?` returns.
    pub(crate) fn setup_text_full(&mut self) -> String {
        self.sync_view_state();
        format!("{}{}{}", crate::store::setup_text(&self.m), self.ref_setup_text(), self.callout_setup_text())
    }

    /// Applies a setup text to the live settings, the way Recall setup does; the acquisition state stays as it is. A
    /// text with a line this build cannot take is refused as a whole (`:SYSTem:SETup`).
    pub(crate) fn apply_setup_text(&mut self, text: &str) -> Result<(), String> {
        let m = crate::store::parse_setup(text)?;
        self.apply_setup_model(m, text);
        Ok(())
    }

    /// The model of a read setup and the looks of the refs and callouts of its text, in place of the live settings.
    fn apply_setup_model(&mut self, mut m: Model, text: &str) {
        self.apply_ref_looks(&crate::store::parse_ref_looks(text, NREF));
        self.callouts = Self::callouts_from_setup(text);
        (m.run, m.status) = (self.m.run, self.m.status);
        // the text size is a preference of this screen, not part of a set-up
        m.text_size = self.m.text_size;
        if m.mask_src != self.m.mask_src {
            self.mask = None;
            self.mask_ref = None;
        }
        // the hardware session's stamps go on (#391)
        m.keep_session(&self.m);
        self.m = m;
        // the generator of a recalled set-up: its wave within what the generator can do, its output off. A recall
        // never switches an output on, and one that was on does not jump to another wave on a circuit
        self.m.afg_on = false;
        self.afg_clamp();
        self.restore_view_state();
        self.view_state_restore(text);
        if self.m.layout == Layout::Studio {
            self.studio.enter = true;
        }
        self.math_state.borrow_mut().forget_fit();
        self.m.order_trigger_levels();
        self.bus_auto = None;
        self.math_fam = if self.m.math == MathOp::Off { MathFamily::Arith } else { self.m.math.family() };
        self.sync_full();
        if self.mask.is_none() {
            self.m.mask_on = false;
        }
        self.build_mask();
        self.clamp_zoom();
        self.relayout();
        self.dirty_map = true;
        self.clear_measurement_statistics();
    }

    /// For the SCPI recorder: `line` is the command of what is about to be done, to be written as it is. With it goes
    /// the model as it is now, before the action: what the user set until then is written first, and what the action
    /// changes is its own. A client's command that comes here is taken out again (`Scpi::poll_program`).
    ///
    /// It is called where the action is done, past every check that can refuse it, and not where a key comes in: the
    /// Quick key, a row of the start menu and the key of the panel end in the same function, and a key that a dialog
    /// takes or a record refuses does nothing.
    pub(crate) fn rec_action(&mut self, line: &str) {
        if self.scpi_rec_want {
            self.scpi_rec_actions.push((line.to_string(), Box::new(self.m.clone())));
        }
    }

    /// Storage > Run SCPI file. While a file runs the row stops it. Else it asks which file: the desktop build with
    /// its Open dialog, the scope with the list of the `.scpi` files where Storage saves, the newest first, as Import
    /// slot lists the waveform files. The file is not run before one is chosen, also when there is only one.
    pub(super) fn scpi_run_ask(&mut self) {
        if self.scpi_replaying {
            self.scpi_replay_cancel = true;
            return;
        }
        if self.desktop_panel.is_some() {
            self.close_menu();
            self.open_scpi_request = true;
            return;
        }
        let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = std::fs::read_dir(self.save_dir())
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("scpi")))
            .map(|p| (p.metadata().and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH), p))
            .collect();
        files.sort_by(|a, b| b.cmp(a));
        self.scpi_files = files.into_iter().map(|x| x.1).collect();
        if self.scpi_files.is_empty() {
            self.toast("No SCPI file in storage");
        } else {
            self.open_picker(Item::StoreRunScpi);
        }
    }

    /// A new file `<prefix>-NNN.scpi` where Storage saves. The SCPI recorder takes its file at the start and writes
    /// it again after every step.
    pub(crate) fn scpi_new_file(&self, prefix: &str) -> std::path::PathBuf {
        self.new_file(&self.save_dir(), prefix, "scpi")
    }

    /// Writes a file of commands and says so, with `note` after the name: what no line could carry (Storage > Save
    /// setup as SCPI, to the file the user named, ui/files.rs), or how many commands were recorded.
    pub(crate) fn scpi_write(&mut self, path: &std::path::Path, text: &str, note: Option<&str>) {
        let r = crate::store::write(path, text.as_bytes());
        match (r.is_ok(), note) {
            (true, Some(note)) => {
                self.flush_stick();
                self.toast(&format!("Saved {}{}, {}", crate::store::file_name(path), self.saved_where(), note));
            }
            _ => self.toast_saved(path, r),
        }
    }
}
