// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The protocol of the USBTMC device without endpoints: the descriptors as FunctionFS reads them, the class requests
//! and the bulk messages, fed as bytes and checked as bytes (USBTMC 1.0, USB488 1.0).

use super::*;
use oos_system::usbtmc::{out_transfer, parse_in_header, request_header};

fn feed(d: &mut Device, b: &[u8]) -> Result<(), Halt> {
    let e = d.out_epoch();
    d.bulk_out(e, b)
}

/// The interpreter of the tests: every complete message is answered by `f`, None for a message without a query.
fn run(d: &mut Device, f: impl Fn(&str) -> Option<String>) {
    while let Some((epoch, m)) = d.take_message() {
        if let Some(a) = f(&String::from_utf8_lossy(&m)) {
            d.answer(epoch, format!("{}\n", a).into_bytes());
        }
    }
}

fn setup(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Setup {
    Setup { request_type, request, value, index, length }
}

fn answer(c: Control) -> Vec<u8> {
    match c {
        Control::Answer(a) => a,
        Control::Stall => panic!("stalled"),
    }
}

/// The data of a Bulk-IN transfer, checked: header for `tag`, padding to 4, a short packet at the end.
fn data_of(t: &[u8], tag: u8) -> (Vec<u8>, bool) {
    let h = parse_in_header(t, tag).unwrap();
    let n = h.size as usize;
    assert!(t.len().is_multiple_of(4), "padded to 4: {}", t.len());
    assert!(!t.len().is_multiple_of(64), "ends with a short packet: {}", t.len());
    assert!(t.len() - HEADER - n < 8 && t[HEADER + n..].iter().all(|&b| b == 0), "only zeros after the data");
    (t[HEADER..HEADER + n].to_vec(), h.eom)
}

fn le32_at(b: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]])
}

// Break it catches: a blob FunctionFS refuses (the write to ep0 fails, the gadget never binds), or one it takes with
// the wrong interface (VISA does not find a USBTMC instrument), or OS descriptors Windows cannot bind WinUSB by
#[test]
fn the_descriptors_are_what_functionfs_reads() {
    let d = descriptors();
    assert_eq!(le32_at(&d, 0), 3, "FUNCTIONFS_DESCRIPTORS_MAGIC_V2");
    assert_eq!(le32_at(&d, 4) as usize, d.len(), "length of the whole blob");
    assert_eq!(le32_at(&d, 8), 1 | 2 | 8, "HAS_FS_DESC | HAS_HS_DESC | HAS_MS_OS_DESC, nothing else");
    assert_eq!((le32_at(&d, 12), le32_at(&d, 16), le32_at(&d, 20)), (4, 4, 2), "fs, hs and OS descriptor counts");
    let mut i = 24;
    for (speed, bulk, interval) in [("full", 64u16, 1u8), ("high", 512, 4)] {
        // the interface: one, three endpoints, 0xFE/0x03/0x01, string 1
        assert_eq!(&d[i..i + 9], &[9, 4, 0, 0, 3, 0xfe, 0x03, 0x01, 1], "{} speed interface", speed);
        i += 9;
        for (address, kind, size, every) in [(0x01, 2, bulk, 0), (0x82, 2, bulk, 0), (0x83, 3, 2, interval)] {
            let s = size.to_le_bytes();
            assert_eq!(&d[i..i + 7], &[7, 5, address, kind, s[0], s[1], every], "{} speed ep {:#x}", speed, address);
            i += 7;
        }
    }
    // the extended compatible ID: header of 11 bytes, one function of 24, as the kernel checks it
    assert_eq!(d[i], 0, "interface 0");
    assert_eq!(le32_at(&d, i + 1), 35, "dwLength");
    assert_eq!(&d[i + 5..i + 11], &[0x00, 0x01, 4, 0, 1, 0], "bcdVersion 1.00, wIndex 4, bCount 1, reserved 0");
    i += 11;
    assert_eq!(&d[i..i + 2], &[0, 1], "first interface 0, Reserved1 1");
    assert_eq!(&d[i + 2..i + 10], b"WINUSB\0\0");
    assert!(d[i + 10..i + 24].iter().all(|&b| b == 0), "no sub-compatible ID, Reserved2 zeros");
    i += 24;
    // the extended property: dwSize = 14 + name + value (the kernel keeps it in a byte), REG_SZ, NUL-terminated
    let start = i;
    assert_eq!(&d[i + 5..i + 11], &[0x00, 0x01, 5, 0, 1, 0], "bcdVersion 1.00, wIndex 5, wCount 1");
    i += 11;
    let size = le32_at(&d, i) as usize;
    assert_eq!(le32_at(&d, start + 1) as usize, 11 + size, "dwLength");
    assert!(size < 256);
    assert_eq!(le32_at(&d, i + 4), 1, "REG_SZ");
    let pnl = u16::from_le_bytes([d[i + 8], d[i + 9]]) as usize;
    assert_eq!(&d[i + 10..i + 10 + pnl], b"DeviceInterfaceGUID\0");
    let pdl = le32_at(&d, i + 10 + pnl) as usize;
    assert_eq!(size, 14 + pnl + pdl);
    let value = &d[i + 14 + pnl..i + 14 + pnl + pdl];
    assert_eq!(value, format!("{}\0", INTERFACE_GUID).as_bytes());
    assert_eq!(value.len(), 39, "a GUID in braces and its NUL");
    assert_eq!(i + size, d.len(), "nothing after the last descriptor");

    let s = strings();
    assert_eq!((le32_at(&s, 0), le32_at(&s, 4) as usize), (2, s.len()), "FUNCTIONFS_STRINGS_MAGIC and length");
    assert_eq!((le32_at(&s, 8), le32_at(&s, 12)), (1, 1), "one string, one language");
    assert_eq!(&s[16..18], &[0x09, 0x04], "US English");
    assert_eq!(&s[18..], b"OOS USBTMC\0");
}

