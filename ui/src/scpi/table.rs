// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:TABLe`: every table the screen shows, by name, with its Index in the first column: `LIST?`, `COUNt? <name>`,
//! `DATA? <name>` (CSV in an IEEE block) and `SAVE <name>[,"path"]` (CSV file). The tables are built in
//! `crate::ui::tabledata` (`TABLES`); the measurement strip is built here, because the SCPI layer has its rows. A new
//! table is a line of `TABLES` and an arm of `Ui::table_data` (or `table_of` here), with its line in docs/scpi/ and
//! its query in ui/tests/scpi.txt (AGENTS.md).

use super::math::unquote;
use super::{block, kw, t, Scpi};
use crate::ui::tabledata::{Table, TABLES};
use crate::ui::Ui;

/// A table by its name: the measurement strip here, the others in the screen's part.
pub(crate) fn table_of(ui: &Ui, name: &str) -> Result<Table, &'static str> {
    if !name.eq_ignore_ascii_case("MEASURE") {
        return ui.table_data(name);
    }
    let strip = super::measure::meas_strip(ui);
    let rows = strip.split(';').filter(|r| !r.is_empty()).enumerate().map(|(i, row)| {
        let f: Vec<&str> = row.split(',').collect();
        let (item, value) = (f[0], f[f.len() - 1]);
        vec![(i + 1).to_string(), item.to_string(), f[1..f.len() - 1].join(" "), value.to_string()]
    });
    Ok(Table::new(&["Index", "Item", "Source", "Value"], rows.collect()))
}

/// What `:TABLe:LIST?` answers: a name and what the table is, a line each.
pub(crate) fn table_list() -> String {
    TABLES.iter().map(|(name, what)| format!("{name},{what}")).collect::<Vec<_>>().join("\n")
}

impl Scpi {
    /// `:TABLe` (`n` without the root).
    pub(super) fn table_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let n0 = n.first().copied().unwrap_or("");
        if n.len() == 1 && query && kw(n0, "LIST") {
            return t(&table_list());
        }
        if n.len() == 1 && (kw(n0, "COUNt") || kw(n0, "DATA") || kw(n0, "SAVE")) {
            let (name, path) = match arg.split_once(',') {
                Some((name, path)) => (unquote(name.trim()), Some(path.trim())),
                None => (unquote(arg.trim()), None),
            };
            let table = match table_of(ui, name) {
                Ok(table) => table,
                Err("Decode computing") if query => return t("BUSY"),
                Err(error) => {
                    self.push_err(format!("-224,\"Illegal parameter value\" {error}: {name}"));
                    return None;
                }
            };
            if kw(n0, "COUNt") && query {
                return t(&table.rows.len().to_string());
            }
            if kw(n0, "DATA") && query {
                return Some(block(table.csv().as_bytes()));
            }
            if kw(n0, "SAVE") && !query {
                let path = path.filter(|p| !p.is_empty()).map(|p| ui.out_path(p));
                if let Err(error) = ui.table_save(name, &table, path) {
                    self.push_err(error);
                }
                return None;
            }
        }
        self.push_err(format!("-113,\"Undefined header\" :TABLe:{}", n.join(":")));
        None
    }
}
