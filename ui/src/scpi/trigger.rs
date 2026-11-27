// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! oos-plus: the trigger types of the screen over SCPI (`:TRIGger:MODE` by name and the `:TRIGger:PULSe`, `SLOPe`,
//! `VIDeo`, `PATTern`, `DURation`, `TIMeout`, `RUNT`, `WINDows`, `DELay`, `SHOLd`, `NEDGe` and `ZONE<n>` subtrees),
//! with `:RECord`, `:NAVigate` and `:SEARch`. Upstream OOS has none of these; they came from oos-plus's scpi.rs and
//! are wired from `exec_cmd` (scpi/dispatch.rs). Each has its line in docs/scpi.md.

use super::decode::{names_trigger, parse_proto, trig_token};
use super::{e, kw, num, parse_bool, parse_ch, Scpi};
use crate::model::*;
use crate::ui::Ui;

/// At least `pairs` vertices, each a time and a voltage. A box is exactly two.
fn point_list(arg: &str, pairs: usize) -> Option<Vec<(f64, f64)>> {
    let nums: Vec<f64> = arg.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| s.parse::<f64>().ok()).collect::<Option<_>>()?;
    if nums.len() < pairs * 2 || nums.len() % 2 != 0 || nums.iter().any(|n| !n.is_finite()) {
        return None;
    }
    if pairs == 2 && nums.len() != 4 {
        return None;
    }
    Some(nums.chunks(2).map(|c| (c[0], c[1])).collect())
}

fn point_text(pts: &[(f64, f64)]) -> String {
    pts.iter().flat_map(|(t, v)| [e(*t), e(*v)]).collect::<Vec<_>>().join(",")
}

/// Which time a trigger command writes.
pub(super) enum TimeSlot {
    Limit,
    Setup,
    Hold,
    Idle,
}

fn slope_token(s: Slope) -> &'static str {
    match s {
        Slope::Rise => "POS",
        Slope::Fall => "NEG",
        Slope::Either => "RFAL",
    }
}

fn parse_slope(arg: &str, allow_either: bool) -> Option<Slope> {
    if kw(arg, "POSitive") {
        Some(Slope::Rise)
    } else if kw(arg, "NEGative") {
        Some(Slope::Fall)
    } else if allow_either && kw(arg, "RFALl") {
        Some(Slope::Either)
    } else {
        None
    }
}

/// One letter per analog channel. An edge is rise or fall, and only the first edge is kept.
fn pattern_bits(arg: &str, edges: bool) -> Option<[Bit; NCH]> {
    let parts: Vec<&str> = arg.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if parts.len() != NCH {
        return None;
    }
    let mut bits = [Bit::X; NCH];
    let mut edge = false;
    for (i, p) in parts.iter().enumerate() {
        let b = if p.eq_ignore_ascii_case("H") {
            Bit::H
        } else if p.eq_ignore_ascii_case("L") {
            Bit::L
        } else if p.eq_ignore_ascii_case("X") {
            Bit::X
        } else if edges && p.eq_ignore_ascii_case("R") {
            Bit::R
        } else if edges && p.eq_ignore_ascii_case("F") {
            Bit::F
        } else {
            return None;
        };
        if matches!(b, Bit::R | Bit::F) && edge {
            bits[i] = Bit::X;
        } else {
            if matches!(b, Bit::R | Bit::F) {
                edge = true;
            }
            bits[i] = b;
        }
    }
    Some(bits)
}