#[test]
fn the_events_of_ep0_are_read_twelve_bytes_each() {
    let mut b = vec![0u8; 12];
    b[8] = 2;
    b.extend_from_slice(&[0xa1, 7, 0, 0, 2, 0, 0x18, 0, 4, 0, 0, 0]);
    b.extend_from_slice(&[0; 5]);
    assert_eq!(events(&b), [Event::Enable, Event::Setup(setup(0xa1, 7, 0, 2, 0x18))]);
}

// Break it catches: a capability the device claims but does not have (a host sends TermChar or REN_CONTROL), or a
// request it claims not to have but answers
#[test]
fn get_capabilities_says_usb488_with_scpi_and_nothing_optional() {
    let mut d = Device::default();
    let c = answer(d.control(&setup(0xa1, 7, 0, 0, 0x18)));
    assert_eq!(
        c,
        [1, 0, 0x00, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0x00, 0x01, 0x04, 0x08, 0, 0, 0, 0, 0, 0, 0, 0],
        "status, bcdUSBTMC, no indicator pulse or TermChar, bcdUSB488, 488.2 interface, SCPI"
    );
    // what it does not offer is stalled: INDICATOR_PULSE, REN_CONTROL, GO_TO_LOCAL, LOCAL_LOCKOUT, a vendor request,
    // an abort sent to the other endpoint
    for s in [
        setup(0xa1, 64, 0, 0, 1),
        setup(0xa1, 160, 1, 0, 1),
        setup(0xa1, 161, 0, 0, 1),
        setup(0xa1, 162, 0, 0, 1),
        setup(0xc1, 7, 0, 0, 0x18),
        setup(0xa2, 1, 1, EP_IN, 2),
        setup(0xa2, 3, 1, EP_OUT, 2),
    ] {
        assert_eq!(d.control(&s), Control::Stall, "{:?}", s);
    }
}

