// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The pulse-width trigger over SCPI: every `:TRIGger:PULSe` command of the stock firmware is set and read back.

use super::*;

/// One line, its answer, and the error queue emptied: a command of the stock set queues nothing.
fn ok(s: &mut Scpi, ui: &mut Ui, cv: &Canvas, cmd: &str) -> String {
    let got = line(s, ui, cv, cmd);
    assert_eq!(line(s, ui, cv, ":SYSTem:ERRor?"), "0,\"No error\"", "{cmd}");
    got
}

#[test]
fn the_pulse_trigger_commands_set_and_read_back() {
    let (mut s, mut ui, cv) = session();
    assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:MODE PULSe;:TRIGger:MODE?"), "PULS");
    // the sources: every analog channel, by long and short name
    for (cmd, want) in [("CHANnel1", "CHAN1"), ("CHAN2", "CHAN2"), ("CHANnel3", "CHAN3"), ("chan4", "CHAN4")] {
        ok(&mut s, &mut ui, &cv, &format!(":TRIGger:PULSe:SOURce {cmd}"));
        assert_eq!(ui.m.trig_type, TrigType::Pulse, "{cmd}: the source leaves the mode alone");
        assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:SOURce?"), want);
        assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:SOURce?"), want);
    }
    // polarity
    for (set, want) in [("POSitive", "POS"), ("NEGative", "NEG"), ("POS", "POS"), ("NEG", "NEG")] {
        ok(&mut s, &mut ui, &cv, &format!(":TRIGger:PULSe:POLarity {set}"));
        assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:POLarity?"), want);
    }
    // the level
    ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:LEVel 0.75");
    assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:LEVel?"), "7.500000E-01");
    // the condition: its own words, and the stock names that carry the polarity with them
    for (set, want) in [("GREater", "GRE"), ("LESS", "LESS"), ("RANGe", "RANG"), ("OUTside", "OUT")] {
        ok(&mut s, &mut ui, &cv, &format!(":TRIGger:PULSe:WHEN {set}"));
        assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:WHEN?"), want);
    }
    for (set, pol, when) in [
        ("PGReater", Pol::Pos, WhenKind::Gt),
        ("PLESs", Pol::Pos, WhenKind::Lt),
        ("PGLess", Pol::Pos, WhenKind::In),
        ("NGReater", Pol::Neg, WhenKind::Gt),
        ("NLESs", Pol::Neg, WhenKind::Lt),
        ("NGLess", Pol::Neg, WhenKind::In),
    ] {
        ok(&mut s, &mut ui, &cv, &format!(":TRIGger:PULSe:WHEN {set}"));
        assert_eq!((ui.m.trig_pol, ui.m.trig_when), (pol, when), "{set}");
    }
    // the widths: WIDTh and LWIDth are the lower limit (the one width of greater and less), UWIDth the upper limit
    ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:LWIDth 4e-6;:TRIGger:PULSe:UWIDth 9e-6");
    assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:LWIDth?;:TRIGger:PULSe:UWIDth?"), "4.000000E-06;9.000000E-06");
    assert_eq!((ui.m.trig_t1, ui.m.trig_t2), (4e-6, 9e-6));
    ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:WIDTh 5e-6");
    assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:WIDTh?;:TRIGger:PULSe:LWIDth?"), "5.000000E-06;5.000000E-06");
    // a value that is no width, no polarity, no When and no level is refused and changes nothing
    for bad in [
        ":TRIGger:PULSe:POLarity UP",
        ":TRIGger:PULSe:WHEN SOON",
        ":TRIGger:PULSe:LWIDth -1",
        ":TRIGger:PULSe:UWIDth 0",
        ":TRIGger:PULSe:LEVel x",
    ] {
        let before = (ui.m.trig_pol, ui.m.trig_when, ui.m.trig_t1, ui.m.trig_level);
        line(&mut s, &mut ui, &cv, bad);
        assert!(line(&mut s, &mut ui, &cv, ":SYSTem:ERRor?").starts_with("-224"), "{bad}");
        assert_eq!(before, (ui.m.trig_pol, ui.m.trig_when, ui.m.trig_t1, ui.m.trig_level), "{bad}");
    }
    line(&mut s, &mut ui, &cv, ":TRIGger:PULSe:FOO 1");
    assert!(line(&mut s, &mut ui, &cv, ":SYSTem:ERRor?").starts_with("-113"));
}

#[test]
fn the_pulse_trigger_takes_a_logic_line_and_a_reference_as_source() {
    let (mut s, mut ui, cv) = session();
    ok(&mut s, &mut ui, &cv, ":LA:STAT ON;:TRIGger:PULSe:SOURce D5");
    assert_eq!(ui.m.trig_type, TrigType::Pulse);
    assert_eq!(ui.m.trig_line(), Some(5), "D5 is the pulse trigger's source");
    assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:SOURce?;:TRIGger:SOURce?"), "D5;D5");
    ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:SOURce CHAN2");
    assert_eq!(ui.m.trig_line(), None);
    // the other types keep refusing a logic line they cannot use
    line(&mut s, &mut ui, &cv, ":TRIGger:SLOPe:SOURce D5");
    assert!(line(&mut s, &mut ui, &cv, ":SYSTem:ERRor?").starts_with("-224"));
    // without the logic analyser a line is refused
    let (mut s, mut ui, cv) = session();
    line(&mut s, &mut ui, &cv, ":TRIGger:PULSe:SOURce D5");
    assert!(line(&mut s, &mut ui, &cv, ":SYSTem:ERRor?").starts_with("-224"));
    // a reference, as for the edge trigger
    let (mut s, mut ui, cv) = reference_session();
    ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:SOURce R1");
    assert_eq!(ok(&mut s, &mut ui, &cv, ":TRIGger:PULSe:SOURce?"), "R1");
}
