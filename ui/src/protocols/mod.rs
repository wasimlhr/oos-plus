// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Protocol modules: a decoder with everything the scope needs around it, behind one registration.
//!
//! A protocol module is one file in this directory with a type that implements [`Protocol`], and one line in
//! [`MODULES`]. That is all: the Decode menu (protocol list, source rows named after the roles, option rows), saved
//! setups, the packet labels on the trace and in the event table, the demo traffic of the host build and the
//! protocol's own view in the pane under the plot come from the trait. What the decoder is lives in `oos-dsp`: its
//! [`ModuleInfo`] (key, name, the lines it takes, the call of the decoder, its bit time, what the protocol trigger
//! finds and starts with) stands next to the decoder and is listed in `oos_dsp::protocols::MODULES`; the module names
//! it with [`Protocol::info`].
//!
//! Every decoder is a module, and every module is always built (docs/adr/code/0071). The seven core protocols
//! (`core.rs`: UART, I2C, SPI, CAN, LIN, Parallel, C-PHY) keep the first places, since the hardware trigger types and
//! SCPI name them (`Proto::UART` ...). `decoders.off` in /oos/system.conf leaves decoders out at run time.
//! `docs/protocol-modules.md` walks through writing one, and `example.rs` (compiled in the tests) uses every hook.

use crate::model::{Model, Proto, Slope};
use oos_dsp::decode::Packet;
use oos_dsp::protocols::ModuleInfo;

mod avionics;
mod core;
mod dcc;
mod debug;
mod dmx512;
#[cfg(test)]
pub mod example;
mod i2s;
mod i3c;
mod linecode;
mod onewire;
mod qspi;
mod serial;
mod spdif;
mod usb;
mod usbpd;
mod vehicle;

/// The registered modules. The order is the order in the Decode menu; the core protocols come first, at fixed places.
/// Never reuse or change a module's `key`: saved setups refer to it.
pub static MODULES: &[&dyn Protocol] = &[
    // the core decoders at fixed places (`Proto::UART` ...): never move them
    &core::Uart,
    &core::I2c,
    &core::Spi,
    &core::Can,
    &core::Lin,
    &core::Parallel,
    &core::Cphy,
    &dmx512::Dmx512,
    &spdif::Spdif,
    &usbpd::UsbPd,
    &dcc::Dcc,
    &linecode::Manchester,
    &linecode::Mlt3,
    &serial::Uart9,
    &serial::ModbusRtu,
    &serial::ModbusAscii,
    &onewire::OneWire,
    &onewire::Ds2482,
    &debug::Swd,
    &debug::Jtag,
    &qspi::Qspi,
    &i3c::I3c,
    &vehicle::CanFd,
    &vehicle::FlexRay,
    &vehicle::Sent,
    &avionics::Arinc429,
    &avionics::Mil1553,
    &i2s::I2s,
    &usb::Usb,
    #[cfg(test)]
    &example::Example,
];

pub use oos_dsp::protocols::{Lines, Shared, OPTS};

/// A row of the Decode menu several protocols share, kept in its own field of the model (`dec_baud`, `dec_bits` ...):
/// SCPI (`:BUS<n>:RS232:BAUD` ...), the hardware protocol triggers and the setups use those fields by name, so a
/// protocol that has such a setting names the shared row ([`Protocol::shared`]) instead of an option of its own and
/// reads its value from [`Lines::sh`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Row {
    Baud,
    Bits,
    Parity,
    /// SPI mode 0..3
    SpiMode,
    CanRate,
    /// CAN: the line is CAN-H (else CAN-L or a transceiver's RX)
    CanH,
    UsbSpeed,
    CphyRate,
    /// the event table in ASCII
    Fmt,
    /// the UART line is inverted (idle low): the polarity of the protocol trigger, `Shared::uart_inv`
    Pol,
    /// SPI: the chip select is active high (the protocol trigger's, `Shared::spi_cs_high`)
    SpiCs,
}

