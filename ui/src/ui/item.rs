// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The menu items: `Item`, one variant per row a menu can show, and `PARTS`, the modules of ui/ that answer for the
//! items they own (kind, label, value, adjust, activate, options, text). A new item gets its variant here (appended:
//! the discriminants stay stable); a new module that owns items gets its `Part` in `PARTS`.

use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum Item {
    DecSel,
    DecTable,
    DecExport,
    SearchTable,
    SearchSave,
    StoreBin,
    PaneSel,
    FullScreen,
    StripPos,
    DecOn,
    DecSrc,
    DecBaud,
    DecCanRate,
    DecCanH,
    DecUsbSpeed,
    /// option row `k` of a protocol module
    DecOpt(usize),
    DecSrc4,
    DecDiff,
    TrigBusWhat,
    TrigBusVal,
    /// protocol trigger settings beyond condition and value (`SerialTrig`)
    SerRel,
    SerMax,
    SerMask,
    UartPol,
    UartStop,
    UartErr,
    SpiCs,
    SpiTimeout,
    I2cDir,
    I2cAddrBits,
    I2cBytes,
    DecThr,
    /// sets the threshold to the middle of the signal on the decoder's source, once
    DecThrMid,
    /// the threshold of the decoder's second, third or fourth line (1..=3), where it is not the decoder's
    DecThrLine(usize),
    DecFmt,
    /// the name of the decoder on its band, and where the band stands in its window (Minimal and Studio)
    DecLabel,
    DecPos,
    DecProto,
    DecSrc2,
    DecCs,
    DecMode,
    DecBits,
    DecParity,
    BusOn,
    BusLabel,
    BusPos,
    BusClk,
    BusClkThr,
    BusData,
    BusDataThr,
    BusRw,
    Zoom,
    AutoRoll,
    ZScale,
    ZDelay,
    MeasStats,
    /// statistics population and the columns shown
    MeasPop,
    MeasShowSd,
    MeasShowMin,
    MeasShowMax,
    MeasStatReset,
    MeasLabels,
    /// where the measurement strip sits
    StripDock,
    XyMode,
    RefSrc,
    RefSave,
    /// Studio: the same, into the next free slot (offered when the selected slot is full)
    RefSaveNext,
    /// Display sheet of Minimal and Studio: the traces on one graticule or a lane each (the plot's hold menu has both)
    Traces,
    /// Reference sheet: clears the slot that is selected (the menu's Clear all clears every slot)
    RefClearOne,
    RefShow(usize),
    RefClear,
    MathOpI,
    MathFam,
    MathA,
    MathB,
    MathScale,
    MathOffset,
    FftWin,
    MathFc,
    MathAuto,
    MathSel,
    /// Studio: turns the selected math channel off (the first row of its menu)
    MathOff,
    MathBw,
    /// the designer's response, order, ripple, symbol time, roll-off and delay of a filter
    MathFiltResp,
    MathFiltOrder,
    MathFiltRipple,
    MathFiltSymbol,
    MathFiltRolloff,
    MathFiltDelay,
    Open(MenuId),
    ChOn(usize),
    ChCoupling(usize),
    ChBw(usize),
    ChProbe(usize),
    ChInvert(usize),
    /// the channel stays on without its trace (`Model::ch_hidden`)
    ChHide(usize),
    ChScale(usize),
    ChOffset(usize),
    HScale,
    HDelay,
    HZero,
    AutoDeskew,
    ClearDeskew,
    TrigSrc,
    TrigSlope,
    TrigMode,
    TrigLevel,
    Trig50,
    TrigType,
    SearchOn,
    TrigLevel2,
    TrigSrc2,
    TrigSlope2,
    TrigPol,
    TrigWhen,
    TrigT1,
    TrigT2,
    VidStd,
    VidSync,
    VidLine,
    Pat(usize),
    PatLevel,
    WinModeI,
    NthIdle,
    NthN,
    Setup,
    Hold,
    AcqMode,
    AcqAvg,
    AcqDepth,
    AcqSeg,
    /// software matching of the ADC cores' offsets (`board::dho::cores`)
    AcqCores,
    /// a note while the sample rate is below the ADC's: what lies above half of it folds down, nothing filters it
    AcqAlias,
    /// segments recorded by the FPGA (`Model::seg_hw`)
    AcqSegHw,
    /// Average mode: the FPGA averages (`Model::avg_hw`)
    AcqAvgHw,
    AcqSegIvl,
    /// the trigger times of the hardware segments
    AcqSegTable,
    MeasSrc,
    /// copies every measurement on Source A to the picked source
    MeasCopyTo,
    MeasAdd(Meas),
    /// shortcut on the press-and-hold menu: the next usual measurement
    AddMeas,
    /// settings of one added measurement, index into `m.meas`
    MeasCfg(u16),
    /// a field of the measurement settings card
    RowF(measrow::RF),
    MeasPair(usize),
    MeasSrcB,
    DvmOn,
    DvmSrc,
    DvmMode,
    CntOn,
    CntSrc,
    CntMode,
    CntDigits,
    CntClear,
    TrigHoldoff,
    ChLabel(usize),
    MathExprI,
    MathExprEdit,
    LogOn,
    LogIvl,
    LogInfo,
    SpecLimitSet,
    SpecLimitClear,
    SpecMargin,
    ChFine,
    ChUnit(usize),
    ChBias(usize),
    ChBiasZero(usize),
    ChSkew(usize),
    ChExpand,
    HFine,
    HExpandI,
    RefColor,
    RefLabel,
    CurC,
    CurD,
    RefSel,
    RefVpd,
    RefOffset,
    RefExport,
    RefImport,
    RefLoad,
    MaskSave,
    MaskLoad,
    MathLinA,
    MathLinB,
    MeasClear,
    MeasIdle,
    MeasOutside,
    MeasN,
    MeasEdges,
    MeasRefs,
    /// Levels: percent or volts, low, middle, high, hysteresis, top/base method, fall levels
    MeasLvType,
    MeasLow,
    MeasMid,
    MeasHigh,
    MeasHyst,
    MeasBaseTop,
    MeasSepFall,
    MeasFallLow,
    MeasFallMid,
    MeasFallHigh,
    MeasGate,
    /// Studio: the badge row under the lanes as Studio chips or as the classic strip
    BadgeStyle,
    MeasTsetup,
    MeasThold,
    CurMode,
    CurSrcB,
    CurA,
    CurB,
    /// Studio cursor settings (ui/addrail.rs): source, second source, readout place, linked, remove
    CurSrc,
    CurSecond,
    CurReadout,
    CurLink,
    CurRemove,
    /// Studio search settings (ui/addrail.rs)
    SrchWhich,
    SrchType,
    SrchSrc,
    SrchSlope,
    SrchLevel,
    SrchLevel2,
    SrchWhen,
    SrchTime,
    SrchTime2,
    SrchShow,
    Srch50,
    SrchFromTrig,
    SrchToTrig,
    SrchEvent,
    SrchRemove,
    Persist,
    Intensity,
    GridStyle,
    /// view density (ui/windows.rs): Scope, Full, Split, Four
    WinLayout,
    WinCh(usize),
    /// window of reference R<n+1> in a split
    WinRef(usize),
    WinMath(usize),
    WinLa,
    ClearDisp,
    Dots,
    /// Display > Layout: the stock OOS bars or the compact ones
    Layout,
    /// Display > Bottom bar: Cards or Compact (`Model::bar_compact`)
    BottomBar,
    /// System > Features: one row per feature of `features::features()`, one per decoder of `PROTOS`
    FeatOn(usize),
    FeatDec(usize),
    /// Display > Monitor view (ui/monitor.rs)
    HdmiView,
    Hdmi,
    TextSize,
    ZInput,
    Histogram,
    SpecOn,
    SpecSrc,
    SpecCenter,
    SpecSpan,
    SpecRbw,
    SpecWin,
    SpecTrace,
    SpecRef,
    SpecWf,
    SpecDist,
    SpecAcprW,
    SpecAcprS,
    SpecPeak,
    /// harmonics the pane's span follows
    SpecHarm,
    /// what a chart of the Trend pane shows
    TrendSel(usize),
    EyeOn,
    /// one press: measure the bit rate from the record and open the eye
    EyeAct,
    EyePlace,
    EyeSrc,
    EyeRate,
    EyeClear,
    EyeBer,
    EyeB,
    EyeHp,
    EyePnLo,
    EyePnHi,
    EyeNpj,
    EyeRail,
    EyeNotch,
    /// closed-loop bandwidth of the recovered clock (0 = auto)
    EyePll,
    PowOn,
    PowV,
    PowI,
    PowMode,
    PowFline,
    PowFsw,
    PowVin,
    PowRef,
    PowClk,
    IdentifyAct,
    ProtoWatch,
    IdFind(usize),
    AfgSel,
    AfgOn,
    AfgArbSrc,
    AfgArbGrab,
    AfgArbFile,
    GridBright,
    GridFrame,
    /// noise reject of the trigger comparators
    TrigNoise,
    /// the auto set-up turns noise reject on for a noisy trigger signal (opt-in)
    TrigAutoNoise,
    /// coupling of the edge trigger
    TrigCoup,
    LaOn,
    DecIn,
    LaShow,
    /// threshold preset / user threshold of pod 0 (D0-D7) or 1 (D8-D15)
    LaThr(usize),
    LaUser(usize),
    LaHeight,
    LaLine(usize),
    LaPat(usize),
    BwOn,
    BwSrc,
    BwHint,
    BwBins,
    AfgFreq,
    AfgFreqEntry,
    AfgAmpl,
    AfgOffset,
    AfgWave,
    AfgDuty,
    AfgSym,
    AfgMod,
    AfgModRate,
    AfgModDepth,
    AfgModShape,
    BodeOn,
    BodeRun,
    BodeStart,
    BodeStop,
    BodePoints,
    BodeAmp,
    BodeIn,
    BodeOut,
    BodeGen,
    ExtModel,
    ExtLink,
    ExtAddr,
    ExtState,
    ExtIdn,
    ExtCh,
    ExtLoad,
    ExtFreq,
    ExtAmpl,
    ExtOut,
    ExtConnect,
    BodeCur,
    BodeMode,
    BodeSwTime,
    BodeSwLog,
    BodeSync,
    BodeSyncEdge,
    BodeSyncLvl,
    BodeRef,
    CalCh,
    CalZero,
    CalAdc,
    CalStart,
    CalCancel,
    CalStatus,
    ZoneSel,
    ZoneOn,
    ZoneSrc,
    ZoneMode,
    ZoneBox,
    ZoneShape,
    ZoneClear,
    About,
    SysInfo,
    /// System > Probe optional memory (ui/upper.rs): the frame memory above 256 MiB probed, `mem.upper` from its answer
    UpperProbe,
    /// System > Opt. memory (ui/upper.rs): the memory above 256 MiB used or not, `mem.upper`
    UpperUse,
    Shot,
    AuxOut,
    Stock,
    Update,
    UpdateRollback,
    BootMode,
    /// System > Start OOS again: OOS at the next power-on once more (`oos-update arm`, as `oos-boot-oos-again`)
    BootAgain,
    Overclock,
    ViewExit,
    ViewSettings,
    SrcOn,
    NetEth,
    NetMac,
    NetWeb,
    NetPorts,
    NetWlan,
    /// System > Network > Remote services (ui/remote.rs): a switch per service, and where it is reached
    RemWeb,
    RemScpi,
    RemVxi,
    RemSsh,
    RemScpiInfo,
    RemVxiInfo,
    /// what MCP of the web remote offers (Off, Read only, Full; `remote.mcp`), and where it is reached
    RemMcp,
    RemMcpInfo,
    /// System > Network > Bluetooth (ui/bt.rs): the state, the actions, and the pickers of found, paired and to be
    /// forgotten devices
    BtState,
    BtScan,
    BtPaired,
    BtForget,
    BtInfo,
    BtFound,
    BtPairedPick,
    BtForgetPick,
    /// System > Network > Remote services > SSH settings (ui/ssh.rs); `SshRemoveKey` is its picker
    SshState,
    SshPassword,
    SshShowPass,
    SshHostKey,
    SshKeys,
    SshKeyList,
    SshImport,
    SshRemove,
    SshRemoveKey,
    /// the mDNS switch, and the scope's name on the network (`<name>.local`, a picker with Other on the keyboard)
    RemMdns,
    RemName,
    /// who may reach the scope (`net.allow`, a picker with Other on the keyboard), and whether that is in force
    RemAllow,
    RemFirewall,
    /// System > Graphics: CPU or GPU (ui/graphics.rs)
    Graphics,
    /// Display > Views (ui/views.rs)
    Views,
    /// Display > Backlight, percent (ui/backlight.rs)
    Backlight,
    WlanInfo,
    WlanScan,
    WlanForget,
    WlanNet,
    WlanForgetNet,
    /// one more autonegotiation for a link at 10 Mbit/s (ethlink.rs)
    NetRetry10,
    /// System > Date and time (ui/time.rs)
    TimeNow,
    NtpOn,
    NtpServer,
    NtpStatus,
    NtpSync,
    TimeZone,
    TimeSet,
    ColorGrade,
    StoreSlot,
    StoreDest,
    StoreSaveSetup,
    /// the set-up as SCPI commands, into a file (scpi/setupscpi.rs)
    StoreSaveScpi,
    /// the SCPI recorder, on and off (scpi/recorder.rs)
    StoreRecScpi,
    /// runs a file of SCPI commands (scpi/replay.rs); while one runs, stops it
    StoreRunScpi,
    StoreRecall,
    /// a setup as a file with a name of its own, beside the slots (ui/files.rs, ADR 0029)
    StoreSetupSave,
    StoreSetupLoad,
    /// the list of files a Storage row loads from: its options are the files of the folder
    StoreFile,
    StoreCsv,
    StorePng,
    StoreFfSave,
    StoreFfLoad,
    /// the whole bench as one file: setup, the record on the screen, the reference slots (ADR 0027)
    StoreSessSave,
    StoreSessLoad,
    MaskOn,
    MaskSrc,
    MaskX,
    MaskY,
    MaskAction,
    MaskInvert,
    MaskCreate,
    MaskFromRef,
    MaskReset,
    MaskClear,
    QuickAct,
    /// option n of the open picker
    PickOpt(usize),
    // start menu entries for the front-panel keys
    RunStop,
    SingleAct,
    AutosetAct,
    DefaultAct,
    SlopeAct,
    ForceAct,
    NavHist,
    NavBack,
    NavFwd,
    QuickRun,
    LockAct,
    PowerAct,
    MagOn,
    MagV,
    MagI,
    MagN,
    MagAe,
    MagLe,
    MagRcu,
    MagPlot,
    MagCur,
    InvOn,
    InvPage,
    InvA,
    InvB,
    InvC,
    InvAng,
    InvPoles,
    InvAn,
    InvQty,
    InvTq,
    InvEv,
    InvEi,
    InvScale,
    InvDeg,
    InvOff,
    InvElec,
    InvQa,
    InvQb,
    InvQi,
    InvCpr,
    InvSwap,
    InvHa,
    InvHb,
    InvHc,
    DptOn,
    DptV,
    DptI,
    DptG,
    DptFourth,
    Dpt4,
    DptPct,
    DptZoom,
    SimPreset,
    SimSlot,
    SimPlay,
    /// SIM mode over the shown references: files and added signals played together as the acquisition
    SimPlayRefs,
    SimLive,
    SimShow,
    SimRate,
    SimPat,
    SimHigh,
    SimLow,
    SimRise,
    SimBw,
    SimDeemph,
    SimNoise,
    SimSsc,
    SimSscCenter,
    SimSscDeviation,
    SimSscHz,
    SimDcd,
    SimCorrRj,
    SimCorrHz,
    SimRj,
    SimPj,
    SimPjHz,
    SimDefaults,
    Offline,
    /// Appended so all previous menu item discriminants remain stable.
    DecCphyRate,
    /// Remove a selected row, indexed across single, pair and catalog rows.
    MeasRemove(u16),
    CatSourceC,
    CatSourceD,
    /// SIM sheet: write the signal to the chosen ref (no SIM mode)
    SimAddRef,
    /// Studio SIM sheet: a waveform file into the SIM slot (and into playback)
    SimLoad,
    /// the number pad of a value row in Default (`numpad.rs`, upstream's)
    NumEntry,
    /// Studio, Display sheet: the row of measurement badges is shown (`studiolayout.rs`)
    StudioBadges,
    /// Studio, Display sheet: smaller lane gaps, badge row and measurement panel
    StudioCompact,
    /// Studio, Display sheet: back to the arrangement the layout starts with
    StudioReset,
    /// Studio, Storage sheet: the view that analyses the loaded files
    StudioOffline,
}

