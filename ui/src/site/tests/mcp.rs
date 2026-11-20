// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! Tests of the typed MCP tools (site/mcp.rs, ADR 0074): what each one asks the interpreter and answers, the
//! arguments it refuses before any command runs, and what it leaves as it was.

use super::*;
use crate::json::{parse, J};

/// One `tools/call` of `name` with `args` (JSON), its `result`.
fn call(ui: &mut Ui, cv: &Canvas, scpi: &mut crate::scpi::Scpi, name: &str, args: &str) -> J {
    let body =
        format!(r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{name}","arguments":{args}}}}}"#);
    let reply = crate::site::mcp(&body, &mut Ctx::new(ui, cv, scpi)).unwrap();
    parse(&reply).unwrap().get("result").cloned().unwrap_or_else(|| panic!("no result: {reply}"))
}

fn data(r: &J) -> &J {
    r.get("structuredContent").unwrap_or_else(|| panic!("no structuredContent: {r}"))
}

fn failed(r: &J) -> bool {
    r.get("isError") == Some(&J::Bool(true))
}

fn num(j: Option<&J>) -> f64 {
    match j {
        Some(J::Num(n)) => *n,
        other => panic!("not a number: {other:?}"),
    }
}

fn items(j: Option<&J>) -> &[J] {
    match j {
        Some(J::Arr(v)) => v,
        other => panic!("not a list: {other:?}"),
    }
}

/// A square of ±0.5 V, 4000 samples at 1 ns, in R1.
fn square_in_r1(ui: &mut Ui) {
    let mut csv = String::from("Time (s),Volts\n");
    for i in 0..4000 {
        csv.push_str(&format!("{},{}\n", i as f64 * 1e-9, if i % 40 < 20 { -0.5 } else { 0.5 }));
    }
    ui.load_waveform_bytes(csv.as_bytes(), "square.csv").unwrap();
}

// Break it catches: a tool answered by tools/call but not offered (or the other way round), a reading tool not marked
// read-only, the closed lists of words left out of the schemas
#[test]
fn mcp_lists_every_tool_with_its_schema_and_annotation() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let r = parse(&crate::site::mcp(body, &mut Ctx::new(&mut ui, &cv, &mut scpi)).unwrap()).unwrap();
    let tools = items(r.get("result").and_then(|x| x.get("tools")));
    let read_only = |name: &str| {
        let t = tools.iter().find(|t| t.get("name").and_then(J::str) == Some(name)).expect(name);
        t.get("annotations").and_then(|a| a.get("readOnlyHint")) == Some(&J::Bool(true))
    };
    for name in ["screenshot", "scope_state", "measure", "waveform", "decode_results", "errors"] {
        assert!(read_only(name), "{name} only reads");
    }
    for name in ["scpi", "configure", "acquire", "setup_decoder"] {
        assert!(!read_only(name), "{name} can change the scope");
    }
    assert_eq!(tools.len(), 10);
    let s = r.to_string();
    assert!(s.contains("\"VPP\"") && s.contains("\"RRDELAY\""), "the measurement items are an enum");
    let protocol = crate::protocols::PROTOS[0].name();
    assert!(s.contains(&format!("\"{protocol}\"")), "the protocols are an enum");
}

#[test]
fn mcp_scope_state_answers_the_set_up_as_json() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    crate::ui::testkit::q(&mut scpi, &mut ui, &cv, ":TIM:SCAL 0.002;:CHAN2:DISP OFF");
    let r = call(&mut ui, &cv, &mut scpi, "scope_state", "{}");
    assert!(!failed(&r), "{r}");
    let d = data(&r);
    assert!((num(d.get("timebase").and_then(|t| t.get("scale"))) - 0.002).abs() < 1e-12, "{d}");
    let channels = items(d.get("channels"));
    assert_eq!(channels.len(), oos_hal::model::platform().caps.channels);
    assert_eq!(channels[1].get("display"), Some(&J::Bool(false)));
    assert!(matches!(d.get("trigger").and_then(|t| t.get("status")), Some(J::Str(_))), "{d}");
    // the text is the same JSON, for a client that reads only the text
    let text = items(r.get("content"))[0].get("text").and_then(J::str).unwrap();
    assert_eq!(parse(text).as_ref(), Some(d));
}