/// The values of the shared rows in `m`.
pub fn shared(m: &crate::model::Model) -> Shared {
    Shared {
        baud: crate::decode::BAUDS[m.dec_baud.min(crate::decode::BAUDS.len() - 1)],
        bits: m.dec_bits,
        parity: m.dec_parity,
        spi_mode: m.dec_mode,
        can_rate: crate::decode::CAN_RATES[m.dec_canrate.min(crate::decode::CAN_RATES.len() - 1)],
        canh: m.dec_canh,
        usb_speed: crate::decode::USB_SPEEDS[m.dec_uspeed.min(crate::decode::USB_SPEEDS.len() - 1)],
        cphy_rate: m.dec_cphy_rate,
        uart_inv: m.ser.uart_inv,
        spi_timeout: m.ser.spi_timeout as f64,
        spi_cs_high: m.ser.spi_cs_high,
    }
}

/// The baud rates of the Decode menu (`decode::BAUDS`), as the choices of a module's option row.
pub const BAUD_NAMES: [&str; 10] =
    ["1200", "2400", "4800", "9600", "19200", "38400", "57600", "115200", "460800", "921600"];
/// The default of a baud row: 115200, as the UART decoder.
pub const BAUD_DEFAULT: usize = 7;

/// The decoder settings of a setup from before a protocol became a module: what its option rows were kept in.
/// A module that was a built-in protocol maps them onto its options (`Protocol::legacy`).
#[derive(Clone, Copy, Debug)]
pub struct Legacy {
    pub baud: usize,
    pub bits: u32,
    pub parity: oos_dsp::decode::Parity,
    pub canrate: usize,
    pub canh: bool,
    pub fdrate: usize,
    pub arate: usize,
    pub tick: usize,
    pub nibbles: u32,
}

/// One option row of the Decode menu: a name and its choices; the setting is the index of the choice.
pub struct Opt {
    pub name: &'static str,
    pub choices: &'static [&'static str],
    pub default: usize,
}

/// Where the demo traffic sits and what the decoder starts with when the protocol is chosen.
pub struct Setup {
    /// channel per role (0 = CH1); `None` leaves an optional role unset
    pub src: [Option<usize>; 4],
    /// logic analyser line per role, for the LA input
    pub la: [Option<u8>; 4],
    /// decision threshold (V)
    pub thr: f32,
    /// a trigger that holds the demo traffic still, set when the protocol is chosen; `None` leaves the trigger alone
    #[allow(dead_code)] // set by the protocol table; the demo does not apply it yet
    pub trig: Option<DemoTrig>,
    /// switch the channels of all its roles on when the protocol is chosen (the demo traffic is on all of them)
    pub show: bool,
    /// shared rows set when the protocol is chosen: the row and its value (an index, or the number of bits, 0/1)
    pub rows: &'static [(Row, u32)],
    /// switch every channel on (the protocol decodes across all of them)
    pub show_all: bool,
    /// the decoder takes the analog channels, not the LA lines
    pub la_off: bool,
}

impl Default for Setup {
    /// Main line on CH3 and a second one on CH4, 3.3 V logic; the demo UART line D8 and clock D10 for the LA; the
    /// trigger as it is.
    fn default() -> Setup {
        Setup {
            src: [Some(2), Some(3), None, None],
            la: [Some(8), Some(10), None, None],
            thr: 1.65,
            trig: None,
            show: false,
            rows: &[],
            show_all: false,
            la_off: false,
        }
    }
}

/// What a column of the Events pane holds of a frame ([`Protocol::columns`]). The texts are those of the band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Col {
    /// the fields whose text begins with this label, without the label ("ID ", "CRC ")
    Label(&'static str),
    /// like `Label`; in a frame without such a field, what the frame holds instead (an error frame of CAN says so in
    /// the column of the identifier)
    LabelOr(&'static str),
    /// one word of the text of such a field, counted from the label, which is word 0 ("RT 17 T SA 02 WC 03": word 1
    /// is the terminal of a MIL-STD-1553 command)
    Part(&'static str, usize),
    /// the field at this place of the frame (the packet identifier after the sync of a USB packet)
    At(usize),
    /// the first address of the frame
    Addr,
    /// the data fields after the first field another column takes by its label, place or kind
    Data,
    /// how many data fields those are
    Count,
    /// everything after that first field, as the band writes it
    Rest,
    /// the first values of the data words (SPI: MOSI)
    First,
    /// their second values (SPI: MISO)
    Second,
    /// a part of one field, which the module writes itself ([`Protocol::field_cells`])
    Own,
    /// a cell of the whole message, which the module writes ([`Protocol::message_cells`], the place counted)
    Msg(usize),
}

/// The trigger of [`Setup::trig`]: an edge of `slope` on the channel of role `role` at `level` (V). With an `idle` time
/// (s) only the first edge after the line was quiet that long (Nth edge, n = 1), else every edge (Edge).
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)] // see `trig`: filled in, not applied yet
pub struct DemoTrig {
    pub role: usize,
    pub slope: Slope,
    pub idle: f32,
    pub level: f32,
}

