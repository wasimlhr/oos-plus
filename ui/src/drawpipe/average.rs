// SPDX-License-Identifier: GPL-2.0-or-later

//! The software Average on the deposit workers. The running average of the record path (`Ui::fold_average`,
//! `condition_record`) takes each frame's screen record x into each point p as `p + (x - p) * a` (a = 1 / count),
//! or x where p is not finite. A run of consecutive frames does the same to whatever p it finds: with finite
//! samples p becomes `g * p + b` (g = (1 - a)^n, b the run folded from 0), where p is not finite `g * x1 + b` (the run
//! folded from its first sample x1), and after a sample that is not finite the run folded from its restart (it no
//! longer depends on p). So the frames of a batch are folded in runs on several cores, in f32 as the record path
//! folds, and the frame loop applies the runs to the average in the frames' order as they arrive: the same average up
//! to the rounding. A run that a worker finishes after the frame loop applied a later one comes with the next step,
//! after it: those frames take the weights of a few places later (with an average of 16 and more, a few per cent of a
//! weight that is itself a few per cent). The idea comes from fumingcaps' fork (release v0.2.1), which folds the Average in ordered runs.

use crate::model::NCH;

/// Consecutive frames folded: the first one's number (`Frame::seq`), how many, of them triggered, and per channel the
/// frames folded, whether no sample that was not finite came (`clean`), and per point `first` (the first sample while
/// the run depends on the point it finds, else NaN) and `b` (the run folded from 0 while it depends, else the run
/// folded from its restart, as the record path folds).
pub struct Run {
    pub seq: u64,
    /// the view it was folded under (`drawpipe::View::gen`)
    pub gen: u64,
    pub frames: usize,
    pub triggered: usize,
    a: f32,
    n: [i32; NCH],
    clean: [bool; NCH],
    first: [Vec<f32>; NCH],
    b: [Vec<f32>; NCH],
}

impl Run {
    /// An empty run from frame `seq` on, for an average of `count`.
    pub fn new(seq: u64, count: u32, gen: u64) -> Run {
        let a = 1.0 / count.max(1) as f32;
        let (first, b) = (Default::default(), Default::default());
        Run { seq, gen, frames: 0, triggered: 0, a, n: [0; NCH], clean: [true; NCH], first, b }
    }

    /// The next frame's screen record (`ch`, its points per channel) folded in; a channel of another number of points
    /// than the run's first is left out of this frame, as the record path leaves it out.
    pub fn push(&mut self, ch: &[Vec<f32>; NCH], triggered: bool) {
        self.push_with(std::array::from_fn(|c| ch[c].len()), |c, k| ch[c][k], triggered);
    }

    /// `push` of a record whose point k of channel c is `x(c, k)`, `len[c]` points (0: the channel is off).
    pub fn push_with(&mut self, len: [usize; NCH], x: impl Fn(usize, usize) -> f32, triggered: bool) {
        self.frames += 1;
        self.triggered += triggered as usize;
        let a = self.a;
        for (c, &n) in len.iter().enumerate() {
            if n == 0 {
                continue;
            }
            if self.n[c] == 0 {
                self.first[c] = (0..n).map(|k| x(c, k)).collect();
                self.b[c] = self.first[c].iter().map(|&v| v * a).collect();
                self.clean[c] = self.first[c].iter().all(|v| v.is_finite());
                self.n[c] = 1;
                continue;
            }
            if self.b[c].len() != n {
                continue;
            }
            self.n[c] += 1;
            let (first, b) = (&mut self.first[c], &mut self.b[c]);
            if self.clean[c] {
                // every point depends on the one the run finds: one multiply-add a point, side by side
                let mut finite = true;
                for (k, p) in b.iter_mut().enumerate() {
                    let v = x(c, k);
                    finite &= v.is_finite();
                    *p += (v - *p) * a;
                }
                if finite {
                    continue;
                }
                // a sample that was not finite: those points no longer depend (folded once more below, as NaN)
                self.clean[c] = false;
                for (k, (f, p)) in first.iter_mut().zip(b.iter_mut()).enumerate() {
                    if !x(c, k).is_finite() {
                        (*f, *p) = (f32::NAN, f32::NAN);
                    }
                }
                continue;
            }
            for (k, (f, p)) in first.iter_mut().zip(b.iter_mut()).enumerate() {
                let v = x(c, k);
                if f.is_finite() {
                    if v.is_finite() {
                        *p += (v - *p) * a;
                    } else {
                        (*f, *p) = (f32::NAN, f32::NAN);
                    }
                } else {
                    *p = if p.is_finite() { *p + (v - *p) * a } else { v };
                }
            }
        }
    }

