// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The scope as a USBTMC instrument (USB488 subclass) on its rear USB port, for VISA and libusb on the computer at
//! the other end of the cable (`USB::0x1209::<pid>::<serial>::INSTR`; docs/usb-device.md, ADR 0080). The interface is
//! a FunctionFS function of the gadget `/bin/oos-gadget` builds; this module is its user-space side.
//!
//! What is here is the protocol, without endpoints: the descriptors and strings that are written to `ep0` before the
//! gadget is bound, the class requests on the control endpoint (USBTMC 1.0 chapter 4, USB488 1.0 chapter 4), and the
//! bulk messages with their tags (USBTMC 1.0 chapter 3): `Device` takes the bytes the Bulk-OUT endpoint delivered and
//! the setup packets of `ep0`, and gives the bytes for Bulk-IN, Interrupt-IN and the answers of the control requests.
//! The tests feed bytes and check bytes. The files of the FunctionFS mount and the threads that block on them are in
//! `ffs` (the submodule), the frame loop's side is `Server`.
//!
//! Like VXI-11 and HiSLIP this is a transport only (ADR 0022): a complete message (DEV_DEP_MSG_OUT up to its EOM)
//! becomes a `Program` of the one SCPI interpreter, and its answer, with a newline, is what the following
//! REQUEST_DEV_DEP_MSG_IN transfers return, each at most what the host asked for. The frame loop never waits for the
//! computer (ADR 0066): the endpoints are read and written by threads of their own.

use crate::Program;
use oos_system::usbtmc::{self as tmc, HEADER};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

// oos-plus: the endpoints are files of a Unix; the Windows desktop build has no rear USB port and never binds
#[cfg(unix)]
pub mod ffs;

/// Where `/bin/oos-gadget` mounts the FunctionFS of the function `ffs.usbtmc`.
pub const MOUNT: &str = "/dev/usbtmc-ffs";

// --------------------------------------------------------------------------------------------------- descriptors

/// The interface: class 0xFE (application specific), subclass 0x03 (USBTMC), protocol 0x01 (USB488).
pub const PROTOCOL: u8 = 0x01;
/// The endpoints as FunctionFS numbers them (`ep1` .. `ep3`, in the order of the descriptors); a setup packet to an
/// endpoint carries this number in wIndex.
pub const EP_OUT: u16 = 1;
pub const EP_IN: u16 = 2;
/// Interrupt-IN, for the status byte (READ_STATUS_BYTE): two bytes, polled every millisecond.
const INTR_SIZE: u16 = 2;
/// Windows binds its WinUSB driver to the interface by this compatible ID (Microsoft OS 1.0 descriptors), and
/// programs find it by this interface GUID. The GUID is this interface's for good: it never changes.
pub const COMPATIBLE_ID: &[u8; 8] = b"WINUSB\0\0";
pub const INTERFACE_GUID: &str = "{D904622F-1F2F-469B-AE0F-FC6A5CD8F693}";
/// The name of the interface (string 1).
pub const INTERFACE_NAME: &str = "OOS USBTMC";

const DESCRIPTORS_MAGIC_V2: u32 = 3;
const STRINGS_MAGIC: u32 = 2;
const HAS_FS_DESC: u32 = 1;
const HAS_HS_DESC: u32 = 2;
const HAS_MS_OS_DESC: u32 = 8;
/// MS OS 1.0 feature descriptors: extended compatible ID (wIndex 4), extended properties (wIndex 5); REG_SZ.
const OS_EXT_COMPAT: u16 = 4;
const OS_EXT_PROP: u16 = 5;
const REG_SZ: u32 = 1;

fn le16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}

fn le32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// The interface and its three endpoints at one speed: bulk packets of `bulk` bytes, the interrupt endpoint polled
/// every `interval` (full speed: in ms; high speed: 2^(interval-1) microframes).
fn interface(v: &mut Vec<u8>, bulk: u16, interval: u8) {
    v.extend_from_slice(&[9, 4, 0, 0, 3, tmc::CLASS, tmc::SUBCLASS, PROTOCOL, 1]);
    for (address, kind, size, every) in [(0x01, 2, bulk, 0), (0x82, 2, bulk, 0), (0x83, 3, INTR_SIZE, interval)] {
        v.extend_from_slice(&[7, 5, address, kind]);
        le16(v, size);
        v.push(every);
    }
}

