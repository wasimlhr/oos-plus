// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Layout, rendering, menus and input handling of the OOS scope screen (1024 x 600).

use crate::board::{board, Board};
use crate::font::{Font, Fonts};
use crate::gfx::*;
use crate::math::*;
use crate::model::*;
use crate::platform::Platform;
use crate::protocols::{Decoder, Module};
use crate::wave::*;

mod acquire;
mod afg;
mod analysers;
mod analysis;
mod autoset;
mod bars;
#[cfg(test)]
mod batch1_tests;
#[cfg(test)]
mod batch2_tests;
#[cfg(test)]
mod batch3a_tests;
#[cfg(test)]
mod batch4_tests;
#[cfg(test)]
mod batch5_tests;
#[cfg(test)]
mod batch6_tests;
#[cfg(test)]
mod batch7_tests;
#[cfg(test)]
mod batch7b_tests;
#[cfg(test)]
mod batch8_tests;
#[cfg(test)]
mod search_bus_tests;
#[cfg(test)]
mod search_dense_tests;
#[cfg(test)]
mod search_several_tests;
#[cfg(test)]
mod search_types_tests;
#[cfg(test)]
mod search_zoom_tests;
#[cfg(test)]
mod batch9_tests;
#[cfg(test)]
mod batch10_tests;
#[cfg(test)]
mod batch11b_tests;
#[cfg(test)]
mod batch11c_tests;
#[cfg(test)]
mod batch11d_tests;
#[cfg(test)]
mod batch11_tests;
mod hotspots;
mod measdrag;
mod axis;
mod measview;
mod caticons;
mod fullchrome;
mod addtiles;
mod menugrid;
mod ranges;
mod trigslider;
mod sharedzoom;
mod classicbadges;
mod measpanel;
mod measscroll;
#[cfg(test)]
mod batch21_tests;
#[cfg(test)]
mod batch22_tests;
#[cfg(test)]
mod batch23_tests;
#[cfg(test)]
mod batch24_tests;
#[cfg(test)]
mod batch25_tests;
#[cfg(test)]
mod batch26_tests;
#[cfg(test)]
mod batch27_tests;
#[cfg(test)]
mod batch29_tests;
#[cfg(test)]
mod measpanel_redesign_tests;
#[cfg(test)]
mod batch30_tests;
#[cfg(test)]
mod batch31_tests;
#[cfg(test)]
mod batch33_tests;
#[cfg(test)]
mod dockwalk_tests;
#[cfg(test)]
mod batch34_tests;
#[cfg(test)]
mod scpimap;
#[cfg(test)]
mod uiaudit_tests;
#[cfg(test)]
mod batch12_tests;
#[cfg(test)]
mod batch13_tests;
#[cfg(test)]
mod batch14_tests;
#[cfg(test)]
mod batch16_tests;
#[cfg(test)]
mod batch17_tests;
#[cfg(test)]
mod batch18_tests;
#[cfg(test)]
mod batch19_tests;
pub mod bode;
mod bt;
mod bus;
mod busview;
mod decband;
mod decrows;
mod deskew;
mod dpt;
mod eyepage;
pub(crate) mod facts;
mod studio;
mod dock;
mod studio_views;
mod studiolayout;
mod sheet;
mod addrail;
mod addmenu;
mod studioctx;
mod scpiext;
mod scpi_layout;
mod scpiext_a;
mod scpiext_b;
mod scpiext_c;
#[cfg(test)]
#[path = "scpi_coverage_tests.rs"]
mod scpi_coverage_tests;
#[cfg(test)]
mod sheet_tests;
#[cfg(test)]
mod studio_parity_tests;
#[cfg(test)]
mod studio_trace_tests;
mod walk_tests;
mod callout;
#[cfg(test)]
mod meas_known_tests;
#[cfg(test)]
mod hdmi_tests;
#[cfg(test)]
mod layout_tests;
#[cfg(test)]
mod fix_tests;
mod feedback;
mod files;
mod hwmask;
mod hwzone;
mod init;
mod input;
mod item;
mod logger;
mod magnetic;
mod meascatalog;
mod measlabels;
mod measure_page;
mod measure_pairs;
mod measurement_nav;
mod menu_actions;
mod menu_dialog;
mod menu_items;
mod menu_text;
mod motor;
mod numpad;
mod offline;
mod overclock;
mod pages;
mod panes;
mod plot_layout;
pub mod pointer;
mod records;
mod reflook;
mod render;
mod render_derived;
mod render_marks;
mod save_recall;
mod scpi_operations;
mod scroll;
mod selfcal;
mod sertrig;
mod service;
mod sim;
pub mod startup;
mod strip;
mod extgen;
mod sysinfo;
mod tables;
mod tools;
mod trigger_controls;
mod update;
mod viewmore;
mod windows;
mod wlan;
mod worker;
mod measrow;
mod workflow;
mod workspace;
pub(crate) use scpi_operations::{OperationState, OperationToken, Operations};
mod backlight;
mod bus_candidate;
mod capture_memory;
mod features;
mod graphics;
#[cfg(test)]
mod interplay_tests;
mod filterplot;
mod livelines;
mod mathwave;
mod handles;
mod measchip;
use mathwave::FILT_PRESETS;
mod monitor;
mod reference_decode;
mod reference_search;
mod remote;
#[cfg(test)]
mod session_tests;
mod setting_items;
mod ssh;
mod time;
pub mod views;
#[cfg(test)]
use acquire::{fast_deposit, fast_plan, fast_take_next};
use item::{Item, PARTS};
#[cfg(test)]
mod backend_followup_tests;
mod bwtest;
mod chinfo;
pub mod la;
#[cfg(test)]
mod lint;
mod script;
#[cfg(test)]
pub(crate) mod testkit;

// The frame (top bar, waveform area, bottom bar with the cards) comes from the screen size: `Ui::frame`,
// `crate::layout`. Top bar: run state, menu buttons, network (buttons drawn 30 px high, tapped over the full bar
// height). Plot: the whole width and everything between the bars; the channel markers (left edge), the trigger level
// marker (right edge), the trigger position marker (top edge), the axis labels, the status line and the measurement
// results are drawn inside it.
pub use crate::layout::{Frame, Screen, METRICS, PANEL_H, PANEL_W};
/// Width of the marker strips at the left and right edge of the plot (the labels keep clear of them).
const MARK_W: i32 = 26;
/// Width of the volts axis of the histogram view.
const HIST_AXIS_W: i32 = 58;
/// Height of the badge of a reference trace (name row, scale and rate row).
const REF_BADGE_H: i32 = 40;
/// Width of the measurement strip when it is docked on the left or the right.
const STRIP_SIDE: i32 = 220;
/// Height of the fork's compact bottom bar (Menu, source pills, Overlay / Stacked, time, trigger) that replaces the
/// cards: `Ui::frame` gives the body the rest of `METRICS.bottom_h`.
pub const BOTTOM_H: i32 = 36;
/// The frame on the 1024 x 600 panel, for the code that still places things for the panel (`PANEL_W` / `PANEL_H`);
/// new code takes `Ui::frame` and `Ui::scr`.
pub const W: i32 = PANEL_W;
pub const H: i32 = PANEL_H;
pub const TOP: Rect = Rect::new(0, 0, W, METRICS.top_h);
#[cfg(test)]
const PLOT_Y: i32 = TOP.h;
pub const BOTTOM: Rect = Rect::new(0, H - BOTTOM_H, W, BOTTOM_H);
/// The normal text size's `Lay::results_w` and `Lay::strip_row`, for the same code.
const RESULTS_W: i32 = LAYS[0].results_w;
const STRIP_ROW: i32 = LAYS[0].strip_row;

/// Where a picture is drawn: everything on `under`, or with `over` (System > Graphics = GPU) the layer over the
/// traces, and the traces as `jobs` for the GPU (`Ui::render_layers`, `crate::gpu`).
pub struct Layers<'a> {
    pub under: &'a mut Canvas,
    pub over: Option<&'a mut Canvas>,
    pub jobs: Vec<TraceJob>,
}

impl Layers<'_> {
    /// The canvas for what lies over the traces.
    pub fn over(&mut self) -> &mut Canvas {
        match &mut self.over {
            Some(o) => o,
            None => self.under,
        }
    }
}