// Break it catches: an answer that ignores the host's TransferSize (Linux refuses "more data than requested"), EOM
// on a transfer that is not the last, a tag of another request, or the rest of an answer lost between requests
#[test]
fn a_long_answer_comes_in_transfers_of_what_the_host_asked_for() {
    let mut d = Device::default();
    feed(&mut d, &out_transfer(1, b":WAV:DATA?\n", true)).unwrap();
    run(&mut d, |m| (m == ":WAV:DATA?\n").then(|| "0123456789".into()));
    assert_eq!(d.status_byte(), STB_MAV);
    let mut got = Vec::new();
    for (tag, max, want, eom) in [(2u8, 4u32, "0123", false), (3, 4, "4567", false), (4, 100, "89\n", true)] {
        assert_eq!(d.next_in(), None, "nothing before the request");
        feed(&mut d, &request_header(tag, max, None)).unwrap();
        let t = d.next_in().unwrap();
        assert_eq!(d.next_in(), None, "one transfer at a time");
        let (data, e) = data_of(&t, tag);
        assert_eq!((String::from_utf8(data.clone()).unwrap().as_str(), e), (want, eom));
        got.extend_from_slice(&data);
        d.in_written(t.len());
        d.in_done();
    }
    assert_eq!(got, b"0123456789\n");
    assert_eq!(d.status_byte(), 0, "no message available");
    // one transfer never has more than MAX_IN_DATA, whatever the host asks
    feed(&mut d, &out_transfer(5, b"*IDN?\n", true)).unwrap();
    run(&mut d, |_| Some("x".repeat(MAX_IN_DATA + 10)));
    feed(&mut d, &request_header(6, u32::MAX, None)).unwrap();
    let (data, eom) = data_of(&d.next_in().unwrap(), 6);
    assert_eq!((data.len(), eom), (MAX_IN_DATA, false));
}

// Break it catches: the next message run while the last transfer of the answer before it is still being written
// (a slow host), so the replies could pass each other
#[test]
fn the_next_message_waits_until_the_last_transfer_is_out() {
    let mut d = Device::default();
    feed(&mut d, &out_transfer(1, b"*IDN?\n", true)).unwrap();
    run(&mut d, |_| Some("OOS".into()));
    feed(&mut d, &request_header(2, 100, None)).unwrap();
    let t = d.next_in().unwrap();
    assert!(data_of(&t, 2).1, "the whole answer in one transfer");
    feed(&mut d, &out_transfer(3, b":RUN\n", true)).unwrap();
    assert_eq!(d.take_message(), None, "the transfer is not written yet");
    d.in_written(t.len());
    d.in_done();
    assert_eq!(d.take_message().map(|m| m.1), Some(b":RUN\n".to_vec()));
}

// Break it catches: a transfer ending on a packet boundary (the host waits for more until its timeout), or padding
// that is not zeros
#[test]
fn a_transfer_of_whole_packets_gets_four_bytes_more() {
    let mut d = Device::default();
    // 12 + 52 = 64 bytes: one full-speed packet without the four extra bytes
    feed(&mut d, &out_transfer(1, b"Q?\n", true)).unwrap();
    run(&mut d, |_| Some("y".repeat(51)));
    feed(&mut d, &request_header(2, 1000, None)).unwrap();
    let t = d.next_in().unwrap();
    assert_eq!(t.len(), 68);
    assert_eq!(data_of(&t, 2), (format!("{}\n", "y".repeat(51)).into_bytes(), true));
}

