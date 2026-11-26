// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `exec_cmd`: one command (a header and its argument) to its answer. It tries the headers in a fixed order: the
//! IEEE 488.2 common commands (`*IDN?`, `*OPC`, `*ESR?`, `*CLS`, `*RST`), then `:RUN`, `:STOP`, `:SINGle`,
//! `:TFORce`, `:AUToscale`, `:CLEar`, then the subsystems. `:CHANnel<n>`, `:TRIGger`, `:ACQuire`, `:WAVeform`'s
//! settings, `:VIEW`, `:CALibrate`, `:BODE`, `:SOURce` (the generator) and the analyser pages' queries are answered
//! here; every other subsystem hands its nodes to the `*_cmd` method in its own file (measure.rs, system.rs, ...).
//! The order is part of the behaviour: a header an earlier branch takes never reaches a later one. A new command of
//! a subsystem with its own file goes there; one of a subsystem answered here goes into its branch here. Either way
//! it comes with its line in docs/scpi.md and its query in ui/tests/scpi.txt (AGENTS.md).

use super::decode::bus_index;
use super::display::bmp;
use super::math::{math_index, unquote};
use super::replay;
use super::ser;
use super::{arg_item, arg_rest, block, e, kw, num, num64, parse_bool, parse_ch, parse_wave, suffix, t, Scpi};
use crate::board::{board, Board};
use crate::gfx::Canvas;
use crate::model::*;
use crate::ui::Ui;

/// `:RUN`, `:STOP` or `:SINGle`: they start and stop acquiring and change no setting.
fn run_state(nodes: &[&str]) -> bool {
    nodes.len() == 1 && ["RUN", "STOP", "SINGle"].iter().any(|c| kw(nodes[0], c))
}

