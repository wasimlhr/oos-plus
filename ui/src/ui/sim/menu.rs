// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The SIM menu: its items and advanced items, their kinds, labels, values and options, and what a turn or a press of
//! one does. A new SIM setting gets its item here.

use super::*;

impl Ui {
    pub(in crate::ui) fn sim_items(&self) -> Vec<Item> {
        let mut items = vec![
            Item::SimPreset,
            Item::SimRate,
            Item::SimPat,
            Item::SimHigh,
            Item::SimLow,
            Item::SimRise,
            Item::SimBw,
            Item::SimDeemph,
            Item::SimNoise,
            Item::SimSsc,
            Item::Open(MenuId::SimAdvanced),
            Item::SimRj,
            Item::SimPj,
            Item::SimPjHz,
            Item::SimSlot,
            Item::SimAddRef,
            Item::SimPlay,
            Item::SimShow,
            Item::SimDefaults,
        ];
        // Studio only: the sheet offers a file for the SIM slot
        if self.m.layout == Layout::Studio { items.insert(items.iter().position(|it| *it == Item::SimAddRef).map_or(0, |i| i + 1), Item::SimLoad); }
        // Run plays the signal of the knobs; the references that are shown (files, added signals) have a play of their own
        if self.sim_can_play_refs() { items.insert(items.iter().position(|it| *it == Item::SimPlay).map_or(0, |i| i + 1), Item::SimPlayRefs); }
        if self.record_mode() {
            items.insert(0, Item::SimLive);
        }
        items
    }

    pub(in crate::ui) fn sim_advanced_items(&self) -> Vec<Item> {
        vec![
            Item::SimSsc,
            Item::SimSscCenter,
            Item::SimSscDeviation,
            Item::SimSscHz,
            Item::SimDcd,
            Item::SimCorrRj,
            Item::SimCorrHz,
            Item::SimDefaults,
        ]
    }

    pub(in crate::ui) fn sim_kind(it: Item) -> Option<Kind> {
        Some(match it {
            Item::SimPlay | Item::SimPlayRefs | Item::SimAddRef | Item::SimLoad | Item::SimLive | Item::SimDefaults => Kind::Action,
            Item::SimPreset | Item::SimPat | Item::SimSsc | Item::SimSscCenter | Item::SimSlot | Item::SimShow => {
                Kind::Choice
            }
            Item::SimRate
            | Item::SimHigh
            | Item::SimLow
            | Item::SimRise
            | Item::SimBw
            | Item::SimDeemph
            | Item::SimNoise
            | Item::SimRj
            | Item::SimPj
            | Item::SimPjHz
            | Item::SimSscDeviation
            | Item::SimSscHz
            | Item::SimDcd
            | Item::SimCorrRj
            | Item::SimCorrHz => Kind::Value,
            _ => return None,
        })
    }