// Break it catches: an argument pasted into a command (`;` starts a second one, which a reading tool would then run),
// an unknown item sent to the interpreter instead of refused, 9.9E37 handed on as a value
#[test]
fn mcp_measure_answers_numbers_and_refuses_what_is_not_an_item() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    square_in_r1(&mut ui);
    let r = call(&mut ui, &cv, &mut scpi, "measure", r#"{"items":["VPP","vmax","FREQUENCY"],"source":"R1"}"#);
    assert!(!failed(&r), "{r}");
    let results = items(data(&r).get("results"));
    assert_eq!(results[1].get("item").and_then(J::str), Some("VMAX"), "the spelling of the list");
    assert!((num(results[0].get("value")) - 1.0).abs() < 0.02, "{r}");
    assert!((num(results[2].get("value")) - 25e6).abs() < 1e5, "{r}");
    let run = ui.m.run;
    for args in
        [r#"{"items":["VPP"],"source":"R1;:STOP"}"#, r#"{"items":["NOSUCH"]}"#, r#"{"items":[]}"#, r#"{"source":"R1"}"#]
    {
        let r = call(&mut ui, &cv, &mut scpi, "measure", args);
        assert!(failed(&r) && r.get("structuredContent").is_none(), "{args}: {r}");
    }
    assert_eq!(ui.m.run, run, "nothing of a refused call ran");
    assert!(scpi.queued_errors().is_empty(), "{:?}", scpi.queued_errors());
}

// Break it catches: the waveform selection of another client changed by the tool, the WORD codes turned into volts
// with the wrong scale, a long record answered whole, a start past the end answered as an empty success
#[test]
fn mcp_waveform_cuts_a_record_and_leaves_the_selection() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    square_in_r1(&mut ui);
    let q = |scpi: &mut crate::scpi::Scpi, ui: &mut Ui, c: &str| crate::ui::testkit::q(scpi, ui, &cv, c);
    q(&mut scpi, &mut ui, ":WAV:SOUR CHAN2;:WAV:FORM ASC;:WAV:MODE MAX;:WAV:STAR 7");
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","points":100}"#);
    assert!(!failed(&r), "{r}");
    let d = data(&r);
    assert_eq!(num(d.get("total_samples")), 4000.0);
    assert_eq!(num(d.get("samples_read")), 4000.0);
    assert_eq!(d.get("next_start"), Some(&J::Null));
    let stats = d.get("stats").unwrap();
    assert!((num(stats.get("max")) - 0.5).abs() < 0.01 && (num(stats.get("min")) + 0.5).abs() < 0.01, "{stats}");
    assert!(num(stats.get("mean")).abs() < 0.01, "{stats}");
    let pairs = items(d.get("minmax"));
    assert_eq!(pairs.len(), 100);
    // 40 samples a bucket: each holds both levels
    assert!(pairs.iter().all(|p| (num(items(Some(p)).get(1)) - num(items(Some(p)).first())) > 0.9), "{d}");
    assert!((num(d.get("dt")) - 1e-9).abs() < 1e-15);
    // fewer samples than points: the samples themselves
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","start":3991}"#);
    assert_eq!(items(data(&r).get("samples")).len(), 10, "{r}");
    // the other client's selection is as it was
    let sel = q(&mut scpi, &mut ui, ":WAV:SOUR?;:WAV:FORM?;:WAV:MODE?;:WAV:STAR?");
    assert_eq!(sel, "CHAN2;ASC;MAX;7");
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","start":5000}"#);
    assert!(failed(&r), "{r}");
    for args in [r#"{"points":1}"#, r#"{"points":10001}"#, r#"{"record":"all"}"#, r#"{"source":"R1 R2"}"#] {
        assert!(failed(&call(&mut ui, &cv, &mut scpi, "waveform", args)), "{args}");
    }
}

