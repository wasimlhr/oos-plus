// Copyright (C) 2026 OOS contributors
// SPDX-License-Identifier: GPL-2.0-or-later

//! The Ext generator page (Menu > Instruments > Ext generator, ADR 0028): a generator beside the scope, on the USB-A
//! port or on the LAN. The page names its model, connects to it and shows what it says it is, and can switch a sine
//! on by itself, to see on the screen that the generator does what it is told. The Bode sweep (ui/bode.rs) sends
//! through the same connection; a sweep that finds none opens one for itself and closes it at its end.
//! `crate::extgen` has the models, the connection and the thread.

use super::*;
use crate::extgen::{self, Conn, GENERATORS};
use std::sync::{mpsc, Arc, Mutex};

/// The connection to the external generator, as the screen sees it.
#[derive(Default)]
pub(super) struct ExtUi {
    /// the way to the generator thread; none: not connected
    tx: Option<mpsc::Sender<extgen::Batch>>,
    /// what the thread says: one per connection, so a thread that ends late says it to nobody
    pub shared: Arc<Mutex<extgen::Shared>>,
    /// batches sent on this connection (`Shared::done` counts the ones that are through)
    pub sent: u64,
    /// the output of the generator, as last commanded on this connection
    pub out: bool,
    /// the Bode sweep opened the connection: it closes it at its end
    pub by_bode: bool,
    /// Connect waits for the `*IDN?` answer to pick the model
    pick: bool,
}

/// Frequency of the page's own output, Hz. What a model cannot do, it refuses.
pub(super) const FREQ: (f64, f64) = (1e-3, 200e6);
/// Amplitude of the page's own output, Vpp.
pub(super) const AMPL: (f64, f64) = (0.002, 20.0);
/// Why the page does not touch the generator now.
const IN_SWEEP: &str = "The Bode sweep is using the generator";

impl Ui {
    /// The model of the external generator: an external entry of the table, whatever a setup file said.
    pub(super) fn ext_model(&self) -> usize {
        if extgen::external(self.m.ext_model) {
            self.m.ext_model
        } else {
            extgen::FIRST_EXTERNAL
        }
    }

    /// What the templates of the model are filled with, for `f` Hz and `a` Vpp.
    pub(super) fn ext_vars(&self, f: f64, a: f64) -> extgen::Vars {
        extgen::Vars { f, a, ch: self.m.ext_ch.min(1), hiz: self.m.ext_hiz }
    }

    fn ext_conn(&self) -> Conn {
        self.ext.shared.lock().unwrap().conn.clone()
    }

    /// Connected, or on the way to it.
    pub(super) fn ext_linked(&self) -> bool {
        self.ext.tx.is_some() && matches!(self.ext_conn(), Conn::Connecting | Conn::On(_))
    }

    /// Opens the connection the page describes (any open one is closed). `identify`: ask `*IDN?` on the LAN too and
    /// pick the model from the answer.
    pub(super) fn ext_connect(&mut self, identify: bool) {
        let shared = Arc::new(Mutex::new(extgen::Shared::default()));
        let target = extgen::Target { usb: self.m.bode_usb, addr: self.m.bode_addr.clone(), identify };
        self.ext =
            ExtUi { tx: Some(extgen::spawn(target, shared.clone())), shared, pick: identify, ..Default::default() };
    }

    /// Closes the connection: the thread ends with its sender.
    pub(super) fn ext_disconnect(&mut self) {
        self.ext = ExtUi::default();
    }

    /// Sends command lines to the generator, with the error query of the model. False without a connection.
    pub(super) fn ext_send(&mut self, lines: Vec<String>) -> bool {
        if lines.is_empty() {
            return true;
        }
        let g = &GENERATORS[self.ext_model()];
        let Some(tx) = &self.ext.tx else { return false };
        if tx.send(extgen::Batch { lines, check: g.err, pace_ms: g.pace_ms }).is_err() {
            return false;
        }
        self.ext.sent += 1;
        true
    }

