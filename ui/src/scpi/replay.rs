// SPDX-License-Identifier: GPL-2.0-or-later
//! Runs a file of SCPI commands on this scope: a recording of the SCPI recorder (scpi/recorder.rs), a set-up saved
//! as commands (scpi/setupscpi.rs), or lines written by hand (Storage > Run SCPI file, the script line
//! `replay <file>`).
//!
//! The lines go through this interpreter like a client's, one command a frame: the screen follows, and what a
//! command leaves for the next frame (a screenshot) is done before the next command. A line that waits for an
//! operation (`*OPC?` after a Single or an auto set-up) is asked again every frame until it has ended, for at most
//! `WAIT_MAX`. A comment line (`#`) and an empty line are no commands. What a line is refused is the file's and not
//! a client's: the command is run with an error queue and status of its own, a client's are put back as they were,
//! and the screen says at the end what was refused. Being a client's, what the file sets is not written by the SCPI
//! recorder.
//!
//! A file is not a client on the network (ADR 0033). It is run by a tap on its name, by a person who has not read it,
//! and it may come from a stick: it sets the scope up and runs it, and no more. What it may not send is refused by
//! the interpreter before any handler (`file_refuses`): a command that takes the word `CONFIRM` (the word is the
//! author's, not the person's who tapped the file), a press on the screen (`:UI:`), a command of `:SYSTem:` that
//! is not a setting of the set-up, the start of a self-calibration, `:MMEMory:`, and a file named outside the save
//! folder. Each is said with what was refused. A client sends such a command itself.

use super::*;

/// The longest a command may wait for its operation, in seconds of the scope's time: a Single that finds no trigger.
const WAIT_MAX: f64 = 30.0;
/// The largest file that is run.
const FILE_MAX: u64 = 1 << 20;

pub(crate) struct Replay {
    name: String,
    lines: std::collections::VecDeque<String>,
    /// the command that is run now, while it waits for its operation, and since when it does
    program: Option<Program>,
    waiting_since: f64,
    /// the commands run so far, and what they were refused
    commands: usize,
    refused: Vec<String>,
}

impl Scpi {
    /// Once a frame (`poll`): the stop, a file to start, the next command of the file that runs.
    pub(crate) fn replay_tick(&mut self, ui: &mut Ui, cv: &Canvas) {
        if std::mem::take(&mut ui.scpi_replay_cancel) {
            if let Some(replay) = self.replay.take() {
                ui.scpi_replaying = false;
                ui.toast(&format!("Stopped {} after {} command{}", replay.name, replay.commands, plural(replay.commands)));
            }
        }
        if let Some(path) = ui.scpi_replay_file.take() { self.replay_start(ui, &path); }
        let Some(mut replay) = self.replay.take() else { return };
        let mut program = match replay.program.take() {
            Some(program) => program,
            None => loop {
                match replay.lines.pop_front() {
                    None => return Self::replay_end(ui, replay),
                    Some(line) if line.trim().is_empty() || line.trim_start().starts_with('#') => {}
                    Some(line) => {
                        (replay.commands, replay.waiting_since) = (replay.commands + 1, ui.now);
                        break Program::new(&line);
                    }
                }
            },
        };
        // the command has an error queue and status of its own: what it is refused is the file's, and a client's
        // queue, event status and last error are afterwards as they were, at their bound too
        // so are its enable registers, and the operations a client waits for with `*OPC`: polled with the file's
        // status they would be taken off the list and their bit lost with it
        let kept = (std::mem::take(&mut self.err), self.event_status, self.error_serial, self.last_error.take());
        let client = (std::mem::take(&mut self.opc_events), self.event_enable, self.service_enable, self.power_on_clear);
        self.from_file = true;
        let done = self.poll_program(ui, cv, &mut program).is_some();
        self.from_file = false;
        replay.refused.append(&mut self.err);
        (self.err, self.event_status, self.error_serial, self.last_error) = kept;
        (self.opc_events, self.event_enable, self.service_enable, self.power_on_clear) = client;
        if !done {
            if ui.now - replay.waiting_since >= WAIT_MAX {
                ui.scpi_replaying = false;
                return ui.toast(&format!("{}: command {} waited {WAIT_MAX:.0} s for its operation; stopped", replay.name, replay.commands));
            }
            replay.program = Some(program);
        }
        self.replay = Some(replay);
    }

