// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The decoders: `:BUS<n>` (`MODE`, `DISPlay`, `FORMat`, `LABel:CONTent`, `POSition`, `EVENt`, `THReshold`, `DATA?`,
//! and the sources and settings under `RS232`, `IIC` and `SPI`) and `:DECode` (sources and roles, `PROTocol`,
//! `STATe`, `SELect`, `THReshold`, `INPut`, `EVENts?`, `BUSY?`, `CPHY:RATE`). A new decoder command goes into
//! `bus_cmd` or `decode_cmd` here, with its line in docs/scpi.md and its query in ui/tests/scpi.txt (AGENTS.md).

use super::{block, e, kw, nearest_index, num, num64, parse_bool, parse_wave, suffix, t, wave_tok, Scpi};
use crate::model::*;
use crate::protocols::{Decoder, PROTOS};
use crate::ui::Ui;

pub(super) fn bus_index(tok: &str) -> Option<usize> {
    let (k, n) = suffix(tok);
    if kw(k, "BUS") {
        n.filter(|&n| (1..=DECODERS).contains(&n)).map(|n| n - 1)
    } else {
        None
    }
}

fn with_bus(ui: &mut Ui, k: usize, f: impl FnOnce(&mut Model)) {
    let was = ui.m.dec_sel;
    if k != was {
        ui.m.dec_select(k);
    }
    f(&mut ui.m);
    if k != was {
        ui.m.dec_select(was);
    }
    if k == was && ui.m.trig_type == TrigType::Bus {
        ui.mark_changed();
    }
}

fn parse_bus_wave(ui: &Ui, text: &str) -> Option<usize> {
    let Some(wave) = parse_wave(text) else { return parse_math_source(text) };
    match wave {
        WaveId::Ch(c) if c < n_ch() => Some(c),
        WaveId::Ref(i) if ui.ref_info(i).is_some() => Some(DEC_REF_BASE + i),
        _ => None,
    }
}

fn bus_source_name(source: usize) -> String {
    match decode_wave(source) {
        Some(WaveId::Ch(c)) => format!("CHAN{}", c + 1),
        Some(WaveId::Ref(i)) => format!("R{}", i + 1),
        None => decode_math(source).map_or("Unavailable".into(), |k| format!("MATH{}", k + 1)),
    }
}

/// `MATH1`..`MATH8` (or `M1`): the decoder source of a math channel, as `DECoder:...:SOURce` takes it.
pub(super) fn parse_math_source(text: &str) -> Option<usize> {
    let (k, n) = suffix(text.trim());
    (kw(k, "MATH") || k.eq_ignore_ascii_case("M"))
        .then_some(n)
        .flatten()
        .filter(|&n| (1..=crate::math::NMATH).contains(&n))
        .map(|n| DEC_MATH_BASE + n - 1)
}

/// A decoder source named as a channel, a reference or a math channel.
fn parse_dec_source(text: &str) -> Option<usize> {
    match parse_wave(text) {
        Some(WaveId::Ch(c)) => Some(c),
        Some(WaveId::Ref(i)) => Some(DEC_REF_BASE + i),
        None => parse_math_source(text),
    }
}

/// Configure the requested decoder slot, preserving the visible/trigger decoder.
/// Only the active decoder's primary role can take over the acquisition source.
fn put_bus_wave(ui: &mut Ui, slot: usize, protocol: Proto, role: usize, source: Option<usize>) -> bool {
    let selected = ui.m.dec_sel;
    if slot != selected {
        ui.m.dec_select(slot);
    }
    let previous_protocol = ui.m.dec_proto;
    ui.m.dec_proto = protocol;
    let ok = if slot == selected {
        ui.configure_decoder_source(role, source)
    } else {
        ui.m.dec_la_in = false;
        match role {
            0 => ui.m.dec_src = source.unwrap_or(ui.m.dec_src),
            1 => ui.m.dec_src2 = source.unwrap_or(ui.m.dec_src2),
            2 => ui.m.dec_cs = source,
            _ => unreachable!(),
        }
        true
    };
    if !ok {
        ui.m.dec_proto = previous_protocol;
    }
    if slot != selected {
        ui.m.dec_select(selected);
    }
    if ok && slot == selected && ui.m.trig_type == TrigType::Bus {
        ui.mark_changed();
    }
    ok
}

