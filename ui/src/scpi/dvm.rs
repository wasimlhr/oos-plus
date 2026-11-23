// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! `:DVM`: `ENABle` and `MODE` of the digital voltmeter, declared in settings/meters.rs, and by hand `SOURce` and
//! `CURRent?`. A new `:DVM` setting is a row of that table, another command goes into `dvm_cmd` here; either with its
//! query in ui/tests/scpi.txt, docs/scpi.md gets the row's line from the table, the other's by hand (AGENTS.md).

use super::{e, kw, t, Scpi};
use crate::ui::Ui;

impl Scpi {
    pub(super) fn dvm_cmd(&mut self, ui: &mut Ui, n: &[&str], query: bool, arg: &str) -> Option<Vec<u8>> {
        if let Some(r) = self.meter_cmd(ui, &crate::settings::meters::DVM_SLICES, n, query, arg, |m| &mut m.dvm_src) {
            return r;
        }
        if n.len() == 1 && kw(n[0], "CURRent") && query {
            return t(&e(ui.dvm_value()));
        }
        self.push_err(format!("-113,\"Undefined header\" :DVM:{}", n.join(":")));
        None
    }
}
