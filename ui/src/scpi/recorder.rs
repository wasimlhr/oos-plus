// SPDX-License-Identifier: GPL-2.0-or-later
//! The SCPI recorder: while it is on, what is set at the scope (touch, keys, knobs, menus) is written as SCPI
//! commands into a file (Storage > SCPI recorder, `:SYSTem:RECorder:STATe`).
//!
//! It does not follow the menu or the knobs. Once a frame the text of the set-up (`store::setup_text`) is looked at:
//! when it has changed and then stood still for `SETTLE`, the commands that take a scope from the set-up of the last
//! step to this one are made and proven on a second scope, as for `:SYSTem:SETup:SCPI?` (`Scpi::mend`,
//! scpi/setupscpi.rs). So a knob turned through twenty steps is one line with the value it was left at, and a
//! setting changed by a drag is seen like one changed in a menu. The file starts with the set-up at the start as
//! commands, so it can be sent to a scope in any state. It is written again after every step: a recording is not
//! lost when the program ends while it is on. The power going is another matter. The file is not synced while the
//! recorder is on, and the local folder is in memory: a recording outlasts the power only on a stick, and for
//! certain only once the recorder was switched off, when the file is synced like every saved one
//! (`Ui::scpi_write`).
//!
//! What a client sets by SCPI while the recorder is on is not the user's (`poll_program`): the set-up after the
//! client's message is the one the next step starts from, and a comment in the file says that a client set something
//! there. What the web remote's keys and touches and a script do is the user's: it comes in as the front panel does.
//!
//! A key or a row that is an action is written as its command, not as settings (`Ui::rec_action`): Run / Stop,
//! Single, Force, Clear, Default, Autoset, and the saves of the Storage menu. What the user set before it is written
//! first, also when it has not stood still yet; before Default that is only what Default leaves alone (the Quick key
//! action, the text size), since the reset takes the rest. The settings the action changes are its own: for Run,
//! Stop, Single, Force, Clear and Default the command is proven like a step, so a setting it leaves different on a
//! second scope gets a line after it; an auto set-up is written as the command alone, since what it finds is the
//! signal's, and a save is never run a second time. A saved set-up is `*SAV <n>` with the slot it went to: the slot
//! of the Storage menu is not part of the set-up, so no line before it would bring it. `*OPC?` stands after Single
//! and Autoset, the operations it waits for. While an auto set-up runs the look at the set-up waits: what the user
//! sets in that time is taken for the auto set-up's and is not written.

use super::setupscpi::named;
use super::*;

/// A setting is written when it has stood still this long, in seconds: a knob that is being turned is one line.
const SETTLE: f64 = 0.6;
/// A recording ends by itself at this many lines: one left on for a week must not fill the memory or the stick.
const LINES_MAX: usize = 20_000;
/// The comment that stands where a client set something.
const BY_CLIENT: &str = "set by a client, not recorded";

pub(crate) struct Recorder {
    /// the set-up the lines written so far stand for: the model at the last step
    base: Model,
    /// the set-up text at the last look
    seen: String,
    /// when it last changed (`Ui::now`), and whether that change waits to stand still
    changed_at: f64,
    pending: bool,
    /// an auto set-up runs: the settings it changes are its own, the look waits for its end
    hold: bool,
    started: f64,
    /// the file, and what it holds: comments (`#`) and commands
    path: std::path::PathBuf,
    pub(crate) lines: Vec<String>,
    /// the commands recorded after the start
    pub(crate) commands: usize,
}

impl Recorder {
    fn text(&self) -> String {
        self.lines.join("\n") + "\n"
    }

    /// The recording has its `LINES_MAX` lines: whatever wrote the last one, a setting or an action, switches the
    /// recorder off, and `rec_tick` saves the file as when the user does it.
    fn full(&self) -> bool {
        self.lines.len() >= LINES_MAX
    }
}

/// A comment line of the file: one line, and no `;` for a reader that splits there.
fn comment(text: &str) -> String {
    format!("# {}", text.replace([';', '\n', '\r'], ","))
}

impl Scpi {
    /// Once a frame, before the clients are read (`poll`): the switch, the look at the set-up, the step that is due.
    pub(crate) fn rec_tick(&mut self, ui: &mut Ui, cv: &Canvas) {
        // the actions first: one done in the frame the recorder is switched off is still one of the recording
        self.rec_actions(ui, cv);
        match (ui.scpi_rec_want, self.rec.is_some()) {
            (true, false) => self.rec_start(ui, cv),
            (false, true) => self.rec_stop(ui, cv),
            _ => {}
        }
        let Some(rec) = self.rec.as_mut() else { return };
        ui.sync_view_state();
        let now = crate::store::setup_text(&ui.m);
        if rec.hold {
            if ui.auto_running() { return; }
            // the auto set-up is done: the set-up it left is where the next step starts
            (rec.base, rec.seen, rec.pending, rec.hold) = (ui.m.clone(), now, false, false);
            return;
        }
        if now != rec.seen {
            (rec.seen, rec.changed_at, rec.pending) = (now, ui.now, true);
        }
        if rec.pending && ui.now - rec.changed_at >= SETTLE { self.rec_step(ui, cv); }
    }

