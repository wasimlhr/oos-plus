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
    let reply = crate::site::mcp(&body, &mut Ctx::new(ui, cv, scpi)).unwrap().text();
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
    let r = parse(&crate::site::mcp(body, &mut Ctx::new(&mut ui, &cv, &mut scpi)).unwrap().text()).unwrap();
    let tools = items(r.get("result").and_then(|x| x.get("tools")));
    let read_only = |name: &str| {
        let t = tools.iter().find(|t| t.get("name").and_then(J::str) == Some(name)).expect(name);
        t.get("annotations").and_then(|a| a.get("readOnlyHint")) == Some(&J::Bool(true))
    };
    for name in
        ["screenshot", "scope_state", "measure", "waveform", "decode_results", "table", "errors", "scpi_reference"]
    {
        assert!(read_only(name), "{name} only reads");
    }
    for name in ["scpi", "configure", "acquire", "setup_decoder", "measure_slot"] {
        assert!(!read_only(name), "{name} can change the scope");
    }
    assert_eq!(tools.len(), 13);
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

// Break it catches: a value asked for the screen that is computed but not a row of the table (`measure` adds none), an
// empty slot read as a number or as null without a reason, a source pasted into the command, a pair item given one
// source, a slot outside 1..99 sent on
#[test]
fn mcp_measure_slot_adds_a_row_and_reads_it_back_by_number() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    square_in_r1(&mut ui);
    ui.m.meas.clear();
    ui.m.meas2.clear();
    let r = call(&mut ui, &cv, &mut scpi, "measure_slot", r#"{"slot":3}"#);
    assert!(failed(&r) && r.to_string().contains("MEAS3 is empty"), "{r}");
    let r = call(&mut ui, &cv, &mut scpi, "measure_slot", r#"{"slot":3,"item":"vpp","source":"R1"}"#);
    assert!(!failed(&r), "{r}");
    assert_eq!(data(&r).get("item").and_then(J::str), Some("VPP,R1"));
    assert!((num(data(&r).get("value")) - 1.0).abs() < 0.02, "{r}");
    assert_eq!(ui.m.meas.len(), 1, "a row of the table");
    let r = call(&mut ui, &cv, &mut scpi, "measure_slot", r#"{"slot":3}"#);
    assert!(!failed(&r) && (num(data(&r).get("value")) - 1.0).abs() < 0.02, "{r}");
    for args in [
        r#"{"slot":0}"#,
        r#"{"slot":100,"item":"VPP"}"#,
        r#"{"slot":2,"item":"NOSUCH"}"#,
        r#"{"slot":2,"item":"VPP","source":"R1;:STOP"}"#,
        r#"{"slot":2,"item":"VPP","source":"R1","source2":"R2"}"#,
        r#"{"slot":2,"item":"VPP","clear":true}"#,
        r#"{"item":"VPP"}"#,
    ] {
        let r = call(&mut ui, &cv, &mut scpi, "measure_slot", args);
        assert!(failed(&r), "{args}: {r}");
    }
    assert_eq!(ui.m.meas.len(), 1, "nothing of a refused call ran");
    let r = call(&mut ui, &cv, &mut scpi, "measure_slot", r#"{"slot":3,"clear":true}"#);
    assert!(!failed(&r) && ui.m.meas.is_empty(), "{r}");
    ui.sys.remote_mcp = crate::sysconf::McpMode::Read;
    let r = call(&mut ui, &cv, &mut scpi, "measure_slot", r#"{"slot":3,"item":"VPP"}"#);
    assert!(failed(&r) && r.to_string().contains("read only") && ui.m.meas.is_empty(), "{r}");
}

// Break it catches: the waveform selection of another client changed by the tool, the WORD codes turned into volts
// with the wrong scale, a long record answered whole (the samples in the answer, where they fill a model's context),
// the file at the link not the record of the answer, a start past the end answered as an empty success
#[test]
fn mcp_waveform_cuts_a_record_and_leaves_the_selection() {
    let _sends = sends();
    let mut h = Web::open(0);
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let r = get(&mut h, &mut ui, &cv, &mut scpi, "/mcp/waveform/r1-1.csv");
    assert!(r.starts_with("HTTP/1.1 409") && r.contains("call the MCP tool waveform"), "{r}");
    square_in_r1(&mut ui);
    let q = |scpi: &mut crate::scpi::Scpi, ui: &mut Ui, c: &str| crate::ui::testkit::q(scpi, ui, &cv, c);
    q(&mut scpi, &mut ui, ":WAV:SOUR CHAN2;:WAV:FORM ASC;:WAV:MODE MAX;:WAV:STAR 7");
    // by default no samples in the answer: the statistics and the link
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory"}"#);
    let d = data(&r);
    assert!(d.get("samples").is_none() && d.get("minmax").is_none(), "{d}");
    let file = d.get("file").unwrap();
    assert_eq!(file.get("url").and_then(J::str), Some("/mcp/waveform/r1-1.csv"), "no address: the path alone");
    assert_eq!((num(file.get("rows")), file.get("columns").and_then(J::str)), (4000.0, Some("time_s,v")));
    let csv = get(&mut h, &mut ui, &cv, &mut scpi, "/mcp/waveform/r1-1.csv");
    assert!(csv.starts_with("HTTP/1.1 200") && csv.contains("text/csv"), "{}", &csv[..200]);
    let body = |csv: &str| csv.split("\r\n\r\n").nth(1).unwrap().to_string();
    let csv = body(&csv);
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!((rows.len(), rows[0]), (4001, "time_s,v"));
    let row = |i: usize| -> Vec<f64> { rows[i].split(',').map(|x| x.parse().unwrap()).collect() };
    assert!((row(2)[0] - row(1)[0] - 1e-9).abs() < 1e-15 && (row(1)[1] + 0.5).abs() < 0.01, "{:?}", &rows[..3]);
    assert_eq!(num(file.get("bytes")), csv.len() as f64);
    // fewer rows than samples: min/max per bucket in the file
    call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","points":100}"#);
    let csv = body(&get(&mut h, &mut ui, &cv, &mut scpi, "/mcp/waveform/r1-2.csv"));
    let rows: Vec<&str> = csv.lines().collect();
    assert_eq!((rows.len(), rows[0]), (101, "time_s,min_v,max_v"));
    ui.net.addr = "192.168.1.9".into();
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","points":100,"inline":100}"#);
    assert_eq!(
        data(&r).get("file").and_then(|f| f.get("url")).and_then(J::str),
        Some("http://192.168.1.9:8080/mcp/waveform/r1-3.csv")
    );
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
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","start":3991,"inline":20}"#);
    assert_eq!(items(data(&r).get("samples")).len(), 10, "{r}");
    // the other client's selection is as it was
    let sel = q(&mut scpi, &mut ui, ":WAV:SOUR?;:WAV:FORM?;:WAV:MODE?;:WAV:STAR?");
    assert_eq!(sel, "CHAN2;ASC;MAX;7");
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","start":5000}"#);
    assert!(failed(&r), "{r}");
    for args in [r#"{"points":1}"#, r#"{"points":100001}"#, r#"{"inline":1001}"#, r#"{"source":"R1 R2"}"#] {
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
    // the decode table says where it is on the screen, and its rows begin with the Index
    assert!(data(&r).get("table_hint").and_then(J::str).is_some_and(|h| h.contains(":DISPlay:LOWer EVENTS")), "{r}");
    assert_eq!(data(&r).get("columns").map(|c| items(Some(c))[0].str()), Some(Some("Index")), "{r}");
}

// Break it catches: a table that cannot be listed or read by name through MCP, a row without its Index
#[test]
fn mcp_reads_every_table_by_name() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let r = call(&mut ui, &cv, &mut scpi, "table", "{}");
    assert!(!failed(&r), "{r}");
    let names: Vec<&str> =
        items(data(&r).get("tables")).iter().filter_map(|t| t.get("name").and_then(J::str)).collect();
    assert_eq!(names, ["EVENTS", "FIELDS", "SEARCH", "SEGMENTS", "PEAKS", "MEASURE"]);
    for name in names {
        let r = call(&mut ui, &cv, &mut scpi, "table", &format!(r#"{{"name":"{name}"}}"#));
        assert!(!failed(&r), "{name}: {r}");
        assert_eq!(data(&r).get("columns").map(|c| items(Some(c))[0].str()), Some(Some("Index")), "{name}: {r}");
    }
    assert!(failed(&call(&mut ui, &cv, &mut scpi, "table", r#"{"name":"NOSUCH"}"#)));
}

// Break it catches: a tool that changes the scope still offered or still run while MCP is read only, a setter hidden
// in a message of the `scpi` tool (after a `;` or a line break), MCP that answers while it is off
#[test]
fn mcp_read_only_and_off() {
    use crate::sysconf::McpMode;
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    ui.sys.remote_mcp = McpMode::Read;
    let body = r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
    let r = parse(&crate::site::mcp(body, &mut Ctx::new(&mut ui, &cv, &mut scpi)).unwrap().text()).unwrap();
    let tools = items(r.get("result").and_then(|x| x.get("tools")));
    let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(J::str)).collect();
    assert_eq!(names, ["scpi", "screenshot", "scope_state", "measure", "waveform", "decode_results", "table", "errors", "scpi_reference"]);
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
    // the waveform tool reads, and its file is there; while off the file is refused too
    square_in_r1(&mut ui);
    assert!(!failed(&call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory"}"#)));
    assert!(crate::site::waveform_csv(&ui, &scpi, "r1-1.csv").is_ok());
    ui.sys.remote_mcp = McpMode::Off;
    assert!(crate::site::waveform_csv(&ui, &scpi, "r1-1.csv").unwrap_err().contains("off"));
    let body = r#"{"jsonrpc":"2.0","id":7,"method":"initialize","params":{}}"#;
    let r = parse(&crate::site::mcp(body, &mut Ctx::new(&mut ui, &cv, &mut scpi)).unwrap().text()).unwrap();
    let e = r.get("error").unwrap_or_else(|| panic!("{r}"));
    assert_eq!((num(e.get("code")), r.get("id")), (-32001.0, Some(&J::Num(7.0))));
}

// Break it catches: a guessed command answered with a bare error and no way to the right one (an assistant then
// guesses again), the reference that does not find a command by what it does or by either form of its header
#[test]
fn mcp_scpi_reference_finds_a_command_instead_of_a_guess() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"command":":CHAN1:VDIV 0.5"}"#);
    assert!(failed(&r) && r.to_string().contains("-113") && r.to_string().contains("scpi_reference"), "{r}");
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"commands":[":CHAN1:VDIV?"]}"#);
    assert!(data(&r).get("hint").and_then(J::str).is_some_and(|h| h.contains("scpi_reference")), "{r}");
    let r = call(&mut ui, &cv, &mut scpi, "scpi_reference", r#"{"search":"channel scale"}"#);
    let found = r.to_string();
    assert!(!failed(&r) && found.contains(":CHANnel<n>:SCALe"), "{found}");
    let r = call(&mut ui, &cv, &mut scpi, "scpi_reference", r#"{"header":":chan1:scal?"}"#);
    let d = data(&r);
    assert_eq!(num(d.get("found")), 1.0, "{d}");
    let c = &items(d.get("commands"))[0];
    assert!(c.get("params").is_some() && c.get("query").and_then(J::str) == Some(":CHANnel<n>:SCALe?"), "{c}");
    let r = call(&mut ui, &cv, &mut scpi, "scpi_reference", r#"{"header":":CHAN1:VDIV"}"#);
    assert_eq!(num(data(&r).get("found")), 0.0);
    let r = call(&mut ui, &cv, &mut scpi, "scpi_reference", "{}");
    assert!(items(data(&r).get("subsystems")).len() > 50 && items(data(&r).get("appendices")).len() > 4, "{r}");
    assert!(failed(&call(&mut ui, &cv, &mut scpi, "scpi_reference", r#"{"search":""}"#)));
    assert!(failed(&call(&mut ui, &cv, &mut scpi, "scpi_reference", r#"{"max_results":51}"#)));
    assert!(failed(&call(&mut ui, &cv, &mut scpi, "scpi_reference", r#"{"search":"scale","offset":-1}"#)));
}

// Break it catches: a search in an assistant's own words answered with nothing, a measurement item that only the
// manual's appendix names not found, a long list cut at 50 with no way on, an appendix or a subsystem's
// introduction and examples that only the PDF has
#[test]
fn mcp_scpi_reference_answers_own_words_items_pages_and_appendices() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let mut ask = |args: &str| call(&mut ui, &cv, &mut scpi, "scpi_reference", args);
    let first = |r: &J| items(data(r).get("commands"))[0].get("header").and_then(J::str).map(str::to_string);
    assert_eq!(first(&ask(r#"{"search":"volts per division"}"#)).as_deref(), Some(":CHANnel<n>:SCALe"));
    let r = ask(r#"{"search":"peak to peak"}"#);
    let found = &items(data(&r).get("items"))[0];
    assert_eq!(found.get("appendix").and_then(J::str), Some("Standard Measurement Items"), "{r}");
    assert!(found.to_string().contains("`VPP`") && found.to_string().contains(":MEASure:ITEM"), "{found}");
    let r = ask(r#"{"search":"xyzzy"}"#);
    let d = data(&r);
    assert!(!failed(&r) && num(d.get("total")) == 0.0 && d.get("hint").is_some() && d.get("items").is_none(), "{r}");
    // A list goes on from `next_offset`, and its last page names none
    let r = ask(r#"{"subsystem":"timebase","max_results":3}"#);
    let d = data(&r);
    assert_eq!((items(d.get("commands")).len(), num(d.get("next_offset"))), (3, 3.0), "{r}");
    assert!(d.get("about").and_then(J::str).is_some_and(|a| a.contains("main time base")), "{r}");
    assert!(d.get("examples").and_then(J::str).is_some_and(|e| e.contains(":TIMebase:")), "{r}");
    let (total, start) = (num(d.get("total")) as usize, first(&r));
    let r = ask(&format!(r#"{{"subsystem":"timebase","max_results":50,"offset":3}}"#));
    let d = data(&r);
    assert!(items(d.get("commands")).len() == total - 3 && d.get("next_offset").is_none() && first(&r) != start, "{r}");
    // Several subsystems hold "trigger": their commands, but no one introduction
    assert!(data(&ask(r#"{"subsystem":"trigger"}"#)).get("about").is_none());
    let r = ask(r#"{"appendix":"error","max_results":5}"#);
    let d = data(&r);
    assert_eq!(d.get("appendix").and_then(J::str), Some("Error Codes"), "{r}");
    assert!(items(d.get("rows")).len() == 5 && num(d.get("next_offset")) == 5.0 && d.get("about").is_some(), "{r}");
    assert!(items(d.get("rows"))[1].get("Code").and_then(J::str) == Some("-113"), "{r}");
    let r = ask(r#"{"appendix":"items"}"#);
    let d = data(&r);
    assert!(d.get("hint").is_some() && items(d.get("appendices")).len() > 4 && d.get("rows").is_none(), "{d}");
}

// Break it catches: the rules of `initialize` grown into a text a small model skips, or naming a tool that was
// renamed or removed (the model then calls a tool that does not exist)
#[test]
fn mcp_initialize_gives_short_rules_that_name_only_real_tools() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let ask = |ui: &mut Ui, scpi: &mut crate::scpi::Scpi, body: &str| {
        parse(&crate::site::mcp(body, &mut Ctx::new(ui, &cv, scpi)).unwrap().text()).unwrap()
    };
    let init = ask(&mut ui, &mut scpi, r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#);
    let rules = init.get("result").and_then(|r| r.get("instructions")).and_then(J::str).unwrap().to_string();
    assert!(rules.len() < 1300, "{} characters", rules.len());
    assert!(rules.contains("scpi_reference with no
arguments") || rules.contains("scpi_reference with no "), "the rules point to the task list");
    assert!(rules.lines().filter(|l| l.starts_with(|c: char| c.is_ascii_digit())).count() >= 5, "{rules}");
    let list = ask(&mut ui, &mut scpi, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
    let tools = items(list.get("result").and_then(|r| r.get("tools")));
    let names: Vec<&str> = tools.iter().filter_map(|t| t.get("name").and_then(J::str)).collect();
    let words = rules.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
    for word in words.filter(|w| w.contains('_') || ["configure", "acquire", "measure", "waveform", "scpi"].contains(w)) {
        assert!(names.contains(&word), "the rules name `{word}`, which is no tool");
    }
}

// Break it catches: a setting answered "ok" with nothing that shows what the scope took (an assistant takes sent for
// done), a query or a common command read back, the read back missing from a list of messages
#[test]
fn mcp_scpi_reads_back_what_it_set() {
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    let text = |r: &J| r.get("content").map(|c| items(Some(c))[0].get("text").and_then(J::str).unwrap().to_string());
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"command":":CHAN1:SCAL 0.5;:TIM:SCAL 0.001"}"#);
    let t = text(&r).unwrap();
    assert!(t.starts_with("ok\nread back :CHAN1:SCAL? -> ") && t.contains("read back :TIM:SCAL? -> "), "{t}");
    let value: f64 = t.lines().nth(1).unwrap().rsplit(' ').next().unwrap().parse().unwrap();
    assert!((value - 0.5).abs() < 1e-9, "{t}");
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"command":":CHAN1:SCAL?;*CLS"}"#);
    assert!(!text(&r).unwrap().contains("read back"), "{r}");
    let r = call(&mut ui, &cv, &mut scpi, "scpi", r#"{"commands":[":CHAN2:DISP ON",":CHAN2:DISP?"]}"#);
    let ran = items(data(&r).get("ran"));
    let back = items(ran[0].get("read_back"));
    assert_eq!(back[0].get("query").and_then(J::str), Some(":CHAN2:DISP?"), "{r}");
    assert!(ran[1].get("read_back").is_none(), "{r}");
    assert!(scpi.queued_errors().is_empty(), "{:?}", scpi.queued_errors());
}

// Break it catches: the file of a waveform call kept for good (it is a temporary file for one download), the time it
// is kept not said in the answer
#[test]
fn mcp_waveform_file_is_deleted_after_its_time() {
    let _sends = sends();
    let mut h = Web::open(0);
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    square_in_r1(&mut ui);
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory"}"#);
    let file = data(&r).get("file").unwrap();
    assert_eq!(num(file.get("expires_in_s")), crate::site::mcp::FILE_TTL.as_secs() as f64);
    assert!(get(&mut h, &mut ui, &cv, &mut scpi, "/mcp/waveform/r1-1.csv").starts_with("HTTP/1.1 200"));
    // as if the call had been made longer ago than the file is kept
    let past = std::time::Instant::now().checked_sub(crate::site::mcp::FILE_TTL + Duration::from_secs(1));
    let Some(past) = past else { return };
    scpi.mcp_files[0].1 = past;
    let r = get(&mut h, &mut ui, &cv, &mut scpi, "/mcp/waveform/r1-1.csv");
    assert!(r.starts_with("HTTP/1.1 409") && r.contains("within 2 minutes"), "{r}");
    assert!(scpi.mcp_files.is_empty(), "the web server's pass deleted it");
}

// Break it catches: one file for every call, so that the file of the first of two calls (CH1, then CH2) is the second
// call's record when it is downloaded after both; files kept without bound
#[test]
fn mcp_waveform_gives_each_call_its_own_file() {
    let _sends = sends();
    let mut h = Web::open(0);
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    square_in_r1(&mut ui);
    let url = |r: &J| data(r).get("file").and_then(|f| f.get("url")).and_then(J::str).unwrap().to_string();
    let first = url(&call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","points":10}"#));
    let second = url(&call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","points":20}"#));
    assert_ne!(first, second);
    let rows = |r: String| r.split("\r\n\r\n").nth(1).unwrap().lines().count();
    // both downloaded after both calls: each is its own call's record
    assert_eq!(rows(get(&mut h, &mut ui, &cv, &mut scpi, &first)), 11);
    assert_eq!(rows(get(&mut h, &mut ui, &cv, &mut scpi, &second)), 21);
    // a name no call gave
    let r = get(&mut h, &mut ui, &cv, &mut scpi, "/mcp/waveform/ch1-99.csv");
    assert!(r.starts_with("HTTP/1.1 409") && r.contains("ch1-99.csv"), "{r}");
    // FILES_MAX calls later the first is gone, the last FILES_MAX are kept
    for _ in 0..crate::site::mcp::FILES_MAX - 1 {
        call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"R1","record":"memory","points":10}"#);
    }
    assert_eq!(scpi.mcp_files.len(), crate::site::mcp::FILES_MAX);
    assert!(get(&mut h, &mut ui, &cv, &mut scpi, &first).starts_with("HTTP/1.1 409"));
    assert!(get(&mut h, &mut ui, &cv, &mut scpi, &second).starts_with("HTTP/1.1 200"));
}

// Break it catches: the stop the tool asks (u64::MAX) answered as the length of a record that is not there, a channel
// that is off answered without the reason
#[test]
fn mcp_waveform_of_a_channel_that_is_off_has_no_samples() {
    let _sends = sends();
    let (mut scpi, mut ui, cv) = crate::ui::testkit::scpi_session();
    crate::ui::testkit::q(&mut scpi, &mut ui, &cv, ":CHAN2:DISP 0");
    let r = call(&mut ui, &cv, &mut scpi, "waveform", r#"{"source":"CHAN2"}"#);
    assert!(failed(&r) && r.to_string().contains("CHAN2 is off"), "{r}");
    assert_eq!(num(data(&r).get("total_samples")), 0.0, "{r}");
}