/// A plot window's traces for the GPU: the phosphor maps of the shown channels (`zoom`: of the zoom window) in
/// `area`, with the tables the CPU would use (`draw_traces`, `draw_graded`).
pub struct TraceJob {
    pub area: Rect,
    pub zoom: bool,
    pub graded: bool,
    pub show: [bool; NCH],
    /// per channel 1024 alphas (not graded)
    pub alpha: Vec<u8>,
    /// 1024 colours (graded)
    pub colours: Vec<u32>,
    pub shift: [u32; NCH],
    pub col: [u32; NCH],
}
/// Layout sizes that grow with the text (Display > Text size). Everything else keeps its place: touch targets are
/// 40 px high at every size, and the larger fonts are set into them.
pub struct Lay {
    /// pitch of the measurement and cursor rows under the plot (the bar is 2 px lower)
    pub strip_row: i32,
    /// menu dialogs: label column and column width of the list rows, column width of the button grids
    pub dlg_label: i32,
    pub dlg_col: i32,
    pub grid_col: i32,
    /// width of the measurement results overlay (statistics on) at the top right of the plot
    pub results_w: i32,
    /// baselines in the bottom cards: name, big value, line under it
    pub card: (i32, i32, i32),
    /// row pitch of the results overlay: one line, two lines
    pub res_pitch: (i32, i32),
}

/// The layouts of Normal, Large and Larger.
pub const LAYS: [Lay; 3] = [
    Lay {
        strip_row: 18,
        dlg_label: 96,
        dlg_col: 400,
        grid_col: 150,
        results_w: 196,
        card: (18, 42, 58),
        res_pitch: (22, 42),
    },
    Lay {
        strip_row: 21,
        dlg_label: 104,
        dlg_col: 420,
        grid_col: 168,
        results_w: 224,
        card: (19, 43, 59),
        res_pitch: (24, 46),
    },
    Lay {
        strip_row: 24,
        dlg_label: 116,
        dlg_col: 440,
        grid_col: 190,
        results_w: 250,
        card: (20, 43, 59),
        res_pitch: (26, 50),
    },
];
/// Menu dialogs: header height, row pitch and control height of the list rows (touch targets of 40 px).
const DLG_HEAD: i32 = 50;
const DLG_ROW: i32 = 44;
const DLG_CTRL: i32 = 40;
/// The field with the entry above the keys of an entry pad (ADR 0077).
const PAD_ENTRY_H: i32 = 62;
/// records kept for history playback
const HISTORY: usize = 200;
/// Points per line a decoder reads from the kept samples of a record while running (`Ui::decode_native`): 73 times
/// the screen record, and still decoded once per record in the frame that shows it.
const DEC_NATIVE_RUN: usize = 262_144;
/// The same for a stopped record, which is decoded once.
const DEC_NATIVE_STOP: usize = 4 << 20;
/// Most points a screen record has (3600, or twice that as min/max pairs). A record of a session with more is an
/// acquisition with its samples (`Ui::session_memory`, `Ui::session_record`).
const SESSION_SCREEN_MAX: usize = 7200;
/// Seconds the trigger level line stays after the level changed.
const LEVEL_LINE_S: f64 = 1.5;
/// Longest wait of a STOP for the hardware's final frame (`Stop::final_until`): a batch in progress, its read and the
/// raw read of the newest frame.
const STOP_FINAL_WAIT: f64 = 3.0;
/// LA words a history entry keeps: above this only the newest record has its LA (200 x 1 M words = 400 MB at most).
const HISTORY_LA_WORDS: usize = 1_000_000;
/// Segment counts of the segmented recording (0 = off); the menu offers those within the limit of the set-up
/// (`Ui::seg_limit`).
pub const SEGMENTS: [usize; 12] = [0, 10, 50, 100, 200, 500, 1000, 2000, 5000, 10_000, 20_000, 50_000];
/// Most segments of the software recording (history RAM).
pub const SEG_MAX: usize = 1000;
/// Hardware segments: intervals after each segment (0x4068, at most 2^32 x 6.4 ns = 27 s).
pub const SEG_IVLS: [f32; 8] = [0.0, 1e-6, 10e-6, 100e-6, 1e-3, 10e-3, 100e-3, 1.0];

// dark theme
const BG: Rgb = rgb(0x0a, 0x0a, 0x0b);
const BAR: Rgb = rgb(0x15, 0x15, 0x16);
const BTN: Rgb = rgb(0x1e, 0x1e, 0x20);
const BTN_BORDER: Rgb = rgb(0x34, 0x34, 0x37);
const CARD_BORDER: Rgb = rgb(0x2c, 0x2c, 0x2e);
const LINE: Rgb = rgb(0x3a, 0x3a, 0x3d);
const DLG_BG: Rgb = rgb(0x12, 0x12, 0x13);
const TEXT: Rgb = rgb(0xf2, 0xf2, 0xf2);
const LABEL: Rgb = rgb(0xc8, 0xc8, 0xc8);
const DIM: Rgb = rgb(0x90, 0x90, 0x92);
const AXIS_TEXT: Rgb = rgb(0xa4, 0xa4, 0xa6);
const DARK_TEXT: Rgb = rgb(0x0a, 0x0a, 0x0b);
/// What dims the screen behind a dialog that takes the input (the power dialog, an entry pad).
const SHADE: Rgb = rgb(0, 0, 0);
const ACCENT: Rgb = rgb(0xf4, 0xf4, 0xf4);
const GREEN: Rgb = rgb(0x3f, 0xb9, 0x50);
const RED: Rgb = rgb(0xf8, 0x51, 0x49);
const AMBER: Rgb = rgb(0xe3, 0xb3, 0x41);
const BLUE: Rgb = rgb(0x79, 0xa6, 0xff);
const CURSOR: Rgb = rgb(0xc4, 0xc4, 0xc6);
/// Figures counted from the signal, and figures that rest on a model.
const MEASURED: Rgb = TEXT;
const MODELLED: Rgb = AMBER;
const GRID_LINE: Rgb = rgb(0x20, 0x20, 0x22);
/// translucent read-outs over the analysers
const PANEL_BG: Rgb = rgb(0x16, 0x16, 0x17);
/// every second row of a table
const ROW_ALT: Rgb = rgb(0x10, 0x10, 0x11);
const GRID_AXIS: Rgb = rgb(0x38, 0x38, 0x3b);
/// graticule: dots of the division lines, frame / centre axes / ticks (the classic dotted scope look)
const GRID_DOT: Rgb = rgb(0x68, 0x68, 0x6b);
const GRID_FRAME: Rgb = rgb(0x64, 0x64, 0x67);
/// the measurement page: the box of the picked measurement, a step lighter than a button so it reads as one panel
const PREVIEW_BG: Rgb = rgb(0x24, 0x24, 0x28);
/// the measurement page: the header row of a group, between a button and its border
const GROUP_HEAD: Rgb = rgb(0x2a, 0x2a, 0x2e);
/// the self-cal banner over the plot: a dark amber, so it is not taken for a dialog while the inputs must stay free
const CAL_BANNER: Rgb = rgb(0x30, 0x22, 0x08);

/// Averaging counts of Acquire > Averages.
/// Stops of the density scale, rare to frequent (`heat_palette`).
const HEAT_STOPS: [(f32, f32, f32); 5] =
    [(40.0, 70.0, 255.0), (0.0, 220.0, 255.0), (40.0, 240.0, 70.0), (255.0, 235.0, 30.0), (255.0, 40.0, 30.0)];

/// Front-panel key codes (data byte of the MCU frame, see docs/protocols/frontpanel.md).
pub mod key {
    pub const REF: u8 = 0x01;
    pub const LA: u8 = 0x02;
    pub const GI: u8 = 0x03;
    pub const HMENU: u8 = 0x04;
    pub const NAVIGATE: u8 = 0x05;
    /// History keys: `<` steps back to older records, `>` forward to newer ones, play-stop goes live.
    pub const NAV_BACK: u8 = 0x06;
    pub const NAV_PLAY: u8 = 0x07;
    pub const NAV_FWD: u8 = 0x08;
    pub const CLEAR: u8 = 0x0b;
    pub const SLOPE: u8 = 0x09;
    pub const FORCE: u8 = 0x0a;
    pub const RUNSTOP: u8 = 0x0c;
    pub const AUTO: u8 = 0x0d;
    pub const SINGLE: u8 = 0x0e;
    pub const MEASURE: u8 = 0x0f;
    pub const CURSOR: u8 = 0x10;
    pub const DEFAULT: u8 = 0x11;
    pub const ANALYSE: u8 = 0x12;
    pub const TOUCHLOCK: u8 = 0x13;
    pub const QUICK: u8 = 0x14;
    pub const CH1: u8 = 0x15;
    pub const MATH: u8 = 0x19;
    pub const TMENU: u8 = 0x1a;
    pub const POWER: u8 = 0x1b;
    pub const HPOS_CW: u8 = 0x8a;
    pub const VPOS_CW: u8 = 0x9a;
    pub const MULTI2_CW: u8 = 0xaa;
    pub const MULTI1_CW: u8 = 0xba;
    pub const HSCALE_CW: u8 = 0xca;
    pub const VSCALE_CW: u8 = 0xda;
    pub const TLEVEL_CW: u8 = 0xea;
}

/// Options of the USB speed (decode::USB_SPEEDS).
const USPEED_NAMES: [&str; 3] = ["Auto", "Low 1.5M", "Full 12M"];