fn proto_token(p: Proto) -> String {
    match p {
        Proto::PARALLEL => "PAR".into(),
        Proto::UART => "RS232".into(),
        Proto::SPI => "SPI".into(),
        Proto::I2C => "IIC".into(),
        Proto::LIN => "LIN".into(),
        Proto::CAN => "CAN".into(),
        other => other.name().chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().to_ascii_uppercase(),
    }
}

pub(super) fn trig_token(ui: &Ui) -> String {
    match ui.m.trig_type {
        TrigType::Edge => "EDGE".into(),
        TrigType::Pulse => "PULS".into(),
        TrigType::Slope => "SLOP".into(),
        TrigType::Video => "VID".into(),
        TrigType::Pattern => "PATT".into(),
        TrigType::Duration => "DUR".into(),
        TrigType::Timeout => "TIM".into(),
        TrigType::Runt => "RUNT".into(),
        TrigType::Window => "WIND".into(),
        TrigType::Delay => "DEL".into(),
        TrigType::SetupHold => "SET".into(),
        TrigType::NthEdge => "NEDG".into(),
        TrigType::Bus => proto_token(ui.m.dec_proto),
    }
}

/// The search marks whatever trigger is already set. Only that name is legal.
pub(super) fn names_trigger(ui: &Ui, arg: &str) -> bool {
    match ui.m.trig_type {
        TrigType::Edge => kw(arg, "EDGE"),
        TrigType::Pulse => kw(arg, "PULSe"),
        TrigType::Slope => kw(arg, "SLOPe"),
        TrigType::Video => kw(arg, "VIDeo"),
        TrigType::Pattern => kw(arg, "PATTern"),
        TrigType::Duration => kw(arg, "DURation"),
        TrigType::Timeout => kw(arg, "TIMeout"),
        TrigType::Runt => kw(arg, "RUNT"),
        TrigType::Window => kw(arg, "WINDow") || kw(arg, "WINDows"),
        TrigType::Delay => kw(arg, "DELay"),
        TrigType::SetupHold => kw(arg, "SETup") || kw(arg, "SHOLd"),
        TrigType::NthEdge => kw(arg, "NEDGe"),
        TrigType::Bus => parse_proto(arg) == Some(ui.m.dec_proto),
    }
}

pub(super) fn parse_proto(arg: &str) -> Option<Proto> {
    let a = arg.trim();
    if kw(a, "PARallel") {
        return Some(Proto::PARALLEL);
    }
    if kw(a, "RS232") || kw(a, "UART") {
        return Some(Proto::UART);
    }
    if kw(a, "IIC") || kw(a, "I2C") {
        return Some(Proto::I2C);
    }
    if kw(a, "LIN") {
        return Some(Proto::LIN);
    }
    if kw(a, "SPI") {
        return Some(Proto::SPI);
    }
    if kw(a, "CAN") {
        return Some(Proto::CAN);
    }
    PROTOS.iter().copied().find(|p| proto_token(*p).eq_ignore_ascii_case(a))
}

