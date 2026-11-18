// SPDX-License-Identifier: GPL-2.0-or-later
//! The set-up as SCPI commands (`:SYSTem:SETup:SCPI?`), the first part of the SCPI recorder: what differs from the
//! Default set-up, one command a line, after a `*RST`. Sent to a scope, the lines bring it to the set-up they were
//! written from.
//!
//! A setting is found by comparing two texts of the set-up file (`store::setup_text`), not by following the menu or
//! the knobs: whatever changed the model, its key shows it. `ROWS` names the headers that carry a key. The value of a
//! line is the answer of the header's own query, so it is in the words the interpreter takes. A key without a row, or
//! whose query fails, is written as `:SYSTem:SET <key> = <value>`: the set-up schema holds every key. The tests send
//! the lines of every row to a second `Ui` and compare the two set-up texts.

use super::*;

/// A key of the set-up file (`<n>` is the number it carries) and the headers that hold what the key holds, `<n>`
/// likewise. The rows are in the order their commands are sent: what limits another setting comes before it (the
/// probe ratio before the scale, the scale before the offset, the trigger type before its values).
pub(crate) const ROWS: &[(&str, &[&str])] = &[
    // ---- channels
    ("ch<n>.on", &[":CHANnel<n>:DISPlay"]),
    ("ch<n>.probe", &[":CHANnel<n>:PROBe"]),
    ("ch<n>.unit", &[":CHANnel<n>:UNITs"]),
    ("ch<n>.coupling", &[":CHANnel<n>:COUPling"]),
    ("ch<n>.bw", &[":CHANnel<n>:BWLimit"]),
    ("ch<n>.invert", &[":CHANnel<n>:INVert"]),
    ("ch<n>.vdiv", &[":CHANnel<n>:SCALe"]),
    ("ch<n>.vfine", &[":CHANnel<n>:SCALe"]),
    ("ch<n>.offset", &[":CHANnel<n>:OFFSet"]),
    ("ch<n>.bias", &[":CHANnel<n>:BIAS"]),
    // ---- horizontal
    ("fine_t", &[":TIMebase:VERNier"]),
    ("tdiv", &[":TIMebase:SCALe"]),
    ("tfine", &[":TIMebase:SCALe"]),
    ("delay", &[":TIMebase:OFFSet"]),
    ("h_expand", &[":TIMebase:HREFerence:MODE"]),
    ("auto_roll", &[":TIMebase:ROLL"]),
    ("zoom", &[":TIMebase:DELay:ENABle"]),
    ("zoom_tdiv", &[":TIMebase:DELay:SCALe"]),
    ("zdelay", &[":TIMebase:DELay:OFFSet"]),
    // ---- acquire
    ("acq", &[":ACQuire:TYPE"]),
    ("avg_count", &[":ACQuire:AVERages"]),
    ("mdepth", &[":ACQuire:MDEPth"]),
    ("segments", &[":ACQuire:ULTRa:MAXFrame"]),
    ("cores", &[":ACQuire:CORes"]),
    ("avg_hw", &[":ACQuire:AVERages:HARDware"]),
    ("seg_hw", &[":ACQuire:SEGMents:HARDware"]),
    ("seg_interval", &[":ACQuire:SEGMents:INTerval"]),
    ("draw_path", &[":SYSTem:PIPeline"]),
    // ---- trigger
    ("trig_type", &[":TRIGger:MODE"]),
    ("trig_src", &[":TRIGger:EDGE:SOURce"]),
    ("trig_src2", &[":TRIGger:SOURce2"]),
    ("slope", &[":TRIGger:EDGE:SLOPe"]),
    ("trig_slope2", &[":TRIGger:SHOLd:SLOPe"]),
    ("trig_mode", &[":TRIGger:SWEep"]),
    ("holdoff", &[":TRIGger:HOLDoff"]),
    ("trig_nrej", &[":TRIGger:NREJect"]),
    ("trig_coup", &[":TRIGger:COUPling"]),
    ("trig_pol", &[":TRIGger:PULSe:POLarity"]),
    ("trig_when", &[":TRIGger:PULSe:WHEN"]),
    ("trig_t1", &[":TRIGger:PULSe:WIDTh"]),
    ("trig_t2", &[":TRIGger:PULSe:UWIDth"]),
    ("video_std", &[":TRIGger:VIDeo:STANdard"]),
    ("video_sync", &[":TRIGger:VIDeo:MODE"]),
    ("video_line", &[":TRIGger:VIDeo:LINE"]),
    ("pat", &[":TRIGger:PATTern:PATTern"]),
    ("win_mode", &[":TRIGger:WINDow:POSition"]),
    ("nth_idle", &[":TRIGger:NEDGe:IDLE"]),
    ("nth_n", &[":TRIGger:NEDGe:EDGE"]),
    ("setup", &[":TRIGger:SHOLd:STIMe"]),
    ("hold", &[":TRIGger:SHOLd:HTIMe"]),
    ("trig_bus", &[":TRIGger:BUS:WHEN"]),
    ("trig_bus_val", &[":TRIGger:BUS:VALue"]),
    ("trig_level", &[":TRIGger:EDGE:LEVel"]),
    ("trig_level2", &[":TRIGger:LEVel2"]),
    // ---- display
    ("layout", &[":DISPlay:LAYout"]),
    ("view", &[":VIEW"]),
    ("hdmi", &[":DISPlay:HDMI"]),
    ("hdmi_view", &[":DISPlay:VIEW:MONitor"]),
    ("persist", &[":DISPlay:GRADing:TIME"]),
    ("intensity", &[":DISPlay:WBRightness"]),
    ("grid", &[":DISPlay:GRID"]),
    ("grid_bright", &[":DISPlay:GBRightness"]),
    ("grid_frame", &[":DISPlay:RULers"]),
    ("color_grade", &[":DISPlay:COLor"]),
    ("dots", &[":DISPlay:TYPE"]),
    ("gen_hidden", &[":DISPlay:GENerator"]),
    ("hist", &[":HISTogram:TYPE"]),
    // ---- measure, cursors, DVM and counter, logger
    ("meas_labels", &[":MEASure:INDicator"]),
    ("meas_full_record", &[":MEASure:GATing"]),
    ("cursor", &[":CURSor:MODE"]),
    ("dvm", &[":DVM:ENABle", ":DVM:SOURce", ":DVM:MODE"]),
    ("counter", &[":COUNter:ENABle", ":COUNter:SOURce", ":COUNter:MODE", ":COUNter:NDIGits"]),
    ("log_ivl", &[":SYSTem:LOG:INTerval"]),
    // ---- decode and bus
    ("dec_sel", &[":DECode:SELect"]),
    ("dec_proto", &[":DECode:PROTocol"]),
    ("dec_on", &[":DECode:STATe"]),
    ("dec_src", &[":DECode:SOURce"]),
    ("dec_src2", &[":DECode:SOURce2"]),
    ("dec_cs", &[":DECode:CS"]),
    ("dec_src4", &[":DECode:SOURce4"]),
    ("dec_diff", &[":DECode:DIFFerential"]),
    ("dec_thr", &[":DECode:THReshold"]),
    ("dec_cphy_rate", &[":DECode:CPHY:RATE"]),
    ("bus_on", &[":BUS:LANE:STATe"]),
    ("bus_label", &[":BUS:LANE:LABel"]),
    ("bus_pos", &[":BUS:LANE:POSition"]),
    ("bus_clk", &[":BUS:LANE:CLOCk:SOURce"]),
    ("bus_data", &[":BUS:LANE:DATA:SOURce"]),
    ("bus_clk_thr", &[":BUS:LANE:CLOCk:THReshold"]),
    ("bus_data_thr", &[":BUS:LANE:DATA:THReshold"]),
    ("bus_rw", &[":BUS:LANE:RW"]),
    // ---- mask
    ("mask_src", &[":MASK:SOURce"]),
    ("mask_x", &[":MASK:X"]),
    ("mask_y", &[":MASK:Y"]),
    ("mask_stop", &[":MASK:OUTPut"]),
    ("mask_inv", &[":MASK:INVert"]),
    ("mask_on", &[":MASK:ENABle"]),
    // ---- the analyser pages
    ("spec", &[":SPECtrum:SOURce", ":SPECtrum:CENTer", ":SPECtrum:SPAN", ":SPECtrum:RBW", ":SPECtrum:WINDow", ":SPECtrum:TRACe", ":SPECtrum:RLEVel", ":SPECtrum:WATerfall", ":SPECtrum:DISTortion"]),
    ("spec_margin", &[":SPECtrum:LIMit:MARGin"]),
    ("spec_acpr", &[":SPECtrum:ACPR:WIDTh", ":SPECtrum:ACPR:SPACing"]),
    ("eye_place", &[":EYE:PLACe"]),
    ("eye_pll", &[":EYE:PLL:BWIDth"]),
    ("power", &[":POWer:VOLTage:SOURce", ":POWer:CURRent:SOURce", ":POWer:PAGE"]),
    ("power_more", &[":POWer:FLINe", ":POWer:FSWitch", ":POWer:VIN:SOURce", ":POWer:REFerence", ":POWer:CLOCk:SOURce"]),
    ("dpt", &[":DPT:VDS:SOURce", ":DPT:ID:SOURce", ":DPT:VGS:SOURce", ":DPT:FOURth", ":DPT:FOURth:SOURce", ":DPT:GATE", ":DPT:ZOOM"]),
    ("bode_mode", &[":BODE:MODE"]),
    // ---- system
    ("aux_out", &[":SYSTem:AOUTput"]),
    ("touch_lock", &[":SYSTem:LOCK"]),
];