    /// The actions since the last frame (`Ui::rec_action`), each written as its command.
    fn rec_actions(&mut self, ui: &mut Ui, cv: &Canvas) {
        let mut actions = std::mem::take(&mut ui.scpi_rec_actions);
        for i in 0..actions.len() {
            // the set-up after this action is the one before the next of the same frame, else the one the scope has
            let mut after = actions.get(i + 1).map(|next| next.1.clone());
            let (line, mut before) = (actions[i].0.clone(), std::mem::take(&mut actions[i].1));
            if let Some(model) = after.as_mut() { std::mem::swap(&mut ui.m, &mut **model); }
            self.rec_action_written(ui, cv, &line, &mut before);
            if let Some(model) = after.as_mut() { std::mem::swap(&mut ui.m, &mut **model); }
        }
    }

    /// One action: `line`, done on the set-up `before`; `ui` has the set-up after it.
    fn rec_action_written(&mut self, ui: &mut Ui, cv: &Canvas, line: &str, before: &mut Model) {
        let Some(rec) = self.rec.as_ref() else { return };
        // what was set before the action and is not written yet is written first, as the set-up was then
        if !rec.hold && (rec.pending || crate::store::setup_text(before) != rec.seen) {
            if line == "*RST" {
                // a reset takes it with everything else, but for what Default leaves alone (the Quick key action,
                // the text size): only that is written, as a step from the set-up of the last step. The view of
                // the screen is the one after the reset by now and is not looked at
                let mut kept = rec.base.clone();
                Ui::default_keeps(before, &mut kept);
                if crate::store::setup_text(&kept) != crate::store::setup_text(&rec.base) {
                    std::mem::swap(&mut ui.m, &mut kept);
                    self.rec_step_to_model(ui, cv);
                    std::mem::swap(&mut ui.m, &mut kept);
                }
            } else {
                std::mem::swap(&mut ui.m, before);
                self.rec_step(ui, cv);
                std::mem::swap(&mut ui.m, before);
            }
        }
        let want = crate::store::setup_text(&ui.m);
        // a command that only sets the state of the scope is proven like a step, from the model before it: what
        // it leaves different on a second scope gets a line after it. A save would write its file a second time
        // there, and an auto set-up finds what the signal of that scope has: these are written as they are.
        let state = matches!(line, ":RUN" | ":STOP" | ":SINGle" | ":TFORce" | ":CLEar" | "*RST");
        let (mut lines, rest) = match state {
            true => self.mend(ui, cv, before, vec![line.to_string()], &want),
            false => (vec![line.to_string()], Vec::new()),
        };
        if matches!(line, ":SINGle" | ":AUToscale") { lines.insert(1, "*OPC?".to_string()); }
        let Some(rec) = self.rec.as_mut() else { return };
        rec.lines.push(comment(&format!("+{:.1} s", (ui.now - rec.started).max(0.0))));
        rec.commands += lines.len();
        rec.lines.extend(lines);
        if let Some(keys) = named(&rest) { rec.lines.push(comment(&format!("not recorded: {keys}"))); }
        // the settings the action changed are its own: the next step starts from the set-up it left
        let auto = line == ":AUToscale";
        if state || auto { (rec.base, rec.seen, rec.pending) = (ui.m.clone(), want, false); }
        rec.hold = rec.hold || auto;
        // no message: the action has said what it did, or why not ("Save failed")
        let _ = crate::store::write(&rec.path, rec.text().as_bytes());
        // a key that is held or pressed again and again fills a recording as a knob does
        if rec.full() { ui.scpi_rec_want = false; }
    }

