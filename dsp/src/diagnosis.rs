// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Why a decoder decoded nothing, or mostly errors, in short sentences a person or an agent can act on. Everything
//! is computed from what the decoder already knows: its bit time ([`ModuleInfo::bit_time`]), the sample spacing, the
//! levels of the line ([`crate::levels::stats`]), its edges at the threshold ([`crate::edges::edges`], the unit
//! interval of [`crate::eye::unit_interval`]), the failed fields, and the reading with the other polarity.
//!
//! A decode that is fine has no diagnosis: [`diagnose`] returns no lines when some packets decoded and fewer than
//! half of them failed. The sentences name the setting to change.

use crate::decode::Packet;
use crate::edges::edges;
use crate::levels::stats;
use crate::protocols::{Lines, Mirror, ModuleInfo, Polarity};

/// The samples per bit time the decoders want (the sampling of the protocol trigger is a twentieth of a bit; below
/// ten the edges of a bit are too coarse to read it reliably).
pub const MIN_SAMPLES_PER_BIT: f64 = 10.0;

/// A record shorter than this many bit times holds no frame of any protocol here.
pub const MIN_RECORD_BITS: f64 = 20.0;

/// What a decoder found out about its line: the sentences, and the figures they come from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diagnosis {
    /// One short sentence each, the most important first; empty when the decode is fine.
    pub lines: Vec<String>,
    /// The bit rate (bit/s) the edges of the line suggest: the unit interval of their spacings. `None` without
    /// two edges. For a Manchester line this is twice the bit rate (an edge in the middle of a bit).
    pub bit_rate: Option<f64>,
    /// The lowest and highest sample of the line (V); NaN without a finite sample.
    pub vmin: f32,
    pub vmax: f32,
    /// Fields decoded, and how many of them failed a check.
    pub packets: usize,
    pub failed: usize,
}

impl Diagnosis {
    /// The share of the fields that failed a check, 0 without any.
    pub fn error_fraction(&self) -> f64 {
        if self.packets == 0 {
            0.0
        } else {
            self.failed as f64 / self.packets as f64
        }
    }
}

/// Value with an SI prefix and three significant digits ("15.6 kSa/s").
pub fn si(x: f64, unit: &str) -> String {
    if !x.is_finite() || x == 0.0 {
        return format!("0 {}", unit);
    }
    let (scale, prefix) = crate::si_prefix(x.abs(), 1);
    let y = x / scale;
    let dec = if y.abs() >= 100.0 {
        0
    } else if y.abs() >= 10.0 {
        1
    } else {
        2
    };
    format!("{:.*} {}{}", dec, y, prefix, unit)
}

/// Whether a decode is poor: nothing decoded, or at least half of what decoded failed its check.
pub fn is_poor(packets: &[Packet]) -> bool {
    packets.is_empty() || packets.iter().filter(|p| !p.ok).count() * 2 >= packets.len()
}

/// The reading of `l` with the module's polarity row set to `pol`, and the failed fields in it; `None` for a module
/// without a polarity row or a decode that is an error.
fn read_as(info: &ModuleInfo, l: &Lines, pol: Polarity) -> Option<(usize, usize)> {
    let (slot, _) = info.polarity?;
    let mut opt = l.opt;
    opt[slot] = pol as usize;
    let p = info.decode(&Lines { opt, ..*l }).ok()?;
    Some((p.len(), p.iter().filter(|x| !x.ok).count()))
}