impl Scpi {
    pub(super) fn exec_cmd(&mut self, ui: &mut Ui, cv: &Canvas, cmd: &str) -> Option<Vec<u8>> {
        if cmd.is_empty() {
            return None;
        }
        let (head, arg) = match cmd.find(char::is_whitespace) {
            Some(i) => (&cmd[..i], cmd[i..].trim()),
            None => (cmd, ""),
        };
        let query = head.ends_with('?');
        let head = head.trim_end_matches('?');
        let up = head.to_ascii_uppercase();
        // a command of a file (scpi/replay.rs) is not a client's: what it may not send is refused before any handler
        if self.from_file && !query {
            if let Some(why) = replay::file_refuses(head, arg) {
                self.push_err(format!("-203,\"Command protected\" {why}: {cmd}"));
                return None;
            }
        }
        // common commands
        if let Some(r) = self.common_cmd(ui, &up, query, arg) {
            return r;
        }
        let nodes: Vec<&str> = head.trim_start_matches(':').split(':').collect();
        // the set-up as commands (scpi/setupscpi.rs): it asks this interpreter for the value of every line. SETup is
        // spelled out, as for :SYSTem:SETup: the short form SET is the key = value command
        if nodes.len() == 3 && kw(nodes[0], "SYSTem") && nodes[1].eq_ignore_ascii_case("SETup") && kw(nodes[2], "SCPI")
        {
            if query {
                return Some(block(self.setup_scpi(ui, cv).as_bytes()));
            }
            self.push_err(format!("-113,\"Undefined header\" {}", cmd));
            return None;
        }
        // the SCPI recorder (scpi/recorder.rs): the switch is the one of Storage > SCPI recorder, taken at the next frame
        if nodes.len() == 3 && kw(nodes[0], "SYSTem") && kw(nodes[1], "RECorder") && kw(nodes[2], "STATe") {
            if query {
                return t(if ui.scpi_rec_want { "1" } else { "0" });
            }
            match parse_bool(arg) {
                Some(on) => ui.scpi_rec_want = on,
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return None;
        }
        if query && nodes.len() == 3 && kw(nodes[0], "SYSTem") && kw(nodes[1], "RECorder") && kw(nodes[2], "DATA") {
            let lines = self.rec_lines();
            let text = if lines.is_empty() { String::new() } else { lines.join("\n") + "\n" };
            return Some(block(text.as_bytes()));
        }
        // the coverage extensions (ui/scpiext*.rs): same model and menu code paths as the screen
        if let Some(r) = ui.scpi_ext_command(&nodes, query, arg) {
            return match r {
                Ok(v) => v,
                Err(msg) => {
                    self.push_err(msg);
                    None
                }
            };
        }
        if let Some(r) = self.run_state_cmd(ui, &nodes, query, arg) {
            return r;
        }
        if let Some(r) = self.trigger_source_cmd(ui, &nodes, query, arg) {
            return r;
        }
        if !query
            && (nodes.len() == 1 && kw(nodes[0], "SINGle")
                || nodes.len() == 2
                    && kw(nodes[0], "TRIGger")
                    && kw(nodes[1], "SWEep")
                    && arg.to_ascii_uppercase().starts_with("SING")
                    && ui.m.run == RunState::Run)
        {
            ui.scpi_arm_operation();
        }
        if nodes.first().is_some_and(|n| kw(n, "TIMebase")) {
            return self.timebase_cmd(ui, &nodes, query, arg);
        }
        if !query
            && nodes.len() == 3
            && kw(nodes[0], "TRIGger")
            && kw(nodes[1], "EDGE")
            && nodes.last().is_some_and(|l| kw(l, "LEVel"))
        {
            if let Some(number) = num(arg) {
                ui.scpi_set_trigger_level(false, number);
                ui.mark_changed();
                return None;
            }
        }
        // read before the settings are borrowed for writing: which channels clip now (:CHANnel<n>:CLIPping?)
        let clipping: Vec<bool> = (0..NCH).map(|c| ui.channel_clipping(c)).collect();
        // a value `model_cmd` refused queued its error and set nothing, so nothing is marked changed
        let errors = self.error_serial;
        let r = self.model_cmd(&mut ui.m, &ui.src, &clipping, &nodes, query, arg);
        if let Some(resp) = r {
            if !query && run_state(&nodes) {
                ui.mark_run_state();
            } else if !query && self.error_serial == errors {
                ui.mark_changed();
            }
            return resp;
        }
        // measurement, waveform, display, system (need the record / canvas)
        let n0 = nodes.first().copied().unwrap_or("");
        if kw(n0, "TRIGger") && nodes.len() == 2 && kw(nodes[1], "MODE") {
            return self.trigger_mode(ui, query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "PULSe") {
            return self.trigger_pulse(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "SLOPe") {
            return self.trigger_slope(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "VIDeo") {
            return self.trigger_video(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "PATTern") {
            return self.trigger_pattern(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "DURation") {
            return self.trigger_duration(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "TIMeout") {
            return self.trigger_timeout(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "RUNT") {
            return self.trigger_runt(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && (kw(nodes[1], "WINDows") || kw(nodes[1], "WINDow")) {
            return self.trigger_window(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "DELay") {
            return self.trigger_delay(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && (kw(nodes[1], "SHOLd") || kw(nodes[1], "SETup")) {
            return self.trigger_setup(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(nodes[1], "NEDGe") {
            return self.trigger_nth(ui, nodes[2], query, arg);
        }
        if kw(n0, "TRIGger") && nodes.len() == 3 && kw(suffix(nodes[1]).0, "ZONE") {
            return self.trigger_zone(ui, suffix(nodes[1]).1, nodes[2], query, arg);
        }
        if kw(n0, "SEARch") {
            return self.search_cmd(ui, &nodes[1..], query, arg);
        }
        if kw(n0, "RECord") {
            return self.record_cmd(ui, &nodes[1..], query, arg);
        }
        if kw(n0, "NAVigate") {
            return self.navigate_cmd(ui, &nodes[1..], query, arg);
        }
        let (channel, channel_n) = suffix(n0);
        if kw(channel, "CHANnel") && nodes.len() == 2 && kw(nodes[1], "ZERO") && !query {
            let Some(c) = channel_n.and_then(|n| n.checked_sub(1)).filter(|&c| c < NCH) else {
                self.push_err(format!("-113,\"Undefined header\" {}", cmd));
                return None;
            };
            if ui.zero_channel(c) {
                ui.mark_changed();
            } else {
                self.push_err("-230,\"Data corrupt or stale\" channel has no waveform".into());
            }
            return None;
        }
        if kw(n0, "MEASure") && nodes.len() == 3 && kw(nodes[1], "STRip") && kw(nodes[2], "DOCK") {
            if query {
                return t(match ui.m.strip_dock {
                    StripDock::Below => "UNDER",
                    StripDock::Left => "LEFT",
                    StripDock::Right => "RIGHT",
                    StripDock::Above => "ABOVE",
                    StripDock::Hidden => "HIDDEN",
                });
            }
            ui.m.strip_dock = if kw(arg, "UNDer") {
                StripDock::Below
            } else if kw(arg, "LEFT") {
                StripDock::Left
            } else if kw(arg, "RIGHt") {
                StripDock::Right
            } else if kw(arg, "ABOVe") {
                StripDock::Above
            } else if kw(arg, "HIDDen") {
                StripDock::Hidden
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.mark_changed();
            return None;
        }
        if kw(n0, "MEASure") && nodes.len() >= 2 && kw(nodes[1], "COUNter") {
            return self.counter_cmd(ui, &nodes[2..], query, arg);
        }
        // oos-plus: the amplitude method, the threshold type and the shared reference levels of the measurements
        if kw(n0, "MEASure") && nodes.len() >= 3 && kw(nodes[1], "AMP") {
            return self.meas_amp(&nodes[2..], query, arg);
        }
        if kw(n0, "MEASure") && nodes.len() == 3 && kw(nodes[1], "THReshold") {
            let which = nodes[2];
            if kw(which, "TYPE") {
                if query {
                    return t("PERC");
                }
                if kw(arg, "PERCent") {
                    return None;
                }
                self.push_err("-224,\"Illegal parameter value\" percent thresholds".into());
                return None;
            }
            if kw(which, "DEFault") && !query {
                ui.m.meas_lpct = 0.1;
                ui.m.meas_hpct = 0.9;
                ui.measure_last();
                ui.mark_changed();
                return None;
            }
            self.push_err(format!("-113,\"Undefined header\" :MEASure:THReshold:{}", which));
            return None;
        }
        if kw(n0, "MEASure") && nodes.len() >= 3 && kw(nodes[1], "LEVel") {
            return self.meas_level_cmd(ui, &nodes[2..], query, arg);
        }
        if kw(n0, "MEASure") {
            return self.measure_cmd(ui, &nodes, query, arg, cmd);
        }
        if let Some(r) = self.analyser_item_cmd(ui, &nodes, query, arg) {
            return r;
        }
        if let Some(r) = self.waveform_cmd(ui, &nodes, query, arg) {
            return r;
        }
        if let Some(r) = self.view_cmd(ui, cv, &nodes, query, arg) {
            return r;
        }
        if suffix(n0).1.is_none_or(|n| (1..=gen_count().max(1)).contains(&n))
            && kw(suffix(n0).0, "SOURce")
            && nodes.len() >= 2
        {
            if let Some(resp) = self.source_cmd(ui, cv, &nodes, query, arg, cmd) {
                return resp;
            }
        }
        // :SAVE:MEMory:WAVeform ["path"[,CSV|BIN]]: the acquisition memory, where :SAVE:WAVeform writes the screen
        if !query && nodes.len() == 3 && kw(n0, "SAVE") && kw(nodes[1], "MEMory") && kw(nodes[2], "WAVeform") {
            let mut parts = arg.split(',');
            let name = parts.next().map(str::trim).filter(|p| !p.is_empty());
            let csv = match parts.next().map(str::trim) {
                None | Some("") => name.is_some_and(|p| unquote(p).to_ascii_lowercase().ends_with(".csv")),
                Some(f) if kw(f, "CSV") => true,
                Some(f) if kw(f, "BIN") => false,
                Some(f) => {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", f));
                    return None;
                }
            };
            let path = name.map(|p| ui.out_path(p));
            if let Err(error) = ui.store_wave_memory(csv, path) {
                self.push_err(error);
            }
            return None;
        }
        // :MMEMory: the files of the save folder, so that what was saved on the scope can be fetched (SCPI-99 names)
        if kw(n0, "MMEMory") && nodes.len() == 2 {
            if kw(nodes[1], "CATalog") && query {
                return t(&ui.mmem_catalog());
            }
            if kw(nodes[1], "DATA") && query {
                return match ui.mmem_data(arg) {
                    Ok(data) => Some(block(&data)),
                    Err(error) => {
                        self.push_err(error);
                        None
                    }
                };
            }
            if kw(nodes[1], "DELete") && !query {
                if let Err(error) = ui.mmem_delete(arg) {
                    self.push_err(error);
                }
                return None;
            }
        }
        if kw(n0, "SYSTem") {
            return self.system_cmd(ui, &nodes, query, arg, cmd);
        }
        if kw(n0, "ACQuire") && nodes.len() == 3 && kw(nodes[1], "SEGMents") && kw(nodes[2], "TIMes") && query {
            return t(&ui.seg_times_text());
        }
        if kw(n0, "DECode") && nodes.len() == 2 && kw(nodes[1], "EVENts") && query {
            return t(&ui.events_text());
        }
        if kw(n0, "DECode") && nodes.len() == 3 && kw(nodes[1], "EVENts") && kw(nodes[2], "COUNt") && query {
            return match ui.events_total() {
                Ok(n) => t(&n.to_string()),
                Err("Decode computing") => t("BUSY"),
                Err(error) => {
                    self.push_err(format!("-230,\"{}\"", error));
                    None
                }
            };
        }
        // what the Events pane lists (the query above always answers a row per field)
        if kw(n0, "DECode") && nodes.len() == 3 && kw(nodes[1], "EVENts") && kw(nodes[2], "VIEW") {
            if query {
                return t(&ui.m.ev_view.name().to_ascii_uppercase());
            }
            match EV_VIEWS.into_iter().find(|v| kw(arg, v.scpi())) {
                Some(v) => {
                    ui.m.ev_view = v;
                    ui.mark_changed();
                }
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return None;
        }
        if kw(n0, "DECode") && nodes.len() == 3 && kw(nodes[1], "EVENts") && kw(nodes[2], "ERRors") {
            if query {
                return t(if ui.m.ev_errors { "1" } else { "0" });
            }
            match parse_bool(arg) {
                Some(on) => {
                    ui.m.ev_errors = on;
                    ui.mark_changed();
                }
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return None;
        }
        if kw(n0, "CURSor") {
            return self.cursor_cmd(ui, &nodes[1..], query, arg);
        }
        if kw(n0, "MASK") {
            return self.mask_cmd(ui, &nodes[1..], query, arg);
        }
        if kw(n0, "HISTogram") {
            return self.hist_cmd(ui, &nodes[1..], query, arg);
        }
        if let Some(k) = math_index(n0) {
            return self.math_cmd(ui, k, &nodes[1..], query, arg);
        }
        if let Some(k) = bus_index(n0) {
            return self.bus_cmd(ui, k, &nodes[1..], query, arg);
        }
        if kw(n0, "DVM") {
            return self.dvm_cmd(ui, &nodes[1..], query, arg);
        }
        if kw(n0, "COUNter") {
            return self.counter_cmd(ui, &nodes[1..], query, arg);
        }
        if kw(n0, "TABLe") {
            return self.table_cmd(ui, &nodes[1..], query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" {}", cmd));
        None
    }

    /// The IEEE 488.2 common commands (`*IDN?`, `*OPC`, `*ESR?`, `*CLS`, `*RST` and the others), by the header in
    /// capitals `up`. None when `up` is none of them (`exec_cmd` goes on), else the answer.
    fn common_cmd(&mut self, ui: &mut Ui, up: &str, query: bool, arg: &str) -> Option<Option<Vec<u8>>> {
        match up {
            "*IDN" => {
                let p = oos_hal::model::platform();
                return Some(t(&format!(
                    "OOS,{},{},oos-ui {}",
                    p.caps.name,
                    p.serial.as_deref().unwrap_or("0"),
                    env!("CARGO_PKG_VERSION")
                )));
            }
            "*OPC" if !query => {
                self.poll_events(ui);
                if self.opc_events.len() < 64 {
                    self.opc_events.push(ui.scpi_operations());
                } else {
                    self.push_err("-350,\"Too many pending OPC events\"".into());
                }
                self.poll_events(ui);
                return Some(None);
            }
            "*ESR" if query => {
                self.poll_events(ui);
                let status = std::mem::take(&mut self.event_status);
                return Some(t(&status.to_string()));
            }
            "*CLS" if !query => {
                self.err.clear();
                self.event_status = 0;
                self.opc_events.clear();
                return Some(None);
            }
            // IEEE 488.2 status. A socket has no service request line, so *SRE only shapes the MSS bit of *STB?.
            "*STB" if query => {
                self.poll_events(ui);
                let mut status = 0u8;
                if !self.err.is_empty() {
                    status |= 4;
                }
                if self.event_status & self.event_enable != 0 {
                    status |= 32;
                }
                if status & self.service_enable != 0 {
                    status |= 64;
                }
                return Some(t(&status.to_string()));
            }
            "*ESE" | "*SRE" => {
                let service = up == "*SRE";
                if query {
                    return Some(t(&(if service { self.service_enable } else { self.event_enable }).to_string()));
                }
                match arg.trim().parse::<u16>().ok().filter(|v| *v <= 255) {
                    // bit 6 of the service request enable register does not exist
                    Some(v) if service => self.service_enable = v as u8 & !64,
                    Some(v) => self.event_enable = v as u8,
                    None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
                }
                return Some(None);
            }
            "*PSC" => {
                if query {
                    return Some(t(if self.power_on_clear { "1" } else { "0" }));
                }
                match arg.trim().parse::<f64>().ok().filter(|v| v.is_finite()) {
                    Some(v) => self.power_on_clear = v != 0.0,
                    None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
                }
                return Some(None);
            }
            // no self test: the answer IEEE 488.2 gives for "passed"; the options are the unit's (`Platform::options`),
            // 0 where it has none
            "*TST" if query => return Some(t("0")),
            "*OPT" if query => return Some(t(&options_text())),
            // every command is finished when the next one is read; an armed Single is waited for with *OPC?
            "*WAI" if !query => return Some(None),
            "*TRG" if !query => {
                ui.force_trigger();
                ui.mark_changed();
                return Some(None);
            }
            "*RST" if !query => {
                self.event_status = 0;
                self.opc_events.clear();
                // not as a key press: a notice on the screen takes a key for its own, and the reset was lost
                ui.sim_cancel_generation();
                ui.default_setup();
                // the waveform transfer too: a page or source left by an earlier read would cut or redirect the next
                (self.wav_src, self.wav_fmt, self.wav_mode, self.wav_start, self.wav_stop) = super::WAV_DEFAULT;
                self.wav_math = None;
                return Some(None);
            }
            _ => {}
        }
        None
    }

    /// The commands of a run: `:RUN`, `:STOP`, `:SINGle`, `:TFORce`, `:AUToscale`, `:CLEar` and the ones that go to the
    /// acquisition router, the decoder's and the LA's. None when no branch takes the nodes (`exec_cmd` goes on), else
    /// the answer.
    fn run_state_cmd(&mut self, ui: &mut Ui, nodes: &[&str], query: bool, arg: &str) -> Option<Option<Vec<u8>>> {
        if !query && nodes.len() == 1 {
            let command = if kw(nodes[0], "STOP") {
                Some(0)
            } else if kw(nodes[0], "RUN") {
                Some(1)
            } else if kw(nodes[0], "SINGle") {
                Some(2)
            } else {
                None
            };
            if command.is_some_and(|command| ui.mixed_bus_command(command)) {
                return Some(None);
            }
        }
        // Rigol: a forced trigger releases Normal or Single when the level never crosses. On a reference or SIM
        // session that is the replay flag (`force_trigger`); on the live hardware it pulses the FPGA force bit.
        if !query
            && ((nodes.len() == 1 && kw(nodes[0], "TFORce"))
                || (nodes.len() == 2 && kw(nodes[0], "TRIGger") && kw(nodes[1], "FORCe")))
        {
            ui.force_trigger();
            return Some(None);
        }
        if nodes.len() == 3
            && kw(nodes[0], "DECode")
            && kw(nodes[1], "CPHY")
            && (kw(nodes[2], "RATE") || kw(nodes[2], "SRATe"))
        {
            return Some(self.decode_cphy_rate_cmd(ui, query, arg));
        }
        if nodes.len() == 2 && kw(nodes[0], "DECode") {
            return Some(self.decode_cmd(ui, nodes[1], query, arg));
        }
        if nodes.len() >= 2 && kw(nodes[0], "LA") {
            return Some(self.la_cmd(ui, &nodes[1..], query, arg));
        }
        // fork debug: raw measure-unit slots (the slot map of the auto set-up's levels)
        if nodes.len() == 2 && kw(nodes[0], "DEBug") && kw(nodes[1], "SLOTs") && query {
            return Some(t(&ui.debug_slots()));
        }
        if nodes.len() == 1 && kw(nodes[0], "AUToscale") && query {
            return Some(t(if ui.auto_running() { "1" } else { "0" }));
        }
        if nodes.len() == 1 && kw(nodes[0], "AUToscale") {
            ui.key(crate::ui::key::AUTO);
            return Some(None);
        }
        if !query && nodes.len() == 1 && kw(nodes[0], "CLEar") {
            ui.key(crate::ui::key::CLEAR);
            return Some(None);
        }
        if !query && nodes.len() == 1 && kw(nodes[0], "STOP") && ui.sim_generating() {
            let _ = ui.sim_play_set(false);
            ui.mark_run_state();
            return Some(None);
        }
        // A reference capture uses the same acquisition state as the screen.
        // RUN and STOP are assignments, so repeating either must not toggle it.
        // Call the shared record handler directly: an open UI dialog must not
        // consume a remote acquisition command as a dialog-dismissal key.
        if !query
            && nodes.len() == 1
            && ui.record_mode()
            && ["RUN", "STOP", "SINGle"].iter().any(|command| kw(nodes[0], command))
        {
            let key = crate::ui::key::RUNSTOP;
            if kw(nodes[0], "RUN") {
                if ui.m.trig_mode == TrigMode::Single {
                    ui.m.trig_mode = TrigMode::Normal;
                }
                if ui.m.run != RunState::Run {
                    ui.record_key(key);
                }
            } else if kw(nodes[0], "STOP") {
                if ui.m.run == RunState::Run {
                    ui.record_key(key);
                } else {
                    let _ = ui.sim_play_set(false);
                }
            } else {
                ui.record_key(crate::ui::key::SINGLE);
            }
            ui.mark_run_state();
            return Some(None);
        }
        None
    }

    /// The trigger source (an LA line, a reference) and the thresholds, through the acquisition router. None when no
    /// branch takes the nodes (`exec_cmd` goes on), else the answer.
    fn trigger_source_cmd(&mut self, ui: &mut Ui, nodes: &[&str], query: bool, arg: &str) -> Option<Option<Vec<u8>>> {
        // Reference trigger identities belong to the UI acquisition router, not
        // a silent physical-channel alias inside Model::trigger().
        if nodes.first().is_some_and(|node| kw(node, "TRIGger")) {
            let last = nodes.last().copied().unwrap_or("");
            let (leaf, source_number) = suffix(last);
            // a zone has a source of its own (`trigger_zone`): `:TRIGger:ZONE<n>:SOURce` is not the trigger's
            let zone = nodes.len() == 3 && kw(suffix(nodes[1]).0, "ZONE");
            if (nodes.len() == 2 || nodes.len() == 3) && kw(leaf, "SOURce") && !zone {
                let secondary = source_number == Some(2);
                if query {
                    if !secondary && ui.m.trig_ref.is_none() {
                        if let Some(line) = ui.m.trig_line() {
                            return Some(t(&format!("D{line}")));
                        }
                    }
                    let id = if secondary { ui.m.trigger_wave2() } else { ui.m.trigger_wave() };
                    return Some(t(&match id {
                        WaveId::Ref(i) => format!("R{}", i + 1),
                        WaveId::Ch(c) => format!("CHAN{}", c + 1),
                    }));
                }
                if let Some(id) = parse_wave(arg) {
                    if !ui.set_trigger_source(id, secondary) {
                        self.push_err("-224,\"Trigger source is unavailable\"".into());
                    }
                    ui.mark_changed();
                    return Some(None);
                }
                // Leave D0..D15 of :TRIGger:EDGE:SOURce and :TRIGger:PULSe:SOURce to the LA source handlers; no other
                // source takes one.
                let edge = nodes.len() == 3 && (kw(nodes[1], "EDGE") || kw(nodes[1], "PULSe"));
                if !edge || !arg.trim().to_ascii_uppercase().starts_with('D') {
                    self.push_err("-224,\"Invalid trigger source\"".into());
                    return Some(None);
                }
            }
            if nodes.len() == 2 && (kw(last, "TYPe") || kw(last, "MODE")) {
                if query {
                    return Some(t(if kw(last, "MODE") {
                        match ui.m.trig_type {
                            TrigType::Edge => "EDGE",
                            TrigType::Pulse => "PULS",
                            TrigType::Slope => "SLOP",
                            TrigType::Video => "VID",
                            TrigType::Pattern => "PATT",
                            TrigType::Duration => "DUR",
                            TrigType::Timeout => "TIM",
                            TrigType::Runt => "RUNT",
                            TrigType::Window => "WIND",
                            TrigType::Delay => "DEL",
                            TrigType::SetupHold => "SET",
                            TrigType::NthEdge => "NEDG",
                            TrigType::Bus => "BUS",
                        }
                    } else {
                        ui.m.trig_type.name()
                    }));
                }
                let name = arg.trim().to_ascii_uppercase();
                let kind = match name.as_str() {
                    "EDGE" => Some(TrigType::Edge),
                    "PULSE" | "PULS" => Some(TrigType::Pulse),
                    "SLOPE" | "SLOP" => Some(TrigType::Slope),
                    "VIDEO" | "VID" => Some(TrigType::Video),
                    "PATTERN" | "PATT" => Some(TrigType::Pattern),
                    "DURATION" | "DUR" => Some(TrigType::Duration),
                    "TIMEOUT" | "TIM" => Some(TrigType::Timeout),
                    "RUNT" => Some(TrigType::Runt),
                    "WINDOW" | "WIND" => Some(TrigType::Window),
                    "DELAY" | "DEL" => Some(TrigType::Delay),
                    "SETUPHOLD" | "SETUP" | "SET" => Some(TrigType::SetupHold),
                    "NTHEDGE" | "NTH" | "NEDGE" | "NEDG" => Some(TrigType::NthEdge),
                    "BUS" => Some(TrigType::Bus),
                    _ => None,
                };
                if let Some(kind) = kind {
                    ui.trigger_type_set(kind);
                    ui.mark_changed();
                } else {
                    self.push_err("-224,\"Invalid trigger type\"".into());
                }
                return Some(None);
            }
            if nodes.len() == 3 && kw(nodes[1], "RUNT") {
                let secondary = if kw(last, "UPPer") || kw(last, "ALEVel") {
                    Some(false)
                } else if kw(last, "LOWer") || kw(last, "BLEVel") {
                    Some(true)
                } else {
                    None
                };
                if let Some(secondary) = secondary {
                    if query {
                        return Some(t(&e(ui.scpi_trigger_level(secondary) as f64)));
                    }
                    let (lower, higher) = (ui.scpi_trigger_level(true), ui.scpi_trigger_level(false));
                    // the pair stays in order, as :MEASure:SETup:MAX|MIN: a threshold past the other is refused
                    match num(arg) {
                        Some(number) if (!secondary && number < lower) || (secondary && number > higher) => {
                            let other = if !secondary { "below the lower" } else { "above the upper" };
                            self.push_err(format!("-224,\"Illegal parameter value\" {other} threshold"));
                        }
                        Some(number) => {
                            ui.scpi_set_trigger_level(secondary, number);
                            ui.mark_changed();
                        }
                        None => self.push_err("-224,\"Invalid runt threshold\"".into()),
                    }
                    return Some(None);
                }
            }
        }
        None
    }

    /// The analyser pages' queries: `:EYE:ITEM?`, `:POWer:*?`, `:SPECtrum:ITEM?` and the others. None when no branch
    /// takes the nodes (`exec_cmd` goes on), else the answer.
    fn analyser_item_cmd(&mut self, ui: &mut Ui, nodes: &[&str], query: bool, arg: &str) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        if kw(n0, "EYE") && nodes.len() == 2 && kw(nodes[1], "ITEM") && query {
            return Some(self.eye_item(ui, arg));
        }
        if kw(n0, "POWer") && nodes.len() == 2 && query {
            if kw(nodes[1], "HARMonics") {
                let src = match self.waves(arg) {
                    Ok(v) => v,
                    Err(s) => {
                        self.push_err(s);
                        return Some(t("9.9E37"));
                    }
                };
                return Some(match ui.fact_harmonics(&src) {
                    Ok(h) if h.is_empty() => t("9.9E37"),
                    Ok(h) => t(&h.iter().map(|(n, a)| format!("{},{}", n, e(*a))).collect::<Vec<_>>().join(";")),
                    Err(()) => {
                        self.push_err("-224,\"Illegal parameter value\"".into());
                        t("9.9E37")
                    }
                });
            }
            if kw(nodes[1], "ITEM") {
                return Some(self.sourced(ui, arg, |ui, item, src| ui.fact_power(item, src)));
            }
        }
        if kw(n0, "SPECtrum") && nodes.len() == 2 && kw(nodes[1], "ITEM") && query {
            return Some(self.sourced(ui, arg, |ui, item, src| ui.fact_spectrum(item, src)));
        }
        // the RBW the spectrum was computed with (the record length can make it coarser than `:SPECtrum:RBW`)
        if kw(n0, "SPECtrum") && nodes.len() == 3 && kw(nodes[1], "RBW") && kw(nodes[2], "ACTual") && query {
            return Some(t(&ui.spectrum_rbw_now().filter(|r| *r > 0.0).map_or_else(|| "9.9E37".to_string(), e)));
        }
        // the lines of the spectrum in the order of the peak table, up to the Harmonics setting: frequency,level;...
        if kw(n0, "SPECtrum") && nodes.len() == 3 && kw(nodes[1], "PEAK") && kw(nodes[2], "LIST") && query {
            let p = ui.spectrum_lines_now().unwrap_or_default();
            return Some(if p.is_empty() {
                t("9.9E37")
            } else {
                t(&p.iter().map(|m| format!("{},{}", e(m.f), e(m.dbv))).collect::<Vec<_>>().join(";"))
            });
        }
        if kw(n0, "MAGnetic") && nodes.len() == 2 && kw(nodes[1], "ITEM") && query {
            return Some(self.sourced(ui, arg, |ui, item, src| ui.fact_magnetic(item, src)));
        }
        if kw(n0, "INVerter") && nodes.len() == 2 && kw(nodes[1], "ITEM") && query {
            let src = match self.waves(arg_rest(arg)) {
                Ok(v) => v,
                Err(s) => {
                    self.push_err(s);
                    return Some(t("9.9E37"));
                }
            };
            return Some(match ui.fact_inverter(&arg_item(arg), &src) {
                Ok(crate::ui::facts::Fact::Word(w)) => t(w),
                Ok(crate::ui::facts::Fact::Num(v)) => t(&e(v)),
                Err(()) => {
                    self.push_err("-224,\"Illegal parameter value\"".into());
                    t("9.9E37")
                }
            });
        }
        if kw(n0, "DPT") && nodes.len() == 2 && kw(nodes[1], "ITEM") && query {
            return Some(self.sourced(ui, arg, |ui, item, src| ui.fact_dpt(item, src)));
        }
        if kw(n0, "BANDwidth") && nodes.len() == 2 && kw(nodes[1], "ITEM") && query {
            return Some(self.sourced(ui, arg, |ui, item, src| ui.fact_bandwidth(item, src)));
        }
        None
    }

    /// `:WAVeform:*`: the source, the mode, the format, the page, the preamble and the data. None when no branch takes
    /// the nodes (`exec_cmd` goes on), else the answer.
    fn waveform_cmd(&mut self, ui: &mut Ui, nodes: &[&str], query: bool, arg: &str) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        if kw(n0, "WAVeform") && nodes.len() == 2 {
            let n1 = nodes[1];
            if kw(n1, "SOURce") {
                if query {
                    return Some(t(&self.wav_source_text()));
                }
                if let Some(id) = parse_wave(arg) {
                    self.wav_src = id;
                    self.wav_math = None;
                } else if let Some(k) = math_index(arg.trim()) {
                    self.wav_math = Some(k);
                } else if !arg.is_empty() {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                }
                return Some(None);
            }
            if kw(n1, "MODE") {
                if query {
                    return Some(t(["NORM", "MAX", "RAW"][self.wav_mode.min(2) as usize]));
                }
                let a = arg.trim();
                self.wav_mode = if kw(a, "RAW") {
                    2
                } else if kw(a, "MAXimum") {
                    1
                } else if kw(a, "NORMal") {
                    0
                } else {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    return Some(None);
                };
                return Some(None);
            }
            if kw(n1, "FORMat") {
                if query {
                    return Some(t(["BYTE", "WORD", "ASC"][self.wav_fmt as usize]));
                }
                let format = if kw(arg.trim(), "BYTE") {
                    Some(0)
                } else if kw(arg.trim(), "WORD") {
                    Some(1)
                } else if kw(arg.trim(), "ASCii") {
                    Some(2)
                } else {
                    None
                };
                if let Some(format) = format {
                    self.wav_fmt = format;
                } else {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                }
                return Some(None);
            }
            if kw(n1, "STARt") || kw(n1, "STOP") {
                let stop = kw(n1, "STOP");
                if query {
                    let n = self.wave_len(ui);
                    // a stop beyond the record reads through its last point, and the query says so
                    let v = if stop {
                        if self.wav_stop == 0 || (n > 0 && self.wav_stop > n) {
                            n
                        } else {
                            self.wav_stop
                        }
                    } else {
                        self.wav_start
                    };
                    return Some(t(&v.to_string()));
                }
                let Some(p) = arg.trim().parse::<u64>().ok().filter(|&p| p >= 1) else {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    return Some(None);
                };
                if stop {
                    self.wav_stop = p;
                    if p < self.wav_start {
                        self.wav_start = p;
                    }
                } else {
                    self.wav_start = p;
                    if self.wav_stop != 0 && self.wav_stop < p {
                        self.wav_stop = p;
                    }
                }
                return Some(None);
            }
            if kw(n1, "POINts") {
                if query {
                    return Some(t(&self.page_count(ui).to_string()));
                }
                // Rigol: the count to read, starting at STARt. stop 0 still means "the rest" until this sets it.
                let Some(n) = arg.trim().parse::<u64>().ok().filter(|&n| n >= 1) else {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    return Some(None);
                };
                self.wav_stop = self.wav_start.saturating_add(n - 1);
                return Some(None);
            }
            if kw(n1, "PREamble") && query {
                return Some(t(&self.wave_preamble(ui)));
            }
            // the scaling fields of the preamble one by one, as the stock firmware has them
            let field = ["XINCrement", "XORigin", "XREFerence", "YINCrement", "YORigin", "YREFerence"]
                .iter()
                .position(|f| kw(n1, f));
            if let Some(k) = field.filter(|_| query) {
                return Some(t(self.wave_preamble(ui).split(',').nth(4 + k).unwrap_or("0")));
            }
            if kw(n1, "DATA") && query {
                return Some(self.wave_data(ui));
            }
        }
        None
    }

    /// `:EYE:STATe`, the Eye/Jitter analyser on or off (the same as `:VIEW EYE` and `:VIEW SCOPe`).
    fn eye_state(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            return t(if ui.m.view == AView::Eye { "1" } else { "0" });
        }
        match parse_bool(arg) {
            Some(on) => ui.set_analysis_view(if on { AView::Eye } else { AView::Scope }),
            None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
        }
        None
    }

    /// `:REFerence`, `:SIM`, `:DISPlay`, `:VIEW`, `:IDENtify?`, `:CALibrate` and `:BODE`. None when no branch takes the
    /// nodes (`exec_cmd` goes on), else the answer.
    fn view_cmd(
        &mut self,
        ui: &mut Ui,
        cv: &Canvas,
        nodes: &[&str],
        query: bool,
        arg: &str,
    ) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        if kw(n0, "REFerence") {
            return Some(self.ref_cmd(ui, &nodes[1..], query, arg));
        }
        if kw(n0, "SIM") {
            return Some(self.sim_cmd(ui, &nodes[1..], query, arg));
        }
        // :DISPlay:GENerator ON|OFF: whether the GEN chip and card are shown in the view shown (the output is the scope's)
        if kw(n0, "DISPlay") && nodes.len() == 2 && kw(nodes[1], "GENerator") {
            if query {
                return Some(t(if ui.m.gen_hidden { "0" } else { "1" }));
            }
            match parse_bool(arg) {
                Some(on) => {
                    ui.m.gen_hidden = !on;
                    ui.mark_changed();
                }
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        if kw(n0, "DISPlay")
            && !(nodes.len() == 2 && kw(nodes[1], "DATA"))
            && !(nodes.len() >= 2 && kw(nodes[1], "VIEW"))
        {
            return Some(self.display_cmd(ui, &nodes[1..], query, arg));
        }
        if kw(n0, "DISPlay") && nodes.len() == 2 && kw(nodes[1], "DATA") && query {
            let kind = arg.split_whitespace().next().unwrap_or("").trim();
            let up = kind.to_ascii_uppercase();
            if up.starts_with("PNG") {
                return Some(Some(block(&crate::store::png_of(cv))));
            }
            if up.is_empty() || up.starts_with("BMP") {
                return Some(Some(block(&bmp(cv))));
            }
            self.push_err(format!("-224,\"Illegal parameter value\" {}", kind));
            return Some(None);
        }
        // Display > Views (ui/views.rs): :DISPlay:VIEW <n> shows view n, :DISPlay:VIEW:COUNt <1..4> how many there are
        if kw(n0, "DISPlay") && nodes.len() >= 2 && kw(nodes[1], "VIEW") {
            let count = nodes.len() == 3 && kw(nodes[2], "COUNt");
            // Display > Monitor view (ui/monitor.rs): the view an HDMI monitor in Layout shows, 0 the panel's
            if nodes.len() == 3 && kw(nodes[2], "MONitor") {
                if query {
                    return Some(t(&ui.m.hdmi_view.to_string()));
                }
                match arg.trim().parse::<usize>().ok() {
                    Some(n) if n <= ui.m.views && (n == 0 || ui.m.views >= 2) => ui.m.hdmi_view = n,
                    _ => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
                }
                return Some(None);
            }
            if nodes.len() > 3 || (nodes.len() == 3 && !count) {
                self.push_err("-113,\"Undefined header\"".into());
                return Some(None);
            }
            if query {
                return Some(t(&(if count { ui.m.views } else { ui.m.view_cur + 1 }).to_string()));
            }
            let n = arg.trim().parse::<usize>().ok();
            match n {
                Some(n) if count && (1..=crate::ui::VIEWS_MAX).contains(&n) => ui.views_set(n),
                Some(n) if !count && (1..=ui.m.views).contains(&n) => ui.view_go(n - 1),
                _ => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        if kw(n0, "EYE") && nodes.len() == 2 && kw(nodes[1], "STATe") {
            return Some(self.eye_state(ui, query, arg));
        }
        // OOS extensions: analyser view, Identify, self-calibration, Bode
        if kw(n0, "VIEW") && nodes.len() == 1 {
            // the analysers' own keywords, and the fork's bandwidth test (BWTest), which the registry keeps off :VIEW
            let mut views = crate::ui::scpi_views();
            if !views.iter().any(|(_, v)| *v == AView::Step) {
                views.push(("BWTest", AView::Step));
            }
            if query {
                return Some(t(&if ui.m.view == AView::Step {
                    "BWTEST".to_string()
                } else {
                    format!("{:?}", ui.m.view).to_ascii_uppercase()
                }));
            }
            match views.iter().find(|(k, _)| kw(arg, k)) {
                Some((_, v)) => ui.set_analysis_view(*v),
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        if kw(n0, "IDENtify") && query {
            return Some(t(&ui.identify_text()));
        }
        if kw(n0, "CALibrate") && nodes.len() == 2 {
            if kw(nodes[1], "STATus") && query {
                return Some(t(&ui.cal_status_text()));
            }
            // :CALibrate:STARt 0..4[,ZERO]   (0 = all channels; the inputs must be disconnected)
            if kw(nodes[1], "STARt") && !query {
                let mut it = arg.split(',');
                let ch = it.next().unwrap_or("0").trim();
                let rest: Vec<&str> = it.map(str::trim).collect();
                ui.selfcal_remote_full(ch, rest.iter().any(|z| kw(z, "ZERO")), rest.iter().any(|z| kw(z, "ADC")));
                return Some(None);
            }
            if kw(nodes[1], "CANCel") && !query {
                ui.selfcal_remote("cancel", false);
                return Some(None);
            }
        }
        if kw(n0, "BODE") && nodes.len() == 2 {
            if kw(nodes[1], "RUN") && !query {
                ui.bode_run(parse_bool(arg).unwrap_or(true));
                return Some(None);
            }
            if kw(nodes[1], "DATA") && query {
                return Some(t(&ui.bode_text()));
            }
            // the mode of the settings table (settings/bode.rs)
            if let Some(row) = crate::settings::command(&crate::settings::bode::SLICES, &nodes[1..]) {
                return Some(self.setting_cmd(ui, row, query, arg));
            }
            if kw(nodes[1], "GENerator") {
                // "usb": the generator on the USB-A port; anything else is its LAN address (kept while on USB)
                if query {
                    return Some(t(if ui.m.bode_usb { "usb" } else { ui.m.bode_addr.as_str() }));
                }
                // one word, like the setup file keeps it
                let a = arg.trim().trim_matches('"');
                if a.contains(char::is_whitespace) {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                } else {
                    // the generator of the Ext generator page: another place is another connection
                    let usb = a.eq_ignore_ascii_case("usb");
                    if !ui.ext_place(usb, if usb { None } else { Some(a) }) {
                        self.push_err("-221,\"Settings conflict\" The Bode sweep is using the generator".into());
                    }
                }
                return Some(None);
            }
        }
        None
    }

    /// A value a known header does not take: -224 with the value, and the command counts as answered (`exec_cmd` sees
    /// the error and marks nothing changed). A branch of `model_cmd` that answered None instead would read as an
    /// unknown header to `exec_cmd`, which ends in -113.
    fn refused(&mut self, arg: &str) -> Option<Option<Vec<u8>>> {
        self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
        Some(None)
    }

    /// The commands that set or read the model alone: `:RUN`, `:STOP`, `:SINGle`, the channels, the trigger and the
    /// acquisition. None when `nodes` is none of them (the next branch of `exec_cmd` gets it), else its answer; a
    /// value a known header does not take is `refused`, never None.
    fn model_cmd(
        &mut self,
        m: &mut Model,
        src: &crate::wave::Source,
        clipping: &[bool],
        nodes: &[&str],
        query: bool,
        arg: &str,
    ) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        let (k0, sfx) = suffix(n0);
        if !query && nodes.len() == 1 {
            if kw(n0, "RUN") {
                m.run = RunState::Run;
                m.run_sweep();
                return Some(None);
            }
            if kw(n0, "STOP") {
                m.run = RunState::Stop;
                return Some(None);
            }
            if kw(n0, "SINGle") {
                m.set_sweep(TrigMode::Single);
                m.run = RunState::Run;
                return Some(None);
            }
        }
        if kw(k0, "CHANnel") && (nodes.len() == 2 || nodes.len() == 3) {
            let c = sfx?.checked_sub(1).filter(|&c| c < NCH)?;
            // the channel settings of the table (settings/channel.rs), of channel c
            if let Some(row) = crate::settings::command(&crate::settings::channel::SLICES, &nodes[1..]) {
                if query {
                    return Some(t(&row.query(m, c)));
                }
                if !row.take(m, c, arg) {
                    return self.refused(arg);
                }
                return Some(None);
            }
            let n1 = nodes[1];
            if nodes.len() == 3 && kw(nodes[1], "LABel") && kw(nodes[2], "CONTent") {
                if query {
                    return Some(t(&m.ch_label[c]));
                }
                let text = arg.trim().trim_matches('"').chars().take(10).collect::<String>();
                m.ch_label[c] = text;
                return Some(None);
            }
            if nodes.len() == 3 && kw(nodes[1], "LABel") && kw(nodes[2], "SHOW") {
                if query {
                    return Some(t(if m.ch_label[c].is_empty() { "0" } else { "1" }));
                }
                return match parse_bool(arg) {
                    Some(false) => {
                        m.ch_label[c].clear();
                        Some(None)
                    }
                    Some(true) => {
                        if m.ch_label[c].is_empty() {
                            m.ch_label[c] = format!("CH{}", c + 1);
                        }
                        Some(None)
                    }
                    None => self.refused(arg),
                };
            }
            if nodes.len() != 2 {
                return None;
            }
            // :CHANnel<n>:HIDE: the channel stays on (acquired, measured, the input of a math trace) without its trace
            if kw(n1, "HIDE") {
                if query {
                    return Some(t(if m.ch_hidden[c] { "1" } else { "0" }));
                }
                m.ch_hidden[c] = parse_bool(arg)?;
                return Some(None);
            }
            if kw(n1, "CLIPping") {
                // OOS: the record on screen reaches the limit of the ADC range on this channel (query only).
                if !query {
                    self.push_err("-113,\"Undefined header\" CLIPping is a query".into());
                    return Some(None);
                }
                return Some(t(if clipping.get(c).copied().unwrap_or(false) { "1" } else { "0" }));
            }
            let expand_gnd = m.v_expand_gnd;
            let slow_deskew = m.time_per_div() > 10e-6;
            let fine = m.fine_v;
            let ch = &mut m.ch[c];
            if kw(n1, "SCALe") {
                if query {
                    return Some(t(&e(ch.volts_per_div() as f64)));
                }
                let old = ch.volts_per_div();
                let Some(v) = num(arg).filter(|&v| v > 0.0) else {
                    return self.refused(arg);
                };
                let want = v / ch.probe;
                (ch.vscale, ch.vfine) = Ui::vscale_for(want, fine);
                if expand_gnd && old > 0.0 {
                    ch.offset *= ch.volts_per_div() / old;
                }
            } else if kw(n1, "OFFSet") || kw(n1, "POSition") {
                if query {
                    return Some(t(&e(ch.offset as f64)));
                }
                let Some(v) = num(arg) else {
                    return self.refused(arg);
                };
                ch.offset = v;
            } else if kw(n1, "COUPling") {
                if query {
                    return Some(t(ch.coupling.name()));
                }
                ch.coupling = match arg.to_ascii_uppercase().as_str() {
                    "DC" => Coupling::Dc,
                    "AC" => Coupling::Ac,
                    "GND" => Coupling::Gnd,
                    "DC50" if board().couplings().contains(&Coupling::Dc50) => Coupling::Dc50,
                    _ => return self.refused(arg),
                };
            } else if kw(n1, "PROBe") {
                if query {
                    return Some(t(&e(ch.probe as f64)));
                }
                let Some(p) = num(arg).filter(|&p| p > 0.0) else {
                    return self.refused(arg);
                };
                ch.probe = p;
            } else if kw(n1, "BIAS") {
                // The screen's zero correction, subtracted from the channel reading.
                // The DHO programmer manual has no header for it.
                if query {
                    return Some(t(&e(ch.bias as f64)));
                }
                let Some(v) = num(arg) else {
                    return self.refused(arg);
                };
                ch.bias = v;
            } else if kw(n1, "TCALibrate") {
                if query {
                    return Some(t(&e(ch.skew as f64)));
                }
                // The manual refuses a deskew change on a slow time base.
                if slow_deskew {
                    self.push_err("-224,\"Illegal parameter value\" timebase".into());
                    return Some(None);
                }
                let Some(v) = num(arg).filter(|v| v.abs() <= SKEW_MAX) else {
                    return self.refused(arg);
                };
                ch.skew = v;
            } else {
                return None;
            }
            // the offset within what the scale can reach now, as the next frame would leave it: the result of a
            // line of commands does not depend on whether a frame ran between them, and a query answers the offset
            // in force
            m.clamp_offsets();
            return Some(None);
        }
        self.trigger_acquire_cmd(m, src, nodes, query, arg)
    }

    /// `model_cmd` of the trigger, then of the acquisition (`acquire_cmd`): None when `nodes` is neither, else its
    /// answer.
    fn trigger_acquire_cmd(
        &mut self,
        m: &mut Model,
        src: &crate::wave::Source,
        nodes: &[&str],
        query: bool,
        arg: &str,
    ) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        if kw(n0, "TRIGger") && nodes.len() == 3 && ["RS232", "SPI", "IIC"].iter().any(|b| kw(nodes[1], b)) {
            // `ser::trig` answers None for a value it does not take as for a key it does not know; every key it
            // knows has a query, so a key whose query answers is known and its value is refused
            return match ser::trig(m, nodes[1], nodes[2], arg, query) {
                None if !query && ser::trig(m, nodes[1], nodes[2], "", true).is_some() => self.refused(arg),
                r => r,
            };
        }
        if kw(n0, "TRIGger") {
            let last = *nodes.last()?;
            if nodes.len() == 2 && kw(last, "STATus") {
                let s = match (m.run, m.status) {
                    (RunState::Stop, _) => "STOP",
                    (_, TrigStatus::Triggered) => "TD",
                    (_, TrigStatus::Auto) => "AUTO",
                    (_, TrigStatus::Wait) => "WAIT",
                    _ => "STOP",
                };
                return Some(t(s));
            }
            // the trigger values of the settings table (settings/trigger.rs)
            if let Some(row) = crate::settings::command(&crate::settings::trigger::SLICES, &nodes[1..]) {
                if query {
                    return Some(t(&row.query(m, 0)));
                }
                if !row.take(m, 0, arg) {
                    return self.refused(arg);
                }
                return Some(None);
            }
            if nodes.len() == 3 && kw(nodes[1], "EDGE") {
                if kw(last, "SOURce") {
                    if query {
                        return Some(t(&m
                            .trig_line()
                            .map_or(format!("CHAN{}", m.trig_src + 1), |n| format!("D{}", n))));
                    }
                    // D0..D15 like Rigol's source names, with the LA on
                    match arg
                        .trim()
                        .to_ascii_uppercase()
                        .strip_prefix('D')
                        .and_then(|n| n.parse::<u8>().ok())
                        .filter(|&n| n < 16 && m.trig_la_ok())
                    {
                        Some(n) => m.trig_la = Some(n),
                        None => match parse_ch(arg) {
                            Some(c) => (m.trig_src, m.trig_la) = (c, None),
                            None => return self.refused(arg),
                        },
                    }
                } else if kw(last, "LEVel") {
                    if query {
                        return Some(t(&e(m.trig_level as f64)));
                    }
                    let Some(v) = num(arg) else {
                        return self.refused(arg);
                    };
                    m.trig_level = v;
                } else {
                    return None;
                }
                return Some(None);
            }
            return None;
        }
        self.acquire_cmd(m, src, nodes, query, arg)
    }

    /// `model_cmd` of the acquisition: None when `nodes` is none of its commands, else its answer.
    fn acquire_cmd(
        &mut self,
        m: &mut Model,
        src: &crate::wave::Source,
        nodes: &[&str],
        query: bool,
        arg: &str,
    ) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        if kw(n0, "ACQuire") && nodes.len() == 3 && kw(nodes[1], "ULTRa") && kw(nodes[2], "MAXFrame") {
            if query {
                return Some(t(&m.segments.to_string()));
            }
            let Some(n) = arg.trim().parse::<usize>().ok() else {
                return self.refused(arg);
            };
            let steps = crate::ui::SEGMENTS;
            m.segments = *steps.iter().min_by_key(|s| s.abs_diff(n)).unwrap_or(&0);
            return Some(None);
        }
        if kw(n0, "ACQuire") && nodes.len() == 2 && !query && kw(nodes[1], "MDEPth") {
            // AUTO or one of the depths of the Acquire menu (`mdepths`), e.g. 1000000 or 1M
            let a = arg.trim().to_ascii_uppercase();
            if a.starts_with("AUTO") {
                m.mdepth = 0;
                return Some(None);
            }
            let (v, mul) = match a.chars().last() {
                Some('K') => (&a[..a.len() - 1], 1e3),
                Some('M') => (&a[..a.len() - 1], 1e6),
                _ => (&a[..], 1.0),
            };
            let Some(x) = num64(v).filter(|x| *x >= 0.0) else {
                return self.refused(arg);
            };
            let want = (x * mul).round() as u64;
            // the query answers the depth the scope works with (a menu depth is cut to the memory it has), so what
            // it answered is taken back: the setting that gives that depth, the one it has first
            let keep = m.mdepth;
            let effective: Vec<u64> = (0..mdepths().len())
                .map(|i| {
                    m.mdepth = i;
                    m.srate_depth().1 as u64
                })
                .collect();
            m.mdepth = keep;
            let pick = if effective[keep] == want {
                Some(keep)
            } else if let Some(i) = (1..mdepths().len()).find(|&i| mdepths()[i].1 == want) {
                Some(i)
            } else if effective[0] == want {
                Some(0)
            } else {
                (1..mdepths().len()).find(|&i| effective[i] == want)
            };
            match pick {
                Some(i) => m.mdepth = i,
                None => return self.refused(arg),
            }
            return Some(None);
        }
        // the acquisition settings of the table (settings/acquisition.rs), where :CORes was, before :SEGMents,
        // :AVERages:HARDware, :TYPE and :AVERages
        if kw(n0, "ACQuire") {
            if let Some(row) = crate::settings::command(&crate::settings::acquisition::SLICES, &nodes[1..]) {
                if query {
                    return Some(t(&row.query(m, 0)));
                }
                if !row.take(m, 0, arg) {
                    return self.refused(arg);
                }
                return Some(None);
            }
        }
        // segmented recording (OOS): :ACQuire:SEGMents <n>|OFF, :SEGMents:INTerval <s> (:HARDware is a row)
        if kw(n0, "ACQuire") && (2..=3).contains(&nodes.len()) && kw(nodes[1], "SEGMents") {
            match (nodes.get(2).copied(), query) {
                (None, true) => return Some(t(&m.segments.to_string())),
                (None, false) => {
                    m.segments = if parse_bool(arg) == Some(false) {
                        0
                    } else {
                        let Ok(n) = arg.trim().parse::<usize>() else {
                            return self.refused(arg);
                        };
                        n.min(crate::ui::SEG_HW_MAX)
                    }
                }
                (Some(x), true) if kw(x, "INTerval") => return Some(t(&e(m.seg_ivl as f64))),
                (Some(x), false) if kw(x, "INTerval") => match num(arg) {
                    Some(v) => m.seg_ivl = v.clamp(0.0, crate::ui::SEG_IVL_MAX),
                    None => return self.refused(arg),
                },
                _ => return None,
            }
            return Some(None);
        }
        if kw(n0, "ACQuire") && nodes.len() == 2 {
            let n1 = nodes[1];
            if query {
                // what the top bar shows: the live hardware record, not the model's stock-system choice for Auto
                let (sr, depth) = src.rate_depth(m);
                if kw(n1, "SRATe") {
                    return Some(t(&e(sr)));
                }
                if kw(n1, "MDEPth") {
                    return Some(t(&e(depth as f64)));
                }
            }
            return None;
        }
        None
    }

    /// `:SOURce<n>`: generator n, on a model that has one. None when no branch takes the nodes (the next branch of
    /// `exec_cmd` gets them), else the answer.
    fn source_cmd(
        &mut self,
        ui: &mut Ui,
        cv: &Canvas,
        nodes: &[&str],
        query: bool,
        arg: &str,
        cmd: &str,
    ) -> Option<Option<Vec<u8>>> {
        let n0 = nodes.first().copied().unwrap_or("");
        if !oos_hal::model::platform().caps.has_gen() {
            self.push_err(format!(
                "-241,\"Hardware missing\" no generator on the {}",
                oos_hal::model::platform().caps.name
            ));
            return Some(None);
        }
        // :SOURce<n> talks to generator n, whichever one the page shows
        let g = suffix(n0).1.unwrap_or(1) - 1;
        if g != ui.m.gen_sel {
            let shown = ui.m.gen_sel;
            ui.m.gen_select(g);
            let r = self.exec_cmd(ui, cv, cmd);
            ui.m.gen_select(shown);
            return Some(r);
        }
        let has = |k: &str| nodes[1..].iter().any(|n| kw(suffix(n).0, k));
        let m = &ui.m;
        let x = num64(arg);
        if has("OUTPut") {
            if query {
                return Some(t(if m.afg_on { "1" } else { "0" }));
            }
            match parse_bool(arg) {
                Some(on) => ui.afg_set(Some(on), None, None, None),
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        if has("MOD") {
            return self.source_mod_cmd(ui, nodes, query, arg);
        }
        // :FUNCtion:SQUare:DUTY / :FUNCtion:PULSe:DUTY and :FUNCtion:RAMP:SYMMetry in %
        // arbitrary wave: :ARB:DATA <v>,<v>,... (one period, any scale), :ARB:CAPTure CHAN<n> (the screen of a channel)
        if has("ARB") {
            if has("DATA") && !query {
                let v: Vec<f32> = arg.split(',').filter_map(num).collect();
                if !ui.afg_arb_load(&v) {
                    self.push_err(format!("-224,\"Illegal parameter value\" {} samples", v.len()));
                }
            } else if has("CAPTure") && !query {
                match parse_ch(arg).ok_or("no channel".to_string()).and_then(|c| ui.afg_arb_capture(c)) {
                    Ok(_) => {}
                    Err(e) => self.push_err(format!("-224,\"Illegal parameter value\" {}", e)),
                }
            } else {
                self.push_err("-113,\"Undefined header\" :SOURce:ARB".into());
            }
            return Some(None);
        }
        if has("FUNCtion") && (has("DUTY") || has("SYMMetry")) {
            let duty = has("DUTY");
            if query {
                return Some(t(&e(if duty { m.afg_duty } else { m.afg_symmetry })));
            }
            match x {
                Some(v) if duty => ui.m.afg_duty = v.clamp(5.0, 95.0),
                Some(v) => ui.m.afg_symmetry = v.clamp(0.0, 100.0),
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        if has("FUNCtion") {
            const SHORT: [&str; 7] = ["SIN", "SQU", "RAMP", "PULS", "NOIS", "DC", "ARB"];
            const LONG: [&str; 7] = ["SINusoid", "SQUare", "RAMP", "PULSe", "NOISe", "DC", "ARB"];
            if query {
                return Some(t(SHORT[m.afg_wave.min(6)]));
            }
            match LONG.iter().position(|l| kw(arg.trim(), l)) {
                Some(i) => {
                    ui.m.afg_wave = i;
                    ui.afg_set(None, None, None, None);
                }
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        let (want, cur): (u8, f64) = if has("FREQuency") {
            (0, m.afg_freq)
        } else if has("VOLTage") && has("OFFSet") {
            (2, m.afg_offset)
        } else if has("VOLTage") {
            (1, m.afg_ampl)
        } else {
            (9, 0.0)
        };
        if want < 9 {
            if query {
                return Some(t(&e(cur)));
            }
            match x {
                Some(v) => {
                    ui.afg_set(None, (want == 0).then_some(v), (want == 1).then_some(v), (want == 2).then_some(v))
                }
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        None
    }

    /// `source_cmd` of the modulation (Rigol DG style): :MOD[:STATe] ON|OFF, :MOD:TYPe AM|FM|PM, :MOD:AM:DEPTh <%>,
    /// :MOD:FM:DEViation <Hz>, :MOD:PM:DEViation <deg>, :MOD:<type>:INTernal:FREQuency <Hz> and
    /// :MOD:<type>:INTernal:FUNCtion <shape>.
    fn source_mod_cmd(&mut self, ui: &mut Ui, nodes: &[&str], query: bool, arg: &str) -> Option<Option<Vec<u8>>> {
        let has = |k: &str| nodes[1..].iter().any(|n| kw(suffix(n).0, k));
        let m = &ui.m;
        let x = num64(arg);
        const SHAPES: [&str; 6] = ["SINusoid", "SQUare", "TRIangle", "RAMP", "NRAMp", "NOISe"];
        let a = arg.trim();
        if has("INTernal") && has("FUNCtion") {
            if query {
                return Some(t(&SHAPES[m.afg_mod_shape.min(5)]
                    .chars()
                    .filter(|c| c.is_ascii_uppercase())
                    .collect::<String>()));
            }
            match SHAPES.iter().position(|s| kw(a, s)) {
                Some(i) => ui.m.afg_mod_shape = i,
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        }
        let (cur, key): (f64, u8) = if has("INTernal") && has("FREQuency") {
            (m.afg_mod_rate, 0)
        } else if has("DEPTh") {
            (m.afg_mod_depth, 1)
        } else if has("DEViation") && has("PM") {
            (m.afg_pm_dev, 3)
        } else if has("DEViation") {
            (m.afg_fm_dev, 2)
        } else if has("TYPe") {
            if query {
                return Some(t(["NONE", "AM", "FM", "PM"][m.afg_mod.min(3)]));
            }
            match ["AM", "FM", "PM"].iter().position(|k| kw(a, k)) {
                Some(i) => ui.m.afg_mod = i + 1,
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            ui.afg_set(None, None, None, None);
            return Some(None);
        } else {
            // :MOD[:STATe]
            if query {
                return Some(t(if m.afg_mod > 0 { "1" } else { "0" }));
            }
            match parse_bool(arg) {
                Some(on) => ui.m.afg_mod = if on { ui.m.afg_mod.max(1) } else { 0 },
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return Some(None);
        };
        if query {
            return Some(t(&e(cur)));
        }
        match x {
            Some(v) => {
                let mm = &mut ui.m;
                match key {
                    0 => mm.afg_mod_rate = v.clamp(0.002, 1e6),
                    1 => mm.afg_mod_depth = v.clamp(0.0, 120.0),
                    2 => mm.afg_fm_dev = v.max(0.01),
                    _ => mm.afg_pm_dev = v.clamp(0.0, 360.0),
                }
                ui.afg_set(None, None, None, None);
            }
            None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
        }
        Some(None)
    }
}

#[cfg(test)]
mod vernier_tests {
    #[test]
    fn vernier_takes_any_scale_at_or_above_a_calibrated_row() {
        let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
        let mut q = |c: &str| crate::ui::testkit::q(&mut scpi, &mut ui, &cv, c);
        // coarse: the nearest 1-2-5 step
        assert_eq!(q(":CHAN1:VERN?"), "0");
        q(":CHAN1:SCAL 0.37");
        assert_eq!(q(":CHAN1:SCAL?"), "5.000000E-01");
        // fine: a row of the ladder is kept, between rows the next one up
        q(":CHAN1:VERN ON");
        assert_eq!(q(":CHAN1:VERN?"), "1");
        q(":CHAN1:SCAL 0.37");
        assert_eq!(q(":CHAN1:SCAL?"), "3.700000E-01");
        q(":CHAN1:SCAL 0.371");
        assert_eq!(q(":CHAN1:SCAL?"), "3.750000E-01");
        q(":CHAN2:PROB 10");
        q(":CHAN2:SCAL 1.234");
        assert_eq!(q(":CHAN2:SCAL?"), "1.240000E+00");
        // the ends of the ladder hold, nonsense changes nothing
        q(":CHAN1:SCAL 1e-9");
        assert_eq!(q(":CHAN1:SCAL?"), "1.000000E-04");
        q(":CHAN1:SCAL 1e9");
        assert_eq!(q(":CHAN1:SCAL?"), "1.000000E+01");
        q(":CHAN1:SCAL -1");
        assert_eq!(q(":CHAN1:SCAL?"), "1.000000E+01");
        // off keeps the scale until the next coarse step
        q(":CHAN1:SCAL 0.37");
        q(":CHAN1:VERN OFF");
        assert_eq!((q(":CHAN1:VERN?"), q(":CHAN1:SCAL?")), ("0".to_string(), "3.700000E-01".to_string()));
        drop(q);
        // a saved setup keeps both
        let b = crate::store::parse_setup(&crate::store::setup_text(&ui.m)).unwrap();
        assert_eq!(b.ch[0].volts_per_div(), ui.m.ch[0].volts_per_div());
    }
}

/// What `*OPT?` and `:SYSTem:OPTion:INSTalled?` answer: the options of this unit separated by commas, `0` for none.
pub(crate) fn options_text() -> String {
    let o = oos_hal::model::platform().options();
    if o.is_empty() {
        "0".to_string()
    } else {
        o.join(",")
    }
}