    /// The run applied to the average `avg` (per channel its points; a channel of another number of points is left as
    /// it is).
    pub fn apply(&self, avg: &mut [Vec<f32>; NCH]) {
        for c in 0..NCH {
            if self.n[c] == 0 || avg[c].len() != self.b[c].len() {
                continue;
            }
            let g = (1.0 - self.a as f64).powi(self.n[c]) as f32;
            for ((v, &f), &b) in avg[c].iter_mut().zip(&self.first[c]).zip(&self.b[c]) {
                *v = match (f.is_finite(), v.is_finite()) {
                    (true, true) => g * *v + b,
                    (true, false) => g * f + b,
                    (false, _) => b,
                };
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The record path's fold of one record into the average (`Ui::fold_average`).
    fn fold(avg: &mut [Vec<f32>; NCH], ch: &[Vec<f32>; NCH], a: f32) {
        for c in 0..NCH {
            if ch[c].len() == avg[c].len() {
                for (p, &x) in avg[c].iter_mut().zip(&ch[c]) {
                    *p = if p.is_finite() { *p + (x - *p) * a } else { x };
                }
            }
        }
    }

    // Break it catches: runs whose average strays from the record path's one record at a time by more than its
    // rounding (a wrong gain, a run folded out of order, a sample that is not finite or a start without an average that
    // is not handled as the record path handles it, a channel of another length).
    #[test]
    fn runs_applied_in_order_give_the_record_paths_average() {
        let mut seed = 5u64;
        let mut rnd = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 40) as f32 / (1u64 << 24) as f32 - 0.5
        };
        for count in [2u32, 16, 64, 1000] {
            let frames: Vec<[Vec<f32>; NCH]> = (0..97)
                .map(|k| {
                    std::array::from_fn(|c| match c {
                        // a channel with gaps now and then (a point outside a record)
                        0 => (0..50)
                            .map(|i| if (i + k) % 23 == 0 { f32::NAN } else { (i as f32 * 0.1).sin() + rnd() })
                            .collect(),
                        1 => (0..50).map(|_| 3.0 + rnd()).collect(),
                        // another number of points from frame 40 on
                        2 => (0..if k < 40 { 50 } else { 30 }).map(|_| rnd()).collect(),
                        _ => Vec::new(),
                    })
                })
                .collect();
            let a = 1.0 / count as f32;
            // the record path from no average (NaN points), and from a finite one
            for start in [f32::NAN, 0.25] {
                let mut want: [Vec<f32>; NCH] = std::array::from_fn(|c| vec![start; frames[0][c].len()]);
                let mut got = want.clone();
                for f in &frames {
                    fold(&mut want, f, a);
                }
                for chunk in frames.chunks(4) {
                    let mut run = Run::new(0, count, 0);
                    for f in chunk {
                        run.push(f, true);
                    }
                    run.apply(&mut got);
                }
                for c in 0..NCH {
                    assert_eq!(got[c].len(), want[c].len());
                    for (i, (g, w)) in got[c].iter().zip(&want[c]).enumerate() {
                        let ok = (g.is_nan() && w.is_nan()) || (g - w).abs() <= 1e-5 * w.abs().max(1.0);
                        assert!(ok, "count {count} start {start} CH{} point {i}: {g} vs {w}", c + 1);
                    }
                }
            }
        }
    }
}
