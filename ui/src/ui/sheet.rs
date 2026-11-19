// SPDX-License-Identifier: GPL-2.0-or-later
//! Studio Settings Sheet: one menu pattern for the Studio layout. A docked inspector at the right of the screen
//! (the plot stays visible), a fixed header (Back, title, x), at most four tabs, a single column of at most seven
//! rows of 44 px, and a pinned footer with at most two actions. The sheet never moves while it is used.
//!
//! Nothing about a setting is decided here: every row is an `Item` of the existing menus and is read and changed
//! with `value`, `options`, `select_option`, `adjust` and `activate`. This file maps items to row types, splits
//! long pages into tabs and draws them. A page that does not fit the pattern (more than seven rows in a tab, more
//! than four tabs, keypad lists) is not converted: `sheet_spec` returns None and the classic dialog is used.
use super::meascatalog::{AddEntry, Preview, ADD_GROUPS};
use super::studio::StudioView;
use super::*;

/// Width of the docked sheet, and the row, header, tab strip and footer heights.
pub(super) const SHEET_W: i32 = 380;
pub(super) const SHEET_ROW: i32 = 44;
pub(super) const SHEET_MAX_ROWS: usize = 7;
pub(super) const SHEET_MAX_TABS: usize = 4;
/// The most options a choice shows as buttons; a longer one is a list.
pub(super) const SHEET_MAX_SEGS: usize = 5;
pub(super) const SHEET_HEAD: i32 = 52;
pub(super) const SHEET_TABS: i32 = 40;
pub(super) const SHEET_FOOT: i32 = 56;
const PAD: i32 = 12;
/// Width of the label column of a row.
const LABEL_W: i32 = 132;

/// The five kinds of row a sheet has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum RowKind {
    /// two to five segments
    Choice,
    /// - value +, as in the dialog of the Default layout; the value opens the list of its options
    Pick,
    /// - value +
    Number,
    /// a switch
    Toggle,
    /// opens another page or runs an action; links go last
    Link,
    /// a line of status: its name and what it says, nothing to press
    Info,
}

/// One tab: its name, its rows (at most seven) and its pinned actions (at most two, destructive last).
#[derive(Clone, Debug)]
pub(super) struct SheetTab {
    pub name: &'static str,
    pub rows: Vec<Item>,
    pub foot: Vec<Item>,
}

impl SheetTab {
    /// The tab has something to show. One that has not (Position while the cursors are off, Scale while math has
    /// no function) keeps its place in the strip, dimmed, and cannot be opened: an empty page helps nobody, and a
    /// strip that came and went would move the rows under the finger.
    pub(super) fn holds(&self) -> bool {
        !self.rows.is_empty() || !self.foot.is_empty()
    }
}

#[derive(Clone, Debug)]
pub(super) struct SheetSpec {
    pub tabs: Vec<SheetTab>,
    /// tab shown when the sheet opens
    pub first: usize,
}

/// Where a menu item goes on the sheet.
#[derive(Clone, Copy)]
enum Slot {
    Tab(usize),
    Foot(usize),
    Skip,
}

/// What the sheet remembers between frames: the tab, the page of a long list, the filter of the add flow.
#[derive(Default)]
pub(super) struct SheetState {
    pub add: super::addrail::AddUi,
    menu: Option<MenuId>,
    tab: usize,
    page_key: Option<(MenuId, Option<Item>)>,
    page: usize,
    pub add_group: usize,
    pub search: String,
    /// how far the Menu page is moved up
    pub scroll: i32,
}

/// Geometry of an items page.
pub(super) struct SheetRowGeo {
    pub it: Item,
    pub kind: RowKind,
    pub row: Rect,
    pub label: Rect,
    pub ctrl: Rect,
}

pub(super) struct SheetGeo {
    pub r: Rect,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub back: Option<Rect>,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub close: Rect,
    pub tabs: Vec<Rect>,
    pub rows: Vec<SheetRowGeo>,
    pub foot: Vec<(Item, Rect)>,
}

/// The cells of an add-panel tile and what each one opens.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum PanelKind {
    Measurements,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    Decode,
    Spectrum,
    Eye,
    Histogram,
    Trend,
    Cursor,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    Sim,
}


/// Items whose list is a keypad or a file list with its own handling: the classic picker keeps them.
fn keypad_pick(it: Item) -> bool {
    matches!(it, Item::TrigBusVal | Item::AfgFreqEntry | Item::WlanNet | Item::WlanForgetNet | Item::NtpServer | Item::TimeZone | Item::RemName | Item::MeasCopyTo)
}

fn destructive(it: Item) -> bool {
    matches!(it, Item::SpecLimitClear | Item::EyeClear | Item::MaskClear | Item::SimDefaults | Item::CurRemove | Item::SrchRemove | Item::RefClear | Item::RefClearOne | Item::MeasClear | Item::UpdateRollback | Item::Stock | Item::PowerAct | Item::DefaultAct | Item::ZoneClear)
}

impl Ui {
    // ------------------------------------------------------------------------------------------------ the plan