// Break it catches: a header with a wrong inverse tag or tag 0 taken as a message (USBTMC 3.2: the device halts
// Bulk-OUT), or the stream not starting again with a header afterwards
#[test]
fn a_wrong_tag_halts_bulk_out_and_the_next_header_starts_again() {
    let mut d = Device::default();
    let mut bad = out_transfer(7, b"*RST\n", true);
    bad[2] = 0;
    assert!(feed(&mut d, &bad[..HEADER]).unwrap_err().0.contains("bTag"));
    assert!(feed(&mut d, &out_transfer(0, b"*RST\n", true)).unwrap_err().0.contains("bTag 0"));
    let mut unknown = out_transfer(8, b"", true);
    unknown[0] = 99;
    assert!(feed(&mut d, &unknown).unwrap_err().0.contains("MsgID 99"));
    assert_eq!(d.take_message(), None);
    feed(&mut d, &out_transfer(9, b"*IDN?\n", true)).unwrap();
    assert_eq!(d.take_message().map(|m| m.1), Some(b"*IDN?\n".to_vec()));
    // a REQUEST for an answer is checked the same way, and its tag is the answer's
    let mut r = request_header(10, 64, None);
    r[2] = 10;
    assert!(feed(&mut d, &r).is_err());
}

// Break it catches: a message split over transfers handed over before its EOM, data of one transfer lost when it
// comes in reads of any size (a bulk packet is 64 or 512 bytes, a read of FunctionFS up to MAX_READ), or padding
// taken as data
#[test]
fn a_message_larger_than_a_packet_is_one_message_at_its_eom() {
    let mut d = Device::default();
    let text: Vec<u8> = (0..2001u32).map(|i| b'a' + (i % 26) as u8).collect();
    let mut stream = out_transfer(1, &text[..1000], false);
    stream.extend_from_slice(&out_transfer(2, &text[1000..], true));
    stream.extend_from_slice(&out_transfer(3, b"*OPC?\n", true));
    for piece in [1usize, 7, 64, 512, 5000] {
        let mut d2 = Device::default();
        for c in stream.chunks(piece) {
            // the reader asks for no more than the device wants; any split is taken
            assert!(d2.out_want() > 0);
            feed(&mut d2, c).unwrap();
        }
        assert_eq!(d2.take_message().map(|m| m.1), Some(text.clone()), "pieces of {}", piece);
        assert_eq!(d2.take_message().map(|m| m.1), Some(b"*OPC?\n".to_vec()));
    }
    // transfers without EOM are no message yet
    feed(&mut d, &out_transfer(4, b":CHAN1:SCAL", false)).unwrap();
    assert_eq!(d.take_message(), None);
    assert_eq!(d.out_want(), MAX_READ, "reads are whole: header and message come in one short packet");
    feed(&mut d, &out_transfer(5, b" 1\n", true)).unwrap();
    assert_eq!(d.take_message().map(|m| m.1), Some(b":CHAN1:SCAL 1\n".to_vec()));
    // a read in the middle of a transfer asks for whole packets as well
    feed(&mut d, &out_transfer(6, &text, true)[..HEADER]).unwrap();
    assert_eq!(d.out_want(), MAX_READ);
}

// Break it catches: a message past MAX_MESSAGE kept in memory (a host could fill the scope's memory)
#[test]
fn a_message_past_the_limit_halts_bulk_out() {
    let mut d = Device::default();
    let big = vec![b'x'; MAX_MESSAGE];
    feed(&mut d, &out_transfer(1, &big, false)).unwrap();
    assert!(feed(&mut d, &out_header_only(2, 1, true)).unwrap_err().0.contains("longer"));
    feed(&mut d, &out_transfer(3, b"*IDN?\n", true)).unwrap();
    assert_eq!(d.take_message().map(|m| m.1), Some(b"*IDN?\n".to_vec()), "what came before is dropped");
}

fn out_header_only(tag: u8, size: u32, eom: bool) -> Vec<u8> {
    oos_system::usbtmc::out_header(tag, size, eom).to_vec()
}

