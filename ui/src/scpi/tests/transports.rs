// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The transports of oos-remote with this interpreter on the demo screen: an empty query still gives its end marker,
//! a write gives nothing, and a query behind a fence that is cancelled gives no reply (ADR 0022).

use super::*;
use oos_remote::{hislip, vxi11};

#[test]
fn vxi11_empty_query_has_a_readable_end_marker_and_cancelled_fence_has_no_reply() {
    let mut ui = crate::ui::testkit::demo_ui();
    let cv = Canvas::new(crate::ui::W, crate::ui::H);
    let mut scpi = Scpi::open(0);
    let mut links = vec![Some(vxi11::Link::<Program>::default())];
    let mut run = |p: &mut Program| scpi.poll_program(&mut ui, &cv, p);
    links[0].as_mut().unwrap().write(":MEAS:CLEAR;:MEAS:STRIP?", &mut run);
    assert_eq!(links[0].as_ref().unwrap().out, b"\n");
    let read = vxi11::device_read(&mut links, &mut run);
    assert_eq!(read, (0, 4, b"\n".to_vec()), "empty query succeeds instead of an immediate read timeout");
    let link = links[0].as_mut().unwrap();
    link.write(":MEAS:CLEAR", &mut run);
    assert!(link.out.is_empty());
    link.write(":MEAS:STRIP?;*OPC?", &mut run);
    assert_eq!(std::mem::take(&mut link.out), b";1\n");
    link.write(":MEAS:STRIP?;:TRIG:EDGE:LEV 10;:SINGLE;*OPC?;:RUN", &mut run);
    assert!(link.out.is_empty());
    assert_eq!(link.waiting(), 1);
    scpi.exec_line(&mut ui, &cv, ":STOP");
    link.pump(&mut |p| scpi.poll_program(&mut ui, &cv, p));
    assert!(link.out.is_empty());
    assert_eq!(link.waiting(), 0);
    assert_eq!(ui.m.run, crate::model::RunState::Stop);
}

#[test]
fn hislip_empty_query_emits_data_end_but_writes_and_cancelled_fences_do_not() {
    let mut ui = crate::ui::testkit::demo_ui();
    let cv = Canvas::new(crate::ui::W, crate::ui::H);
    let mut scpi = Scpi::open(0);
    let mut session = hislip::SyncSession::<Program>::default();
    let mut run = |p: &mut Program| scpi.poll_program(&mut ui, &cv, p);
    let empty = session.data_end(7, ":MEAS:CLEAR;:MEAS:STRIP?", &mut run);
    assert_eq!(empty.len(), 1);
    assert_eq!((empty[0].typ, empty[0].param, empty[0].data.clone()), (hislip::DATA_END_TYPE, 7, vec![b'\n']));
    assert!(session.data_end(8, ":MEAS:CLEAR", &mut run).is_empty());
    assert_eq!(session.data_end(9, ":MEAS:STRIP?;*OPC?", &mut run)[0].data, b";1\n");
    let pending = session.data_end(10, ":MEAS:STRIP?;:TRIG:EDGE:LEV 10;:SINGLE;*OPC?;:RUN", &mut run);
    assert!(pending.is_empty(), "an earlier empty query must not emit a premature response at a pending fence");
    let mut program = session.take_waiting().unwrap();
    assert!(program.has_reply);
    scpi.exec_line(&mut ui, &cv, ":STOP");
    assert_eq!(scpi.poll_program(&mut ui, &cv, &mut program), Some(Vec::new()));
    assert!(!program.has_reply, "cancelled queries must not emit an empty DATA_END response");
    assert_eq!(ui.m.run, crate::model::RunState::Stop);
}

// Break it catches: device_readstb answered 0 with an error queued and with an answer waiting (seen on a unit,
// 2026-11-18), so a VISA client that polls the status byte never saw either (oos-plus).
#[test]
fn vxi11_the_status_byte_of_a_link_is_the_one_of_the_interpreter() {
    let mut ui = crate::ui::testkit::demo_ui();
    let cv = Canvas::new(crate::ui::W, crate::ui::H);
    let mut scpi = Scpi::open(0);
    let mut links = vec![Some(vxi11::Link::<Program>::default())];
    let mut run = |p: &mut Program| scpi.poll_program(&mut ui, &cv, p);
    fn stb(links: &mut Vec<Option<vxi11::Link<Program>>>, run: &mut dyn FnMut(&mut Program) -> Option<Vec<u8>>) -> u32 {
        let reply = vxi11::core_call(links, 13, &[0, 0, 1000, 1000], None, run);
        u32::from_be_bytes(reply[reply.len() - 4..].try_into().unwrap())
    }
    assert_eq!(stb(&mut links, &mut run), 0);
    links[0].as_mut().unwrap().write(":NOSUCH:HEADER 1;*IDN?", &mut run);
    assert_eq!(stb(&mut links, &mut run), 4 | 16, "an error is queued and an answer waits");
    links[0].as_mut().unwrap().out.clear();
    assert_eq!(stb(&mut links, &mut run), 4);
}

// `*ESR?` over VXI-11, as a VISA client sends it: device_write with the END flag and with or without a line end,
// then device_read. The register is read and cleared like on the socket (oos-plus).
#[test]
fn vxi11_the_event_status_is_read_over_device_write_and_device_read() {
    let mut ui = crate::ui::testkit::demo_ui();
    let cv = Canvas::new(crate::ui::W, crate::ui::H);
    let mut scpi = Scpi::open(0);
    let mut links: Vec<Option<vxi11::Link<Program>>> = Vec::new();
    let mut run = |p: &mut Program| scpi.poll_program(&mut ui, &cv, p);
    vxi11::core_call(&mut links, 10, &[1, 0, 0], Some(b"inst0"), &mut run);
    let mut ask = |text: &[u8]| {
        let r = vxi11::core_call(&mut links, 11, &[0, 1000, 0, 8], Some(text), &mut run);
        let mut x = vxi11::Xdr::new(&r);
        assert_eq!((x.u32(), x.u32()), (Some(0), Some(text.len() as u32)), "{:?}", String::from_utf8_lossy(text));
        let r = vxi11::core_call(&mut links, 12, &[0, 1024, 1000, 0, 0, 0], None, &mut run);
        let mut x = vxi11::Xdr::new(&r);
        let (error, reason) = (x.u32(), x.u32());
        (error, reason, String::from_utf8_lossy(x.opaque().unwrap_or(&[])).into_owned())
    };
    for text in [&b"*ESR?"[..], b"*ESR?\n", b"*ESR?\r\n", b"*esr?\n", b"*ESR? \n"] {
        assert_eq!(ask(text), (Some(0), Some(4), "0\n".to_string()), "{:?}", String::from_utf8_lossy(text));
    }
    assert_eq!(ask(b":NOSUCH:HEADER?\n").2, "-113,\"Undefined header\" :NOSUCH:HEADER?\n");
    assert_eq!(ask(b"*ESR?\n").2, "32\n", "the command error bit");
    assert_eq!(ask(b"*ESR?\n").2, "0\n", "read and cleared");
    assert_eq!(ask(b"*IDN?;*ESR?\n").2.split(';').nth(1), Some("0\n"));
}