/// Longest record the web page takes as CSV (about 50 MB of text).
const WEB_CSV_MAX: usize = 2_000_000;

/// Height of a decoder band in the plot: its boxes and the room to the next band.
const DEC_BAND: i32 = 32;
/// Height of the boxes of a band: room for the bold small text and a finger.
const DEC_BOX: i32 = 24;

/// Channel colours (yellow, teal, pink, sky blue: a colour-blind safe set). CH2 teal and CH4 blue as the colour
/// rings of the scope's own probes and inputs have them (CH2 cyan, CH4 blue); until 0.1.11 the two were the other way.
const PALETTE: [Rgb; 8] = [
    rgb(0xf0, 0xe4, 0x42),
    rgb(0x2e, 0xc4, 0xa0),
    rgb(0xe0, 0x8c, 0xc0),
    rgb(0x56, 0xb4, 0xe9),
    rgb(0xf0, 0x90, 0x40),
    rgb(0xa0, 0x90, 0xf0),
    rgb(0xc0, 0xd0, 0x60),
    rgb(0xe0, 0x60, 0x60),
];
const CH_COLORS: [Rgb; NCH] = {
    let mut c = [PALETTE[0]; NCH];
    let mut i = 0;
    while i < NCH {
        c[i] = PALETTE[i];
        i += 1;
    }
    c
};
const MATH_COLOR: Rgb = rgb(0xa9, 0x8b, 0xff);
/// Trace colours of M1..M8 (M1 = the classic math colour).
const MATH_COLORS: [Rgb; 8] = [
    MATH_COLOR,
    rgb(0xff, 0x8b, 0xd0),
    rgb(0x8b, 0xe9, 0xff),
    rgb(0xff, 0xc8, 0x6b),
    rgb(0x9b, 0xff, 0x8b),
    rgb(0xff, 0x9b, 0x8b),
    rgb(0xd8, 0xd8, 0xd8),
    rgb(0x8b, 0xa8, 0xff),
];
/// Colours of the references.
const REF_COLORS: [(&str, Rgb); 5] = [
    ("White", rgb(0xd8, 0xd8, 0xd8)),
    ("Red", rgb(0xf0, 0x60, 0x60)),
    ("Green", rgb(0x60, 0xe0, 0x90)),
    ("Blue", rgb(0x70, 0xa8, 0xf8)),
    ("Violet", rgb(0xc8, 0x88, 0xf0)),
];

pub(crate) use analysers::scpi_views;

pub use eyepage::EYE_ITEMS;
pub use files::{FileAsk, FileKind};
// the limits that the model, the settings and the setup files count with too (model/limits.rs)
pub use crate::model::{CNT_MODES, DVM_MODES, PANE_HARMS, SEG_HW_MAX, SEG_IVL_MAX, VIEWS_MAX};
pub(crate) use crate::protocols::PROTOS;
pub(crate) use records::copy_page;
use bars::draw_slope;
use menu_dialog::fitted_text;

/// What Clear eye / Clear counter / Reset counts / Clear limit / Delete (zone) say when there was nothing to clear.
pub(super) const NOTHING_TO_CLEAR: &str = "Nothing to clear";

/// Keys of the frequency keypad: the unit ends the entry.
const FREQ_KEYS: [&str; 18] =
    ["7", "8", "9", "mHz", "4", "5", "6", "Hz", "1", "2", "3", "kHz", "0", ".", "Del", "MHz", "Clear", "Cancel"];
/// Keys of the hex keypad picker.
/// Largest protocol trigger value: 40 bits (five I2C data bytes).
pub const BUS_VAL_MAX: u64 = 0xff_ffff_ffff;

const KEYPAD: [&str; 19] =
    ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "A", "B", "C", "D", "E", "F", "Del", "Clear", "OK"];

/// The one long-press time: a finger held this long opens the hold action of what it is on (a channel's quick actions,
/// the Measure short menu, the plot's actions); a channel key on the front panel held as long switches AC/DC
/// (`Ui::panel_key`). A ring fills under the finger meanwhile (`Ui::press_ring`).
pub const LONG_PRESS: f64 = 0.5;

pub const KEY_NAMES: [(&str, u8); 30] = [
    ("navback", key::NAV_BACK),
    ("navplay", key::NAV_PLAY),
    ("navfwd", key::NAV_FWD),
    ("ref", 0x01),
    ("la", 0x02),
    ("gi", 0x03),
    ("hmenu", 0x04),
    ("navigate", 0x05),
    ("slope", 0x09),
    ("force", 0x0a),
    ("clear", 0x0b),
    ("runstop", 0x0c),
    ("auto", 0x0d),
    ("single", 0x0e),
    ("measure", 0x0f),
    ("cursor", 0x10),
    ("default", 0x11),
    ("analyse", 0x12),
    ("touchlock", 0x13),
    ("quick", 0x14),
    ("ch1", 0x15),
    ("ch2", 0x16),
    ("ch3", 0x17),
    ("ch4", 0x18),
    ("math", 0x19),
    ("tmenu", 0x1a),
    ("power", 0x1b),
    ("hpos", 0x8c),
    ("vpos", 0x9c),
    ("tlevel", 0xec),
];
/// Knob names -> CW code (CCW = CW + 1, push = CW + 2).
pub const KNOB_NAMES: [(&str, u8); 7] = [
    ("hpos", key::HPOS_CW),
    ("vpos", key::VPOS_CW),
    ("multi2", key::MULTI2_CW),
    ("multi1", key::MULTI1_CW),
    ("hscale", key::HSCALE_CW),
    ("vscale", key::VSCALE_CW),
    ("tlevel", key::TLEVEL_CW),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MenuId {
    Main,
    Channel(usize),
    /// the rarer settings of a channel
    ChMore(usize),
    /// measurements into a file at an interval (ui/logger.rs)
    Logger,
    Horizontal,
    Trigger,
    Acquire,
    Measure,
    MeasAdd,
    /// delay and phase between two channels
    MeasPair,
    /// statistics: on or off, population, columns, reset
    MeasStat,
    /// the settings card of one measurement row (ui/measrow.rs)
    MeasRow,
    /// shared reference levels (the Studio sheet has them on a tab of Measure)
    MeasLevels,
    /// separate fall levels
    MeasFall,
    /// digital voltmeter and counter (ui/tools.rs)
    Dvm,
    Cursor,
    Display,
    Utility,
    Math,
    Ref,
    Decode,
    /// one bus drawn on the plot
    Bus,
    Storage,
    Mask,
    Zone,
    Spectrum,
    Eye,
    Power,
    Magnetic,
    Inverter,
    Dpt,
    Identify,
    SelfCal,
    /// network overview and WLAN through a USB stick (ui/wlan.rs)
    Network,
    /// System > Features (ui/features.rs) and its decoder page
    Features,
    /// System > Features > Decoders, page n (`features::DEC_PAGE` a page)
    Decoders(usize),
    /// which network services run (ui/remote.rs)
    Remote,
    /// the SSH server: password, host key, trusted keys (ui/ssh.rs)
    Ssh,
    /// mice and keyboards through a Bluetooth stick (ui/bt.rs)
    Bluetooth,
    /// clock, NTP and time zone (ui/time.rs)
    Time,
    Bode,
    Sim,
    SimAdvanced,
    Afg,
    /// a generator beside the scope, on the USB-A port or the LAN (ui/extgen.rs)
    ExtGen,
    BwTest,
    /// the overflow menu of an analyser view or XY (viewmore.rs)
    ViewMore,
    /// logic analyser (ui/la.rs) and its line selection
    La,
    LaLines,
    LaPat,
    /// waveform windows and full screen (ui/windows.rs)
    Windows,
    /// list of all options of a choice (`Ui::pick` names the item); picking returns to the menu of the item
    Pick,
    /// Studio: the panels that can be added next to the plot (ui/sheet.rs)
    AddPanel,
    /// Full legacy measurement and pair catalog, including Selected.
    #[cfg_attr(not(test), allow(dead_code))] // only the tests use it
    MeasLegacy,
    /// Studio: the settings of the search added from the Add panel (ui/addrail.rs)
    Search,
    MeasSelected,
}

/// Buttons of the top bar.
#[derive(Clone, Copy, PartialEq, Debug)]
enum TopBtn {
    Menu(MenuId),
    System,
    Load,
    /// Back to live acquisition from a simulation or a stored waveform.
    Live,
}

/// An open picker: the choice item, and the menu (with its Back target and focus) to return to.
#[derive(Clone, Copy, Debug)]
struct Pick {
    it: Item,
    menu: MenuId,
    parent: Option<MenuId>,
    focus: usize,
    /// the first line of options shown, when the options are more than fit on the screen (ADR 0006)
    top: i32,
}

#[derive(Clone, Copy, Default)]
struct MStat {
    n: u64,
    sum: f64,
    sumsq: f64,
    min: f64,
    max: f64,
}

impl MStat {
    /// Adds one acquisition's value. With the current population only the latest acquisition is kept.
    fn push(&mut self, v: f64, current: bool) {
        if current { *self = MStat::default(); }
        if self.n == 0 { self.min = v; self.max = v; }
        self.n += 1;
        self.sum += v;
        self.sumsq += v * v;
        self.min = self.min.min(v);
        self.max = self.max.max(v);
    }

    fn add(&mut self, v: f64) {
        self.push(v, false);
    }
}

#[derive(Clone)]
struct RefWave {
    src: usize,
    /// index into REF_COLORS
    color: usize,
    label: String,
    t0: f64,
    dt: f64,
    v: std::sync::Arc<Vec<f32>>,
    vpd: f32,
    offset: f32,
    on: bool,
}

#[cfg(test)]
impl RefWave {
    /// A shown reference of the samples `v` (`dt` apart, the first at `t0`): slot colour 0, no label, 1 V/div, no
    /// offset; the tests change what they need with `..RefWave::test(..)`.
    fn test(v: Vec<f32>, dt: f64, t0: f64) -> RefWave {
        RefWave { src: 0, color: 0, label: String::new(), t0, dt, v: v.into(), vpd: 1.0, offset: 0.0, on: true }
    }
}

#[derive(PartialEq)]
enum Kind {
    Choice,
    Value,
    Action,
}

pub enum Ev {
    Key(u8),
    Touch { id: i32, phase: Phase, x: i32, y: i32 },
    Wheel { x: i32, y: i32, steps: i32, ctrl: bool, shift: bool, alt: bool },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Down,
    Move,
    Up,
}

#[derive(Clone, Copy, PartialEq)]
enum Drag {
    None,
    Pending,
    /// went down in an edge strip of the plot with more than one view: a horizontal swipe switches views, a vertical
    /// drag becomes `Pending` (ui/views.rs)
    ViewEdge,
    /// drawing zone k (box: from the touch-down point to the finger; shape: the finger's path)
    Zone(usize),
    /// draw-a-box is armed: draw repeated zoom boxes until a tap/menu cancels
    ZoomBox,
    Offset(usize),
    RefOffset(usize),
    /// the vertical position of math channel k (its lane was dragged up or down)
    MathOffset(usize),
    Delay,
    Overview,
    /// a new zoom box drawn on the empty overview (Studio)
    ZoomNew,
    ZoomLeft,
    ZoomRight,
    ZoomVertical,
    Level,
    Level2,
    CursorA,
    CursorB,
    CursorC,
    CursorD,
    /// the bus, by its vertical position
    Bus,
    /// a list: by its content or its scroll bar (ui/scroll.rs)
    Scroll(scroll::List, scroll::Grip),
    /// a divider between two columns of the event table (ui/tables.rs)
    TableCol(usize),
}

struct TouchState {
    down: bool,
    x0: i32,
    y0: i32,
    x: i32,
    y: i32,
    /// a press-and-hold already acted (the Zoom button's popover, a chip's menu); the lift is not a tap
    held: bool,
    drag: Drag,
    start_offset: f32,
    start_delay: f32,
    start_level: f32,
    start_cur: f32,
    /// the waveform window the finger went down in
    win: usize,
}

/// What the power dialog asked for; the main loop carries it out (panel MCU frames, docs/protocols/board.md 4/8.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowerAction {
    Off,
    Restart,
}

