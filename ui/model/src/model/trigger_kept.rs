// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! What a trigger type and a trigger source keep while another is chosen (#49). A type left keeps its own values (the
//! width and its condition, the polarity, the pattern, the times, the second source and level, the video choices); a
//! type chosen again gets them back. A channel left as the source keeps its trigger level (each channel has a scale of
//! its own); a channel chosen again gets it back. Choosing a type never changes the source. None of this is part of a
//! setup: a setup holds the values of the type and source it was saved with, and a loaded setup starts with nothing
//! kept.

use super::*;

/// The values of one trigger type that other types use too, so that they are kept per type.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TypeValues {
    slope: Slope,
    pol: Pol,
    when: WhenKind,
    t1: f32,
    t2: f32,
    /// the lower level (Slope, Runt, Window), level B (Delay) or the clock level (Setup/Hold)
    level2: f32,
    /// the source the lower level of a two-level type was set on: it comes back on that source only
    on: WaveId,
    src2: usize,
    ref2: Option<usize>,
    slope2: Slope,
    win_mode: WinMode,
    nth_idle: f32,
    nth_n: u32,
    setup: f32,
    hold: f32,
    video_std: VideoStd,
    video_sync: VSync,
    video_line: u32,
    pat: [Bit; NCH],
}

/// What the trigger types and sources keep; empty in a new or loaded model.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct TrigKept {
    /// per type, by its place in `TRIG_TYPES`
    types: [Option<TypeValues>; TRIG_TYPES.len()],
    /// per channel: the trigger level, and the lower level of a two-level type
    levels: [Option<f32>; NCH],
    lower: [Option<f32>; NCH],
}

fn type_index(t: TrigType) -> usize {
    TRIG_TYPES.iter().position(|&x| x == t).unwrap_or(0)
}

impl Model {
    fn type_values(&self) -> TypeValues {
        TypeValues {
            slope: self.slope,
            pol: self.trig_pol,
            when: self.trig_when,
            t1: self.trig_t1,
            t2: self.trig_t2,
            level2: self.trig_level2,
            on: self.trigger_wave(),
            src2: self.trig_src2,
            ref2: self.trig_ref2,
            slope2: self.trig_slope2,
            win_mode: self.win_mode,
            nth_idle: self.nth_idle,
            nth_n: self.nth_n,
            setup: self.setup,
            hold: self.hold,
            video_std: self.video_std,
            video_sync: self.video_sync,
            video_line: self.video_line,
            pat: self.pat,
        }
    }

    /// Keeps the values of the trigger type set now; called before another type is chosen.
    pub fn keep_trig_type(&mut self) {
        self.trig_kept.types[type_index(self.trig_type)] = Some(self.type_values());
    }

    /// Gives the trigger type set now the values it kept, if it has been left before. The lower level of a two-level
    /// type comes back on the source it was set on, on another channel the one that channel kept. A second source
    /// comes back when it is a reference (the screen drops one that is no longer loaded) or a channel that is on.
    /// True when the type had values kept.
    pub fn take_kept_trig_type(&mut self) -> bool {
        let Some(v) = self.trig_kept.types[type_index(self.trig_type)] else { return false };
        (self.slope, self.trig_pol, self.trig_when, self.trig_t1, self.trig_t2) = (v.slope, v.pol, v.when, v.t1, v.t2);
        (self.trig_slope2, self.win_mode, self.nth_idle, self.nth_n) = (v.slope2, v.win_mode, v.nth_idle, v.nth_n);
        (self.setup, self.hold, self.pat) = (v.setup, v.hold, v.pat);
        (self.video_std, self.video_sync, self.video_line) = (v.video_std, v.video_sync, v.video_line);
        if v.ref2.is_some() || self.ch[v.src2.min(NCH - 1)].on {
            (self.trig_src2, self.trig_ref2) = (v.src2, v.ref2);
        }
        if !self.trig_type.two_level() || v.on == self.trigger_wave() {
            self.trig_level2 = v.level2;
        } else if let WaveId::Ch(c) = self.trigger_wave() {
            self.trig_level2 = self.trig_kept.lower[c].unwrap_or(self.trig_level2);
        }
        if self.trig_type != TrigType::Runt && self.trig_when == WhenKind::Off {
            self.trig_when = WhenKind::Lt;
        }
        self.order_trigger_levels();
        true
    }