/// The `key = value` lines of a set-up text.
fn pairs(text: &str) -> Vec<(&str, &str)> {
    text.lines().filter(|l| !l.starts_with('#')).filter_map(|l| l.split_once('=')).map(|(k, v)| (k.trim(), v.trim())).collect()
}

/// The keys whose value in `after` is not the one in `before`, in the order of `after`, each with its value there;
/// then the keys only `before` has, without one (a key that is only written while it differs from its default).
pub(crate) fn changed(before: &str, after: &str) -> Vec<(String, Option<String>)> {
    let old: std::collections::BTreeMap<&str, &str> = pairs(before).into_iter().collect();
    let new = pairs(after);
    let mut out: Vec<(String, Option<String>)> = new.iter().filter(|(k, v)| old.get(k) != Some(v)).map(|(k, v)| (k.to_string(), Some(v.to_string()))).collect();
    let now: std::collections::BTreeSet<&str> = new.iter().map(|x| x.0).collect();
    out.extend(pairs(before).into_iter().filter(|(k, _)| !now.contains(k)).map(|(k, _)| (k.to_string(), None)));
    out
}

/// The row of `key`: its place in `ROWS` and its headers, with the number of the key in place of `<n>`.
pub(crate) fn row_of(key: &str) -> Option<(usize, Vec<String>)> {
    ROWS.iter().enumerate().find_map(|(i, (pattern, headers))| {
        let n = match pattern.split_once("<n>") {
            None => (*pattern == key).then_some("")?,
            Some((a, b)) => key.strip_prefix(a)?.strip_suffix(b).filter(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))?,
        };
        Some((i, headers.iter().map(|h| h.replace("<n>", n)).collect()))
    })
}

