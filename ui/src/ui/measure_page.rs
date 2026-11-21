// SPDX-License-Identifier: GPL-2.0-or-later
//! The Measure settings page (the top bar's Measure): its rows, the places the Dock row offers and their names, and the
//! layout of upstream's page in the Default layout, two columns of named groups. Minimal and Studio open the levels
//! and the statistics as pages of their own and lay the page out as every other one.

use super::*;

impl Ui {
    /// The measurement menu: upstream's rows in the Default layout; Minimal and Studio open the levels and the
    /// statistics as pages of their own.
    pub(super) fn measure_items(&self) -> Vec<Item> {
        if self.m.layout == Layout::Default {
            return vec![
                Item::MeasSrc,
                Item::MeasSrcB,
                Item::MeasCopyTo,
                Item::MeasGate,
                Item::MeasRefs,
                Item::MeasStats,
                Item::MeasLabels,
                Item::StripDock,
                Item::MeasIdle,
                Item::MeasTsetup,
                Item::MeasThold,
                Item::MeasOutside,
                Item::MeasN,
                Item::MeasEdges,
                Item::Open(MenuId::MeasPair),
                Item::Open(MenuId::Dvm),
                Item::MeasClear,
            ];
        }
        let mut v = vec![
            Item::MeasSrc,
            Item::MeasSrcB,
            Item::MeasCopyTo,
            Item::MeasGate,
            Item::Open(MenuId::MeasPair),
            Item::Open(MenuId::Dvm),
            Item::Open(MenuId::MeasLevels),
            Item::MeasIdle,
            Item::MeasTsetup,
            Item::MeasThold,
            Item::MeasOutside,
            Item::MeasN,
            Item::MeasEdges,
            Item::Open(MenuId::MeasStat),
            Item::MeasLabels,
            Item::StripDock,
            Item::MeasClear,
        ];
        if self.m.layout == Layout::Studio {
            v.retain(|i| *i != Item::StripDock);
            let g = v.iter().position(|i| *i == Item::MeasGate).unwrap_or(0);
            v.insert(g + 1, Item::BadgeStyle);
        }
        if self.sheets_on() {
            // the sheet has the levels on a tab of their own; the stock layout opens them from one row
            let at = v.iter().position(|i| *i == Item::Open(MenuId::MeasLevels)).unwrap_or(0);
            v.splice(at..=at, self.items(MenuId::MeasLevels));
        }
        v
    }

    /// Refs offers upstream's 10/90 and 20/80; Custom only while the levels are custom (an edit of Low, Mid or High on
    /// the levels page makes them so).
    pub(super) fn refs_custom_offered(&self) -> bool {
        self.m.meas_lv.custom
    }

    /// The places of the measurement strip that the Dock row offers: upstream's four in the Default layout (and Hidden
    /// while it is the one chosen), every one in Minimal and Studio.
    pub(super) fn strip_docks(&self) -> Vec<StripDock> {
        StripDock::ALL
            .into_iter()
            .filter(|&d| self.m.layout != Layout::Default || d != StripDock::Hidden || self.m.strip_dock == d)
            .collect()
    }

    /// A place of the strip by the name its row shows: upstream's names in the Default layout.
    pub(super) fn strip_dock_label(&self, d: StripDock) -> &'static str {
        match (self.m.layout, d) {
            (Layout::Default, StripDock::Below) => "Under",
            (Layout::Default, StripDock::Above) => "Above",
            _ => d.label(),
        }
    }

    /// The group of the measurement settings a row belongs to (`MEASURE_GROUPS`).
    fn measure_group(it: Item) -> usize {
        match it {
            Item::MeasSrc | Item::MeasSrcB | Item::MeasCopyTo => 0,
            Item::MeasGate | Item::MeasRefs => 1,
            Item::MeasStats | Item::MeasLabels | Item::StripDock => 2,
            Item::MeasIdle | Item::MeasTsetup | Item::MeasThold | Item::MeasOutside | Item::MeasN | Item::MeasEdges => {
                3
            }
            _ => 4,
        }
    }

    /// The groups of the measurement settings: name and column.
    const MEASURE_GROUPS: [(&'static str, usize); 5] =
        [("Sources", 0), ("Measure over", 0), ("Display", 0), ("Parameters", 1), ("More", 1)];

    /// Measurement settings of the Default layout (upstream's): two columns of named groups (what is measured, over
    /// what, how the results show; the parameters of single measurements, and the pages beside them), below the top
    /// bar. The pages of the last group share a line. None on another page, and in Minimal and Studio.
    pub(super) fn measure_layout(&self) -> Option<Dlg> {
        if self.menu != MenuId::Measure || self.m.layout != Layout::Default {
            return None;
        }
        const HEAD: i32 = 24;
        let l = self.lay();
        let gap = 16;
        let cw = l.dlg_col;
        let w = 24 + 2 * cw + gap;
        let (ax, _) = self.menu_anchor();
        let x = (ax - w / 2).clamp(METRICS.edge, self.scr.w - METRICS.edge - w);
        let y = self.frame().top.bottom() + 4;
        let mut col_y = [y + DLG_HEAD - 8; 2];
        let (mut rows, mut heads) = (Vec::new(), Vec::new());
        let mut last = usize::MAX;
        // the left half of a line of two pages is taken: the next page goes right of it
        let mut half: Option<i32> = None;
        for it in self.items(MenuId::Measure) {
            let g = Self::measure_group(it);
            let (name, col) = Self::MEASURE_GROUPS[g];
            let cx = x + 12 + col as i32 * (cw + gap);
            if g != last {
                heads.push((Rect::new(cx, col_y[col], cw, HEAD), name));
                col_y[col] += HEAD;
                last = g;
                half = None;
            }
            if let Item::Open(_) = it {
                let hw = (cw - 6) / 2;
                let ctrl = match half.take() {
                    Some(ry) => Rect::new(cx + hw + 6, ry, cw - hw - 6, DLG_CTRL),
                    None => {
                        let ry = col_y[col];
                        col_y[col] += DLG_ROW;
                        half = Some(ry);
                        Rect::new(cx, ry, hw, DLG_CTRL)
                    }
                };
                rows.push(Row { it, label: Rect::new(ctrl.x, ctrl.y, 0, 0), ctrl, ctl: Ctl::Button });
                continue;
            }
            half = None;
            let ry = col_y[col];
            col_y[col] += DLG_ROW;
            let label = Rect::new(cx, ry, l.dlg_label, DLG_CTRL);
            let lw = l.dlg_label + 4;
            let ctrl = if Self::kind(it) == Kind::Action {
                Rect::new(cx, ry, cw, DLG_CTRL)
            } else {
                Rect::new(cx + lw, ry, cw - lw, DLG_CTRL)
            };
            rows.push(Row { it, label, ctl: self.ctl(it, ctrl.w), ctrl });
        }
        let h = col_y.into_iter().max().unwrap_or(y) - y + 6;
        let r = Rect::new(x, y, w, h);
        let close = Rect::new(r.x + r.w - 52, r.y + 6, 44, DLG_CTRL);
        let back = self.parent.map(|_| Rect::new(close.x - 88, close.y, 82, DLG_CTRL));
        Some(Dlg { r, close, back, rows, heads, bar: None })
    }
}
