// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! SCPI server (TCP 5555, the port of the stock firmware), a stock-compatible subset:
//! *IDN? *RST *OPC[?] *CLS *ESR?, :RUN :STOP :SINGle :TFORce :AUToscale :CLEar,
//! :CHANnel<n>:CLIPping? (OOS: 1 while the record reaches the end of the ADC range),
//! :CHANnel<n>:DISPlay|SCALe|OFFSet|POSition|COUPling|BWLimit|PROBe|INVert|UNITs|VERNier|TCALibrate|BIAS|EXPand|LABel:SHOW|LABel:CONTent,
//! :TIMebase[:MAIN]:SCALe|OFFSet, :TIMebase:DELay:ENABle|SCALe|OFFSet, :TIMebase:VERNier, :TIMebase:HREFerence:MODE, :TIMebase:MODE, :TIMebase:ROLL,
//! :TIMebase:XY:ENABle|X|Y|Z,
//! :TRIGger:MODE[?]|TYPe[?] :TRIGger:EDGE:SOURce|LEVel|SLOPe, :TRIGger:COUPling DC|AC|LFR|HFR, :TRIGger:SWEep, :TRIGger:HOLDoff, :TRIGger:STATus?,
//! :ACQuire:SRATe?|MDEPth[?] (AUTO, 1k..125M)|TYPE NORM/PEAK/AVER/HRES|AVERages|ULTRa:MAXFrame,
//! :MEASure:ITEM[?] <item>[,CHANn|Rn[,src]] (the set form adds a readout row), :MEASure:SOURce, :MEASure:CLEar, :MEASure:INDicator,
//! :MEASure:STRip?, :MEASure:READout?, :EYE:ITEM? <figure>,
//! :MEASure:STATistic:DISPlay|RESet|POPulation|SHOW|ITEM, :MEASure:REFS 10/90|20/80, :MEASure:TSETup, :MEASure:THOLd,
//! :MEASure:BURSt:IDLE, :MEASure:OUTSide ABOVe|BELow|BOTH, :MEASure:NPERiods, :MEASure:EDGes CLOCk|DATA,
//! :CURSor:MODE OFF|MAN|TRAC|XY, :CURSor:TRACk:MODE X, :CURSor:MANual:TYPE TIME|AMPLitude, :CAX|:CBX|:CAY|:CBY, :AXValue|:AYValue|:BXValue|:BYValue, :XDELta|:IXDelta|:YDELta, :CURSor:MEASure:INDicator,
//! :DVM:ENABle|SOURce|MODE|CURRent?, :COUNter:ENABle|SOURce|MODE|NDIGits|CURRent?,
//! :MEASure:GATing SCREEN|NONE,
//! :MASK:ENABle|SOURce|X|Y|CREate|RESet|OPERate|FAILed?|PASSed?|TOTal?,
//! :HISTogram:ENABle|TYPE|SOURce, :HISTogram:STATistics:RESult?,
//! :MATH<n>:DISPlay|OPERator|SOURce1|SOURce2|SCALe|OFFSet|EXPRession|RESet, :MATH<n>:FFT:SOURce|WINDow|SCALe, :MATH<n>:FILTer:TYPE|W1|W2,
//! :BUS<n>:MODE|DISPlay|FORMat|EVENt|THReshold|DATA?, :BUS<n>:RS232:TX|RX|BAUD|PARity|DBITs, :BUS<n>:IIC:SCLK:SOURce|SDA:SOURce, :BUS<n>:SPI:SCLK:SOURce|MOSI:SOURce|SS:SOURce|DBITs,
//! :POWer:ITEM? <item>[,src...], :POWer:HARMonics? [src], :SPECtrum:ITEM? <item>[,src], :SPECtrum:RBW:ACTual?, :SPECtrum:PEAK:LIST?,
//! :BODE:MODE LOOP|PSRR|IMPedance,
//! :MAGnetic:ITEM? <item>[,V,I], :INVerter:ITEM? <item>[,src...], :DPT:ITEM? <item>[,V,I[,G]], :BANDwidth:ITEM? <item>[,src],
//! :WAVeform:SOURce CHANn|Rn|MODE NORM/MAX/RAW|FORMat BYTE/WORD/ASC|STARt|STOP|POINts|DATA?|PREamble?,
//! :SAVE:WAVeform <file.csv|file.bin> (the Storage menu's screen waveform; an empty path is the CSV button),
//! :SAVE:SETup [slot|file.stp] and :LOAD:SETup [slot|file.stp] (the Storage menu's setup; an empty path is the current slot),
//! :DISPlay:DATA? [BMP|PNG], :DISPlay:TYPE|GRADing:TIME|WBRightness|GBRightness|GRID|RULers|COLor,
//! :REFerence:SLOT R1..R10, :CURRent, :SOURce, :VSCale, :VOFFset, :COLor, :RESet, :LABel:CONTent, :LOAD <file>, :IMPort [file], :EXPort [file], :SAVE, :SHOW ON|OFF, :CLEar,
//! :MEASure:SETup:MAX|MIN|MID (the middle stays 50), :MEASure:THReshold:TYPE PERCent, :MEASure:THReshold:DEFault, :MEASure:SETup:PSA|PSB|DSA|DSB,
//! :MEASure:AMP:TYPE MANual (histogram), :MEASure:AMP:MANual:TOP|BASE HISTogram,
//! :SIM:PRESet <name>, :SIM:SLOT R1..R10, :SIM:PLAY ON|OFF, :SIM:SHOW ON|OFF,
//! :SIM:SSC:MODE OFF|DOWN|CENTER, :SIM:SSC:DEViation <fraction>, :SIM:SSC:FREQuency <Hz>,
//! :SIM:JITTer:DCD <seconds pp>|:CORRelated <seconds RMS>|:CORNer <Hz>, :SIM:PATTern PRBS7..31.
//! SIM configuration is applied together on the next :SIM:PLAY ON. :SIM:BUSY? reports background generation.
//! :DECode:{SOURce,SOURce2,CS,SOURce4,DIFFerential} names CH1..CH4/R1..R10 (NONE for optional roles).
//! :DECode:{PROTocol,STATe,SELect,THReshold,INPut} configures the selected decoder; EVENts? returns BUSY while
//! a long stored record is computing, and BUSY? polls without changing acquisition or statistics.
//! :DECode:CPHY:RATE {AUTO|symbols_per_second} sets a known C-PHY symbol rate; :DECode:SRATe is an alias.
//! :LA:STATe, :LA:DIGital<n>:DISPlay, :LA:POD<n>:THReshold, :LA:POINts?, :LA:PREamble?, :LA:DATA? (u16 LE words),
//! :COUNter:TOTalize:CLEar, :DISPlay:VIEW <n>, :DISPlay:VIEW:COUNt 1..4, :DISPlay:VIEW:MONitor 0..4 (HDMI Layout),
//! :SYSTem:ERRor?, :SYSTem:STATe?, :SYSTem:GRAPhics CPU|GPU, OOS: :VIEW SCOPe|SPECtrum|EYE|POWer|BODE|MAGnetic|INVerter|DPT, :IDENtify?, :CALibrate:STARt 0..4[,ZERO] / :STATus? /
//! :TRIGger:RS232|SPI|IIC:* (protocol trigger, scpi/ser.rs), OOS: :ACQuire:CORes[?] MATCh|RAW, :ACQuire:SEGMents[:HARDware|:INTerval], :ACQuire:AVERages:HARDware,
//! :SYSTem:ERRor?, :SYSTem:STATe?, :SYSTem:GRAPhics CPU|GPU, :SYSTem:AOUTput TOUT|OFF (rear AUX OUT; OFF is OOS), fork: :SYSTem:DEBug:DVM? (DVM block raw), :SYSTem:DEBug:MASK? (mask unit raw), :SYSTem:DEBug:MASK:MAP[?] key=value, :SYSTem:DEBug:MASK:COLumns? a,b, :SYSTem:DEBug:ZONE? (zone unit raw), :SYSTem:DEBug:SERial? / UART? (TPU UART, SPI, I2C words), OOS: :ACQuire:AVERages:HARDware ON|OFF (FPGA average), :ACQuire:SEGMents <n>|OFF, :SEGMents:HARDware ON|OFF, :SEGMents:INTerval <s>,
//! :SEGMents:TIMes? (hardware segments: trigger times since the first, s), :VIEW SCOPe|SPECtrum|EYE|POWer|BODE, :DISPlay:VIEW <n>, :DISPlay:VIEW:COUNt 1..4, :IDENtify?, :CALibrate:STARt 0..4[,ZERO] / :STATus? /
//! fork: :TRIGger:ZONE<n>:STATe|SOURce|MODE|PLACement|POINts|CLEar, :TRIGger:DELay:SA|SB|ASLop|BSLop|TYPE,
//! :TRIGger:SHOLd:DSRC|CSRC|TYPE, :TRIGger:DURation:TYPE, :TRIGger:SLOPe:POLarity|WINDow, :TRIGger:VIDeo:POLarity,
//! :RECord:CURRent|:WREPlay:FCURrent|:WREPlay:FMAX?, :NAVigate:FRAMe:NEXT|BACK|STARt|END, :SEARch:VALue?,
//! :CHANnel<n>:ZERO, :MATH<n>:RESet, :MEASure:STRip:DOCK, :MEASure:COUNter:ENABle|SOURce|VALue?,
//! :MEASure:AMP:TYPE|MANual:TOP|BASE, :MEASure:THReshold:TYPE|DEFault, :MEASure:AREA, :MEASure:SETup:MID, :CURSor:TRACk:MODE,
//! :CANCel, :BODE:RUN ON|OFF / :DATA? / :GENerator "ip:port",
//! built-in generator: :SOURce[1]:OUTPut[:STATe] ON|OFF, :FUNCtion SINusoid|SQUare|RAMP|PULSe|NOISe|DC|ARB, :ARB:DATA, :ARB:CAPTure, :FUNCtion:SQUare:DUTY, :FUNCtion:RAMP:SYMMetry, :MOD[:STATe], :MOD:TYPe AM|FM|PM, :MOD:AM:DEPTh,
//! :MOD:FM|PM:DEViation, :MOD:<type>:INTernal:FREQuency|FUNCtion, :FREQuency <Hz>,
//! :VOLTage[:AMPLitude] <Vpp>, :VOLTage:OFFSet <V> (all with queries)
//! Settings go through the same model as the screen, so the screen follows remote changes.
//!
//! The interpreter is this module. This file holds the session: the connections, a message (`Program`) with its
//! `*OPC` fences, the error queue, and the parsing helpers every subsystem shares. `exec_cmd` (scpi/dispatch.rs)
//! tries the headers in their fixed order and hands a subtree to the file of its subsystem (measure.rs, cursor.rs,
//! decode.rs, ...; the protocol trigger is ser.rs); the tests are in scpi/tests.rs and scpi/tests/. A new command
//! goes into the file of its subsystem, with its line in docs/scpi.md and its query in ui/tests/scpi.txt (AGENTS.md).