// Break it catches: an abort of Bulk-OUT that leaves the device in the middle of the aborted transfer (the next
// header read as its data), bytes read before the abort taken after it, or the status bytes of USBTMC 4.2.1.2-3
#[test]
fn abort_bulk_out_in_the_middle_of_a_transfer() {
    let mut d = Device::default();
    // nothing in progress: STATUS_FAILED with the last tag
    assert_eq!(answer(d.control(&setup(0xa2, 1, 5, EP_OUT, 2))), [0x80, 0]);
    let t = out_transfer(5, &[b'z'; 100], true);
    let epoch = d.out_epoch();
    feed(&mut d, &t[..HEADER + 40]).unwrap();
    // another tag: STATUS_TRANSFER_NOT_IN_PROGRESS with the tag in progress
    assert_eq!(answer(d.control(&setup(0xa2, 1, 4, EP_OUT, 2))), [0x81, 5]);
    assert_eq!(answer(d.control(&setup(0xa2, 1, 5, EP_OUT, 2))), [0x01, 5]);
    assert_eq!(answer(d.control(&setup(0xa2, 2, 0, EP_OUT, 8))), [0x01, 0, 0, 0, 40, 0, 0, 0], "40 bytes received");
    // the rest the reader had already read before the abort is dropped, the next transfer starts with its header
    d.bulk_out(epoch, &t[HEADER + 40..]).unwrap();
    assert_eq!(d.out_want(), MAX_READ);
    feed(&mut d, &out_transfer(6, b"*IDN?\n", true)).unwrap();
    assert_eq!(d.take_message().map(|m| m.1), Some(b"*IDN?\n".to_vec()));
}

// Break it catches: an abort of Bulk-IN that keeps sending the aborted answer, CHECK_ABORT_BULK_IN_STATUS saying
// success while the host still has to read the end of the transfer, or a tag mismatch taken as success
#[test]
fn abort_bulk_in_in_the_middle_of_an_answer() {
    let mut d = Device::default();
    feed(&mut d, &out_transfer(1, b":DISP:DATA?\n", true)).unwrap();
    run(&mut d, |_| Some("#9000000020".to_string() + &"p".repeat(20)));
    feed(&mut d, &request_header(2, 8, None)).unwrap();
    let t = d.next_in().unwrap();
    assert_eq!(answer(d.control(&setup(0xa2, 3, 9, EP_IN, 2))), [0x81, 2], "another tag");
    assert_eq!(answer(d.control(&setup(0xa2, 3, 2, EP_IN, 2))), [0x01, 2]);
    assert!(d.in_aborted(), "the writer stops and ends the transfer");
    assert_eq!(answer(d.control(&setup(0xa2, 4, 0, EP_IN, 8)))[..2], [0x02, 1], "pending: the host reads on");
    d.in_written(t.len());
    d.in_done();
    assert_eq!(answer(d.control(&setup(0xa2, 4, 0, EP_IN, 8))), [0x01, 0, 0, 0, 8, 0, 0, 0], "8 data bytes sent");
    // the rest of the aborted answer is gone: a new request waits for a new answer
    feed(&mut d, &request_header(3, 64, None)).unwrap();
    assert_eq!(d.next_in(), None);
    assert_eq!(d.status_byte() & STB_MAV, 0);
    feed(&mut d, &out_transfer(4, b"*IDN?\n", true)).unwrap();
    run(&mut d, |_| Some("OOS".into()));
    assert_eq!(data_of(&d.next_in().unwrap(), 3), (b"OOS\n".to_vec(), true), "the waiting request gets it");
    d.in_done();
    // a request that waits for an answer, aborted: nothing in progress, and the request is dropped
    feed(&mut d, &request_header(5, 64, None)).unwrap();
    assert_eq!(answer(d.control(&setup(0xa2, 3, 5, EP_IN, 2))), [0x80, 5]);
    feed(&mut d, &out_transfer(6, b"*IDN?\n", true)).unwrap();
    run(&mut d, |_| Some("OOS".into()));
    assert_eq!(d.next_in(), None, "no request: the answer waits");
}

