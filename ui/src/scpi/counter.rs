// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:COUNter`: `ENABle`, `MODE` and `NDIGits` of the frequency counter, declared in settings/meters.rs, and by hand
//! `SOURce`, `CURRent?` and `TOTalize:CLEar`. A new `:COUNter` setting is a row of that table, another command goes
//! into `counter_cmd` here; either with its query in ui/tests/scpi.txt, docs/scpi.md gets the row's line from the
//! table, the other's by hand (AGENTS.md).

use super::{e, kw, t, Scpi};
use crate::ui::Ui;

impl Scpi {
    pub(super) fn counter_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        let slices = &crate::settings::meters::COUNTER_SLICES;
        if let Some(r) = self.meter_cmd(ui, slices, n, query, arg, |m| &mut m.cnt_src) {
            return r;
        }
        let n0 = n.first().copied().unwrap_or("");
        if n.len() == 1 && query && (kw(n0, "CURRent") || kw(n0, "VALue")) {
            return t(&e(ui.counter_value()));
        }
        if n.len() == 2 && kw(n0, "TOTalize") && kw(n[1], "CLEar") && !query {
            ui.counter_clear();
            return None;
        }
        self.push_err(format!("-113,\"Undefined header\" :COUNter:{}", n.join(":")));
        None
    }
}