use crate::gfx::Canvas;
use crate::model::*;
use crate::ui::Ui;

pub use display::bmp;

mod analysis;
mod counter;
mod cursor;
mod decode;
mod dispatch;
mod display;
mod dvm;
mod histogram;
mod la;
mod mask;
mod math;
mod measure;
pub(crate) use measure::{meas_kind, pair_kind};
mod recorder;
mod reference;
mod replay;
mod ser;
mod setupscpi;
mod sim;
mod system;
mod timebase;
mod trigger;
mod waveform;

pub(crate) use measure::measurement_items;
#[cfg(test)]
use std::io::Read;
use std::net::{TcpListener, TcpStream};

/// One bounded SCPI message, retained by its transport while *OPC? is pending.
#[derive(Default)]
pub struct Program {
    commands: std::collections::VecDeque<(String, bool)>,
    out: Vec<u8>,
    waiting: Option<crate::ui::Operations>,
    separator: u8,
    pub bytes: usize,
    /// A completed query may legitimately produce zero payload bytes.
    pub has_reply: bool,
    invalid: bool,
}
impl Program {
    pub fn new(text: &str) -> Self {
        if text.len() > MAX_LINE {
            return Self { invalid: true, ..Self::default() };
        }
        let mut commands = std::collections::VecDeque::new();
        for (command, first) in split_message(text) {
            if commands.len() >= 1024 {
                return Self { invalid: true, ..Self::default() };
            }
            commands.push_back((command, first));
        }
        Self { commands, separator: b';', bytes: text.len(), ..Self::default() }
    }
    fn append(&mut self, reply: &[u8]) {
        if self.has_reply {
            self.out.push(self.separator);
        }
        self.out.extend_from_slice(reply);
        self.separator = b';';
        self.has_reply = true;
    }
    /// Adapter used by transport unit fixtures with a synchronous test interpreter.
    #[cfg(test)]
    pub fn immediate(&mut self, exec: &mut dyn FnMut(&str) -> Vec<u8>) -> Vec<u8> {
        while let Some((command, new_line)) = self.commands.pop_front() {
            if new_line && self.has_reply {
                self.separator = b'\n';
            }
            let reply = exec(&command);
            if !reply.is_empty() || command.split_whitespace().next().is_some_and(|header| header.ends_with('?')) {
                self.append(&reply);
            }
        }
        std::mem::take(&mut self.out)
    }
}