// Break it catches: a key the tool does not know dropped without a word (a typo of `scale` set nothing and answered
// success), commands sent in an order that fails (a level before its source), the read-back missing
#[test]
fn mcp_configure_sets_and_reads_back_and_refuses_unknown_keys() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let args = r#"{"timebase":{"scale":0.001},"channels":[{"channel":1,"scale":0.5,"offset":0.25,"coupling":"AC"}],"trigger":{"source":"CHAN1","level":0.1,"sweep":"NORMal"}}"#;
    let r = call(&mut ui, &cv, &mut scpi, "configure", args);
    assert!(!failed(&r), "{r}");
    let state = data(&r).get("state").unwrap();
    assert!((num(state.get("timebase").and_then(|t| t.get("scale"))) - 1e-3).abs() < 1e-12);
    let ch1 = &items(state.get("channels"))[0];
    assert!((num(ch1.get("scale")) - 0.5).abs() < 1e-9 && (num(ch1.get("offset")) - 0.25).abs() < 1e-9, "{ch1}");
    assert_eq!(ch1.get("coupling").and_then(J::str), Some("AC"));
    assert!((num(state.get("trigger").and_then(|t| t.get("level"))) - 0.1).abs() < 1e-6, "{state}");
    assert_eq!(items(data(&r).get("applied")).len(), 7);
    let before = ui.m.time_per_div();
    for args in [
        r#"{"timebase":{"scale":0.002,"scael":1}}"#,
        r#"{"timebase":{"scale":"fast"}}"#,
        r#"{"channels":[{"channel":9,"scale":1}]}"#,
        r#"{"display":{}}"#,
    ] {
        let r = call(&mut ui, &cv, &mut scpi, "configure", args);
        assert!(failed(&r), "{args}: {r}");
    }
    assert_eq!(ui.m.time_per_div(), before, "a refused call sets nothing");
    // a value the interpreter refuses: the error is in the answer, the rest is applied
    let r = call(&mut ui, &cv, &mut scpi, "configure", r#"{"channels":[{"channel":1,"coupling":"XX","scale":1}]}"#);
    assert!(failed(&r), "{r}");
    assert!(r.to_string().contains(":CHAN1:COUP XX"), "{r}");
    assert!((ui.m.ch[0].volts_per_div() - 1.0).abs() < 1e-6);
}