/// A line of demo traffic: voltage at time `t` (s) within one period.
pub type DemoLine = Box<dyn Fn(f64) -> f64 + Send + Sync>;

/// Demo traffic of the host build: one waveform per role, on the channels of [`Protocol::setup`], repeated every
/// `period` seconds.
pub struct Demo {
    pub period: f64,
    pub lines: [Option<DemoLine>; 4],
}

/// What a module keeps for its view from one record to the next (the devices seen so far ...). The scope gives each
/// decoder its own, empty at first, and empties it when the protocol changes or the display is cleared.
pub type Memory = Option<Box<dyn std::any::Any + Send>>;

/// The memory of a view as the module's own type `T`, made with `T::default()` when it is empty or of another type.
pub fn memory<T: Default + Send + 'static>(m: &mut Memory) -> &mut T {
    if !m.as_ref().is_some_and(|b| b.is::<T>()) {
        *m = Some(Box::new(T::default()));
    }
    m.as_mut().and_then(|b| b.downcast_mut::<T>()).unwrap()
}

/// Colour hint of a view's status line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Neutral,
    Good,
    Bad,
}

/// A protocol's own picture of what it decoded, in the pane under the plot (Display > Pane > Bus view): a status line
/// and columns of text rows, for example the devices on a bus and their last state.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct View {
    pub status: Option<(String, Tone)>,
    pub sections: Vec<Section>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Section {
    pub title: String,
    pub rows: Vec<String>,
}

/// A protocol module. Only `info` is required; everything else has a default that
/// fits a single-line serial protocol.
pub trait Protocol: Sync {
    /// What oos-dsp says of the decoder: its stable key, its name, the lines it takes, whether the main one may be a
    /// differential pair and the rule and start value of the protocol trigger, next to the decoder in its family's
    /// file (`oos_dsp::protocols::MODULES`).
    fn info(&self) -> &'static ModuleInfo;

