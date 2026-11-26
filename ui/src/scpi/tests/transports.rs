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

/// The computer on the rear USB port, played by the scope's own USBTMC host (`usbtmc::Usbtmc`, the Bode's): what it
/// sends goes into the device of the USBTMC server, and each read runs passes of the frame loop with this interpreter
/// until the device has a Bulk-IN transfer for it.
struct Cable {
    server: oos_remote::usbtmc_device::Server<Program>,
    scpi: Scpi,
    ui: crate::ui::Ui,
    cv: Canvas,
}

impl Cable {
    /// One pass of the frame loop: the messages that came, to the interpreter; the status byte as it is then.
    fn pass(&mut self) {
        let (scpi, ui, cv) = (&mut self.scpi, &mut self.ui, &self.cv);
        let errors = scpi.has_errors();
        self.server.poll_async(&mut |p| scpi.poll_program(ui, cv, p), errors);
    }
}

impl crate::usbtmc::Bulk for Cable {
    fn bulk_out(&mut self, data: &[u8], _: u32) -> std::io::Result<usize> {
        let sh = self.server.shared();
        let mut d = sh.dev();
        let epoch = d.out_epoch();
        d.bulk_out(epoch, data).map_err(|h| std::io::Error::other(h.0))?;
        Ok(data.len())
    }
    fn bulk_in(&mut self, buf: &mut [u8], _: u32) -> std::io::Result<usize> {
        let sh = self.server.shared();
        for _ in 0..10 {
            self.pass();
            let mut d = sh.dev();
            if let Some(t) = d.next_in() {
                d.in_written(t.len());
                d.in_done();
                buf[..t.len()].copy_from_slice(&t);
                return Ok(t.len());
            }
        }
        Err(std::io::Error::from_raw_os_error(libc::ETIMEDOUT))
    }
    fn clear_halt(&mut self, _: bool) -> std::io::Result<()> {
        Ok(())
    }
}

// Break it catches: the USBTMC transport answering itself instead of handing the message to the interpreter, an
// answer cut at the host's TransferSize without the rest in the next transfers, or the status byte not following the
// interpreter's error queue
#[test]
fn usbtmc_round_trip_through_the_interpreter() {
    let (scpi, ui, cv) = reference_session();
    let cable = Cable { server: oos_remote::usbtmc_device::Server::new(None), scpi, ui, cv };
    let mut host = crate::usbtmc::Usbtmc::new(cable);
    let idn = host.query("*IDN?").unwrap();
    assert!(idn.starts_with("OOS,") && idn.contains("oos-ui"), "{}", idn);
    host.command(":CHAN1:SCAL 0.5").unwrap();
    assert_eq!(host.query(":CHAN1:SCAL?").unwrap().parse::<f64>().unwrap(), 0.5);
    // an answer longer than the 4096 bytes the host asks for per request comes in several transfers
    host.command(":WAV:SOUR R1;:WAV:FORM ASC").unwrap();
    let data = host.query(":WAV:DATA?").unwrap();
    assert!(data.len() > 3 * 4096, "{} bytes", data.len());
    assert_eq!(data.split(',').count(), host.query(":WAV:POIN?").unwrap().parse::<usize>().unwrap());
    // an error in the queue is the status byte's EAV (as of the last pass of the frame loop); reading it clears it
    let stb = |host: &mut crate::usbtmc::Usbtmc<Cable>| {
        host.bus.pass();
        let sh = host.bus.server.shared();
        sh.dev().control(&oos_remote::usbtmc_device::Setup {
            request_type: 0xa1,
            request: 128,
            value: 2,
            index: 0,
            length: 3,
        });
        let n = sh.dev().take_notification().unwrap();
        n[1]
    };
    assert_eq!(stb(&mut host), 0);
    host.command(":NO:SUCH:COMMand").unwrap();
    host.query("*OPC?").unwrap();
    assert_eq!(stb(&mut host), oos_remote::usbtmc_device::STB_EAV);
    assert!(host.query(":SYST:ERR?").unwrap().starts_with("-113"));
    host.query("*OPC?").unwrap();
    assert_eq!(stb(&mut host), 0);
}
