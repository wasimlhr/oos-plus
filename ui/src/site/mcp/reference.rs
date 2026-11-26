// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The SCPI reference for the MCP tool `scpi_reference` (ADR 0074): the command chapter of the SCPI Programming
//! Manual, `docs/manual/commands.json`, which `docs/manual/gen_commands.py` writes from docs/scpi/ and the layer `test`
//! keeps in step with it. An assistant looks a command up here instead of guessing its name: every command the
//! interpreter takes is in it (`tools/scpi-coverage.py`), and a header it does not have is answered -113. It asks in
//! its own words, not the reference's, so a search ranks the commands by how well they fit and knows what an
//! assistant says for a word of the reference (`PHRASES`, `ALSO`).

use crate::json::{parse, J};
use std::sync::OnceLock;

const MANUAL: &str = include_str!("../../../../docs/manual/commands.json");

/// Commands one call answers by default, and at most.
pub const RESULTS_DEFAULT: usize = 20;
pub const RESULTS_MAX: usize = 50;

/// Rows of the appendices one search answers at most.
pub const ITEMS_MAX: usize = 8;

/// What a word of a search counts by where a command has it (`Reference::words`): a word in the header says more
/// than one in the description, and that more than one in a parameter's text.
const WEIGHTS: [usize; 3] = [4, 3, 1];
/// A search answers the commands whose score is at least this part of the best one's, so that a command with every
/// word in its header is not followed by all that hold one of them.
const KEEP: (usize, usize) = (3, 4);

/// What an assistant says for what the reference calls otherwise, as whole phrases: each is replaced by the
/// reference's words before the words are looked up.
const PHRASES: &[(&str, &str)] = &[
    ("volts per division", "channel scale"),
    ("volts per div", "channel scale"),
    ("v div", "channel scale"),
    ("time per division", "timebase scale"),
    ("seconds per division", "timebase scale"),
    ("s div", "timebase scale"),
    ("record length", "memory depth"),
    ("sampling rate", "sample rate"),
    ("single shot", "single"),
    ("eye diagram", "eye"),
    ("bode plot", "bode"),
    ("pass fail", "mask"),
    ("signal generator", "generator"),
    ("function generator", "generator"),
    ("waveform generator", "generator"),
    ("factory reset", "rst default"),
    ("triggered", "trigger status"),
    ("screen shot", "screenshot"),
    ("move a window", "dock move"),
    ("move the window", "dock move"),
    ("move a view", "dock move"),
    ("move a panel", "dock move"),
    ("next to", "dock move"),
    ("resize a window", "dock size"),
    ("resize a panel", "dock size"),
    ("resize a view", "dock size"),
    ("list the windows", "dock list"),
    ("list the views", "dock list"),
    ("float a window", "dock float"),
    ("close a window", "dock close"),
    ("maximise", "dock maximise"),
    ("maximize", "dock maximise"),
    ("show the simulator", "dock show simulator"),
    ("open the simulator", "dock show simulator"),
    ("simulate a signal", "sim play"),
    ("simulate", "sim"),
    ("zoom into", "zoom here"),
    ("tap the screen", "ui tap"),
    ("buttons on the screen", "ui elements"),
    ("press a menu", "ui menu select"),
];
/// Words of a search that say nothing about a command.
const STOP: &[&str] = &[
    "the", "a", "an", "of", "to", "per", "is", "are", "for", "in", "and", "or", "how", "do", "does", "i", "my", "it",
    "its", "set", "sets", "get", "change", "what", "with", "scope", "oscilloscope", "turn", "command",
];
/// Words the reference uses where a search has another: a command fits the word of the search or one of these.
const ALSO: &[(&str, &str)] = &[
    ("vertical", "channel"),
    ("horizontal", "timebase"),
    ("input", "channel"),
    ("attenuation", "probe ratio"),
    ("bandwidth", "bwlimit"),
    ("on", "display enable state"),
    ("off", "display enable state"),
    ("enable", "state display"),
    ("show", "display"),
    ("position", "offset"),
    ("delay", "offset"),
    ("zoom", "delay"),
    ("rising", "slope positive"),
    ("falling", "slope negative"),
    ("glitch", "pulse"),
    ("mode", "type"),
    ("kind", "type"),
    ("level", "threshold"),
    ("threshold", "level"),
    ("capture", "acquire"),
    ("acquisition", "acquire"),
    ("averaging", "averages"),
    ("average", "averages"),
    ("depth", "mdepth"),
    ("download", "data"),
    ("fetch", "data"),
    ("picture", "screenshot"),
    ("image", "screenshot"),
    ("store", "save"),
    ("recall", "load"),
    ("restore", "load"),
    ("reset", "rst"),
    ("autoset", "autoscale"),
    ("queue", "error"),
    ("uart", "rs232"),
    ("serial", "rs232"),
    ("i2c", "iic"),
    ("decode", "bus"),
    ("decoder", "bus"),
    ("fft", "spectrum"),
    ("center", "centre"),
    ("jitter", "tie"),
    ("persistence", "grading"),
    ("awg", "generator source"),
    ("analyzer", "analyser"),
    ("voltmeter", "dvm"),
    ("ip", "ipaddress"),
    ("lan", "network"),
    ("ethernet", "network"),
    ("label", "name"),
    ("colour", "color"),
];