/// What is written first to `ep0` (FunctionFS, `include/uapi/linux/usb/functionfs.h`): the V2 header with the
/// counts, the descriptors for full and high speed (the rear port is USB 2.0: no super speed), and the Microsoft OS
/// descriptors: the compatible ID `WINUSB` and the property `DeviceInterfaceGUID`. Linux's composite gadget sends a
/// property of type REG_SZ but not REG_MULTI_SZ, so the GUID is the single-string form (`DeviceInterfaceGUID`, not
/// `...GUIDs`). FunctionFS gives these to the host only when the gadget has `os_desc` on, for the configuration
/// linked there (configuration 1, the one Windows takes).
pub fn descriptors() -> Vec<u8> {
    let mut v = Vec::new();
    le32(&mut v, DESCRIPTORS_MAGIC_V2);
    le32(&mut v, 0); // the length, below
    le32(&mut v, HAS_FS_DESC | HAS_HS_DESC | HAS_MS_OS_DESC);
    le32(&mut v, 4); // full speed: interface and three endpoints
    le32(&mut v, 4); // high speed
    le32(&mut v, 2); // OS descriptors: compatible ID, property
    interface(&mut v, 64, 1);
    interface(&mut v, 512, 4);
    // extended compatible ID: the header (interface, dwLength, bcdVersion 1.00, wIndex, bCount, reserved), then one
    // function: first interface, reserved 1, compatible ID, sub-compatible ID, six reserved
    v.push(0);
    le32(&mut v, 11 + 24);
    le16(&mut v, 0x0100);
    le16(&mut v, OS_EXT_COMPAT);
    v.extend_from_slice(&[1, 0]);
    v.extend_from_slice(&[0, 1]);
    v.extend_from_slice(COMPATIBLE_ID);
    v.extend_from_slice(&[0; 8 + 6]);
    // extended property: the header (interface, dwLength, bcdVersion, wIndex, wCount), then dwSize, the type, the
    // name and the value, both NUL-terminated in UTF-8 here (the kernel sends them in UTF-16)
    let name = b"DeviceInterfaceGUID\0";
    let value = format!("{}\0", INTERFACE_GUID).into_bytes();
    let size = 14 + name.len() + value.len();
    v.push(0);
    le32(&mut v, (11 + size) as u32);
    le16(&mut v, 0x0100);
    le16(&mut v, OS_EXT_PROP);
    le16(&mut v, 1);
    le32(&mut v, size as u32);
    le32(&mut v, REG_SZ);
    le16(&mut v, name.len() as u16);
    v.extend_from_slice(name);
    le32(&mut v, value.len() as u32);
    v.extend_from_slice(&value);
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&n.to_le_bytes());
    v
}

/// What is written to `ep0` second: one language (US English), one string, the interface's name.
pub fn strings() -> Vec<u8> {
    let mut v = Vec::new();
    le32(&mut v, STRINGS_MAGIC);
    le32(&mut v, 0);
    le32(&mut v, 1); // strings
    le32(&mut v, 1); // languages
    le16(&mut v, 0x0409);
    v.extend_from_slice(INTERFACE_NAME.as_bytes());
    v.push(0);
    let n = v.len() as u32;
    v[4..8].copy_from_slice(&n.to_le_bytes());
    v
}

// --------------------------------------------------------------------------------------------------- ep0 events

/// A setup packet (USB 2.0 table 9-2).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Setup {
    pub request_type: u8,
    pub request: u8,
    pub value: u16,
    pub index: u16,
    pub length: u16,
}

impl Setup {
    /// Whether the data stage goes to the host (an answer is written to `ep0`; reading `ep0` stalls instead).
    pub fn to_host(&self) -> bool {
        self.request_type & 0x80 != 0
    }
}

/// What reading `ep0` delivers (`struct usb_functionfs_event`, 12 bytes each).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    Bind,
    Unbind,
    /// the host chose a configuration with this interface: the endpoints work
    Enable,
    /// the configuration ended (unplugged, another one chosen, reset)
    Disable,
    Setup(Setup),
    Suspend,
    Resume,
    Other(u8),
}

