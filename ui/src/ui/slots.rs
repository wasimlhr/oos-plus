// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Numbered measurement slots `MEAS<n>` (n = 1..=`SLOTS`): a slot holds one row of the measurement table, so a
//! measurement is added to a slot first (it is computed and is a row on the screen) and read back from it by number.
//! The number is kept in `Model::meas_slots` against what the row measures, not against its place in the table, so it
//! stays when other rows are added or removed; a slot whose row is gone reads as empty.

use super::*;
use crate::model::RowKey;

/// Slots there are: `MEAS1` to `MEAS99`.
pub(crate) const SLOTS: u8 = 99;

impl Ui {
    /// The row of slot `n`, when the slot holds one that is still in the table.
    pub(crate) fn slot_row(&self, n: u8) -> Option<RowKey> {
        let key = self.m.meas_slots.iter().find(|(o, _)| *o == n).map(|(_, k)| *k)?;
        self.m.row_keys().contains(&key).then_some(key)
    }

    /// The slots in use, by number, with their rows.
    pub(crate) fn slots_in_use(&self) -> Vec<(u8, RowKey)> {
        let live = self.m.row_keys();
        let mut v: Vec<(u8, RowKey)> = self.m.meas_slots.iter().filter(|(_, k)| live.contains(k)).copied().collect();
        v.sort_by_key(|(n, _)| *n);
        v
    }

    /// The number of the slot a row is in.
    pub(crate) fn slot_of(&self, key: RowKey) -> Option<u8> {
        self.m.meas_slots.iter().find(|(_, k)| *k == key).map(|(n, _)| *n)
    }

    /// Puts a row of the table in slot `n`. A slot that held another row gives that row up (it leaves the table), and
    /// a row is in one slot only, so a row that was in another slot moves.
    pub(crate) fn slot_assign(&mut self, n: u8, key: RowKey) {
        if let Some(old) = self.slot_row(n).filter(|old| *old != key) {
            self.slot_remove_row(old);
        }
        let live = self.m.row_keys();
        self.m.meas_slots.retain(|(o, k)| *o != n && *k != key && live.contains(k));
        self.m.meas_slots.push((n, key));
        self.m.meas_slots.sort_by_key(|(n, _)| *n);
        self.mark_changed();
    }

    /// Empties slot `n` and takes its row off the table; false when the slot was empty.
    pub(crate) fn slot_clear(&mut self, n: u8) -> bool {
        let Some(key) = self.slot_row(n) else {
            self.m.meas_slots.retain(|(o, _)| *o != n);
            return false;
        };
        self.slot_remove_row(key);
        self.m.meas_slots.retain(|(o, _)| *o != n);
        true
    }

    fn slot_remove_row(&mut self, key: RowKey) {
        let index = match key {
            RowKey::One(k, id) => self.m.meas.iter().position(|&e| e == (k, id)),
            RowKey::Pair(k, a, b) => self.m.meas2.iter().position(|&e| e == (k, a, b)).map(|i| self.m.meas.len() + i),
            RowKey::Cat(..) => None,
        };
        if let Some(i) = index {
            self.remove_measurement(i);
        }
    }

    /// The value of a row, as the table shows it (NaN: none yet).
    pub(crate) fn slot_value(&self, key: RowKey) -> f64 {
        match key {
            RowKey::One(k, id) => self.fact_meas(k, id),
            RowKey::Pair(k, a, b) => self.pair_at(k, a, b),
            RowKey::Cat(..) => f64::NAN,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The badge of a slot leads the name of its row in the table; a row without a slot has none.
    #[test]
    fn a_numbered_row_shows_its_badge() {
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        ui.m.meas.clear();
        ui.m.meas2.clear();
        ui.m.cat.clear();
        ui.meas_add(Meas::Vpp, WaveId::Ch(0)).unwrap();
        ui.meas_add(Meas::Vmax, WaveId::Ch(0)).unwrap();
        ui.slot_assign(12, RowKey::One(Meas::Vmax, WaveId::Ch(0)));
        let names: Vec<String> = ui.meas_rows().into_iter().map(|r| r.name).collect();
        assert!(!names[0].starts_with("M12 "), "{names:?}");
        assert!(names[1].starts_with("M12 "), "{names:?}");
    }
}