pub struct Reference {
    /// The subsystems' titles, in the manual's order.
    pub titles: Vec<String>,
    /// What the manual says before each subsystem's commands, and its examples after them.
    pub about: Vec<[String; 2]>,
    /// Every command: its subsystem's index in `titles`, its entry, the nodes of its header.
    pub commands: Vec<(usize, J, Vec<Node>)>,
    /// Every command's words by where they stand (`WEIGHTS`): in its header, in its description and its subsystem's
    /// title, in its parameters, its remarks and what its query returns.
    words: Vec<[Vec<String>; 3]>,
    /// The appendices, in the manual's order.
    pub appendices: Vec<Appendix>,
}

/// An appendix of the manual: the tables a command's parameter points to (the measurement items, the error codes).
pub struct Appendix {
    pub title: String,
    /// What the manual says before its tables: the commands its rows are for.
    pub about: String,
    /// Its rows as objects of their columns (and `table`, where the appendix has several), each with its words.
    pub rows: Vec<(J, Vec<String>)>,
}

/// One node of a header pattern, `CHANnel<n>`: its short form `CHAN`, its long form `CHANNEL`, whether a number may
/// follow it, and whether the node itself may be left out (`[:MAIN]`).
pub struct Node {
    short: String,
    long: String,
    suffix: bool,
    optional: bool,
}

/// What the scope can do, by what an assistant wants done: the task in its words, the subsystems of the manual that
/// hold it (a part of each title, as `subsystem` takes it), and where to start. An assistant that is only told the
/// typed tools does not know the screen can be arranged, a signal simulated or a menu pressed; the answer of
/// `scpi_reference` without arguments lists this, and the instructions of `initialize` point to it.
pub const TASKS: &[(&str, &[&str], &str)] = &[
    (
        "Arrange the screen: dock, float, move, resize, tab, maximise or close the views (waveform, measurements, eye,          simulator, events)",
        &["Docking", "Views"],
        ":DOCK:LIST shows every view and where it is; then :DOCK:MOVE, :DOCK:FLOAT, :DOCK:SIZE, :DOCK:SHOW, :DOCK:CLOSe",
    ),
    (
        "Zoom, fit the record, one plot or one lane per trace",
        &["Studio zoom", "Display and layout"],
        ":ZOOM:HERE, :ZOOM:BOX, :VIEW:FIT, :VIEW:TRACes",
    ),
    (
        "Simulate a signal (a lane such as PCIe 4.0, a pattern, jitter, spread) and add it as a reference",
        &["Signal simulator"],
        ":SIM:PRESet, :SIM:PLAY ON, then :SIM:ADDRef",
    ),
    (
        "Add or show a plot or analyser: eye, spectrum, histogram, XY, measurement table, bode, power, mask",
        &["Eye and jitter", "Spectrum", "Measurements: Readouts", "Mask", "Bode", "Power, magnetic"],
        ":DOCK:SHOW for the measurement table and the simulator; the others open with their own commands (:EYE,          :SPECtrum, :BODE ...)",
    ),
    (
        "Reference waveforms and math traces: save a trace, load a file, add a math trace",
        &["Reference waveforms", "Math"],
        ":REFerence<n>..., :MATH<n>:...",
    ),
    ("Decode a bus (UART, I2C, SPI, CAN, 1553 ...) and read its events", &["Decoders and bus"], "setup_decoder, decode_results"),
    ("Mark and find events in the record", &["Search"], ":SEARch:..."),
    ("Logic analyser lines", &["Logic analyser"], ":LA:STATe"),
    ("Generator: built-in and external", &["Built-in generator", "External generator"], ":SOURce<n>:..."),
    ("Cursors, DVM, frequency counter", &["Cursors", "DVM", "Counter"], ":CURSor:..., :DVM:..., :COUNter:..."),
    ("Save and load setups, waveforms and screens", &["Save and load"], ":SAVE:..., :LOAD:..."),
    (
        "Anything a person does with a finger that no command above covers: read the controls on the screen, tap, drag,          open and press a menu row",
        &["Screen controls", "Right-click menu"],
        ":UI:ELEMents, :UI:TAP, :UI:DRAG, :UI:CONText, :UI:MENU?, :UI:MENU:SELect; check the result with the screenshot tool",
    ),
];