    /// A Bode sweep is stepping the external generator: it reads what the thread says, and the page keeps its hands
    /// off the generator.
    pub(super) fn ext_in_sweep(&self) -> bool {
        self.bode.running && !self.bode.free && extgen::external(self.m.bode_gen)
    }

    /// Every pass of the loop: the model from the `*IDN?` answer after Connect, and what the thread has to say.
    pub(super) fn ext_poll(&mut self) {
        if self.ext.tx.is_none() {
            return;
        }
        let sweep = self.ext_in_sweep();
        let (conn, idn, err, note) = {
            let mut s = self.ext.shared.lock().unwrap();
            let idn = if self.ext.pick { s.idn.clone() } else { None };
            let (err, note) = if sweep { (None, None) } else { (s.err.take(), s.note.take()) };
            (s.conn.clone(), idn, err, note)
        };
        if conn != Conn::Connecting {
            self.ext.pick = false;
        }
        if let Some(i) = idn.as_deref().and_then(extgen::for_idn) {
            if i != self.ext_model() {
                self.ext_set_model(i);
                self.mark_changed();
            }
        }
        if let Some(n) = note {
            self.toast(&n);
        }
        if let Some(e) = err {
            self.toast(&e);
        }
        // a connection that failed is none: the reason stays in the State row
        if matches!(conn, Conn::Failed(_)) && !sweep {
            (self.ext.tx, self.ext.out, self.ext.by_bode) = (None, false, false);
        }
    }

    /// The model of the page; a Bode sweep set to External follows it.
    fn ext_set_model(&mut self, i: usize) {
        self.m.ext_model = i;
        if extgen::external(self.m.bode_gen) {
            self.m.bode_gen = i;
        }
    }

    /// The page's own output: load, sine and amplitude (`all`), the frequency, and the output switch as it is.
    fn ext_apply(&mut self, all: bool) {
        let g = &GENERATORS[self.ext_model()];
        let v = self.ext_vars(self.m.ext_freq, self.m.ext_ampl);
        if all {
            self.ext_send(extgen::lines(g.setup, &v));
        }
        self.ext_send(extgen::lines(g.step, &v));
        self.ext_send(extgen::lines(if self.ext.out { g.on } else { g.off }, &v));
    }