impl Scpi {
    /// One of the four decoders. RS232 is the screen's UART. The Event table (Default) or the Events pane (Minimal,
    /// Studio) is one list for every bus.
    pub(super) fn bus_cmd(&mut self, ui: &mut Ui, k: usize, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let n0 = n.first().copied().unwrap_or("");
        let cfg = ui.m.dec_slot(k);
        if n.len() == 1 && kw(n0, "MODE") {
            if query {
                return t(&proto_token(cfg.proto));
            }
            let Some(p) = parse_proto(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            with_bus(ui, k, |m| m.set_dec_proto_only(p));
            return None;
        }
        if n.len() == 1 && kw(n0, "DISPlay") {
            if query {
                return t(if cfg.on { "1" } else { "0" });
            }
            return match parse_bool(arg) {
                Some(on) => {
                    with_bus(ui, k, |m| m.dec_on = on);
                    None
                }
                None => {
                    self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                    None
                }
            };
        }
        if n.len() == 1 && kw(n0, "FORMat") {
            if query {
                return t(if cfg.ascii { "ASC" } else { "HEX" });
            }
            let ascii = if kw(arg, "HEX") {
                Some(false)
            } else if kw(arg, "ASCii") {
                Some(true)
            } else {
                None
            };
            let Some(ascii) = ascii else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            with_bus(ui, k, |m| m.dec_ascii = ascii);
            return None;
        }
        // the name on the band, and the place of the band: divisions from the middle of its window, or BOTTom
        if n.len() == 2 && kw(n0, "LABel") && kw(n[1], "CONTent") {
            if query {
                return t(&cfg.label);
            }
            let label = bus_label(arg.trim().trim_matches('"'));
            with_bus(ui, k, |m| m.dec_label = label);
            return None;
        }
        if n.len() == 1 && kw(n0, "POSition") {
            if query {
                return t(&cfg.pos.map_or("BOTT".to_string(), |p| e(p as f64)));
            }
            let pos = if kw(arg, "BOTTom") { Some(None) } else { num(arg).map(|v| dec_pos_kept(v as f32)) };
            let Some(pos) = pos else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            with_bus(ui, k, |m| m.dec_pos = pos);
            return None;
        }
        if n.len() == 1 && kw(n0, "EVENt") {
            if query {
                return t(if ui.decode_table_on() { "1" } else { "0" });
            }
            return self.set_arg(ui, arg, parse_bool(arg), |ui, on| ui.set_decode_table(on));
        }
        if n.len() == 1 && kw(n0, "THReshold") {
            if query {
                // `? <line>`: the threshold of that line of the decoder (2 to 4), its own or the decoder's
                let line = arg.trim().parse::<usize>().ok().filter(|l| (2..=4).contains(l));
                return t(&e(line.and_then(|l| cfg.thr_line[l - 2]).unwrap_or(cfg.thr) as f64));
            }
            let num = arg.split(',').next_back().unwrap_or("").trim();
            // once, each line to the middle of its own signal
            if kw(num, "AUTO") {
                match ui.dec_signal_middles(&ui.m.dec_view(k)) {
                    Some((main, lines)) => with_bus(ui, k, |m| {
                        m.dec_thr = main;
                        for (k, t) in lines.into_iter().enumerate() {
                            if t.is_some() {
                                m.dec_thr_line[k] = t;
                            }
                        }
                    }),
                    None => self.push_err("-230,\"No signal on the decoder's source\"".to_string()),
                }
                return None;
            }
            // `<line>,<volts>`: the threshold of line 2 to 4 alone (`<line>,DEF`: the decoder's again)
            let line =
                arg.split_once(',').and_then(|(l, _)| l.trim().parse::<usize>().ok()).filter(|l| (2..=4).contains(l));
            if let Some(l) = line {
                let own = if kw(num, "DEFault") { Some(None) } else { num64(num).map(|v| Some(v as f32)) };
                match own {
                    Some(v) => with_bus(ui, k, |m| m.dec_thr_line[l - 2] = v),
                    None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
                }
                return None;
            }
            let Some(v) = num64(num) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            with_bus(ui, k, |m| m.dec_thr = v as f32);
            return None;
        }
        if n.len() == 1 && kw(n0, "DATA") && query {
            return self.bus_data(ui, k);
        }
        if n.len() == 2 && kw(n0, "RS232") {
            return self.bus_rs232(ui, k, n[1], query, arg);
        }
        if n.len() == 3 && kw(n0, "IIC") && kw(n[2], "SOURce") && (kw(n[1], "SCLK") || kw(n[1], "SDA")) {
            return self.bus_iic_source(ui, k, n, query, arg);
        }
        if n.len() == 3
            && kw(n0, "SPI")
            && kw(n[2], "SOURce")
            && (kw(n[1], "SCLK") || kw(n[1], "MOSI") || kw(n[1], "SS"))
        {
            return self.bus_spi_source(ui, k, n, query, arg);
        }
        if n.len() == 2 && kw(n0, "SPI") && kw(n[1], "DBITs") {
            return self.bus_bits(ui, k, Proto::SPI, 4..=32, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :BUS{}:{}", k + 1, n.join(":")));
        None
    }

    /// `:BUS<n>:DATA?`: the decoder's protocol and its rows of the event table.
    fn bus_data(&mut self, ui: &Ui, k: usize) -> Option<Vec<u8>> {
        let cfg = ui.m.dec_slot(k);
        let events = match ui.decode_events_result() {
            Ok(events) => events,
            Err("Decode computing") => return t("BUSY"),
            Err(error) => {
                self.push_err(format!("-230,\"{}\"", error));
                return None;
            }
        };
        let prefix = format!("D{} ", k + 1);
        let rows = events
            .lines()
            .filter(|line| line.splitn(3, ',').nth(2).is_some_and(|rest| rest.starts_with(&prefix)))
            .collect::<Vec<_>>()
            .join("\n");
        Some(block(format!("{}\n{}", proto_token(cfg.proto), rows).as_bytes()))
    }

    /// `:BUS<n>:IIC:SCLK|SDA:SOURce` (`n` without the root).
    fn bus_iic_source(&mut self, ui: &mut Ui, k: usize, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let cfg = ui.m.dec_slot(k);
        let clock = kw(n[1], "SCLK");
        if query {
            return t(&bus_source_name(if clock { cfg.src2 } else { cfg.src }));
        }
        let Some(source) = parse_bus_wave(ui, arg) else {
            self.push_err(format!("-224,\"Decoder source is unavailable\" {}", arg));
            return None;
        };
        if !put_bus_wave(ui, k, Proto::I2C, usize::from(clock), Some(source)) {
            self.push_err("-224,\"Decoder source is unavailable\"".into());
        }
        None
    }

    /// `:BUS<n>:SPI:SCLK|MOSI|SS:SOURce` (`n` without the root); SS takes OFF.
    fn bus_spi_source(&mut self, ui: &mut Ui, k: usize, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let cfg = ui.m.dec_slot(k);
        let role = if kw(n[1], "SCLK") {
            1
        } else if kw(n[1], "MOSI") {
            0
        } else {
            2
        };
        if query {
            return t(&match role {
                0 => bus_source_name(cfg.src),
                1 => bus_source_name(cfg.src2),
                _ => cfg.cs.map(bus_source_name).unwrap_or_else(|| "OFF".into()),
            });
        }
        let source = if role == 2 && (kw(arg, "OFF") || kw(arg, "NONE")) {
            None
        } else {
            match parse_bus_wave(ui, arg) {
                Some(source) => Some(source),
                None => {
                    self.push_err(format!("-224,\"Decoder source is unavailable\" {}", arg));
                    return None;
                }
            }
        };
        if !put_bus_wave(ui, k, Proto::SPI, role, source) {
            self.push_err("-224,\"Decoder source is unavailable\"".into());
        }
        None
    }

    fn bus_rs232(&mut self, ui: &mut Ui, k: usize, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let cfg = ui.m.dec_slot(k);
        if kw(which, "TX") || kw(which, "RX") {
            let role = usize::from(kw(which, "RX"));
            if query {
                return t(&bus_source_name(if role == 1 { cfg.src2 } else { cfg.src }));
            }
            let Some(source) = parse_bus_wave(ui, arg) else {
                self.push_err(format!("-224,\"Decoder source is unavailable\" {}", arg));
                return None;
            };
            if !put_bus_wave(ui, k, Proto::UART, role, Some(source)) {
                self.push_err("-224,\"Decoder source is unavailable\"".into());
            }
            return None;
        }
        if kw(which, "BAUD") {
            if query {
                return t(&format!("{}", crate::decode::BAUDS[cfg.baud.min(crate::decode::BAUDS.len() - 1)] as u64));
            }
            let Some(v) = num64(arg).filter(|v| *v > 0.0) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            let i = nearest_index(&crate::decode::BAUDS, v);
            with_bus(ui, k, |m| {
                m.dec_proto = Proto::UART;
                m.dec_baud = i;
            });
            return None;
        }
        if kw(which, "PARity") {
            if query {
                return t(match cfg.parity {
                    oos_dsp::decode::Parity::None => "NONE",
                    oos_dsp::decode::Parity::Odd => "ODD",
                    oos_dsp::decode::Parity::Even => "EVEN",
                });
            }
            let p = if kw(arg, "NONE") {
                Some(oos_dsp::decode::Parity::None)
            } else if kw(arg, "ODD") {
                Some(oos_dsp::decode::Parity::Odd)
            } else if kw(arg, "EVEN") {
                Some(oos_dsp::decode::Parity::Even)
            } else {
                None
            };
            let Some(p) = p else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            with_bus(ui, k, |m| {
                m.dec_proto = Proto::UART;
                m.dec_parity = p;
            });
            return None;
        }
        if kw(which, "DBITs") {
            return self.bus_bits(ui, k, Proto::UART, 5..=9, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :BUS{}:RS232:{}", k + 1, which));
        None
    }

    /// `:BUS<n>:SPI:DBITs` and `:BUS<n>:RS232:DBITs`: the word length of bus `k`, which a command sets to one of
    /// `bits` with the protocol `proto`.
    fn bus_bits(
        &mut self,
        ui: &mut Ui,
        k: usize,
        proto: Proto,
        bits: std::ops::RangeInclusive<u32>,
        query: bool,
        arg: &str,
    ) -> Option<Vec<u8>> {
        if query {
            return t(&format!("{}", ui.m.dec_slot(k).bits));
        }
        let Some(v) = arg.trim().parse::<u32>().ok().filter(|v| bits.contains(v)) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        with_bus(ui, k, |m| {
            m.dec_proto = proto;
            m.dec_bits = v;
        });
        None
    }

    pub(super) fn decode_cmd(&mut self, ui: &mut Ui, node: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        if kw(node, "SRATe") {
            return self.decode_cphy_rate_cmd(ui, query, arg);
        }
        let text = |value: String| Some(value.into_bytes());
        let illegal = |s: &mut Self| {
            s.push_err(format!("-224,\"Illegal decoder parameter\" {}", arg));
            None
        };
        if kw(node, "EVENts") && query {
            return match ui.decode_events_result() {
                Ok(events) => text(events),
                Err("Decode computing") => text("BUSY".into()),
                Err(error) => {
                    self.push_err(format!("-230,\"{}\"", error));
                    None
                }
            };
        }
        if kw(node, "BUSY") && query {
            return text(if ui.decode_busy() { "1" } else { "0" }.into());
        }
        let role = if kw(node, "SOURce") || kw(node, "SOURce1") || node.eq_ignore_ascii_case("SOUR1") {
            Some(0)
        } else if kw(node, "SOURce2") || node.eq_ignore_ascii_case("SOUR2") {
            Some(1)
        } else if kw(node, "CS") {
            Some(2)
        } else if kw(node, "SOURce4") || node.eq_ignore_ascii_case("SOUR4") {
            Some(3)
        } else if kw(node, "DIFFerential") {
            Some(4)
        } else {
            None
        };
        if let Some(role) = role {
            if query {
                if ui.m.dec_on_la() && role < 4 {
                    return text(ui.m.dec_la[role].map_or("NONE".into(), |line| format!("D{line}")));
                }
                let source = match role {
                    0 => Some(ui.m.dec_src),
                    1 => Some(ui.m.dec_src2),
                    2 => ui.m.dec_cs,
                    3 => ui.m.dec_src4,
                    _ => ui.m.dec_diff,
                };
                // as the other queries name a source (CHAN3, R1), which the setter takes back
                return text(source.filter(|&s| decode_valid(s)).map_or("NONE".into(), |s| match decode_wave(s) {
                    Some(wave) => wave_tok(wave),
                    None => bus_source_name(s),
                }));
            }
            if ui.m.dec_on_la() && role < 4 {
                let source = arg
                    .strip_prefix('D')
                    .or_else(|| arg.strip_prefix('d'))
                    .and_then(|v| v.parse::<u8>().ok())
                    .filter(|&v| v < 16);
                if source.is_none() && !(role >= 2 && arg.eq_ignore_ascii_case("NONE")) {
                    return illegal(self);
                }
                ui.m.dec_la[role] = source;
                ui.mark_changed();
                return None;
            }
            let source = if arg.eq_ignore_ascii_case("NONE") {
                None
            } else {
                match parse_dec_source(arg) {
                    Some(source) => Some(source),
                    None => return illegal(self),
                }
            };
            if !ui.configure_decoder_source(role, source) {
                return illegal(self);
            }
            ui.mark_changed();
            return None;
        }
        if kw(node, "PROTocol") || node.eq_ignore_ascii_case("PROTO") {
            if query {
                return text(ui.m.dec_proto.name().into());
            }
            let compact = |s: &str| {
                s.chars().filter(|c| c.is_ascii_alphanumeric()).flat_map(char::to_uppercase).collect::<String>()
            };
            let name = compact(arg);
            let Some(protocol) = PROTOS.iter().copied().find(|p| compact(p.name()) == name) else {
                return illegal(self);
            };
            // SCPI configures an instrument, not the demo's suggested sources/trigger.
            ui.m.set_dec_proto_only(protocol);
            if protocol == Proto::CPHY {
                ui.m.dec_thr = 0.02;
                ui.m.dec_la_in = false;
            }
            ui.mark_changed();
            return None;
        }
        if kw(node, "STATe") {
            if query {
                return text(if ui.m.dec_on { "1" } else { "0" }.into());
            }
            let Some(on) = parse_bool(arg) else { return illegal(self) };
            ui.m.dec_on = on;
            ui.mark_changed();
            return None;
        }
        if kw(node, "SELect") {
            if query {
                return text((ui.m.dec_sel + 1).to_string());
            }
            let Some(slot) = arg.parse::<usize>().ok().filter(|n| (1..=DECODERS).contains(n)) else {
                return illegal(self);
            };
            ui.m.dec_select(slot - 1);
            return None;
        }
        if kw(node, "THReshold") {
            if query {
                return text(e(ui.m.dec_thr as f64));
            }
            // once, to the middle of the signal on the decoder's source
            if kw(arg.trim(), "AUTO") {
                match ui.dec_signal_middles(&ui.m) {
                    Some((main, lines)) => {
                        ui.m.dec_thr = main;
                        for (k, t) in lines.into_iter().enumerate() {
                            if t.is_some() {
                                ui.m.dec_thr_line[k] = t;
                            }
                        }
                        ui.mark_changed();
                    }
                    None => self.push_err("-230,\"No signal on the decoder's source\"".to_string()),
                }
                return None;
            }
            let Some(threshold) = num(arg) else { return illegal(self) };
            ui.m.dec_thr = threshold;
            ui.mark_changed();
            return None;
        }
        if kw(node, "INPut") {
            if query {
                return text(if ui.m.dec_la_in { "LA" } else { "WAVEFORM" }.into());
            }
            if arg.eq_ignore_ascii_case("LA") && ui.m.la_on() && ui.m.dec_proto != Proto::CPHY {
                ui.m.dec_la_in = true;
            } else if arg.eq_ignore_ascii_case("WAVEFORM") || arg.eq_ignore_ascii_case("ANALOG") {
                ui.m.dec_la_in = false;
            } else {
                return illegal(self);
            }
            ui.mark_changed();
            return None;
        }
        self.push_err(format!("-113,\"Undefined header\" DECode:{}", node));
        None
    }

    pub(super) fn decode_cphy_rate_cmd(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            return Some(if ui.m.dec_cphy_rate == 0.0 { b"AUTO".to_vec() } else { e(ui.m.dec_cphy_rate).into_bytes() });
        }
        match crate::decode::parse_cphy_rate(arg) {
            Ok(rate) => {
                ui.m.dec_cphy_rate = rate;
                ui.mark_changed();
            }
            Err(_) => self.push_err(format!("-224,\"Invalid C-PHY symbol rate\" {}", arg)),
        }
        None
    }
}