/// How often the commands are mended against the second scope; the last time with key = value lines only.
const PASSES: usize = 4;

/// The second scope: an interpreter no client reaches and a model with no hardware behind it, sent `lines`. What
/// Default leaves alone (`prefs`: the Quick key action and the text size) is as on the first.
fn second_scope(lines: &[String], prefs: (Quick, TextSize), cv: &Canvas) -> (Scpi, Ui) {
    let (mut scpi, mut scope) = (Scpi::detached(), Ui::new(crate::wave::Source::Demo { t: 0.0 }));
    (scope.m.quick, scope.m.text_size) = prefs;
    for l in lines { scpi.exec_line(&mut scope, cv, l); }
    (scpi, scope)
}

impl Scpi {
    /// `:SYSTem:SETup:SCPI?`: `*RST`, then the commands for everything that differs from the Default set-up.
    ///
    /// A command can move more than its own setting (a source switches its channel on, a trigger type brings its
    /// level), so the lines are proven before they are given out. A second scope is sent every line, and what it
    /// still differs in gets a command after the others. A line it refuses, or that moves nothing on it, is not
    /// written, and its key goes as key = value. At the end the lines as they are given out are sent to a scope that
    /// has seen nothing else: if its set-up text is not the one of `ui`, the answer is given and an error says which
    /// keys differ (a value with a `;` in it, which no line can carry).
    pub(crate) fn setup_scpi(&mut self, ui: &mut Ui, cv: &Canvas) -> String {
        ui.sync_view_state();
        let want = crate::store::setup_text(&ui.m);
        let prefs = (ui.m.quick, ui.m.text_size);
        let mut lines = vec!["*RST".to_string()];
        let (mut scpi, mut scope) = second_scope(&lines, prefs, cv);
        let mut refused = std::collections::BTreeSet::new();
        for pass in 0..PASSES {
            let have = crate::store::setup_text(&scope.m);
            let commands = self.setup_commands(ui, cv, &have, &want, &refused, pass + 1 == PASSES);
            if commands.is_empty() { break; }
            for c in commands {
                let before = crate::store::setup_text(&scope.m);
                // not every handler counts its errors (`error_serial`): the queue itself says whether the line was taken
                scpi.err.clear();
                scpi.exec_line(&mut scope, cv, &c);
                let taken = scpi.err.is_empty();
                if taken && crate::store::setup_text(&scope.m) != before {
                    lines.push(c);
                    continue;
                }
                // the header does not carry this value (the value is not one of its own, or is as exact as it goes)
                refused.insert(c.split_whitespace().next().unwrap_or("").to_string());
                // a refused line may have done a part: the second scope is again what the lines alone make of it
                if !taken { (scpi, scope) = second_scope(&lines, prefs, cv); }
            }
        }
        let (scpi, scope) = second_scope(&lines, prefs, cv);
        let mut rest: Vec<String> = changed(&crate::store::setup_text(&scope.m), &want).into_iter().map(|x| x.0).collect();
        if !scpi.err.is_empty() { rest.push(format!("(refused: {})", scpi.err.join(" "))); }
        if !rest.is_empty() { self.push_err(format!("-221,\"Settings conflict\" the commands do not bring back: {}", rest.join(" "))); }
        lines.join("\n") + "\n"
    }