// what a transport of oos-remote hands to this interpreter (ADR 0022, ADR 0064)
impl oos_remote::Program for Program {
    fn new(text: &str) -> Self {
        Program::new(text)
    }
    fn bytes(&self) -> usize {
        self.bytes
    }
    fn has_reply(&self) -> bool {
        self.has_reply
    }
}

struct Client {
    s: TcpStream,
    buf: Vec<u8>,
    program: Option<Program>,
    /// the replies not yet written (#192): its next command runs once they are out
    out: crate::net::Outbox,
    /// the client closed its side: it is dropped once its replies are out
    closing: bool,
}

impl Client {
    /// Writes what the socket takes now of the replies not yet written.
    fn flush(&mut self) -> crate::net::Sent {
        self.out.flush(&self.s)
    }
}

/// Longest command line (`:SOURce:ARB:DATA` with its samples is the longest there is); a client past it is dropped.
const MAX_LINE: usize = 4 << 20;

/// The `#<n><length><data>` definite-length block that starts at `i` (a `#` after a blank, a comma or the header):
/// the index just after its data. `None`: not a block, or its data is not all there yet (`complete` tells which).
fn block_span(b: &[u8], i: usize) -> Option<(usize, bool)> {
    if b.get(i) != Some(&b'#') || i == 0 || !matches!(b[i - 1], b' ' | b'\t' | b',') { return None; }
    let n = (*b.get(i + 1)? as char).to_digit(10).filter(|&n| n >= 1)? as usize;
    let digits = b.get(i + 2..i + 2 + n);
    let Some(digits) = digits else { return Some((b.len(), false)) };
    let len: usize = std::str::from_utf8(digits).ok()?.parse().ok().filter(|&l| l <= MAX_LINE)?;
    let end = i + 2 + n + len;
    Some((end.min(b.len()), end <= b.len()))
}