/// The reference, read from the manual the first time it is asked.
pub fn reference() -> &'static Reference {
    static R: OnceLock<Reference> = OnceLock::new();
    R.get_or_init(|| {
        let manual = parse(MANUAL).expect("docs/manual/commands.json is JSON");
        let (mut titles, mut about, mut commands, mut words) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        if let Some(J::Arr(subsystems)) = manual.get("subsystems") {
            for s in subsystems {
                let title = s.get("title").and_then(J::str).unwrap_or("");
                titles.push(title.to_string());
                about.push([text(s.get("intro")), text(s.get("after"))]);
                if let Some(J::Arr(list)) = s.get("commands") {
                    for c in list {
                        let nodes = nodes(c.get("header").and_then(J::str).unwrap_or(""));
                        words.push(command_words(c, title, &nodes));
                        commands.push((titles.len() - 1, c.clone(), nodes));
                    }
                }
            }
        }
        let appendices = match manual.get("appendices") {
            Some(J::Arr(list)) => list.iter().map(appendix_of).collect(),
            _ => Vec::new(),
        };
        Reference { titles, about, commands, words, appendices }
    })
}

/// The text of a part of the manual: a string, or blocks (`p`, `h`, `pre`, the items of `ul`), one per line.
fn text(part: Option<&J>) -> String {
    let line = |v: &J| match v {
        J::Arr(items) => items.iter().filter_map(J::str).collect::<Vec<_>>().join("\n"),
        v => v.str().unwrap_or("").to_string(),
    };
    match part {
        Some(J::Str(s)) => s.clone(),
        Some(J::Arr(blocks)) => {
            let block = |b: &J| match b {
                J::Obj(fields) => fields.iter().map(|(_, v)| line(v)).collect::<Vec<_>>().join("\n"),
                _ => String::new(),
            };
            blocks.iter().map(block).collect::<Vec<_>>().join("\n")
        }
        _ => String::new(),
    }
}

/// The words of a text, lower case: its runs of letters and digits.
fn words_of(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    lower.split(|c: char| !c.is_ascii_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_string).collect()
}

/// The words of a command by where they stand (`Reference::words`); a header gives both forms of its nodes.
fn command_words(c: &J, title: &str, nodes: &[Node]) -> [Vec<String>; 3] {
    let field = |k: &str| c.get(k).and_then(J::str).unwrap_or("");
    let mut header = words_of(&field("header").replace("<n>", ""));
    header.extend(nodes.iter().map(|n| n.short.to_lowercase()));
    let mut rest = field("returns").to_string();
    for list in ["params", "remarks"] {
        if let Some(J::Arr(items)) = c.get(list) {
            let said = |i: &J| match i {
                J::Arr(cells) => cells.iter().skip(2).filter_map(J::str).collect::<Vec<_>>().join(" "),
                i => i.str().unwrap_or("").to_string(),
            };
            items.iter().for_each(|i| rest = format!("{rest} {}", said(i)));
        }
    }
    [header, words_of(&format!("{title} {}", field("desc"))), words_of(&rest)]
}