/// The diagnosis of the decode of `l` by `info` that gave `packets` (read as the options choose: the polarity of
/// the row applies). `info.bit_time` and the lines are the module's; a module without a bit time gets no sample
/// rate or record sentence.
pub fn diagnose(info: &ModuleInfo, l: &Lines, packets: &[Packet]) -> Diagnosis {
    let main = l.main();
    let failed = packets.iter().filter(|p| !p.ok).count();
    let mut d = Diagnosis { vmin: f32::NAN, vmax: f32::NAN, packets: packets.len(), failed, ..Default::default() };
    let s = stats(main, l.dt);
    if s.valid {
        d.vmin = s.vmin;
        d.vmax = s.vmax;
    }
    let spacings: Vec<f64> = edges(main, l.thr, l.hy).windows(2).map(|w| (w[1].t - w[0].t) * l.dt).collect();
    d.bit_rate = crate::eye::unit_interval(&spacings).filter(|u| *u > 0.0).map(|u| 1.0 / u);
    if !is_poor(packets) {
        return d;
    }
    let mut out: Vec<String> = Vec::new();
    macro_rules! say {
        ($s:expr) => {
            out.push($s)
        };
    }
    // the line itself: no samples, no signal, a threshold outside of it
    let mut line_fault = false;
    if !s.valid {
        say!("the source has no samples: check that its channel is on".to_string());
        d.lines = out;
        return d;
    }
    let (vmin, vmax) = (s.vmin, s.vmax);
    // a line that moves less than a thousandth of the vertical swing of any real signal, or exactly nothing
    if vmax - vmin < 1e-3 {
        say!(format!(
            "the line is flat at {}: no signal; check the probe, the channel and its vertical scale",
            si(((vmin + vmax) / 2.0) as f64, "V")
        ));
        line_fault = true;
    } else {
        let mirror_zero = info.polarity.is_some_and(|(_, m)| m == Mirror::Zero);
        let outside = if mirror_zero { l.thr.abs() >= vmax.max(-vmin) } else { l.thr >= vmax || l.thr <= vmin };
        if outside {
            say!(format!(
                "the threshold {} is outside the signal ({} to {}): set it between the levels, about {} (Decode > Threshold, AUTO)",
                si(l.thr as f64, "V"),
                si(vmin as f64, "V"),
                si(vmax as f64, "V"),
                si(((vmin + vmax) / 2.0) as f64, "V")
            ));
            line_fault = true;
        }
    }
    // the sample rate against the bit time of the protocol, and a record too short to hold a frame
    let bit = info.bit_time(&l.opt, &l.sh);
    let record = main.len() as f64 * l.dt;
    if bit > 0.0 && l.dt > 0.0 {
        let per_bit = bit / l.dt;
        if per_bit < MIN_SAMPLES_PER_BIT {
            say!(format!(
                "sample rate {} is too low for {}: {:.1} samples per bit, use at least {} (a shorter time base, or less memory depth at the same time base)",
                si(1.0 / l.dt, "Sa/s"),
                si(1.0 / bit, "b/s"),
                per_bit,
                si(MIN_SAMPLES_PER_BIT / bit, "Sa/s")
            ));
        }
        if record < MIN_RECORD_BITS * bit {
            say!(format!(
                "the record is {} long, under {} bit times of {}: use a longer time base or more memory",
                si(record, "s"),
                MIN_RECORD_BITS as u32,
                si(bit, "s")
            ));
        }
        // a line whose edges are far from the bit rate the decoder is set to
        if let Some(rate) = d.bit_rate.filter(|r| {
            let ratio = r * bit;
            !line_fault && !(0.4..=2.5).contains(&ratio)
        }) {
            say!(format!(
                "the edges suggest about {}, the decoder is set to {}: change its bit rate",
                si(rate, "b/s"),
                si(1.0 / bit, "b/s")
            ));
        }
    }
    // polarity: the other reading is clean, or clearly better
    let chosen = info.polarity_of(&l.opt);
    if info.polarity.is_some() && chosen == Polarity::Normal && !line_fault {
        let (np, nf) = (packets.len(), failed);
        if let Some((ip, inf)) = read_as(info, l, Polarity::Inverted) {
            if ip > 0 && inf * 2 < ip && (np == 0 || inf * np < nf * ip) {
                let was = if np == 0 {
                    "nothing decodes".to_string()
                } else if nf == np {
                    format!("all {} words fail their check", np)
                } else {
                    format!("{} of {} words fail their check", nf, np)
                };
                say!(format!(
                    "{}, but the inverted line decodes with {} error{} in {}: the polarity is probably inverted: set Polarity to Auto or Inverted, or use the other line of the pair",
                    was,
                    inf,
                    if inf == 1 { "" } else { "s" },
                    ip
                ));
            }
        }
    }
    // an explanation of last resort
    if out.is_empty() {
        if packets.is_empty() {
            say!("nothing decoded: no frame of this protocol found in the line; check the protocol, its bit rate and the source"
                .to_string());
        } else {
            say!(format!(
                "{} of {} fields fail their check: check the threshold, the bit rate and the polarity",
                failed,
                packets.len()
            ));
        }
    }
    d.lines = out;
    d
}