    /// The commands that take a scope from the set-up text `before` to `after`, the set-up `ui` has now: the lines
    /// with a header in the order of `ROWS`, then the keys without one, as key = value. A row with a header in
    /// `refused` is not used, and none is with `plain`. A key no line can carry has none: a value with a `;` (the
    /// interpreter splits a message there), or a key without a usable header that is back at a default the text
    /// does not write.
    pub(crate) fn setup_commands(&mut self, ui: &mut Ui, cv: &Canvas, before: &str, after: &str, refused: &std::collections::BTreeSet<String>, plain: bool) -> Vec<String> {
        // a query that fails here is a row to mend, not an error of the client: the queue stays as it was
        let kept = (self.err.clone(), self.event_status, self.error_serial, self.last_error.clone());
        let mut by_header: Vec<(usize, String)> = Vec::new();
        let mut by_key = Vec::new();
        for (key, value) in changed(before, after) {
            let row = row_of(&key).filter(|(_, headers)| !plain && !headers.iter().any(|h| refused.contains(h)));
            let echoed = row.and_then(|(i, headers)| headers.iter().map(|h| self.echo(ui, cv, h)).collect::<Option<Vec<String>>>().map(|lines| (i, lines)));
            match (echoed, value) {
                (Some((i, lines)), _) => by_header.extend(lines.into_iter().map(|l| (i, l))),
                (None, Some(v)) if !v.contains(';') => by_key.push(format!(":SYSTem:SET {key} = {v}")),
                (None, _) => {}
            }
        }
        (self.err, self.event_status, self.error_serial, self.last_error) = kept;
        by_header.sort_by_key(|x| x.0);
        let mut lines: Vec<String> = Vec::new();
        for (_, l) in by_header {
            // two keys of one header (a scale and its fine factor) are one command
            if !lines.contains(&l) { lines.push(l); }
        }
        lines.extend(by_key);
        lines
    }