    /// The option rows of the Decode menu.
    fn options(&self) -> &'static [Opt] {
        &[]
    }

    /// Text of a decoded field on the trace and in the event table; `None` = the generic text of its kind.
    fn text(&self, _p: &Packet, _opt: &[usize; OPTS]) -> Option<String> {
        None
    }

    /// The field a frame of the protocol begins with, for a protocol that has one (the break of LIN, the sync of
    /// SENT, the start condition of I2C). A message on the band and in the Events pane is then a frame, from one
    /// such field to the next, whatever pauses it has inside. `None`: the messages are what was sent without a pause
    /// clearly longer than the usual one (a UART line, SPI without a chip select, Modbus).
    fn frame_start(&self) -> Option<fn(&Packet) -> bool> {
        None
    }

    /// The columns the Events pane has for a frame of the protocol in place of one text: a title and what the
    /// column holds of the frame ([`Col`]). Empty: the frame is one text, as the band writes it. The pane uses them
    /// when it lists messages, is wide enough, and every decoder that is on has the same columns.
    fn columns(&self) -> &'static [(&'static str, Col)] {
        &[]
    }

    /// The columns the Events pane has for one field of the protocol in place of its text, where a field has parts
    /// worth a column each (a word of MIL-STD-1553: its kind, terminal, direction ...). Every one is [`Col::Own`]:
    /// the cells come from [`Protocol::field_cells`]. Empty: a field is its text.
    fn field_columns(&self) -> &'static [(&'static str, Col)] {
        &[]
    }

    /// The cells of the field `p` for [`Protocol::field_columns`], one a column.
    fn field_cells(&self, _p: &Packet) -> Vec<String> {
        Vec::new()
    }

    /// What failed in the field `p`, for the last column of the Events pane where it lists fields ("Parity"). None:
    /// the pane writes that one check failed.
    fn field_error(&self, _p: &Packet) -> Option<&'static str> {
        None
    }

    /// The cells of the message `pk` for the columns that are `Col::Msg(n)` (n-th of the list); empty where a module
    /// has none.
    fn message_cells(&self, _pk: &[Packet]) -> Vec<String> {
        Vec::new()
    }

    /// What is wrong with the message `pk` as a whole, in words, for the Error column of the Events pane where it
    /// lists messages ("Incomplete packet; Unframed"). Empty: nothing the module can say beyond the failed checks of
    /// the fields.
    fn message_error(&self, _pk: &[Packet]) -> &'static str {
        ""
    }

    /// Whether `p` is a mark the module adds for the screen and not a field of the bus (the start of an SPI frame,
    /// where the chip select goes active). A mark begins a message and is no row of the Fields view.
    fn mark(&self, _p: &Packet) -> bool {
        false
    }

    /// Whether `p` is a field every message begins with and that says nothing of what the message is (the sync of
    /// a USB packet, the start sequence of a FlexRay frame). The text of a message leaves it out, so a box with
    /// little room begins with what tells one message from the next; as a field it has its box and its row.
    fn lead(&self, _p: &Packet) -> bool {
        false
    }

    /// Whether the data of the decoder is shown as characters with these options (a Format option of the module:
    /// Hex, ASCII). The UART has the shared Format row instead ([`Row::Fmt`]).
    fn chars(&self, _opt: &[usize; OPTS]) -> bool {
        false
    }

    /// Whether [`Protocol::bits`] gives the bits of a field. The band on the plot then has a bit row above its boxes
    /// at a time base that shows single bits (a bit of the first field that has them is wide enough to read).
    fn bit_row(&self) -> bool {
        false
    }

    /// The bits of field `p` as the decoder read them, in the time units of `p` (they lie between its `t0` and
    /// `t1`); `None` = this field has none.
    fn bits(&self, _p: &Packet, _opt: &[usize; OPTS], _sh: &Shared) -> Option<Vec<oos_dsp::decode::Bit>> {
        None
    }

    /// The event table (and its CSV, `:...EVENts?`): one row per command, as (time of its first packet, text, check
    /// passed), instead of one row per field. `None` = a row per field with `text`.
    fn events(&self, _p: &[Packet], _opt: &[usize; OPTS]) -> Option<Vec<(f64, String, bool)>> {
        None
    }

    /// Sources and threshold when the protocol is chosen.
    fn setup(&self) -> Setup {
        Setup::default()
    }

    /// Demo traffic for the host build; built once.
    fn demo(&self) -> Option<Demo> {
        None
    }

    /// The protocol's own view of the decoded packets of a new record (`None` = no view). Called once per record;
    /// `mem` carries what the module wants to keep across records. A module with a view gets the Bus view pane opened
    /// when it is chosen and no pane is open.
    fn view(&self, _p: &[Packet], _opt: &[usize; OPTS], _mem: &mut Memory) -> Option<View> {
        None
    }

    /// The shared rows of the Decode menu the protocol shows, before its own options ([`Row`]).
    fn shared(&self) -> &'static [Row] {
        &[]
    }

    /// For a protocol that was built in before it became a module: its option settings from a setup that kept them in
    /// the shared decoder fields (`Legacy`); `None` = the defaults.
    fn legacy(&self, _old: &Legacy) -> Option<[usize; OPTS]> {
        None
    }
}

impl dyn Protocol {
    /// The stable key of the module ([`ModuleInfo::key`]).
    pub fn key(&self) -> &'static str {
        self.info().key
    }

    /// The name of the module ([`ModuleInfo::name`]).
    pub fn name(&self) -> &'static str {
        self.info().name
    }
}

/// Every protocol of the Decode menu, in the order of `MODULES`.
pub static PROTOS: std::sync::LazyLock<Vec<Proto>> =
    std::sync::LazyLock::new(|| (0..MODULES.len()).map(|i| Proto::Module(i as u8)).collect());

/// The module of a protocol of the model: the model holds the index, the registry knows the modules (ADR 0049). A
/// trait of the registry, since the model's types can have methods only in oos-model (ADR 0064).
pub trait Module {
    /// The module behind the protocol.
    fn module(self) -> Option<&'static dyn Protocol>;
}

impl Module for Proto {
    fn module(self) -> Option<&'static dyn Protocol> {
        let Proto::Module(i) = self;
        Some(get(i))
    }
}