    fn replay_start(&mut self, ui: &mut Ui, path: &std::path::Path) {
        let name = crate::store::file_name(path);
        if self.replay.is_some() { return ui.toast("A SCPI file is running; stop it first"); }
        // the file is foreign data: a link or a device node under its name is not followed (a read of one may never
        // end), and no more than the bound is read whatever the size says
        let text = match std::fs::symlink_metadata(path) {
            Ok(file) if !file.is_file() => Err("not a regular file".to_string()),
            Ok(file) if file.len() > FILE_MAX => Err("larger than 1 MiB".to_string()),
            Ok(_) => read_bounded(path).and_then(|bytes| match bytes.len() as u64 > FILE_MAX {
                true => Err("larger than 1 MiB".to_string()),
                // a file an editor saved: a byte order mark before the first line, a letter that is not UTF-8 in a comment
                false => Ok(String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).into_owned()),
            }),
            Err(e) => Err(e.to_string()),
        };
        match text {
            Ok(text) => {
                // a line ends with LF, CR LF or CR alone: an empty line is no command
                let lines = text.split(['\n', '\r']).map(str::to_string).collect();
                self.replay = Some(Box::new(Replay { name: name.clone(), lines, program: None, waiting_since: ui.now, commands: 0, refused: Vec::new() }));
                ui.scpi_replaying = true;
                ui.toast(&format!("Running {name}"));
            }
            Err(why) => ui.toast(&format!("Run failed: {name}: {why}")),
        }
    }

    fn replay_end(ui: &mut Ui, replay: Box<Replay>) {
        ui.scpi_replaying = false;
        let (n, refused) = (replay.commands, replay.refused.len());
        match replay.refused.first() {
            None => ui.toast(&format!("Ran {}: {n} command{}", replay.name, plural(n))),
            Some(first) => ui.toast(&format!("{}: {refused} of {n} command{} refused, first {first}", replay.name, plural(n))),
        }
    }
}

/// At most `FILE_MAX` + 1 bytes of a file.
fn read_bounded(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path).and_then(|file| file.take(FILE_MAX + 1).read_to_end(&mut bytes)).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// What a file may not send, and why; `None` for a command that is run (ADR 0033). `head` is the header of a
/// command that is no query, `arg` its arguments.
pub(super) fn file_refuses(head: &str, arg: &str) -> Option<&'static str> {
    let nodes: Vec<&str> = head.trim_start_matches(':').split(':').collect();
    let (first, last) = (nodes[0], nodes[nodes.len() - 1]);
    // as the commands that need the word read it (`ui/scpiext_c.rs`): one argument, not in quotes
    if arg.split(',').any(|part| part.trim().eq_ignore_ascii_case("CONFIRM")) { return Some("CONFIRM is not taken from a file"); }
    if kw(first, "UI") { return Some("a file does not press the screen"); }
    if kw(first, "MMEMory") { return Some("a file does not delete a file"); }
    if kw(first, "CALibrate") && kw(last, "STARt") { return Some("a self-calibration is started at the scope"); }
    if kw(first, "SYSTem") && !system_setting(&nodes) { return Some("a file sets the scope up, not its system"); }
    let names_file = kw(first, "SAVE") || kw(first, "LOAD") || ["SAVE", "LOAD", "EXPort", "IMPort"].iter().any(|k| kw(last, k));
    if names_file && leaves_folder(arg) { return Some("a file names files of the save folder only"); }
    None
}

/// A command of `:SYSTem:` that sets a part of the set-up: the key = value command, the set-up as a whole, and the
/// headers the set-up is written with (`setupscpi::ROWS`).
fn system_setting(nodes: &[&str]) -> bool {
    if nodes.len() == 2 && kw(nodes[1], "SETup") { return true; }
    setupscpi::ROWS.iter().flat_map(|(_, headers)| headers.iter()).any(|header| {
        let parts: Vec<&str> = header.trim_start_matches(':').split(':').collect();
        parts.len() == nodes.len() && parts.iter().zip(nodes).all(|(part, node)| kw(suffix(node).0, part.trim_end_matches("<n>")))
    })
}