/// Size of one event.
pub const EVENT: usize = 12;

/// The events in what a read of `ep0` gave; a rest shorter than an event is left out.
pub fn events(b: &[u8]) -> Vec<Event> {
    b.as_chunks::<EVENT>()
        .0
        .iter()
        .map(|e| match e[8] {
            0 => Event::Bind,
            1 => Event::Unbind,
            2 => Event::Enable,
            3 => Event::Disable,
            4 => Event::Setup(Setup {
                request_type: e[0],
                request: e[1],
                value: u16::from_le_bytes([e[2], e[3]]),
                index: u16::from_le_bytes([e[4], e[5]]),
                length: u16::from_le_bytes([e[6], e[7]]),
            }),
            5 => Event::Suspend,
            6 => Event::Resume,
            t => Event::Other(t),
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ the protocol

// class requests: USBTMC 1.0 table 15, USB488 1.0 table 9
const INITIATE_ABORT_BULK_OUT: u8 = 1;
const CHECK_ABORT_BULK_OUT_STATUS: u8 = 2;
const INITIATE_ABORT_BULK_IN: u8 = 3;
const CHECK_ABORT_BULK_IN_STATUS: u8 = 4;
const INITIATE_CLEAR: u8 = 5;
const CHECK_CLEAR_STATUS: u8 = 6;
const GET_CAPABILITIES: u8 = 7;
const READ_STATUS_BYTE: u8 = 128;
/// bmRequestType of the class requests: to the host, class, to the interface or to an endpoint.
const TO_INTERFACE: u8 = 0xa1;
const TO_ENDPOINT: u8 = 0xa2;

// USBTMC_status (USBTMC 1.0 table 16)
const STATUS_SUCCESS: u8 = 0x01;
const STATUS_PENDING: u8 = 0x02;
const STATUS_FAILED: u8 = 0x80;
const STATUS_TRANSFER_NOT_IN_PROGRESS: u8 = 0x81;

// MsgIDs on Bulk-OUT besides DEV_DEP_MSG_OUT and REQUEST_DEV_DEP_MSG_IN (USBTMC table 2, USB488 table 3)
const VENDOR_SPECIFIC_OUT: u8 = 126;
const TRIGGER: u8 = 128;

/// The answer to GET_CAPABILITIES (USBTMC table 37 with USB488 table 8): USBTMC 1.00, neither talk-only nor
/// listen-only, no INDICATOR_PULSE, no TermChar; USB488 1.00, a 488.2 interface without REN_CONTROL, GO_TO_LOCAL and
/// LOCAL_LOCKOUT and without TRIGGER, a device that speaks SCPI but has no service request (SR0), remote/local (RL0)
/// or device trigger (DT0). The requests it does not offer are stalled.
pub const CAPABILITIES: [u8; 0x18] = [
    STATUS_SUCCESS,
    0,
    0x00,
    0x01, // bcdUSBTMC 1.00
    0,    // interface: no INDICATOR_PULSE, not talk-only, not listen-only
    0,    // device: no TermChar
    0,
    0,
    0,
    0,
    0,
    0,
    0x00,
    0x01, // bcdUSB488 1.00
    0x04, // 488.2 interface; no REN_CONTROL / GO_TO_LOCAL / LOCAL_LOCKOUT, no TRIGGER
    0x08, // SCPI; SR0, RL0, DT0
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    0,
];

/// Status byte bits (IEEE 488.2 11.2): the error queue is not empty (SCPI's EAV), a message is available (MAV).
pub const STB_EAV: u8 = 0x04;
pub const STB_MAV: u8 = 0x10;

/// The longest program message, as one SCPI line or one HiSLIP message (`:SOURce:ARB:DATA`): a longer one halts
/// Bulk-OUT and is dropped.
pub const MAX_MESSAGE: usize = 4 << 20;
/// Messages waiting for the interpreter: past this the Bulk-OUT endpoint is not read (the host's transfers wait).
const MAX_INBOX: usize = 16;
/// Data bytes in one Bulk-IN transfer, whatever the host asked for: a larger answer takes more requests, each a
/// transfer of this size (the message goes on until EOM). Keeps one copy of a transfer small.
pub const MAX_IN_DATA: usize = 1 << 20;
/// Bytes asked for in one read of Bulk-OUT.
pub const MAX_READ: usize = 64 << 10;
/// The full-speed packet size; high speed's 512 is a multiple of it.
const FS_PACKET: usize = 64;

/// The answer to a control request: bytes for the data stage, or a stall.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Answer(Vec<u8>),
    Stall,
}

/// A Bulk-OUT transfer that is not USBTMC: the endpoint is halted, and the stream starts again with a header.
#[derive(Clone, Debug, PartialEq)]
pub struct Halt(pub String);

/// The transfer being read from Bulk-OUT.
#[derive(Default)]
struct Incoming {
    /// the header bytes so far
    head: Vec<u8>,
    /// the header, once complete: its MsgID, tag, data bytes and EOM
    header: Option<tmc::Header>,
    /// bytes of the transfer still to come (data and padding)
    left: usize,
    /// data bytes of it taken so far
    got: usize,
}

/// The Bulk-IN transfer being written: its tag, data bytes, bytes written, and whether it was aborted.
#[derive(Clone, Copy, Debug)]
struct InFlight {
    tag: u8,
    data: usize,
    written: usize,
    aborted: bool,
}

/// The state of the interface, shared by the threads of the endpoints and the frame loop (`Server`).
#[derive(Default)]
pub struct Device {
    incoming: Incoming,
    /// the message being received (DEV_DEP_MSG_OUT transfers before the one with EOM)
    message: Vec<u8>,
    /// the tag of the last Bulk-OUT transfer, for INITIATE_ABORT_BULK_OUT
    out_tag: u8,
    /// data bytes of the transfer aborted last (CHECK_ABORT_BULK_OUT_STATUS)
    out_aborted: u32,
    /// counts aborts and clears of Bulk-OUT: bytes read before one are dropped (`bulk_out`)
    out_epoch: u64,
    /// complete messages for the interpreter
    inbox: VecDeque<Vec<u8>>,
    /// the pending REQUEST_DEV_DEP_MSG_IN: its tag and the most data bytes the host takes
    request: Option<(u8, usize)>,
    /// the answers not yet sent, each one message; `sent` bytes of the first are out
    answers: VecDeque<Vec<u8>>,
    sent: usize,
    in_flight: Option<InFlight>,
    /// data bytes sent of the Bulk-IN transfer aborted last (CHECK_ABORT_BULK_IN_STATUS)
    in_aborted: u32,
    /// counts clears and resets: an answer to a message from before one is dropped (`answer`)
    epoch: u64,
    /// the interpreter's error queue is not empty (the frame loop sets it)
    pub eav: bool,
    /// the notification for Interrupt-IN (READ_STATUS_BYTE), not yet written
    notify: Option<[u8; 2]>,
}

impl Device {
    /// Everything dropped: the host configured the interface again, or it went (ENABLE, DISABLE, UNBIND). A Bulk-IN
    /// transfer being written ends when its write fails.
    pub fn reset(&mut self) {
        let in_flight = self.in_flight.map(|f| InFlight { aborted: true, ..f });
        let (out_epoch, epoch, eav) = (self.out_epoch + 1, self.epoch + 1, self.eav);
        *self = Device { in_flight, out_epoch, epoch, eav, ..Device::default() };
    }

    /// Clears what came in and what goes out (INITIATE_CLEAR, and the Bulk-OUT side of `reset`).
    fn clear(&mut self) {
        self.incoming = Incoming::default();
        self.message.clear();
        self.out_epoch += 1;
        self.inbox.clear();
        self.request = None;
        self.answers.clear();
        self.sent = 0;
        self.epoch += 1;
        if let Some(f) = &mut self.in_flight {
            f.aborted = true;
        }
    }

    /// The status byte: MAV while an answer waits, EAV while the interpreter has errors to read.
    pub fn status_byte(&self) -> u8 {
        (if self.answers.is_empty() { 0 } else { STB_MAV }) | (if self.eav { STB_EAV } else { 0 })
    }

    /// Answers a class request of `ep0`. Others (vendor requests, those not offered by `CAPABILITIES`, a request to
    /// the wrong endpoint) are stalled.
    pub fn control(&mut self, s: &Setup) -> Control {
        let tag = s.value as u8;
        let answer = match (s.request_type, s.request, s.index) {
            (TO_INTERFACE, GET_CAPABILITIES, _) => CAPABILITIES.to_vec(),
            (TO_INTERFACE, INITIATE_CLEAR, _) => {
                self.clear();
                vec![STATUS_SUCCESS]
            }
            // pending while a Bulk-IN transfer is still written: bmClear bit 0 says the host has to read it first
            (TO_INTERFACE, CHECK_CLEAR_STATUS, _) => match self.in_flight {
                Some(_) => vec![STATUS_PENDING, 1],
                None => vec![STATUS_SUCCESS, 0],
            },
            (TO_INTERFACE, READ_STATUS_BYTE, _) => {
                if !(2..=127).contains(&s.value) {
                    vec![STATUS_FAILED, tag, 0]
                } else {
                    // with an Interrupt-IN endpoint the byte goes there, with the tag; the answer has 0 in its place
                    self.notify = Some([0x80 | tag, self.status_byte()]);
                    vec![STATUS_SUCCESS, tag, 0]
                }
            }
            (TO_ENDPOINT, INITIATE_ABORT_BULK_OUT, EP_OUT) => self.abort_out(tag),
            (TO_ENDPOINT, CHECK_ABORT_BULK_OUT_STATUS, EP_OUT) => {
                let mut v = vec![STATUS_SUCCESS, 0, 0, 0];
                v.extend_from_slice(&self.out_aborted.to_le_bytes());
                v
            }
            (TO_ENDPOINT, INITIATE_ABORT_BULK_IN, EP_IN) => self.abort_in(tag),
            (TO_ENDPOINT, CHECK_ABORT_BULK_IN_STATUS, EP_IN) => {
                let (status, more) = match self.in_flight {
                    Some(f) if f.aborted => (STATUS_PENDING, 1),
                    _ => (STATUS_SUCCESS, 0),
                };
                let mut v = vec![status, more, 0, 0];
                v.extend_from_slice(&self.in_aborted.to_le_bytes());
                v
            }
            _ => return Control::Stall,
        };
        Control::Answer(answer)
    }

    /// INITIATE_ABORT_BULK_OUT for the transfer `tag` (USBTMC 4.2.1.2): the transfer being received, or the message
    /// it belongs to, is dropped, and the stream starts again with a header.
    fn abort_out(&mut self, tag: u8) -> Vec<u8> {
        let busy = self.incoming.header.is_some() || !self.incoming.head.is_empty() || !self.message.is_empty();
        if !busy {
            return vec![STATUS_FAILED, self.out_tag];
        }
        if tag != self.out_tag {
            return vec![STATUS_TRANSFER_NOT_IN_PROGRESS, self.out_tag];
        }
        self.out_aborted = self.incoming.got as u32;
        self.incoming = Incoming::default();
        self.message.clear();
        self.out_epoch += 1;
        vec![STATUS_SUCCESS, tag]
    }

    /// INITIATE_ABORT_BULK_IN for the transfer `tag` (USBTMC 4.2.1.4): the transfer being written stops, with a short
    /// packet, and the rest of its message is dropped. A request not yet answered is dropped too, but then nothing is
    /// in progress.
    fn abort_in(&mut self, tag: u8) -> Vec<u8> {
        match &mut self.in_flight {
            Some(f) if f.tag == tag && !f.aborted => {
                f.aborted = true;
                if self.sent > 0 {
                    self.answers.pop_front();
                    self.sent = 0;
                }
                vec![STATUS_SUCCESS, tag]
            }
            Some(f) => vec![STATUS_TRANSFER_NOT_IN_PROGRESS, f.tag],
            None => {
                if self.request.is_some_and(|(t, _)| t == tag) {
                    self.request = None;
                    if self.sent > 0 {
                        self.answers.pop_front();
                        self.sent = 0;
                    }
                }
                vec![STATUS_FAILED, tag]
            }
        }
    }

    /// The tag of `abort_out` and `bulk_out`: bytes read before an abort or a clear belong to a transfer that is gone.
    pub fn out_epoch(&self) -> u64 {
        self.out_epoch
    }

    /// How many bytes the next read of Bulk-OUT asks for: `MAX_READ`, whole packets; 0 while the interpreter has
    /// enough messages waiting. A read ends with the host's transfer (its short packet), and `bulk_out` splits what it
    /// got by the headers. A read of only what is left of a header or a transfer is not enough: a host sends header
    /// and message in one short packet, and what did not fit the smaller read was lost on the unit (dwc3, 6.12).
    pub fn out_want(&self) -> usize {
        if self.inbox.len() >= MAX_INBOX {
            0
        } else {
            MAX_READ
        }
    }

    /// Takes bytes from Bulk-OUT, read while `out_epoch` was `epoch` (older bytes are dropped). Any split of the
    /// stream works: the headers say where a transfer ends. A header that is not USBTMC, a MsgID the device does not
    /// take, or a message longer than `MAX_MESSAGE` is a `Halt`: the caller halts the endpoint, and what came of the
    /// message is dropped.
    pub fn bulk_out(&mut self, epoch: u64, mut b: &[u8]) -> Result<(), Halt> {
        if epoch != self.out_epoch {
            return Ok(());
        }
        while !b.is_empty() {
            let Some(h) = self.incoming.header else {
                let n = (HEADER - self.incoming.head.len()).min(b.len());
                self.incoming.head.extend_from_slice(&b[..n]);
                b = &b[n..];
                if self.incoming.head.len() == HEADER {
                    let head = std::mem::take(&mut self.incoming.head);
                    if let Err(e) = self.header(&head) {
                        self.incoming = Incoming::default();
                        self.message.clear();
                        return Err(e);
                    }
                }
                continue;
            };
            let n = self.incoming.left.min(b.len());
            let data = (h.size as usize).saturating_sub(self.incoming.got).min(n);
            if h.msg == tmc::DEV_DEP_MSG_OUT {
                self.message.extend_from_slice(&b[..data]);
            }
            self.incoming.got += data;
            self.incoming.left -= n;
            b = &b[n..];
            if self.incoming.left == 0 {
                self.end_of_transfer(h);
            }
        }
        Ok(())
    }

    /// A complete header: what it asks for, and how many bytes of its transfer follow.
    fn header(&mut self, head: &[u8]) -> Result<(), Halt> {
        let h = tmc::parse_header(head).map_err(Halt)?;
        if h.tag == 0 {
            return Err(Halt("bTag 0".into()));
        }
        let size = h.size as usize;
        let follow = match h.msg {
            tmc::DEV_DEP_MSG_OUT => {
                self.out_tag = h.tag;
                if self.message.len() + size > MAX_MESSAGE {
                    return Err(Halt(format!("message longer than {} bytes", MAX_MESSAGE)));
                }
                (HEADER + size).next_multiple_of(4) - HEADER
            }
            // the data of a vendor's message are read and dropped
            VENDOR_SPECIFIC_OUT => (HEADER + size).next_multiple_of(4) - HEADER,
            tmc::REQUEST_DEV_DEP_MSG_IN => {
                // TermChar is not offered (CAPABILITIES): the answer ends where the message ends
                self.request = Some((h.tag, size.min(MAX_IN_DATA)));
                0
            }
            // not offered (DT0): taken and ignored
            TRIGGER => 0,
            m => return Err(Halt(format!("MsgID {}", m))),
        };
        self.incoming = Incoming { header: Some(h), left: follow, ..Incoming::default() };
        if follow == 0 {
            self.end_of_transfer(h);
        }
        Ok(())
    }

    fn end_of_transfer(&mut self, h: tmc::Header) {
        self.incoming = Incoming::default();
        if h.msg == tmc::DEV_DEP_MSG_OUT && h.attr & 1 != 0 {
            self.inbox.push_back(std::mem::take(&mut self.message));
        }
    }

    /// The next complete message for the interpreter, with the epoch its answer has to carry; only once the answers
    /// before it are out, the last transfer of one included until `in_done` (ADR 0066: replies keep their order and
    /// do not pile up).
    pub fn take_message(&mut self) -> Option<(u64, Vec<u8>)> {
        if !self.answers.is_empty() || self.in_flight.is_some() {
            return None;
        }
        self.inbox.pop_front().map(|m| (self.epoch, m))
    }

    /// The interpreter's answer to a message of `epoch`: dropped when the device was cleared since.
    pub fn answer(&mut self, epoch: u64, a: Vec<u8>) {
        if epoch == self.epoch {
            self.answers.push_back(a);
        }
    }

    /// The next Bulk-IN transfer, once a request and an answer are both there and no transfer is being written:
    /// DEV_DEP_MSG_IN with the request's tag and at most the bytes it asked for, EOM where the answer ends, padded to
    /// a multiple of 4. A transfer of a multiple of 64 bytes gets 4 bytes more, so it ends with a short packet at
    /// either speed (USBTMC 3.3: the device ends a transfer with a short packet; the host takes up to wMaxPacketSize-1
    /// bytes after the data).
    pub fn next_in(&mut self) -> Option<Vec<u8>> {
        if self.in_flight.is_some() || self.answers.is_empty() {
            return None;
        }
        let (tag, max) = self.request.take()?;
        let a = &self.answers[0];
        let n = (a.len() - self.sent).min(max);
        let eom = self.sent + n == a.len();
        let mut t = tmc::transfer(&tmc::in_header(tag, n as u32, eom), &a[self.sent..self.sent + n]);
        if t.len().is_multiple_of(FS_PACKET) {
            t.extend_from_slice(&[0; 4]);
        }
        if eom {
            self.answers.pop_front();
            self.sent = 0;
        } else {
            self.sent += n;
        }
        self.in_flight = Some(InFlight { tag, data: n, written: 0, aborted: false });
        Some(t)
    }

    /// Whether the transfer being written was aborted (or cleared): the writer stops and ends it with a short packet.
    pub fn in_aborted(&self) -> bool {
        self.in_flight.is_some_and(|f| f.aborted)
    }

    /// `n` more bytes of the transfer being written are out.
    pub fn in_written(&mut self, n: usize) {
        if let Some(f) = &mut self.in_flight {
            f.written += n;
        }
    }

    /// The transfer being written is done (sent, aborted or failed).
    pub fn in_done(&mut self) {
        if let Some(f) = self.in_flight.take() {
            if f.aborted {
                self.in_aborted = f.written.saturating_sub(HEADER).min(f.data) as u32;
            }
        }
    }

    /// The notification for Interrupt-IN, if one waits.
    pub fn take_notification(&mut self) -> Option<[u8; 2]> {
        self.notify.take()
    }
}

// ------------------------------------------------------------------------------------------------ the frame loop

/// The device and what wakes the threads that wait for it.
#[derive(Default)]
pub struct Shared {
    dev: Mutex<Device>,
    wake: Condvar,
    /// the switch (`usb.tmc`): the service thread keeps a session only while it is on
    want: std::sync::atomic::AtomicBool,
    /// what the menu shows (`State`), as a number
    state: std::sync::atomic::AtomicU8,
    /// the service thread runs (`Server::set_open` starts one when none does, `ffs::service` ends)
    running: std::sync::atomic::AtomicBool,
}

/// Where the function is, for System > Network > Remote services.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum State {
    Off,
    /// switched on, the FunctionFS not mounted yet or its descriptors not taken (the gadget is built by oos-gadget)
    Starting,
    /// the descriptors are written, no computer has configured the interface
    NoComputer,
    /// a computer has configured it
    Connected,
}

impl State {
    pub fn text(self) -> &'static str {
        match self {
            State::Off => "off",
            State::Starting => "starting",
            State::NoComputer => "no computer",
            State::Connected => "connected",
        }
    }
}