/// What the model's decoder settings mean with the module's options: a trait of the registry, since the model's types
/// can have methods only where they are declared (ADR 0064: the model becomes a crate of its own).
pub trait Decoder {
    /// Switches the decoder's protocol and nothing else (SCPI configures an instrument, not the demo's sources); a
    /// module that is newly chosen starts from its default options.
    fn set_dec_proto_only(&mut self, p: Proto);

    /// The option settings of the decoder's protocol module, in range.
    fn dec_opts(&self) -> [usize; OPTS];

    /// Bit time of the decoder's protocol (s), for the protocol trigger's sampling and search window.
    fn bus_bit(&self) -> f64;

    /// The record length the protocol trigger needs: a whole frame of the protocol before the trigger point (a module
    /// may say: USB auto, low-speed packets), else 200 bit times.
    fn bus_frame_span(&self) -> f64;
}

impl Decoder for Model {
    fn set_dec_proto_only(&mut self, p: Proto) {
        if let (Proto::Module(i), true) = (p, self.dec_proto != p) {
            self.dec_opt = opt_default(i);
        }
        self.dec_proto = p;
    }

    fn dec_opts(&self) -> [usize; OPTS] {
        let Proto::Module(i) = self.dec_proto;
        opt_fit(i, self.dec_opt)
    }

    fn bus_bit(&self) -> f64 {
        let Proto::Module(i) = self.dec_proto;
        get(i).info().bit_time(&self.dec_opts(), &shared(self))
    }

    fn bus_frame_span(&self) -> f64 {
        self.dec_proto
            .module()
            .and_then(|p| p.info().frame_span(&self.dec_opts(), &shared(self)))
            .unwrap_or(200.0 * self.bus_bit())
    }
}

/// Module `i` of [`MODULES`].
pub fn get(i: u8) -> &'static dyn Protocol {
    MODULES[i as usize]
}

/// The option settings of module `i` brought into range (a setup from another version may hold anything).
pub fn opt_fit(i: u8, opt: [u8; OPTS]) -> [usize; OPTS] {
    let o = get(i).options();
    std::array::from_fn(|k| o.get(k).map_or(0, |x| (opt[k] as usize).min(x.choices.len().saturating_sub(1))))
}

/// The default option settings of module `i`.
pub fn opt_default(i: u8) -> [u8; OPTS] {
    let o = get(i).options();
    std::array::from_fn(|k| o.get(k).map_or(0, |x| x.default as u8))
}