/// A file name that is not one of the save folder: an absolute path, a drive, or a step up.
fn leaves_folder(arg: &str) -> bool {
    let name = arg.split(',').next().unwrap_or("").trim().trim_matches(['"', '\'']);
    name.starts_with(['/', '\\']) || name.contains(':') || arg.split(['/', '\\', ',', '"', '\'']).any(|part| part.trim() == "..")
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wave::Source;

    fn session() -> (Scpi, Ui, Canvas) {
        (Scpi::open(0), Ui::new(Source::Demo { t: 0.0 }), Canvas::new(crate::ui::W, crate::ui::H))
    }

    fn text(ui: &mut Ui) -> String {
        ui.sync_view_state();
        crate::store::setup_text(&ui.m)
    }

    fn folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("oos-replay-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Frames of the scope and of the interpreter until the file has ended, at most `frames`.
    fn run(s: &mut Scpi, ui: &mut Ui, cv: &Canvas, frames: usize) {
        for _ in 0..frames {
            ui.step(0.05);
            s.poll(ui, cv);
            if !ui.scpi_replaying && ui.scpi_replay_file.is_none() { return; }
        }
    }

    /// A recording made on one scope, with settings, a single sweep and its wait, run on a second one: the same
    /// set-up, nothing refused, and nothing in the queue of a client.
    #[test]
    fn a_recording_runs_on_a_second_scope() {
        use crate::ui::key;
        let dir = folder("recording");
        let (mut s, mut ui, cv) = session();
        ui.store_dir = dir.clone();
        for _ in 0..20 { ui.step(0.05); }
        ui.scpi_rec_want = true;
        s.poll(&mut ui, &cv);
        for (key, value) in [("tdiv", "0.001"), ("ch2.on", "true"), ("ch1.coupling", "Ac")] {
            crate::store::set(&mut ui.m, key, value).unwrap();
            ui.mark_changed();
            ui.now += 0.05;
            s.poll(&mut ui, &cv);
            ui.now += 1.0;
            s.poll(&mut ui, &cv);
        }
        ui.key(key::SINGLE);
        ui.now += 0.1;
        s.poll(&mut ui, &cv);
        ui.scpi_rec_want = false;
        s.poll(&mut ui, &cv);
        let want = text(&mut ui);
        let file = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).find(|p| p.extension().is_some_and(|x| x == "scpi")).unwrap();
        let lines = std::fs::read_to_string(&file).unwrap();
        assert!(lines.contains(":SINGle\n*OPC?\n") && lines.contains("\n# "), "{lines}");

        let (mut s, mut ui, cv) = session();
        ui.scpi_replay_file = Some(file.clone());
        s.poll(&mut ui, &cv);
        assert!(ui.scpi_replaying);
        run(&mut s, &mut ui, &cv, 400);
        let commands = lines.lines().filter(|l| !l.starts_with('#') && !l.trim().is_empty()).count();
        assert_eq!(ui.toast_text(), format!("Ran {}: {commands} commands", crate::store::file_name(&file)));
        assert!(Ui::toast_is_success(&ui.toast_text()));
        assert_eq!(super::super::setupscpi::changed(&want, &text(&mut ui)), Vec::new(), "{lines}");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One command a frame; a refused line is named at the end, the others are run, and a client's error queue
    /// keeps what was in it and gets nothing.
    #[test]
    fn a_refused_line_is_said_and_is_not_a_clients_error() {
        let dir = folder("refused");
        let file = dir.join("hand.scpi");
        std::fs::write(&file, "# by hand\n\n:NO:SUCH:HEADer 1\n  # indented comment\n:CHANnel2:DISPlay ON\n:CHANnel3:DISPlay ON\n").unwrap();
        let (mut s, mut ui, cv) = session();
        s.exec_line(&mut ui, &cv, ":ANOTHer:UNKNown 1");
        let queued = s.queued_errors().to_vec();
        assert_eq!(queued.len(), 1);
        ui.scpi_replay_file = Some(file);
        s.poll(&mut ui, &cv);
        assert_eq!(ui.toast_text(), "Running hand.scpi");
        assert!(Ui::toast_is_info(&ui.toast_text()));
        assert!(!ui.m.ch[1].on, "the first command is the refused one");
        s.poll(&mut ui, &cv);
        assert!(ui.m.ch[1].on && !ui.m.ch[2].on, "one command a frame");
        run(&mut s, &mut ui, &cv, 10);
        assert!(ui.m.ch[2].on && !ui.scpi_replaying);
        let message = ui.toast_text();
        assert!(message.starts_with("hand.scpi: 1 of 3 commands refused, first -113"), "{message}");
        assert!(!Ui::toast_is_success(&message));
        assert_eq!(s.queued_errors(), queued);
        // a file that is not there
        ui.scpi_replay_file = Some(dir.join("none.scpi"));
        s.poll(&mut ui, &cv);
        assert!(!ui.scpi_replaying && ui.toast_text().starts_with("Run failed: none.scpi"), "{}", ui.toast_text());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a file is refused does not reach a client: not its queue, also when that is full, not its event status,
    /// not its last error. A file an editor saved (a byte order mark, CR LF, a letter that is not UTF-8) runs.
    #[test]
    fn a_files_errors_leave_a_clients_status_as_it_was() {
        let dir = folder("status");
        let file = dir.join("notepad.scpi");
        std::fs::write(&file, b"\xef\xbb\xbf:CHANnel2:DISPlay ON\r\n# caf\xe9\r\n:NO:SUCH:HEADer 1\r\n:CHANnel3:DISPlay ON\r\n").unwrap();
        let (mut s, mut ui, cv) = session();
        let ask = |s: &mut Scpi, ui: &mut Ui, c: &str| String::from_utf8(s.exec_line(ui, &cv, c)).unwrap();
        ui.scpi_replay_file = Some(file.clone());
        run(&mut s, &mut ui, &cv, 20);
        assert!(ui.m.ch[1].on && ui.m.ch[2].on, "the first line, after the mark, and the last");
        assert!(ui.toast_text().starts_with("notepad.scpi: 1 of 3 commands refused, first -113"), "{}", ui.toast_text());
        assert_eq!(ask(&mut s, &mut ui, "*ESR?"), "0", "no event of a client");
        assert_eq!(ask(&mut s, &mut ui, ":SYSTem:ERRor?"), "0,\"No error\"");
        // a client that never read its errors: its queue is full, and stays what it was
        for i in 0..40 { ask(&mut s, &mut ui, &format!(":NO:CLIent:HEADer{i} 1")); }
        let queued = s.queued_errors().to_vec();
        ui.scpi_replay_file = Some(file);
        run(&mut s, &mut ui, &cv, 20);
        assert!(ui.toast_text().starts_with("notepad.scpi: 1 of 3 commands refused, first -113"), "{}", ui.toast_text());
        assert_eq!(s.queued_errors(), queued);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A line with CONFIRM is not run from a file and is said; a file of comments runs no command; a second file
    /// while one runs is not started.
    #[test]
    fn a_file_does_not_confirm_and_one_file_runs_at_a_time() {
        let dir = folder("confirm");
        let file = dir.join("guarded.scpi");
        std::fs::write(&file, ":CHANnel2:DISPlay ON\n:CHANnel3:DISPlay ON,CONFIRM\n:CHANnel4:DISPlay on , confirm\n").unwrap();
        let (mut s, mut ui, cv) = session();
        ui.scpi_replay_file = Some(file.clone());
        run(&mut s, &mut ui, &cv, 20);
        assert!(ui.m.ch[1].on && !ui.m.ch[2].on && !ui.m.ch[3].on);
        let message = ui.toast_text();
        assert!(message.starts_with("guarded.scpi: 2 of 3 commands refused, first -203") && message.contains("not taken from a file"), "{message}");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        let empty = dir.join("empty.scpi");
        std::fs::write(&empty, "# nothing\n\n").unwrap();
        ui.scpi_replay_file = Some(empty);
        run(&mut s, &mut ui, &cv, 5);
        assert_eq!(ui.toast_text(), "Ran empty.scpi: 0 commands");
        // one at a time
        let wait = dir.join("wait.scpi");
        std::fs::write(&wait, ":SINGle\n*OPC?\n").unwrap();
        ui.scpi_replay_file = Some(wait);
        for _ in 0..4 { s.poll(&mut ui, &cv); }
        assert!(ui.scpi_replaying);
        ui.scpi_replay_file = Some(file);
        s.poll(&mut ui, &cv);
        assert!(ui.scpi_replaying && ui.toast_text() == "A SCPI file is running; stop it first", "{}", ui.toast_text());
        ui.scpi_replay_cancel = true;
        s.poll(&mut ui, &cv);
        assert!(ui.toast_text().starts_with("Stopped wait.scpi"), "{}", ui.toast_text());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What a file may not send (ADR 0033), by the command: what switches the scope off, presses its screen, changes
    /// its system, starts a self-calibration, deletes a file or names one outside the save folder. What sets the
    /// scope up is sent, also where it looks alike: a label that begins with a slash or is the word in quotes.
    #[test]
    fn what_a_file_may_not_send() {
        let refused = |line: &str| {
            let (head, arg) = line.split_once(' ').unwrap_or((line, ""));
            file_refuses(head, arg.trim())
        };
        for (line, why) in [
            (":SYSTem:POWer:OFF CONFIRM", "CONFIRM"),
            (":SYST:POW:REST confirm", "CONFIRM"),
            (":SYSTem:UPDate:ROLLback 1, Confirm", "CONFIRM"),
            (":UI:TAP 100,100", "press the screen"),
            (":UI:MENU:SELect \"Power off\"", "press the screen"),
            (":UI:DRAG 1,2,3,4", "press the screen"),
            (":SYSTem:REMote:SERVice:TELNet ON", "not its system"),
            (":SYST:REM:SERV:SCPI OFF", "not its system"),
            (":SYSTem:WLAN:FORGet \"bench\"", "not its system"),
            (":SYSTem:RECorder:STATe ON", "not its system"),
            (":SYSTem:DATE 2020,1,1", "not its system"),
            (":SYSTem:NAME \"other\"", "not its system"),
            (":CALibrate:STARt 0", "self-calibration"),
            (":CAL:STAR", "self-calibration"),
            (":MMEMory:DELete \"shot-001.png\"", "delete"),
            (":SAVE:SETup \"/oos/system.conf\"", "save folder"),
            (":SAVE:IMAGe \"../../oos/x.png\"", "save folder"),
            (":SAVE:WAVeform \"a/../../x.csv\",CSV", "save folder"),
            (":SAVE:SESSion \"C:\\x.oss\"", "save folder"),
            (":SAVE:MEMory:WAVeform /mnt/usb/x.csv", "save folder"),
            (":LOAD:SETup '/proc/kmsg'", "save folder"),
            (":MASK:SAVE \"/etc/x\"", "save folder"),
            (":REFerence:LOAD \"\\x\"", "save folder"),
            (":SOURce1:ARB:LOAD \"/dev/zero\"", "save folder"),
        ] {
            assert!(refused(line).is_some_and(|said| said.contains(why)), "{line}: {:?}", refused(line));
        }
        for line in [
            ":SYSTem:SET tdiv = 1e-3", ":SYST:SET tdiv = 1e-3", ":SYSTem:SETup #15hello", ":SYSTem:LOCK ON", ":SYST:LOG:INT 2",
            ":SYSTem:AOUTput 1", ":CHANnel1:LABel \"/CS\"", ":CHANnel1:LABel \"Confirm\"",
            ":BUS1:LABel \"a..b\"", ":SAVE:IMAGe \"shots/a.png\"", ":SAVE:SETup", ":SAVE:SLOT 3", ":REFerence:SAVE 2",
            ":LOAD:SESSion \"bench.oss\"", ":CALibrate:DESKew 1,0", ":AUToscale", ":SINGle", ":TIMebase:SCALe 1e-3",
        ] {
            assert_eq!(refused(line), None, "{line}");
        }
    }

    /// Run from a file, such a command is refused and said, nothing of it is done, and the rest of the file runs: the
    /// file outside the save folder is not written, the one in it is. A file whose lines end with CR alone runs.
    #[test]
    fn a_refused_command_does_nothing_and_the_file_goes_on() {
        let dir = folder("authority");
        let (outside, file) = (dir.join("outside.txt"), dir.join("stick.scpi"));
        let saves = dir.join("saves");
        std::fs::create_dir_all(&saves).unwrap();
        let lines = [
            "# a file from a stick".to_string(), ":CHANnel2:DISPlay ON".to_string(), ":UI:MENU:SELect \"Power off\"".to_string(),
            ":SYSTem:RECorder:STATe ON".to_string(), ":CALibrate:STARt 0".to_string(),
            format!(":SAVE:SETup \"{}\"", outside.display()), ":SAVE:SETup \"kept.txt\"".to_string(), ":CHANnel3:DISPlay ON".to_string(),
        ];
        std::fs::write(&file, lines.join("\r")).unwrap();
        let (mut s, mut ui, cv) = session();
        ui.store_dir = saves.clone();
        s.exec_line(&mut ui, &cv, ":SAVE:DESTination DESKtop");
        ui.scpi_replay_file = Some(file);
        run(&mut s, &mut ui, &cv, 30);
        assert!(ui.m.ch[1].on && ui.m.ch[2].on, "the lines before and after are run");
        assert!(!ui.scpi_rec_want && !outside.exists(), "nothing of a refused command is done");
        assert!(saves.join("kept.txt").exists(), "a file of the save folder is written");
        let message = ui.toast_text();
        assert!(message.starts_with("stick.scpi: 4 of 7 commands refused, first -203,\"Command protected\" a file does not press the screen"), "{message}");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A client that waits for its operation with `*OPC` has its bit also when the operation ends while a file runs,
    /// and keeps its enable registers whatever the file sends.
    #[test]
    fn a_clients_opc_and_enables_outlive_a_file() {
        let dir = folder("opc");
        let file = dir.join("long.scpi");
        std::fs::write(&file, "*ESE 0\n*SRE 0\n*OPC\n".to_string() + &":CHANnel2:DISPlay ON\n".repeat(40)).unwrap();
        let (mut s, mut ui, cv) = session();
        let ask = |s: &mut Scpi, ui: &mut Ui, c: &str| String::from_utf8(s.exec_line(ui, &cv, c)).unwrap();
        ask(&mut s, &mut ui, "*ESE 1");
        ask(&mut s, &mut ui, "*SRE 32");
        ask(&mut s, &mut ui, ":SINGle");
        ask(&mut s, &mut ui, "*OPC");
        ui.scpi_replay_file = Some(file);
        // the file starts before the scope goes on: the sweep ends while it runs
        s.poll(&mut ui, &cv);
        assert!(ui.scpi_replaying);
        run(&mut s, &mut ui, &cv, 60);
        assert!(!ui.scpi_replaying && ui.m.ch[1].on, "{}", ui.toast_text());
        assert_eq!((ask(&mut s, &mut ui, "*ESE?"), ask(&mut s, &mut ui, "*SRE?")), ("1".to_string(), "32".to_string()));
        assert_eq!(ask(&mut s, &mut ui, "*ESR?"), "1", "the operation of the client is complete, and said once");
        assert_eq!(ask(&mut s, &mut ui, "*ESR?"), "0");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// What is not a regular file is not read: a folder, and a link (to a device a read of which never ends).
    #[test]
    fn a_folder_or_a_link_is_not_run() {
        let dir = folder("link");
        let (mut s, mut ui, cv) = session();
        ui.scpi_replay_file = Some(dir.clone());
        run(&mut s, &mut ui, &cv, 3);
        assert!(!ui.scpi_replaying && ui.toast_text().ends_with("not a regular file"), "{}", ui.toast_text());
        #[cfg(unix)]
        {
            let (real, link) = (dir.join("real.scpi"), dir.join("link.scpi"));
            std::fs::write(&real, ":CHANnel2:DISPlay ON\n").unwrap();
            std::os::unix::fs::symlink(&real, &link).unwrap();
            ui.scpi_replay_file = Some(link);
            run(&mut s, &mut ui, &cv, 3);
            assert!(!ui.m.ch[1].on && ui.toast_text().ends_with("not a regular file"), "{}", ui.toast_text());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A command that waits for an operation that does not end: the file can be stopped, and stops by itself.
    #[test]
    fn a_wait_can_be_stopped_and_ends_by_itself() {
        let dir = folder("wait");
        let file = dir.join("wait.scpi");
        std::fs::write(&file, ":TRIGger:SWEep NORMal\n:TRIGger:EDGE:LEVel 4\n:SINGle\n*OPC?\n:CHANnel2:DISPlay ON\n").unwrap();
        let (mut s, mut ui, cv) = session();
        ui.scpi_replay_file = Some(file.clone());
        // no frames of the scope: the single sweep never finds its trigger
        for _ in 0..8 { s.poll(&mut ui, &cv); }
        assert!(ui.scpi_replaying && !ui.m.ch[1].on, "waiting at the fence");
        ui.scpi_replay_cancel = true;
        s.poll(&mut ui, &cv);
        assert!(!ui.scpi_replaying && ui.toast_text() == "Stopped wait.scpi after 4 commands", "{}", ui.toast_text());
        // again, and the scope's time goes past the longest wait
        ui.scpi_replay_file = Some(file);
        for _ in 0..8 { s.poll(&mut ui, &cv); }
        assert!(ui.scpi_replaying);
        ui.now += WAIT_MAX + 1.0;
        s.poll(&mut ui, &cv);
        assert!(!ui.scpi_replaying && ui.toast_text().contains("command 4 waited 30 s"), "{}", ui.toast_text());
        assert!(!ui.m.ch[1].on, "what comes after the wait is not run");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Storage > Run SCPI file on the scope: the files of the save folder to choose from, the newest first; the
    /// chosen one runs. Pressed while a file runs, the row stops it.
    #[test]
    fn the_storage_row_lists_the_files_and_runs_the_chosen_one() {
        let dir = folder("row");
        std::fs::write(dir.join("first.scpi"), ":CHANnel2:DISPlay ON\n").unwrap();
        std::fs::write(dir.join("second.scpi"), ":CHANnel3:DISPlay ON\n:SINGle\n*OPC?\n").unwrap();
        std::fs::write(dir.join("notes.txt"), "not commands\n").unwrap();
        let (mut s, mut ui, cv) = session();
        ui.store_dir = dir.clone();
        let ask = |s: &mut Scpi, ui: &mut Ui, c: &str| String::from_utf8(s.exec_line(ui, &cv, c)).unwrap();
        ui.script_menu("Storage").unwrap();
        ui.script_do("Run SCPI file").unwrap();
        let menu = ask(&mut s, &mut ui, ":UI:MENU?");
        assert!(menu.contains("first.scpi") && menu.contains("second.scpi") && !menu.contains("notes.txt"), "{menu}");
        ask(&mut s, &mut ui, ":UI:MENU:SELect \"first.scpi\"");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        run(&mut s, &mut ui, &cv, 20);
        assert!(ui.m.ch[1].on && ui.toast_text() == "Ran first.scpi: 1 command", "{}", ui.toast_text());
        // the second waits at its fence: the row is the stop now
        ui.script_menu("Storage").unwrap();
        ui.script_do("Run SCPI file").unwrap();
        ask(&mut s, &mut ui, ":UI:MENU:SELect \"second.scpi\"");
        for _ in 0..6 { s.poll(&mut ui, &cv); }
        assert!(ui.scpi_replaying && ui.m.ch[2].on);
        ui.script_menu("Storage").unwrap();
        ui.script_do("Stop SCPI file").unwrap();
        s.poll(&mut ui, &cv);
        assert!(!ui.scpi_replaying && ui.toast_text().starts_with("Stopped second.scpi"), "{}", ui.toast_text());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
