// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The bus on the plot: its switch, its place and whether the address byte holds the read/write bit. They have no
//! commands; their rows are the Bus menu (ui/bus.rs, ui/setting_items.rs). The slices are the places of
//! `store::setup_text` that write them, so the keys of a setup file keep their order.
//!
//! Still written out by hand: the label (free text, the menu edits it on the keyboard), the clock and the data line
//! (wave sources), their thresholds (the menu steps by a 25th of the scale of the line, which for a reference slot is
//! not in the model) and the kind (I2C is the only one, the menu has no row for it).

use super::{CustomIo, Scpi, Setting, Ty, Val};
use crate::model::{BUS_POS_MAX, BUS_POS_MIN};

/// The slices, in the order of the docs table.
pub const SLICES: [&[Setting]; 3] = [&ON, &POS, &RW];

/// Written first of the bus.
pub const ON: [Setting; 1] = [Setting {
    key: "bus_on",
    scpi: Scpi::none("a trace of the screen, not of the decoders"),
    ty: Ty::Bool,
    label: "Bus",
    #[cfg(test)]
    doc: "Draws the bus on the plot: the words of I2C from a clock and a data line, in hex.",
    accept: None,
    step: None,
    get: |m, _| Val::Bool(m.bus.on),
    put: |m, _, v| m.bus.on = v.index() != 0,
}];

/// Written after `bus_label`.
pub const POS: [Setting; 1] = [Setting {
    key: "bus_pos",
    scpi: Scpi::none("a trace of the screen, not of the decoders"),
    ty: Ty::Real { range: Some((BUS_POS_MIN as f64, BUS_POS_MAX as f64)), shown: |x| format!("{:.1} div", x) },
    label: "Position",
    #[cfg(test)]
    doc: "Where the bus is drawn, in divisions above the centre; the menu steps by a tenth of a division.",
    accept: None,
    step: Some(|m, _, d| Val::F32((m.bus.pos + 0.1 * d as f32).clamp(BUS_POS_MIN, BUS_POS_MAX))),
    get: |m, _| Val::F32(m.bus.pos),
    put: |m, _, v| m.bus.pos = v.secs(),
}];

/// Written last of the bus.
pub const RW: [Setting; 1] = [Setting {
    key: "bus_rw",
    scpi: Scpi::none("a trace of the screen, not of the decoders"),
    ty: Ty::Custom(&RW_IO),
    label: "R/W bit",
    #[cfg(test)]
    doc: "Whether the address byte shows the read/write bit or only the seven bits of the address.",
    accept: None,
    step: None,
    get: |m, _| Val::Bool(m.bus.addr_rw),
    put: |m, _, v| m.bus.addr_rw = v.index() != 0,
}];

/// A switch in a setup; the menu names its two sides.
const RW_IO: CustomIo = CustomIo {
    store: |v| (v.index() != 0).to_string(),
    load: |t| crate::words::p_bool(t).map(Val::Bool),
    query: |v| (v.index() != 0).to_string(),
    take: |_| None,
    labels: &["Left out", "Included"],
    #[cfg(test)]
    values: "none",
};
