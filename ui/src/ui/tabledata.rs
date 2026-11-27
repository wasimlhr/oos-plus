// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Every table the screen shows, as data: a head and rows whose first cell is the row's Index (its number in the
//! list of the record, from 1, the same in the pane, in the CSV file and in the answer of `:TABLe:DATA?`). One
//! mechanism serves the SCPI commands (`:TABLe:LIST?`, `:TABLe:DATA?`, `:TABLe:COUNt?`, `:TABLe:SAVE`), the MCP tool
//! `table` and the Save-as-CSV buttons, so a table is added once.

use super::*;

/// The names of the tables this part builds (the measurement table is built by the SCPI layer, which has its rows).
pub const TABLES: [(&str, &str); 6] = [
    ("EVENTS", "decode table: a row per message, field or line (the Events pane, by :DECode:EVENts:VIEW)"),
    ("FIELDS", "decode fields: a row per field of every decoder (the rows of :DECode:EVENts?)"),
    ("SEARCH", "the events of the search"),
    ("SEGMENTS", "the hardware segments and their trigger times"),
    ("PEAKS", "the peak table of the spectrum"),
    ("MEASURE", "the measurement strip: item, sources, value"),
];

/// A table: its name, the titles of its columns (the first is "Index") and its rows.
#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub head: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

