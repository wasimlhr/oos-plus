// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:MEASure`: the items and channel pairs the strip can show (`ITEM`, `STRip?`, `READout?`, `SOURce`, `CLEar`),
//! statistics, thresholds (`REFS`, `SETup`, `THOLd`), gating, burst idle, Time Outside, Duration N, the edges and the
//! indicator (a row of settings/measure.rs). A new `:MEASure` setting of a setup file is a row of that table; another
//! command goes into `measure_cmd` here (a new item into `MEAS_ITEMS` or `PAIR_ITEMS`), with its line in docs/scpi.md;
//! either with its query in ui/tests/scpi.txt (AGENTS.md).

use super::math::math_index;
use super::{e, kw, num64, parse_bool, parse_wave, shown, t, wave_tok, Scpi};
use crate::model::*;
use crate::ui::slots::SLOTS;
use crate::ui::Ui;

/// Single-channel items the strip can show. The first spelling of a measurement is the one `STRip` and `READout` print.
/// A second spelling is only another way to ask.
const MEAS_ITEMS: &[(&str, &str, Meas)] = &[
    ("VPP", "VPP", Meas::Vpp),
    ("VMAX", "VMAX", Meas::Vmax),
    ("VMIN", "VMIN", Meas::Vmin),
    ("VAVG", "VAVG", Meas::Vavg),
    ("VRMS", "VRMS", Meas::Vrms),
    ("VAMP", "VAMP", Meas::Vamp),
    ("FREQuency", "FREQUENCY", Meas::Freq),
    ("PERiod", "PERIOD", Meas::Period),
    ("RTIMe", "RTIME", Meas::Rise),
    ("FTIMe", "FTIME", Meas::Fall),
    ("PWIDth", "PWIDTH", Meas::PWidth),
    ("PDUTy", "PDUTY", Meas::PDuty),
    ("NDUTy", "NDUTY", Meas::NDuty),
    ("NWIDth", "NWIDTH", Meas::NWidth),
    ("VTOP", "VTOP", Meas::Vtop),
    ("VBASe", "VBASE", Meas::Vbase),
    ("OVERshoot", "OVERSHOOT", Meas::Overshoot),
    ("PREShoot", "PRESHOOT", Meas::Preshoot),
    ("HTIMe", "HTIME", Meas::HighTime),
    ("LTIMe", "LTIME", Meas::LowTime),
    ("BWIDth", "BWIDTH", Meas::Burst),
    ("TOUTside", "TOUTSIDE", Meas::Outside),
    ("NPERiods", "NPERIODS", Meas::NPeriods),
    ("CAMP", "CAMP", Meas::CycAmp),
    ("CYCAmp", "CAMP", Meas::CycAmp),
    ("CTOP", "CTOP", Meas::CycTop),
    ("CBASe", "CBASE", Meas::CycBase),
    ("CPP", "CPP", Meas::CycPp),
    ("CYCPP", "CPP", Meas::CycPp),
    ("CMAX", "CMAX", Meas::CycMax),
    ("CMIN", "CMIN", Meas::CycMin),
    ("VUPPer", "VUPPER", Meas::Vupper),
    ("VMID", "VMID", Meas::Vmid),
    ("VLOWer", "VLOWER", Meas::Vlower),
    ("CAVG", "CAVG", Meas::VavgCyc),
    ("CRMS", "CRMS", Meas::VrmsCyc),
    ("PVRMs", "CRMS", Meas::VrmsCyc),
    ("ACRms", "ACRMS", Meas::VrmsAc),
    ("SDEViation", "SDEVIATION", Meas::StdDev),
    ("AREA", "AREA", Meas::Area),
    ("MARea", "AREA", Meas::Area),
    ("CARea", "CAREA", Meas::AreaCyc),
    ("MPARea", "CAREA", Meas::AreaCyc),
    ("PPULses", "PPULSES", Meas::PPulses),
    ("NPULses", "NPULSES", Meas::NPulses),
    ("PEDGes", "PEDGES", Meas::PEdges),
    ("NEDGes", "NEDGES", Meas::NEdges),
    ("TVMAX", "TVMAX", Meas::TVmax),
    ("TVMIN", "TVMIN", Meas::TVmin),
    ("PSLew", "PSLEW", Meas::PSlew),
    ("PSLewrate", "PSLEW", Meas::PSlew),
    ("NSLew", "NSLEW", Meas::NSlew),
    ("NSLewrate", "NSLEW", Meas::NSlew),
    ("ESTUI", "ESTUI", Meas::EstUi),
    ("UI", "ESTUI", Meas::EstUi),
    ("MEDian", "MEDIAN", Meas::Median),
    ("MODE", "MODE", Meas::Mode),
    ("LSLO", "LSLO", Meas::LinSlope),
    ("LINSLOPE", "LSLO", Meas::LinSlope),
    ("LINT", "LINT", Meas::LinInt),
    ("LININT", "LINT", Meas::LinInt),
    ("PAM0", "PAM0", Meas::Pam0),
    ("L0", "PAM0", Meas::Pam0),
    ("PAM1", "PAM1", Meas::Pam1),
    ("L1", "PAM1", Meas::Pam1),
    ("PAM2", "PAM2", Meas::Pam2),
    ("L2", "PAM2", Meas::Pam2),
    ("PAM3", "PAM3", Meas::Pam3),
    ("L3", "PAM3", Meas::Pam3),
    ("PSEP", "PSEP", Meas::PamSep),
    ("BITS", "BITS", Meas::Bits),
    ("ONES", "BITS", Meas::Bits),
    ("PLLUI", "PLLUI", Meas::PllUi),
    ("UIWTD", "UIWTD", Meas::UiWtd),
    ("FOLD", "FOLD", Meas::FoldRms),
];