    fn rec_start(&mut self, ui: &mut Ui, cv: &Canvas) {
        let (setup, rest) = self.setup_scpi_proven(ui, cv);
        let version = env!("CARGO_PKG_VERSION");
        let mut lines = vec![comment(&format!("OOS SCPI recording v1 (oos-ui {version})"))];
        lines.push(comment("the set-up at the start"));
        lines.extend(setup.lines().map(str::to_string));
        if let Some(keys) = named(&rest) { lines.push(comment(&format!("not carried: {keys}"))); }
        lines.push(comment("what was set at the scope from then on"));
        let rec = Recorder {
            base: ui.m.clone(),
            seen: crate::store::setup_text(&ui.m),
            changed_at: ui.now,
            pending: false,
            hold: false,
            started: ui.now,
            path: ui.scpi_new_file("recording"),
            lines,
            commands: 0,
        };
        // an action from before the recorder was on is not one of the recording
        ui.scpi_rec_actions.clear();
        // the file is there from the start; a folder that cannot be written is said when the recorder is switched off
        let _ = crate::store::write(&rec.path, rec.text().as_bytes());
        self.rec = Some(Box::new(rec));
        ui.toast("SCPI recorder on");
    }

    /// The commands from the set-up of the last step to the one the scope has now, proven, and written.
    fn rec_step(&mut self, ui: &mut Ui, cv: &Canvas) {
        ui.sync_view_state();
        self.rec_step_to_model(ui, cv);
    }

    /// The step to the model `ui` holds, as it is: what the screen keeps of the view is not written into it first
    /// (`Ui::sync_view_state`), so the model may be another than the scope's own (`rec_action_written`).
    fn rec_step_to_model(&mut self, ui: &mut Ui, cv: &Canvas) {
        let Some(mut rec) = self.rec.take() else { return };
        let want = crate::store::setup_text(&ui.m);
        let (step, rest) = self.mend(ui, cv, &rec.base, Vec::new(), &want);
        let wrote = !step.is_empty() || !rest.is_empty();
        if !step.is_empty() {
            rec.lines.push(comment(&format!("+{:.1} s", (rec.changed_at - rec.started).max(0.0))));
            ui.toast(&format!("Recorded {}", step[step.len() - 1]));
            rec.commands += step.len();
            rec.lines.extend(step);
        }
        if let Some(keys) = named(&rest) { rec.lines.push(comment(&format!("not recorded: {keys}"))); }
        (rec.base, rec.seen, rec.pending) = (ui.m.clone(), want, false);
        if wrote { let _ = crate::store::write(&rec.path, rec.text().as_bytes()); }
        if rec.full() { ui.scpi_rec_want = false; }
        self.rec = Some(rec);
    }

    fn rec_stop(&mut self, ui: &mut Ui, cv: &Canvas) {
        // what was set last is written without the wait; what an auto set-up that still runs has set is its own
        if !self.rec.as_ref().is_some_and(|r| r.hold) { self.rec_step(ui, cv); }
        let Some(rec) = self.rec.take() else { return };
        let n = rec.commands;
        ui.scpi_write(&rec.path, &rec.text(), Some(&format!("{n} command{}", if n == 1 { "" } else { "s" })));
        self.rec_last = rec.lines;
    }

    /// The model before a client's message is run, while the recorder is on (`poll_program`).
    pub(crate) fn rec_before_remote(&self, ui: &Ui) -> Option<Model> {
        self.rec.as_ref().map(|_| ui.m.clone())
    }

    /// What a client's message set is not the user's. What the user had set and was not written yet is written
    /// first, as the set-up was before the message; then the set-up after the message is the one the next step starts
    /// from.
    pub(crate) fn rec_after_remote(&mut self, ui: &mut Ui, cv: &Canvas, mut before: Model) {
        if ui.m == before { return; }
        let Some(rec) = self.rec.as_ref() else { return };
        if rec.pending || crate::store::setup_text(&before) != rec.seen {
            // the queries of the step read the model: for the step it is the one from before the message
            std::mem::swap(&mut ui.m, &mut before);
            self.rec_step(ui, cv);
            std::mem::swap(&mut ui.m, &mut before);
        }
        let Some(rec) = self.rec.as_mut() else { return };
        ui.sync_view_state();
        (rec.base, rec.seen, rec.pending) = (ui.m.clone(), crate::store::setup_text(&ui.m), false);
        // an auto set-up the client started goes on for frames: what it changes is the client's too
        rec.hold = rec.hold || ui.auto_running();
        // one comment for a run of messages of a client: the file does not bring what it set
        if !rec.lines.last().is_some_and(|l| *l == comment(BY_CLIENT)) {
            rec.lines.push(comment(BY_CLIENT));
            let _ = crate::store::write(&rec.path, rec.text().as_bytes());
        }
    }

    /// `:SYSTem:RECorder:DATA?`: the lines of the recording that runs, else of the last one.
    pub(crate) fn rec_lines(&self) -> &[String] {
        self.rec.as_ref().map_or(&self.rec_last, |r| &r.lines)
    }
}

#[cfg(test)]
mod tests {
    use super::super::setupscpi::changed;
    use super::*;
    use crate::wave::Source;