// Break it catches: INITIATE_CLEAR that leaves a message, an answer or a half-read transfer behind, an answer of a
// message from before the clear sent after it, or CHECK_CLEAR_STATUS done while a transfer is still written
#[test]
fn clear_drops_everything_in_both_directions() {
    let mut d = Device::default();
    feed(&mut d, &out_transfer(1, b"*IDN?\n", true)).unwrap();
    let (old, _) = d.take_message().unwrap();
    feed(&mut d, &out_transfer(2, b":RUN\n", true)).unwrap();
    feed(&mut d, &out_transfer(3, b"half", false)).unwrap();
    feed(&mut d, &request_header(4, 64, None)).unwrap();
    assert_eq!(answer(d.control(&setup(0xa1, 5, 0, 0, 1))), [0x01]);
    assert_eq!(answer(d.control(&setup(0xa1, 6, 0, 0, 2))), [0x01, 0]);
    d.answer(old, b"OOS\n".to_vec());
    assert_eq!(d.take_message(), None, "the queued message is gone");
    assert_eq!(d.next_in(), None, "the request and the old answer are gone");
    feed(&mut d, &out_transfer(5, b" end\n", true)).unwrap();
    assert_eq!(d.take_message().map(|m| m.1), Some(b" end\n".to_vec()), "the half message is gone");
    // a clear while a transfer is written: pending until the writer is done, bmClear says the host reads first
    feed(&mut d, &out_transfer(6, b"Q?\n", true)).unwrap();
    let (e, _) = d.take_message().unwrap();
    d.answer(e, b"long answer\n".to_vec());
    feed(&mut d, &request_header(7, 4, None)).unwrap();
    assert!(d.next_in().is_some());
    assert_eq!(answer(d.control(&setup(0xa1, 5, 0, 0, 1))), [0x01]);
    assert!(d.in_aborted());
    assert_eq!(answer(d.control(&setup(0xa1, 6, 0, 0, 2))), [0x02, 1]);
    d.in_done();
    assert_eq!(answer(d.control(&setup(0xa1, 6, 0, 0, 2))), [0x01, 0]);
}

// Break it catches: the status byte answered in the control request although an Interrupt-IN endpoint is there
// (USB488 4.3.1: then it goes there, with the tag), or MAV and EAV not following the device
#[test]
fn read_status_byte_answers_on_the_interrupt_endpoint() {
    let mut d = Device::default();
    assert_eq!(answer(d.control(&setup(0xa1, 128, 2, 0, 3))), [0x01, 2, 0]);
    assert_eq!(d.take_notification(), Some([0x82, 0]));
    assert_eq!(d.take_notification(), None);
    d.eav = true;
    feed(&mut d, &out_transfer(1, b"*IDN?\n", true)).unwrap();
    run(&mut d, |_| Some("OOS".into()));
    assert_eq!(answer(d.control(&setup(0xa1, 128, 127, 0, 3))), [0x01, 127, 0]);
    assert_eq!(d.take_notification(), Some([0x80 | 127, STB_MAV | STB_EAV]));
    // a tag outside 2..127
    assert_eq!(answer(d.control(&setup(0xa1, 128, 1, 0, 3))), [0x80, 1, 0]);
    assert_eq!(d.take_notification(), None);
}

// Break it catches: a host that sends many messages without reading filling the memory, or the reader stopped for
// good when the interpreter caught up
#[test]
fn bulk_out_waits_while_the_interpreter_has_enough() {
    let mut d = Device::default();
    for tag in 1..=MAX_INBOX as u8 {
        feed(&mut d, &out_transfer(tag, b":RUN\n", true)).unwrap();
    }
    assert_eq!(d.out_want(), 0, "the endpoint is not read: the host's transfers wait");
    d.take_message();
    assert_eq!(d.out_want(), MAX_READ);
}

