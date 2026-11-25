// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:CURSor`: `MODE`, the manual and track cursors (`MANual:TYPE`, `CAX|CBX|CAY|CBY`, `AXValue|AYValue|BXValue|
//! BYValue`), the snap of the time cursors (`SNAP:EDGE`, `SNAP:EXECute`), the deltas (`XDELta|IXDelta|YDELta`) and `MEASure:INDicator`. A new `:CURSor` command goes into
//! `cursor_cmd` here, with its line in docs/scpi.md and its query in ui/tests/scpi.txt (AGENTS.md).

use super::{e, kw, num64, parse_wave, shown, t, wave_tok, Scpi};
use crate::model::*;
use crate::ui::Ui;

impl Scpi {
    pub(super) fn cursor_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let n0 = n.first().copied().unwrap_or("");
        if n.is_empty() || (n.len() == 1 && kw(n0, "MODE")) {
            if n.is_empty() {
                self.push_err("-113,\"Undefined header\" :CURSor".into());
                return None;
            }
            return self.cursor_mode(ui, query, arg);
        }
        if n.len() == 2 && kw(n0, "MEASure") && kw(n[1], "INDicator") {
            return self.meas_indicator(ui, query, arg);
        }
        if n.len() == 2 && kw(n0, "SNAP") && kw(n[1], "EDGE") {
            return self.cursor_snap_edge(ui, query, arg);
        }
        if n.len() == 2 && kw(n0, "SNAP") && kw(n[1], "EXECute") {
            if query {
                self.push_err("-113,\"Undefined header\" :CURSor:SNAP:EXECute?".into());
            } else if !ui.cursor_snap_now() {
                self.push_err("-221,\"Settings conflict\" no time cursors on, or no edge near them".into());
            }
            return None;
        }
        if n.len() == 2 && kw(n0, "MANual") && kw(n[1], "TYPE") {
            return self.cursor_type(ui, query, arg);
        }
        // Track cursors follow a time and read the voltage there.
        if n.len() == 2 && kw(n0, "TRACk") && kw(n[1], "MODE") {
            if query {
                return Some(b"X".to_vec());
            }
            if kw(arg, "X") {
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" track follows time".into());
            return None;
        }
        if n.len() == 2
            && (kw(n0, "MANual") || kw(n0, "TRACk"))
            && (kw(n[1], "SOURce") || kw(n[1], "SOURce1") || kw(n[1], "SOURce2"))
        {
            return self.cursor_src(ui, n, query, arg);
        }
        if n.len() != 2 {
            self.push_err(format!("-113,\"Undefined header\" :CURSor:{}", n.join(":")));
            return None;
        }
        let which = n[1];
        let xy = kw(n0, "XY");
        let known = kw(n0, "MANual") || kw(n0, "TRACk") || xy;
        if !known {
            self.push_err(format!("-113,\"Undefined header\" :CURSor:{}", n.join(":")));
            return None;
        }
        let x = kw(which, "CAX") || (xy && kw(which, "AX"));
        let xb = kw(which, "CBX") || (xy && kw(which, "BX"));
        let y = kw(which, "CAY") || (xy && kw(which, "AY"));
        let yb = kw(which, "CBY") || (xy && kw(which, "BY"));
        if x || xb {
            return self.cursor_axis(ui, query, arg, true, x);
        }
        if (y || yb) && query && kw(n0, "TRACk") && ui.m.cursor == CursorMode::Track {
            // a track cursor has no Y of its own: its Y is the trace under it, the value AYValue and BYValue answer
            let source = if yb { ui.cursor_b_source() } else { ui.cursor_source() };
            let at = ui.cursor_time_at(if y { ui.m.cur_a } else { ui.m.cur_b });
            return Some(shown(ui.wave_at_source(source, at).map(|v| v as f64).unwrap_or(f64::NAN)).into_bytes());
        }
        if y || yb {
            return self.cursor_axis(ui, query, arg, false, y);
        }
        if !query {
            self.push_err(format!("-113,\"Undefined header\" :CURSor:{}", n.join(":")));
            return None;
        }
        self.cursor_readout(ui, n)
    }

    /// `:CURSor:MODE`: OFF, MANual, TRACk or XY.
    fn cursor_mode(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            let s = match ui.m.cursor {
                CursorMode::Off => "OFF",
                CursorMode::Time | CursorMode::Volt => "MAN",
                CursorMode::Track => "TRAC",
                CursorMode::Xy => "XY",
            };
            return t(s);
        }
        let a = arg.trim();
        ui.m.cursor = if kw(a, "OFF") {
            CursorMode::Off
        } else if kw(a, "MANual") {
            if ui.m.cursor == CursorMode::Volt {
                CursorMode::Volt
            } else {
                CursorMode::Time
            }
        } else if kw(a, "TRACk") {
            CursorMode::Track
        } else if a.eq_ignore_ascii_case("XY") {
            CursorMode::Xy
        } else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        None
    }

    /// `:CURSor:SNAP:EDGE`: OFF, RISing, FALLing or EITHer, the edges the time cursors snap to when dropped.
    fn cursor_snap_edge(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            return t(match ui.m.cur_snap {
                CursorSnap::Off => "OFF",
                CursorSnap::Rising => "RIS",
                CursorSnap::Falling => "FALL",
                CursorSnap::Either => "EITH",
            });
        }
        let a = arg.trim();
        ui.m.cur_snap = if kw(a, "OFF") {
            CursorSnap::Off
        } else if kw(a, "RISing") {
            CursorSnap::Rising
        } else if kw(a, "FALLing") {
            CursorSnap::Falling
        } else if kw(a, "EITHer") {
            CursorSnap::Either
        } else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        None
    }

    /// `:CURSor:MANual:TYPE`: TIME or AMPLitude.
    fn cursor_type(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            return t(if ui.m.cursor == CursorMode::Volt { "AMPL" } else { "TIME" });
        }
        if kw(arg, "TIME") {
            if ui.m.cursor != CursorMode::Xy {
                ui.m.cursor = CursorMode::Time;
            }
        } else if kw(arg, "AMPLitude") {
            if ui.m.cursor != CursorMode::Xy {
                ui.m.cursor = CursorMode::Volt;
            }
        } else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        }
        None
    }

    /// `:CURSor:MANual|TRACk:SOURce[1|2]`: the waveform a cursor reads (`n` without the root).
    fn cursor_src(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let n0 = n[0];
        let second = kw(n[1], "SOURce2");
        let track = kw(n0, "TRACk");
        if query {
            return t(&wave_tok(if second && track { ui.cursor_b_source() } else { ui.cursor_source() }));
        }
        let math = super::decode::parse_math_source(arg).and_then(crate::model::decode_math);
        let source = match (parse_wave(arg), math) {
            (Some(source), _) => source,
            (None, Some(k)) => match ui.ref_math_slot(k) {
                Ok(slot) => WaveId::Ref(slot),
                Err(m) => {
                    self.push_err(format!("-221,\"Settings conflict; {m}\""));
                    return None;
                }
            },
            (None, None) => {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            }
        };
        let ok = if second && track {
            ui.set_cursor_b_source(source)
        } else if second {
            source == ui.cursor_source()
        } else {
            ui.set_cursor_source(source)
        };
        if !ok {
            self.push_err("-224,\"Cursor source is unavailable or unsupported\"".into());
        }
        None
    }

    /// The read-outs under `:CURSor:MANual|TRACk|XY` (`AXValue?` to `YDELta?`, `n` without the root).
    fn cursor_readout(&mut self, ui: &Ui, n: &[&str]) -> Option<Vec<u8>> {
        let which = n[1];
        let miss = || Some(b"9.9E37".to_vec());
        let (ta, tb) = (ui.cursor_time_at(ui.m.cur_a), ui.cursor_time_at(ui.m.cur_b));
        let y_of = |a: bool| {
            if ui.m.cursor == CursorMode::Xy {
                ui.cursor_volt_at(if a { ui.m.cur_c } else { ui.m.cur_d })
            } else if ui.m.cursor == CursorMode::Volt {
                ui.cursor_volt_at(if a { ui.m.cur_a } else { ui.m.cur_b })
            } else if matches!(ui.m.cursor, CursorMode::Track | CursorMode::Time) {
                let source =
                    if !a && ui.m.cursor == CursorMode::Track { ui.cursor_b_source() } else { ui.cursor_source() };
                ui.wave_at_source(source, if a { ta } else { tb }).map(|v| v as f64).unwrap_or(f64::NAN)
            } else {
                f64::NAN
            }
        };
        if kw(which, "AXValue") {
            return if ui.m.cursor == CursorMode::Volt || ui.m.cursor == CursorMode::Off { miss() } else { t(&e(ta)) };
        }
        if kw(which, "BXValue") {
            return if ui.m.cursor == CursorMode::Volt || ui.m.cursor == CursorMode::Off { miss() } else { t(&e(tb)) };
        }
        if kw(which, "AYValue") {
            return t(&e(y_of(true)));
        }
        if kw(which, "BYValue") {
            return t(&e(y_of(false)));
        }
        if kw(which, "XDELta") {
            return if matches!(ui.m.cursor, CursorMode::Time | CursorMode::Track | CursorMode::Xy) {
                t(&e(tb - ta))
            } else {
                miss()
            };
        }
        if kw(which, "IXDelta") {
            let d = (tb - ta).abs();
            return if d > 0.0 && matches!(ui.m.cursor, CursorMode::Time | CursorMode::Track | CursorMode::Xy) {
                t(&e(1.0 / d))
            } else {
                miss()
            };
        }
        if kw(which, "YDELta") {
            if ui.m.cursor == CursorMode::Track
                && ui.cursor_source_unit(ui.cursor_source()) != ui.cursor_source_unit(ui.cursor_b_source())
            {
                return miss();
            }
            let d = y_of(false) - y_of(true);
            return t(&e(d));
        }
        self.push_err(format!("-113,\"Undefined header\" :CURSor:{}", n.join(":")));
        None
    }

    /// Horizontal (`x`) or vertical cursor. `a` is cursor A.
    fn cursor_axis(&mut self, ui: &mut Ui, query: bool, arg: &str, x: bool, a: bool) -> Option<Vec<u8>> {
        // the cursors A and B, but C and D for the voltage in XY (cursor A is A or C)
        let place = |ui: &Ui, x: bool, a: bool| -> Option<f32> {
            let m = &ui.m;
            match (x, m.cursor) {
                (true, CursorMode::Volt | CursorMode::Off)
                | (false, CursorMode::Time | CursorMode::Track | CursorMode::Off) => None,
                (false, CursorMode::Xy) => Some(if a { m.cur_c } else { m.cur_d }),
                _ => Some(if a { m.cur_a } else { m.cur_b }),
            }
        };
        if query {
            let Some(frac) = place(ui, x, a) else { return Some(b"9.9E37".to_vec()) };
            let v = if x { ui.cursor_time_at(frac) } else { ui.cursor_volt_at(frac) };
            return Some(e(v).into_bytes());
        }
        let Some(v) = num64(arg) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        let frac = if x { ui.cursor_frac_time(v) } else { ui.cursor_frac_volt(v) };
        // a cursor of the other axis than the mode's turns it into XY (the voltage cursors there are C and D); one of
        // either axis turns Off into that axis
        let m = &mut ui.m;
        let (cd, mode) = match (x, m.cursor) {
            (true, CursorMode::Volt) => {
                (m.cur_c, m.cur_d) = (m.cur_a, m.cur_b);
                (false, CursorMode::Xy)
            }
            (false, CursorMode::Time | CursorMode::Track | CursorMode::Xy) => (true, CursorMode::Xy),
            (true, CursorMode::Off) => (false, CursorMode::Time),
            (false, CursorMode::Off) => (false, CursorMode::Volt),
            (_, mode) => (false, mode),
        };
        let cur = match (cd, a) {
            (false, true) => &mut m.cur_a,
            (false, false) => &mut m.cur_b,
            (true, true) => &mut m.cur_c,
            (true, false) => &mut m.cur_d,
        };
        *cur = frac;
        m.cursor = mode;
        None
    }
}