/// Channel-pair rows, in the same order as the Measure menu (`PAIRS`).
const PAIR_ITEMS: &[(&str, &str, usize)] = &[
    ("RRDelay", "RRDELAY", 0),
    ("FFDelay", "FFDELAY", 1),
    ("RFDelay", "RFDELAY", 2),
    ("FRDelay", "FRDELAY", 3),
    ("RRPHase", "RRPHASE", 4),
    ("FFPHase", "FFPHASE", 5),
    ("RFPHase", "RFPHASE", 6),
    ("FRPHase", "FRPHASE", 7),
    ("RRSkew", "RRSKEW", 8),
    ("FFSkew", "FFSKEW", 9),
    ("RFSkew", "RFSKEW", 10),
    ("FRSkew", "FRSKEW", 11),
    ("SETup", "SETUP", 12),
    ("HOLD", "HOLD", 13),
    ("SETHold", "SETHOLD", 14),
    ("FSETup", "FSETUP", 15),
    ("FHOLd", "FHOLD", 16),
    ("DSETup", "DSETUP", 17),
    ("DHOLd", "DHOLD", 18),
    ("DSEThold", "DSETHOLD", 19),
];

/// The items `:MEASure:ITEM?` takes, each once in the spelling `STRip?` prints, and whether it is a pair (two
/// sources): the closed list the `measure` tool of MCP offers (site/mcp.rs).
pub(crate) fn measurement_items() -> Vec<(&'static str, bool)> {
    let mut v: Vec<(&'static str, bool)> = Vec::new();
    for (_, token, _) in MEAS_ITEMS {
        if !v.iter().any(|(t, _)| t == token) {
            v.push((token, false));
        }
    }
    v.extend(PAIR_ITEMS.iter().map(|(_, token, _)| (*token, true)));
    v
}

/// `SLOT<n>` of a header: Some(Some(n)) for a slot number, Some(None) for `SLOT` alone, None for another node.
fn slot_node(node: &str) -> Option<Option<u8>> {
    let (k, n) = super::suffix(node);
    if !kw(k, "SLOT") {
        return None;
    }
    if n.is_none() && !node.eq_ignore_ascii_case(k) {
        return None;
    }
    Some(n.filter(|n| (1..=SLOTS as usize).contains(n)).map(|n| n as u8))
}

/// What a row measures as `STRip?` writes it, without the value: `VPP,CHAN1`, `RRDELAY,CHAN1,CHAN2`.
fn slot_item(key: RowKey) -> String {
    match key {
        RowKey::One(k, id) => format!("{},{}", meas_token(k), wave_tok(id)),
        RowKey::Pair(k, a, b) => format!("{},{},{}", pair_token(k), wave_tok(a), wave_tok(b)),
        RowKey::Cat(id, ..) => format!("CAT{id}"),
    }
}

/// The slots in use, `n,ITEM,source[,source],value` each, separated by semicolons.
fn slot_list(ui: &Ui) -> String {
    if ui.m.meas_slots.is_empty() {
        return String::new();
    }
    ui.slots_in_use().into_iter().map(|(n, key)| format!("{n},{},{}", slot_item(key), e(ui.slot_value(key)))).collect::<Vec<_>>().join(";")
}

pub(crate) fn meas_kind(item: &str) -> Option<Meas> {
    MEAS_ITEMS.iter().find(|it| kw(item, it.0)).map(|it| it.2)
}

pub(super) fn meas_token(k: Meas) -> &'static str {
    MEAS_ITEMS.iter().find(|it| it.2 == k).map(|it| it.1).unwrap_or("ITEM")
}