    /// `header` with the answer of its own query as the value: the command that sets what is set now.
    fn echo(&mut self, ui: &mut Ui, cv: &Canvas, header: &str) -> Option<String> {
        let (serial, queued) = (self.error_serial, self.err.len());
        let reply = self.exec_cmd(ui, cv, &format!("{header}?"));
        let value = String::from_utf8(reply?).ok()?;
        let value = value.trim();
        (self.error_serial == serial && self.err.len() == queued && !value.is_empty()).then(|| format!("{header} {value}"))
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

    /// Sends the lines to a second scope: its set-up text, and the errors it queued.
    fn replay(lines: &str) -> (String, Vec<String>) {
        let (mut s, mut ui, cv) = session();
        for l in lines.lines() { s.exec_line(&mut ui, &cv, l); }
        (text(&mut ui), s.queued_errors().to_vec())
    }

    /// Values of the keys whose default is a name, which `variants` cannot change by itself.
    const SAMPLES: &[(&str, &str)] = &[
        ("ch<n>.coupling", "Ac"), ("ch<n>.coupling", "Gnd"), ("h_expand", "Left"), ("h_expand", "Trigger"), ("acq", "Peak"), ("acq", "Average"), ("acq", "HiRes"),
        ("trig_type", "Pulse"), ("slope", "Fall"), ("slope", "Either"), ("trig_slope2", "Fall"), ("trig_mode", "Normal"), ("trig_pol", "Neg"),
        ("trig_when", "Gt"), ("trig_when", "In"), ("video_std", "Ntsc"), ("video_sync", "Line"), ("video_sync", "Odd"), ("pat", "L X X H"), ("pat", "H X R H"),
        ("win_mode", "Exit"), ("win_mode", "Inside"), ("cursor", "Time"), ("aux_out", "Off"), ("aux_out", "PassFail"), ("draw_path", "batch"),
        ("hdmi", "Mirror"), ("view", "Spectrum"), ("view", "Eye"), ("layout", "studio"), ("layout", "default"), ("ch<n>.unit", "A"), ("ch<n>.bias", "0.5"),
        ("ch<n>.vfine", "0.8"), ("tfine", "0.8"), ("gen_hidden", "true"), ("eye_pll", "100000"), ("ch<n>.bw", "20 MHz"),
        ("trig_coup", "Ac"), ("trig_coup", "LfReject"), ("hist", "Vertical"), ("hist", "Horizontal"), ("eye_place", "Side"), ("eye_place", "Under"),
        ("bode_mode", "Psrr"), ("bode_mode", "Z"), ("avg_count", "32"), ("avg_count", "64"), ("segments", "100"), ("bus_label", "SPI"), ("dec_src4", "1"),
        ("dec_diff", "1"), ("zdelay", "0.0001"), ("dec_proto", "Spi"), ("dec_proto", "Can"), ("bus_pos", "-2"), ("bus_pos", "1"),
    ];

    /// Other values a key could have: every blank-separated part of its default changed by itself.
    fn variants(default: &str) -> Vec<String> {
        let parts: Vec<&str> = default.split(' ').collect();
        let mut out = Vec::new();
        for (i, p) in parts.iter().enumerate() {
            let mut other: Vec<String> = Vec::new();
            match *p {
                "true" => other.push("false".into()),
                "false" => other.push("true".into()),
                _ => {}
            }
            if let Ok(n) = p.parse::<i64>() {
                other.extend([n + 1, n + 2, n - 1].into_iter().filter(|v| *v >= 0 || n < 0).map(|v| v.to_string()));
            } else if let Ok(x) = p.parse::<f64>() {
                other.extend([x * 2.0, x * 0.5, x + 0.25].map(|v| v.to_string()));
            }
            for o in other {
                let mut q: Vec<&str> = parts.clone();
                q[i] = &o;
                out.push(q.join(" "));
            }
        }
        out
    }

    #[test]
    fn the_keys_that_differ_are_found_in_the_order_of_the_new_text() {
        let before = "# x\na = 1\nb = 2\nc = 3\nd = x = y\n";
        let after = "# x\nb = 5\na = 1\nd = x = z\ne = 9\n";
        let got = changed(before, after);
        let want: Vec<(String, Option<String>)> = [("b", Some("5")), ("d", Some("x = z")), ("e", Some("9")), ("c", None)].into_iter().map(|(k, v)| (k.to_string(), v.map(str::to_string))).collect();
        assert_eq!(got, want);
        assert!(changed(before, before).is_empty());
    }

    #[test]
    fn a_key_finds_its_row_with_its_number() {
        assert_eq!(row_of("ch2.on").unwrap().1, [":CHANnel2:DISPlay"]);
        assert_eq!(row_of("tdiv").unwrap().1, [":TIMebase:SCALe"]);
        assert!(row_of("ch.on").is_none() && row_of("chx.on").is_none() && row_of("skin").is_none());
        // one row a key
        let mut seen = std::collections::BTreeSet::new();
        for (key, headers) in ROWS {
            assert!(seen.insert(*key), "{key} has two rows");
            assert!(!headers.is_empty() && headers.iter().all(|h| h.starts_with(':')), "{key}");
        }
    }

    #[test]
    fn the_default_setup_is_the_reset_alone() {
        let (mut s, mut ui, cv) = session();
        assert_eq!(s.setup_scpi(&mut ui, &cv), "*RST\n");
        // and after the Default key, whatever was set before
        s.exec_line(&mut ui, &cv, ":CHANnel2:DISPlay ON");
        assert_ne!(s.setup_scpi(&mut ui, &cv), "*RST\n");
        s.exec_line(&mut ui, &cv, "*RST");
        assert_eq!(s.setup_scpi(&mut ui, &cv), "*RST\n");
        let (replayed, errors) = replay("*RST\n");
        assert_eq!(replayed, text(&mut ui));
        assert!(errors.is_empty(), "{errors:?}");
    }

    #[test]
    fn the_query_answers_a_block_and_leaves_the_error_queue_alone() {
        let (mut s, mut ui, cv) = session();
        s.exec_line(&mut ui, &cv, ":CHANnel2:DISPlay ON");
        s.exec_line(&mut ui, &cv, ":NO:SUCH:HEADer 1");
        let queued = s.queued_errors().to_vec();
        assert_eq!(queued.len(), 1);
        let reply = String::from_utf8(s.exec_line(&mut ui, &cv, ":SYSTem:SETup:SCPI?")).unwrap();
        let body = "*RST\n:CHANnel2:DISPlay 1\n";
        assert_eq!(reply, format!("#9{:09}{body}", body.len()));
        assert_eq!(String::from_utf8(s.exec_line(&mut ui, &cv, ":SYST:SETup:SCPI?")).unwrap(), reply, "the short form of the first keyword");
        assert_eq!(s.queued_errors(), queued);
        // :SYST:SET is the key = value command: SETup is spelled out here too
        s.exec_line(&mut ui, &cv, ":SYST:SET:SCPI?");
        assert_eq!(s.queued_errors().len(), 2);
        s.exec_line(&mut ui, &cv, "*CLS");
        s.exec_line(&mut ui, &cv, ":NO:SUCH:HEADer 1");
        // there is nothing to set
        s.exec_line(&mut ui, &cv, ":SYSTem:SETup:SCPI 1");
        assert_eq!(s.queued_errors().len(), 2);
    }

    /// The rows no value of the test reaches by their own header (the key takes values only another setting allows,
    /// or the test has no other value for it). They are still replayed and compared, as key = value lines.
    const UNPROVEN: &[&str] = &["ch<n>.vfine", "tfine", "trig_bus", "hdmi_view", "grid", "dec_sel"];

    /// Keys the set-up schema can write alone but no menu or command does: selecting a decoder also keeps the one
    /// that was selected. Their rows are tried with commands (`what_a_command_moves_...`).
    const BY_COMMAND: &[&str] = &["dec_sel"];

    /// Every row, from the Default set-up: a key is given another value, the commands go to a second scope, and that
    /// scope has the same set-up and no error. For every row at least one value travels by the row's own headers (by
    /// one of them where a key holds several settings: a line that moves nothing is not written).
    #[test]
    fn every_row_brings_a_second_scope_to_the_same_setup() {
        let default = text(&mut session().1);
        let (mut bad, mut unproven) = (Vec::new(), Vec::new());
        for (pattern, headers) in ROWS {
            let mut proven = 0;
            for n in ["1", "2"] {
                let key = pattern.replace("<n>", n);
                let own = pairs(&default).into_iter().find(|(k, _)| *k == key).map(|x| x.1.to_string());
                let mut values = own.as_deref().map(variants).unwrap_or_default();
                values.extend(SAMPLES.iter().filter(|(p, _)| p == pattern).map(|(_, v)| v.to_string()));
                for value in values.into_iter().filter(|_| !BY_COMMAND.contains(pattern)) {
                    let (mut s, mut ui, cv) = session();
                    if crate::store::set(&mut ui.m, &key, &value).is_err() { continue; }
                    ui.mark_changed();
                    let want = text(&mut ui);
                    if !changed(&default, &want).iter().any(|(k, _)| *k == key) { continue; }
                    let lines = s.setup_scpi(&mut ui, &cv);
                    let own_errors = s.queued_errors().to_vec();
                    let (got, errors) = replay(&lines);
                    let differ = changed(&want, &got);
                    if !differ.is_empty() || !errors.is_empty() || !own_errors.is_empty() {
                        bad.push(format!("{key} = {value}: differs {differ:?}, errors {errors:?} {own_errors:?}\n{lines}"));
                    } else if !lines.contains(&format!(":SYSTem:SET {key} ")) && headers.iter().any(|h| lines.contains(&format!("{} ", h.replace("<n>", n)))) {
                        proven += 1;
                    }
                }
                if !pattern.contains("<n>") { break; }
            }
            if proven == 0 { unproven.push(*pattern); }
        }
        assert!(bad.is_empty() && unproven == UNPROVEN, "{} values fail:\n{}\nthe rows no value travelled by its own headers: {unproven:?}", bad.len(), bad.join("\n"));
    }

    /// Settings a command moves besides its own are put right by a later line: a source that is off again, a level
    /// back at its default after the trigger type brought another.
    #[test]
    fn what_a_command_moves_besides_its_own_setting_is_put_right() {
        let (mut s, mut ui, cv) = session();
        for c in [":BUS:LANE:CLOCk:SOURce CHANnel2", ":CHANnel2:DISPlay OFF", ":TRIGger:MODE PULSe", ":TRIGger:EDGE:LEVel 1.5", ":DECode:SELect 2", ":DECode:STATe ON"] {
            s.exec_line(&mut ui, &cv, c);
            assert!(s.queued_errors().is_empty(), "{c}: {:?}", s.queued_errors());
        }
        let want = text(&mut ui);
        let lines = s.setup_scpi(&mut ui, &cv);
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        assert!(lines.contains(":DECode:SELect ") && lines.contains(":BUS:LANE:CLOCk:SOURce "), "{lines}");
        let (got, errors) = replay(&lines);
        assert!(errors.is_empty(), "{errors:?}\n{lines}");
        assert_eq!(changed(&want, &got), Vec::new(), "{lines}");
    }

    /// A value its query does not write to the last digit travels once by its header and then as key = value: the
    /// header line is not sent again and again.
    #[test]
    fn a_value_finer_than_its_query_is_one_line_and_its_key() {
        let (mut s, mut ui, cv) = session();
        crate::store::set(&mut ui.m, "ch1.offset", "0.123456789").unwrap();
        ui.mark_changed();
        let want = text(&mut ui);
        let lines = s.setup_scpi(&mut ui, &cv);
        assert!(s.queued_errors().is_empty(), "{:?}", s.queued_errors());
        assert_eq!(lines.matches(":CHANnel1:OFFSet ").count(), 1, "{lines}");
        assert_eq!(lines.matches(":SYSTem:SET ch1.offset = ").count(), 1, "{lines}");
        let (got, errors) = replay(&lines);
        assert!(errors.is_empty(), "{errors:?}\n{lines}");
        assert_eq!(changed(&want, &got), Vec::new(), "{lines}");
    }

    /// A value with a `;` cannot travel in a line (the interpreter splits a message there): it is not written, and
    /// the error names its key. The other settings still travel.
    #[test]
    fn a_value_with_a_semicolon_is_named_not_written() {
        let (mut s, mut ui, cv) = session();
        crate::store::set(&mut ui.m, "studio_dock", "tree;f@1:2:300:200").unwrap();
        s.exec_line(&mut ui, &cv, ":CHANnel2:DISPlay ON");
        let want = text(&mut ui);
        assert!(want.contains("studio_dock = tree;f@1:2:300:200"), "the key is in the set-up text");
        let lines = s.setup_scpi(&mut ui, &cv);
        assert_eq!(lines, "*RST\n:CHANnel2:DISPlay 1\n");
        let errors = s.queued_errors().to_vec();
        assert!(errors.len() == 1 && errors[0].starts_with("-221") && errors[0].contains("studio_dock"), "{errors:?}");
    }

    /// A set-up made by commands, as a user would make it, with settings that lean on each other.
    #[test]
    fn a_setup_of_many_settings_comes_back_on_a_second_scope() {
        let (mut s, mut ui, cv) = session();
        for c in [":CHANnel2:DISPlay ON", ":CHAN1:SCAL 0.5", ":CHANnel1:OFFSet 0.25", ":CHANnel2:COUPling AC", ":TIM:SCAL 1e-3", ":TIMebase:OFFSet 2e-3",
            ":TRIGger:MODE PULSe", ":TRIG:PULS:POL NEG", ":TRIG:PULS:WHEN GREater", ":TRIG:PULS:WIDT 2e-6", ":TRIGger:SWEep NORMal", ":TRIGger:EDGE:SOURce CHANnel2",
            ":ACQuire:TYPE AVERages", ":ACQuire:AVERages 64", ":CURSor:MODE MANual", ":MEASure:ITEM VPP,CHAN2", ":TRIG:VID:STAN NTSC"] {
            s.exec_line(&mut ui, &cv, c);
            assert!(s.queued_errors().is_empty(), "{c}: {:?}", s.queued_errors());
        }
        let want = text(&mut ui);
        let lines = s.setup_scpi(&mut ui, &cv);
        assert!(lines.starts_with("*RST\n:CHANnel2:DISPlay 1\n"), "{lines}");
        let (got, errors) = replay(&lines);
        assert!(errors.is_empty(), "{errors:?}\n{lines}");
        assert_eq!(changed(&want, &got), Vec::new(), "{lines}");
        assert_eq!(got, want);
    }

    /// The "Equivalent header" column of `docs/scpi/setup-keys.md` names the headers of the rows.
    #[test]
    fn the_setup_keys_page_names_the_headers_of_the_rows() {
        let page = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../docs/scpi/setup-keys.md")).unwrap();
        let mut wrong = Vec::new();
        for (key, headers) in ROWS {
            let Some(row) = page.lines().find(|l| l.starts_with('|') && l.split('|').nth(1).is_some_and(|cell| cell.contains(&format!("`{key}`")))) else {
                wrong.push(format!("{key}: no row"));
                continue;
            };
            let cell = row.trim_end().trim_end_matches('|').rsplit('|').next().unwrap_or("");
            for h in headers.iter().filter(|h| !cell.contains(&format!("`{h}`"))) { wrong.push(format!("{key}: {h} is not in {cell:?}")); }
        }
        assert!(wrong.is_empty(), "docs/scpi/setup-keys.md:\n{}", wrong.join("\n"));
    }
}