impl Ui {
    /// Power off or restart, as the buttons of the power dialog ask (SCPI `:SYSTem:POWer:OFF|RESTart CONFIRM`): the main loop
    /// carries it out. False while an update job runs: a cut now leaves recovery half-written (docs/update.md).
    pub(crate) fn power_request(&mut self, action: PowerAction) -> bool {
        if self.update_busy() {
            self.toast("Update running: wait until it is done");
            return false;
        }
        self.power.action = Some(action);
        self.toast(match action { PowerAction::Off => "Powering off ...", PowerAction::Restart => "Restarting ..." });
        true
    }

    /// How many stored frames can be shown. Frame 1 is the oldest.
    pub(crate) fn history_len(&self) -> usize {
        self.history.recs.len()
    }

    /// The frame on screen, 1-based. Live is the newest stored frame.
    pub(crate) fn history_frame(&self) -> usize {
        self.history.pos.map(|i| i + 1).unwrap_or(self.history.recs.len())
    }

    /// Shows frame `frame` (1-based) and stops. Frame 1 is the oldest.
    pub(crate) fn history_show(&mut self, frame: usize) -> bool {
        let n = self.history.recs.len();
        if frame == 0 || frame > n {
            return false;
        }
        self.m.run = RunState::Stop;
        let i = frame - 1;
        self.history.pos = Some(i);
        self.last[0] = Some(self.history.recs[i].clone());
        self.measure_last();
        self.toast(&format!("History {}/{}", frame, n));
        true
    }

}

/// Samples of one channel for the measurements in the background: a window of the hardware frame (shared, not
/// copied) or an own copy of the screen record.
enum MeasSrc {
    Frame(std::sync::Arc<crate::board::Frame>, usize, usize, usize),
    Own(Vec<f32>),
}

impl MeasSrc {
    fn samples(&self) -> &[f32] {
        match self {
            MeasSrc::Frame(f, c, i0, i1) => &f.ch[*c][*i0..*i1],
            MeasSrc::Own(v) => v,
        }
    }
}

/// One record's measurements: (channel, samples, dt, the smallest swing that counts as a signal, min/max pairs).
type MeasJob = (u64, Vec<(usize, MeasSrc, f64, f64, bool)>);
type MeasWork<'a> = (usize, &'a [f32], f64, f64, bool);
type MeasDone = (u64, Vec<(usize, Stats, oos_dsp::measure::Analysis)>);

/// The frame holds every sample of its acquisition: not min/max pairs, not a part read for a view.
fn whole_frame(f: &crate::board::Frame) -> bool {
    !f.envelope && f.acq.1 > 0 && f.ch.iter().any(|v| v.len() >= f.acq.1)
}

/// The measurements of a record, each channel on its own thread (the four channels of 100 kpts took 27 ms in a row
/// on the unit); min/max pairs in time on the mean of neighbours (#118).
fn measure_channels(work: &[MeasWork]) -> Vec<(usize, Stats, oos_dsp::measure::Analysis)> {
    let one = |&(c, v, dt, min, p): &MeasWork| {
        let a = if p {
            oos_dsp::measure::analyze_pairs(v, dt, min, 0.1, 0.9)
        } else {
            oos_dsp::measure::analyze_min(v, dt, min)
        };
        (c, stats(v, dt), a)
    };
    if work.len() < 2 {
        return work.iter().map(one).collect();
    }
    std::thread::scope(|s| {
        let hs: Vec<_> = work.iter().map(|w| s.spawn(move || one(w))).collect();
        hs.into_iter().map(|h| h.join().expect("measurement thread")).collect()
    })
}

/// The measurements in the background, on the unit (`Ui::measure_in_background`): a thread on the slower cores
/// measures the newest record while the frame loop goes on, and its results are taken a frame or two later. A record
/// that comes while one is measured waits; a newer one replaces it. Without it (tests, scripts, the host) the
/// measurements are made in the frame they belong to.
struct MeasWorker {
    tx: std::sync::mpsc::Sender<MeasJob>,
    rx: std::sync::mpsc::Receiver<MeasDone>,
    /// a job is out
    busy: bool,
}