/// The choices of an item, for its picker.
type OptionsFn = fn(&Ui, Item) -> Option<Vec<String>>;

/// One part of the menus (a module of ui/): what it answers about the items it owns, None where it has nothing of
/// that kind. A part answers None (false) for an item that is not its own, and no item of a menu belongs to two parts
/// (`no_menu_item_has_two_parts`), so the order of `PARTS` decides nothing.
pub(super) struct Part {
    pub(super) kind: Option<fn(Item) -> Option<Kind>>,
    pub(super) label: Option<fn(Item) -> Option<&'static str>>,
    pub(super) value: Option<fn(&Ui, Item) -> Option<String>>,
    pub(super) adjust: Option<fn(&mut Ui, Item, i32) -> bool>,
    pub(super) activate: Option<fn(&mut Ui, Item) -> bool>,
    pub(super) options: Option<OptionsFn>,
    /// a line of text, not a control
    pub(super) text: Option<fn(Item) -> bool>,
}

const NO_PART: Part =
    Part { kind: None, label: None, value: None, adjust: None, activate: None, options: None, text: None };

/// The parts of the menus, asked by `kind`, `label`, `value`, `adjust`, `activate`, `options` and `ctl` before the
/// items `ui.rs` keeps itself. `remote` answers for `ssh` too, except the actions.
pub(super) const PARTS: [Part; 30] = [
    // oos-plus parts: Studio's rows about its arrangement, the Add rail of Studio, the per-row measurement sheet,
    // the external generator
    Part {
        kind: Some(Ui::studiolayout_kind),
        label: Some(Ui::studiolayout_label),
        value: Some(Ui::studiolayout_value),
        adjust: Some(Ui::studiolayout_adjust),
        activate: Some(Ui::studiolayout_activate),
        options: Some(Ui::studiolayout_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::addrail_kind),
        label: Some(Ui::addrail_label),
        value: Some(Ui::addrail_value),
        adjust: Some(Ui::addrail_adjust),
        options: Some(Ui::addrail_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::measrow_kind),
        label: Some(Ui::measrow_label),
        value: Some(Ui::measrow_value),
        adjust: Some(Ui::measrow_adjust),
        options: Some(Ui::measrow_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::ext_kind),
        label: Some(Ui::ext_label),
        value: Some(Ui::ext_value),
        adjust: Some(Ui::ext_adjust),
        activate: Some(Ui::ext_activate),
        options: Some(Ui::ext_options),
        text: Some(Ui::ext_text),
    },
    Part {
        kind: Some(Ui::meas_kind),
        label: Some(Ui::meas_cfg_label),
        value: Some(Ui::meas_value),
        adjust: Some(Ui::meas_adjust),
        options: Some(Ui::meas_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::ana_kind),
        label: Some(Ui::ana_label),
        value: Some(Ui::ana_value),
        adjust: Some(Ui::ana_adjust),
        activate: Some(Ui::ana_activate),
        options: Some(Ui::ana_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::magnetic_kind),
        label: Some(Ui::magnetic_label),
        value: Some(Ui::magnetic_value),
        adjust: Some(Ui::magnetic_adjust),
        options: Some(Ui::magnetic_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::inverter_kind),
        label: Some(Ui::inverter_label),
        value: Some(Ui::inverter_value),
        adjust: Some(Ui::inverter_adjust),
        options: Some(Ui::inverter_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::dpt_kind),
        label: Some(Ui::dpt_label),
        value: Some(Ui::dpt_value),
        adjust: Some(Ui::dpt_adjust),
        options: Some(Ui::dpt_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::cal_kind),
        label: Some(Ui::cal_label),
        value: Some(Ui::cal_value),
        adjust: Some(Ui::cal_adjust),
        activate: Some(Ui::cal_activate),
        options: Some(Ui::cal_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::bode_kind),
        label: Some(Ui::bode_label),
        value: Some(Ui::bode_value),
        adjust: Some(Ui::bode_adjust),
        activate: Some(Ui::bode_activate),
        options: Some(Ui::bode_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::sim_kind),
        label: Some(Ui::sim_label),
        value: Some(Ui::sim_value),
        adjust: Some(Ui::sim_adjust),
        activate: Some(Ui::sim_activate),
        options: Some(Ui::sim_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::offline_kind),
        label: Some(Ui::offline_label),
        value: Some(Ui::offline_value),
        adjust: Some(Ui::offline_adjust),
        options: Some(Ui::offline_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::afg_kind),
        label: Some(Ui::afg_label),
        value: Some(Ui::afg_value),
        adjust: Some(Ui::afg_adjust),
        activate: Some(Ui::afg_activate),
        options: Some(Ui::afg_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::bw_kind),
        label: Some(Ui::bw_label),
        value: Some(Ui::bw_value),
        adjust: Some(Ui::bw_adjust),
        options: Some(Ui::bw_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::la_kind),
        label: Some(Ui::la_label),
        value: Some(Ui::la_value),
        adjust: Some(Ui::la_adjust),
        options: Some(Ui::la_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::win_kind),
        label: Some(Ui::win_label),
        value: Some(Ui::win_value),
        adjust: Some(Ui::win_adjust),
        options: Some(Ui::win_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::bus_kind),
        label: Some(Ui::bus_label),
        value: Some(Ui::bus_value),
        adjust: Some(Ui::bus_adjust),
        options: Some(Ui::bus_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::time_kind),
        label: Some(Ui::time_label),
        value: Some(Ui::time_value),
        adjust: Some(Ui::time_adjust),
        activate: Some(Ui::time_activate),
        options: Some(Ui::time_options),
        text: Some(Ui::time_text),
    },
    Part {
        kind: Some(Ui::remote_kind),
        label: Some(Ui::remote_label),
        value: Some(Ui::remote_value),
        adjust: Some(Ui::remote_adjust),
        options: Some(Ui::remote_options),
        text: Some(Ui::remote_text),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::graphics_kind),
        label: Some(Ui::graphics_label),
        value: Some(Ui::graphics_value),
        adjust: Some(Ui::graphics_adjust),
        options: Some(Ui::graphics_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::views_kind),
        label: Some(Ui::views_label),
        value: Some(Ui::views_value),
        adjust: Some(Ui::views_adjust),
        options: Some(Ui::views_options),
        ..NO_PART
    },
    Part {
        label: Some(Ui::monitor_label),
        value: Some(Ui::monitor_value),
        adjust: Some(Ui::monitor_adjust),
        options: Some(Ui::monitor_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::features_kind),
        label: Some(Ui::features_label),
        value: Some(Ui::features_value),
        adjust: Some(Ui::features_adjust),
        options: Some(Ui::features_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::backlight_kind),
        label: Some(Ui::backlight_label),
        value: Some(Ui::backlight_value),
        adjust: Some(Ui::backlight_adjust),
        ..NO_PART
    },
    Part { activate: Some(Ui::ssh_activate), ..NO_PART },
    Part {
        kind: Some(Ui::upper_kind),
        label: Some(Ui::upper_label),
        value: Some(Ui::upper_value),
        adjust: Some(Ui::upper_adjust),
        activate: Some(Ui::upper_activate),
        options: Some(Ui::upper_options),
        ..NO_PART
    },
    Part {
        kind: Some(Ui::bt_kind),
        label: Some(Ui::bt_label),
        value: Some(Ui::bt_value),
        activate: Some(Ui::bt_activate),
        options: Some(Ui::bt_options),
        text: Some(Ui::bt_text),
        ..NO_PART
    },
    Part { kind: Some(Ui::row_pad_kind), label: Some(Ui::row_pad_label), ..NO_PART },
    Part {
        kind: Some(Ui::setting_kind),
        label: Some(Ui::setting_label),
        value: Some(Ui::setting_value),
        adjust: Some(Ui::setting_adjust),
        options: Some(Ui::setting_options),
        ..NO_PART
    },
];