    /// The trigger level goes with its source: the channel `before` keeps the level it had, a channel chosen as the
    /// source again gets its level back (and its lower level, for a two-level type). A reference keeps none.
    pub fn trig_level_to_source(&mut self, before: WaveId) {
        let now = self.trigger_wave();
        if now == before {
            return;
        }
        let two = self.trig_type.two_level();
        if let WaveId::Ch(c) = before {
            self.trig_kept.levels[c] = Some(self.trig_level);
            if two {
                self.trig_kept.lower[c] = Some(self.trig_level2);
            }
        }
        if let WaveId::Ch(c) = now {
            self.trig_level = self.trig_kept.levels[c].unwrap_or(self.trig_level);
            if two {
                self.trig_level2 = self.trig_kept.lower[c].unwrap_or(self.trig_level2);
            }
        }
        self.order_trigger_levels();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_type_chosen_again_gets_its_own_values_back() {
        let mut m = Model::default();
        let choose = |m: &mut Model, t: TrigType| {
            m.keep_trig_type();
            m.trig_type = t;
            m.take_kept_trig_type();
        };
        choose(&mut m, TrigType::Pulse);
        (m.trig_t1, m.trig_when, m.trig_pol) = (5e-6, WhenKind::Gt, Pol::Neg);
        choose(&mut m, TrigType::Slope);
        // Slope starts from what Pulse left (nothing kept for it yet), then sets its own
        assert_eq!(m.trig_t1, 5e-6);
        (m.trig_t1, m.trig_when, m.trig_pol) = (2e-6, WhenKind::Lt, Pol::Pos);
        choose(&mut m, TrigType::Pulse);
        assert_eq!((m.trig_t1, m.trig_when, m.trig_pol), (5e-6, WhenKind::Gt, Pol::Neg), "Pulse's own");
        choose(&mut m, TrigType::Slope);
        assert_eq!((m.trig_t1, m.trig_when, m.trig_pol), (2e-6, WhenKind::Lt, Pol::Pos), "Slope's own");
        // a reference as the second source comes back with its type
        choose(&mut m, TrigType::Delay);
        m.set_trigger_wave(WaveId::Ref(2), true);
        choose(&mut m, TrigType::SetupHold);
        m.set_trigger_wave(WaveId::Ch(1), true);
        choose(&mut m, TrigType::Delay);
        assert_eq!(m.trigger_wave2(), WaveId::Ref(2), "Delay's B");
        // the source stays whatever the type
        m.trig_src = 2;
        choose(&mut m, TrigType::Pulse);
        assert_eq!(m.trig_src, 2);
    }

    #[test]
    fn a_channel_chosen_again_gets_its_level_back() {
        let mut m = Model::default();
        m.set_trigger_wave(WaveId::Ch(0), false);
        m.trig_level = 1.5;
        m.set_trigger_wave(WaveId::Ch(1), false);
        // nothing kept for CH2 yet: the level stays
        assert_eq!(m.trig_level, 1.5);
        m.trig_level = 0.2;
        m.set_trigger_wave(WaveId::Ch(0), false);
        assert_eq!(m.trig_level, 1.5, "CH1's level");
        // a reference in between keeps none and leaves the channel's
        m.set_trigger_wave(WaveId::Ref(0), false);
        m.trig_level = 3.0;
        m.set_trigger_wave(WaveId::Ch(1), false);
        assert_eq!(m.trig_level, 0.2, "CH2's level");
        // the band of a two-level type goes with its channel too
        m.trig_type = TrigType::Runt;
        (m.trig_level, m.trig_level2) = (0.3, 0.1);
        m.set_trigger_wave(WaveId::Ch(0), false);
        (m.trig_level, m.trig_level2) = (2.0, 0.5);
        m.set_trigger_wave(WaveId::Ch(1), false);
        assert_eq!((m.trig_level, m.trig_level2), (0.3, 0.1));
    }

    #[test]
    fn a_new_model_keeps_nothing() {
        assert_eq!(Model::default().trig_kept, TrigKept::default());
    }
}