    /// Which tab (or footer) each item of a menu goes to. `names` are the tab names, in order.
    fn sheet_plan(&self, menu: MenuId) -> Option<(&'static [&'static str], fn(Item) -> Slot)> {
        use Item::*;
        use MenuId as M;
        Some(match menu {
            M::Trigger => (&["Type and source", "Settings"], |it| match it {
                TrigType | TrigSrc | TrigSrc2 | TrigSlope | TrigSlope2 | TrigPol | DecProto | TrigBusWhat | TrigBusVal | Pat(_) | WinModeI
                    | VidStd | VidSync | VidLine | TrigLevel | TrigLevel2 | Trig50 | PatLevel | Open(M::LaPat) => Slot::Tab(0),
                SearchTable | SearchSave => Slot::Foot(1),
                Open(M::Zone) => Slot::Skip,
                _ => Slot::Tab(1),
            }),
            M::Channel(_) | M::ChMore(_) => (&["Vertical", "More"], |it| match it {
                ChOn(_) | ChScale(_) | ChOffset(_) | ChCoupling(_) | ChProbe(_) | ChBw(_) | ChInvert(_) => Slot::Tab(0),
                Open(M::ChMore(_)) => Slot::Skip,
                ChBiasZero(_) | AutoDeskew => Slot::Foot(1),
                SrcOn => Slot::Foot(0),
                _ => Slot::Tab(1),
            }),
            M::Measure => (&["Settings", "Sources", "Parameters", "Levels"], |it| match it {
                MeasGate | BadgeStyle | Open(M::MeasStat) | MeasLabels | Open(M::MeasPair) | Open(M::Dvm) => Slot::Tab(0),
                MeasSrc | MeasSrcB | MeasCopyTo => Slot::Tab(1),
                MeasLvType | MeasRefs | MeasLow | MeasMid | MeasHigh | MeasHyst | MeasBaseTop => Slot::Tab(3),
                Open(M::MeasFall) => Slot::Foot(3),
                // the way to a new measurement beside the way to none
                Open(M::MeasAdd) | MeasClear => Slot::Foot(0),
                StripDock => Slot::Skip,
                _ => Slot::Tab(2),
            }),
            M::MeasFall => (&["Fall levels"], |_| Slot::Tab(0)),
            M::MeasStat => (&["Statistics"], |it| match it { MeasStatReset => Slot::Foot(0), _ => Slot::Tab(0) }),
            M::Display => (&["Waveform", "Grid and views", "Screen"], |it| match it {
                Persist | ColorGrade | Intensity | Dots | Histogram | XyMode | ZInput | TrendSel(_) => Slot::Tab(0),
                Open(M::Windows) | Views | GridStyle | GridBright | GridFrame | Layout | Traces => Slot::Tab(1),
                // Clear wipes what persistence has drawn: under the tab that holds Persistence
                ClearDisp => Slot::Foot(0),
                _ => Slot::Tab(2),
            }),
            // the first tab is the top of the Default dialog in its order: mode, averaging, memory, segments, aliasing.
            // The rarer rows have the second tab: with averaging or segments on, one tab held more rows than a sheet
            // has, and the page fell back to the dialog. The way on to Horizontal is under both tabs
            M::Acquire => (&["Acquire", "More"], |it| match it {
                AcqSegIvl | AcqSegTable | AcqCores | AcqDrawPath => Slot::Tab(1),
                Open(M::Horizontal) => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            M::Horizontal => (&["Time base", "Zoom"], |it| match it {
                Zoom | ZScale | ZDelay | ZoomBar => Slot::Tab(1),
                AutoDeskew | ClearDeskew => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            M::Cursor => (&["Cursors", "Position"], |it| match it { CurMode | CurSrc | CurSrcB | CurSecond | CurReadout | CurLink => Slot::Tab(0), CurRemove => Slot::Foot(0), _ => Slot::Tab(1) }),
            M::Search => (&["Edge search"], |it| match it { SrchRemove => Slot::Foot(0), _ => Slot::Tab(0) }),
            M::Ref => (&["Reference", "Scale and files"], |it| match it {
                RefSel | RefSrc | RefLabel | RefColor | RefSaveNext | RefImport => Slot::Tab(0),
                RefShow(_) => Slot::Skip,
                // Store, and Clear for the slot that is shown; Clear all is with the files
                RefSave | RefClearOne => Slot::Foot(0),
                RefClear => Slot::Foot(1),
                _ => Slot::Tab(1),
            }),
            M::Decode | M::Bus => (&["Decode", "Settings", "Bus"], |it| match it {
                // (the B line of a differential pair is a setting of its own: on the first page it stood under the
                // source of every UART, where most have none)
                DecSel | DecOn | DecProto | DecIn | DecSrc | DecSrc2 | DecCs | DecSrc4 => Slot::Tab(0),
                DecTable | DecExport => Slot::Foot(1),
                // an action: under the sources, whose signal it reads (the foot of Settings has its two)
                DecThrMid => Slot::Foot(0),
                BusLabel => Slot::Foot(2),
                BusOn | BusPos | BusClk | BusClkThr | BusData | BusDataThr | BusRw => Slot::Tab(2),
                _ => Slot::Tab(1),
            }),
            M::Math => (&["Function", "Scale"], |it| match it {
                MathSel | MathFam | MathOpI | MathA | MathB | MathExprEdit | FftWin => Slot::Tab(0),
                // Turn off is pinned under the rows: as their first one it moved every row when a function was chosen
                MathOff => Slot::Foot(0),
                MathExprI => Slot::Skip,
                MathAuto => Slot::Foot(1),
                _ => Slot::Tab(1),
            }),
            // Connectivity says where the scope is reached and what time it has, above the pages that set it
            M::Utility => (&["Connectivity", "Device", "Service", "Advanced"], |it| match it {
                NetEth | NetWlan | NetWeb | TimeNow | Open(M::Network) | Open(M::Remote) | Open(M::Time) => Slot::Tab(0),
                Offline | QuickAct | Graphics | Shot => Slot::Tab(1),
                Open(M::SelfCal) | SysInfo | About | Update => Slot::Tab(2),
                UpdateRollback => Slot::Foot(2),
                Stock => Slot::Foot(3),
                _ => Slot::Tab(3),
            }),
            M::Spectrum => (&["Spectrum", "Traces", "Limits"], |it| match it {
                SpecOn | SpecSrc | SpecCenter | SpecSpan | SpecRbw | SpecWin | SpecHarm => Slot::Tab(0),
                SrcOn => Slot::Foot(0),
                SpecLimitSet | SpecLimitClear => Slot::Foot(2),
                SpecAcprW | SpecAcprS | SpecMargin => Slot::Tab(2),
                _ => Slot::Tab(1),
            }),
            M::Eye => (&["Eye", "Jitter", "Filters"], |it| match it {
                EyeOn | EyePlace | EyeSrc | EyeRate | EyeBer | EyeB => Slot::Tab(0),
                EyeAct | SrcOn => Slot::Foot(0),
                EyeHp | EyePnLo | EyePnHi | EyeNpj => Slot::Tab(1),
                EyeClear => Slot::Foot(2),
                _ => Slot::Tab(2),
            }),
            M::Power => (&["Power"], |it| match it {
                SrcOn => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            M::Storage => (&["Save", "Export", "SCPI"], |it| match it {
                // a session is saved and recalled like a setup, with the samples: it sits beside the setup rows
                StoreDest | StoreSlot | StoreSaveSetup | StoreRecall | StoreSessSave | StoreSessLoad => Slot::Tab(0),
                // the set-up as commands, the recorder and the file that is run: with them Export had more rows
                // than a tab holds
                StoreSaveScpi | StoreRecScpi | StoreRunScpi => Slot::Tab(2),
                _ => Slot::Tab(1),
            }),
            M::Mask => (&["Test", "Build"], |it| match it {
                MaskOn | MaskSrc | MaskInvert | MaskAction => Slot::Tab(0),
                SrcOn | MaskReset => Slot::Foot(0),
                MaskClear => Slot::Foot(1),
                _ => Slot::Tab(1),
            }),
            // what is played and where it goes, then the knobs: levels and channel, jitter, spread spectrum. The rows
            // of the Advanced page are rows of these tabs; the actions are pinned by `sheet_spec`
            M::Sim => (&["Signal", "Levels", "Jitter", "Spread"], |it| match it {
                SimPreset | SimRate | SimPat | SimSlot | SimShow | SimLoad | SimPlayRefs | SimLive => Slot::Tab(0),
                SimHigh | SimLow | SimRise | SimBw | SimDeemph | SimNoise => Slot::Tab(1),
                SimRj | SimPj | SimPjHz | SimDcd | SimCorrRj | SimCorrHz => Slot::Tab(2),
                SimSsc | SimSscCenter | SimSscDeviation | SimSscHz => Slot::Tab(3),
                _ => Slot::Skip,
            }),
            M::SimAdvanced => (&["Spread spectrum", "Other jitter"], |it| match it {
                SimSsc | SimSscCenter | SimSscDeviation | SimSscHz => Slot::Tab(0),
                SimDefaults => Slot::Foot(1),
                _ => Slot::Tab(1),
            }),
            M::Afg => (&["Signal", "Modulation"], |it| match it {
                AfgMod | AfgModRate | AfgModDepth | AfgModShape => Slot::Tab(1),
                AfgArbGrab | AfgArbFile => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            // where the generator is and what it says it is, then the sine the page switches on by itself
            M::ExtGen => (&["Connection", "Output"], |it| match it {
                ExtCh | ExtLoad | ExtFreq | ExtAmpl | ExtOut => Slot::Tab(1),
                ExtConnect => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            M::La => (&["Logic", "Thresholds"], |it| match it {
                LaThr(_) | LaUser(_) => Slot::Tab(1),
                _ => Slot::Tab(0),
            }),
            M::LaLines => (&["D0-3", "D4-7", "D8-11", "D12-15"], |it| match it {
                LaLine(i) => Slot::Tab(i / 4),
                _ => Slot::Tab(0),
            }),
            M::LaPat => (&["D0-3", "D4-7", "D8-11", "D12-15"], |it| match it {
                LaPat(i) => Slot::Tab(i / 4),
                _ => Slot::Tab(0),
            }),
            M::Logger => (&["Data logger"], |_| Slot::Tab(0)),
            M::Dvm => (&["DVM", "Counter"], |it| match it {
                DvmOn | DvmSrc | DvmMode => Slot::Tab(0),
                CntClear => Slot::Foot(1),
                _ => Slot::Tab(1),
            }),
            M::Dpt => (&["Signals", "View"], |it| match it {
                DptPct | DptZoom => Slot::Tab(1),
                SrcOn => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            M::Inverter => (&["Phases", "Signal"], |it| match it {
                InvOn | InvA | InvB | InvC => Slot::Tab(0),
                SrcOn => Slot::Foot(0),
                _ => Slot::Tab(1),
            }),
            M::Magnetic => (&["Signals", "Core"], |it| match it {
                MagN | MagAe | MagLe | MagRcu => Slot::Tab(1),
                SrcOn => Slot::Foot(0),
                _ => Slot::Tab(0),
            }),
            M::BwTest => (&["Bandwidth test"], |_| Slot::Tab(0)),
            M::Bode => (&["Sweep", "Connections"], |it| match it {
                BodeIn | BodeOut | BodeGen | Open(M::ExtGen) => Slot::Tab(1),
                BodeRun => Slot::Foot(0),
                SrcOn => Slot::Foot(1),
                _ => Slot::Tab(0),
            }),
            M::Zone => (&["Zone"], |it| match it { ZoneClear => Slot::Foot(0), _ => Slot::Tab(0) }),
            // the two sources are pinned under every tab: "A: CH1" to "B: CH2"
            M::MeasPair => (&["Delay", "Phase", "Skew", "Setup / hold"], |it| match it {
                MeasSrc => Slot::Foot(0),
                MeasSrcB => Slot::Foot(1),
                Item::MeasPair(i) => { let i = i as usize; Slot::Tab(if i < 12 { i / 4 } else { 3 }) }
                _ => Slot::Skip,
            }),
            M::MeasLevels => (&["Levels"], |it| match it { Open(M::MeasFall) => Slot::Foot(0), _ => Slot::Tab(0) }),
            M::SelfCal => (&["Self-calibration"], |it| match it { CalStart => Slot::Foot(0), _ => Slot::Tab(0) }),
            M::Time => (&["Date and time"], |it| match it { NtpSync | TimeSet => Slot::Foot(0), _ => Slot::Tab(0) }),
            M::Network => (&["Ethernet", "WLAN"], |it| match it {
                NetWlan | WlanScan | WlanForget | WlanInfo => Slot::Tab(1),
                _ => Slot::Tab(0),
            }),
            M::Windows => (&["Windows"], |_| Slot::Tab(0)),
            // the switches of the services, then who may reach them and where they are
            M::Remote => (&["Services", "Access"], |it| match it {
                RemWeb | RemScpi | RemVxi | RemTelnet | RemSsh | RemMdns | Open(M::Ssh) => Slot::Tab(0),
                _ => Slot::Tab(1),
            }),
            M::Ssh => (&["SSH"], |it| match it { SshImport | SshRemove => Slot::Foot(0), _ => Slot::Tab(0) }),
            _ => return None,
        })
    }

    /// The menus whose lines of status stay on the sheet as Info rows.
    fn sheet_keeps_text(menu: MenuId) -> bool {
        matches!(menu, MenuId::Acquire | MenuId::Logger | MenuId::SelfCal | MenuId::Time | MenuId::Network | MenuId::Remote | MenuId::Ssh | MenuId::Utility | MenuId::ExtGen)
    }

    /// Rows a tab of `menu` may have: seven, and eight for the setup and hold pairs, which are one group.
    fn sheet_max_rows(menu: MenuId) -> usize {
        if menu == MenuId::MeasPair { SHEET_MAX_ROWS + 1 } else { SHEET_MAX_ROWS }
    }

    /// The items a sheet of `menu` draws from: the menus it merges.
    fn sheet_source_items(&self, menu: MenuId) -> Vec<Item> {
        match menu {
            MenuId::Channel(c) | MenuId::ChMore(c) => self.items(MenuId::Channel(c)).into_iter().chain(self.items(MenuId::ChMore(c))).collect(),
            MenuId::Decode | MenuId::Bus => self.items(MenuId::Decode).into_iter().chain(self.items(MenuId::Bus)).collect(),
            MenuId::MeasPair => [Item::MeasSrc, Item::MeasSrcB].into_iter().chain(self.items(MenuId::MeasPair)).collect(),
            // Measure: its footer leads to the list of measurements to add
            MenuId::Measure => [Item::Open(MenuId::MeasAdd)].into_iter().chain(self.items(MenuId::Measure)).collect(),
            // Reference: Clear for the one slot, which the menu has not
            MenuId::Ref => self.items(MenuId::Ref).into_iter().chain([Item::RefClearOne]).collect(),
            // SIM: the knobs of its Advanced page are rows of the sheet's own tabs, so the link to that page is left out
            MenuId::Sim => {
                let mut v: Vec<Item> = self.items(MenuId::Sim).into_iter().filter(|it| *it != Item::Open(MenuId::SimAdvanced)).collect();
                for it in self.items(MenuId::SimAdvanced) { if !v.contains(&it) { v.push(it); } }
                v
            }
            // System: the state lines of the network and the clock, and the page of the remote services, which the
            // menu has one level down
            MenuId::Utility => [Item::NetEth, Item::NetWlan, Item::NetWeb, Item::TimeNow].into_iter()
                .chain(self.items(MenuId::Utility)).chain([Item::Open(MenuId::Remote)]).collect(),
            // Display: the skin and the choice of the bottom bar are rows of the Default layout's menu. A sheet is up
            // only in Minimal and Studio, whose bars are the compact ones whatever Bottom bar says
            MenuId::Display => {
                // and Overlay or Stacked, which Studio had in the hold menu of the plot alone
                self.items(MenuId::Display).into_iter().filter(|it| !matches!(it, Item::Skin | Item::BottomBar)).chain([Item::Traces]).collect()
            }
            m => self.items(m),
        }
    }

    /// The start menu of the Studio sheet: the grouped Menu of every layout, as one page (drawn by menugrid.rs).
    fn sheet_main_spec(&self) -> SheetSpec {
        let rows: Vec<Item> = self.items(MenuId::Main);
        SheetSpec { tabs: vec![SheetTab { name: "Menu", rows, foot: Vec::new() }], first: 0 }
    }

    /// The sheet of a menu, or None when the menu has none yet or does not fit the pattern.
    pub(super) fn sheet_spec(&self, menu: MenuId) -> Option<SheetSpec> {
        if !self.sheets_on() {
            return None;
        }
        // the Menu page and the measurement row page scroll and drag with Studio's touch handling: Studio only
        if matches!(menu, MenuId::Main | MenuId::MeasRow) && self.m.layout != Layout::Studio {
            return None;
        }
        let spec = if menu == MenuId::Main {
            // the Menu is one scrolling page of groups (menugrid.rs): no tabs, no row limit
            return Some(self.sheet_main_spec());
        } else if menu == MenuId::MeasRow {
            let tabs = self.measrow_sheet_tabs();
            if tabs.is_empty() { return None; }
            SheetSpec { tabs, first: 0 }
        } else {
            let (names, place) = self.sheet_plan(menu)?;
            let mut tabs: Vec<SheetTab> = names.iter().map(|n| SheetTab { name: n, rows: Vec::new(), foot: Vec::new() }).collect();
            let mut items = self.sheet_source_items(menu);
            // the add-source button belongs to the page it fixes
            if !Self::sheet_keeps_text(menu) { items.retain(|it| !matches!(self.ctl(*it, 200), Ctl::Text)); }
            for it in items {
                match place(it) {
                    Slot::Tab(i) => tabs[i].rows.push(it),
                    Slot::Foot(i) => tabs[i].foot.push(it),
                    Slot::Skip => {}
                }
            }
            // Minimal keeps the stock strip and cursors: where the strip docks stays on the Measure page, and the
            // three cursor rows are one tab
            if self.m.layout != Layout::Studio {
                if menu == MenuId::Measure && self.items(menu).contains(&Item::StripDock) { tabs[0].rows.push(Item::StripDock); }
                if menu == MenuId::Cursor && tabs.len() > 1 {
                    let rest: Vec<Item> = tabs.drain(1..).flat_map(|t| t.rows).collect();
                    tabs[0].rows.extend(rest);
                }
            }
            // SIM: the slot's rows follow the slot, and Live comes last, so no row moves when SIM mode starts under
            // the finger. Run is under every tab: beside Add on the tab of the slot, beside Defaults on the knob tabs
            if menu == MenuId::Sim {
                // (Play references is offered while live, Back to live while a record plays: one or the other, in the same place)
                let order = [Item::SimPreset, Item::SimRate, Item::SimPat, Item::SimSlot, Item::SimShow, Item::SimLoad, Item::SimPlayRefs, Item::SimLive];
                tabs[0].rows.sort_by_key(|it| order.iter().position(|o| o == it).unwrap_or(order.len()));
                for (i, t) in tabs.iter_mut().enumerate() {
                    t.foot = if i == 0 { vec![Item::SimAddRef, Item::SimPlay] } else { vec![Item::SimPlay, Item::SimDefaults] };
                }
            }
            // System > Connectivity: the pages in the order of the lines above them
            if menu == MenuId::Utility {
                let order = [Item::NetEth, Item::NetWlan, Item::NetWeb, Item::TimeNow, Item::Open(MenuId::Network), Item::Open(MenuId::Remote), Item::Open(MenuId::Time)];
                tabs[0].rows.sort_by_key(|it| order.iter().position(|o| o == it).unwrap_or(order.len()));
            }
            // Remote services: the name and who is allowed first, then the addresses
            if menu == MenuId::Remote {
                let order = [Item::RemName, Item::RemAllow, Item::RemFirewall, Item::NetWeb, Item::RemScpiInfo, Item::RemVxiInfo, Item::RemTelnetInfo];
                tabs[1].rows.sort_by_key(|it| order.iter().position(|o| o == it).unwrap_or(order.len()));
            }
            // DVM and counter, the logic analyser and the double pulse had two tabs of two to five rows each: they are
            // one page while all of it fits one (seven rows, two actions in the footer)
            if matches!(menu, MenuId::Dvm | MenuId::La | MenuId::Dpt) && tabs.len() > 1 {
                let (rows, feet) = (tabs.iter().map(|t| t.rows.len()).sum::<usize>(), tabs.iter().map(|t| t.foot.len()).sum::<usize>());
                if rows <= SHEET_MAX_ROWS && feet <= 2 {
                    let rest: Vec<SheetTab> = tabs.drain(1..).collect();
                    for t in rest {
                        tabs[0].rows.extend(t.rows);
                        tabs[0].foot.extend(t.foot);
                    }
                }
            }
            // Decode: what the protocol itself needs (baud rate, bits, parity, the SPI mode, the CAN rate ...) follows
            // its lines on the first page while that page has the room; the rest stays on Settings. They were all on
            // the second page: a UART was switched on and read nothing until the baud rate was found there
            if matches!(menu, MenuId::Decode | MenuId::Bus) && tabs.len() > 1 {
                use Item::*;
                while tabs[0].rows.len() < SHEET_MAX_ROWS {
                    let Some(i) = tabs[1].rows.iter().position(|it| matches!(it, DecBaud | DecBits | DecParity | DecMode | DecCanRate | DecCanH | DecUsbSpeed | DecCphyRate | DecFmt)) else { break };
                    let row = tabs[1].rows.remove(i);
                    tabs[0].rows.push(row);
                }
            }
            // Trigger: Sweep (Auto, Normal, Single) goes with the type and the source while that tab has the room. A
            // type with many rows of its own keeps it on Settings: an eighth row would take the sheet away
            if menu == MenuId::Trigger && tabs.len() > 1 && tabs[0].rows.len() < SHEET_MAX_ROWS {
                if let Some(i) = tabs[1].rows.iter().position(|it| *it == Item::TrigMode) {
                    let sweep = tabs[1].rows.remove(i);
                    tabs[0].rows.push(sweep);
                }
            }
            // a link to another page comes after the rows of this one. The lines of the logic analyser keep their
            // link under the row it continues
            if menu != MenuId::La {
                for t in &mut tabs { t.rows.sort_by_key(|it| matches!(it, Item::Open(_))); }
            }
            // the next-free action is offered only when the selected slot is taken
            if matches!(menu, MenuId::Ref) && self.refs[self.ref_sel].is_none() {
                for t in &mut tabs { t.rows.retain(|it| *it != Item::RefSaveNext); }
            }
            // Ref: the ten slots are one Pick, and the shown switch is the one of the selected slot
            if matches!(menu, MenuId::Ref) {
                let sel = self.ref_sel;
                let at = 2.min(tabs[0].rows.len());
                tabs[0].rows.insert(at, Item::RefShow(sel));
            }
            // Acquire: the way on to Horizontal is under both tabs
            if menu == MenuId::Acquire {
                for t in &mut tabs { t.foot = vec![Item::Open(MenuId::Horizontal)]; }
            }
            // the two sources of a pair are under every tab ("A: CH1", "B: CH2"): a slot of the plan is the foot of one tab
            if menu == MenuId::MeasPair {
                for t in &mut tabs { t.foot = vec![Item::MeasSrc, Item::MeasSrcB]; }
            }
            let first = match menu {
                MenuId::ChMore(_) => 1,
                MenuId::Bus => 2,
                _ => 0,
            };
            SheetSpec { tabs, first }
        };
        if spec.tabs.len() > SHEET_MAX_TABS {
            return None;
        }
        // a third action does not fit the footer, an eighth row does not fit the body
        if spec.tabs.iter().any(|t| t.foot.len() > 2 || t.rows.len() > Self::sheet_max_rows(menu)) {
            return None;
        }
        Some(SheetSpec {
            tabs: spec.tabs.into_iter().map(|mut t| { t.foot.sort_by_key(|it| destructive(*it)); t }).collect(),
            first: spec.first,
        })
    }

    // ------------------------------------------------------------------------------------------------ state

    /// The sheet is the menu on screen: Studio, a menu open, a plan for it.
    /// The layouts whose menus are sheets: Minimal and Studio. The stock layout keeps the stock dialogs.
    pub(super) fn sheets_on(&self) -> bool {
        self.m.compact()
    }

    pub(super) fn sheet_active(&self) -> bool {
        if !self.sheets_on() || !self.menu_open || self.full_on() {
            return false;
        }
        match self.menu {
            // Studio's quick Add panel; the Measure page of every layout once it is expanded (`studio.meas_page`)
            MenuId::MeasAdd => self.m.layout == Layout::Studio && !self.studio.meas_page,
            MenuId::AddPanel => self.m.layout == Layout::Studio,
            MenuId::Pick => self.pick.is_some_and(|p| !keypad_pick(p.it) && !self.options(p.it).is_empty() && self.sheet_spec(p.menu).is_some()),
            m => self.sheet_spec(m).is_some(),
        }
    }

    pub(super) fn sheet_dock_w(&self) -> i32 {
        if self.sheet_active() && !self.menu_is_wide() { SHEET_W + 4 } else { 0 }
    }

    /// The rectangle of the menu on screen, sheet or classic dialog.
    pub(super) fn menu_rect(&self) -> Option<Rect> {
        if self.sheet_active() { Some(self.sheet_rect()) } else { self.dlg_layout().map(|d| d.r) }
    }

    /// The sheet's place: the right edge of the body, from the top bar to the bottom bar. It is the same for every page.
    pub(super) fn sheet_rect(&self) -> Rect {
        if self.menu_is_wide() { return self.menu_wide_rect(); }
        let f = self.frame();
        let y = f.top.bottom();
        Rect::new(self.scr.w - SHEET_W, y, SHEET_W, (f.bottom.y - y).max(SHEET_HEAD + SHEET_FOOT))
    }

    fn sheet_tab(&self, spec: &SheetSpec) -> usize {
        let n = spec.tabs.len().max(1);
        let want = if self.sheet.menu == Some(self.menu) { self.sheet.tab.min(n - 1) } else { spec.first.min(n - 1) };
        // the tab that was open may hold nothing by now: the first that does is shown
        if spec.tabs.get(want).is_some_and(SheetTab::holds) { want } else { spec.tabs.iter().position(SheetTab::holds).unwrap_or(want) }
    }

    fn sheet_page(&self, key: Option<Item>) -> usize {
        if self.sheet.page_key == Some((self.menu, key)) { self.sheet.page } else { 0 }
    }

    fn sheet_set_tab(&mut self, i: usize) {
        self.sheet.menu = Some(self.menu);
        self.sheet.tab = i;
    }

    fn sheet_set_page(&mut self, key: Option<Item>, page: usize) {
        self.sheet.page_key = Some((self.menu, key));
        self.sheet.page = page;
    }

    // ------------------------------------------------------------------------------------------------ rows

    /// Width of the label column of a row: the fixed one, or only what the label needs when that makes the room for the
    /// buttons of its choice. The dialog of the Default layout shows such a choice as buttons (the acquisition mode,
    /// the cursor mode); in the fixed column it became a list, one tap more for every change.
    pub(super) fn sheet_label_w(&self, it: Item) -> i32 {
        let row = SHEET_W - 2 * PAD;
        if it == Item::TrigType || it == Item::RefColor || Self::kind(it) != Kind::Choice || !matches!(self.ctl(it, row - LABEL_W), Ctl::Pick) {
            return LABEL_W;
        }
        let need = self.fonts().m.width(&self.sheet_label(it)) + 14;
        match self.ctl(it, row - need) {
            Ctl::Seg(o, _) if need < LABEL_W && o.len() <= SHEET_MAX_SEGS => need,
            _ => LABEL_W,
        }
    }

    /// The row type of an item, from the control the classic dialog would give it.
    pub(super) fn sheet_row_kind(&self, it: Item) -> RowKind {
        let w = SHEET_W - 2 * PAD - self.sheet_label_w(it);
        if it == Item::TrigType || it == Item::RefColor {
            return RowKind::Choice;
        }
        if let Item::RefShow(_) = it {
            return RowKind::Toggle;
        }
        match self.ctl(it, w) {
            Ctl::Seg(o, _) if o.len() == 2 && o[0] == "Off" && o[1] == "On" => RowKind::Toggle,
            Ctl::Seg(o, _) if o.len() <= SHEET_MAX_SEGS => RowKind::Choice,
            Ctl::Seg(..) | Ctl::Pick => RowKind::Pick,
            Ctl::Value => RowKind::Number,
            Ctl::Button => RowKind::Link,
            Ctl::Text => RowKind::Info,
        }
    }

    /// A list row without its steps: the value alone, which opens the list. For the protocol of a decoder: the
    /// steps went through some forty names in an order that says nothing.
    pub(super) fn sheet_pick_plain(it: Item) -> bool {
        it == Item::DecProto
    }

    /// Options and selection of a Choice row. The trigger type shows its common types inline and the rest behind More.
    pub(super) fn sheet_choice(&self, it: Item) -> (Vec<String>, Option<usize>) {
        if it == Item::TrigType {
            let common = ["Edge", "Pulse", "Slope"];
            let cur = self.value(Item::TrigType);
            let mut v: Vec<String> = common.iter().map(|s| s.to_string()).collect();
            return match common.iter().position(|c| *c == cur) {
                Some(i) => { v.push("More".into()); (v, Some(i)) }
                None => { v.push(cur); (v, Some(3)) }
            };
        }
        if it == Item::RefColor {
            // the five slot colours as one choice (drawn as swatches)
            let sel = self.refs.get(self.ref_sel).and_then(Option::as_ref).map(|r| r.color.min(REF_COLORS.len() - 1));
            return (REF_COLORS.iter().map(|c| c.0.to_string()).collect(), sel);
        }
        match self.ctl(it, SHEET_W - 2 * PAD - self.sheet_label_w(it)) {
            Ctl::Seg(o, s) => (o, s),
            _ => (Vec::new(), None),
        }
    }

    /// Whether the switch of a Toggle row is on.
    pub(super) fn sheet_on(&self, it: Item) -> bool {
        match it {
            Item::RefShow(i) => self.refs.get(i).and_then(|r| r.as_ref()).is_some_and(|r| r.on),
            // its value names the slot ("R1 on", "empty"), so the switch reads the slot itself
            Item::SimShow => self.sim_shown() == Some(true),
            _ => self.value(it) == "On",
        }
    }

    /// What a row says: its label.
    /// The label of the Store button: the slot the picker shows, and "Replace" when it is taken.
    pub(super) fn ref_store_label(&self) -> String {
        let (src, sel) = (self.wave_name(WaveId::Ch(self.ref_src)), self.ref_sel);
        if self.refs[sel].is_some() { format!("Replace R{} with {}", sel + 1, src) } else { format!("Store {} as R{}", src, sel + 1) }
    }

    pub(super) fn ref_store_next_label(&self) -> String {
        let i = self.refs.iter().position(|r| r.is_none()).unwrap_or(NREF - 1);
        format!("Store as R{} (next free)", i + 1)
    }

    /// A text in two parts at the space nearest its middle (the second is empty without a space).
    pub(super) fn two_lines(v: &str) -> (&str, &str) {
        let mid = v.len() / 2;
        match v.char_indices().filter(|&(_, ch)| ch == ' ').min_by_key(|&(i, _)| i.abs_diff(mid)) {
            Some((i, _)) => (&v[..i], &v[i + 1..]),
            None => (v, ""),
        }
    }

    pub(super) fn sheet_label(&self, it: Item) -> String {
        match it {
            Item::SrcOn => {
                let names: Vec<String> = self.off_sources(self.menu).iter().map(|c| format!("CH{}", c + 1)).collect();
                if names.is_empty() { "Switch on source".into() } else { format!("Turn on {}", names.join(", ")) }
            }
            Item::SimAddRef => format!("Add to R{}", self.m.sim_slot + 1),
            Item::SimLoad => format!("Load file into R{}…", self.m.sim_slot + 1),
            // on the Access tab of Remote services the addresses stand apart from their switches: they say whose they are
            Item::NetWeb if matches!(self.menu_root(), MenuId::Remote | MenuId::Utility) => "Web address".into(),
            Item::TimeNow if self.menu_root() == MenuId::Utility => "Clock".into(),
            Item::RemScpiInfo => "SCPI port".into(),
            Item::RemVxiInfo => "VXI-11 ports".into(),
            Item::RemTelnetInfo => "telnet port".into(),
            Item::SimShow => format!("Show R{}", self.m.sim_slot + 1),
            Item::SimLive => "Back to live".into(),
            Item::SimPlayRefs => {
                let shown = self.sim_shown_refs();
                let names: Vec<String> = shown.iter().map(|i| format!("R{}", i + 1)).collect();
                format!("Play {} {}", if shown.len() == 1 { "reference" } else { "references" }, names.join(", "))
            }
            // the tab is named Spread: its rows do not repeat it
            Item::SimSsc => "Spread spectrum".into(),
            Item::SimSscCenter => "Profile".into(),
            Item::SimSscDeviation => "Deviation".into(),
            Item::SimSscHz => "Frequency".into(),
            Item::RefImport => format!("Load file into R{}…", self.ref_sel + 1),
            Item::RefShow(_) => "Shown".into(),
            Item::Open(MenuId::MeasAdd) => "Add measurement".into(),
            Item::RefClearOne => format!("Clear R{}", self.ref_sel + 1),
            // the rows below are those of one decoder of four: the switch says whose it is
            Item::DecSel => "Decoders".into(),
            Item::DecOn => format!("Decoder D{}", self.m.dec_sel + 1),
            // a line is named by what it is in the protocol (Clock, Data, CS), where the protocol says so
            Item::DecSrc | Item::DecSrc2 | Item::DecCs | Item::DecSrc4 => {
                let role = match it { Item::DecSrc => 0, Item::DecSrc2 => 1, Item::DecCs => 2, _ => 3 };
                self.m.dec_proto.roles()[role].map_or_else(|| self.item_label(it).to_string(), str::to_string)
            }
            // a line's own threshold is named after the line; the decoder's is the main line's where others have one
            Item::DecThrLine(role) => match self.m.dec_proto.roles().get(role).copied().flatten() {
                Some(name) => format!("{} threshold", name),
                None => self.item_label(it).to_string(),
            },
            Item::DecThr if self.m.dec_proto != Proto::CPHY && (1..4).any(|r| self.m.dec_role_src(r).is_some()) => {
                self.m.dec_proto.roles()[0].map_or_else(|| "Threshold".to_string(), |name| format!("{} threshold", name))
            }
            Item::RefSaveNext => self.ref_store_next_label(),
            Item::RowF(measrow::RF::Used | measrow::RF::StMean | measrow::RF::StRange | measrow::RF::StN) => self.measrow_text(it).unwrap_or_default(),
            _ => self.item_label(it).to_string(),
        }
    }

    /// Chrome shared by every page: Back (always at the left), the close button, the header rectangle.
    pub(super) fn sheet_chrome(&self) -> (Rect, Option<Rect>, Rect) {
        let r = self.sheet_rect();
        let back = self.sheet_has_back().then(|| Rect::new(r.x + 8, r.y + 4, 76, 44));
        (r, back, Rect::new(r.right() - 52, r.y + 4, 44, 44))
    }

    fn sheet_has_back(&self) -> bool {
        (self.menu == MenuId::AddPanel && self.sheet.add.kind.is_some()) || self.parent.is_some() || self.menu == MenuId::Pick || !self.menu_history.is_empty()
    }

    /// Geometry of an items page.
    pub(super) fn sheet_geo(&self) -> Option<SheetGeo> {
        let spec = self.sheet_spec(self.menu)?;
        let (r, back, close) = self.sheet_chrome();
        let multi = spec.tabs.len() > 1;
        let tab = self.sheet_tab(&spec);
        let tw = (r.w - 2 * PAD) / spec.tabs.len().max(1) as i32;
        let tabs = if multi {
            (0..spec.tabs.len()).map(|i| Rect::new(r.x + PAD + i as i32 * tw, r.y + SHEET_HEAD, tw, SHEET_TABS)).collect()
        } else {
            Vec::new()
        };
        let body_y = r.y + SHEET_HEAD + if multi { SHEET_TABS } else { 0 };
        let t = &spec.tabs[tab];
        let mut rows = Vec::new();
        for (i, &it) in t.rows.iter().enumerate() {
            let row = Rect::new(r.x + PAD, body_y + i as i32 * SHEET_ROW, r.w - 2 * PAD, SHEET_ROW);
            let kind = self.sheet_row_kind(it);
            let lw = self.sheet_label_w(it);
            let label = Rect::new(row.x, row.y, lw, row.h);
            let ctrl = match kind {
                RowKind::Link => Rect::new(row.x, row.y + 2, row.w, 40),
                RowKind::Toggle => Rect::new(row.right() - 56, row.y + 9, 52, 26),
                _ => Rect::new(row.x + lw, row.y + 2, row.w - lw, 40),
            };
            rows.push(SheetRowGeo { it, kind, row, label, ctrl });
        }
        let foot = self.sheet_foot_rects(r, &t.foot);
        Some(SheetGeo { r, back, close, tabs, rows, foot })
    }

    fn sheet_foot_rects(&self, r: Rect, foot: &[Item]) -> Vec<(Item, Rect)> {
        let y = r.bottom() - SHEET_FOOT + 6;
        let w = r.w - 2 * PAD;
        match foot.len() {
            0 => Vec::new(),
            1 => vec![(foot[0], Rect::new(r.x + PAD, y, w, 44))],
            _ => {
                // the danger button is narrower and set apart from the main one
                let danger = destructive(foot[1]) && !destructive(foot[0]);
                let (a, b) = if danger { (w * 58 / 100, w * 30 / 100) } else { ((w - 8) / 2, (w - 8) / 2) };
                vec![(foot[0], Rect::new(r.x + PAD, y, a, 44)), (foot[1], Rect::new(r.right() - PAD - b, y, b, 44))]
            }
        }
    }

    // ------------------------------------------------------------------------------------------------ drawing

    pub(super) fn sheet_draw_chrome(&self, cv: &mut Canvas, title: &str) {
        let f = self.fonts();
        let (r, back, close) = self.sheet_chrome();
        // it floats over the docks: a shadow on its left and under it
        if self.dock_on() {
            for d in 1..=8 { cv.blend(Rect::new(r.x - d * 2, r.y + d, 2, r.h), BG, (70 - d * 7) as u32); }
        }
        cv.round(r, 8, ACCENT, 255);
        cv.round(r.inset(2), 7, DLG_BG, 255);
        let left = back.map_or(r.x + 14, |b| b.right() + 8);
        let mut t = title.to_string();
        // a title that is too long for the bar loses its detail (the path after "Storage"), not its name
        if f.mb.width(&t) > close.x - left - 6 { if let Some((name, _)) = t.split_once(" \u{b7} ") { t = name.to_string(); } }
        while f.mb.width(&t) > close.x - left - 6 && t.chars().count() > 2 {
            let mut c = t.chars();
            c.next();
            c.next();
            t = format!("\u{2026}{}", c.as_str());
        }
        cv.text_in(&f.mb, Rect::new(left, r.y + 4, close.x - left - 4, 44), 0, &t, TEXT);
        if let Some(b) = back {
            btn(cv, b, BTN, BTN_BORDER);
            cv.text_in(&f.sb, b, 1, "Back", TEXT);
        }
        btn(cv, close, BTN, BTN_BORDER);
        cv.text_in(&f.mb, close, 1, "\u{d7}", TEXT);
        cv.hline(r.x + 6, r.right() - 7, r.y + SHEET_HEAD - 1, LINE);
    }

    fn sheet_draw_footer(&self, cv: &mut Canvas, r: Rect, foot: &[(Item, Rect)]) {
        let f = self.fonts();
        if !foot.is_empty() { cv.hline(r.x + 6, r.right() - 7, r.bottom() - SHEET_FOOT, LINE); }
        for &(it, b) in foot {
            let red = destructive(it);
            let armed = self.studio.md.armed.is_some_and(|(a, t)| a == it && self.now - t < 3.0);
            // the main action is the accent one (Store); a danger button is red and says when it waits for its second tap
            let primary = it == Item::RefSave;
            btn(cv, b, if armed { mix(DLG_BG, RED, 90) } else if red { mix(DLG_BG, RED, 30) } else if primary { mix(DLG_BG, ACCENT, 40) } else { BTN }, if red { RED } else if primary { ACCENT } else { BTN_BORDER });
            let label = if armed { "Tap again to confirm".to_string() } else if it == Item::RefSave {
                self.ref_store_label()
            } else if it == Item::MeasSrc || it == Item::MeasSrcB {
                format!("{}: {}", if it == Item::MeasSrc { "A" } else { "B" }, self.pair_source_caption(if it == Item::MeasSrc { self.meas_src } else { self.meas_src_b }))
            } else { self.sheet_label(it) };
            let font = if f.mb.width(&label) > b.w - 12 { &f.sb } else { &f.mb };
            cv.text_in(font, b, 1, &label, if red { RED } else { TEXT });
        }
    }

    pub(super) fn draw_sheet(&self, cv: &mut Canvas) {
        if !self.sheet_active() {
            return;
        }
        match self.menu {
            MenuId::MeasAdd => return self.draw_sheet_add(cv),
            MenuId::AddPanel => return self.draw_sheet_add_panel(cv),
            MenuId::Pick => return self.draw_sheet_pick(cv),
            MenuId::Main => return self.draw_menu_grid(cv),
            _ => {}
        }
        let Some(g) = self.sheet_geo() else { return };
        let f = self.fonts();
        self.sheet_draw_chrome(cv, &self.menu_title(self.menu));
        let spec = self.sheet_spec(self.menu).expect("sheet");
        let cur = self.sheet_tab(&spec);
        for (i, r) in g.tabs.iter().enumerate() {
            let on = i == cur;
            if on {
                cv.round(*r, 5, rgb(0x2e, 0x2e, 0x32), 255);
                cv.round_outline(*r, 5, ACCENT);
            }
            let dimmed = mix(DLG_BG, DIM, 60);
            cv.text_in(&f.sb, *r, 1, spec.tabs[i].name, if on { TEXT } else if spec.tabs[i].holds() { DIM } else { dimmed });
        }
        for row in &g.rows {
            self.sheet_draw_row(cv, row);
        }
        self.sheet_draw_footer(cv, g.r, &g.foot);
    }

    /// The minus and plus of a Number row, drawn as strokes: the bundled font has the hyphen but no minus sign (U+2212),
    /// and a hyphen is short and sits high. Both strokes are 2 px thick and as wide as the plus is tall.
    pub(super) fn draw_sign(cv: &mut Canvas, r: Rect, plus: bool) {
        let (cx, cy) = (r.x + r.w / 2, r.y + r.h / 2);
        cv.fill(Rect::new(cx - 7, cy - 1, 14, 2), TEXT);
        if plus { cv.fill(Rect::new(cx - 1, cy - 7, 2, 14), TEXT); }
    }

    fn sheet_draw_row(&self, cv: &mut Canvas, row: &SheetRowGeo) {
        let f = self.fonts();
        let it = row.it;
        let label = self.sheet_label(it);
        let c = row.ctrl;
        if row.kind != RowKind::Link {
            let font = if f.m.width(&label) > row.label.w - 6 { &f.s } else { &f.m };
            cv.set_clip(Rect::new(row.label.x, row.label.y, row.label.w - 4, row.label.h));
            cv.text_in(font, row.label, 0, &label, LABEL);
            cv.no_clip();
        }
        match row.kind {
            RowKind::Choice => {
                let (opts, sel) = self.sheet_choice(it);
                let n = opts.len().max(1) as i32;
                let sw = (c.w - (n - 1) * 4) / n;
                for (k, o) in opts.iter().enumerate() {
                    let r = Rect::new(c.x + k as i32 * (sw + 4), c.y, sw, c.h);
                    if it == Item::RefColor {
                        // slot colour as swatches
                        cv.round(r, 5, REF_COLORS[k.min(REF_COLORS.len() - 1)].1, 255);
                        if sel == Some(k) { cv.round_outline(r.inset(-1), 6, TEXT); cv.round_outline(r, 5, DARK_TEXT); }
                        continue;
                    }
                    // the lines of a decoder: a channel in its colour, as its pill in the bottom bar
                    let wave = matches!(it, Item::DecSrc | Item::DecSrc2 | Item::DecCs | Item::DecSrc4 | Item::DecDiff).then(|| o.strip_prefix("CH").and_then(|n| n.parse::<usize>().ok()).filter(|n| (1..=NCH).contains(n)).map(|n| CH_COLORS[n - 1])).flatten();
                    if let Some(col) = wave {
                        if sel == Some(k) { cv.round(r, 5, col, 255); } else { btn(cv, r, BTN, col); }
                        cv.text_in(&f.sb, r, 1, o, if sel == Some(k) { DARK_TEXT } else { col });
                    } else if sel == Some(k) {
                        cv.round(r, 5, ACCENT, 255);
                        cv.text_in(&f.sb, r, 1, o, DARK_TEXT);
                    } else if it == Item::DecSel && self.m.dec_is_on(k) {
                        // four decoders run side by side: one that is on shows it while another is edited, marked
                        // like a pair that is measured
                        btn(cv, r, mix(BTN, ACCENT, 50), ACCENT);
                        cv.text_in(&f.sb, r, 1, o, TEXT);
                    } else {
                        btn(cv, r, BTN, BTN_BORDER);
                        cv.text_in(&f.s, r, 1, o, TEXT);
                    }
                }
            }
            RowKind::Pick => {
                // - value +: the steps of the dialog of the Default layout, and the value opens the list. A row
                // whose list is long and has no order (the protocol of a decoder) is the value alone
                let plain = Self::sheet_pick_plain(it);
                let (mi, mid, pl) = if plain { (Rect::new(c.x, c.y, 0, 0), c, Rect::new(c.right(), c.y, 0, 0)) } else { Self::value_parts(c) };
                if !plain {
                    btn(cv, mi, BTN, BTN_BORDER);
                    Self::draw_sign(cv, mi, false);
                }
                btn(cv, mid, BTN, BTN_BORDER);
                let v = self.value(it);
                let font = if f.m.width(&v) > mid.w - 34 { &f.s } else { &f.m };
                cv.set_clip(Rect::new(mid.x + 6, mid.y, mid.w - 30, mid.h));
                cv.text_in(font, Rect::new(mid.x + 10, mid.y, mid.w - 34, mid.h), 0, &v, TEXT);
                cv.no_clip();
                chevron_down(cv, mid.x + mid.w - 18, mid.y + mid.h / 2 - 3, DIM);
                if !plain {
                    btn(cv, pl, BTN, BTN_BORDER);
                    Self::draw_sign(cv, pl, true);
                }
            }
            RowKind::Number => {
                let (mi, mid, pl) = Self::value_parts(c);
                btn(cv, mi, BTN, BTN_BORDER);
                Self::draw_sign(cv, mi, false);
                btn(cv, mid, BTN, BTN_BORDER);
                cv.set_clip(mid);
                cv.text_in(&f.m, mid, 1, &self.value(it), TEXT);
                cv.no_clip();
                btn(cv, pl, BTN, BTN_BORDER);
                Self::draw_sign(cv, pl, true);
            }
            RowKind::Toggle => {
                let on = self.sheet_on(it);
                cv.round(c, c.h / 2, if on { ACCENT } else { BTN_BORDER }, 255);
                cv.circle(if on { c.right() - c.h / 2 } else { c.x + c.h / 2 }, c.y + c.h / 2, c.h / 2 - 3, if on { DARK_TEXT } else { DIM });
            }
            RowKind::Info => {
                // what the line says, at the right; the small font when the normal one does not fit
                let v = self.value(it);
                let font = if f.m.width(&v) > c.w - 8 { &f.s } else { &f.m };
                cv.set_clip(c);
                if font.width(&v) > c.w - 8 {
                    // too long even then: two lines, broken at the space nearest the middle, so the start is not cut off
                    let (a, b) = Self::two_lines(&v);
                    cv.text_in(font, Rect::new(c.x, c.y + 2, c.w - 6, c.h / 2), 2, a, TEXT);
                    cv.text_in(font, Rect::new(c.x, c.y + c.h / 2 - 2, c.w - 6, c.h / 2), 2, b, TEXT);
                } else {
                    cv.text_in(font, Rect::new(c.x, c.y, c.w - 6, c.h), 2, &v, TEXT);
                }
                cv.no_clip();
            }
            RowKind::Link => {
                let open = matches!(it, Item::Open(_));
                // a pair that is measured between the two sources shown is marked: a tap takes it away again
                let added = matches!(it, Item::MeasPair(k) if self.m.meas2.contains(&(k, self.meas_src, self.meas_src_b)));
                btn(cv, c, if added { mix(BTN, ACCENT, 50) } else { BTN }, if added { ACCENT } else { BTN_BORDER });
                if added {
                    let (kx, ky) = (c.right() - 30, c.y + c.h / 2);
                    for d in 0..2 { cv.line(kx, ky + d, kx + 5, ky + 5 + d, ACCENT); cv.line(kx + 5, ky + 5 + d, kx + 14, ky - 5 + d, ACCENT); }
                }
                let tail = if it == Item::MathExprEdit { self.value(Item::MathExprI) } else { String::new() };
                let room = c.w - 40 - if tail.is_empty() { 0 } else { (f.s.width(&tail) + 12).min(c.w / 2) };
                cv.set_clip(Rect::new(c.x + 8, c.y, room, c.h));
                cv.text_in(&f.m, Rect::new(c.x + 14, c.y, room, c.h), 0, &label, TEXT);
                cv.no_clip();
                if !tail.is_empty() {
                    cv.set_clip(Rect::new(c.x + room, c.y, c.w - room - 30, c.h));
                    cv.text_in(&f.s, Rect::new(c.x + room, c.y, c.w - room - 30, c.h), 2, &tail, DIM);
                    cv.no_clip();
                }
                if open {
                    chevron(cv, c.right() - 16, c.y + c.h / 2, 1, DIM);
                }
            }
        }
        if self.item_blocked(it).is_some() {
            Self::draw_blocked_veil(cv, if row.kind == RowKind::Link { Rect::new(0, 0, 0, 0) } else { row.label }, c);
        }
    }

    // ------------------------------------------------------------------------------------------------ tapping

    /// The Back button: to the page before, or out of a picker.
    fn sheet_back(&mut self) {
        if self.menu == MenuId::AddPanel && self.sheet.add.kind.is_some() {
            self.sheet.add.kind = None;
            return;
        }
        if self.menu == MenuId::Pick {
            return self.close_picker();
        }
        if let Some((menu, focus)) = self.menu_history.pop() {
            self.menu = menu;
            self.focus = focus;
            self.parent = self.menu_history.last().map(|e| e.0);
        } else {
            self.menu = self.parent.take().unwrap_or(MenuId::Main);
            self.focus = 0;
        }
    }

    /// A tap while the sheet is the menu. False: outside it.
    pub(super) fn sheet_tap(&mut self, x: i32, y: i32) -> bool {
        let (r, back, close) = self.sheet_chrome();
        if !r.contains(x, y) {
            return false;
        }
        if close.contains(x, y) {
            self.close_menu();
            return true;
        }
        if back.is_some_and(|b| b.contains(x, y)) {
            self.sheet_back();
            return true;
        }
        match self.menu {
            MenuId::MeasAdd => return self.sheet_add_tap(x, y),
            MenuId::AddPanel => return self.sheet_add_panel_tap(x, y),
            MenuId::Pick => return self.sheet_pick_tap(x, y),
            MenuId::Main => return self.menu_grid_tap(x, y),
            _ => {}
        }
        let Some(g) = self.sheet_geo() else { return true };
        for (i, t) in g.tabs.iter().enumerate() {
            if t.contains(x, y) {
                // a tab that holds nothing now is not opened
                if self.sheet_spec(self.menu).is_some_and(|s| s.tabs.get(i).is_some_and(SheetTab::holds)) { self.sheet_set_tab(i); }
                return true;
            }
        }
        for &(it, b) in &g.foot {
            if b.contains(x, y) {
                // a danger button asks first: the second tap within 3 s does it
                if destructive(it) && !self.studio.md.armed.is_some_and(|(a, t)| a == it && self.now - t < 3.0) {
                    self.studio.md.armed = Some((it, self.now));
                    return true;
                }
                self.studio.md.armed = None;
                // the pinned sources of the pair page open their list
                if matches!(it, Item::MeasSrc | Item::MeasSrcB) { self.open_picker(it); } else { self.activate(it); }
                return true;
            }
        }
        for row in &g.rows {
            if !row.row.contains(x, y) {
                continue;
            }
            self.sheet_row_tap(row, x, y);
            break;
        }
        true
    }

    fn sheet_row_tap(&mut self, row: &SheetRowGeo, x: i32, y: i32) {
        let it = row.it;
        match row.kind {
            RowKind::Choice => {
                if !row.ctrl.contains(x, y) {
                    return;
                }
                let (opts, _) = self.sheet_choice(it);
                let n = opts.len().max(1) as i32;
                let k = ((x - row.ctrl.x) * n / row.ctrl.w).clamp(0, n - 1) as usize;
                if it == Item::TrigType && k == 3 {
                    return self.open_picker(it);
                }
                self.select_option(it, &opts[k]);
            }
            RowKind::Pick if Self::sheet_pick_plain(it) => {
                if row.ctrl.contains(x, y) || row.label.contains(x, y) { self.open_picker(it); }
            }
            RowKind::Pick => {
                let (mi, _, pl) = Self::value_parts(row.ctrl);
                if mi.contains(x, y) {
                    self.adjust(it, -1);
                } else if pl.contains(x, y) {
                    self.adjust(it, 1);
                } else if row.ctrl.contains(x, y) || row.label.contains(x, y) {
                    self.open_picker(it);
                }
            }
            RowKind::Number => {
                let (mi, mid, pl) = Self::value_parts(row.ctrl);
                if mi.contains(x, y) {
                    self.adjust(it, -1);
                } else if pl.contains(x, y) {
                    self.adjust(it, 1);
                } else if mid.contains(x, y) {
                    // the value opens the numeric pad
                    self.open_numpad(it);
                }
            }
            RowKind::Toggle => {
                if let Item::RefShow(_) = it {
                    self.activate(it);
                } else if it == Item::SimShow {
                    // one step is the switch: its value is the slot's name, never "On" or "Off", and selecting an
                    // option stepped it eight times, back to where it was
                    self.adjust(it, 1);
                } else {
                    let on = self.sheet_on(it);
                    self.select_option(it, if on { "Off" } else { "On" });
                }
            }
            RowKind::Link => self.activate(it),
            RowKind::Info => {}
        }
    }

    // ------------------------------------------------------------------------------------------------ pick page

    /// Cells of the picker: two columns of up to seven rows.
    pub(super) fn sheet_pick_cells(&self) -> (Vec<(usize, Rect)>, usize, usize) {
        let (r, _, _) = self.sheet_chrome();
        let n = self.pick.map_or(0, |p| self.options(p.it).len());
        let per = SHEET_MAX_ROWS * 2;
        let pages = n.div_ceil(per).max(1);
        let page = self.sheet_page(self.pick.map(|p| p.it)).min(pages - 1);
        let w = (r.w - 2 * PAD - 8) / 2;
        let cells = (page * per..n.min((page + 1) * per)).enumerate().map(|(i, k)| {
            let (col, row) = ((i / SHEET_MAX_ROWS) as i32, (i % SHEET_MAX_ROWS) as i32);
            (k, Rect::new(r.x + PAD + col * (w + 8), r.y + SHEET_HEAD + 6 + row * SHEET_ROW, w, 40))
        }).collect();
        (cells, page, pages)
    }

    fn sheet_pager(&self, page: usize, pages: usize) -> [Rect; 2] {
        let r = self.sheet_rect();
        let y = r.bottom() - SHEET_FOOT + 6;
        let half = (r.w - 2 * PAD - 8) / 2;
        let _ = (page, pages);
        [Rect::new(r.x + PAD, y, half, 44), Rect::new(r.x + PAD + half + 8, y, half, 44)]
    }

    fn draw_sheet_pager(&self, cv: &mut Canvas, page: usize, pages: usize) {
        if pages < 2 {
            return;
        }
        let f = self.fonts();
        let r = self.sheet_rect();
        cv.hline(r.x + 6, r.right() - 7, r.bottom() - SHEET_FOOT, LINE);
        for (b, label) in self.sheet_pager(page, pages).into_iter().zip(["Previous".to_string(), format!("Next \u{b7} {}/{}", page + 1, pages)]) {
            btn(cv, b, BTN, BTN_BORDER);
            cv.text_in(&f.m, b, 1, &label, TEXT);
        }
    }

    fn draw_sheet_pick(&self, cv: &mut Canvas) {
        let Some(p) = self.pick else { return };
        let f = self.fonts();
        self.sheet_draw_chrome(cv, &self.menu_title(MenuId::Pick));
        let opts = self.options(p.it);
        let cur = self.value(p.it);
        let (cells, page, pages) = self.sheet_pick_cells();
        for (k, c) in cells {
            let on = opts[k] == cur;
            if on {
                cv.round(c, 5, ACCENT, 255);
            } else {
                btn(cv, c, BTN, BTN_BORDER);
            }
            let text = &opts[k];
            let font = if f.m.width(text) + 10 <= c.w { &f.m } else { &f.s };
            cv.set_clip(c);
            cv.text_in(font, Rect::new(c.x + 4, c.y, c.w - 8, c.h), 1, text, if on { DARK_TEXT } else { TEXT });
            cv.no_clip();
        }
        self.draw_sheet_pager(cv, page, pages);
    }

    fn sheet_pick_tap(&mut self, x: i32, y: i32) -> bool {
        let (cells, page, pages) = self.sheet_pick_cells();
        for (k, c) in cells {
            if c.contains(x, y) {
                self.activate(Item::PickOpt(k));
                return true;
            }
        }
        if pages > 1 {
            let [prev, next] = self.sheet_pager(page, pages);
            let key = self.pick.map(|p| p.it);
            if prev.contains(x, y) && page > 0 {
                self.sheet_set_page(key, page - 1);
            } else if next.contains(x, y) && page + 1 < pages {
                self.sheet_set_page(key, page + 1);
            }
        }
        true
    }

    // ------------------------------------------------------------------------------------------------ add measurement

    /// The sources on the screen: channels that are on and references that are shown.
    pub(super) fn sheet_shown_sources(&self) -> Vec<WaveId> {
        self.wave_ids().into_iter().filter(|id| match *id {
            WaveId::Ch(c) => c < n_ch() && self.m.ch[c].on,
            WaveId::Ref(i) => self.refs.get(i).and_then(|r| r.as_ref()).is_some_and(|r| r.on),
        }).collect()
    }

    /// Opens the add flow with a source that is on the screen.
    pub(super) fn sheet_add_open(&mut self) {
        let shown = self.sheet_shown_sources();
        if !shown.contains(&self.meas_src) {
            if let Some(&first) = shown.first() {
                self.sheet_pick_source(first);
            }
        }
        self.sheet.search.clear();
    }

    pub(super) fn sheet_pick_source(&mut self, id: WaveId) {
        let name = self.wave_name(id);
        self.select_option(Item::MeasSrc, &name);
    }

    /// The entries of the list the filter leaves: the search text, else the category chip.
    pub(super) fn sheet_add_entries(&self) -> Vec<AddEntry> {
        let q = self.sheet.search.trim().to_ascii_lowercase();
        super::meascatalog::add_entries().into_iter().filter(|e| {
            if q.is_empty() { e.group == self.sheet.add_group } else { e.name.to_ascii_lowercase().contains(&q) }
        }).collect()
    }

    /// Geometry of the add page: source chips, filter chips, the list rows and the pager.
    pub(super) fn sheet_add_geo(&self) -> AddGeo {
        let (r, _, _) = self.sheet_chrome();
        let shown = self.sheet_shown_sources();
        let y0 = r.y + SHEET_HEAD;
        let w = r.w - 2 * PAD;
        let n = shown.len().clamp(1, 8) as i32;
        let cw = (w - (n - 1) * 4) / n;
        let chips = shown.iter().take(8).enumerate().map(|(i, &id)| (id, Rect::new(r.x + PAD + i as i32 * (cw + 4), y0 + 2, cw, 40))).collect();
        let gw = (w - 4 * 4) / 5;
        let groups = (0..ADD_GROUPS.len()).map(|i| Rect::new(r.x + PAD + i as i32 * (gw + 4), y0 + 46, gw, 40)).collect();
        let find = Rect::new(r.x + PAD + 4 * (gw + 4), y0 + 46, gw, 40);
        let list_y = y0 + 90;
        // a grid of tiles, two across: a picture and a name each
        let fit = Self::tile_fit(self.tiles_avail(list_y));
        let entries = self.sheet_add_entries();
        let _ = fit;
        // the grid scrolls inside its category: no pages, no pager
        let list = Rect::new(r.x + PAD, list_y, w, r.bottom() - 8 - list_y);
        let scroll = self.studio.md.tile_scroll.clamp(0, self.tile_scroll_max_of(entries.len(), list.h));
        let rows: Vec<(usize, Rect)> = (0..entries.len())
            .map(|k| (k, Self::tile_rect(Rect::new(list.x, list.y - scroll, list.w, list.h), k)))
            .filter(|(_, t)| t.bottom() > list.y && t.y < list.bottom())
            .collect();
        let none = Rect::new(0, 0, 0, 0);
        AddGeo { chips, groups, find, rows, page: 0, pages: 1, prev: none, next: none, list }
    }

    /// The button beside the close button of the quick Add panel: it opens the Measure page of every layout, with
    /// the categories, the sources a measurement reads, Selected and Settings.
    pub(super) fn sheet_add_expand(&self) -> Rect {
        let (_, _, close) = self.sheet_chrome();
        Rect::new(close.x - 48, close.y, 44, 44)
    }

    /// From the quick Add panel to the Measure page and back; the menu stays the same one, so Back and close act as before.
    pub(super) fn studio_meas_page(&mut self, page: bool) {
        self.studio.meas_page = page;
        self.studio.md.info = None;
        if !page { self.sheet_add_open(); }
    }

    pub(super) fn draw_sheet_add(&self, cv: &mut Canvas) {
        let f = self.fonts();
        self.sheet_draw_chrome(cv, "Add measurements");
        let expand = self.sheet_add_expand();
        btn(cv, expand, BTN, BTN_BORDER);
        super::studio::icon_expand(cv, expand.x + expand.w / 2, expand.y + expand.h / 2, TEXT);
        let g = self.sheet_add_geo();
        if g.chips.is_empty() {
            let (r, _, _) = self.sheet_chrome();
            cv.text_in(&f.m, Rect::new(r.x + PAD, r.y + SHEET_HEAD + 2, r.w - 2 * PAD, 40), 0, "No source is on: turn on a channel", DIM);
        }
        for &(id, c) in &g.chips {
            let on = id == self.meas_src;
            if on {
                cv.round(c, 5, self.wave_color(id), 255);
            } else {
                btn(cv, c, BTN, self.wave_color(id));
            }
            cv.text_in(&f.mb, c, 1, &self.wave_name(id), if on { DARK_TEXT } else { TEXT });
        }
        let searching = !self.sheet.search.trim().is_empty();
        for (i, c) in g.groups.iter().enumerate() {
            let on = !searching && i == self.sheet.add_group;
            if on {
                cv.round(*c, 5, rgb(0x2e, 0x2e, 0x32), 255);
                cv.round_outline(*c, 5, ACCENT);
            } else {
                btn(cv, *c, BTN, BTN_BORDER);
            }
            cv.text_in(&f.s, *c, 1, ADD_GROUPS[i], if on { TEXT } else { DIM });
        }
        btn(cv, g.find, if searching { ACCENT } else { BTN }, if searching { ACCENT } else { BTN_BORDER });
        let label = if searching { format!("\u{d7} {}", self.sheet.search.trim()) } else { "Find".to_string() };
        cv.set_clip(g.find);
        cv.text_in(&f.s, Rect::new(g.find.x + 3, g.find.y, g.find.w - 6, g.find.h), 1, &label, if searching { DARK_TEXT } else { TEXT });
        cv.no_clip();
        let entries = self.sheet_add_entries();
        if entries.is_empty() {
            let (r, _, _) = self.sheet_chrome();
            cv.text_in(&f.m, Rect::new(r.x + PAD, r.y + SHEET_HEAD + 90, r.w - 2 * PAD, 44), 0, "Nothing matches", DIM);
        }
        cv.set_clip(g.list);
        for &(k, tile) in &g.rows {
            self.draw_add_tile(cv, tile, entries[k].cat);
        }
        cv.no_clip();
        // a scrollbar, since there is no pager
        let total = entries.len().div_ceil(super::addtiles::TILE_COLS as usize) as i32 * super::addtiles::TILE_H;
        if total > g.list.h {
            let (x, th) = (g.list.right() - 3, (g.list.h * g.list.h / total).max(20));
            let ty = g.list.y + (g.list.h - th) * self.studio.md.tile_scroll.clamp(0, (total - g.list.h).max(1)) / (total - g.list.h).max(1);
            cv.fill(Rect::new(x, ty, 2, th), DIM);
        }
    }

    pub(super) fn sheet_add_tap(&mut self, x: i32, y: i32) -> bool {
        if self.sheet_add_expand().contains(x, y) {
            self.studio_meas_page(true);
            return true;
        }
        let g = self.sheet_add_geo();
        for &(id, c) in &g.chips {
            if c.contains(x, y) {
                self.sheet_pick_source(id);
                return true;
            }
        }
        for (i, c) in g.groups.iter().enumerate() {
            if c.contains(x, y) {
                self.sheet.add_group = i;
                self.sheet.search.clear();
                self.studio.md.tile_scroll = 0;
                self.sheet_set_page(None, 0);
                return true;
            }
        }
        if g.find.contains(x, y) {
            if self.sheet.search.trim().is_empty() {
                self.wlan.osk = Some(wlan::Osk::for_search(""));
            } else {
                self.sheet.search.clear();
                self.sheet_set_page(None, 0);
            }
            return true;
        }

        let entries = self.sheet_add_entries();
        for &(k, tile) in &g.rows {
            if !tile.contains(x, y) {
                continue;
            }
            let cat = entries[k].cat;
            // the "i": the info card
            if Self::tile_info_rect(tile).contains(x, y) {
                self.studio.md.info = Some(cat);
                break;
            }
            match self.add_preview(cat) {
                Preview::Off(ch) => {
                    // the first tap says why; a second one soon after is the fix, in place
                    if self.studio.md.need_tap.is_some_and(|(c, t)| c == cat && self.now - t < 3.0) {
                        self.studio.md.need_tap = None;
                        self.select_option(Item::ChOn(ch), "On");
                    } else {
                        self.studio.md.need_tap = Some((cat, self.now));
                        self.toast(&format!("Needs CH{} on: tap again to turn it on", ch + 1));
                    }
                }
                Preview::Reason(s) => self.toast(&s),
                // an added tile shows its card, where Remove is
                _ if self.add_is_on(cat) => self.studio.md.info = Some(cat),
                _ => self.sheet_add_toggle(cat),
            }
            break;
        }
        true
    }

    /// One tap adds the measurement (a second one removes it); a new one is pinned so it shows at once.
    pub(super) fn sheet_add_toggle(&mut self, cat: super::meascatalog::Cat) {
        let was = self.add_is_on(cat);
        self.catalog_add(cat);
        if !was && self.add_is_on(cat) {
            let key = self.add_key(cat);
            self.studio_set_pin(key, true);
            // the toast says what it reads, not only that it was added
            let rows = self.meas_rows();
            if let Some(row) = self.studio_index(key).and_then(|i| rows.get(i)) {
                if row.v.is_finite() && !row.computing {
                    self.toast(&format!("{} {} \u{b7} {}", row.name, self.wave_name(row.src), row.text()));
                }
            }
        }
    }

    // ------------------------------------------------------------------------------------------------ add panel

    /// Whether the panel of a tile is on the screen.
    pub(super) fn panel_shown(&self, k: PanelKind) -> bool {
        match k {
            PanelKind::Measurements => matches!(self.studio.view, StudioView::Docked | StudioView::Floating | StudioView::Only),
            PanelKind::Decode => self.table == Some(tables::TableKind::Decode),
            PanelKind::Spectrum => self.m.pane == tables::PANE_SPECTRUM || self.m.view == AView::Spectrum,
            PanelKind::Eye => self.m.view == AView::Eye,
            PanelKind::Histogram => self.m.hist != Hist::Off,
            PanelKind::Trend => self.m.pane == tables::PANE_TREND,
            PanelKind::Cursor => self.m.cursor != CursorMode::Off,
            PanelKind::Sim => self.studio.view == StudioView::Sim,
        }
    }

    /// Opens a panel docked, with the pages that already exist; the sheet closes so the panel is seen.
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub(super) fn studio_add_panel(&mut self, k: PanelKind) {
        self.close_menu();
        self.studio_apply_panel(k);
    }

    /// The panel of a kind on the screen; the Add panel calls this and stays open.
    pub(super) fn studio_apply_panel(&mut self, k: PanelKind) {
        match k {
            PanelKind::Measurements => self.studio_set_view(StudioView::Docked),
            PanelKind::Decode => {
                if self.table != Some(tables::TableKind::Decode) {
                    self.activate(Item::DecTable);
                }
            }
            PanelKind::Spectrum => self.select_option(Item::PaneSel, "Spectrum"),
            PanelKind::Trend => self.select_option(Item::PaneSel, "Trend"),
            PanelKind::Eye => {
                self.select_option(Item::EyePlace, "Side");
                if self.m.view != AView::Eye {
                    self.adjust(Item::EyeOn, 1);
                }
            }
            PanelKind::Histogram => self.select_option(Item::Histogram, "Vertical"),
            PanelKind::Cursor => {
                if self.m.cursor == CursorMode::Off {
                    self.select_option(Item::CurMode, "Time");
                }
            }
            PanelKind::Sim => self.studio_set_view(StudioView::Sim),
        }
        self.relayout();
    }

}

/// The parts of the add-measurement page.
pub(super) struct AddGeo {
    /// the room of the tile grid
    pub list: Rect,
    pub chips: Vec<(WaveId, Rect)>,
    pub groups: Vec<Rect>,
    pub find: Rect,
    pub rows: Vec<(usize, Rect)>,
    #[allow(dead_code)] // the page geometry is complete; touches use only `pages` so far
    pub page: usize,
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    pub pages: usize,
    #[allow(dead_code)] // the page geometry is complete; touches use only `pages` so far
    pub prev: Rect,
    #[allow(dead_code)] // the page geometry is complete; touches use only `pages` so far
    pub next: Rect,
}

impl Ui {
    /// The search text of the add list, from the keyboard.
    pub(super) fn sheet_set_search(&mut self, text: &str) {
        self.sheet.search = text.to_string();
        self.sheet_set_page(None, 0);
    }
}