pub(crate) fn pair_kind(item: &str) -> Option<usize> {
    PAIR_ITEMS.iter().find(|it| kw(item, it.0)).map(|it| it.2)
}

fn pair_token(k: usize) -> &'static str {
    PAIR_ITEMS.get(k.min(PAIR_ITEMS.len() - 1)).map(|it| it.1).unwrap_or("PAIR")
}

/// The measurement strip: each row, then the DVM and the counter when they are on.
fn meas_strip(ui: &Ui) -> String {
    let mut p = Vec::new();
    for &(k, id) in &ui.m.meas {
        p.push(format!("{},{},{}", meas_token(k), wave_tok(id), e(ui.fact_meas(k, id))));
    }
    for &(k, a, b) in &ui.m.meas2 {
        p.push(format!("{},{},{},{}", pair_token(k), wave_tok(a), wave_tok(b), e(ui.pair_at(k, a, b))));
    }
    for (id, [a, b, c, d], value) in ui.catalog_export_values() {
        p.push(format!("CAT{},{},{},{},{},{}", id, wave_tok(a), wave_tok(b), wave_tok(c), wave_tok(d), e(value)));
    }
    if ui.m.dvm_on {
        let mode = ["DC", "ACRMS", "ACDC"][ui.m.dvm_mode.min(2)];
        p.push(format!("DVM,{},{},{}", mode, wave_tok(WaveId::Ch(ui.m.dvm_src.min(NCH - 1))), e(ui.dvm_value())));
    }
    if ui.m.cnt_on {
        let mode = ["FREQUENCY", "PERIOD", "TOTALIZE"][ui.m.cnt_mode.min(2)];
        p.push(format!(
            "COUNTER,{},{},{}",
            mode,
            wave_tok(WaveId::Ch(ui.m.cnt_src.min(NCH - 1))),
            e(ui.counter_value())
        ));
    }
    p.join(";")
}

/// Strip rows, and the eye figures when the eye page is the one on screen.
fn meas_readout(ui: &Ui) -> String {
    let mut parts = Vec::new();
    let strip = meas_strip(ui);
    if !strip.is_empty() {
        parts.push(strip);
    }
    if ui.m.view == AView::Eye {
        let eye = crate::ui::EYE_ITEMS
            .iter()
            .map(|name| {
                let v = ui.eye_readings().iter().find(|(n, _)| n == name).map(|(_, v)| *v).unwrap_or(f64::NAN);
                format!("{},{}", name.to_ascii_uppercase(), e(v))
            })
            .collect::<Vec<_>>()
            .join(";");
        parts.push(eye);
    }
    parts.join(";")
}