impl Shared {
    /// The device, locked; a thread that panicked with it leaves it usable.
    pub fn dev(&self) -> MutexGuard<'_, Device> {
        self.dev.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Wakes every thread waiting for the device.
    pub fn wake(&self) {
        self.wake.notify_all();
    }

    /// Waits up to `ms` for a change of the device.
    pub fn wait<'a>(&self, g: MutexGuard<'a, Device>, ms: u64) -> MutexGuard<'a, Device> {
        self.wake
            .wait_timeout(g, std::time::Duration::from_millis(ms))
            .map(|(g, _)| g)
            .unwrap_or_else(|e| e.into_inner().0)
    }

    pub fn wanted(&self) -> bool {
        self.want.load(std::sync::atomic::Ordering::SeqCst)
    }

    pub fn set_state(&self, s: State) {
        self.state.store(s as u8, std::sync::atomic::Ordering::SeqCst);
    }

    /// The service thread is about to end: false when the switch came on again meanwhile and no other thread was
    /// started for it, so this one goes on.
    pub fn may_end(&self) -> bool {
        use std::sync::atomic::Ordering::SeqCst;
        self.running.store(false, SeqCst);
        !self.wanted() || self.running.swap(true, SeqCst)
    }

    pub fn state(&self) -> State {
        match self.state.load(std::sync::atomic::Ordering::SeqCst) {
            1 => State::Starting,
            2 => State::NoComputer,
            3 => State::Connected,
            _ => State::Off,
        }
    }
}