    fn session() -> (Scpi, Ui, Canvas) {
        (Scpi::open(0), Ui::new(Source::Demo { t: 0.0 }), Canvas::new(crate::ui::W, crate::ui::H))
    }

    /// A folder of the test's own under the temporary directory. The file of a recording is written when the
    /// recorder is switched on, so every test that starts one saves into such a folder. It is removed when the test
    /// ends, also a test that fails. The name of the test and the process are in the name: two tests never share
    /// one, nor two runs at once.
    struct Folder(std::path::PathBuf);

    impl Folder {
        fn new(name: &str) -> Folder {
            let dir = std::env::temp_dir().join(format!("oos-recorder-{name}-{}", std::process::id()));
            // what a run with the same number left when it was killed
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Folder(dir)
        }
    }

    impl Drop for Folder {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    impl AsRef<std::path::Path> for Folder {
        fn as_ref(&self) -> &std::path::Path {
            &self.0
        }
    }

    /// A scope that saves into a folder of the test's own; the folder is there as long as the last value is kept.
    fn session_in(name: &str) -> (Scpi, Ui, Canvas, Folder) {
        let (s, mut ui, cv) = session();
        let dir = Folder::new(name);
        ui.store_dir = dir.0.clone();
        (s, ui, cv, dir)
    }

    fn text(ui: &mut Ui) -> String {
        ui.sync_view_state();
        crate::store::setup_text(&ui.m)
    }

    /// The scope's time goes on by `seconds`, then a frame of the interpreter.
    fn tick(s: &mut Scpi, ui: &mut Ui, cv: &Canvas, seconds: f64) {
        ui.now += seconds;
        s.poll(ui, cv);
    }

    /// A frame that sees what was set, then one after it has stood still.
    fn settle(s: &mut Scpi, ui: &mut Ui, cv: &Canvas) {
        tick(s, ui, cv, 0.05);
        tick(s, ui, cv, 1.0);
    }

    /// What the user sets: the model, as a knob or a drag changes it.
    fn user(ui: &mut Ui, key: &str, value: &str) {
        crate::store::set(&mut ui.m, key, value).unwrap();
        ui.mark_changed();
    }

    /// The lines of a recording after the start (the commands, without the comments).
    fn recorded(lines: &[String]) -> Vec<String> {
        let at = lines.iter().position(|l| l == "# what was set at the scope from then on").expect("the mark of the start");
        lines[at + 1..].iter().filter(|l| !l.starts_with('#')).cloned().collect()
    }

    /// Sends the lines to a second scope, comments and all: its set-up text, and the errors it queued.
    fn replay(lines: &[String]) -> (String, Vec<String>) {
        let (mut s, mut ui, cv) = session();
        for l in lines { s.exec_line(&mut ui, &cv, l); }
        (text(&mut ui), s.queued_errors().to_vec())
    }

    fn start(s: &mut Scpi, ui: &mut Ui, cv: &Canvas) {
        // no test leaves a file where Storage saves by default
        assert!(ui.save_dir().starts_with(std::env::temp_dir()), "the test has its own folder (`session_in`)");
        ui.scpi_rec_want = true;
        s.poll(ui, cv);
        assert!(s.rec.is_some());
    }

    #[test]
    fn what_is_set_at_the_scope_is_recorded_and_a_turned_knob_is_one_line() {
        let (mut s, mut ui, cv, _dir) = session_in("knob");
        // before the recorder is on: part of the set-up at the start
        s.exec_line(&mut ui, &cv, ":CHANnel2:DISPlay ON");
        start(&mut s, &mut ui, &cv);
        assert!(s.rec_lines().contains(&":CHANnel2:DISPlay 1".to_string()) && recorded(s.rec_lines()).is_empty());
        // a knob turned through three steps, a frame each
        for t in ["0.0005", "0.001", "0.002"] {
            user(&mut ui, "tdiv", t);
            tick(&mut s, &mut ui, &cv, 0.1);
        }
        assert!(recorded(s.rec_lines()).is_empty(), "still turning: {:?}", s.rec_lines());
        tick(&mut s, &mut ui, &cv, 0.3);
        assert!(recorded(s.rec_lines()).is_empty(), "not still for long enough");
        tick(&mut s, &mut ui, &cv, 0.4);
        assert_eq!(recorded(s.rec_lines()), [":TIMebase:SCALe 2.000000E-03"]);
        assert_eq!(ui.toast_text(), "Recorded :TIMebase:SCALe 2.000000E-03");
        assert!(Ui::toast_is_info(&ui.toast_text()) && Ui::toast_is_info("SCPI recorder on"), "a neutral message, not a warning");
        // another setting
        user(&mut ui, "ch1.coupling", "Ac");
        settle(&mut s, &mut ui, &cv);
        s.exec_line(&mut ui, &cv, "*CLS");
        let want = text(&mut ui);
        let lines = s.rec_lines().to_vec();
        assert_eq!(recorded(&lines).len(), 2, "{lines:?}");
        assert!(lines.iter().any(|l| l == "# +0.3 s"), "the time of the last turn of the knob: {lines:?}");
        // sent to a scope in another state, the whole file brings this set-up, and the comments are no errors
        let (got, errors) = replay(&lines);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(changed(&want, &got), Vec::new(), "{lines:?}");
    }