impl Scpi {
    /// `:MEASure` (`nodes` with the root).
    pub(super) fn measure_cmd(
        &mut self,
        ui: &mut Ui,
        nodes: &[&str],
        query: bool,
        arg: &str,
        cmd: &str,
    ) -> Option<Vec<u8>> {
        if nodes.len() == 2 && query {
            if kw(nodes[1], "ITEM") {
                return self.meas_item(ui, arg);
            }
            if kw(nodes[1], "STRip") {
                return t(&meas_strip(ui));
            }
            // 1 while the measurements of the record shown are computed, so a value of 9.9E37 is "not yet", not "none"
            if kw(nodes[1], "PENDing") {
                return t(if ui.meas_pending_now() { "1" } else { "0" });
            }
            // every measurement on the screen: the strip, and the eye figures when that page is up
            if kw(nodes[1], "READout") {
                return t(&meas_readout(ui));
            }
        }
        if (2..=3).contains(&nodes.len()) && slot_node(nodes[1]).is_some() {
            return self.meas_slot_cmd(ui, nodes, query, arg);
        }
        if nodes.len() == 3 && kw(nodes[1], "BURSt") && kw(nodes[2], "IDLE") {
            if query {
                return t(&e(ui.m.meas_idle));
            }
            match num64(arg).filter(|v| *v >= 0.0) {
                Some(v) => {
                    ui.m.meas_idle = v;
                    ui.measure_last();
                    ui.mark_changed();
                }
                None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
            }
            return None;
        }
        if nodes.len() == 3 && kw(nodes[1], "STATistic") {
            return self.meas_stat_cmd(ui, nodes[2], query, arg);
        }
        if nodes.len() == 3 && kw(nodes[1], "SETup") {
            let which = nodes[2];
            if kw(which, "MAX") || kw(which, "MIN") {
                return self.meas_threshold(ui, kw(which, "MAX"), query, arg);
            }
            if kw(which, "MID") {
                // Edges are taken at half the amplitude. The middle is not a separate percent.
                if query {
                    return t("50");
                }
                if arg.trim().parse::<f64>().ok().is_some_and(|v| (v - 50.0).abs() < 1e-6) {
                    return None;
                }
                self.push_err("-224,\"Illegal parameter value\" middle is 50".into());
                return None;
            }
            if kw(which, "PSA") || kw(which, "DSA") || kw(which, "PSB") || kw(which, "DSB") {
                let second = kw(which, "PSB") || kw(which, "DSB");
                if query {
                    return t(&wave_tok(if second { ui.meas_source_b() } else { ui.meas_source() }));
                }
                let id = self.named_source(arg)?;
                if second {
                    ui.set_meas_source_b(id);
                } else {
                    ui.set_meas_source(id);
                }
                ui.mark_changed();
                return None;
            }
        }
        if nodes.len() == 2 {
            let n1 = nodes[1];
            if kw(n1, "GATing") {
                if query {
                    return t(if ui.m.meas_full_record { "NONE" } else { "SCREEN" });
                }
                let full_record = if kw(arg.trim(), "SCReen") {
                    false
                } else if kw(arg.trim(), "NONE") {
                    true
                } else {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    return None;
                };
                ui.set_measurement_full_record(full_record);
                return None;
            }
            if kw(n1, "SOURce") {
                return self.meas_sources(ui, query, arg);
            }
            if kw(n1, "CLEar") && !query {
                ui.meas_clear_remote();
                return None;
            }
            if kw(n1, "ITEM") && !query {
                return self.meas_place(ui, arg);
            }
            if kw(n1, "REFS") {
                return self.meas_refs(ui, query, arg);
            }
            if kw(n1, "INDicator") {
                return self.meas_indicator(ui, query, arg);
            }
            if kw(n1, "AREA") {
                // Measurements read the main record. The zoom window does not change them.
                if query {
                    return t("MAIN");
                }
                if kw(arg, "MAIN") {
                    return None;
                }
                self.push_err("-224,\"Illegal parameter value\" measurements use the main record".into());
                return None;
            }
            if kw(n1, "OUTSide") || kw(n1, "NPERiods") || kw(n1, "EDGes") {
                return self.meas_gate(ui, n1, query, arg);
            }
            if kw(n1, "TSETup") || kw(n1, "THOLd") {
                return self.meas_setup_hold(ui, n1, query, arg);
            }
            if kw(n1, "ITEM") && query {
                return self.meas_item(ui, arg);
            }
        }
        self.push_err(format!("-113,\"Undefined header\" {}", cmd));
        None
    }