/// Messages run in one pass of the frame loop, at most.
const PER_PASS: usize = 16;

/// The frame loop's side: hands complete messages to the interpreter and their answers to the device. The endpoints
/// are served by the threads of `ffs`, started and stopped with the switch.
pub struct Server<P: Program> {
    shared: Arc<Shared>,
    /// the program being run (a `*OPC?` waits in it), with the epoch of its message
    program: Option<(u64, P)>,
    /// what makes the gadget bind once the descriptors are written (`oos-gadget bind`), on the scope only
    bind: Option<fn()>,
}

impl<P: Program> Server<P> {
    /// A server without endpoints: on a computer, and on the scope until the switch is on. `bind` is run once the
    /// function's descriptors are written (None: never started, a computer).
    pub fn new(bind: Option<fn()>) -> Self {
        Server { shared: Arc::default(), program: None, bind }
    }

    /// The switch (`usb.tmc`): on starts the thread that waits for the FunctionFS mount and serves the endpoints;
    /// off ends its session (the gadget is taken down by `oos-gadget apply`, which ends the blocking reads).
    pub fn set_open(&mut self, on: bool) {
        self.shared.want.store(on, std::sync::atomic::Ordering::SeqCst);
        self.shared.wake();
        let Some(bind) = self.bind else { return };
        #[cfg(not(unix))]
        let _ = bind;
        // one service thread: a running one that was about to end goes on instead (`Shared::may_end`)
        #[cfg(unix)]
        if on && !self.shared.running.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.shared.set_state(State::Starting);
            let shared = self.shared.clone();
            let spawned = std::thread::Builder::new()
                .name("usbtmc".into())
                .spawn(move || ffs::service(&shared, std::path::Path::new(MOUNT), bind));
            if let Err(e) = spawned {
                eprintln!("oos-ui: USBTMC: {}", e);
                self.shared.running.store(false, std::sync::atomic::Ordering::SeqCst);
            }
        }
    }

    /// What the menu shows.
    pub fn state(&self) -> State {
        if self.shared.wanted() {
            self.shared.state()
        } else {
            State::Off
        }
    }

    /// One pass of the frame loop: the complete messages to the interpreter (`exec`, which keeps a program that waits
    /// for `*OPC?` for a later pass), their answers to the device. `errors`: the interpreter's error queue is not
    /// empty (the status byte's EAV). Never waits.
    pub fn poll_async(&mut self, exec: &mut dyn FnMut(&mut P) -> Option<Vec<u8>>, errors: bool) {
        let epoch = {
            let mut d = self.shared.dev();
            d.eav = errors;
            d.epoch
        };
        // a program of a message from before a clear is not run on
        if self.program.as_ref().is_some_and(|(e, _)| *e != epoch) {
            self.program = None;
        }
        for _ in 0..PER_PASS {
            if self.program.is_none() {
                let Some((epoch, m)) = self.shared.dev().take_message() else { return };
                self.program = Some((epoch, P::new(&String::from_utf8_lossy(&m))));
            }
            let Some((epoch, p)) = &mut self.program else { return };
            let Some(mut reply) = exec(p) else { return };
            let (epoch, has_reply) = (*epoch, p.has_reply());
            self.program = None;
            if has_reply {
                reply.push(b'\n');
                self.shared.dev().answer(epoch, reply);
                self.shared.wake();
            }
        }
    }

    /// The device, for the tests that play the host (`ffs` is not started there).
    #[cfg(any(test, feature = "testkit"))]
    pub fn shared(&self) -> Arc<Shared> {
        self.shared.clone()
    }
}

#[cfg(test)]
mod tests;
