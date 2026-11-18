// SPDX-License-Identifier: GPL-2.0-or-later
//! The SCPI recorder: while it is on, what is set at the scope (touch, keys, knobs, menus) is written as SCPI
//! commands, and the lines are saved as a file when it is switched off (Storage > SCPI recorder,
//! `:SYSTem:RECorder:STATe`).
//!
//! It does not follow the menu or the knobs. Once a frame the text of the set-up (`store::setup_text`) is looked at:
//! when it has changed and then stood still for `SETTLE`, the commands that take a scope from the set-up of the last
//! step to this one are made and proven on a second scope, as for `:SYSTem:SETup:SCPI?` (`Scpi::mend`,
//! scpi/setupscpi.rs). So a knob turned through twenty steps is one line with the value it was left at, and a
//! setting changed by a drag is seen like one changed in a menu. The file starts with the set-up at the start as
//! commands, so it can be sent to a scope in any state.
//!
//! What a client sets by SCPI while the recorder is on is not the user's: it goes into the set-up of the last step
//! and is not written. Actions that leave no setting (Run, Single, Clear, a saved file) are not recorded yet.

use super::setupscpi::{changed, named};
use super::*;

/// A setting is written when it has stood still this long, in seconds: a knob that is being turned is one line.
const SETTLE: f64 = 0.6;

pub(crate) struct Recorder {
    /// the set-up the lines written so far stand for: the model at the last step
    base: Model,
    /// the set-up text at the last look
    seen: String,
    /// when it last changed (`Ui::now`), and whether that change waits to stand still
    changed_at: f64,
    pending: bool,
    started: f64,
    /// the file so far: comments (`#`) and commands
    pub(crate) lines: Vec<String>,
    /// the commands recorded after the start
    pub(crate) commands: usize,
}

/// A comment line of the file. The interpreter splits a message at `;` before it sees that a line is a comment.
fn comment(text: &str) -> String {
    format!("# {}", text.replace([';', '\n', '\r'], ","))
}

impl Scpi {
    /// Once a frame, before the clients are read (`poll`): the switch, the look at the set-up, the step that is due.
    pub(crate) fn rec_tick(&mut self, ui: &mut Ui, cv: &Canvas) {
        match (ui.scpi_rec_want, self.rec.is_some()) {
            (true, false) => self.rec_start(ui, cv),
            (false, true) => self.rec_stop(ui, cv),
            _ => {}
        }
        let Some(rec) = self.rec.as_mut() else { return };
        ui.sync_view_state();
        let now = crate::store::setup_text(&ui.m);
        if now != rec.seen {
            (rec.seen, rec.changed_at, rec.pending) = (now, ui.now, true);
        }
        if rec.pending && ui.now - rec.changed_at >= SETTLE { self.rec_step(ui, cv); }
    }

    fn rec_start(&mut self, ui: &mut Ui, cv: &Canvas) {
        let (setup, rest) = self.setup_scpi_proven(ui, cv);
        let mut lines = vec![comment(&format!("OOS SCPI recording v1 (oos-ui {})", env!("CARGO_PKG_VERSION"))), comment("the set-up at the start")];
        lines.extend(setup.lines().map(str::to_string));
        if let Some(keys) = named(&rest) { lines.push(comment(&format!("not carried: {keys}"))); }
        lines.push(comment("what was set at the scope from then on"));
        let seen = crate::store::setup_text(&ui.m);
        self.rec = Some(Box::new(Recorder { base: ui.m.clone(), seen, changed_at: ui.now, pending: false, started: ui.now, lines, commands: 0 }));
        ui.toast("SCPI recorder on");
    }

    /// The commands from the set-up of the last step to the one the scope has now, proven, and written.
    fn rec_step(&mut self, ui: &mut Ui, cv: &Canvas) {
        let Some(mut rec) = self.rec.take() else { return };
        ui.sync_view_state();
        let want = crate::store::setup_text(&ui.m);
        let (step, rest) = self.mend(ui, cv, &rec.base, Vec::new(), &want);
        if !step.is_empty() {
            rec.lines.push(comment(&format!("+{:.1} s", (rec.changed_at - rec.started).max(0.0))));
            ui.toast(&format!("Recorded {}", step[step.len() - 1]));
            rec.commands += step.len();
            rec.lines.extend(step);
        }
        if let Some(keys) = named(&rest) { rec.lines.push(comment(&format!("not recorded: {keys}"))); }
        (rec.base, rec.seen, rec.pending) = (ui.m.clone(), want, false);
        self.rec = Some(rec);
    }