// Break it catches: the frame loop's side running a message before the answer of the one before is out (ADR 0066),
// a program waiting for `*OPC?` run again from the start, or one from before a clear answered after it
#[test]
fn the_server_runs_one_message_at_a_time_and_keeps_a_waiting_one() {
    let mut s = Server::<crate::Line>::new(None);
    let sh = s.shared();
    assert_eq!(s.state(), State::Off);
    for (tag, m) in [(1u8, ":RUN;*IDN?\n"), (2, "*IDN?\n"), (3, ":STOP\n")] {
        feed(&mut sh.dev(), &out_transfer(tag, m.as_bytes(), true)).unwrap();
    }
    let ran = std::cell::RefCell::new(Vec::new());
    let mut exec = |p: &mut crate::Line| {
        Some(p.immediate(&mut |c| {
            ran.borrow_mut().push(c.to_string());
            if c.ends_with('?') {
                b"OOS".to_vec()
            } else {
                Vec::new()
            }
        }))
    };
    s.poll_async(&mut exec, true);
    assert_eq!(*ran.borrow(), [":RUN", "*IDN?"], "the second message waits for the first answer to be read");
    assert_eq!(sh.dev().status_byte(), STB_MAV | STB_EAV);
    feed(&mut sh.dev(), &request_header(4, 64, None)).unwrap();
    assert_eq!(data_of(&sh.dev().next_in().unwrap(), 4), (b"OOS\n".to_vec(), true));
    sh.dev().in_done();
    s.poll_async(&mut exec, false);
    assert_eq!(ran.borrow().len(), 3);
    feed(&mut sh.dev(), &request_header(5, 64, None)).unwrap();
    assert_eq!(data_of(&sh.dev().next_in().unwrap(), 5), (b"OOS\n".to_vec(), true));
    sh.dev().in_done();
    s.poll_async(&mut exec, false);
    assert_eq!(ran.borrow().last().unwrap(), ":STOP", "a message without a query has no answer");
    assert_eq!(sh.dev().status_byte(), 0);

    // a program that waits (`*OPC?`) is kept and asked again on the next pass; a clear drops it
    feed(&mut sh.dev(), &out_transfer(6, b"*OPC?\n", true)).unwrap();
    let asked = std::cell::Cell::new(0);
    s.poll_async(
        &mut |_: &mut crate::Line| {
            asked.set(asked.get() + 1);
            None
        },
        false,
    );
    s.poll_async(
        &mut |_: &mut crate::Line| {
            asked.set(asked.get() + 1);
            None
        },
        false,
    );
    assert_eq!(asked.get(), 2, "the same program, once per pass");
    answer(sh.dev().control(&setup(0xa1, 5, 0, 0, 1)));
    s.poll_async(
        &mut |p: &mut crate::Line| {
            asked.set(asked.get() + 1);
            Some(p.immediate(&mut |_| b"1".to_vec()))
        },
        false,
    );
    assert_eq!(asked.get(), 2, "the program from before the clear is dropped, not run");
    feed(&mut sh.dev(), &request_header(7, 64, None)).unwrap();
    assert_eq!(sh.dev().next_in(), None);
}

// Break it catches: two service threads at once after the switch went off and on quickly (both would write
// descriptors), or none at all when it came on while the old one was ending
#[test]
fn one_service_thread_follows_the_switch() {
    use std::sync::atomic::Ordering::SeqCst;
    use std::time::{Duration, Instant};
    let mut s = Server::<crate::Line>::new(Some(|| {}));
    let sh = s.shared();
    let gone = |sh: &Shared| {
        let end = Instant::now() + Duration::from_secs(30);
        while sh.running.load(SeqCst) && Instant::now() < end {
            std::thread::sleep(Duration::from_millis(5));
        }
        !sh.running.load(SeqCst)
    };
    for _ in 0..3 {
        s.set_open(true);
        assert_eq!(s.state(), State::Starting, "no FunctionFS here: it waits for the mount");
        s.set_open(false);
        s.set_open(true);
        assert!(sh.running.load(SeqCst));
        s.set_open(false);
        assert_eq!(s.state(), State::Off);
        assert!(gone(&sh), "the thread ends with the switch");
    }
    // the thread that was about to end goes on when the switch came on in between, and no second one starts
    sh.running.store(true, SeqCst);
    s.set_open(true);
    assert!(!sh.may_end(), "on again: the old thread goes on");
    s.set_open(false);
    assert!(sh.may_end());
    assert!(!sh.running.load(SeqCst));
}