    /// The output of the generator on or off (the Output row, `:EXTGen:OUTPut`). On needs a connection; the error
    /// says why nothing was sent.
    pub(super) fn ext_output(&mut self, on: bool) -> Result<(), &'static str> {
        if self.ext_in_sweep() {
            return Err(IN_SWEEP);
        }
        if !self.ext_linked() {
            self.ext.out = false;
            return if on { Err("Connect the generator first") } else { Ok(()) };
        }
        self.ext.out = on;
        if on {
            self.ext_apply(true);
        } else {
            let g = &GENERATORS[self.ext_model()];
            let v = self.ext_vars(self.m.ext_freq, self.m.ext_ampl);
            self.ext_send(extgen::lines(g.off, &v));
        }
        Ok(())
    }

    /// Connect or disconnect (the Connect row, `:EXTGen:CONNect`).
    pub(super) fn ext_link(&mut self, on: bool) -> Result<(), &'static str> {
        if self.ext_in_sweep() {
            return Err(IN_SWEEP);
        }
        if !on {
            if self.ext.tx.is_some() {
                self.ext_disconnect();
                self.toast("Generator disconnected");
            }
        } else if !self.ext_linked() {
            self.ext_connect(true);
            let at = if self.m.bode_usb { "the USB port".to_string() } else { self.m.bode_addr.clone() };
            self.toast(&format!("Connecting to the generator on {}", at));
        }
        Ok(())
    }

    /// Whether the settings of the page may change now: not while a sweep steps the generator.
    pub(super) fn ext_free(&self) -> Result<(), &'static str> {
        if self.ext_in_sweep() {
            Err(IN_SWEEP)
        } else {
            Ok(())
        }
    }

    /// The state of the connection in one word (`:EXTGen:STATe?`).
    pub(super) fn ext_state_word(&self) -> &'static str {
        match self.ext_conn() {
            Conn::Off => "OFF",
            Conn::Connecting => "CONNECTING",
            Conn::On(_) => "ON",
            Conn::Failed(_) => "FAILED",
        }
    }

    /// The address of the generator on the LAN (`:EXTGen:ADDRess`, `:BODE:GENerator`): another address is another
    /// connection.
    pub(super) fn ext_set_addr(&mut self, addr: &str) {
        if self.m.bode_addr != addr {
            self.m.bode_addr = addr.to_string();
            if !self.m.bode_usb {
                self.ext_disconnect();
            }
        }
    }

    /// Where the generator is (`:BODE:GENerator`): on the USB-A port, or on the LAN at `addr` (none: the address
    /// stays). False while a sweep steps the generator.
    pub fn ext_place(&mut self, usb: bool, addr: Option<&str>) -> bool {
        if self.ext_in_sweep() {
            return false;
        }
        let was = (self.m.bode_usb, self.m.bode_addr.clone());
        self.m.bode_usb = usb;
        if let Some(a) = addr {
            self.m.bode_addr = a.to_string();
        }
        if was != (self.m.bode_usb, self.m.bode_addr.clone()) {
            self.ext_disconnect();
        }
        true
    }

    /// The load the generator drives (`:EXTGen:LOAD`).
    pub(super) fn ext_set_load(&mut self, hiz: bool) {
        if self.m.ext_hiz != hiz {
            self.m.ext_hiz = hiz;
            if self.ext.out && self.ext_linked() {
                self.ext_apply(true);
            }
        }
    }

    /// The `*IDN?` answer of the connected generator (`:EXTGen:IDN?`); empty without one.
    pub(super) fn ext_idn(&self) -> String {
        self.ext.shared.lock().unwrap().idn.clone().unwrap_or_default()
    }

    pub(super) fn ext_items(&self) -> Vec<Item> {
        let g = &GENERATORS[self.ext_model()];
        let mut v = vec![Item::ExtModel, Item::ExtLink];
        // the address only on the network
        if !self.m.bode_usb {
            v.push(Item::ExtAddr);
        }
        v.extend([Item::ExtState, Item::ExtIdn]);
        // channel and load only for a model whose commands name them
        if g.has("{c}") {
            v.push(Item::ExtCh);
        }
        if g.has("{z}") {
            v.push(Item::ExtLoad);
        }
        v.extend([Item::ExtFreq, Item::ExtAmpl, Item::ExtOut, Item::ExtConnect]);
        v
    }

    pub(super) fn ext_kind(it: Item) -> Option<Kind> {
        Some(match it {
            Item::ExtModel | Item::ExtLink | Item::ExtCh | Item::ExtLoad | Item::ExtOut => Kind::Choice,
            Item::ExtAddr | Item::ExtFreq | Item::ExtAmpl => Kind::Value,
            Item::ExtConnect => Kind::Action,
            _ => return None,
        })
    }

    /// The rows that only show something.
    pub(super) fn ext_text(it: Item) -> bool {
        matches!(it, Item::ExtState | Item::ExtIdn)
    }

    pub(super) fn ext_label(it: Item) -> Option<&'static str> {
        Some(match it {
            Item::Open(MenuId::ExtGen) => "Ext generator",
            Item::ExtModel => "Model",
            Item::ExtLink => "Connection",
            Item::ExtAddr => "Address",
            Item::ExtState => "State",
            Item::ExtIdn => "Identity",
            Item::ExtCh => "Gen channel",
            Item::ExtLoad => "Load",
            Item::ExtFreq => "Frequency",
            Item::ExtAmpl => "Amplitude",
            Item::ExtOut => "Output",
            Item::ExtConnect => "Connect / disconnect",
            _ => return None,
        })
    }

    pub(super) fn ext_value(&self, it: Item) -> Option<String> {
        let m = &self.m;
        // the thread names what it talks to "generator ..."; the row already says whose state it is
        let bare = |s: &str| s.strip_prefix("generator ").unwrap_or(s).to_string();
        Some(match it {
            Item::ExtModel => GENERATORS[self.ext_model()].name.into(),
            Item::ExtLink => {
                if m.bode_usb {
                    "USB".into()
                } else {
                    "LAN".into()
                }
            }
            Item::ExtAddr => m.bode_addr.clone(),
            Item::ExtState => match self.ext_conn() {
                Conn::Off => "Not connected".into(),
                Conn::Connecting => "Connecting".into(),
                Conn::On(what) => format!("Connected: {}", bare(&what)),
                Conn::Failed(why) => bare(&why),
            },
            Item::ExtIdn => match self.ext_idn() {
                idn if idn.is_empty() => "-".into(),
                idn => extgen::idn_short(&idn),
            },
            Item::ExtCh => (m.ext_ch.min(1) + 1).to_string(),
            Item::ExtLoad => {
                if m.ext_hiz {
                    "High-Z".into()
                } else {
                    "50 \u{3a9}".into()
                }
            }
            Item::ExtFreq => si_short(m.ext_freq, "Hz"),
            Item::ExtAmpl => si_short(m.ext_ampl, "Vpp"),
            Item::ExtOut => {
                if self.ext.out {
                    "On".into()
                } else {
                    "Off".into()
                }
            }
            Item::ExtConnect => String::new(),
            _ => return None,
        })
    }

    pub(super) fn ext_options(&self, it: Item) -> Option<Vec<String>> {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect();
        Some(match it {
            Item::ExtModel => extgen::externals().map(|i| GENERATORS[i].name.to_string()).collect(),
            Item::ExtLink => v(&["LAN", "USB"]),
            Item::ExtCh => v(&["1", "2"]),
            Item::ExtLoad => v(&["High-Z", "50 \u{3a9}"]),
            Item::ExtOut => v(&["Off", "On"]),
            _ => return None,
        })
    }

    pub(super) fn ext_adjust(&mut self, it: Item, d: i32) -> bool {
        if !matches!(
            it,
            Item::ExtModel
                | Item::ExtLink
                | Item::ExtAddr
                | Item::ExtCh
                | Item::ExtLoad
                | Item::ExtFreq
                | Item::ExtAmpl
                | Item::ExtOut
        ) {
            return false;
        }
        // a running sweep keeps the generator it started with
        let ok = if it == Item::ExtOut { self.ext_output(!self.ext.out) } else { self.ext_free() };
        if let Err(why) = ok {
            self.toast(why);
        }
        if it == Item::ExtOut || ok.is_err() {
            return true;
        }
        let ladder = |x: f64, lo: f64, hi: f64| {
            let l: Vec<f64> = (-3..9)
                .flat_map(|e| [1.0, 2.0, 5.0].map(|k| k * 10f64.powi(e)))
                .filter(|&v| v >= lo * 0.999 && v <= hi * 1.001)
                .collect();
            let i = l.iter().position(|&v| v >= x * 0.999).unwrap_or(l.len() - 1);
            l[(i as i32 + d).clamp(0, l.len() as i32 - 1) as usize]
        };
        match it {
            Item::ExtModel => {
                let all: Vec<usize> = extgen::externals().collect();
                let i = all.iter().position(|&g| g == self.ext_model()).unwrap_or(0);
                self.ext_set_model(all[(i as i32 + d).rem_euclid(all.len() as i32) as usize]);
            }
            // two choices: an odd step switches. Another place is another connection
            Item::ExtLink => {
                self.m.bode_usb ^= d % 2 != 0;
                self.ext_disconnect();
            }
            Item::ExtAddr => {
                // the knob moves the last number of an IPv4 address
                let m = &mut self.m;
                let (head, tail) = m.bode_addr.rsplit_once('.').unwrap_or(("192.168.1", "100"));
                let (oct, port) = tail.split_once(':').map_or((tail, None), |(o, p)| (o, Some(p)));
                let o = (oct.parse::<i32>().unwrap_or(100) + d).clamp(1, 254);
                m.bode_addr = match port {
                    Some(p) => format!("{}.{}:{}", head, o, p),
                    None => format!("{}.{}", head, o),
                };
                self.ext_disconnect();
            }
            Item::ExtCh => self.m.ext_ch = (self.m.ext_ch.min(1) as i32 + d).rem_euclid(2) as usize,
            Item::ExtLoad => self.m.ext_hiz ^= d % 2 != 0,
            Item::ExtFreq => self.m.ext_freq = ladder(self.m.ext_freq, FREQ.0, FREQ.1),
            Item::ExtAmpl => self.m.ext_ampl = ladder(self.m.ext_ampl, AMPL.0, AMPL.1),
            _ => {}
        }
        // an output that is on follows its settings (the model and the place are for the next Connect or Output on)
        if self.ext.out && self.ext_linked() {
            match it {
                Item::ExtFreq => self.ext_apply(false),
                Item::ExtAmpl | Item::ExtLoad | Item::ExtCh => self.ext_apply(true),
                _ => {}
            }
        }
        true
    }

    pub(super) fn ext_activate(&mut self, it: Item) -> bool {
        if it != Item::ExtConnect {
            return false;
        }
        let on = !self.ext_linked();
        if let Err(why) = self.ext_link(on) {
            self.toast(why);
        }
        true
    }

    /// `:EXTGen:FREQuency` and `:EXTGen:AMPLitude`: the value as given, within the range of the page.
    pub(super) fn ext_set(&mut self, f: Option<f64>, a: Option<f64>) {
        if let Some(f) = f {
            self.m.ext_freq = f.clamp(FREQ.0, FREQ.1);
        }
        if let Some(a) = a {
            self.m.ext_ampl = a.clamp(AMPL.0, AMPL.1);
        }
        if self.ext.out && self.ext_linked() && !self.ext_in_sweep() {
            self.ext_apply(a.is_some());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extgen::tests::fake_generator;

    /// Steps the screen until `f` holds (at most 5 s of the test's own clock).
    fn until(ui: &mut Ui, f: impl Fn(&Ui) -> bool) -> bool {
        for _ in 0..500 {
            ui.step(0.02);
            if f(ui) {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    /// What the generator got, without the error queries and the mark of a closed connection.
    fn lines_of(got: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        got.lock()
            .unwrap()
            .iter()
            .filter(|l| *l != "SYSTem:ERRor?" && *l != crate::extgen::tests::CLOSED)
            .cloned()
            .collect()
    }

    #[test]
    fn the_page_is_in_the_start_menu_and_shows_its_rows() {
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        assert!(ui.items(MenuId::Main).contains(&Item::Open(MenuId::ExtGen)));
        assert_eq!(Ui::label(Item::Open(MenuId::ExtGen)), "Ext generator");
        ui.open_menu(MenuId::ExtGen);
        // the first external model, on the LAN with its address: no channel or load row it would not use
        let items = ui.items(MenuId::ExtGen);
        assert_eq!(
            items,
            [
                Item::ExtModel,
                Item::ExtLink,
                Item::ExtAddr,
                Item::ExtState,
                Item::ExtIdn,
                Item::ExtCh,
                Item::ExtFreq,
                Item::ExtAmpl,
                Item::ExtOut,
                Item::ExtConnect
            ]
        );
        assert_eq!(
            (ui.value(Item::ExtModel), ui.value(Item::ExtLink), ui.value(Item::ExtState), ui.value(Item::ExtIdn)),
            ("APPL".into(), "LAN".into(), "Not connected".into(), "-".into())
        );
        assert_eq!(ui.options(Item::ExtModel), ["APPL", "BSWV", "SCPI", "CHANnel", "OWON DGE"]);
        // USB: no address. The OWON DGE: channel and load
        ui.adjust(Item::ExtLink, 1);
        ui.adjust(Item::ExtModel, -1);
        assert!(ui.m.bode_usb && ui.value(Item::ExtModel) == "OWON DGE");
        let items = ui.items(MenuId::ExtGen);
        assert!(!items.contains(&Item::ExtAddr) && items.contains(&Item::ExtCh) && items.contains(&Item::ExtLoad));
        assert_eq!((ui.value(Item::ExtCh), ui.value(Item::ExtLoad)), ("1".into(), "High-Z".into()));
        ui.adjust(Item::ExtModel, -2);
        assert_eq!(ui.value(Item::ExtModel), "SCPI");
        assert!(!ui.items(MenuId::ExtGen).contains(&Item::ExtCh), "plain SCPI names no channel");
        let mut cv = Canvas::new(PANEL_W, PANEL_H);
        ui.render(&mut cv);
        // the address: the knob moves its last number, the port stays
        ui.adjust(Item::ExtLink, 1);
        ui.adjust(Item::ExtAddr, 5);
        assert_eq!(ui.m.bode_addr, "192.168.1.105:5555");
    }

    #[test]
    fn no_usb_device_says_so_and_output_needs_a_connection() {
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        ui.m.bode_usb = true;
        ui.adjust(Item::ExtOut, 1);
        assert!(!ui.ext.out && ui.toast.as_ref().unwrap().0 == "Connect the generator first");
        ui.activate(Item::ExtConnect);
        assert!(ui.toast.as_ref().unwrap().0.contains("USB port"));
        // no USB device in the tests: the message says so, and the State row keeps the reason
        assert!(
            until(&mut ui, |u| u.toast.as_ref().is_some_and(|t| t.0.starts_with("generator USB: no USB device found"))),
            "{:?}",
            ui.toast
        );
        assert!(
            !ui.ext_linked() && ui.value(Item::ExtState).starts_with("USB: no USB device found"),
            "{}",
            ui.value(Item::ExtState)
        );
        // Connect again tries again
        ui.activate(Item::ExtConnect);
        assert!(ui.ext.tx.is_some());
    }

    #[test]
    fn connect_picks_the_model_and_the_output_sends_its_commands() {
        let (addr, got) = fake_generator("OWON,DGE2070,12345678,SCPI:99.0 FV:V1.0.4.7.0", "90000000Hz");
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        ui.m.bode_addr = addr.clone();
        assert_eq!(ui.value(Item::ExtModel), "APPL");
        ui.activate(Item::ExtConnect);
        assert!(until(&mut ui, |u| u.value(Item::ExtState).starts_with("Connected")), "{}", ui.value(Item::ExtState));
        assert_eq!(ui.value(Item::ExtState), format!("Connected: {}", addr));
        // the model from the answer; the serial number and the firmware are not shown
        assert!(
            until(&mut ui, |u| u.toast.as_ref().is_some_and(|t| t.0.ends_with(": OWON DGE2070"))),
            "{:?}",
            ui.toast
        );
        assert_eq!((ui.value(Item::ExtModel), ui.value(Item::ExtIdn)), ("OWON DGE".into(), "OWON DGE2070".into()));
        // output on: load, sine, offset, amplitude, then the frequency, then the switch
        (ui.m.ext_freq, ui.m.ext_ampl) = (1e3, 0.5);
        ui.adjust(Item::ExtOut, 1);
        assert!(ui.ext.out && ui.value(Item::ExtOut) == "On");
        assert!(until(&mut ui, |u| u.ext.shared.lock().unwrap().done == u.ext.sent));
        assert_eq!(
            lines_of(&got)[1..],
            [
                "OUTPut1:IMPedance INFinity",
                "SOURce1:FUNCtion:SHAPe SIN",
                "SOURce1:VOLTage:LEVel:IMMediate:OFFSet 0mV",
                "SOURce1:VOLTage:LEVel:IMMediate:AMPLitude 0.5Vpp",
                "SOURce1:FREQuency:FIXed 1000Hz",
                "OUTPut1:STATe ON"
            ]
        );
        // a new frequency goes out alone, with the switch as it is
        got.lock().unwrap().clear();
        ui.adjust(Item::ExtFreq, 1);
        assert_eq!(ui.m.ext_freq, 2e3);
        assert!(until(&mut ui, |u| u.ext.shared.lock().unwrap().done == u.ext.sent));
        assert_eq!(lines_of(&got), ["SOURce1:FREQuency:FIXed 2000Hz", "OUTPut1:STATe ON"]);
        // what the generator refuses is said on the screen, and the connection stays
        ui.ext_set(Some(90e6), None);
        assert!(
            until(&mut ui, |u| u
                .toast
                .as_ref()
                .is_some_and(|t| t.0.starts_with("The generator refused SOURce1:FREQuency:FIXed 90000000Hz"))),
            "{:?}",
            ui.toast
        );
        assert!(ui.ext_linked());
        // the second channel at 50 ohms
        got.lock().unwrap().clear();
        ui.adjust(Item::ExtCh, 1);
        ui.adjust(Item::ExtLoad, 1);
        assert!(until(&mut ui, |u| u.ext.shared.lock().unwrap().done == u.ext.sent));
        assert!(lines_of(&got).contains(&"OUTPut2:IMPedance 50".to_string()), "{:?}", got.lock().unwrap());
        // off, then disconnect
        got.lock().unwrap().clear();
        ui.adjust(Item::ExtOut, 1);
        assert!(until(&mut ui, |u| u.ext.shared.lock().unwrap().done == u.ext.sent));
        assert_eq!(lines_of(&got), ["OUTPut2:STATe OFF"]);
        ui.activate(Item::ExtConnect);
        assert!(!ui.ext_linked() && ui.value(Item::ExtState) == "Not connected" && ui.value(Item::ExtIdn) == "-");
        // nothing was asked that this generator never answers
        assert!(!got.lock().unwrap().iter().any(|l| l.contains("ESR")));
    }

    #[test]
    fn the_setup_keeps_the_generator() {
        let mut ui = Ui::new(Source::Demo { t: 0.0 });
        ui.m.bode_usb = true;
        ui.adjust(Item::ExtModel, -1);
        ui.adjust(Item::ExtCh, 1);
        ui.adjust(Item::ExtLoad, 1);
        (ui.m.ext_freq, ui.m.ext_ampl) = (25e3, 0.2);
        let text = crate::store::setup_text(&ui.m);
        assert!(text.contains("extgen = 6 2 50 25000 0.2\n"), "{}", text);
        let m = crate::store::parse_setup(&text).unwrap();
        assert_eq!(
            (m.ext_model, m.ext_ch, m.ext_hiz, m.ext_freq, m.ext_ampl, m.bode_usb),
            (6, 1, false, 25e3, 0.2, true)
        );
        // the Bode sweep on External is the model of the page, in the file too
        ui.adjust(Item::BodeGen, 1);
        assert_eq!((ui.value(Item::BodeGen), ui.m.bode_gen), ("External".into(), 6));
        ui.adjust(Item::ExtModel, 1);
        assert_eq!((ui.m.ext_model, ui.m.bode_gen), (1, 1));
        // a file from before the page: the generator of its Bode line
        let old: String = crate::store::setup_text(&ui.m)
            .lines()
            .filter(|l| !l.starts_with("extgen"))
            .map(|l| format!("{}\n", l.replace(" 1 192.168.1.100:5555", " 5 192.168.1.100:5555")))
            .collect();
        let m = crate::store::parse_setup(&old).unwrap();
        assert_eq!((m.bode_gen, m.ext_model, m.ext_hiz), (5, 5, true));
        // the built-in generator leaves the model of the page alone
        ui.m.bode_gen = extgen::BUILTIN;
        let m = crate::store::parse_setup(&crate::store::setup_text(&ui.m)).unwrap();
        assert_eq!((m.bode_gen, m.ext_model), (extgen::BUILTIN, 1));
    }
}