#[cfg(test)]
mod part_tests {
    use super::*;

    // Break it catches: an item of a menu that two parts answer for, so the order of PARTS (and of the six chains it
    // replaced, which did not agree) decides what the menu shows or does. Actions are not run.
    #[test]
    fn no_menu_item_has_two_parts() {
        use MenuId::*;
        let ui = crate::ui::testkit::demo_ui();
        let mut items: Vec<Item> = Vec::new();
        let menus = [
            Main,
            Channel(0),
            ChMore(0),
            Logger,
            Horizontal,
            Trigger,
            Acquire,
            Measure,
            MeasAdd,
            MeasHold,
            MeasPair,
            Dvm,
            Cursor,
            Display,
            Utility,
            Math,
            Ref,
            Decode,
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
            Network,
            Features,
            Decoders(0),
            Remote,
            Ssh,
            Bluetooth,
            Time,
            Bode,
            Sim,
            SimAdvanced,
            Afg,
            BwTest,
            ViewMore,
            La,
            LaLines,
            LaPat,
            Windows,
            MeasSelected,
        ];
        for m in menus {
            for it in ui.items(m) {
                if !items.contains(&it) {
                    items.push(it);
                }
            }
        }
        let mut owned = 0;
        for &it in &items {
            let mut u = crate::ui::testkit::demo_ui();
            let claims: Vec<usize> = (0..PARTS.len())
                .filter(|&i| {
                    let p = &PARTS[i];
                    p.kind.is_some_and(|f| f(it).is_some())
                        | p.label.is_some_and(|f| f(it).is_some())
                        | p.value.is_some_and(|f| f(&ui, it).is_some())
                        | p.options.is_some_and(|f| f(&ui, it).is_some())
                        | p.text.is_some_and(|f| f(it))
                        | p.adjust.is_some_and(|f| f(&mut u, it, 1))
                })
                .collect();
            assert!(claims.len() <= 1, "{:?} belongs to parts {:?}", it, claims);
            owned += claims.len();
        }
        assert!(items.len() > 400 && owned > 200, "{} items, {} of a part", items.len(), owned);
    }
}