    /// `:MEASure:SLOT[<n>][:ITEM|:CLEar]` (ui/slots.rs): `SLOT<n> <item>[,A[,B]]` adds the row and gives it the number,
    /// `SLOT<n>?` reads it, `SLOT<n>:ITEM?` says what it measures, `SLOT<n>:CLEar` empties it, `SLOT:LIST?` lists them. A
    /// slot that is empty is an error, never 9.9E37, which is "not computed yet".
    fn meas_slot_cmd(&mut self, ui: &mut Ui, nodes: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let n = match slot_node(nodes[1]) {
            Some(Some(n)) => n,
            _ => {
                if nodes.len() == 3 && query && kw(nodes[2], "LIST") {
                    return t(&slot_list(ui));
                }
                self.push_err(format!("-224,\"Illegal parameter value; a slot is MEAS1 to MEAS{SLOTS}: :MEASure:SLOT<n>\""));
                return None;
            }
        };
        let empty = |s: &mut Self| {
            s.push_err(format!(
                "-221,\"Settings conflict; MEAS{n} is empty: add a measurement with :MEASure:SLOT{n} <item>,<source>\""
            ));
            None
        };
        if nodes.len() == 2 {
            if !query {
                let Some(key) = self.meas_place_row(ui, arg) else { return None };
                ui.slot_assign(n, key);
                return None;
            }
            return match ui.slot_row(n) {
                Some(key) => t(&e(ui.slot_value(key))),
                None => empty(self),
            };
        }
        if kw(nodes[2], "ITEM") && query {
            return match ui.slot_row(n) {
                Some(key) => t(&slot_item(key)),
                None => empty(self),
            };
        }
        if kw(nodes[2], "CLEar") && !query {
            ui.slot_clear(n);
            return None;
        }
        self.push_err(format!("-113,\"Undefined header\" :MEASure:{}:{}", nodes[1], nodes[2]));
        None
    }