fn pattern_text(bits: &[Bit; NCH], edges: bool) -> String {
    bits.iter()
        .map(|b| match b {
            Bit::H => "H",
            Bit::L => "L",
            Bit::R if edges => "R",
            Bit::F if edges => "F",
            _ => "X",
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The line knob stops at half the standard's lines, ten short of the last one.
fn video_line_max(std: VideoStd) -> u32 {
    (std.lines() / 2).saturating_sub(10).max(1)
}

impl Scpi {
    /// The trigger-type knob, including a protocol trigger by the decoder's name.
    pub(super) fn trigger_mode(&mut self, ui: &mut Ui, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if query {
            return t(&trig_token(ui));
        }
        let kind = if kw(arg, "EDGE") {
            Some(TrigType::Edge)
        } else if kw(arg, "PULSe") {
            Some(TrigType::Pulse)
        } else if kw(arg, "SLOPe") {
            Some(TrigType::Slope)
        } else if kw(arg, "VIDeo") {
            Some(TrigType::Video)
        } else if kw(arg, "PATTern") {
            Some(TrigType::Pattern)
        } else if kw(arg, "DURation") {
            Some(TrigType::Duration)
        } else if kw(arg, "TIMeout") {
            Some(TrigType::Timeout)
        } else if kw(arg, "RUNT") {
            Some(TrigType::Runt)
        } else if kw(arg, "WINDow") {
            Some(TrigType::Window)
        } else if kw(arg, "DELay") {
            Some(TrigType::Delay)
        } else if kw(arg, "SETup") {
            Some(TrigType::SetupHold)
        } else if kw(arg, "NEDGe") {
            Some(TrigType::NthEdge)
        } else {
            None
        };
        if let Some(kind) = kind {
            ui.trig_type_set(kind);
            ui.mark_changed();
            return None;
        }
        if let Some(p) = parse_proto(arg) {
            ui.trig_bus_set(p);
            ui.mark_changed();
            return None;
        }
        self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
        None
    }

    /// Pulse width, polarity, source, and level. The width that the When row is not using returns -221.
    pub(super) fn trigger_pulse(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SOURce") {
            if query {
                return t(&ui.m.trig_line().map_or(format!("CHAN{}", ui.m.trig_src + 1), |n| format!("D{}", n)));
            }
            let word = arg.trim();
            if let Some(n) = word.to_ascii_uppercase().strip_prefix('D').and_then(|n| n.parse::<u8>().ok()).filter(|&n| n < 16) {
                if !ui.m.la_on() {
                    self.push_err("-224,\"Illegal parameter value\" no logic analyser".into());
                    return None;
                }
                ui.m.trig_type = TrigType::Pulse;
                ui.m.trig_la = Some(n);
                ui.mark_changed();
                return None;
            }
            let Some(c) = parse_ch(word) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Pulse;
            ui.m.ch[c].on = true;
            ui.m.trig_src = c;
            ui.m.trig_la = None;
            ui.mark_changed();
            return None;
        }
        if kw(which, "POLarity") {
            if query {
                return t(if ui.m.trig_pol == Pol::Pos { "POS" } else { "NEG" });
            }
            ui.m.trig_type = TrigType::Pulse;
            ui.m.trig_pol = if kw(arg, "POSitive") {
                Pol::Pos
            } else if kw(arg, "NEGative") {
                Pol::Neg
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.mark_changed();
            return None;
        }
        if kw(which, "WHEN") {
            if query {
                return t(match ui.m.trig_when {
                    WhenKind::Gt => "GRE",
                    WhenKind::Lt => "LESS",
                    WhenKind::In => "GLES",
                    WhenKind::Out => "OUT",
                    WhenKind::Off => "OFF",
                });
            }
            ui.m.trig_type = TrigType::Pulse;
            ui.m.trig_when = if kw(arg, "GREater") {
                WhenKind::Gt
            } else if kw(arg, "LESS") {
                WhenKind::Lt
            } else if kw(arg, "GLESs") {
                WhenKind::In
            } else if kw(arg, "OUTside") {
                WhenKind::Out
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.mark_changed();
            return None;
        }
        if kw(which, "LEVel") {
            if query {
                return t(&e(ui.m.trig_level as f64));
            }
            let Some(v) = num(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Pulse;
            ui.m.trig_level = v;
            ui.mark_changed();
            return None;
        }
        if kw(which, "UWIDth") || kw(which, "LWIDth") {
            return self.when_width(ui, kw(which, "UWIDth"), query, arg, TrigType::Pulse);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:PULSe:{}", which));
        None
    }

    /// Slope time between the upper and lower levels. The screen always uses both levels.
    pub(super) fn trigger_slope(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SOURce") {
            if query {
                return t(&format!("CHAN{}", ui.m.trig_src + 1));
            }
            let Some(c) = parse_ch(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Slope;
            ui.m.ch[c].on = true;
            ui.m.trig_src = c;
            ui.m.trig_la = None;
            ui.mark_changed();
            return None;
        }
        if kw(which, "POLarity") {
            if query {
                return t(match ui.m.slope {
                    Slope::Rise => "POS",
                    Slope::Fall => "NEG",
                    Slope::Either => "RFAL",
                });
            }
            ui.m.trig_type = TrigType::Slope;
            ui.m.slope = if kw(arg, "POSitive") {
                Slope::Rise
            } else if kw(arg, "NEGative") {
                Slope::Fall
            } else if kw(arg, "RFAL") || kw(arg, "EITHer") {
                Slope::Either
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.mark_changed();
            return None;
        }
        if kw(which, "WHEN") {
            if query {
                return t(match ui.m.trig_when {
                    WhenKind::Gt => "GRE",
                    WhenKind::Lt => "LESS",
                    WhenKind::In => "GLES",
                    WhenKind::Out => "OUT",
                    WhenKind::Off => "OFF",
                });
            }
            ui.m.trig_type = TrigType::Slope;
            ui.m.trig_when = if kw(arg, "GREater") {
                WhenKind::Gt
            } else if kw(arg, "LESS") {
                WhenKind::Lt
            } else if kw(arg, "GLESs") {
                WhenKind::In
            } else if kw(arg, "OUTside") {
                WhenKind::Out
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.mark_changed();
            return None;
        }
        if kw(which, "WINDow") {
            if query {
                return t("TAB");
            }
            if kw(arg, "TAB") {
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" both levels are set".into());
            return None;
        }
        if kw(which, "ALEVel") || kw(which, "BLEVel") {
            let high = kw(which, "ALEVel");
            if query {
                return t(&e(if high { ui.m.trig_level } else { ui.m.trig_level2 } as f64));
            }
            let Some(v) = num(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Slope;
            if high {
                ui.m.trig_level = v;
            } else {
                ui.m.trig_level2 = v;
            }
            ui.mark_changed();
            return None;
        }
        if kw(which, "TUPPer") || kw(which, "TLOWer") {
            return self.when_width(ui, kw(which, "TUPPer"), query, arg, TrigType::Slope);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:SLOPe:{}", which));
        None
    }

    /// PAL or NTSC, the sync the screen shows, and the line number while that sync is Line.
    pub(super) fn trigger_video(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SOURce") {
            if query {
                return t(&format!("CHAN{}", ui.m.trig_src + 1));
            }
            let Some(c) = parse_ch(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Video;
            ui.m.ch[c].on = true;
            ui.m.trig_src = c;
            ui.m.trig_la = None;
            ui.mark_changed();
            return None;
        }
        if kw(which, "POLarity") {
            // The hardware programs one polarity. The video menu has no polarity row.
            if query {
                return t("POS");
            }
            if kw(arg, "POSitive") {
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" video polarity is fixed".into());
            return None;
        }
        if kw(which, "STANdard") {
            if query {
                return t(if ui.m.video_std == VideoStd::Pal { "PALS" } else { "NTSC" });
            }
            let std = if kw(arg, "PALSecam") || arg.trim().eq_ignore_ascii_case("PAL") {
                Some(VideoStd::Pal)
            } else if kw(arg, "NTSC") {
                Some(VideoStd::Ntsc)
            } else {
                None
            };
            let Some(std) = std else {
                self.push_err("-224,\"Illegal parameter value\" pal or ntsc".into());
                return None;
            };
            ui.m.trig_type = TrigType::Video;
            ui.m.video_std = std;
            let max = video_line_max(std);
            if ui.m.video_line > max {
                ui.m.video_line = max;
            }
            ui.mark_changed();
            return None;
        }
        if kw(which, "MODE") {
            if query {
                return t(match ui.m.video_sync {
                    VSync::AllLines => "ALIN",
                    VSync::Line => "LINE",
                    VSync::Odd => "ODDF",
                    VSync::Even => "EVEN",
                    VSync::AllFields => "FIELD",
                });
            }
            let sync = if kw(arg, "ALINes") {
                Some(VSync::AllLines)
            } else if kw(arg, "LINE") {
                Some(VSync::Line)
            } else if kw(arg, "ODDField") {
                Some(VSync::Odd)
            } else if kw(arg, "EVENfield") {
                Some(VSync::Even)
            } else if kw(arg, "FIELds") {
                Some(VSync::AllFields)
            } else {
                None
            };
            let Some(sync) = sync else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Video;
            ui.m.video_sync = sync;
            ui.mark_changed();
            return None;
        }
        if kw(which, "LINE") {
            if ui.m.video_sync != VSync::Line {
                self.push_err("-221,\"Settings conflict\"".into());
                return if query { t("9.9E37") } else { None };
            }
            if query {
                return t(&ui.m.video_line.to_string());
            }
            let max = video_line_max(ui.m.video_std);
            let Some(n) = arg.trim().parse::<u32>().ok().filter(|&n| (1..=max).contains(&n)) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Video;
            ui.m.video_line = n;
            ui.mark_changed();
            return None;
        }
        if kw(which, "LEVel") {
            if query {
                return t(&e(ui.m.trig_level as f64));
            }
            let Some(v) = num(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Video;
            ui.m.trig_level = v;
            ui.mark_changed();
            return None;
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:VIDeo:{}", which));
        None
    }

    /// H, L, X, rise, or fall on each analog channel. A second edge becomes X, which is the edge the trigger keeps.
    pub(super) fn trigger_pattern(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "PATTern") {
            if query {
                return t(&pattern_text(&ui.m.pat, true));
            }
            let Some(bits) = pattern_bits(arg, true) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Pattern;
            ui.m.pat = bits;
            ui.mark_changed();
            return None;
        }
        if kw(which, "SOURce") {
            return self.trigger_line(ui, TrigType::Pattern, query, arg);
        }
        if kw(which, "LEVel") {
            return self.pattern_level(ui, TrigType::Pattern, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:PATTern:{}", which));
        None
    }

    /// The same channel bits without an edge, plus the time the pattern must last.
    pub(super) fn trigger_duration(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "TYPE") {
            if query {
                return t(&pattern_text(&ui.m.pat, false));
            }
            let Some(bits) = pattern_bits(arg, false) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Duration;
            ui.m.pat = bits;
            ui.mark_changed();
            return None;
        }
        if kw(which, "SOURce") {
            return self.trigger_line(ui, TrigType::Duration, query, arg);
        }
        if kw(which, "LEVel") {
            return self.pattern_level(ui, TrigType::Duration, query, arg);
        }
        if kw(which, "WHEN") {
            if query {
                return t(match ui.m.trig_when {
                    WhenKind::Gt => "GRE",
                    WhenKind::Lt => "LESS",
                    WhenKind::In => "GLES",
                    WhenKind::Out | WhenKind::Off => "UNGL",
                });
            }
            let w = if kw(arg, "GREater") {
                WhenKind::Gt
            } else if kw(arg, "LESS") {
                WhenKind::Lt
            } else if kw(arg, "GLESs") {
                WhenKind::In
            } else if kw(arg, "UNGLess") || kw(arg, "OUTSide") {
                WhenKind::Out
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Duration;
            ui.m.trig_when = w;
            ui.mark_changed();
            return None;
        }
        if kw(which, "TUPPer") {
            return self.when_width(ui, true, query, arg, TrigType::Duration);
        }
        if kw(which, "TLOWer") {
            return self.when_width(ui, false, query, arg, TrigType::Duration);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:DURation:{}", which));
        None
    }

    /// The level source: an analog channel, or a logic line while the logic analyser is on.
    pub(super) fn trigger_line(&mut self, ui: &mut Ui, kind: TrigType, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if query {
            return t(&ui.m.trig_line().map_or(format!("CHAN{}", ui.m.trig_src + 1), |n| format!("D{}", n)));
        }
        let digital = arg.trim().to_ascii_uppercase().strip_prefix('D').and_then(|n| n.parse::<u8>().ok()).filter(|&n| n < 16 && ui.m.trig_la_ok());
        if let Some(n) = digital {
            ui.m.trig_type = kind;
            ui.m.trig_la = Some(n);
            ui.mark_changed();
            return None;
        }
        let Some(c) = parse_ch(arg) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        ui.m.ch[c].on = true;
        ui.m.trig_src = c;
        ui.m.trig_la = None;
        ui.mark_changed();
        None
    }

    /// One threshold per analog channel. The source names which channel.
    pub(super) fn pattern_level(&mut self, ui: &mut Ui, kind: TrigType, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let mut parts = arg.split(',').map(str::trim).filter(|s| !s.is_empty());
        let Some(c) = parts.next().and_then(parse_ch) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        if query {
            return t(&e(ui.m.pat_levels[c] as f64));
        }
        let Some(v) = parts.next().and_then(num) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        ui.m.pat_levels[c] = v;
        ui.mark_changed();
        None
    }

    /// A quiet stretch longer than the timeout, on one edge or on either edge.
    pub(super) fn trigger_timeout(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        if kw(which, "SOURce") {
            return self.trigger_line(ui, TrigType::Timeout, query, arg);
        }
        if kw(which, "SLOPe") {
            return self.edge_slope(ui, TrigType::Timeout, false, true, query, arg);
        }
        if kw(which, "TIME") {
            return self.positive_time(ui, TrigType::Timeout, query, arg, TimeSlot::Limit);
        }
        if kw(which, "LEVel") {
            return self.pair_level(ui, TrigType::Timeout, true, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:TIMeout:{}", which));
        None
    }

    /// A pulse that crosses one level and turns back before the other. None means any width.
    pub(super) fn trigger_runt(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SOURce") {
            return self.trigger_line(ui, TrigType::Runt, query, arg);
        }
        if kw(which, "POLarity") {
            return self.pulse_polarity(ui, TrigType::Runt, query, arg);
        }
        if kw(which, "WHEN") {
            if query {
                return t(match ui.m.trig_when {
                    WhenKind::Off => "NONE",
                    WhenKind::Gt => "GRE",
                    WhenKind::Lt => "LESS",
                    WhenKind::In => "GLES",
                    WhenKind::Out => "OUT",
                });
            }
            let w = if kw(arg, "NONE") {
                WhenKind::Off
            } else if kw(arg, "GREater") {
                WhenKind::Gt
            } else if kw(arg, "LESS") {
                WhenKind::Lt
            } else if kw(arg, "GLESs") {
                WhenKind::In
            } else if kw(arg, "OUTSide") {
                WhenKind::Out
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Runt;
            ui.m.trig_when = w;
            ui.mark_changed();
            return None;
        }
        if kw(which, "WUPPer") {
            return self.when_width(ui, true, query, arg, TrigType::Runt);
        }
        if kw(which, "WLOWer") {
            return self.when_width(ui, false, query, arg, TrigType::Runt);
        }
        if kw(which, "ALEVel") {
            return self.pair_level(ui, TrigType::Runt, true, query, arg);
        }
        if kw(which, "BLEVel") {
            return self.pair_level(ui, TrigType::Runt, false, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:RUNT:{}", which));
        None
    }

    /// Enter, exit, or time spent inside the two levels. The edge is always either, which is how the window is drawn.
    pub(super) fn trigger_window(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SOURce") {
            return self.trigger_line(ui, TrigType::Window, query, arg);
        }
        if kw(which, "SLOPe") {
            if query {
                return t("RFAL");
            }
            if kw(arg, "RFALl") {
                ui.m.trig_type = TrigType::Window;
                ui.mark_changed();
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" window edge is fixed".into());
            return None;
        }
        if kw(which, "POSition") {
            if query {
                return t(match ui.m.win_mode {
                    WinMode::Enter => "ENT",
                    WinMode::Exit => "EXIT",
                    WinMode::Inside => "TIME",
                });
            }
            let mode = if kw(arg, "ENTer") {
                Some(WinMode::Enter)
            } else if kw(arg, "EXIT") {
                Some(WinMode::Exit)
            } else if kw(arg, "TIME") {
                Some(WinMode::Inside)
            } else {
                None
            };
            let Some(mode) = mode else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Window;
            ui.m.win_mode = mode;
            ui.mark_changed();
            return None;
        }
        if kw(which, "TIME") || kw(which, "WHEN") {
            if ui.m.win_mode != WinMode::Inside {
                self.push_err("-221,\"Settings conflict\"".into());
                return if query { t("9.9E37") } else { None };
            }
        }
        if kw(which, "TIME") {
            return self.positive_time(ui, TrigType::Window, query, arg, TimeSlot::Limit);
        }
        if kw(which, "WHEN") {
            if query {
                return t(match ui.m.trig_when {
                    WhenKind::Gt | WhenKind::Off => "GRE",
                    WhenKind::Lt => "LESS",
                    WhenKind::In => "GLES",
                    WhenKind::Out => "OUT",
                });
            }
            let w = if kw(arg, "GREater") {
                WhenKind::Gt
            } else if kw(arg, "LESS") {
                WhenKind::Lt
            } else if kw(arg, "GLESs") {
                WhenKind::In
            } else if kw(arg, "OUTSide") {
                WhenKind::Out
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Window;
            ui.m.trig_when = w;
            ui.mark_changed();
            return None;
        }
        if kw(which, "ALEVel") {
            return self.pair_level(ui, TrigType::Window, true, query, arg);
        }
        if kw(which, "BLEVel") {
            return self.pair_level(ui, TrigType::Window, false, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:WINDows:{}", which));
        None
    }

    /// Source A, then source B, and how long may pass between those edges.
    pub(super) fn trigger_delay(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SA") {
            return self.trigger_line(ui, TrigType::Delay, query, arg);
        }
        if kw(which, "SB") {
            return self.source_b(ui, TrigType::Delay, query, arg);
        }
        if kw(which, "ASLop") {
            return self.edge_slope(ui, TrigType::Delay, false, true, query, arg);
        }
        if kw(which, "BSLop") {
            return self.edge_slope(ui, TrigType::Delay, true, true, query, arg);
        }
        if kw(which, "TYPE") {
            if query {
                return t(match ui.m.trig_when {
                    WhenKind::Gt | WhenKind::Off => "GRE",
                    WhenKind::Lt => "LESS",
                    WhenKind::In => "GLES",
                    WhenKind::Out => "GOUT",
                });
            }
            let w = if kw(arg, "GREater") {
                WhenKind::Gt
            } else if kw(arg, "LESS") {
                WhenKind::Lt
            } else if kw(arg, "GLESs") {
                WhenKind::In
            } else if kw(arg, "GOUT") || kw(arg, "OUTSide") {
                WhenKind::Out
            } else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::Delay;
            ui.m.trig_when = w;
            ui.mark_changed();
            return None;
        }
        if kw(which, "TUPPer") {
            return self.when_width(ui, true, query, arg, TrigType::Delay);
        }
        if kw(which, "TLOWer") {
            return self.when_width(ui, false, query, arg, TrigType::Delay);
        }
        if kw(which, "ALEVel") {
            return self.pair_level(ui, TrigType::Delay, true, query, arg);
        }
        if kw(which, "BLEVel") {
            return self.pair_level(ui, TrigType::Delay, false, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:DELay:{}", which));
        None
    }

    /// Data and clock. Both the setup time and the hold time stay active.
    pub(super) fn trigger_setup(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "DSRC") {
            return self.trigger_line(ui, TrigType::SetupHold, query, arg);
        }
        if kw(which, "CSRC") {
            return self.source_b(ui, TrigType::SetupHold, query, arg);
        }
        if kw(which, "SLOPe") {
            return self.edge_slope(ui, TrigType::SetupHold, true, true, query, arg);
        }
        if kw(which, "TYPE") {
            if query {
                return t("SETH");
            }
            if kw(arg, "SETHold") {
                ui.m.trig_type = TrigType::SetupHold;
                ui.mark_changed();
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" setup and hold are both active".into());
            return None;
        }
        if kw(which, "STIMe") {
            return self.positive_time(ui, TrigType::SetupHold, query, arg, TimeSlot::Setup);
        }
        if kw(which, "HTIMe") {
            return self.positive_time(ui, TrigType::SetupHold, query, arg, TimeSlot::Hold);
        }
        if kw(which, "DLEVel") {
            return self.pair_level(ui, TrigType::SetupHold, true, query, arg);
        }
        if kw(which, "CLEVel") {
            return self.pair_level(ui, TrigType::SetupHold, false, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:SHOLd:{}", which));
        None
    }

    /// The edge count after a quiet stretch.
    pub(super) fn trigger_nth(&mut self, ui: &mut Ui, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if kw(which, "SOURce") {
            return self.trigger_line(ui, TrigType::NthEdge, query, arg);
        }
        if kw(which, "SLOPe") {
            return self.edge_slope(ui, TrigType::NthEdge, false, true, query, arg);
        }
        if kw(which, "IDLE") {
            return self.positive_time(ui, TrigType::NthEdge, query, arg, TimeSlot::Idle);
        }
        if kw(which, "EDGE") {
            if query {
                return t(&ui.m.nth_n.to_string());
            }
            let Some(n) = arg.trim().parse::<u32>().ok().filter(|&n| (1..=65535).contains(&n)) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.trig_type = TrigType::NthEdge;
            ui.m.nth_n = n;
            ui.mark_changed();
            return None;
        }
        if kw(which, "LEVel") {
            return self.pair_level(ui, TrigType::NthEdge, true, query, arg);
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:NEDGe:{}", which));
        None
    }

    /// The stored frames. Frame 1 is the oldest. Live reports the newest.
    pub(super) fn record_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let current = n.len() == 1 && kw(n[0], "CURRent") || n.len() == 2 && kw(n[0], "WREPlay") && kw(n[1], "FCURrent");
        if current {
            if query {
                return t(&ui.history_frame().to_string());
            }
            return self.show_frame(ui, arg);
        }
        if n.len() == 2 && kw(n[0], "WREPlay") && kw(n[1], "FMAX") && query {
            return t(&ui.history_len().to_string());
        }
        self.push_err(format!("-113,\"Undefined header\" :RECord:{}", n.join(":")));
        None
    }

    /// Older, newer, the first frame, and the last frame.
    pub(super) fn navigate_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        if query || n.len() != 2 || !kw(n[0], "FRAMe") {
            self.push_err(format!("-113,\"Undefined header\" :NAVigate:{}", n.join(":")));
            return None;
        }
        let _ = arg;
        let step = if kw(n[1], "NEXT") {
            Some(1)
        } else if kw(n[1], "BACK") {
            Some(-1)
        } else if kw(n[1], "STARt") {
            Some(0)
        } else if kw(n[1], "END") {
            Some(0)
        } else {
            None
        };
        let Some(step) = step else {
            self.push_err(format!("-113,\"Undefined header\" :NAVigate:FRAMe:{}", n[1]));
            return None;
        };
        if ui.history_len() == 0 {
            self.push_err("-221,\"Settings conflict\" history empty".into());
            return None;
        }
        if kw(n[1], "STARt") {
            ui.history_show(1);
        } else if kw(n[1], "END") {
            ui.history_show(ui.history_len());
        } else {
            ui.history_step(step);
        }
        ui.mark_changed();
        None
    }

    pub(super) fn show_frame(&mut self, ui: &mut Ui, arg: &str) -> Option<Vec<u8>> {
        let n = ui.history_len();
        if n == 0 {
            self.push_err("-221,\"Settings conflict\" history empty".into());
            return None;
        }
        let Some(frame) = arg.trim().parse::<usize>().ok().filter(|&k| (1..=n).contains(&k)) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.history_show(frame);
        ui.mark_changed();
        None
    }

    /// Mark events of the trigger already set. The screen has no separate search type.
    pub(super) fn search_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let which = n.first().copied().unwrap_or("");
        if n.len() != 1 {
            self.push_err(format!("-113,\"Undefined header\" :SEARch:{}", n.join(":")));
            return None;
        }
        if kw(which, "STATe") {
            if query {
                return t(if ui.search_on { "1" } else { "0" });
            }
            let Some(on) = parse_bool(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.search_on = on;
            ui.mark_changed();
            return None;
        }
        if kw(which, "COUNt") && query {
            let n = if ui.search_on { ui.search_events().len() } else { 0 };
            return t(&n.to_string());
        }
        if kw(which, "MODE") {
            if query {
                return t(&trig_token(ui));
            }
            if names_trigger(ui, arg) {
                return None;
            }
            self.push_err("-224,\"Illegal parameter value\" search uses the trigger".into());
            return None;
        }
        let events = if ui.search_on { ui.search_events() } else { Default::default() };
        if kw(which, "EVENt") {
            if query {
                let shown = if events.is_empty() { "0".to_string() } else { self.search_at.to_string() };
                return t(&shown);
            }
            if !ui.search_on {
                self.push_err("-221,\"Settings conflict\" search is off".into());
                return None;
            }
            let Some(k) = arg.trim().parse::<u32>().ok().map(|k| if k == 0 { 1 } else { k }) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            if k < 1 || k as usize > events.len() {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            }
            if ui.m.run == RunState::Run {
                self.push_err("-221,\"Settings conflict\" stop the scope".into());
                return None;
            }
            ui.go_to_time(events[k as usize - 1]);
            self.search_at = k;
            ui.mark_changed();
            return None;
        }
        if kw(which, "VALue") && query {
            if !ui.search_on || events.is_empty() {
                self.push_err("-221,\"Settings conflict\" no search events".into());
                return t("9.9E37");
            }
            let k = if arg.trim().is_empty() { self.search_at } else { arg.trim().parse::<u32>().unwrap_or(0) };
            if k < 1 || k as usize > events.len() {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return t("9.9E37");
            }
            return t(&e(events[k as usize - 1]));
        }
        self.push_err(format!("-113,\"Undefined header\" :SEARch:{}", which));
        None
    }

    /// Two zones. A box is two corners. A shape is three or more vertices. Rigol has no zone command; these names are Keysight's.
    pub(super) fn trigger_zone(&mut self, ui: &mut Ui, index: Option<usize>, which: &str, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let Some(i) = index.and_then(|n| n.checked_sub(1)).filter(|&i| i < ui.m.zones.len()) else {
            self.push_err("-224,\"Illegal parameter value\" zone 1 or 2".into());
            return if query { t("9.9E37") } else { None };
        };
        if kw(which, "STATe") {
            if query {
                return t(if ui.m.zones[i].on { "1" } else { "0" });
            }
            let Some(on) = parse_bool(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.zones[i].on = on;
            ui.mark_changed();
            return None;
        }
        if kw(which, "SOURce") {
            if query {
                return t(&format!("CHAN{}", ui.m.zones[i].src + 1));
            }
            let Some(c) = parse_ch(arg) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.zones[i].src = c;
            ui.mark_changed();
            return None;
        }
        if kw(which, "MODE") {
            if query {
                return t(if ui.m.zones[i].inside { "INT" } else { "NOT" });
            }
            let inside = if kw(arg, "INTersect") {
                Some(true)
            } else if kw(arg, "NOTintersect") {
                Some(false)
            } else {
                None
            };
            let Some(inside) = inside else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.zones[i].inside = inside;
            ui.mark_changed();
            return None;
        }
        if kw(which, "PLACement") {
            if query {
                if ui.m.zones[i].pts.len() != 2 {
                    self.push_err("-221,\"Settings conflict\" zone is not a box".into());
                    return t("9.9E37");
                }
                return t(&point_text(&ui.m.zones[i].pts));
            }
            let Some(pts) = point_list(arg, 2) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.zones[i].pts = pts;
            ui.mark_changed();
            return None;
        }
        if kw(which, "POINts") {
            if query {
                if ui.m.zones[i].pts.len() < 2 {
                    self.push_err("-221,\"Settings conflict\" zone has no outline".into());
                    return t("9.9E37");
                }
                return t(&point_text(&ui.m.zones[i].pts));
            }
            let Some(pts) = point_list(arg, 3) else {
                self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
                return None;
            };
            ui.m.zones[i].pts = pts;
            ui.mark_changed();
            return None;
        }
        if kw(which, "CLEar") && !query {
            let src = ui.m.zones[i].src;
            ui.m.zones[i] = crate::zone::Zone { src, ..Default::default() };
            ui.mark_changed();
            return None;
        }
        self.push_err(format!("-113,\"Undefined header\" :TRIGger:ZONE{}:{}", i + 1, which));
        None
    }

    /// Rising, falling, or either edge. `second` is source B or the clock.
    pub(super) fn edge_slope(&mut self, ui: &mut Ui, kind: TrigType, second: bool, allow_either: bool, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let cur = if second { ui.m.trig_slope2 } else { ui.m.slope };
        if query {
            return t(slope_token(cur));
        }
        let Some(s) = parse_slope(arg, allow_either) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        if second {
            ui.m.trig_slope2 = s;
        } else {
            ui.m.slope = s;
        }
        ui.mark_changed();
        None
    }

    pub(super) fn pulse_polarity(&mut self, ui: &mut Ui, kind: TrigType, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if query {
            return t(if ui.m.trig_pol == Pol::Pos { "POS" } else { "NEG" });
        }
        let pol = if kw(arg, "POSitive") {
            Some(Pol::Pos)
        } else if kw(arg, "NEGative") {
            Some(Pol::Neg)
        } else {
            None
        };
        let Some(pol) = pol else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        ui.m.trig_pol = pol;
        ui.mark_changed();
        None
    }

    /// Upper is the screen's first level. Lower is the second level.
    pub(super) fn pair_level(&mut self, ui: &mut Ui, kind: TrigType, upper: bool, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let cur = if upper { ui.m.trig_level } else { ui.m.trig_level2 };
        if query {
            return t(&e(cur as f64));
        }
        let Some(v) = num(arg) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        if upper {
            ui.m.trig_level = v;
        } else {
            ui.m.trig_level2 = v;
        }
        ui.mark_changed();
        None
    }

    /// A time the screen stores in seconds.
    pub(super) fn positive_time(&mut self, ui: &mut Ui, kind: TrigType, query: bool, arg: &str, slot: TimeSlot) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let cur = match slot {
            TimeSlot::Limit => ui.m.trig_t1,
            TimeSlot::Setup => ui.m.setup,
            TimeSlot::Hold => ui.m.hold,
            TimeSlot::Idle => ui.m.nth_idle,
        };
        if query {
            return t(&e(cur as f64));
        }
        let Some(v) = num(arg).filter(|&v| v > 0.0) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        match slot {
            TimeSlot::Limit => ui.m.trig_t1 = v,
            TimeSlot::Setup => ui.m.setup = v,
            TimeSlot::Hold => ui.m.hold = v,
            TimeSlot::Idle => ui.m.nth_idle = v,
        }
        ui.mark_changed();
        None
    }

    /// Source B or the clock. Analog channels only, which is the source knob.
    pub(super) fn source_b(&mut self, ui: &mut Ui, kind: TrigType, query: bool, arg: &str) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        if query {
            return t(&format!("CHAN{}", ui.m.trig_src2 + 1));
        }
        let Some(c) = parse_ch(arg) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        ui.m.ch[c].on = true;
        ui.m.trig_src2 = c;
        ui.mark_changed();
        None
    }

    /// Upper (`upper`) or lower time limit for a pulse, a slope, a duration, a runt, or a delay. Greater uses the lower limit. Less uses the upper limit. Inside and outside use both. The single limit is the screen's Time.
    pub(super) fn when_width(&mut self, ui: &mut Ui, upper: bool, query: bool, arg: &str, kind: TrigType) -> Option<Vec<u8>> {
        let t = |s: &str| Some(s.as_bytes().to_vec());
        let uses = match ui.m.trig_when {
            WhenKind::Gt => !upper,
            WhenKind::Lt => upper,
            WhenKind::In | WhenKind::Out => true,
            WhenKind::Off => false,
        };
        if !uses {
            self.push_err("-221,\"Settings conflict\"".into());
            return if query { t("9.9E37") } else { None };
        }
        let read = if matches!(ui.m.trig_when, WhenKind::In | WhenKind::Out) {
            if upper { ui.m.trig_t2 } else { ui.m.trig_t1 }
        } else {
            ui.m.trig_t1
        };
        if query {
            return t(&e(read as f64));
        }
        let Some(v) = num(arg).filter(|&v| v > 0.0) else {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        };
        ui.m.trig_type = kind;
        if matches!(ui.m.trig_when, WhenKind::In | WhenKind::Out) {
            if upper {
                if ui.m.trig_t1 > v {
                    ui.m.trig_t1 = v;
                }
                ui.m.trig_t2 = v;
            } else {
                if ui.m.trig_t2 < v {
                    ui.m.trig_t2 = v;
                }
                ui.m.trig_t1 = v;
            }
        } else {
            ui.m.trig_t1 = v;
        }
        ui.mark_changed();
        None
    }
}