    #[test]
    fn what_a_client_sets_is_not_recorded() {
        let (mut s, mut ui, cv, _dir) = session_in("client");
        start(&mut s, &mut ui, &cv);
        // a client alone
        s.exec_line(&mut ui, &cv, ":CHANnel3:DISPlay ON");
        tick(&mut s, &mut ui, &cv, 1.0);
        assert!(recorded(s.rec_lines()).is_empty(), "{:?}", s.rec_lines());
        // the user, then a client in the same frame, before the look
        user(&mut ui, "tdiv", "0.001");
        s.exec_line(&mut ui, &cv, ":CHANnel4:DISPlay ON");
        s.exec_line(&mut ui, &cv, ":CHANnel1:SCALe?");
        tick(&mut s, &mut ui, &cv, 1.0);
        assert_eq!(recorded(s.rec_lines()), [":TIMebase:SCALe 1.000000E-03"]);
        // a reset by a client: the set-up of the last step is the new one, and the user's next setting is one line
        s.exec_line(&mut ui, &cv, "*RST");
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()).len(), 1, "the reset is the client's: {:?}", s.rec_lines());
        user(&mut ui, "ch1.coupling", "Ac");
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()).len(), 2, "{:?}", s.rec_lines());
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        // the file says where a client set something, once for a run of its messages
        assert_eq!(s.rec_lines().iter().filter(|l| **l == comment(BY_CLIENT)).count(), 2, "{:?}", s.rec_lines());
    }

    /// A client's message while the user is in the middle of a turn of a knob: what the user has set by then is
    /// written, with the user's value, and the client's setting is not.
    #[test]
    fn a_client_in_the_middle_of_a_turn_does_not_take_the_users_setting() {
        let (mut s, mut ui, cv, _dir) = session_in("turn");
        s.exec_line(&mut ui, &cv, ":CHANnel1:COUPling AC");
        start(&mut s, &mut ui, &cv);
        user(&mut ui, "tdiv", "0.001");
        tick(&mut s, &mut ui, &cv, 0.1);
        assert!(recorded(s.rec_lines()).is_empty(), "still turning");
        // back to a default, a reset, a query: whatever the client sends
        s.exec_line(&mut ui, &cv, ":CHANnel1:COUPling DC");
        assert_eq!(recorded(s.rec_lines()), [":TIMebase:SCALe 1.000000E-03"], "{:?}", s.rec_lines());
        user(&mut ui, "tdiv", "0.002");
        tick(&mut s, &mut ui, &cv, 0.1);
        s.exec_line(&mut ui, &cv, "*RST");
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()), [":TIMebase:SCALe 1.000000E-03", ":TIMebase:SCALe 2.000000E-03"], "{:?}", s.rec_lines());
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
    }

    #[test]
    fn the_commands_switch_it_and_give_the_lines() {
        let (mut s, mut ui, cv, dir) = session_in("commands");
        let ask = |s: &mut Scpi, ui: &mut Ui, c: &str| String::from_utf8(s.exec_line(ui, &cv, c)).unwrap();
        assert_eq!(ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe?"), "0");
        assert_eq!(ask(&mut s, &mut ui, ":SYSTem:RECorder:DATA?"), "#9000000000");
        ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe ON");
        assert_eq!(ask(&mut s, &mut ui, ":SYST:REC:STAT?"), "1");
        tick(&mut s, &mut ui, &cv, 0.1);
        assert!(s.rec.is_some());
        // the file is there while the recorder is on, with what was written so far: the end of the program loses nothing
        let so_far: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
        assert_eq!(so_far.len(), 1);
        assert!(std::fs::read_to_string(&so_far[0]).unwrap().ends_with("# what was set at the scope from then on\n"));
        user(&mut ui, "ch1.coupling", "Ac");
        tick(&mut s, &mut ui, &cv, 0.05);
        tick(&mut s, &mut ui, &cv, 1.0);
        assert!(std::fs::read_to_string(&so_far[0]).unwrap().ends_with(":CHANnel1:COUPling AC\n"), "written after the step");
        user(&mut ui, "tdiv", "0.001");
        tick(&mut s, &mut ui, &cv, 0.1);
        // switched off before the setting has stood still: it is written all the same
        ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe OFF");
        tick(&mut s, &mut ui, &cv, 0.1);
        assert!(s.rec.is_none());
        let files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
        assert!(files.len() == 1 && files[0].file_name().is_some_and(|n| n.to_string_lossy().starts_with("recording")), "{files:?}");
        let file = std::fs::read_to_string(&files[0]).unwrap();
        assert!(file.starts_with("# OOS SCPI recording v1") && file.ends_with(":TIMebase:SCALe 1.000000E-03\n"), "{file}");
        let message = ui.toast_text();
        assert!(message.starts_with("Saved recording") && message.ends_with("2 commands"), "{message}");
        // the lines of the last recording stay to be read
        let data = ask(&mut s, &mut ui, ":SYSTem:RECorder:DATA?");
        assert_eq!(data, format!("#9{:09}{file}", file.len()));
        ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe MAYBE");
        assert!(s.queued_errors().len() == 1 && s.queued_errors()[0].starts_with("-224"), "{:?}", s.queued_errors());
    }

    #[test]
    fn the_storage_row_switches_it() {
        let (mut s, mut ui, cv, dir) = session_in("row");
        ui.script_menu("Storage").unwrap();
        ui.script_do("SCPI recorder").unwrap();
        s.poll(&mut ui, &cv);
        assert!(s.rec.is_some() && ui.toast_text() == "SCPI recorder on");
        ui.script_do("SCPI recorder").unwrap();
        s.poll(&mut ui, &cv);
        assert!(s.rec.is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(ui.toast_text().ends_with("0 commands"), "{}", ui.toast_text());
    }

    /// The keys that are actions are their commands, in the order they were pressed, with what was set before them
    /// first; the mode a single sweep sets is its own and no line. The whole file brings the set-up.
    #[test]
    fn a_key_that_is_an_action_is_its_command() {
        use crate::ui::key;
        let (mut s, mut ui, cv, _dir) = session_in("key");
        start(&mut s, &mut ui, &cv);
        user(&mut ui, "tdiv", "0.001");
        tick(&mut s, &mut ui, &cv, 0.1);
        // before the turn of the knob has stood still
        ui.key(key::SINGLE);
        tick(&mut s, &mut ui, &cv, 0.1);
        assert_eq!(recorded(s.rec_lines()), [":TIMebase:SCALe 1.000000E-03", ":SINGle", "*OPC?"], "{:?}", s.rec_lines());
        for k in [key::RUNSTOP, key::RUNSTOP, key::CLEAR, key::FORCE] {
            ui.key(k);
            settle(&mut s, &mut ui, &cv);
        }
        // the mode a single sweep sets and Run takes back are the commands' own: no line of a setting
        let got = recorded(s.rec_lines());
        assert_eq!(got, [":TIMebase:SCALe 1.000000E-03", ":SINGle", "*OPC?", ":STOP", ":RUN", ":CLEar", ":TFORce"]);
        let want = text(&mut ui);
        let lines: Vec<String> = s.rec_lines().iter().filter(|l| *l != "*OPC?").cloned().collect();
        let (replayed, errors) = replay(&lines);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(changed(&want, &replayed), Vec::new(), "{lines:?}");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
    }

    /// Default is the reset alone, whatever was set before; an auto set-up is its command, and what it found is not
    /// written as settings.
    #[test]
    fn default_and_autoset_are_one_command_each() {
        use crate::ui::key;
        let (mut s, mut ui, cv, _dir) = session_in("default");
        // the demo signal has been seen: an auto set-up has something to find
        for _ in 0..20 { ui.step(0.05); }
        start(&mut s, &mut ui, &cv);
        user(&mut ui, "tdiv", "0.001");
        user(&mut ui, "ch1.vdiv", "0.5");
        tick(&mut s, &mut ui, &cv, 0.1);
        ui.key(key::DEFAULT);
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()), ["*RST"], "{:?}", s.rec_lines());
        // far from what the signal needs, and written; then the auto set-up changes it back
        user(&mut ui, "tdiv", "0.5");
        settle(&mut s, &mut ui, &cv);
        let before = text(&mut ui);
        ui.key(key::AUTO);
        settle(&mut s, &mut ui, &cv);
        assert!(!changed(&before, &text(&mut ui)).is_empty(), "the auto set-up of the demo signal changes settings");
        assert_eq!(recorded(s.rec_lines()), ["*RST", ":TIMebase:SCALe 5.000000E-01", ":AUToscale", "*OPC?"], "{:?}", s.rec_lines());
        // what the user sets after it is a step from the set-up it left
        user(&mut ui, "ch1.coupling", "Ac");
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()).len(), 5, "{:?}", s.rec_lines());
    }

    /// A save of the Storage menu is its command and is not run a second time; what a client does with the same
    /// commands is not the user's action. A set-up is saved with its slot (`*SAV <n>`): the slot is not part of the
    /// set-up, so a scope whose menu stands at another one would write that one.
    #[test]
    fn a_save_is_its_command_and_a_clients_action_is_not_recorded() {
        let (mut s, mut ui, cv, dir) = session_in("save");
        start(&mut s, &mut ui, &cv);
        for c in [":SINGle", ":STOP", ":RUN", ":CLEar", ":TFORce", ":AUToscale", ":SAVE:SETup", "*RST"] {
            s.exec_line(&mut ui, &cv, c);
            settle(&mut s, &mut ui, &cv);
        }
        assert!(recorded(s.rec_lines()).is_empty(), "a client's: {:?}", s.rec_lines());
        s.exec_line(&mut ui, &cv, "*CLS");
        // the set-up files of a folder, by name
        let saved = |dir: &std::path::Path| {
            let names = std::fs::read_dir(dir).unwrap().filter_map(|e| e.ok()).map(|e| e.file_name());
            let names = names.map(|n| n.to_string_lossy().into_owned()).filter(|n| n.starts_with("setup"));
            let mut names: Vec<String> = names.collect();
            names.sort();
            names
        };
        assert_eq!(saved(dir.as_ref()), ["setup1.txt"], "the client's save, to the slot the menu starts with");
        // the slot of the Storage menu (`:SAVE:SLOT` is its row) is no setting of the set-up: no line for it
        s.exec_line(&mut ui, &cv, ":SAVE:SLOT 3");
        ui.script_menu("Storage").unwrap();
        // no waveform yet (the scope has not run a frame): the row says so and nothing is recorded
        ui.script_do("Save waveform (CSV)").unwrap();
        settle(&mut s, &mut ui, &cv);
        assert!(recorded(s.rec_lines()).is_empty() && ui.toast_text() == "No waveform to save", "{:?}", s.rec_lines());
        ui.script_do("Save setup").unwrap();
        assert_eq!(saved(dir.as_ref()), ["setup1.txt", "setup3.txt"], "the file of the slot");
        // the row has written it; the recorder, a frame later, must not run the save again
        for name in saved(dir.as_ref()) { std::fs::remove_file(dir.0.join(name)).unwrap(); }
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()), ["*SAV 3"], "{:?}", s.rec_lines());
        assert!(saved(dir.as_ref()).is_empty(), "written once, by the row");
        assert!(ui.toast_text().starts_with("Saved setup"), "the row's message stays: {}", ui.toast_text());
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        // run on a scope whose menu stands at another slot, the recording writes the slot it was made with
        let lines = s.rec_lines().to_vec();
        let (mut s, mut ui, cv, dir) = session_in("save-replay");
        for l in &lines { s.exec_line(&mut ui, &cv, l); }
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        assert_eq!(saved(dir.as_ref()), ["setup3.txt"], "{lines:?}");
    }

    /// The action is recorded where it is done: the Quick key set to Clear is a Clear. Two actions in one frame are
    /// two commands, each with its own settings: the auto set-up's are not written after the reset. An action in the
    /// frame the recorder is switched off is in the file.
    #[test]
    fn an_action_is_recorded_where_it_is_done() {
        use crate::ui::key;
        let (mut s, mut ui, cv, _dir) = session_in("done");
        for _ in 0..20 { ui.step(0.05); }
        // what the Quick key does is a setting: set before the recorder is on, it is no line
        ui.m.quick = Quick::Clear;
        start(&mut s, &mut ui, &cv);
        ui.key(key::QUICK);
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()), [":CLEar"], "{:?}", s.rec_lines());
        user(&mut ui, "tdiv", "0.5");
        settle(&mut s, &mut ui, &cv);
        ui.key(key::DEFAULT);
        ui.key(key::AUTO);
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()), [":CLEar", ":TIMebase:SCALe 5.000000E-01", "*RST", ":AUToscale", "*OPC?"], "{:?}", s.rec_lines());
        ui.key(key::SINGLE);
        ui.scpi_rec_want = false;
        s.poll(&mut ui, &cv);
        assert!(s.rec.is_none());
        assert_eq!(recorded(s.rec_lines()).last().map(String::as_str), Some("*OPC?"), "{:?}", s.rec_lines());
        assert!(recorded(s.rec_lines()).contains(&":SINGle".to_string()));
    }

    /// A recording ends by itself at `LINES_MAX` lines, whether the line that fills it is a setting or an action:
    /// the recorder goes off, and the file is saved as when it is switched off.
    #[test]
    fn a_recording_ends_by_itself_when_it_is_full() {
        use crate::ui::key;
        for action in [false, true] {
            let (mut s, mut ui, cv, dir) = session_in(if action { "full-action" } else { "full-setting" });
            start(&mut s, &mut ui, &cv);
            // a long recording: all the lines but the two of one more step (its time and its command) are there
            let rec = s.rec.as_mut().unwrap();
            let fill = LINES_MAX - 2 - rec.lines.len();
            rec.lines.extend(vec![comment("a line of a long recording"); fill]);
            match action {
                true => ui.key(key::CLEAR),
                false => user(&mut ui, "tdiv", "0.001"),
            }
            settle(&mut s, &mut ui, &cv);
            assert!(!ui.scpi_rec_want, "full: the switch of the menu is off (action: {action})");
            s.poll(&mut ui, &cv);
            assert!(s.rec.is_none(), "action: {action}");
            assert_eq!(s.rec_lines().len(), LINES_MAX, "action: {action}");
            let want = if action { ":CLEar" } else { ":TIMebase:SCALe 1.000000E-03" };
            assert_eq!(s.rec_lines().last().map(String::as_str), Some(want));
            let files: Vec<_> = std::fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).map(|e| e.path()).collect();
            assert_eq!(files.len(), 1, "{files:?}");
            assert_eq!(std::fs::read_to_string(&files[0]).unwrap().lines().count(), LINES_MAX);
            let message = ui.toast_text();
            assert!(message.starts_with("Saved recording") && message.ends_with("1 command"), "{message}");
        }
    }

    /// Default leaves the text size and the Quick key action alone, so the reset does not take them with it as it
    /// takes the other settings: one that was set just before the key, whether the recorder has seen it or not, is
    /// written before the reset.
    #[test]
    fn what_default_leaves_alone_is_written_before_the_reset() {
        use crate::ui::key;
        let (mut s, mut ui, cv, _dir) = session_in("kept");
        start(&mut s, &mut ui, &cv);
        // seen by the recorder but not still for long enough, and with it a setting the reset takes
        user(&mut ui, "text_size", "Larger");
        user(&mut ui, "tdiv", "0.001");
        tick(&mut s, &mut ui, &cv, 0.1);
        ui.key(key::DEFAULT);
        settle(&mut s, &mut ui, &cv);
        assert_eq!(recorded(s.rec_lines()), [":SYSTem:SET text_size = Larger", "*RST"], "{:?}", s.rec_lines());
        // set in the frame of the key: the recorder has not seen it
        user(&mut ui, "quick", "Clear");
        ui.key(key::DEFAULT);
        settle(&mut s, &mut ui, &cv);
        let lines = s.rec_lines().to_vec();
        assert_eq!(recorded(&lines)[2..], [":SYSTem:SET quick = Clear", "*RST"], "{lines:?}");
        assert!(ui.m.text_size == TextSize::Larger && ui.m.quick == Quick::Clear, "Default has left them");
        // nothing was set: Default is the reset alone, as before
        ui.key(key::DEFAULT);
        settle(&mut s, &mut ui, &cv);
        let lines = s.rec_lines().to_vec();
        assert_eq!(recorded(&lines)[4..], ["*RST"], "{lines:?}");
        // the whole file brings a second scope to this set-up, the text size and the Quick key with it
        let want = text(&mut ui);
        let (got, errors) = replay(&lines);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(changed(&want, &got), Vec::new(), "{lines:?}");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
    }

    #[test]
    fn a_comment_line_is_no_command() {
        let (mut s, mut ui, cv) = session();
        assert!(s.exec_line(&mut ui, &cv, "# a comment").is_empty());
        assert!(s.exec_line(&mut ui, &cv, "# +1.5 s").is_empty());
        assert_eq!(String::from_utf8(s.exec_line(&mut ui, &cv, "# one\n:CHANnel2:DISPlay ON\n# two\n:CHANnel2:DISPlay?")).unwrap(), "1");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        // a comment someone wrote by hand may hold a `;`: nothing after it is run
        assert!(s.exec_line(&mut ui, &cv, "# off again; :CHANnel2:DISPlay OFF").is_empty());
        assert_eq!(String::from_utf8(s.exec_line(&mut ui, &cv, "  # indented\n:CHANnel2:DISPlay?")).unwrap(), "1");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        assert_eq!(comment("a;b\nc"), "# a,b,c");
    }
}
