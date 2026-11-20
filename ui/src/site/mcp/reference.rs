// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! The SCPI reference for the MCP tool `scpi_reference` (ADR 0074): the command chapter of the SCPI Programming
//! Manual, `docs/manual/commands.json`, which `docs/manual/gen_commands.py` writes from docs/scpi/ and the layer `test`
//! keeps in step with it. An assistant looks a command up here instead of guessing its name: every command the
//! interpreter takes is in it (`tools/scpi-coverage.py`), and a header it does not have is answered -113.

use crate::json::{parse, J};
use std::sync::OnceLock;

const MANUAL: &str = include_str!("../../../../docs/manual/commands.json");

/// Commands one call answers by default, and at most.
pub const RESULTS_DEFAULT: usize = 20;
pub const RESULTS_MAX: usize = 50;

pub struct Reference {
    /// The subsystems' titles, in the manual's order.
    pub titles: Vec<String>,
    /// Every command: its subsystem's index in `titles`, its entry, the nodes of its header.
    pub commands: Vec<(usize, J, Vec<Node>)>,
}

/// One node of a header pattern, `CHANnel<n>`: its short form `CHAN`, its long form `CHANNEL`, whether a number may
/// follow it, and whether the node itself may be left out (`[:MAIN]`).
pub struct Node {
    short: String,
    long: String,
    suffix: bool,
    optional: bool,
}

/// The reference, read from the manual the first time it is asked.
pub fn reference() -> &'static Reference {
    static R: OnceLock<Reference> = OnceLock::new();
    R.get_or_init(|| {
        let manual = parse(MANUAL).expect("docs/manual/commands.json is JSON");
        let mut titles = Vec::new();
        let mut commands = Vec::new();
        if let Some(J::Arr(subsystems)) = manual.get("subsystems") {
            for s in subsystems {
                titles.push(s.get("title").and_then(J::str).unwrap_or("").to_string());
                if let Some(J::Arr(list)) = s.get("commands") {
                    for c in list {
                        let header = c.get("header").and_then(J::str).unwrap_or("");
                        commands.push((titles.len() - 1, c.clone(), nodes(header)));
                    }
                }
            }
        }
        Reference { titles, commands }
    })
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

/// The commands whose header, syntax, description or subsystem holds every word of `words`, those with a word in the
/// header first.
pub fn search(words: &str, subsystem: Option<&str>) -> Vec<&'static (usize, J, Vec<Node>)> {
    let r = reference();
    let words: Vec<String> = words.split_whitespace().map(str::to_lowercase).collect();
    let subsystem = subsystem.map(str::to_lowercase);
    let mut hits: Vec<(usize, &(usize, J, Vec<Node>))> = r
        .commands
        .iter()
        .filter(|(s, _, _)| subsystem.as_ref().is_none_or(|want| r.titles[*s].to_lowercase().contains(want)))
        .filter_map(|entry| {
            let (s, c, _) = entry;
            let field = |k: &str| c.get(k).and_then(J::str).unwrap_or("").to_lowercase();
            let header = field("header");
            let text = format!("{header} {} {} {} {}", field("set"), field("query"), field("desc"), r.titles[*s]);
            let text = text.to_lowercase();
            words
                .iter()
                .all(|w| text.contains(w.as_str()))
                .then(|| (words.iter().filter(|w| header.contains(w.as_str())).count(), entry))
        })
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0));
    hits.into_iter().map(|(_, e)| e).collect()
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

/// A command whole: also its parameters, what its query returns, an example and the remarks of the manual.
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
    for k in ["returns", "example"] {
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

    #[test]
    fn a_search_finds_the_command_by_what_it_does() {
        let found = headers(search("trigger level", None));
        assert!(found.iter().any(|h| h.contains("LEVel")), "{found:?}");
        assert!(headers(search("scale", Some("channels"))).contains(&":CHANnel<n>:SCALe".to_string()));
        assert!(search("no such words anywhere", None).is_empty());
        let full = full(lookup(":CHAN1:SCAL")[0]);
        assert!(full.get("params").is_some() && full.get("query").is_some(), "{full}");
    }
}