/// A cell of a CSV file: in quotes where it holds a comma, a quote or a line break.
fn csv_cell(s: &str) -> String {
    if s.contains([',', '"', '\n']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

impl Table {
    pub fn new(head: &[&str], rows: Vec<Vec<String>>) -> Self {
        Table { head: head.iter().map(|s| s.to_string()).collect(), rows }
    }

    /// The table as CSV, a line per row, the head first.
    pub fn csv(&self) -> String {
        let line = |cells: &[String]| cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(",");
        let mut t = line(&self.head);
        for row in &self.rows {
            t.push('\n');
            t.push_str(&line(row));
        }
        t.push('\n');
        t
    }
}

fn seconds(t: f64) -> String {
    if t.is_finite() {
        format!("{:.9e}", t)
    } else {
        String::new()
    }
}

impl Ui {
    /// The words of the Events pane's count line for the rows it lists now: "36 words decoded".
    pub fn events_count_text(&self, n: usize) -> String {
        let one = n == 1;
        let word = match (self.m.ev_view, self.m.dec_proto.name()) {
            (EvView::Fields, "1553") => {
                if one {
                    "word"
                } else {
                    "words"
                }
            }
            (EvView::Fields, _) => {
                if one {
                    "field"
                } else {
                    "fields"
                }
            }
            (EvView::Lines, _) => {
                if one {
                    "line"
                } else {
                    "lines"
                }
            }
            (EvView::Messages, _) => {
                if one {
                    "message"
                } else {
                    "messages"
                }
            }
        };
        format!("{n} {word} decoded")
    }

    /// The rows the Events pane lists, all of them (the "Errors only" filter does not apply): the count of
    /// `:DECode:EVENts:COUNt?`.
    pub fn events_total(&self) -> Result<usize, &'static str> {
        self.events_wait()?;
        Ok(self.decode_rows().len())
    }

    /// Err("Decode computing") while a decoder that is on is still working.
    fn events_wait(&self) -> Result<(), &'static str> {
        for k in 0..DECODERS {
            if self.m.dec_is_on(k) {
                self.decode_packets_of(&self.m.dec_view(k))?;
            }
        }
        Ok(())
    }

    /// The Events pane as a table: Index, time, end, source, the number of fields, the columns of the protocol (or
    /// the text and the hex bytes), and the error in words.
    fn events_table(&self) -> Result<Table, &'static str> {
        self.events_wait()?;
        let rows = self.decode_rows();
        let own =
            rows.first().map(|r| r.cols).filter(|c| !c.is_empty() && rows.iter().all(|r| std::ptr::eq(r.cols, *c)));
        let mut head = vec!["Index", "Time (s)", "End (s)", "Source", "Len"];
        match own {
            Some(cols) => head.extend(cols.iter().map(|c| c.0)),
            None => head.extend(["Text", "Hex"]),
        }
        head.push("Error");
        let cells = rows.iter().map(|e| {
            let mut row = vec![e.index.to_string(), seconds(e.t), seconds(e.t1), e.who.clone(), e.len.to_string()];
            match own {
                Some(cols) => row.extend((0..cols.len()).map(|k| e.cells.get(k).cloned().unwrap_or_default())),
                None => row.extend([e.text.clone(), e.hex.clone()]),
            }
            row.push(e.error_text());
            row
        });
        Ok(Table::new(&head, cells.collect()))
    }

    fn fields_table(&self) -> Result<Table, &'static str> {
        self.events_wait()?;
        let rows = self.decode_events().into_iter().enumerate();
        let cells = rows.map(|(i, e)| {
            vec![(i + 1).to_string(), seconds(e.t), e.who, e.text, if e.ok { "ok" } else { "error" }.into(), e.why]
        });
        Ok(Table::new(&["Index", "Time (s)", "Source", "Event", "Ok", "Error"], cells.collect()))
    }

    /// A table by its name (`TABLES`, but MEASURE), or why there is none.
    pub fn table_data(&self, name: &str) -> Result<Table, &'static str> {
        match name.to_ascii_uppercase().as_str() {
            "EVENTS" => self.events_table(),
            "FIELDS" => self.fields_table(),
            "SEARCH" => {
                let rows = self.table_rows(tables::TableKind::Search).into_iter().enumerate();
                let cells = rows.map(|(i, e)| vec![(i + 1).to_string(), seconds(e.t), e.who, e.text]);
                Ok(Table::new(&["Index", "Time (s)", "Source", "Event"], cells.collect()))
            }
            "SEGMENTS" => {
                let rows = self.table_rows(tables::TableKind::Segments).into_iter();
                let cells = rows.map(|e| vec![e.who, seconds(e.t), e.text]);
                Ok(Table::new(&["Index", "Time since first (s)", "Step"], cells.collect()))
            }
            "PEAKS" => {
                let peaks = self.azr.peaks.iter().enumerate();
                let cells =
                    peaks.map(|(i, p)| vec![(i + 1).to_string(), format!("{:e}", p.f), format!("{:.2}", p.dbv)]);
                Ok(Table::new(&["Index", "Frequency (Hz)", "Level (dBV)"], cells.collect()))
            }
            _ => Err("No such table"),
        }
    }

    /// Saves a table as CSV: to `path`, or into a new file of the storage directory.
    pub fn table_save(&mut self, name: &str, table: &Table, path: Option<std::path::PathBuf>) -> Result<(), String> {
        let p = path.unwrap_or_else(|| self.new_file(&self.save_dir(), &name.to_ascii_lowercase(), "csv"));
        let r = crate::store::write(&p, table.csv().as_bytes());
        let failed = r.as_ref().err().map(|e| format!("-250,\"Mass storage error\" {e}"));
        self.toast_saved(&p, r);
        failed.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The 1553 demo on a demo screen, stopped on a record with messages.
    fn demo_1553() -> Ui {
        let mut ui = crate::ui::testkit::demo_ui();
        ui.set_dec_proto(Proto::from_key("Mil1553").unwrap());
        ui.m.dec_on = true;
        ui.m.tscale = tscales().iter().position(|&x| (x - 20e-6).abs() < 20e-6 * 1e-3).unwrap();
        ui.m.trig_type = TrigType::Edge;
        ui.m.trig_mode = TrigMode::Auto;
        for _ in 0..6 {
            ui.step(0.05);
        }
        ui
    }

    // Break it catches: a table without its Index first, an Index that is not the number of the row in the record,
    // the 1553 columns of the Tektronix table missing (type, terminal, direction, subaddress, status bits, payload,
    // error), the count line and the count query disagreeing with the rows
    #[test]
    fn the_1553_decode_table_has_an_index_and_the_columns_of_the_tektronix_table() {
        let ui = demo_1553();
        let messages = ui.table_data("EVENTS").unwrap();
        assert_eq!(&messages.head[..2], ["Index", "Time (s)"]);
        for want in ["RT", "T/R", "SA", "WC/MC", "Data", "Status", "Error"] {
            assert!(messages.head.iter().any(|h| h == want), "{want}: {:?}", messages.head);
        }
        assert!(!messages.rows.is_empty());
        for (i, row) in messages.rows.iter().enumerate() {
            assert_eq!(row[0], (i + 1).to_string());
            assert_eq!(row.len(), messages.head.len());
        }
        let mut ui = ui;
        ui.m.ev_view = EvView::Fields;
        let fields = ui.table_data("EVENTS").unwrap();
        for want in
            ["Type", "RT", "T/R", "SA", "Mode code / words", "Data", "Status", "Status bits", "C/S payload", "Error"]
        {
            assert!(fields.head.iter().any(|h| h == want), "{want}: {:?}", fields.head);
        }
        let col = |name: &str| fields.head.iter().position(|h| h == name).unwrap();
        // the command of the demo: terminal 1, receive, subaddress 1, 3 words; its payload is the 11 bits behind the terminal
        let command = fields.rows.iter().find(|r| r[col("Type")] == "Command").expect("a command word");
        assert_eq!(command[col("RT")], "01");
        assert_eq!(command[col("C/S payload")].len(), 11);
        let status = fields.rows.iter().find(|r| r[col("Type")] == "Status").expect("a status word");
        assert_eq!(status[col("Status bits")].matches('-').count(), 2);
        assert_eq!(ui.events_total().unwrap(), fields.rows.len());
        assert_eq!(ui.events_count_text(fields.rows.len()), format!("{} words decoded", fields.rows.len()));
        // the rows of :DECode:EVENts? and the table FIELDS have the same Index first
        let text = ui.events_text();
        assert_eq!(text.lines().count(), ui.table_data("FIELDS").unwrap().rows.len());
        assert!(text.lines().next().unwrap().starts_with("1,"), "{text}");
    }

    // Break it catches: `:DECode:EVENts?` and the table FIELDS marking a word ok that the Error column of the Events
    // pane flags (an agent read 63 ok words under a table of 63 errors)
    #[test]
    fn the_rows_of_the_event_query_agree_with_the_error_column() {
        let mut ui = demo_1553();
        // a record cut short: the last message loses words, so a message error appears
        ui.m.ev_view = EvView::Fields;
        let rows = ui.decode_events();
        assert!(!rows.is_empty());
        for r in &rows {
            assert_eq!(r.ok, r.why.is_empty() && r.ok, "{r:?}");
        }
        let fields = ui.table_data("FIELDS").unwrap();
        let (ok, err) = (fields.head.iter().position(|h| h == "Ok").unwrap(), fields.head.len() - 1);
        assert_eq!(fields.head[err], "Error");
        for row in &fields.rows {
            assert_eq!(row[ok] == "ok", row[err].is_empty(), "{row:?}");
        }
        // the text of the query: an error row ends with its reason
        for (line, r) in ui.events_text().lines().zip(&rows) {
            assert_eq!(line.ends_with(&format!(",error,{}", r.why)) || r.ok || r.why.is_empty(), true, "{line}");
        }
    }

    // Break it catches: a decode error left as a bare "error" for a message that is incomplete or has the wrong count
    #[test]
    fn a_1553_message_that_is_not_whole_says_what_is_wrong() {
        use oos_dsp::decode::{Kind, Packet, MIL1553_COMMAND as C};
        let proto = Proto::from_key("Mil1553").unwrap();
        let module = proto.module().unwrap();
        let w = |t: f64, cmd: bool, value: u32, value2: Option<u32>, ok: bool| Packet {
            t0: t * 20e-6,
            t1: t * 20e-6 + 20e-6,
            kind: Kind::Word { cmd },
            value,
            value2,
            ok,
        };
        // RT 1 receive SA 1, 2 words
        let command = w(0.0, true, 0x0822, Some(C), true);
        let whole = [command.clone(), w(1.0, false, 1, None, true), w(2.0, false, 2, None, true)];
        assert_eq!(module.message_error(&whole), "");
        assert_eq!(module.message_error(&whole[..2]), "Word count mismatch");
        assert_eq!(module.message_error(&whole[..1]), "Incomplete packet");
        assert_eq!(module.message_error(&whole[1..2]), "Incomplete packet; Unframed");
        assert_eq!(module.message_error(&whole[1..]), "Unframed");
        let gap = [command.clone(), w(1.0, false, 1, None, true), w(3.0, false, 2, None, true)];
        assert_eq!(module.message_error(&gap), "Data: Noncontiguous Data");
        let bad = [command, w(1.0, false, 1, None, false), w(2.0, false, 2, None, true)];
        assert_eq!(module.message_error(&bad), "Manchester or parity error");
    }

    #[test]
    fn every_table_is_there_by_name_with_its_index_first() {
        let ui = demo_1553();
        for (name, _) in TABLES.iter().filter(|(n, _)| *n != "MEASURE") {
            let t = ui.table_data(name).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert_eq!(t.head[0], "Index", "{name}");
            assert!(t.csv().starts_with("Index,"), "{name}");
        }
        assert!(ui.table_data("NOSUCH").is_err());
    }

    #[test]
    fn csv_quotes_what_needs_it() {
        let t = Table::new(&["Index", "Text"], vec![vec!["1".into(), "a,b \"c\"".into()]]);
        assert_eq!(t.csv(), "Index,Text\n1,\"a,b \"\"c\"\"\"\n");
    }
}