/// An appendix of the manual with the rows of its tables.
fn appendix_of(a: &J) -> Appendix {
    let title = a.get("title").and_then(J::str).unwrap_or("").to_string();
    let mut rows = Vec::new();
    if let Some(J::Arr(tables)) = a.get("tables") {
        for t in tables {
            let table = t.get("title").and_then(J::str);
            let (Some(J::Arr(columns)), Some(J::Arr(list))) = (t.get("columns"), t.get("rows")) else { continue };
            for row in list {
                let J::Arr(cells) = row else { continue };
                let mut fields: Vec<(String, J)> = table.iter().map(|t| ("table".to_string(), J::s(t))).collect();
                fields.extend(columns.iter().zip(cells).map(|(k, v)| (k.str().unwrap_or("").to_string(), v.clone())));
                let said: Vec<&str> = cells.iter().filter_map(J::str).collect();
                let words = words_of(&format!("{title} {} {}", table.unwrap_or(""), said.join(" ")));
                rows.push((J::Obj(fields), words));
            }
        }
    }
    Appendix { about: text(a.get("intro")), title, rows }
}

/// A word without the ending that only bends it: `averaging` and `averages` are the same word to a search.
fn stem(word: &str) -> &str {
    let cut = |end: &&str| word.strip_suffix(*end).filter(|s| s.len() >= 3);
    ["ing", "ed", "es", "s"].iter().find_map(cut).unwrap_or(word)
}

/// How well a word of a search fits a word of the reference: 2 the same word, 1 the same but for its ending or one
/// the start of the other (`measure`, `measurement`), 0 another word. Short words (`on`, `la`) fit only whole.
fn fits(said: &str, word: &str) -> usize {
    let start = || word.starts_with(stem(said)) || (word.len() >= 5 && said.starts_with(word));
    let near = || stem(said) == stem(word) || start();
    match said == word {
        true => 2,
        false => (said.len() >= 4 && near()) as usize,
    }
}

/// What a search asks for: for each of its words that says something, the words of the reference that fit it.
fn wanted(search: &str) -> Vec<Vec<String>> {
    let mut text = format!(" {} ", words_of(search).join(" "));
    for (said, meant) in PHRASES {
        text = text.replace(&format!(" {said} "), &format!(" {meant} "));
    }
    let every: Vec<&str> = text.split_whitespace().collect();
    let saying: Vec<&str> = every.iter().copied().filter(|w| !STOP.contains(w)).collect();
    let mut out: Vec<Vec<String>> = Vec::new();
    for word in if saying.is_empty() { every } else { saying } {
        let also = ALSO.iter().filter(|(said, _)| *said == word).flat_map(|(_, meant)| meant.split(' '));
        let fit: Vec<String> = std::iter::once(word).chain(also).map(str::to_string).collect();
        if !out.contains(&fit) {
            out.push(fit);
        }
    }
    out
}

/// How well a command fits a search: for each word asked for, the best place the command has it.
fn score(wanted: &[Vec<String>], words: &[Vec<String>; 3]) -> usize {
    let best = |fit: &Vec<String>| {
        let one = |f: &String, place: &Vec<String>| place.iter().map(|w| fits(f, w)).max().unwrap_or(0);
        let of = |place: &Vec<String>| fit.iter().map(|f| one(f, place)).max().unwrap_or(0);
        words.iter().zip(WEIGHTS).map(|(place, weight)| weight * of(place)).max().unwrap_or(0)
    };
    wanted.iter().map(best).sum()
}

/// The nodes of a header pattern of the manual: `:CHANnel<n>:SCALe`, `:SOURce[<n>]:VOLTage[:LEVel]`, `*IDN`.
fn nodes(pattern: &str) -> Vec<Node> {
    let pattern = pattern.replace("[<n>]", "<n>");
    let mut out = Vec::new();
    let (mut depth, mut node, mut optional) = (0usize, String::new(), false);
    let mut flush = |node: &mut String, optional: bool| {
        let text = std::mem::take(node);
        let suffix = text.contains("<n>");
        let name: String = text.replace("<n>", "").chars().filter(|c| *c != '?').collect();
        if name.is_empty() {
            return;
        }
        let short: String = name.chars().take_while(|c| !c.is_ascii_lowercase()).collect();
        out.push(Node { short: short.to_ascii_uppercase(), long: name.to_ascii_uppercase(), suffix, optional });
    };
    for c in pattern.chars() {
        match c {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            ':' => {
                flush(&mut node, optional);
                optional = depth > 0;
            }
            c => node.push(c),
        }
    }
    flush(&mut node, optional);
    out
}