/// The demo traffic of module `i`, built on first use.
pub fn demo(i: u8) -> Option<&'static Demo> {
    static D: std::sync::OnceLock<Vec<Option<Demo>>> = std::sync::OnceLock::new();
    D.get_or_init(|| MODULES.iter().map(|m| m.demo()).collect())[i as usize].as_ref()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Keys are unique and safe for setup files; names are unique; option defaults are in range; every role a module
    /// names has a demo channel when the module has demo traffic.
    #[test]
    fn modules_are_well_formed() {
        let mut keys = std::collections::HashSet::new();
        let mut names = std::collections::HashSet::new();
        for m in MODULES {
            assert!(!m.key().is_empty() && m.key().chars().all(|c| c.is_ascii_alphanumeric()), "key {:?}", m.key());
            assert!(keys.insert(m.key()), "key {} twice", m.key());
            assert!(names.insert(m.name()), "name {} twice", m.name());
            assert!(m.options().len() <= OPTS, "{}: more than {} options", m.name(), OPTS);
            for o in m.options() {
                assert!(o.default < o.choices.len(), "{}: {} default", m.name(), o.name);
            }
            assert!(m.info().roles[0].is_some(), "{}: no main role", m.name());
        }
        // the core decoders at their fixed places (`Proto::UART` ...)
        for (p, key) in [
            (crate::model::Proto::UART, "Uart"),
            (crate::model::Proto::I2C, "I2c"),
            (crate::model::Proto::SPI, "Spi"),
            (crate::model::Proto::CAN, "Can"),
            (crate::model::Proto::LIN, "Lin"),
            (crate::model::Proto::PARALLEL, "Parallel"),
            (crate::model::Proto::CPHY, "Cphy"),
        ] {
            assert_eq!(p.key(), key);
        }
    }

    /// The packets of a module's demo traffic over `rounds` rounds of it, decoded as the scope decodes them.
    fn demo_packets(i: usize, rounds: f64) -> Option<Vec<Packet>> {
        let m = MODULES[i];
        let d = demo(i as u8)?;
        // (the shared rows of its set-up: the traffic is made for that baud rate, those bits)
        let mut model = crate::model::Model::default();
        crate::ui::Ui::dec_apply_rows(&mut model, m.setup().rows);
        let sh = shared(&model);
        let dt = m.info().bit_time(&opt_fit(i as u8, opt_default(i as u8)), &sh) / 40.0;
        let n = (rounds * d.period / dt) as usize;
        let wave = |f: &DemoLine| (0..n).map(|k| f((k as f64 * dt).rem_euclid(d.period)) as f32).collect::<Vec<f32>>();
        let waves: Vec<Option<Vec<f32>>> = d.lines.iter().map(|l| l.as_ref().map(wave)).collect();
        let (opt, lines) = (opt_fit(i as u8, opt_default(i as u8)), std::array::from_fn(|k| waves[k].as_deref()));
        let thr = m.setup().thr;
        // (the lines as the four channels too, in the order of their roles: a Parallel bus reads a bank of them)
        let src = [Some(0), Some(1), Some(2), Some(3)];
        let l = Lines { lines, dt, thr, hy: 0.1, opt, sh, chans: lines, src };
        Some(m.info().decode(&l).unwrap_or_else(|e| panic!("{}: {}", m.name(), e)))
    }

    /// A module that says where its frames begin: its demo traffic falls into frames that each begin with that
    /// field and hold no second one, and three rounds of the traffic are more than one frame. (A record may begin
    /// inside a frame: what stands before the first start is a message of its own.)
    #[test]
    fn a_module_with_frames_has_one_start_in_each() {
        let mut seen = Vec::new();
        for (i, m) in MODULES.iter().enumerate() {
            let (Some(begins), Some(p)) = (m.frame_start(), demo_packets(i, 3.0)) else { continue };
            let frames = oos_dsp::decode::frames(&p, begins);
            let starts = p.iter().filter(|x| begins(x)).count();
            assert!(starts >= 2, "{}: {starts} frame starts in three rounds of its demo", m.name());
            let before = usize::from(!begins(&p[0]));
            assert_eq!(frames.len(), starts + before, "{}", m.name());
            for f in frames.iter().skip(before) {
                let inside = p[f.clone()].iter().filter(|x| begins(x)).count();
                assert!(begins(&p[f.start]) && inside == 1, "{}: {f:?}", m.name());
            }
            seen.push(m.name());
        }
        // (with the core decoders alone: I2C, CAN, LIN)
        assert!(seen.len() >= 3 && seen.contains(&"I2C") && seen.contains(&"LIN"), "{seen:?}");
    }

    /// Every module with demo traffic decodes it without errors and finds something.
    #[test]
    fn modules_decode_their_demo() {
        for (i, m) in MODULES.iter().enumerate() {
            let Some(d) = demo(i as u8) else { continue };
            let sh = shared(&Default::default());
            let dt = m.info().bit_time(&opt_fit(i as u8, opt_default(i as u8)), &sh) / 40.0;
            let n = (3.0 * d.period / dt) as usize;
            let waves: Vec<Option<Vec<f32>>> = d
                .lines
                .iter()
                .map(|l| l.as_ref().map(|f| (0..n).map(|k| f((k as f64 * dt).rem_euclid(d.period)) as f32).collect()))
                .collect();
            let s = m.setup();
            let lines: [Option<&[f32]>; 4] = std::array::from_fn(|k| waves[k].as_deref());
            let (opt, src) = (opt_fit(i as u8, opt_default(i as u8)), [Some(0), Some(1), Some(2), Some(3)]);
            let l = Lines { lines, dt, thr: s.thr, hy: 0.1, opt, sh, chans: lines, src };
            let p = m.info().decode(&l).unwrap_or_else(|e| panic!("{}: {}", m.name(), e));
            assert!(!p.is_empty(), "{}: nothing decoded from its demo", m.name());
            // the byte that ends an I2C read is not acknowledged, and is no error: no module has an exception here
            let bad = p.iter().find(|x| !x.ok);
            assert!(bad.is_none(), "{}: errors in its own demo: {:?}", m.name(), bad);
        }
    }

    /// The modules without demo traffic, each with the reason: `modules_decode_their_demo` runs none of them, and
    /// the host build shows no signal for them. A module in this list that has demo traffic fails the test, so the
    /// list only gets shorter.
    const NO_DEMO: [(&str, &str); 2] = [
        ("Uart", "the demo source has UART traffic on CH3 by itself, whatever the decoder"),
        ("I3c", "none written yet; the decoder is tested in oos-dsp (dsp/tests/decode/i3c.rs)"),
    ];

    /// What a decoder needs besides its module, each failure with its name and the part that is missing: its key in
    /// ui/tests/golden/decoder-keys.txt (a key never changes: `OOS_BLESS=1 cargo test --locked -q protocols::tests`
    /// adds the line of a new one and no other), its `ModuleInfo` in `oos_dsp::protocols::MODULES` at the same place
    /// (and a module for every decoder there), its name under Protocol decoders in wiki/Features.md, demo traffic or
    /// the reason it has none, and with demo traffic its case in `extended_decoders_decode_their_demo`. The number of
    /// decoders the wiki and README.md give is the number registered. What a `ModuleInfo` needs is checked in oos-dsp
    /// (dsp/src/protocols.rs).
    #[test]
    fn every_decoder_is_complete() {
        use crate::ui::testkit::{count_of, document, exempt_once, names, section, stored_names};
        let wiki = section(&document("wiki/Features.md"), "**Protocol decoders");
        // the example module is compiled in the tests only
        let mods: Vec<_> = MODULES.iter().filter(|m| m.key() != "Example").collect();
        for m in &mods {
            assert!(names(&wiki, m.name()), "{}: not named under Protocol decoders in wiki/Features.md", m.name());
            match NO_DEMO.iter().find(|x| x.0 == m.key()) {
                Some(x) => {
                    assert!(m.demo().is_none(), "{}: it has demo traffic, take it out of NO_DEMO", m.key());
                    assert!(!x.1.is_empty(), "{}: in NO_DEMO without a reason", m.key());
                }
                None => assert!(m.demo().is_some(), "{}: no demo traffic, and no reason in NO_DEMO", m.key()),
            }
        }
        // the modules beyond the core ones with demo traffic are shown on the screen of the demo by a test
        let shown = include_str!("../ui/tests.rs");
        let shown = shown.split_once("fn extended_decoders_decode_their_demo()").expect("the test of the demos").1;
        let shown = shown.split_once("\n}\n").expect("its end").0;
        for m in mods.iter().skip(7).filter(|m| m.demo().is_some()) {
            let case = format!("m(\"{}\")", m.key());
            assert!(
                shown.contains(&case),
                "{}: no case {} in extended_decoders_decode_their_demo (ui/src/ui/tests.rs)",
                m.name(),
                case
            );
        }
        let keys: Vec<&str> = mods.iter().map(|m| m.key()).collect();
        // a module is a decoder of oos-dsp, in the order of its registry, and every one of them has its module
        let dsp = &oos_dsp::protocols::MODULES;
        for m in &mods {
            let registered = dsp.iter().any(|&i| std::ptr::eq(i, m.info()));
            assert!(registered, "{}: its ModuleInfo is not in oos_dsp::protocols::MODULES", m.key());
        }
        let order: Vec<&str> = dsp.iter().map(|i| i.key).filter(|k| keys.contains(k)).collect();
        assert_eq!(keys, order, "MODULES: not in the order of oos_dsp::protocols::MODULES");
        for i in dsp.iter().filter(|i| i.key != "Example") {
            assert!(keys.contains(&i.key), "{}: in oos_dsp::protocols::MODULES, no module in ui/src/protocols", i.key);
        }
        exempt_once("NO_DEMO", &NO_DEMO.map(|x| x.0), &keys);
        stored_names("decoder-keys.txt", &mods.iter().map(|m| m.key().to_string()).collect::<Vec<_>>());
        assert_eq!(count_of(&wiki, "Protocol decoders"), Some(mods.len()), "the number in wiki/Features.md");
        assert_eq!(count_of(&document("README.md"), "protocol decoders"), Some(mods.len()), "the number in README.md");
    }
}