#[test]
fn mcp_acquire_errors_and_scpi_lists() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let r = call(&mut ui, &cv, &mut scpi, "acquire", r#"{"action":"stop"}"#);
    assert_eq!(data(&r).get("trigger_status").and_then(J::str), Some("STOP"), "{r}");
    assert!(failed(&call(&mut ui, &cv, &mut scpi, "acquire", r#"{"action":"explode"}"#)));
    // a list of messages stops at the first that fails; the errors tool empties the queue
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"commands":[":TIM:SCAL 0.005",":NOSUCH 1",":TIM:SCAL 0.01"]}"#);
    assert!(failed(&r), "{r}");
    assert_eq!(num(data(&r).get("stopped_at")), 1.0);
    assert!((ui.m.time_per_div() - 0.005).abs() < 1e-9, "the message after the failed one did not run");
    let r = call(&mut ui, &cv, &mut scpi, "errors", "{}");
    let queue = items(data(&r).get("errors_queued"));
    assert!(queue.len() == 1 && queue[0].to_string().contains("-113"), "{r}");
    assert!(scpi.queued_errors().is_empty());
    assert!(items(data(&call(&mut ui, &cv, &mut scpi, "errors", "{}")).get("errors_queued")).is_empty());
}

#[test]
fn mcp_decoder_set_up_and_its_rows() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let protocol = crate::protocols::PROTOS[0].name();
    let args = format!(r#"{{"slot":1,"protocol":"{}","state":false}}"#, protocol.to_lowercase());
    let r = call(&mut ui, &cv, &mut scpi, "setup_decoder", &args);
    assert!(!failed(&r), "{r}");
    let d = data(&r).get("decoder").unwrap();
    assert_eq!(d.get("protocol").and_then(J::str), Some(protocol), "{d}");
    assert_eq!(items(data(&r).get("applied")).first().and_then(J::str), Some(":DEC:SEL 1"));
    for args in [r#"{"protocol":"NOSUCH"}"#, r#"{"slot":0}"#, r#"{"slot":1.5}"#, r#"{"colour":"red"}"#] {
        assert!(failed(&call(&mut ui, &cv, &mut scpi, "setup_decoder", args)), "{args}");
    }
    let r = call(&mut ui, &cv, &mut scpi, "decode_results", r#"{"max_rows":5}"#);
    assert!(!failed(&r), "{r}");
    assert_eq!(data(&r).get("pending"), Some(&J::Bool(false)), "{r}");
    assert!(failed(&call(&mut ui, &cv, &mut scpi, "decode_results", r#"{"max_rows":0}"#)));
}

// Break it catches: a tool that changes the scope still offered or still run while MCP is read only, a setter hidden
// in a message of the `scpi` tool (after a `;` or a line break), MCP that answers while it is off
#[test]
fn mcp_read_only_and_off() {
    use crate::sysconf::McpMode;
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    ui.sys.remote_mcp = McpMode::Read;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let r = parse(&crate::site::mcp(body, &mut Ctx::new(&mut ui, &cv, &mut scpi)).unwrap()).unwrap();
    let tools = items(r.get("result").and_then(|x| x.get("tools")));
    let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(J::str)).collect();
    assert_eq!(names, ["scpi", "screenshot", "scope_state", "measure", "waveform", "decode_results", "errors"]);
    assert!(tools.iter().all(|t| t.get("annotations").and_then(|a| a.get("readOnlyHint")) == Some(&J::Bool(true))));
    let scale = ui.m.time_per_div();
    for (name, args) in [
        ("configure", r#"{"timebase":{"scale":0.5}}"#),
        ("acquire", r#"{"action":"stop"}"#),
        ("setup_decoder", r#"{"state":true}"#),
        ("scpi", r#"{"command":":TIM:SCAL 0.5"}"#),
        ("scpi", r#"{"command":":TIM:SCAL?;:TIM:SCAL 0.5"}"#),
        ("scpi", r#"{"command":":TIM:SCAL?\n:TIM:SCAL 0.5"}"#),
        ("scpi", r#"{"commands":[":TIM:SCAL?",":STOP"]}"#),
        ("scpi", r#"{"command":""}"#),
    ] {
        let r = call(&mut ui, &cv, &mut scpi, name, args);
        assert!(failed(&r) && r.to_string().contains("read only"), "{name} {args}: {r}");
    }
    assert_eq!(ui.m.time_per_div(), scale, "nothing was set");
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"command":":TIM:SCAL?;*IDN?"}"#);
    assert!(!failed(&r) && r.to_string().contains("OOS,"), "queries run: {r}");
    assert!(!failed(&call(&mut ui, &cv, &mut scpi, "scope_state", "{}")));
    ui.sys.remote_mcp = McpMode::Off;
    let body = r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{}}"#;
    let r = parse(&crate::site::mcp(body, &mut Ctx::new(&mut ui, &cv, &mut scpi)).unwrap()).unwrap();
    let e = r.get("error").unwrap_or_else(|| panic!("{r}"));
    assert_eq!((num(e.get("code")), r.get("id")), (-32001.0, Some(&J::Num(7.0))));
}