    fn rec_stop(&mut self, ui: &mut Ui, cv: &Canvas) {
        // what was set last is written without the wait
        self.rec_step(ui, cv);
        let Some(rec) = self.rec.take() else { return };
        let n = rec.commands;
        ui.scpi_save("recording", &(rec.lines.join("\n") + "\n"), Some(&format!("{n} command{}", if n == 1 { "" } else { "s" })));
        self.rec_last = rec.lines;
    }

    /// The set-up text before a client's message is run, while the recorder is on (`poll_program`).
    pub(crate) fn rec_before_remote(&self, ui: &Ui) -> Option<String> {
        self.rec.as_ref().map(|_| crate::store::setup_text(&ui.m))
    }

    /// What a client's message set is not the user's: it goes into the set-up of the last step and is not written.
    pub(crate) fn rec_after_remote(&mut self, ui: &Ui, before: &str) {
        let Some(rec) = self.rec.as_mut() else { return };
        let after = crate::store::setup_text(&ui.m);
        if after == before { return; }
        // a change the user made in this frame that the look has not seen yet still waits to be written
        if before != rec.seen { (rec.changed_at, rec.pending) = (ui.now, true); }
        for (key, value) in changed(before, &after) {
            let set = value.is_some_and(|v| crate::store::set(&mut rec.base, &key, &v).is_ok());
            if !set {
                // a key that is gone is back at a default the text does not write (a reset, a recalled set-up): the
                // last step is the whole set-up as it is now, and what waited to be written is given up
                (rec.base, rec.pending) = (ui.m.clone(), false);
                break;
            }
        }
        rec.seen = after;
    }

    /// `:SYSTem:RECorder:DATA?`: the lines of the recording that runs, else of the last one.
    pub(crate) fn rec_lines(&self) -> &[String] {
        self.rec.as_ref().map_or(&self.rec_last, |r| &r.lines)
    }
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
        ui.scpi_rec_want = true;
        s.poll(ui, cv);
        assert!(s.rec.is_some());
    }

    #[test]
    fn what_is_set_at_the_scope_is_recorded_and_a_turned_knob_is_one_line() {
        let (mut s, mut ui, cv) = session();
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
        let (mut s, mut ui, cv) = session();
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
    }

    #[test]
    fn the_commands_switch_it_and_give_the_lines() {
        let (mut s, mut ui, cv) = session();
        let dir = std::env::temp_dir().join(format!("oos-recorder-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        ui.store_dir = dir.clone();
        let ask = |s: &mut Scpi, ui: &mut Ui, c: &str| String::from_utf8(s.exec_line(ui, &cv, c)).unwrap();
        assert_eq!(ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe?"), "0");
        assert_eq!(ask(&mut s, &mut ui, ":SYSTem:RECorder:DATA?"), "#9000000000");
        ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe ON");
        assert_eq!(ask(&mut s, &mut ui, ":SYST:REC:STAT?"), "1");
        tick(&mut s, &mut ui, &cv, 0.1);
        assert!(s.rec.is_some());
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
        assert!(message.starts_with("Saved recording") && message.ends_with("1 command"), "{message}");
        // the lines of the last recording stay to be read
        let data = ask(&mut s, &mut ui, ":SYSTem:RECorder:DATA?");
        assert_eq!(data, format!("#9{:09}{file}", file.len()));
        ask(&mut s, &mut ui, ":SYSTem:RECorder:STATe MAYBE");
        assert!(s.queued_errors().len() == 1 && s.queued_errors()[0].starts_with("-224"), "{:?}", s.queued_errors());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_storage_row_switches_it() {
        let (mut s, mut ui, cv) = session();
        let dir = std::env::temp_dir().join(format!("oos-recorder-row-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        ui.store_dir = dir.clone();
        ui.script_menu("Storage").unwrap();
        ui.script_do("SCPI recorder").unwrap();
        s.poll(&mut ui, &cv);
        assert!(s.rec.is_some() && ui.toast_text() == "SCPI recorder on");
        ui.script_do("SCPI recorder").unwrap();
        s.poll(&mut ui, &cv);
        assert!(s.rec.is_none());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        assert!(ui.toast_text().ends_with("0 commands"), "{}", ui.toast_text());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_comment_line_is_no_command() {
        let (mut s, mut ui, cv) = session();
        assert!(s.exec_line(&mut ui, &cv, "# a comment").is_empty());
        assert!(s.exec_line(&mut ui, &cv, "# +1.5 s").is_empty());
        assert_eq!(String::from_utf8(s.exec_line(&mut ui, &cv, "# one\n:CHANnel2:DISPlay ON\n# two\n:CHANnel2:DISPlay?")).unwrap(), "1");
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        assert_eq!(comment("a;b\nc"), "# a,b,c");
    }
}