/// Whether the header of a command as a client writes it, `:CHAN1:SCAL?`, `:channel1:scale`, is this pattern's.
fn matches(pattern: &[Node], given: &[String]) -> bool {
    let Some(p) = pattern.first() else { return given.is_empty() };
    let node_fits = |g: &str| {
        let name = if p.suffix { g.trim_end_matches(|c: char| c.is_ascii_digit()) } else { g };
        name == p.short || name == p.long || g == p.short || g == p.long
    };
    (p.optional && matches(&pattern[1..], given))
        || given.first().is_some_and(|g| node_fits(g) && matches(&pattern[1..], &given[1..]))
}

/// The nodes of a header as a client writes it, upper case, without its parameters and its `?`.
fn given(header: &str) -> Vec<String> {
    let header = header.split_whitespace().next().unwrap_or("");
    header.trim_start_matches(':').trim_end_matches('?').split(':').map(str::to_ascii_uppercase).collect()
}

/// The commands whose header is `header`, in either form.
pub fn lookup(header: &str) -> Vec<&'static (usize, J, Vec<Node>)> {
    let g = given(header);
    reference().commands.iter().filter(|(_, _, n)| matches(n, &g)).collect()
}

/// The query that reads back what one command of a message set, `:CHAN1:SCAL 0.5` -> `:CHAN1:SCAL?`: None for a
/// query, a common command (`*RST`), or a command whose reference entry has no query without parameters.
pub fn read_back(command: &str) -> Option<String> {
    let header = command.split_whitespace().next()?;
    if header.ends_with('?') || header.starts_with('*') {
        return None;
    }
    let plain = |c: &J| c.get("query").and_then(J::str).is_some_and(|q| q.ends_with('?') && !q.contains(' '));
    lookup(header).iter().any(|(_, c, _)| plain(c)).then(|| format!("{header}?"))
}

/// The commands that fit the words of a search best, the best first and then in the manual's order: a word counts
/// by where the command has it (`WEIGHTS`), a phrase and a word an assistant says count as the reference's own
/// (`PHRASES`, `ALSO`), and a command need not hold every word. Without words, the commands of `subsystem`.
pub fn search(words: &str, subsystem: Option<&str>) -> Vec<&'static (usize, J, Vec<Node>)> {
    let r = reference();
    let wanted = wanted(words);
    let subsystem = subsystem.map(str::to_lowercase);
    let mut hits: Vec<(usize, &(usize, J, Vec<Node>))> = r
        .commands
        .iter()
        .zip(&r.words)
        .filter(|((s, _, _), _)| subsystem.as_ref().is_none_or(|want| r.titles[*s].to_lowercase().contains(want)))
        .map(|(entry, words)| (score(&wanted, words), entry))
        .collect();
    if !wanted.is_empty() {
        let best = hits.iter().map(|h| h.0).max().unwrap_or(0);
        hits.retain(|h| h.0 > 0 && h.0 * KEEP.1 >= best * KEEP.0);
        hits.sort_by(|a, b| b.0.cmp(&a.0));
    }
    hits.into_iter().map(|(_, e)| e).collect()
}

/// The rows of the appendices that hold every word of a search, by appendix and `ITEMS_MAX` in all: the item a
/// command takes (`VPP` for "peak to peak"), with what the appendix says of its use. The setup keys are left out:
/// where a key has a command, the search finds the command.
pub fn items(words: &str) -> Vec<J> {
    let wanted = wanted(words);
    let has = |row: &(J, Vec<String>), fit: &Vec<String>| fit.iter().any(|f| row.1.iter().any(|w| fits(f, w) > 0));
    let (mut out, mut left) = (Vec::new(), ITEMS_MAX);
    for a in reference().appendices.iter().filter(|a| a.title != "Setup Keys") {
        let holds = a.rows.iter().filter(|row| wanted.iter().all(|fit| has(row, fit)));
        let rows: Vec<J> = holds.take(left).map(|(j, _)| j.clone()).collect();
        left -= rows.len();
        if !rows.is_empty() {
            out.push(J::obj(&[("appendix", J::s(&a.title)), ("about", J::s(&a.about)), ("rows", J::Arr(rows))]));
        }
    }
    out
}

