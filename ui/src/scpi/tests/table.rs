// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Tests of `:TABLe` and `:DECode:EVENts:COUNt?` (scpi/table.rs, ui/src/ui/tabledata.rs). A test of a new table goes
//! here; its query also goes into ui/tests/scpi.txt, its line into docs/scpi/table.md (AGENTS.md).

use super::*;

/// The text of an IEEE block answer.
fn text(answer: &str) -> &str {
    let n = answer[1..2].parse::<usize>().unwrap();
    &answer[2 + n..]
}

// Break it catches: a table in the list that cannot be read, a table that does not begin with its Index, a count that
// disagrees with the rows, a name that is not a table answered with rows, a CSV file written without its head
#[test]
fn every_table_is_listed_counted_read_and_saved_by_name() {
    let (mut s, mut ui, cv) = session();
    let list = line(&mut s, &mut ui, &cv, ":TABLe:LIST?");
    let names: Vec<&str> = list.lines().filter_map(|l| l.split(',').next()).collect();
    assert_eq!(names, ["EVENTS", "FIELDS", "SEARCH", "SEGMENTS", "PEAKS", "MEASURE"]);
    for name in names {
        let data = line(&mut s, &mut ui, &cv, &format!(":TABLe:DATA? {name}"));
        let csv = text(&data);
        assert!(csv.starts_with("Index,"), "{name}: {csv}");
        let count = line(&mut s, &mut ui, &cv, &format!(":TABLe:COUNt? {name}"));
        assert_eq!(count.trim().parse::<usize>().unwrap(), csv.lines().count() - 1, "{name}");
    }
    assert!(line(&mut s, &mut ui, &cv, ":TABLe:COUNt? NOSUCH").starts_with("-224"));
    let dir = crate::ui::testkit::Scratch::new("tables");
    std::fs::create_dir_all(&*dir).unwrap();
    let path = dir.join("peaks.csv");
    line(&mut s, &mut ui, &cv, &format!(":TABLe:SAVE PEAKS,\"{}\"", path.display()));
    assert!(std::fs::read_to_string(&path).unwrap().starts_with("Index,Frequency (Hz),Level (dBV)\n"));
}

// Break it catches: the measurement table without its slots' items, the decode count disagreeing with the decode table
#[test]
fn the_measurement_table_and_the_decode_count_follow_the_screen() {
    let (mut s, mut ui, cv) = session();
    ui.m.meas.clear();
    ui.m.meas2.clear();
    assert_eq!(line(&mut s, &mut ui, &cv, ":TABLe:COUNt? MEASURE").trim(), "0");
    line(&mut s, &mut ui, &cv, ":MEASure:ITEM VPP,CHAN1");
    let t = line(&mut s, &mut ui, &cv, ":TABLe:DATA? MEASURE");
    let csv = text(&t);
    assert!(csv.lines().nth(1).is_some_and(|row| row.starts_with("1,VPP,CHAN1,")), "{csv}");
    line(&mut s, &mut ui, &cv, ":DECode:STATe 1");
    let n = line(&mut s, &mut ui, &cv, ":DECode:EVENts:COUNt?");
    let rows = line(&mut s, &mut ui, &cv, ":TABLe:COUNt? EVENTS");
    assert_eq!(n, rows);
}