impl MeasWorker {
    fn start() -> Option<MeasWorker> {
        let (tx, jobs) = std::sync::mpsc::channel::<MeasJob>();
        let (done, rx) = std::sync::mpsc::channel::<MeasDone>();
        std::thread::Builder::new()
            .name("measure".into())
            .spawn(move || {
                crate::placement::to_little_cores();
                while let Ok((n, work)) = jobs.recv() {
                    let w: Vec<MeasWork> =
                        work.iter().map(|(c, v, dt, min, p)| (*c, v.samples(), *dt, *min, *p)).collect();
                    if done.send((n, measure_channels(&w))).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(MeasWorker { tx, rx, busy: false })
    }
}

/// The screen: the state of each part, and what the whole screen shares (ADR 0062). A part keeps its state in a struct
/// of its module and has one field here (`bt: bt::BtUi`, `cat: meascatalog::Catalog`, ...). What stays a field of its
/// own here, and why:
/// - the model `m` and the source `src`: every part reads the settings and the records;
/// - the scope's own settings `sys` and their file `sys_path`: most pages, the main loop, the remote commands and
///   the command file read them, and every page that changes one writes the file (`sys_save`);
/// - the screen `scr` and the time `now`: every part draws on the one and counts by the other;
/// - the open menu (`menu`, `menu_open`, `parent`, `menu_history`, `pick`, `focus`): the shared menu code runs every
///   page from it (ADR 0060);
/// - the finger (`touch`), the message (`toast`) and the dialog over all (`dialog`): one of each for the whole screen.
///
/// The other loose fields are still to move into their parts; `tools/code-metrics.py --list` names them (`ui-fields`,
/// which only goes down).
pub struct Ui {
    cat: meascatalog::Catalog,
    /// the page of the measurement readouts shown and of Measure > Selected (measurement_nav.rs)
    meas_pages: measurement_nav::Pages,
    workflow_hold: Option<workflow::LiveSession>,
    session_is_sim: bool,
    replay: workflow::Replay,
    workflow_epoch: u64,
    bus_candidate: bus_candidate::State,
    pub open_wave_request: bool,
    load_slot_ask: bool,
    load_destination: Option<usize>,
    pub m: Model,
    pub src: Source,
    pub ph: Phosphor,
    pub phz: Phosphor,
    /// the zoom window: its vertical magnification, the time base it opened on, how it was closed (plot_layout.rs)
    zoom_view: plot_layout::ZoomView,
    /// The last Measure > Copy meas. to: from, to, how many (the row shows the result instead of "Pick").
    meas_copied: Option<(WaveId, WaveId, usize)>,
    /// the plot's rectangles, its waveform windows, the one being drawn, the pass of the second picture (plot_layout.rs)
    plot: plot_layout::Plot,
    /// Full density on the panel, and whether the expand button asked for it (windows.rs)
    full: windows::Full,
    /// The time view (zoomed or not, s/div, delay) the time cursors were last placed in (`cursor_follow_view`).
    cursor_view: Option<(bool, f32, f32)>,
    rng: Rng,
    trace_bus: std::cell::RefCell<bus::Cache>,
    menu: MenuId,
    menu_open: bool,
    /// menu that opened the current one (Back button)
    parent: Option<MenuId>,
    /// Previous menu locations, so nested Back preserves the complete route.
    menu_history: Vec<(MenuId, usize)>,
    /// the open picker (menu == MenuId::Pick)
    pick: Option<Pick>,
    focus: usize,
    /// the sources the measurements are added for: A, and B of delay and phase (analysis/measure.rs)
    sources: analysis::Sources,
    last: [Option<Record>; 1],
    /// history playback: the last acquired records and the one viewed (records.rs)
    history: records::History,
    /// mark all trigger events in the record (search)
    pub(crate) search_on: bool,
    /// the measurements (oos-dsp) of the last record per channel and of each ref slot (analysis/measure.rs)
    measured: analysis::Measured,
    /// the measurements in the background (records.rs)
    meas_bg: records::MeasBackground,
    /// what choosing the Bus trigger switched on by itself (`set_trig_type`): the channels and whether the decoder; leaving
    /// Bus switches exactly those off again, what the user had on stays
    bus_auto: Option<([bool; NCH], bool)>,
    /// STOP: the request for the hardware's final frame, and the records shown at the stop (docs/fork/stop-exact.md)
    stop: acquire::Stop,
    toast: Option<(String, f64)>,
    pub(crate) dialog: Option<Vec<String>>,
    /// System info page (sysinfo.rs)
    sysinfo: sysinfo::Page,
    touch: TouchState,
    /// what the input keeps between two events: keys and fingers held, knob times, the zone path, the pinch (input.rs)
    input: input::State,
    /// the zoom box: armed, and the one being drawn (plot_layout.rs)
    zoom_box: plot_layout::ZoomBox,
    /// the zone trigger: the zone the menu edits, the drawing armed, the records dropped, the zone unit (hwzone.rs)
    zone_trig: hwzone::ZoneTrig,
    /// the fast path for the queued hardware frames: its trigger statistics, its share of a screen and its times
    pub(crate) fast: acquire::FastPath,
    /// segmented recording, in software and in the hardware (records.rs)
    seg: records::Segments,
    /// XY cursors: the multipurpose knobs move the level pair (C, D), not the time pair (`cursor_knob`)
    cur_knob_level: bool,
    /// the record the software zone filter refused last, as text for `:SYSTem:DEBug:ZONE?`
    pub(crate) zone_last: String,
    /// STOP: the record shown was loaded from a session, with its samples. What the hardware has in its memory is
    /// another acquisition, so the STOP does not read that again in its place (`stop_replay`); RUN ends it
    loaded_record: bool,
    /// analysers (analysis.rs)
    azr: analysis::State,
    /// self-calibration page (selfcal.rs)
    cal: selfcal::State,
    /// Bode sweep (bode.rs)
    bode: bode::State,
    /// the external generator and its connection (extgen.rs)
    ext: extgen::ExtUi,
    pub(crate) workspace: workspace::Workspace,
    studio: studio::Studio,
    sheet: sheet::SheetState,
    /// SIM: Play is moving the timebase window through the selected record
    sim_play: bool,
    /// SIM mode playing offline refs (file or Add to R<n>) as live acquisitions: the slots in play.
    sim_refs: Vec<usize>,
    /// SIM: the generator thread and the error it left for SCPI (sim.rs)
    sim_gen: sim::Generator,
    /// Offline mode: whether it is on, the reference on its card, the live settings it puts back (offline.rs)
    offline: offline::Mode,
    /// Take over, offline waveforms, or the live scope.
    look: reflook::RefLook,
    /// the settings of each added measurement, and the row whose settings dialog is open (reflook.rs)
    meas_own: reflook::OwnMeas,
    /// The same of the views that are not shown (`ui/views.rs`).
    view_own: [std::collections::HashMap<(Meas, WaveId), reflook::MeasOwn>; VIEWS_MAX],
    /// The gate a row measures with while its key or samples are made (None: the shared one).
    row_gate: std::cell::Cell<Option<bool>>,
    /// the external display: whether one is attached and its size (windows.rs)
    pub hdmi: windows::Hdmi,
    /// `render_waveform` is drawing the monitor's waveform area (Display > HDMI = Waveform): the frame has no bottom
    /// bar, and the drawing stops before the bars and menus
    wave_pass: bool,
    /// the start-up screen and the last start-up's result for About (startup.rs)
    start_up: startup::StartUp,
    /// signed updates (update.rs): running oos-update job, watcher notice
    upd: update::UpdateUi,
    wlan: wlan::WlanUi,
    /// what the second pane keeps between frames
    pn: panes::PaneState,
    log: logger::Logger,
    single_armed: bool,
    /// the statistics panel of the measurements: shown, folded to its header (strip.rs)
    results: strip::Results,
    /// Eye page: its right column, sliding, and the plot that fills the chart area (eyepage.rs)
    eye_col: eyepage::Column,
    /// Clear pressed: the screen stays empty (also in STOP) until the next record
    cleared: bool,
    refs: [Option<RefWave>; NREF],
    /// short texts pinned to a point of a signal (`callout.rs`)
    callouts: Vec<callout::Callout>,
    cnt: tools::Counter,
    dvm: tools::Dvm,
    /// the Reference page: the slot its rows work on, the channel Save copies, the files the Import picker shows
    /// (tools/refs.rs)
    ref_page: tools::RefPage,
    /// Storage, a file by its name (ui/files.rs): the question for the dialog of the desktop build
    file_request: Option<FileAsk>,
    /// the files the list of a Storage row is showing, and what they are
    file_list: Vec<std::path::PathBuf>,
    file_kind: FileKind,
    /// the file the keyboard said is there already: OK once more replaces it
    file_replace: Option<std::path::PathBuf>,
    /// the files of commands Storage > Run SCPI file offers on the scope, the newest first
    scpi_files: Vec<std::path::PathBuf>,
    /// Storage > Run SCPI file: the file to run, and the stop of the one that runs. The interpreter takes them at
    /// its next poll (`Scpi::replay_tick`) and says here whether a file runs.
    pub scpi_replay_file: Option<std::path::PathBuf>,
    pub scpi_replay_cancel: bool,
    pub scpi_replaying: bool,
    /// the desktop build asks for the file with its Open dialog (desktop.rs)
    pub open_scpi_request: bool,
    xy: bool,
    /// the XY plot's X and Y channels (the Add panel sets them; the default is CH1 against CH2)
    xy_src: [usize; 2],
    pub now: f64,
    /// what the step saw last: the trigger level and its change, the last record and the usual interval (acquire.rs)
    seen: acquire::Seen,
    /// the counts of the screen and the acquisition: frames drawn, records taken in, their rates (acquire.rs)
    pub counts: acquire::Counts,
    /// the auto set-up under way on the hardware, the bar of views after one, the last `:DEBug:SLOTs?` (autoset.rs)
    auto: autoset::Auto,
    /// the jobs a remote command can wait for: their numbers and the receipts of the finished ones (scpi_operations.rs)
    ops: scpi_operations::Ledger,
    /// the network as the main loop last read it: the addresses, the LAN link, the WLAN stick (wlan.rs)
    pub net: wlan::Network,
    /// a mouse and a keyboard (USB or Bluetooth): the pointer and the modifier keys (ui/pointer.rs)
    pub mouse: pointer::Mouse,
    /// the desktop build: what its loopback services say (desktop/lan.rs), shown in Network > Ports
    pub lan_status: String,
    #[cfg_attr(target_os = "linux", allow(dead_code))] // used by the desktop build and the tests
    pub lan_web_port: Option<u16>,
    /// System > Network > Bluetooth (ui/bt.rs), and the stick and its inputs for the top bar
    bt: bt::BtUi,
    /// System > Overclock: limits.conf and the ADC clock of the next start (overclock.rs)
    pub overclock: overclock::Next,
    /// settings of the scope itself (sysconf.rs); written to `sys_path` (none in tests: nothing written)
    pub sys: crate::sysconf::SysConf,
    pub sys_path: Option<std::path::PathBuf>,
    /// NTP worker and schedule (ui/time.rs)
    ntp: time::NtpUi,
    ssh: ssh::SshUi,
    /// System > Network > Remote services > Allowed (ui/remote.rs)
    remote: remote::RemoteUi,
    /// the system clock was set in this boot (NTP or by hand): file names carry the date
    pub clock_known: bool,
    pub(crate) times: acquire::FrameTimes,
    pub info: String,
    pub dirty_map: bool,
    /// the counts the traces are graded against (render.rs)
    grade: render::Grade,
    /// where files are stored: the folder, the destination, the setup slot (save_recall.rs)
    pub files: save_recall::Files,
    /// the math channels: the menu's category, the results and their worker, the scales to refit (mathwave.rs)
    math: mathwave::Math,
    capture_views: std::cell::RefCell<capture_memory::State>,
    /// the screen the layout is made for: the panel, another size in host test runs (`--size`), or the window of
    /// the desktop build (`set_screen`)
    pub scr: Screen,
    /// the USB stick: whether one is mounted, and when the sticks are looked for next (save_recall.rs)
    stick: save_recall::Stick,
    /// the logic analyser: the probe as last read, the pixel columns of the last LA record drawn (la.rs)
    la: la::Lanes,
    /// the mask test in software: the reference waveform, the mask built from it, its counts (render_derived.rs)
    mask_sw: render_derived::MaskTest,
    /// the mask unit in the FPGA: the mask in its columns, its latest counters, the number that clears them, and why
    /// the acquisition thread dropped the unit (hwmask.rs)
    mask_unit: hwmask::Unit,
    /// what the screen asks of the main loop: a screenshot, the end, commands for the front panel (input.rs)
    pub asks: input::Asks,
    /// Storage > Save setup as SCPI: the file to write, under the name the user gave it (ui/files.rs). The interpreter
    /// writes it at its next poll, since the lines are the answers of its own queries (`Scpi::poll`,
    /// scpi/setupscpi.rs).
    pub setup_scpi_request: Option<std::path::PathBuf>,
    /// Storage > SCPI recorder, `:SYSTem:RECorder:STATe`: on or off. The interpreter follows at its next poll.
    pub scpi_rec_want: bool,
    /// What the SCPI recorder writes as a command and not as settings (scpi/recorder.rs): a key or a row that leaves
    /// no setting or owns the settings it changes, each with the model from before it acted. The interpreter takes
    /// them at its next poll. Filled only while the recorder is on.
    pub scpi_rec_actions: Vec<(String, Box<Model>)>,
    /// the power key's dialog and what it asked the main loop for (input.rs)
    pub power: input::Power,
    /// hex entry of the keypad picker (protocol trigger value)
    keypad: String,
    /// the numeric entry pad of the Studio sheets (ui/numpad.rs); its text is `keypad`
    numpad: Option<numpad::NumPad>,
    /// the number pad of a value row in Default, upstream's (ui/numpad.rs); its text is `keypad`
    row_pad: numpad::RowPad,
    /// the value a ladder stepper started from when it was between two rungs: (item, that value, the rung it went to)
    ladder_home: Option<(Item, f64, f64)>,
    /// the last press of an `Add` stepper that ran into a limit: (item, the limit, the value it came from)
    clamp_home: Option<(Item, f64, f64)>,
    /// The source the cursors measure when it was picked in the cursor menu: only the cursors move to it (the active
    /// channel and the measurement source stay). Selecting a trace anywhere else lets the cursors follow it again.
    cursor_pin: Option<WaveId>,
    /// the Eye source was chosen on purpose (its sheet, SIM, a loaded record): Auto keeps it instead of the selected lane
    eye_src_chosen: bool,
    /// Channels a source row turned on by itself (`feedback.rs`): turned off again when the row moves on, unless used.
    auto_on: Vec<usize>,
    /// the decoded lines of the records shown, analog and LA, with times in s (render_derived.rs)
    decoded: render_derived::Decoded,
    /// the table window over the plot: what it shows and its first row shown (tables.rs)
    table: tables::Window,
    /// the boxes of the decoder bands drawn in this frame: where a finger finds one, its decoder, its start time
    band_hits: std::cell::RefCell<Vec<(Rect, usize, f64)>>,
    /// where the second and the third column of the event table start, from its left edge, once a divider was dragged
    table_cols: Option<[i32; 2]>,
}

/// One line of the measurement strip when it is docked on the left or the right.
struct StripLine {
    pill: Option<(String, Rgb)>,
    name: String,
    /// the name where the long one does not fit (Positive Duty Cycle, +Duty); the same where there is no short one
    short: String,
    value: String,
    col: Rgb,
    /// the name lines up with the names under a channel pill
    align: bool,
}

impl Ui {
    /// The fonts of the text size.
    pub fn fonts(&self) -> &'static Fonts {
        Fonts::get(self.m.text_size.index())
    }

    /// The layout sizes of the text size.
    pub fn lay(&self) -> &'static Lay {
        &LAYS[self.m.text_size.index()]
    }

    /// Called after remote (SCPI) changes: same effect as a change on the screen.
    pub fn mark_changed(&mut self) {
        self.settings_changed();
    }

    /// After `:RUN`, `:STOP` or `:SINGle`: as `mark_changed`, but the measurement statistics stay (on a unit,
    /// 2026-11-20, `:STOP` dropped a count of 104 to 1; the Run/Stop key keeps them).
    pub fn mark_run_state(&mut self) {
        self.acquiring_changed();
    }

    pub fn toast(&mut self, s: &str) {
        self.toast = Some((s.to_string(), self.now + 2.5));
    }

    /// Clear (CLEAR key, Display > Clear, Quick key, SCPI :CLEar): empties the screen like a scope does. The
    /// waveforms, persistence / colour-grade maps, averaging, measurements and their statistics, mask counts and
    /// search marks start over; in STOP the screen stays empty until the next acquisition (RUN draws again at once).
    fn clear_display(&mut self) {
        self.rec_action(":CLEar");
        self.counts.wfm_total = 0;
        self.bus_views_clear();
        self.ph.clear();
        self.phz.clear();
        (self.grade.vmax, self.grade.vmaxz, self.grade.vmax_cg, self.grade.vmaxz_cg) = ([1; NCH], [1; NCH], 1, 1);
        // without a last record there is nothing to redraw in STOP, average with, compute math / decode / search on
        self.last[0] = None;
        self.history.pos = None;
        self.cleared = true;
        self.measured.stats = [Stats::default(); NCH];
        // a measurement still out, or a record waiting for the worker, belongs to what was cleared away
        (self.meas_bg.n, self.meas_bg.want, self.meas_bg.pending) = (self.meas_bg.n + 1, false, None);
        self.measured.ch = Default::default();
        self.clear_measurement_statistics();
        self.mask_counts_reset();
        (self.zone_trig.drops, self.zone_trig.clear) = (0, self.zone_trig.clear + 1);
        self.hw_avg_restart();
        self.toast("Display cleared");
    }

    /// The hardware average starts over (`Model::hw_avg_clear`); nothing changes for the other sources.
    fn hw_avg_restart(&mut self) {
        if self.src.is_hw() {
            self.m.hw_avg_clear += 1;
        }
    }

    /// QUICK key and the Quick button of the top bar: the action chosen in System > Quick key.
    fn quick(&mut self) {
        match self.m.quick {
            Quick::Screenshot => { self.rec_action(":SAVE:IMAGe"); self.asks.shot = Some(String::new()) }
            Quick::SavePng => self.file_save_auto(FileKind::Shot),
            Quick::SaveSetup => self.activate(Item::StoreSaveSetup),
            Quick::SaveCsv => self.file_save_auto(FileKind::WaveCsv),
            Quick::Autoset => self.key(key::AUTO),
            Quick::Single => self.key(key::SINGLE),
            Quick::RunStop => self.key(key::RUNSTOP),
            Quick::Clear => self.clear_display(),
            Quick::NextView => {
                if self.m.views < 2 {
                    return self.toast("Set Display > Views first");
                }
                self.view_step(1)
            }
        }
    }

    /// The text of the message on the screen, for the tests of other modules.
    #[cfg(test)]
    pub(crate) fn toast_text(&self) -> String {
        self.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default()
    }

    /// What the level of the edge trigger counts from: 0 V, or with AC or LF reject coupling the mean of the source's
    /// signal. The trigger then sees the signal without its DC part (`tpu::edge_coupled` leaves the channel's offset
    /// out of the level), so a level of 0 V is at the signal's mean and a level that was right with DC coupling is
    /// off by that mean: on a unit a line between 0 and 0.55 V stopped triggering at 0.255 V when the coupling went
    /// to AC and triggered again from 0.1 V down (2026-11-18). The mean is the one of the record on the screen,
    /// which is assumed to be near what the trigger's filter settles on.
    pub(crate) fn trig_level_base(&self) -> f32 {
        let s = self.measured.stats[self.m.trig_src.min(NCH - 1)];
        if matches!(self.m.trig_coupling(), TrigCoup::Ac | TrigCoup::LfReject) && s.valid && s.vavg.is_finite() { s.vavg } else { 0.0 }
    }

    fn level_50(&mut self) {
        if self.m.trig_ref.is_some() {
            self.fit_reference_trigger_levels();
            self.settings_changed();
            return;
        }
        let s = self.measured.stats[self.m.trig_src];
        if s.valid {
            // with AC or LF-reject coupling the trigger compares the source without its mean, and its level counts from
            // there: halfway through the signal is the middle less the mean. The middle itself sat above the whole
            // swing of a signal that does not cross 0 V (0 to 3.3 V: 1.65 V over a mean of 1.65 V), and the trigger
            // did not fire (#279)
            let src = self.m.trig_src.min(NCH - 1);
            let mean = self.last[0].as_ref().map_or(0.0, |r| match r.raw.as_ref() {
                Some(raw) => crate::wave::trig_mean(&self.m, &raw.frame.ch[src]),
                None => crate::wave::trig_mean(&self.m, &r.ch[src]),
            });
            self.m.trig_level = crate::wave::mid_level(&s) - mean;
            self.settings_changed();
        }
    }
}

#[derive(Clone)]
enum Ctl {
    /// segmented buttons: options, selected
    Seg(Vec<String>, Option<usize>),
    /// an on/off setting: one field with its state and a slide switch, a tap anywhere on it switches (ADR 0075)
    Switch(bool),
    /// - value +
    Value,
    /// - value +, the value opens the list of all options (MenuId::Pick)
    Pick,
    Button,
    /// plain text: a state, nothing to press
    Text,
}

struct Row {
    it: Item,
    label: Rect,
    ctrl: Rect,
    ctl: Ctl,
}

/// The bottom row.
struct Cards {
    ch: [Rect; NCH],
    la: Option<Rect>,
    gen: Option<Rect>,
    /// the REF card of the Default bottom row: only while a ref is shown over the live channels
    refs: Option<Rect>,
    time: Rect,
    trig: Rect,
}

/// Width of the LA and generator cards, and the width a channel card keeps before they stay narrow.
const AUX_NARROW: i32 = 56;
const AUX_WIDE: i32 = 104;
const CH_MIN: i32 = 100;
/// Widest TIME and trigger cards: more room only spreads their text (the panel never gets that far, at most 234 / 324).
const TIME_CARD_MAX: i32 = 240;
const TRIG_CARD_MAX: i32 = 330;
/// Colour of the generator card.
const GEN_COLOR: Rgb = rgb(0xb3, 0x94, 0xf5);

/// Menu dialog layout.
struct Dlg {
    r: Rect,
    close: Rect,
    back: Option<Rect>,
    rows: Vec<Row>,
    /// group labels (start menu)
    heads: Vec<(Rect, &'static str)>,
    /// the scroll bar and the area of the rows, when the rows are more than fit (a long picker, ADR 0006)
    bar: Option<(scroll::Bar, Rect)>,
}

/// Temperature map of the colour-graded display, 256 steps: blue (rare) -> cyan -> green -> yellow -> red (frequent).
fn heat_palette() -> &'static [Rgb; 256] {
    static PAL: std::sync::OnceLock<[Rgb; 256]> = std::sync::OnceLock::new();
    PAL.get_or_init(|| {
        let mut p = [0; 256];
        for (i, e) in p.iter_mut().enumerate() {
            let t = i as f32 / 255.0 * 4.0;
            let k = (t as usize).min(3);
            let f = t - k as f32;
            let (a, b) = (HEAT_STOPS[k], HEAT_STOPS[k + 1]);
            let ch = |x: f32, y: f32| (x + (y - x) * f).round() as u32;
            *e = rgb(ch(a.0, b.0), ch(a.1, b.1), ch(a.2, b.2));
        }
        p
    })
}

/// Where a reference badge goes: the wanted height, or the nearest free slot (36 px apart) between `lo` and `hi`.
fn badge_spot(taken: &[i32], y: i32, lo: i32, hi: i32) -> i32 {
    (0..8)
        .flat_map(|k| [y + 36 * k, y - 36 * k])
        .map(|c| c.clamp(lo, hi))
        .find(|c| taken.iter().all(|t| (t - c).abs() >= 36))
        .unwrap_or(y.clamp(lo, hi))
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

/// The first of `fonts` in which `s` is at most `room` wide, else the last.
impl Ui {
    /// Font of a menu row that is only text (a long status line): the menu font, else smaller ones down to the small
    /// font of the normal text size.
    fn text_row_font(&self, room: i32, s: &str) -> &'static Font {
        let (f, n) = (self.fonts(), Fonts::get(0));
        fit_font(&[&f.m, &f.s, &n.m, &n.s], room, s)
    }
}

fn fit_font(fonts: &[&'static Font], room: i32, s: &str) -> &'static Font {
    fonts.iter().copied().find(|f| f.width(s) <= room).unwrap_or(fonts[fonts.len() - 1])
}

fn backed_text(cv: &mut Canvas, f: &Font, x: i32, y: i32, s: &str, col: Rgb) -> i32 {
    cv.blend(Rect::new(x - 3, y - f.size + 2, f.width(s) + 6, f.size + 2), BG, 180);
    cv.text(f, x, y, s, col)
}

/// Button: 2 px border, filled.
fn btn(cv: &mut Canvas, r: Rect, fill: Rgb, border: Rgb) {
    cv.framed(r, 5, 2, border, 4, fill);
}

fn chevron_down(cv: &mut Canvas, x: i32, y: i32, col: Rgb) {
    for d in 0..2 {
        cv.line(x, y + d, x + 4, y + 4 + d, col);
        cv.line(x + 4, y + 4 + d, x + 8, y + d, col);
    }
}

/// "180", "1.10k", "12.5k".
fn fmt_count(v: f64) -> String {
    if v >= 1e3 {
        let s = si_digits(v, "", 3);
        s.replace(' ', "")
    } else {
        format!("{:.0}", v)
    }
}

/// Text of one decoded field.
fn packet_text(m: &Model, p: &oos_dsp::decode::Packet) -> String {
    use oos_dsp::decode::{Kind, I2C_NAK_END};
    if let Some(t) = m.dec_proto.module().and_then(|x| x.text(p, &m.dec_opts())) {
        return t;
    }
    let hex = |v: u32| if m.dec_bits > 8 { format!("{:04X}", v) } else { format!("{:02X}", v) };
    // a character where the decoder shows characters: a word of one line that passed its check (an I2C byte that
    // was not acknowledged keeps its value and its mark, a MOSI/MISO pair its two values)
    // (the mark on the byte that ends an I2C read is no second value)
    let second = p.value2.filter(|_| m.dec_proto != Proto::I2C);
    let chars = p.kind == Kind::Data && second.is_none() && p.ok && decband::ascii(m);
    // Default keeps upstream's mark on the byte a controller ends an I2C read with: not acknowledged, and no error
    let nak_end = m.layout == Layout::Default && m.dec_proto == Proto::I2C && p.value2 == Some(I2C_NAK_END);
    match p.kind {
        Kind::Addr { read } => {
            format!("{} {}{}", hex(p.value), if read { "R" } else { "W" }, if p.ok { "" } else { " N" })
        }
        Kind::Id { ext, rtr } => format!(
            "ID {}{}{}",
            if ext {
                format!("{:08X}", p.value)
            } else if m.dec_proto == Proto::LIN {
                format!("{:02X}", p.value)
            } else {
                format!("{:03X}", p.value)
            },
            if rtr { " RTR" } else { "" },
            if p.ok { "" } else { " !P" }
        ),
        Kind::Sample { right } => {
            format!("{} {:0w$X}", if right { "R" } else { "L" }, p.value, w = (m.dec_bits as usize).div_ceil(4))
        }
        Kind::Word { cmd } => {
            format!("{} {:04X}{}", if cmd { "C/S" } else { "D" }, p.value, if p.ok { "" } else { " !P" })
        }
        // the frame CRC of FlexRay is 24 bits, its header CRC 11: all their digits, so a leading zero is no lost digit
        Kind::Crc if m.dec_proto.module().is_some_and(|x| x.key() == "FlexRay") => {
            format!("CRC {:06X}{}", p.value, if p.ok { "" } else { " !" })
        }
        Kind::Field("HCRC") => format!("HCRC {:03X}{}", p.value, if p.ok { "" } else { " !" }),
        Kind::Crc => format!(
            "{} {:X}{}",
            if m.dec_proto == Proto::LIN { "CS" } else { "CRC" },
            p.value,
            if p.ok { "" } else { " !" }
        ),
        Kind::Field(l) if matches!(l, "MAC" | "SRC") && p.value2.is_some() => {
            format!(
                "{} {}{}",
                if l == "SRC" { "SRC" } else { "MAC" },
                oos_dsp::linecode::mac_text(p.value, p.value2.unwrap_or(0)),
                if p.ok { "" } else { " !" }
            )
        }
        Kind::Field(l) if matches!(l, "IP" | "DIP") => format!(
            "{} {}{}",
            if l == "DIP" { "DIP" } else { "IP" },
            oos_dsp::linecode::ip_text(p.value),
            if p.ok { "" } else { " !" }
        ),
        Kind::Field(l) => match p.value2 {
            Some(v2) if matches!(l, "DST" | "SRC" | "SER" | "PID") => {
                format!("{} {:04X}{:08X}{}", l, v2, p.value, if p.ok { "" } else { " !" })
            }
            _ => format!("{} {:X}{}", l, p.value, if p.ok { "" } else { " !" }),
        },
        Kind::Ack => format!("ACK {}", p.value),
        _ if nak_end => format!("{} N", hex(p.value)),
        _ if chars && p.value == 0x20 => "SP".to_string(),
        _ if chars && (0x21..0x7f).contains(&p.value) => format!("{}", p.value as u8 as char),
        _ => match second {
            Some(v2) => format!("{}/{}", hex(p.value), hex(v2)),
            None if m.dec_proto == Proto::I2C && !p.ok => format!("{} N", hex(p.value)),
            None => hex(p.value),
        },
    }
}

/// The short tag of an event (start, stop, acknowledge ...), None for a field with a value.
fn packet_tag(m: &Model, p: &oos_dsp::decode::Packet) -> Option<&'static str> {
    use oos_dsp::decode::Kind;
    // a field a protocol module names itself is no tag (the ACK of SWD has a value)
    if m.dec_proto.module().is_some_and(|x| x.text(p, &m.dec_opts()).is_some()) {
        return None;
    }
    let event = matches!(p.kind, Kind::Field(l) if l.starts_with('!'));
    if !(event || matches!(p.kind, Kind::Start | Kind::RepStart | Kind::Stop | Kind::Break | Kind::Ack)) {
        return None;
    }
    Some(match p.kind {
        Kind::Field(l) => &l[1..],
        // (CAN and CAN-FD alike: the start of a frame is the same bit on both)
        Kind::Start if m.dec_proto.module().is_some_and(|x| matches!(x.key(), "Can" | "CanFd")) => "SOF",
        Kind::Start => "S",
        Kind::RepStart => "Sr",
        Kind::Break => "BRK",
        Kind::Ack if p.ok => "ACK",
        Kind::Ack => "NAK",
        _ => "P",
    })
}

/// The largest 1-2-5 step that is not above `x`.
fn nice_step_below(x: f64) -> f64 {
    if !(x.is_finite() && x > 0.0) { return 1.0; }
    let base = 10f64.powf(x.log10().floor());
    [5.0, 2.0, 1.0].into_iter().map(|m| m * base).find(|&s| s <= x * 1.000001).unwrap_or(base)
}

/// The labelled times of the time axis, as (whole steps from the trigger, seconds): the window is `delay` +- 5 divisions
/// of `tdiv`; a step is `every` divisions. Whole steps, so a window put anywhere has round labels.
#[cfg_attr(not(test), allow(dead_code))] // only the tests use it
fn studio_time_ticks(tdiv: f32, delay: f32, every: i32) -> Vec<(i64, f64)> {
    studio_time_ticks_at(tdiv, delay, tdiv as f64 * every as f64)
}

/// `studio_time_ticks` with the step given (a lane whose time/div is not on the ladder labels round steps).
fn studio_time_ticks_at(tdiv: f32, delay: f32, step: f64) -> Vec<(i64, f64)> {
    let half = tdiv as f64 * HDIV as f64 / 2.0;
    let (left, right) = (delay as f64 - half, delay as f64 + half);
    let first = (left / step - 1e-6).ceil() as i64;
    (first..).map(|k| (k, k as f64 * step)).take_while(|&(_, t)| t <= right + step * 1e-6).collect()
}

/// Axis label with the SI prefix of the axis `span` and as many decimals as `step` and `base` (offset / delay)
/// need: "-1.0 µs", "500 mV", "3.25 V".
fn axis_label(v: f32, step: f32, base: f32, span: f32, unit: &str) -> String {
    axis_label64(v as f64, step, base, span, unit)
}

/// `axis_label` for a time that does not fit an f32 (a deep zoom far from the trigger).
fn axis_label64(v: f64, step: f32, base: f32, span: f32, unit: &str) -> String {
    // up to giga: an axis does not go to tera
    let (mult, p) = si_prefix(span.abs().max(step.abs()) as f64, 1);
    let st = (step as f64 / mult).abs();
    // decimals that show `x` (a step or the base): a whole number of the last digit, not 0 (a step of 0.001 ms is not
    // "0"), at most `max`: 6 for the step (a delay of seconds at a time base of microseconds), 3 for the base (an
    // offset dragged to 1.454545 V shows as 1.455)
    let needed = |x: f64, max: i32| {
        if x < 1e-12 {
            return 0;
        }
        let mut dec = 0;
        while dec < max {
            let k = x * 10f64.powi(dec);
            if k.round() >= 1.0 && (k.round() - k).abs() < 1e-3 {
                break;
            }
            dec += 1;
        }
        dec
    };
    let dec = needed(st, 9).max(needed((base as f64 / mult).abs(), 3));
    // a step far below one unit of the prefix (a deep zoom) needs digits down to its own size
    let dec = if st > 0.0 && st < 1.0 { dec.max((-st.log10() - 1e-6).ceil().min(9.0) as i32) } else { dec };
    let x = v / mult;
    let x = if x.abs() < st * 1e-3 { 0.0 } else { x };
    format!("{:.*} {}{}", dec as usize, x, p, unit)
}

/// Small chevron pointing left (dir -1) or right (dir 1), centred at (x, y).
fn chevron(cv: &mut Canvas, x: i32, y: i32, dir: i32, col: Rgb) {
    for d in 0..2 {
        cv.line(x - 2 * dir + d, y - 5, x + 3 * dir + d, y, col);
        cv.line(x + 3 * dir + d, y, x - 2 * dir + d, y + 5, col);
    }
}

#[cfg(test)]
mod acquisition_tests;
#[cfg(test)]
mod menu_tests;
#[cfg(test)]
mod stop_tests;
#[cfg(test)]
mod tests;

/// Menu text of a hardware segment interval.
fn seg_ivl_name(x: f32) -> String {
    if x <= 0.0 {
        "None".into()
    } else {
        si(x as f64, "s")
    }
}

#[cfg(test)]
mod aux_out_tests {
    use super::*;

    #[test]
    fn aux_out_in_the_system_menu_with_the_offered_choices() {
        let mut ui = crate::ui::testkit::demo_ui();
        assert!(ui.items(MenuId::Utility).contains(&Item::AuxOut));
        let want: &[&str] = if board().mask_hw() || board().zone_hw() {
            &["Trigger out", "Off", "Pass/fail"]
        } else {
            &["Trigger out", "Off"]
        };
        assert_eq!(ui.options(Item::AuxOut), want);
        assert_eq!(ui.value(Item::AuxOut), "Trigger out");
        ui.select_option(Item::AuxOut, "Off");
        assert_eq!((ui.m.aux_out, ui.value(Item::AuxOut).as_str()), (AuxOut::Off, "Off"));
        ui.select_option(Item::AuxOut, "Trigger out");
        assert_eq!(ui.m.aux_out, AuxOut::TrigOut);
        // pass/fail where the mask test runs in hardware
        assert_eq!(AuxOut::choices(true), AUX_OUTS);
        assert_eq!(AuxOut::choices(false), [AuxOut::TrigOut, AuxOut::Off]);
    }
}

#[cfg(test)]
mod zoom_tests;
#[cfg(test)]
mod funccheck_tests;
