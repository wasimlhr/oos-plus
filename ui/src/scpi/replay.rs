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
//! A file has the authority of a client on the network, with one exception. The commands that switch the scope off,
//! install an update or close a service take the word `CONFIRM`, so that they are not sent by accident. In a file
//! the word is the author's, not the person's who tapped the file: a line with `CONFIRM` is not run, and is said
//! with what was refused. A client sends such a command itself.

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
                    Some(line) if confirms(&line) => {
                        replay.commands += 1;
                        replay.refused.push(format!("-203,\"Command protected\" CONFIRM is not taken from a file: {}", line.trim()));
                    }
                    Some(line) => {
                        (replay.commands, replay.waiting_since) = (replay.commands + 1, ui.now);
                        break Program::new(&line);
                    }
                }
            },
        };
        // the command has an error queue and status of its own: what it is refused is the file's, and a client's
        // queue, event status and last error are afterwards as they were, at their bound too
        let kept = (std::mem::take(&mut self.err), self.event_status, self.error_serial, self.last_error.take());
        let done = self.poll_program(ui, cv, &mut program).is_some();
        replay.refused.append(&mut self.err);
        (self.err, self.event_status, self.error_serial, self.last_error) = kept;
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
        let text = match std::fs::metadata(path) {
            Ok(file) if file.len() > FILE_MAX => Err("larger than 1 MiB".to_string()),
            // a file an editor saved: a byte order mark before the first line, a letter that is not UTF-8 in a comment
            Ok(_) => std::fs::read(path).map(|bytes| String::from_utf8_lossy(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(&bytes)).into_owned()).map_err(|e| e.to_string()),
            Err(e) => Err(e.to_string()),
        };
        match text {
            Ok(text) => {
                let lines = text.lines().map(str::to_string).collect();
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

/// A line that carries the word `CONFIRM` as one of its arguments.
fn confirms(line: &str) -> bool {
    line.split(|c: char| c == ',' || c == ';' || c.is_whitespace()).any(|word| word.trim_matches('"').eq_ignore_ascii_case("CONFIRM"))
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
        std::fs::write(&file, ":CHANnel2:DISPlay ON\n:CHANnel3:DISPlay ON,CONFIRM\n:CHANnel4:DISPlay on , \"confirm\"\n").unwrap();
        assert!(confirms(":SYSTem:X 1,CONFIRM") && confirms(":A:B confirm") && !confirms(":A:B CONFIRMED") && !confirms("# CONFIRMs nothing"));
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
