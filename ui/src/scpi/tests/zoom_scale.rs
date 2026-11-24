// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:TIMebase:DELay:SCALe` and `:ENABle`: which zoom scale a request leaves in effect.

use crate::model::tscales;
use crate::ui::testkit::{q, scpi_session};

/// The zoom scale in effect after `:TIMebase:SCALe <main>`, then the requests in `order`; the ladder step stored and
/// the answer of `:TIMebase:DELay:SCALe?` (one value: the screen reads the stored step).
fn zoom_after(main: &str, enable_first: bool, want: &str) -> (f32, String) {
    let (mut s, mut ui, cv) = scpi_session();
    q(&mut s, &mut ui, &cv, &format!(":TIMebase:SCALe {}", main));
    let scale = format!(":TIMebase:DELay:SCALe {}", want);
    if enable_first {
        q(&mut s, &mut ui, &cv, ":TIMebase:DELay:ENABle 1");
        q(&mut s, &mut ui, &cv, &scale);
    } else {
        q(&mut s, &mut ui, &cv, &scale);
        q(&mut s, &mut ui, &cv, ":TIMebase:DELay:ENABle 1");
    }
    let a = q(&mut s, &mut ui, &cv, ":TIMebase:DELay:SCALe?");
    assert_eq!(a.trim().parse::<f32>().unwrap(), tscales()[ui.m.zscale], "answer and stored step differ");
    (tscales()[ui.m.zscale], a)
}

// Break it catches: an enable that drops the scale asked before it (2.5 us ended as 5 us), or a request rounded
// differently by the order of the commands.
#[test]
fn a_zoom_request_gives_the_same_step_in_either_order() {
    for enable_first in [false, true] {
        for (want, step) in [("2.5E-6", 2e-6), ("3E-6", 2e-6), ("4E-6", 5e-6), ("5E-6", 5e-6)] {
            assert_eq!(zoom_after("50E-6", enable_first, want).0, step, "{} enable_first {}", want, enable_first);
        }
    }
}

#[test]
fn a_zoom_scale_is_never_slower_than_the_main_time_base() {
    for enable_first in [false, true] {
        assert_eq!(zoom_after("2E-6", enable_first, "5E-6").0, 2e-6);
    }
}

#[test]
fn the_zoom_opens_three_steps_below_the_main_time_base_when_none_was_asked() {
    let (mut s, mut ui, cv) = scpi_session();
    q(&mut s, &mut ui, &cv, ":TIMebase:SCALe 50E-6");
    q(&mut s, &mut ui, &cv, ":TIMebase:DELay:ENABle 1");
    assert_eq!(q(&mut s, &mut ui, &cv, ":TIMebase:DELay:SCALe?").trim().parse::<f32>().unwrap(), 5e-6);
}