/// The end of the first message in `buf`: the newline that is not inside a block. `None`: not complete yet.
pub(crate) fn message_end(buf: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i < buf.len() {
        match buf[i] {
            b'\n' => return Some(i),
            b'#' => match block_span(buf, i) {
                Some((_, false)) => return None,
                Some((end, true)) => { i = end; continue; }
                None => {}
            },
            _ => {}
        }
        i += 1;
    }
    None
}

/// Splits a program message into its commands: at newlines and semicolons, but not inside a definite-length block. The flag
/// is set for the first command of each line.
fn split_message(text: &str) -> Vec<(String, bool)> {
    let b = text.as_bytes();
    let (mut out, mut start, mut first, mut i) = (Vec::new(), 0, true, 0);
    // where the current line begins: a comment is a line that begins with `#`
    let mut line_start = 0;
    let mut flush = |from: usize, to: usize, first: &mut bool| {
        let c = text[from..to].trim();
        if !c.is_empty() { out.push((c.to_string(), *first)); *first = false; }
    };
    while i < b.len() {
        match b[i] {
            b';' => { flush(start, i, &mut first); start = i + 1; }
            b'\n' | b'\r' => { flush(start, i, &mut first); start = i + 1; line_start = i + 1; first = true; }
            // a comment line of a file of commands (a recording, scpi/recorder.rs), sent as it is: it is no command,
            // to the end of its line and with a `;` in it. Only at the start of a line: after a `;` a `#` is part of a
            // command. A block is an argument and never starts a command.
            b'#' if text[line_start..i].trim().is_empty() => {
                while i < b.len() && b[i] != b'\n' && b[i] != b'\r' { i += 1; }
                start = i;
                continue;
            }
            b'#' => if let Some((end, true)) = block_span(b, i) {
                if text.is_char_boundary(end) { i = end; continue; }
            },
            _ => {}
        }
        i += 1;
    }
    flush(start, b.len(), &mut first);
    out
}
/// Connections at once.
const MAX_CLIENTS: usize = 8;
pub use crate::net::RESP_MAX;
const ERR_MAX: usize = 20;
const ERR_LEN: usize = 256;

pub struct Scpi {
    l: Option<TcpListener>,
    clients: Vec<Client>,
    /// when a client that does not take its replies is given up (`net::Limits`); a test sets them far out
    send_limits: crate::net::Limits,
    /// :WAVeform:SOURce, a channel or a reference slot
    wav_src: WaveId,
    /// `:WAVeform:SOURce MATH<n>`: the math channel read instead of `wav_src`
    wav_math: Option<usize>,
    wav_fmt: u8, // 0 BYTE, 1 WORD, 2 ASCII
    /// :WAVeform:MODE: 0 NORMal (screen), 1 MAXimum (memory while stopped), 2 RAW (memory)
    wav_mode: u8,
    /// :WAVeform:STARt and :WAVeform:STOP, 1-based inclusive. `stop == 0` reads through the last point.
    wav_start: u64,
    wav_stop: u64,
    err: Vec<String>,
    /// who may connect (`netacl`, net.allow; empty: everyone): new connections, and after a change those held
    pub acl: crate::netacl::Shared,
    /// the changes of `acl` looked at (`netacl::Shared::changed`)
    acl_seen: u64,
    event_status: u8,
    /// `*ESE`, `*SRE` and `*PSC`: the enable registers of IEEE 488.2 and the power-on status clear flag
    event_enable: u8,
    service_enable: u8,
    power_on_clear: bool,
    error_serial: u64,
    last_error: Option<String>,
    opc_events: Vec<crate::ui::Operations>,
    /// The search event last jumped to, 1-based. 0 means none.
    search_at: u32,
    /// The SCPI recorder while it is on (scpi/recorder.rs), and the lines of the recording before.
    rec: Option<Box<recorder::Recorder>>,
    rec_last: Vec<String>,
    /// The file of commands that runs (scpi/replay.rs).
    replay: Option<Box<replay::Replay>>,
    /// The command that runs is one of that file, not a client's: `replay::file_refuses` says what it may not send.
    from_file: bool,
    /// The CSV files of the last MCP `waveform` calls (at most `site::mcp::FILES_MAX`), each with its name and when
    /// it was made, served as `GET /mcp/waveform/<name>` for `site::mcp::FILE_TTL` (site/mcp.rs).
    pub(crate) mcp_files: Vec<(String, std::time::Instant, Vec<u8>)>,
    /// The calls of `waveform` so far, the number in the name of each file.
    pub(crate) mcp_calls: u32,
}