    pub(in crate::ui) fn sim_label(it: Item) -> Option<&'static str> {
        Some(match it {
            Item::Open(MenuId::Sim) => "SIM",
            Item::Open(MenuId::SimAdvanced) => "Advanced",
            Item::SimPreset => "Preset",
            Item::SimRate => "Rate",
            Item::SimPat => "Pattern",
            Item::SimHigh => "High",
            Item::SimLow => "Low",
            Item::SimRise => "Rise 20-80%",
            Item::SimBw => "Bandwidth",
            Item::SimDeemph => "De-emph",
            Item::SimNoise => "Noise",
            Item::SimSsc => "Spread",
            Item::SimSscCenter => "SSC profile",
            Item::SimSscDeviation => "SSC deviation",
            Item::SimSscHz => "SSC frequency",
            Item::SimDcd => "DCD p-p",
            Item::SimCorrRj => "Corr RJ RMS",
            Item::SimCorrHz => "Corr RJ corner",
            Item::SimRj => "RJ RMS",
            Item::SimPj => "PJ peak",
            Item::SimPjHz => "PJ freq",
            Item::SimSlot => "Slot",
            Item::SimPlay => "Run simulation mode",
            Item::SimPlayRefs => "Play references",
            Item::SimAddRef => "Add to ref",
            Item::SimLoad => "Load file...",
            Item::SimLive => "Live",
            Item::SimShow => "Show",
            Item::SimDefaults => "Defaults",
            _ => return None,
        })
    }

    pub(in crate::ui) fn sim_value(&self, it: Item) -> Option<String> {
        let m = &self.m;
        Some(match it {
            Item::SimPreset => self.sim_lane().name.into(),
            Item::SimRate => si_short(m.sim_rate, "b/s"),
            Item::SimPat => format!("PRBS-{}", m.sim_prbs),
            Item::SimHigh => si_short(m.sim_vhigh, "V"),
            Item::SimLow => si_short(m.sim_vlow, "V"),
            Item::SimRise => si_short(m.sim_rise, "s"),
            Item::SimBw => si_short(m.sim_bw, "Hz"),
            Item::SimDeemph => format!("{:.1} dB", m.sim_deemph),
            Item::SimNoise => si_short(m.sim_noise, "V"),
            Item::SimSsc => {
                if m.sim_ssc {
                    "On".into()
                } else {
                    "Off".into()
                }
            }
            Item::SimSscCenter => {
                if m.sim_ssc_center {
                    "Center".into()
                } else {
                    "Down, DC removed".into()
                }
            }
            Item::SimSscDeviation => {
                format!("{:.2} % {}", 100.0 * m.sim_ssc_deviation, if m.sim_ssc_center { "pk" } else { "p-p" })
            }
            Item::SimSscHz => si_short(m.sim_ssc_hz, "Hz"),
            Item::SimDcd => si_short(m.sim_dcd_pp, "s"),
            Item::SimCorrRj => si_short(m.sim_corr_rj, "s"),
            Item::SimCorrHz => si_short(m.sim_corr_hz, "Hz"),
            Item::SimRj => si_short(m.sim_rj, "s"),
            Item::SimPj => si_short(m.sim_pj, "s"),
            Item::SimPjHz => {
                if m.sim_pj > 0.0 {
                    si_short(m.sim_pjhz, "Hz")
                } else {
                    "off".into()
                }
            }
            Item::SimSlot => format!("R{}", m.sim_slot + 1),
            Item::SimPlay | Item::SimPlayRefs | Item::SimAddRef | Item::SimLoad => String::new(),
            Item::SimDefaults => String::new(),
            Item::SimShow => match &self.refs[m.sim_slot] {
                None => "empty".into(),
                Some(r) => {
                    let name = if r.label.is_empty() { format!("R{}", m.sim_slot + 1) } else { r.label.clone() };
                    format!("{} {}", name, if r.on { "on" } else { "off" })
                }
            },
            _ => return None,
        })
    }

    pub(in crate::ui) fn sim_options(&self, it: Item) -> Option<Vec<String>> {
        Some(match it {
            Item::SimPreset => serial::names().into_iter().map(|s| s.to_string()).collect(),
            Item::SimPat => PRBS.iter().map(|n| format!("PRBS-{n}")).collect(),
            Item::SimSsc => vec!["Off".into(), "On".into()],
            Item::SimSscCenter => vec!["Down, DC removed".into(), "Center".into()],
            Item::SimSlot => (0..NREF).map(|i| format!("R{}", i + 1)).collect(),
            Item::SimShow => vec!["Off".into(), "On".into()],
            _ => return None,
        })
    }

    pub(in crate::ui) fn sim_adjust(&mut self, it: Item, d: i32) -> bool {
        let running = self.sim_requested_running();
        match it {
            Item::SimPreset => {
                let n = LANES.len() as i32;
                self.m.sim_preset = (self.m.sim_preset as i32 + d).rem_euclid(n) as usize;
                self.m.apply_sim_preset();
            }
            Item::SimRate => { let v = self.ladder_eng(it, self.m.sim_rate.max(ranges::SIM_RATE.0), d, ranges::SIM_RATE); self.m.sim_rate = v; }
            Item::SimPat => {
                let i = PRBS.iter().position(|&p| p == self.m.sim_prbs).unwrap_or(0);
                self.m.sim_prbs = PRBS[(i as i32 + d).rem_euclid(PRBS.len() as i32) as usize];
            }
            Item::SimHigh => {
                self.m.sim_vhigh = (self.m.sim_vhigh + 0.05 * d as f64).clamp(ranges::SIM_LEVEL.0, ranges::SIM_LEVEL.1)
            }
            Item::SimLow => {
                self.m.sim_vlow = (self.m.sim_vlow + 0.05 * d as f64).clamp(ranges::SIM_LEVEL.0, ranges::SIM_LEVEL.1)
            }
            Item::SimRise => {
                let v = self.ladder_eng(it, self.m.sim_rise.max(ranges::SIM_RISE.0), d, ranges::SIM_RISE);
                self.m.sim_rise = v;
            }
            Item::SimBw => {
                let v = self.ladder_eng(it, self.m.sim_bw.max(ranges::SIM_BW.0), d, ranges::SIM_BW);
                self.m.sim_bw = v;
            }
            Item::SimDeemph => {
                self.m.sim_deemph =
                    (self.m.sim_deemph + 0.5 * d as f64).clamp(ranges::SIM_DEEMPH.0, ranges::SIM_DEEMPH.1)
            }
            Item::SimNoise => {
                let v = self.ladder_eng(it, self.m.sim_noise.max(ranges::SIM_NOISE.0), d, ranges::SIM_NOISE);
                self.m.sim_noise = v;
            }
            Item::SimSsc => self.m.sim_ssc = !self.m.sim_ssc,
            Item::SimSscCenter => self.m.sim_ssc_center = !self.m.sim_ssc_center,
            Item::SimSscDeviation => {
                self.m.sim_ssc_deviation = (self.m.sim_ssc_deviation + d as f64 * 0.0005)
                    .clamp(ranges::SIM_SSC_DEV.0, ranges::SIM_SSC_DEV.1)
            }
            Item::SimSscHz => {
                let v = self.ladder_eng(it, self.m.sim_ssc_hz, d, ranges::SIM_SSC_HZ);
                self.m.sim_ssc_hz = v;
            }
            Item::SimDcd => {
                let v = self.ladder_eng(it, self.m.sim_dcd_pp, d, ranges::SIM_DCD);
                self.m.sim_dcd_pp = v;
            }
            Item::SimCorrRj => {
                let v = self.ladder_eng(it, self.m.sim_corr_rj, d, ranges::SIM_CORR_RJ);
                self.m.sim_corr_rj = v;
            }
            Item::SimCorrHz => {
                let v = self.ladder_eng(it, self.m.sim_corr_hz, d, ranges::SIM_CORR_HZ);
                self.m.sim_corr_hz = v;
            }
            Item::SimRj => {
                let v = self.ladder_eng(it, self.m.sim_rj.max(ranges::SIM_RJ.0), d, ranges::SIM_RJ);
                self.m.sim_rj = v;
            }
            Item::SimPj => {
                let v = self.ladder_eng(it, self.m.sim_pj, d, ranges::SIM_PJ);
                self.m.sim_pj = v;
            }
            Item::SimPjHz => {
                let v = self.ladder_eng(it, self.m.sim_pjhz.max(ranges::SIM_PJ_HZ.0), d, ranges::SIM_PJ_HZ);
                self.m.sim_pjhz = v;
            }
            Item::SimSlot => self.m.sim_slot = (self.m.sim_slot as i32 + d).clamp(0, NREF as i32 - 1) as usize,
            Item::SimShow => {
                if let Some(r) = &mut self.refs[self.m.sim_slot] {
                    r.on = !r.on;
                } else {
                    let slot = self.m.sim_slot + 1;
                    self.toast(&format!("R{slot} is empty: Play or Add to ref first"));
                }
            }
            _ => return false,
        }
        if (self.session_is_sim || self.sim_generating()) && it != Item::SimShow {
            self.sim_request_ui(running);
        }
        true
    }

    pub(in crate::ui) fn sim_activate(&mut self, it: Item) -> bool {
        match it {
            Item::SimLive => self.return_to_live(),
            Item::SimAddRef => self.sim_add_to_ref(),
            Item::SimLoad => self.studio_load_file(self.m.sim_slot),
            Item::SimPlay if self.sim_generating() => {
                if self.session_is_sim && self.m.run == RunState::Run {
                    self.record_key(key::RUNSTOP);
                } else {
                    self.sim_cancel_generation();
                }
                self.toast("SIM generation cancelled");
            }
            Item::SimPlay if self.sim_mode() => {
                self.record_key(key::RUNSTOP);
            }
            // Run is the signal of the knobs, whatever else is on the screen: a reference that is shown stays shown
            // beside it, and the knobs go on changing the signal. (It used to play the shown references instead, and
            // then no knob did anything.) `:SIM:PLAY ON` has always done this
            Item::SimPlay => self.sim_request_ui(true),
            Item::SimPlayRefs => {
                if self.sim_can_play_refs() {
                    self.sim_start_refs();
                } else {
                    self.toast("No reference is shown: Add to ref or load a file first");
                }
            }
            Item::SimDefaults => {
                let regenerate = self.session_is_sim || self.sim_generating();
                let running = self.sim_requested_running();
                self.m.apply_sim_preset();
                if regenerate {
                    self.sim_request_ui(running);
                } else {
                    self.toast("SIM knobs from the preset");
                }
            }
            _ => return false,
        }
        true
    }
}
