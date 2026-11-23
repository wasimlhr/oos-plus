// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Display settings: histogram, the QUICK key, the external monitor, text size, cursors, waveform windows, density,
//! grid, the measurement strip and persistence. A new display setting goes here.

/// Waveform histogram: off, vertical (voltage distribution of the whole screen), horizontal (time distribution in a
/// band of +-0.5 div around the trigger level).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hist {
    Off,
    Vertical,
    Horizontal,
}

/// What the QUICK key (and the Quick button of the top bar) does; set in Utility > Quick key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quick {
    /// PPM of the screen in /tmp (the behaviour before the setting existed)
    Screenshot,
    /// PNG in the storage directory (Storage > Save screenshot)
    SavePng,
    SaveSetup,
    SaveCsv,
    Autoset,
    Single,
    RunStop,
    Clear,
    /// the next of Display > Views (ui/views.rs)
    NextView,
}

pub const QUICKS: [Quick; 9] = [
    Quick::Screenshot,
    Quick::SavePng,
    Quick::SaveSetup,
    Quick::SaveCsv,
    Quick::Autoset,
    Quick::Single,
    Quick::RunStop,
    Quick::Clear,
    Quick::NextView,
];

impl Quick {
    /// Name in the Utility menu and its picker.
    pub fn name(self) -> &'static str {
        match self {
            Quick::Screenshot => "Screenshot",
            Quick::SavePng => "Save PNG",
            Quick::SaveSetup => "Save setup",
            Quick::SaveCsv => "Save CSV",
            Quick::Autoset => "Autoset",
            Quick::Single => "Single",
            Quick::RunStop => "Run/Stop",
            Quick::Clear => "Clear",
            Quick::NextView => "Next view",
        }
    }

    /// Label of the Quick button in the top bar.
    pub fn short(self) -> &'static str {
        match self {
            Quick::Screenshot => "Snap",
            Quick::SavePng => "PNG",
            Quick::SaveSetup => "Setup",
            Quick::SaveCsv => "CSV",
            Quick::Autoset => "Auto",
            Quick::Single => "Single",
            Quick::RunStop => "Run/Stop",
            Quick::Clear => "Clear",
            Quick::NextView => "View",
        }
    }
}

/// Display > Layout. Default: the stock OOS top bar and cards. Minimal: the compact OOS-plus bars. Studio: the compact
/// bars with a source chip per channel, stacked lanes and pinned measurement badges (`ui/studio.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layout {
    Default,
    Minimal,
    Studio,
}

pub const LAYOUTS: [Layout; 3] = [Layout::Default, Layout::Minimal, Layout::Studio];

/// What a row of the decoder results is (the Events pane).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum EvView {
    /// one row per message
    #[default]
    Messages,
    /// one row per field
    Fields,
    /// one row per line of a character stream (UART shown as ASCII); messages for every other decoder
    Lines,
}

pub const EV_VIEWS: [EvView; 3] = [EvView::Messages, EvView::Fields, EvView::Lines];

impl EvView {
    /// The name on the pane and in the setup file (lower case there).
    pub fn name(self) -> &'static str {
        match self {
            EvView::Messages => "Messages",
            EvView::Fields => "Fields",
            EvView::Lines => "Lines",
        }
    }

    /// The keyword of `:DECode:EVENts:VIEW`.
    pub fn scpi(self) -> &'static str {
        match self {
            EvView::Messages => "MESSages",
            EvView::Fields => "FIELds",
            EvView::Lines => "LINes",
        }
    }

    /// A setup value, `messages`, `fields` or `lines`.
    pub fn parse(s: &str) -> Option<EvView> {
        EV_VIEWS.into_iter().find(|v| v.name().eq_ignore_ascii_case(s))
    }
}

impl Layout {
    /// The name in the menu and in the setup file (lower case there).
    pub fn name(self) -> &'static str {
        match self {
            Layout::Default => "Default",
            Layout::Minimal => "Minimal",
            Layout::Studio => "Studio",
        }
    }

    /// A setup value, `default`, `minimal` or `studio`.
    pub fn parse(s: &str) -> Option<Layout> {
        LAYOUTS.into_iter().find(|l| l.name().eq_ignore_ascii_case(s))
    }
}