pub(crate) use crate::words::{e, kw, nearest_index, num, num64, parse_bool};

/// The answer of a query that is the text `s`, as the subsystems return it.
fn t(s: &str) -> Option<Vec<u8>> {
    Some(s.as_bytes().to_vec())
}

/// Splits a keyword with a numeric suffix: "CHAN2" -> ("CHAN", 2).
pub(crate) fn suffix(tok: &str) -> (&str, Option<usize>) {
    let i = tok.find(|c: char| c.is_ascii_digit()).unwrap_or(tok.len());
    (&tok[..i], tok[i..].parse().ok())
}

pub(crate) fn parse_ch(s: &str) -> Option<usize> {
    let (k, n) = suffix(s.trim());
    if kw(k, "CHANnel") {
        n.filter(|&n| (1..=NCH).contains(&n)).map(|n| n - 1)
    } else {
        None
    }
}

fn arg_item(arg: &str) -> String {
    arg.split(',').next().unwrap_or("").trim().to_ascii_uppercase()
}

fn arg_rest(arg: &str) -> &str {
    arg.split_once(',').map(|(_, r)| r).unwrap_or("")
}

/// A measurement source: CHANn, or a ref slot R1–R10 (also REF1–REF10).
pub(crate) fn parse_wave(s: &str) -> Option<WaveId> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(c) = parse_ch(s) {
        return Some(WaveId::Ch(c));
    }
    let (k, n) = suffix(s);
    if kw(k, "REF") || k.eq_ignore_ascii_case("R") {
        return n.filter(|&n| (1..=NREF).contains(&n)).map(|n| WaveId::Ref(n - 1));
    }
    None
}

pub(crate) fn wave_tok(id: WaveId) -> String {
    match id {
        WaveId::Ch(c) => format!("CHAN{}", c + 1),
        WaveId::Ref(i) => format!("R{}", i + 1),
    }
}