    /// `:MEASure:SOURce`: source A, and B after a comma.
    fn meas_sources(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            return t(&wave_tok(ui.meas_source()));
        }
        let words: Vec<_> = arg.split(',').map(str::trim).collect();
        if words.is_empty() || words.len() > 2 || words.iter().any(|word| word.is_empty()) {
            self.push_err("-224,\"Illegal parameter value\"".into());
            return None;
        }
        let a = self.named_source(words[0])?;
        let b = if words.len() == 2 { Some(self.named_source(words[1])?) } else { None };
        ui.set_meas_source(a);
        if let Some(b) = b {
            ui.set_meas_source_b(b);
        }
        ui.mark_changed();
        None
    }

    /// `:MEASure:REFS`: the 10/90 or 20/80 threshold pair.
    fn meas_refs(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            let ten = ui.m.meas_lpct < 0.15 && ui.m.meas_hpct > 0.85;
            return t(if ten { "10/90" } else { "20/80" });
        }
        let a = arg.trim().replace(' ', "");
        let (lo, hi) = if a == "10/90" || a == "1090" || a == "10" {
            (0.1, 0.9)
        } else if a == "20/80" || a == "2080" || a == "20" {
            (0.2, 0.8)
        } else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.meas_lpct = lo;
        ui.m.meas_hpct = hi;
        ui.measure_last();
        ui.mark_changed();
        None
    }

    /// `:MEASure:TSETup|THOLd` (`n1`): the setup and hold times.
    fn meas_setup_hold(&mut self, ui: &mut Ui, n1: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let setup = kw(n1, "TSETup");
        if query {
            return t(&e(if setup { ui.m.meas_tsetup } else { ui.m.meas_thold }));
        }
        match num64(arg).filter(|v| *v >= 0.0) {
            Some(v) => {
                if setup {
                    ui.m.meas_tsetup = v;
                } else {
                    ui.m.meas_thold = v;
                }
                ui.mark_changed();
            }
            None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
        }
        None
    }

    /// Top and base come from the histogram. AUTO and MAXMin are not on this screen.
    pub(super) fn meas_amp(&mut self, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if n.len() == 1 && kw(n[0], "TYPE") {
            if query {
                return t("MAN");
            }
            if kw(arg, "MANual") {
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" amplitude uses the histogram".into());
            return None;
        }
        if n.len() == 2 && kw(n[0], "MANual") && (kw(n[1], "TOP") || kw(n[1], "BASE")) {
            if query {
                return t("HIST");
            }
            if kw(arg, "HISTogram") {
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" top and base are histogram modes".into());
            return None;
        }
        self.push_err(format!("-113,\"Undefined header\" :MEASure:AMP:{}", n.join(":")));
        None
    }

    /// `:MEASure:LEVel:...`: the shared reference levels of the measurements (`Ui::meas_level_command`).
    pub(super) fn meas_level_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        match ui.meas_level_command(n, query, arg) {
            Ok(r) => r.map(String::into_bytes),
            Err(e) => {
                self.push_err(e);
                None
            }
        }
    }

    /// Upper (`high`) or lower threshold, as a percent of the waveform, the same pair `:MEASure:REFS` sets.
    fn meas_threshold(&mut self, ui: &mut Ui, high: bool, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            let pct = if high { ui.m.meas_hpct } else { ui.m.meas_lpct };
            return Some(format!("{}", (pct * 100.0).round() as i32).into_bytes());
        }
        let Some(pct) = num64(arg).filter(|v| (0.0..=100.0).contains(v)) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        let frac = pct / 100.0;
        if high {
            if frac <= ui.m.meas_lpct {
                self.push_err("-224,\"Illegal parameter value\" below the lower threshold".into());
                return None;
            }
            ui.m.meas_hpct = frac;
        } else {
            if frac >= ui.m.meas_hpct {
                self.push_err("-224,\"Illegal parameter value\" above the upper threshold".into());
                return None;
            }
            ui.m.meas_lpct = frac;
        }
        ui.measure_last();
        ui.mark_changed();
        None
    }

    fn meas_stat_cmd(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        if kw(which, "DISPlay") {
            if query {
                return t(if ui.stats_shown() { "1" } else { "0" });
            }
            return match parse_bool(arg) {
                Some(on) => {
                    ui.set_stats_shown(on);
                    None
                }
                None => {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    None
                }
            };
        }
        if kw(which, "RESet") && !query {
            ui.stats_reset();
            return None;
        }
        if kw(which, "POPulation") {
            if query {
                return t(if ui.m.meas_pop_current { "CURR" } else { "ACC" });
            }
            let want = if kw(arg.trim(), "CURRent") { true } else if kw(arg.trim(), "ACCumulated") { false } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            if want != ui.m.meas_pop_current { ui.m.meas_pop_current = want; ui.stats_reset(); }
            ui.mark_changed();
            return None;
        }
        // the columns of the statistics: any of SDEViation, MINimum, MAXimum, or NONE
        if kw(which, "SHOW") {
            if query {
                let s = ui.m.meas_show;
                let v: Vec<&str> = [(s[0], "SDEV"), (s[1], "MIN"), (s[2], "MAX")].into_iter().filter(|x| x.0).map(|x| x.1).collect();
                return t(&if v.is_empty() { "NONE".to_string() } else { v.join(",") });
            }
            let mut show = [false; 3];
            for part in arg.split(',').map(str::trim).filter(|p| !p.is_empty()) {
                if kw(part, "SDEViation") { show[0] = true; } else if kw(part, "MINimum") { show[1] = true; } else if kw(part, "MAXimum") { show[2] = true; }
                else if !kw(part, "NONE") {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    return None;
                }
            }
            ui.m.meas_show = show;
            ui.mark_changed();
            return None;
        }
        if !kw(which, "ITEM") {
            self.push_err(format!("-113,\"Undefined header\" :MEASure:STATistic:{}", which));
            return None;
        }
        let mut parts = arg.split(',').map(str::trim).filter(|s| !s.is_empty());
        let first = parts.next().unwrap_or("");
        if !query {
            let item = first;
            let rest: Vec<&str> = parts.collect();
            let before = self.error_serial;
            self.meas_place(ui, &std::iter::once(item).chain(rest).collect::<Vec<_>>().join(","));
            if self.error_serial == before {
                ui.set_stats_shown(true);
            }
            return None;
        }
        let kind = if kw(first, "MAXimum") {
            0
        } else if kw(first, "MINimum") {
            1
        } else if kw(first, "CURRent") {
            2
        } else if kw(first, "AVERages") {
            3
        } else if kw(first, "DEViation") {
            4
        } else if kw(first, "CNT") || kw(first, "COUNt") {
            5
        } else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", first));
            return t("9.9E37");
        };
        let Some(item) = parts.next() else {
            self.push_err("-224,\"Illegal parameter value\"".into());
            return t("9.9E37");
        };
        let source = |s: &mut Self, word: Option<&str>, fallback: WaveId| -> Option<WaveId> {
            match word.filter(|w| !w.is_empty()) {
                None => Some(fallback),
                Some(word) => s.named_source(word),
            }
        };
        let (n, current, min, max, mean, dev) = if let Some(k) = meas_kind(item) {
            let Some(src) = source(self, parts.next(), ui.meas_source()) else { return t("9.9E37") };
            ui.meas_remote_stat(k, src)
        } else if let Some(k) = pair_kind(item) {
            let Some(a) = source(self, parts.next(), ui.meas_source()) else { return t("9.9E37") };
            let Some(b) = source(self, parts.next(), ui.meas_source_b()) else { return t("9.9E37") };
            ui.pair_remote_stat(k, a, b)
        } else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", item));
            return t("9.9E37");
        };
        let v = match kind {
            0 => max,
            1 => min,
            2 => current,
            3 => mean,
            4 => dev,
            _ => return t(&e(n as f64)),
        };
        t(&e(v))
    }

    /// Put a measurement on the readout. The query form stays in `meas_item`.
    fn meas_place(&mut self, ui: &mut Ui, arg: &str) -> Option<Vec<u8>> {
        self.meas_place_row(ui, arg);
        None
    }

    /// `meas_place`, answering the row that is on the readout afterwards (added, or there already); None after an
    /// error, which is queued.
    fn meas_place_row(&mut self, ui: &mut Ui, arg: &str) -> Option<RowKey> {
        let mut parts = arg.split(',');
        let item = parts.next().unwrap_or("").trim();
        if item.is_empty() {
            self.push_err("-224,\"Illegal parameter value\"".into());
            return None;
        }
        if let Some(k) = meas_kind(item) {
            let (word, fallback) = (parts.next(), ui.meas_source());
            let src = self.source_of(ui, word, fallback)?;
            if parts.next().is_some() {
                self.push_err("-224,\"Too many measurement sources\"".into());
                return None;
            }
            if let Err(reason) = ui.meas_add(k, src) {
                self.push_err(format!("-223,\"{}\"", reason));
                return None;
            }
            return Some(RowKey::One(k, src));
        }
        if let Some(k) = pair_kind(item) {
            let (word, fallback) = (parts.next(), ui.meas_source());
            let a = self.source_of(ui, word, fallback)?;
            let (word, fallback) = (parts.next(), ui.meas_source_b());
            let b = self.source_of(ui, word, fallback)?;
            if parts.next().is_some() {
                self.push_err("-224,\"Too many measurement sources\"".into());
                return None;
            }
            if let Err(reason) = ui.meas_add_pair(k, a, b) {
                self.push_err(format!("-224,\"{}\"", reason));
                return None;
            }
            return Some(RowKey::Pair(k, a, b));
        }
        self.push_err(format!("-224,\"Illegal parameter value\" {}", item));
        None
    }

    /// Burst idle, the side Time Outside counts, how many periods Duration N spans, and which edges it uses.
    /// The DHO programmer manual has no header for these Measure-menu settings.
    fn meas_gate(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        if kw(which, "OUTSide") {
            if query {
                return t(match ui.m.meas_out {
                    oos_dsp::measure::Outside::Above => "ABOV",
                    oos_dsp::measure::Outside::Below => "BEL",
                    oos_dsp::measure::Outside::Both => "BOTH",
                });
            }
            ui.m.meas_out = if kw(arg, "ABOVe") {
                oos_dsp::measure::Outside::Above
            } else if kw(arg, "BELow") {
                oos_dsp::measure::Outside::Below
            } else if kw(arg, "BOTH") {
                oos_dsp::measure::Outside::Both
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
        } else if kw(which, "NPERiods") {
            if query {
                return t(&ui.m.meas_n.to_string());
            }
            let Some(n) = arg.trim().parse::<u32>().ok().filter(|n| (1..=32).contains(n)) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.meas_n = n;
        } else if kw(which, "EDGes") {
            if query {
                return t(if ui.m.meas_data { "DATA" } else { "CLOC" });
            }
            if kw(arg, "CLOCk") {
                ui.m.meas_data = false;
            } else if kw(arg, "DATA") {
                ui.m.meas_data = true;
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            }
        } else {
            self.push_err(format!("-113,\"Undefined header\" :MEASure:{}", which));
            return None;
        }
        ui.measure_last();
        ui.mark_changed();
        None
    }

    /// The labels the screen draws on the trace for the measurements, a row of settings/measure.rs. Both Rigol headers
    /// drive that one switch.
    pub(super) fn meas_indicator(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        self.setting_cmd(ui, crate::settings::measure::INDICATOR, query, arg)
    }

    /// :MEASure:ITEM? <item>[,CHANn|Rn[,src]]
    fn meas_item(&mut self, ui: &Ui, arg: &str) -> Option<Vec<u8>> {
        let mut parts = arg.split(',');
        let item = parts.next().unwrap_or("").trim();
        if item.is_empty() {
            self.push_err("-224,\"Illegal parameter value\"".into());
            return t("9.9E37");
        }
        if kw(item, "SETHold") || item.eq_ignore_ascii_case("SHOLD") {
            let waves = match self.waves(&parts.collect::<Vec<_>>().join(",")) {
                Ok(waves) => waves,
                Err(error) => {
                    self.push_err(error);
                    return t("9.9E37");
                }
            };
            // A is the data, B the clock, as for SETUP and HOLD and on the measurement page
            let (data, clock) = match waves.as_slice() {
                [] => ui.meas_pair_sources(),
                [data, clock] => (*data, *clock),
                _ => {
                    self.push_err("-224,\"Illegal parameter value\"".into());
                    return t("9.9E37");
                }
            };
            return t(&e(ui.fact_setup_hold(data, clock)));
        }
        // no source: CH1, as before. A word that is not CHANn or R1–R10 is an error.
        let word = parts.next().map(str::trim).filter(|s| !s.is_empty());
        if let Some(k) = word.and_then(math_index) {
            // a math channel is not a measurement source of the screen: the item is taken over its whole record
            return match meas_kind(item) {
                Some(kind) => t(&shown(ui.fact_meas_math(kind, k))),
                None => { self.push_err("-224,\"Illegal parameter value\"".into()); t("9.9E37") }
            };
        }
        let src = match word {
            None => ui.meas_source(),
            Some(s) => match self.named_source(s) {
                Some(id) => id,
                None => return t("9.9E37"),
            },
        };
        if let Some(k) = meas_kind(item) {
            return t(&e(ui.fact_meas(k, src)));
        }
        if let Some(k) = pair_kind(item) {
            let Some(word) = parts.next().map(str::trim).filter(|s| !s.is_empty()) else {
                self.push_err("-224,\"Illegal parameter value\"".into());
                return t("9.9E37");
            };
            let Some(b) = self.named_source(word) else { return t("9.9E37") };
            return t(&e(ui.pair_at(k, src, b)));
        }
        self.push_err("-224,\"Illegal parameter value\"".into());
        t("9.9E37")
    }

    /// The source of a row that is added (`:MEASure:ITEM <item>,<source>`): `fallback` without a word, a channel or
    /// reference, or a math trace, which the row reads from the reference slot that holds a copy of it (the way a
    /// cursor reads one), so it is a row of the table like any other.
    fn source_of(&mut self, ui: &mut Ui, word: Option<&str>, fallback: WaveId) -> Option<WaveId> {
        let Some(word) = word.map(str::trim).filter(|w| !w.is_empty()) else { return Some(fallback) };
        if let Some(k) = math_index(word) {
            return match ui.ref_math_slot(k) {
                Ok(slot) => Some(WaveId::Ref(slot)),
                Err(m) => {
                    self.push_err(format!("-221,\"Settings conflict; {m}\""));
                    None
                }
            };
        }
        self.named_source(word)
    }

    fn named_source(&mut self, s: &str) -> Option<WaveId> {
        match parse_wave(s) {
            Some(id) => Some(id),
            None => {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", s));
                None
            }
        }
    }
}