/// The appendices whose title holds `title`.
pub fn appendices(title: &str) -> Vec<&'static Appendix> {
    let title = title.to_lowercase();
    reference().appendices.iter().filter(|a| a.title.to_lowercase().contains(&title)).collect()
}

/// A command in a list: its header, the forms to send and what it does.
pub fn brief((s, c, _): &(usize, J, Vec<Node>)) -> J {
    let mut out = vec![("header", c.get("header").cloned().unwrap_or(J::Null))];
    for k in ["set", "query", "desc"] {
        if let Some(v) = c.get(k) {
            out.push((k, v.clone()));
        }
    }
    out.push(("subsystem", J::s(&reference().titles[*s])));
    J::obj(&out)
}

/// A command whole: also its parameters, what its query returns, an example, the headers of related commands (where
/// its row of the reference names them) and the remarks of the manual.
pub fn full(entry: &(usize, J, Vec<Node>)) -> J {
    let J::Obj(mut out) = brief(entry) else { return J::Null };
    let c = &entry.1;
    if let Some(J::Arr(params)) = c.get("params") {
        let param = |p: &J| {
            let J::Arr(f) = p else { return J::Null };
            let at = |i: usize| f.get(i).cloned().unwrap_or(J::Null);
            J::obj(&[("name", at(0)), ("type", at(1)), ("description", at(2)), ("range", at(3))])
        };
        out.push(("params".into(), J::Arr(params.iter().map(param).collect())));
    }
    for k in ["returns", "example", "related"] {
        if let Some(v) = c.get(k) {
            out.push((k.into(), v.clone()));
        }
    }
    if let Some(J::Arr(remarks)) = c.get("remarks") {
        let text = |r: &J| J::s(r.str().unwrap_or("").trim_start_matches("- "));
        out.push(("remarks".into(), J::Arr(remarks.iter().map(text).collect())));
    }
    out.push(("oos_extension".into(), J::Bool(c.get("ext") == Some(&J::Bool(true)))));
    J::Obj(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(found: Vec<&(usize, J, Vec<Node>)>) -> Vec<String> {
        found.iter().map(|(_, c, _)| c.get("header").and_then(J::str).unwrap().to_string()).collect()
    }

    // Break it catches: a header of the manual that its own short or long form does not find (a node form read
    // wrongly, an optional node made required), the numbers of a channel taken as part of the name
    #[test]
    fn every_command_of_the_manual_is_found_by_its_short_and_long_form() {
        let r = reference();
        assert!(r.commands.len() > 600 && r.titles.len() > 50, "{} commands", r.commands.len());
        for (_, c, n) in &r.commands {
            let header = c.get("header").and_then(J::str).unwrap();
            let form = |long: bool| -> Vec<String> {
                let node = |p: &Node| format!("{}{}", if long { &p.long } else { &p.short }, if p.suffix { "2" } else { "" });
                n.iter().filter(|p| !p.optional).map(node).collect()
            };
            assert!(matches(n, &form(false)) && matches(n, &form(true)), "{header}");
        }
        assert_eq!(headers(lookup(":CHAN1:SCAL?")), [":CHANnel<n>:SCALe"]);
        assert_eq!(headers(lookup(":channel3:scale 0.5")), [":CHANnel<n>:SCALe"]);
        assert_eq!(headers(lookup("*IDN?")), ["*IDN"]);
        assert!(lookup(":CHAN1:VDIV").is_empty(), "a guessed name is no command");
        assert!(!lookup(":TIM:SCAL").is_empty() && !lookup(":TIMebase:MAIN:SCALe").is_empty());
    }

    // Break it catches: a search that answers nothing, or other commands first, for what an assistant says where the
    // reference has another word (volts per division, record length, glitch, a word bent otherwise), one word no
    // command holds emptying the answer, a word of one command answered with every command of its subsystem
    #[test]
    fn a_search_finds_the_command_by_what_an_assistant_says() {
        for (said, header) in [
            ("volts per division", ":CHANnel<n>:SCALe"),
            ("vertical scale", ":CHANnel<n>:SCALe"),
            ("probe attenuation", ":CHANnel<n>:PROBe"),
            ("input coupling", ":CHANnel<n>:COUPling"),
            ("bandwidth limit", ":CHANnel<n>:BWLimit"),
            ("turn channel on", ":CHANnel<n>:DISPlay"),
            ("time per division", ":TIMebase[:MAIN]:SCALe"),
            ("horizontal position", ":TIMebase[:MAIN]:OFFSet"),
            ("sample rate", ":ACQuire:SRATe"),
            ("record length", ":ACQuire:MDEPth"),
            ("averaging", ":ACQuire:AVERages"),
            ("single shot", ":SINGle"),
            ("trigger level", ":TRIGger:EDGE:LEVel"),
            ("trigger source", ":TRIGger:EDGE:SOURce"),
            ("rising edge", ":TRIGger:EDGE:SLOPe"),
            ("trigger type", ":TRIGger:MODE"),
            ("pulse width trigger", ":TRIGger:PULSe:WHEN"),
            ("glitch", ":TRIGger:PULSe:WHEN"),
            ("force trigger", ":TFORce"),
            ("is the scope triggered", ":TRIGger:STATus"),
            ("measure rise time", ":MEASure:ITEM"),
            ("download waveform", ":WAVeform:DATA"),
            ("screenshot", ":DISPlay:DATA"),
            ("recall setup", ":LOAD:SETup"),
            ("spectrum center frequency", ":SPECtrum:CENTer"),
            ("uart baud rate", ":BUS<n>:RS232:BAUD"),
            ("decoder threshold", ":BUS<n>:THReshold"),
            ("eye diagram", ":EYE:SOURce"),
            ("mask test", ":MASK:ENABle"),
            ("bode plot", ":BODE:STARt"),
            ("signal generator frequency", ":SOURce[<n>]:FREQuency"),
            ("awg output on", ":SOURce[<n>]:OUTPut[:STATe]"),
            ("error queue", ":SYSTem:ERRor"),
            ("factory reset", "*RST"),
            ("ip address", ":SYSTem:COMMunicate:LAN:IPADdress"),
            ("logic analyzer", ":LA:STATe"),
            ("persistence", ":DISPlay:GRADing:TIME"),
            ("zoom", ":TIMebase:DELay:ENABle"),
            ("frequency counter", ":COUNter:ENABle"),
        ] {
            let found = headers(search(said, None));
            assert!(found.iter().take(5).any(|h| h == header), "{said}: {:?}", &found[..found.len().min(5)]);
        }
        assert!(headers(search("scale", Some("channels"))).contains(&":CHANnel<n>:SCALe".to_string()));
        assert!(search("xyzzy qwerty", None).is_empty());
        assert_eq!(headers(search("holdoff xyzzy", None)), [":TRIGger:HOLDoff"]);
        assert!(search("trigger level", None).len() < 40, "{}", search("trigger level", None).len());
        let full = full(lookup(":CHAN1:SCAL")[0]);
        assert!(full.get("params").is_some() && full.get("query").is_some(), "{full}");
    }

    // Break it catches: a task of `TASKS` that names a subsystem the manual no longer has (an assistant is sent to an
    // empty list), a task with no commands behind it, a start command that is no command of the reference
    #[test]
    fn every_task_points_to_subsystems_and_commands_of_the_manual() {
        let r = reference();
        for (task, parts, start) in TASKS {
            for part in *parts {
                assert!(!search("", Some(part)).is_empty(), "{task}: no subsystem {part}");
            }
            for word in start.split([' ', ',', ';']).filter(|w| w.starts_with(':')) {
                let head = word.trim_end_matches(['.', '?', ':']);
                if head.matches(':').count() < 2 || word.contains("...") {
                    continue;
                }
                assert!(!lookup(head).is_empty() || !lookup(&format!("{head}?")).is_empty(), "{task}: {word}");
            }
        }
        assert!(r.titles.len() > TASKS.len());
    }

    // Break it catches: what an assistant asks of the screen, the simulator or the views not finding the command that
    // does it in the first five (the words the manual uses are not the ones said)
    #[test]
    fn a_search_finds_the_command_that_arranges_the_screen() {
        for (said, header) in [
            ("move a window", ":DOCK:MOVE"),
            ("dock the eye next to the waveform", ":DOCK:MOVE"),
            ("float a view", ":DOCK:FLOAT"),
            ("resize a panel", ":DOCK:SIZE"),
            ("close a view", ":DOCK:CLOSe"),
            ("list the windows", ":DOCK:LIST"),
            ("maximise a view", ":DOCK:MAXimise"),
            ("show the simulator", ":DOCK:SHOW"),
            ("simulate a signal", ":SIM:PLAY"),
            ("add the simulated signal to a reference", ":SIM:ADDRef"),
            ("simulator preset", ":SIM:PRESet"),
            ("tap the screen", ":UI:TAP"),
            ("list the buttons on the screen", ":UI:ELEMents"),
            ("press a menu row", ":UI:MENU:SELect"),
            ("fit the record", ":VIEW:FIT"),
            ("zoom into a time", ":ZOOM:HERE"),
        ] {
            let found = headers(search(said, None));
            assert!(found.iter().take(5).any(|h| h == header), "{said}: {:?}", &found[..found.len().min(5)]);
        }
    }

    // Break it catches: a related command that is no command (a header renamed, a typing error in a See also cell: an
    // assistant then sends it), a command that names itself, the related commands left out of the whole answer
    #[test]
    fn a_related_command_is_a_command_of_the_reference() {
        let mut named = 0;
        for (_, c, _) in &reference().commands {
            let header = c.get("header").and_then(J::str).unwrap();
            let Some(J::Arr(related)) = c.get("related") else { continue };
            for other in related.iter().filter_map(J::str) {
                let found = headers(lookup(&other.replace("<n>", "1")));
                assert_eq!(found, [other], "{header} names {other}");
                assert_ne!(other, header);
                named += 1;
            }
        }
        assert!(named > 20, "{named} related commands in the reference");
        let scale = full(lookup(":CHAN1:SCAL")[0]);
        assert!(scale.to_string().contains(r#""related":[":CHANnel<n>:OFFSet""#), "{scale}");
        assert!(scale.get("desc").and_then(J::str).is_some_and(|d| d.contains("volts per division")), "{scale}");
    }

    // Break it catches: the item a command takes not found by what it measures (an assistant then guesses `PKPK`),
    // an item answered without the appendix that says which command takes it, the setup keys among the items
    #[test]
    fn a_search_finds_the_item_of_an_appendix_by_what_it_is() {
        let asked = [("peak to peak", "`VPP`"), ("frequency measurement", "`FREQuency`"), ("rise time", "`RTIMe`")];
        for (said, item) in asked {
            let found = items(said);
            let first = &found[0];
            assert_eq!(first.get("appendix").and_then(J::str), Some("Standard Measurement Items"), "{said}");
            assert!(first.get("about").and_then(J::str).is_some_and(|a| a.contains(":MEASure:ITEM")), "{first}");
            let Some(J::Arr(rows)) = first.get("rows") else { panic!("{first}") };
            assert!(rows.iter().any(|r| r.get("Item").and_then(J::str) == Some(item)), "{said}: {rows:?}");
        }
        let rows = |a: &J| a.get("rows").map_or(0, |r| if let J::Arr(rows) = r { rows.len() } else { 0 });
        assert!(items("voltage").iter().map(rows).sum::<usize>() == ITEMS_MAX && items("xyzzy").is_empty());
        assert!(items("trigger level").iter().all(|a| a.get("appendix").and_then(J::str) != Some("Setup Keys")));
        assert!(appendices("error")[0].rows.len() > 20 && appendices("setup keys")[0].rows.len() > 200);
        let channels = |[about, examples]: &[String; 2]| about.contains("CHANnel<n>") && examples.contains(":CHAN");
        assert!(reference().about.iter().any(channels));
    }
}