/// A word that is written like a reference slot (`R12`, `REF0`) but names none: a value such as `RED` is not one.
pub(crate) fn slot_like(s: &str) -> bool {
    let up = s.trim().to_ascii_uppercase();
    up.strip_prefix("REF").or_else(|| up.strip_prefix('R')).is_some_and(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
}

fn parse_slot(s: &str) -> Option<usize> {
    let s = s.trim().trim_matches('"');
    if let Some(WaveId::Ref(i)) = parse_wave(s) {
        return Some(i);
    }
    s.parse::<usize>().ok().filter(|&n| (1..=NREF).contains(&n)).map(|n| n - 1)
}

pub(crate) use crate::model::nearest_step as nearest;

/// A value of a query: a number in the stock format, 9.9E37 where there is none (`e` writes that for NaN too).
pub(crate) fn shown(v: f64) -> String {
    e(v)
}

impl Scpi {
    pub fn open(port: u16) -> Scpi {
        Scpi { l: Self::bind(port), ..Self::detached() }
    }

    /// An interpreter no client reaches: the second scope the set-up as commands is proven on (scpi/setupscpi.rs),
    /// and the one a script of the command line runs in.
    pub(crate) fn detached() -> Scpi {
        Scpi {
            l: None,
            clients: Vec::new(),
            send_limits: Default::default(),
            wav_src: WaveId::Ch(0),
            wav_math: None,
            wav_fmt: 0,
            wav_mode: 0,
            wav_start: 1,
            wav_stop: 0,
            err: Vec::new(),
            search_at: 0,
            event_status: 0,
            event_enable: 0,
            service_enable: 0,
            power_on_clear: true,
            error_serial: 0,
            last_error: None,
            opc_events: Vec::new(),
            acl: crate::netacl::Shared::default(),
            acl_seen: 0,
            rec: None,
            rec_last: Vec::new(),
            replay: None,
            from_file: false,
            mcp_files: Vec::new(),
            mcp_calls: 0,
        }
    }

    fn bind(port: u16) -> Option<TcpListener> {
        let l = crate::net6::tcp_service(port, "SCPI");
        if l.is_none() {
            eprintln!("oos-ui: SCPI port {} not available", port);
        }
        l
    }

    /// A port that was still taken when it was opened (the oos-ui before this one, ending, #212): taken now if it is
    /// free. True when that happened.
    pub fn listen_again(&mut self, port: u16) -> bool {
        if self.l.is_some() {
            return false;
        }
        self.l = crate::net6::tcp_service(port, "SCPI");
        self.l.is_some()
    }

    /// A row of the settings table (settings.rs) answers the query or takes the argument, which marks the model
    /// changed; a value it does not take is refused with -224.
    fn setting_cmd(&mut self, ui: &mut Ui, row: &crate::settings::Setting, query: bool, arg: &str) -> Option<Vec<u8>> {
        if query {
            return Some(row.query(&ui.m, 0).into_bytes());
        }
        if !row.take(&mut ui.m, 0, arg) {
            self.push_err(format!("-224,\"Illegal parameter value\" {}", arg));
            return None;
        }
        ui.mark_changed();
        None
    }

    /// A command that sets one value: `v` is its argument parsed, which `set` takes into `ui` and marks the model
    /// changed; an argument that did not parse (`None`) is refused with -224 and changes nothing.
    fn set_arg<T>(&mut self, ui: &mut Ui, arg: &str, v: Option<T>, set: impl FnOnce(&mut Ui, T)) -> Option<Vec<u8>> {
        match v {
            Some(v) => {
                set(ui, v);
                ui.mark_changed();
            }
            None => self.push_err(format!("-224,\"Illegal parameter value\" {}", arg)),
        }
        None
    }

    /// The commands every meter has (`:COUNter`, `:DVM`, `:MASK`): the rows of its table (`slices`) and `SOURce`,
    /// the channel `src` of the model. None when `n` names neither.
    fn meter_cmd(
        &mut self,
        ui: &mut Ui,
        slices: &[&'static [crate::settings::Setting]],
        n: &[&str],
        query: bool,
        arg: &str,
        src: fn(&mut Model) -> &mut usize,
    ) -> Option<Option<Vec<u8>>> {
        if let Some(row) = crate::settings::command(slices, n) {
            return Some(self.setting_cmd(ui, row, query, arg));
        }
        if n.len() != 1 || !kw(n[0], "SOURce") {
            return None;
        }
        if query {
            return Some(t(&format!("CHAN{}", (*src(&mut ui.m)).min(n_ch() - 1) + 1)));
        }
        Some(self.set_arg(ui, arg, parse_ch(arg), |ui, c| *src(&mut ui.m) = c))
    }

    /// Queue a SCPI error and latch the matching *ESR bit. A query that fails still answers with this text,
    /// so a client blocked on a line never waits forever.
    fn push_err(&mut self, mut message: String) {
        if message.len() > ERR_LEN {
            let mut end = ERR_LEN;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
        }
        let code = message.split(',').next().unwrap_or("").trim().parse::<i32>().unwrap_or(0);
        self.event_status |= match code {
            -199..=-100 => 32,
            -299..=-200 => 16,
            -399..=-300 => 8,
            -499..=-400 => 4,
            _ => 16,
        };
        self.error_serial = self.error_serial.wrapping_add(1);
        self.last_error = Some(message.clone());
        if self.err.len() < ERR_MAX {
            self.err.push(message);
        } else {
            self.err[ERR_MAX - 1] = "-350,\"Queue overflow\"".into();
            self.event_status |= 8;
        }
    }

    pub fn port(&self) -> Option<u16> {
        self.l.as_ref().and_then(|l| l.local_addr().ok()).map(|a| a.port())
    }

    /// System > Network > Remote services: the port open (`Some`), or closed with its connections.
    pub fn set_open(&mut self, port: Option<u16>) {
        match port {
            Some(p) if self.l.is_none() => self.l = Self::bind(p),
            Some(_) => {}
            None => {
                if self.l.take().is_some() {
                    eprintln!("oos-ui: SCPI off");
                }
                self.clients.clear();
            }
        }
    }

    pub fn poll(&mut self, ui: &mut Ui, cv: &Canvas) {
        // Storage > Save setup as SCPI: the row asks, this interpreter has the queries the lines are made of
        if let Some(path) = ui.setup_scpi_request.take() {
            let (text, rest) = self.setup_scpi_proven(ui, cv);
            ui.scpi_write(&path, &text, setupscpi::named(&rest).map(|keys| format!("without {keys}")).as_deref());
        }
        // the SCPI recorder looks at the set-up here, before a client's message is run: what it sees is the user's
        self.rec_tick(ui, cv);
        // Storage > Run SCPI file: the next command of the file, as a client's
        self.replay_tick(ui, cv);
        if let Some(l) = &self.l {
            let (clients, limits) = (&mut self.clients, self.send_limits);
            // each connection costs a read per pass of the frame loop, one that does not wait (`net::read_some`)
            crate::net::accept_from(l, &self.acl, |s| {
                if clients.len() < MAX_CLIENTS {
                    clients.push(Client {
                        s,
                        buf: Vec::new(),
                        program: None,
                        out: crate::net::Outbox::with(limits),
                        closing: false,
                    });
                }
            });
        }
        // net.allow changed on the screen: a client the list now leaves out is closed, as at `accept`
        if self.acl.changed(&mut self.acl_seen) {
            let acl = &self.acl;
            self.clients.retain(|c| acl.permits_peer(c.s.peer_addr()));
        }
        let mut i = 0;
        while i < self.clients.len() {
            // what is still to be written goes first, as much as the socket takes now (#192)
            if self.clients[i].flush() == crate::net::Sent::Gone {
                self.clients.remove(i);
                continue;
            }
            let mut closed = false;
            // the client sent its last bytes (`echo "*IDN?" | nc host 5555` closes its side after the line): what came
            // before is still answered, as in 0.1.12 and on the stock system; nothing more is read
            let mut eof = self.clients[i].closing;
            if !eof {
                let c = &mut self.clients[i];
                let end = crate::net::read_some(&c.s, &mut c.buf, MAX_LINE, 4096, 65536);
                (eof, closed) = (end.eof, end.closed);
            }
            // a program waiting for an operation (`*OPC?`) when the client went is not resumed: no later command of it runs
            if eof && self.clients[i].program.is_some() {
                closed = true;
            }
            // a client still taking a reply gets its next one after it, so its replies keep their order and do not pile up
            while !closed && self.clients[i].out.is_empty() {
                if self.clients[i].program.is_none() {
                    let Some(p) = message_end(&self.clients[i].buf) else {
                        break;
                    };
                    let line: Vec<u8> = self.clients[i].buf.drain(..=p).collect();
                    self.clients[i].program = Some(Program::new(&String::from_utf8_lossy(&line)));
                }
                // there is one: kept from before, or made from the line just taken
                let Some(mut program) = self.clients[i].program.take() else { break };
                let Some(mut response) = self.poll_program(ui, cv, &mut program) else {
                    // after the end of the input a program that would have to wait is dropped, and no later line runs
                    if eof {
                        self.clients[i].buf.clear();
                    } else {
                        self.clients[i].program = Some(program);
                    }
                    break;
                };
                if program.has_reply {
                    response.push(b'\n');
                    self.clients[i].out.push(&response);
                    closed = self.clients[i].flush() == crate::net::Sent::Gone;
                }
            }
            let c = &mut self.clients[i];
            // at the end of its input a client leaves once every line it sent is answered and the replies are out
            let ended = eof && (!c.out.is_empty() || c.buf.contains(&b'\n'));
            if closed || c.buf.len() > MAX_LINE || (eof && !ended) {
                self.clients.remove(i);
            } else {
                c.closing = ended;
                i += 1;
            }
        }
    }

    /// Immediate adapter for HTTP and in-process callers (the desktop build, a set-up of SCPI lines, the tests).
    /// Deferred barriers must use `poll_program`: returning 1 here while an operation is pending would be false.
    /// Commands before a pending barrier execute once; the remaining message is rejected.
    pub fn exec_line(&mut self, ui: &mut Ui, cv: &Canvas, line: &str) -> Vec<u8> {
        self.exec_line_checked(ui, cv, line).unwrap_or_default()
    }

    pub fn exec_line_checked(&mut self, ui: &mut Ui, cv: &Canvas, line: &str) -> Result<Vec<u8>, &'static str> {
        self.exec_message_checked(ui, cv, line).map(|(response, _)| response)
    }

    /// The errors that are queued, oldest first; they stay queued (`:SYSTem:ERRor?` takes them).
    pub fn queued_errors(&self) -> &[String] {
        &self.err
    }

    pub fn exec_message_checked(
        &mut self,
        ui: &mut Ui,
        cv: &Canvas,
        line: &str,
    ) -> Result<(Vec<u8>, bool), &'static str> {
        let mut program = Program::new(line);
        match self.poll_program(ui, cv, &mut program) {
            Some(response) => Ok((response, program.has_reply)),
            None => {
                let error = "Operation pending; use raw TCP or HiSLIP for *OPC?";
                self.push_err(format!("-221,\"{}\"", error));
                self.normalize_errors();
                Err(error)
            }
        }
    }

    fn poll_events(&mut self, ui: &Ui) {
        self.opc_events.retain(|fence| match fence.state(ui) {
            crate::ui::OperationState::Pending => true,
            crate::ui::OperationState::Complete => {
                self.event_status |= 1;
                false
            }
            crate::ui::OperationState::Cancelled => false,
        });
    }

    /// Advance a bounded per-client message without waiting on the UI thread.
    /// The continuation owns its earlier replies and remaining commands: no write is replayed.
    pub fn poll_program(&mut self, ui: &mut Ui, cv: &Canvas, program: &mut Program) -> Option<Vec<u8>> {
        let before = self.rec_before_remote(ui);
        let actions = ui.scpi_rec_actions.len();
        let response = self.advance_program(ui, cv, program);
        // a client's :AUToscale, :CLEar, :TFORce or :SAVE:IMAGe runs the code of the key and the row: it is not the
        // user's action
        ui.scpi_rec_actions.truncate(actions);
        if let Some(before) = before { self.rec_after_remote(ui, cv, before); }
        self.normalize_errors();
        response
    }

    fn append_program_reply(&mut self, program: &mut Program, response: &[u8]) -> bool {
        let separator = usize::from(program.has_reply);
        if response.len() > RESP_MAX.saturating_sub(program.out.len().saturating_add(separator)) {
            self.push_err("-363,\"Output buffer overrun; remaining commands dropped\"".into());
            program.commands.clear();
            return false;
        }
        program.append(response);
        true
    }

    fn advance_program(&mut self, ui: &mut Ui, cv: &Canvas, program: &mut Program) -> Option<Vec<u8>> {
        self.poll_events(ui);
        if program.invalid {
            self.push_err("-363,\"Program message too large or too many commands\"".into());
            return Some(Vec::new());
        }
        if let Some(fence) = &program.waiting {
            match fence.state(ui) {
                crate::ui::OperationState::Pending => return None,
                crate::ui::OperationState::Complete => {
                    program.waiting = None;
                    if !self.append_program_reply(program, b"1") {
                        return Some(std::mem::take(&mut program.out));
                    }
                }
                crate::ui::OperationState::Cancelled => {
                    self.push_err("-410,\"OPC query interrupted by cancelled operation\"".into());
                    program.commands.clear();
                    program.out.clear();
                    program.has_reply = false;
                    program.waiting = None;
                    return Some(Vec::new());
                }
            }
        }
        while let Some((command, new_line)) = program.commands.pop_front() {
            if new_line && program.has_reply {
                program.separator = b'\n';
            }
            if program.out.len() >= RESP_MAX {
                self.push_err("-363,\"Input buffer overrun\" rest of the line dropped".into());
                program.commands.clear();
                break;
            }
            if command.eq_ignore_ascii_case("*OPC?") {
                let fence = ui.scpi_operations();
                if fence.state(ui) == crate::ui::OperationState::Pending {
                    program.waiting = Some(fence);
                    return None;
                }
                if !self.append_program_reply(program, b"1") {
                    break;
                }
            } else if let Some(response) = self.exec(ui, cv, &command) {
                if !self.append_program_reply(program, &response) {
                    break;
                }
            }
        }
        Some(std::mem::take(&mut program.out))
    }

    fn exec(&mut self, ui: &mut Ui, cv: &Canvas, cmd: &str) -> Option<Vec<u8>> {
        if let Some(error) = ui.take_sim_error() {
            self.push_err(format!("-224,\"SIM generation failed\" {error}"));
        }
        let before = self.error_serial;
        let r = self.exec_cmd(ui, cv, cmd);
        // a query with no payload would leave the client blocked on the next line: answer with the error instead
        let query = cmd.split_whitespace().next().unwrap_or("").ends_with('?');
        let r = if query && r.is_none() {
            if self.error_serial == before {
                self.push_err(format!("-113,\"Undefined header\" {}", cmd));
            }
            self.last_error.as_ref().map(|error| error.as_bytes().to_vec())
        } else {
            r
        };
        self.normalize_errors();
        r
    }

    fn normalize_errors(&mut self) {
        // a client that never asks for its errors, or sends megabytes of nonsense, must not fill the memory
        for e in &mut self.err {
            if e.len() > ERR_LEN {
                *e = e.chars().take(ERR_LEN).collect();
            }
        }
        if self.err.len() > ERR_MAX {
            self.err.truncate(ERR_MAX - 1);
            self.push_err("-350,\"Queue overflow\"".into());
        }
    }
}

/// IEEE 488.2 definite-length block: #9 + 9 digits + data.
pub(crate) fn block(d: &[u8]) -> Vec<u8> {
    let mut o = format!("#9{:09}", d.len()).into_bytes();
    o.extend_from_slice(d);
    o
}

#[cfg(test)]
mod tests;