/// What an HDMI monitor shows. Mirror: the panel picture, scaled. Waveform: a settings line and the waveform area,
/// scaled. Layout: the whole screen laid out again at the monitor's size, sharp, with the traces stretched; it costs a
/// second render per monitor frame (about 5 to 10 ms on the unit at 1920 x 1080), so the panel may get fewer frames.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum HdmiMode {
    Mirror,
    #[default]
    Waveform,
    Layout,
}

pub const HDMI_MODES: [HdmiMode; 3] = [HdmiMode::Mirror, HdmiMode::Waveform, HdmiMode::Layout];

/// Size of the text on the screen (Display > Text size): Normal is the original look, Large and Larger use fonts 2 and
/// 4 px bigger with the rows, cards and dialogs grown to match. A preference: Default keeps it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TextSize {
    #[default]
    Normal,
    Large,
    Larger,
}

pub const TEXT_SIZES: [TextSize; 3] = [TextSize::Normal, TextSize::Large, TextSize::Larger];

impl TextSize {
    pub fn name(self) -> &'static str {
        match self {
            TextSize::Normal => "Normal",
            TextSize::Large => "Large",
            TextSize::Larger => "Larger",
        }
    }

    /// 0, 1, 2: the font set and the layout metrics.
    pub fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CursorMode {
    Off,
    Time,
    Volt,
    /// time cursors that read the active channel where they stand
    Track,
    /// time and voltage cursors at once
    Xy,
}

/// How the plot is split into waveform windows; every channel and math channel stands in one of them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum WinLayout {
    #[default]
    One,
    /// two side by side
    Cols,
    /// two one above the other
    Rows,
    /// two by two
    Quad,
}

impl WinLayout {
    pub const ALL: [WinLayout; 4] = [WinLayout::One, WinLayout::Cols, WinLayout::Rows, WinLayout::Quad];

    pub fn count(self) -> usize {
        match self {
            WinLayout::One => 1,
            WinLayout::Cols | WinLayout::Rows => 2,
            WinLayout::Quad => 4,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            WinLayout::One => "One",
            WinLayout::Cols => "Side by side",
            WinLayout::Rows => "Stacked",
            WinLayout::Quad => "Four",
        }
    }
}

/// How much of the 1024×600 picture the plot takes. The same four apply to the panel and to a scaled window of it,
/// and to the waveform on the external display.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Density {
    /// top bar, plot, measurement strip, cards
    #[default]
    Scope,
    /// plot and measurement strip
    Full,
    /// plot, and under it a pane of numbers or two stacked traces
    Split,
    /// four equal panes
    Four,
}

impl Density {
    pub const ALL: [Density; 4] = [Density::Scope, Density::Full, Density::Split, Density::Four];

    /// The name of the density, for the Windows page and messages.
    pub fn label(self) -> &'static str {
        match self {
            Density::Scope => "Scope",
            Density::Full => "Full",
            Density::Split => "Split",
            Density::Four => "Four",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Grid {
    Full,
    Light,
    None,
}

/// Where the measurement strip is drawn: below the graticule, on a side, above it, or not at all (Hidden). The one
/// setting behind Measure > Dock, Display > Windows > Strip and the card's context menu; the setup key is `strip`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StripDock {
    Below,
    Left,
    Right,
    Above,
    Hidden,
}

impl StripDock {
    pub const ALL: [StripDock; 5] = [StripDock::Below, StripDock::Left, StripDock::Right, StripDock::Above, StripDock::Hidden];

    pub fn label(self) -> &'static str {
        match self {
            StripDock::Below => "Below plot",
            StripDock::Left => "Left",
            StripDock::Right => "Right",
            StripDock::Above => "Above plot",
            StripDock::Hidden => "Hidden",
        }
    }
}

/// Persistence: name, seconds (0 = minimum, <0 = infinite).
pub const PERSIST: [(&str, f32); 7] =
    [("Min", 0.0), ("100 ms", 0.1), ("500 ms", 0.5), ("1 s", 1.0), ("5 s", 5.0), ("10 s", 10.0), ("Infinite", -1.0)];

