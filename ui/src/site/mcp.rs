// Copyright (C) 2026 dekajoule
// SPDX-License-Identifier: GPL-2.0-or-later

//! MCP of the web remote (`POST /mcp`, docs/http.md): JSON-RPC 2.0 with `initialize`, `ping`, `tools/list` and
//! `tools/call`. The tools `scpi` and `screenshot`, and the typed tools beside them (ADR 0074): each typed tool is a
//! fixed sequence of commands for the one interpreter (`Scpi::exec_message_checked`, ADR 0022). It checks its arguments
//! before it builds a command (a word that could end one, `;`, never reaches the interpreter), answers JSON, as
//! `structuredContent` and as its text, and names the errors each command queued. A new tool gets its line in
//! `tools/list`, its arm in `tools/call`, its entry in docs/http.md and its test in site/tests.rs;
//! `tools/http-coverage.py` names what is missing.

use super::Ctx;
use crate::gfx::Canvas;
use crate::json::{parse, J};
use crate::scpi::Scpi;
use crate::sysconf::McpMode;
use crate::ui::Ui;

/// Points `waveform` answers by default, and at most.
const POINTS_DEFAULT: usize = 1000;
const POINTS_MAX: usize = 10_000;
/// Samples of a record `waveform` reads in one call (2 MiB as WORD); the answer says where the next call starts.
const READ_MAX: u64 = 1 << 20;
/// Rows `decode_results` answers by default, and at most.
const ROWS_DEFAULT: usize = 200;
const ROWS_MAX: usize = 2000;
/// Items one `measure` call asks.
const ITEMS_MAX: usize = 20;
/// Messages one `scpi` call with `commands` runs.
const MESSAGES_MAX: usize = 64;
/// The interpreter answers 9.9E37 for a value it does not have (scpi.md, Measurements): above this it is none.
const NO_VALUE: f64 = 1e37;

/// One MCP JSON-RPC message; None for notifications (answered with 202 and no body).
pub fn mcp(body: &str, cx: &mut Ctx) -> Option<String> {
    let (ui, cv, scpi) = (&mut *cx.ui, cx.cv, &mut *cx.scpi);
    let err = |id: J, code: f64, msg: &str| {
        J::obj(&[
            ("jsonrpc", J::s("2.0")),
            ("id", id),
            ("error", J::obj(&[("code", J::Num(code)), ("message", J::s(msg))])),
        ])
        .to_string()
    };
    let Some(req) = parse(body) else { return Some(err(J::Null, -32700.0, "parse error")) };
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(J::str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(J::Null);
    let args = params.get("arguments").cloned().unwrap_or(J::Obj(Vec::new()));
    // Remote services > MCP (`remote.mcp`): off answers every request with an error; read only offers the tools that
    // only read, and `scpi` for queries alone
    let read = match ui.sys.remote_mcp {
        McpMode::Off => return Some(err(id, -32001.0, OFF)),
        McpMode::Read => true,
        McpMode::Full => false,
    };
    let mut run = Run { ui, cv, scpi, errors: Vec::new() };
    let result = match method {
        "initialize" => {
            let ver = params.get("protocolVersion").and_then(J::str).unwrap_or("2025-06-18").to_string();
            J::obj(&[
                ("protocolVersion", J::Str(ver)),
                ("capabilities", J::obj(&[("tools", J::obj(&[]))])),
                ("serverInfo", J::obj(&[("name", J::s("oos-ui")), ("version", J::s(env!("CARGO_PKG_VERSION")))])),
                ("instructions", J::s(INSTRUCTIONS)),
            ])
        }
        "ping" => J::obj(&[]),
        "tools/list" => J::obj(&[(
            "tools",
            J::Arr(only_reads(read, vec![
                if read {
                    tool(("name", J::s("scpi")), SCPI_QUERIES, scpi_schema(), READS)
                } else {
                    tool(("name", J::s("scpi")), SCPI, scpi_schema(), WRITES)
                },
                tool(("name", J::s("screenshot")), SCREENSHOT, schema(&[], &[]), READS),
                tool(("name", J::s("scope_state")), SCOPE_STATE, schema(&[], &[]), READS),
                tool(("name", J::s("measure")), MEASURE, measure_schema(), READS),
                tool(("name", J::s("waveform")), WAVEFORM, waveform_schema(), READS),
                tool(("name", J::s("decode_results")), DECODE_RESULTS, decode_results_schema(), READS),
                tool(("name", J::s("errors")), ERRORS, schema(&[], &[]), READS),
                tool(("name", J::s("configure")), CONFIGURE, configure_schema(), WRITES),
                tool(("name", J::s("acquire")), ACQUIRE, acquire_schema(), WRITES),
                tool(("name", J::s("setup_decoder")), SETUP_DECODER, setup_decoder_schema(), WRITES),
            ])),
        )]),
        "tools/call" => match params.get("name").and_then(J::str) {
            name if read && !reads_only(name, &args) => refuse(READ_ONLY),
            Some("scpi") => scpi_tool(&mut run, &args),
            Some("screenshot") => J::obj(&[(
                "content",
                J::Arr(vec![J::obj(&[
                    ("type", J::s("image")),
                    ("mimeType", J::s("image/png")),
                    ("data", J::Str(crate::base64::encode(&crate::gfx::png::png_of(cv)))),
                ])]),
            )]),
            Some("scope_state") => {
                let state = scope_state(&mut run);
                answer(&mut run, state)
            }
            Some("measure") => measure(&mut run, &args),
            Some("waveform") => waveform(&mut run, &args),
            Some("decode_results") => decode_results(&mut run, &args),
            Some("errors") => errors(&mut run),
            Some("configure") => configure(&mut run, &args),
            Some("acquire") => acquire(&mut run, &args),
            Some("setup_decoder") => setup_decoder(&mut run, &args),
            _ => return Some(err(id, -32602.0, "unknown tool")),
        },
        _ => return Some(err(id, -32601.0, "method not found")),
    };
    Some(J::obj(&[("jsonrpc", J::s("2.0")), ("id", id), ("result", result)]).to_string())
}

const INSTRUCTIONS: &str = "Rigol DHO800/DHO900 running OOS. Start with scope_state to see the set-up, configure and \
    acquire to change it, measure and waveform to read the signal, decode_results for protocol decoders, screenshot to \
    see the screen. The scpi tool takes any command of OOS's SCPI reference (Rigol-style: :TIM:SCAL 1e-3, \
    :CHAN1:SCAL 0.5, :MEAS:ITEM? VPP,CHAN1). Values are in base units (V, s, Hz).";

const SCPI: &str = "Run SCPI on the oscilloscope: `command`, one program message (commands separated by ';'), answers \
    the response of its queries; or `commands`, a list of messages run in order that stops at the first error. The \
    commands are those of OOS's SCPI reference (Rigol-style: :TIM:SCAL 1e-3, :CHAN1:SCAL 0.5, :MEAS:ITEM? VPP,CHAN1).";
const SCPI_QUERIES: &str = "Run SCPI queries on the oscilloscope (MCP is read only on this scope): `command`, one \
    program message whose every command is a query (its header ends in '?', commands separated by ';'), or \
    `commands`, a list of such messages. The queries are those of OOS's SCPI reference (:TIM:SCAL?, \
    :MEAS:ITEM? VPP,CHAN1).";
const OFF: &str = "MCP is off on this scope (System > Network > Remote services > MCP)";
const READ_ONLY: &str = "MCP is read only on this scope (System > Network > Remote services > MCP): this tool, or \
    a command that is not a query, is refused";
const SCREENSHOT: &str ="The current oscilloscope screen as a PNG image.";
const SCOPE_STATE: &str = "The whole set-up in one answer: every channel (display, scale V/div, offset V, coupling, \
    probe, bandwidth limit), the timebase (s/div, delay s), the acquisition (sample rate, memory depth, mode) and the \
    trigger (type, status STOP|TD|AUTO|WAIT, sweep, source, level, slope, coupling). Ask it after `acquire` to see \
    whether a single shot stopped.";
const MEASURE: &str = "Measure items of the measurement list on a source, as numbers in base units (V, s, Hz, %, \
    degrees); null where the scope has no value (no edge on screen, still computing). Pair items (delay, phase, skew, \
    setup, hold) take `source` and `source2`. Does not add rows to the screen's measurement strip.";
const WAVEFORM: &str = "A record of a source cut to a number of points: statistics (min, max, mean, rms, peak-to-peak \
    in volts) and the samples, or min/max pairs per bucket when the record is longer than `points`. `record` screen \
    reads what is drawn, memory the acquisition memory, 1 Mi samples per call from `start` (the answer gives \
    `next_start`). The waveform selection of other SCPI clients is left as it was.";
const DECODE_RESULTS: &str = "The events of the decoders that are on, as rows (time s, decoder, text, ok), \
    `max_rows` from `offset`. `pending` while the decoders are still computing: ask again.";
const ERRORS: &str = "Empty the SCPI error queue and answer its entries, oldest first.";
const CONFIGURE: &str = "Set channels, timebase, trigger and acquisition from one object; every part is optional. \
    Answers the commands that were applied, the errors, and the set-up read back as scope_state answers it (a scale \
    is set to the nearest one the scope has).";
const ACQUIRE: &str = "Run, stop, single, force a trigger, autoset or clear. Answers at once with the trigger status; \
    it does not wait for a trigger (ask scope_state until trigger.status is STOP after a single).";
const SETUP_DECODER: &str = "Set up the selected decoder slot, or `slot`: protocol, input, sources, threshold, on or \
    off. Answers the commands that were applied, the errors, and the slot read back.";

const TRIGGER_TYPES: &[&str] = &[
    "EDGE",
    "PULSE",
    "SLOPE",
    "VIDEO",
    "PATTERN",
    "DURATION",
    "TIMEOUT",
    "RUNT",
    "WINDOW",
    "DELAY",
    "SETUPHOLD",
    "NTHEDGE",
    "BUS",
];

/// The annotation `readOnlyHint` of a tool that only reads, and of one that can change the scope (MCP 2025-06-18).
const READS: bool = true;
const WRITES: bool = false;

/// A line of `tools/list`. The name comes as the pair `("name", J::s(..))`, as `tools/http-coverage.py` reads it.
fn tool(name: (&str, J), description: &str, input: J, read_only: bool) -> J {
    J::obj(&[
        name,
        ("description", J::s(description)),
        ("inputSchema", input),
        (
            "annotations",
            J::obj(&[
                ("readOnlyHint", J::Bool(read_only)),
                ("destructiveHint", J::Bool(false)),
                ("openWorldHint", J::Bool(false)),
            ]),
        ),
    ])
}

/// The tools of `tools/list`, without those that can change the scope while MCP is read only.
fn only_reads(read: bool, mut tools: Vec<J>) -> Vec<J> {
    if read {
        tools.retain(|t| t.get("annotations").and_then(|a| a.get("readOnlyHint")) == Some(&J::Bool(true)));
    }
    tools
}

/// While MCP is read only: whether a call of `name` only reads. `scpi` does when every command of every message is a
/// query, its header ending in `?` (a line break, which could start a command of its own, is refused).
fn reads_only(name: Option<&str>, args: &J) -> bool {
    let query = |message: &str| {
        !message.contains(['\n', '\r'])
            && message.split(';').all(|c| c.split_whitespace().next().is_some_and(|h| h.ends_with('?')))
    };
    match name {
        Some("configure" | "acquire" | "setup_decoder") => false,
        Some("scpi") => match (args.get("command"), args.get("commands")) {
            (Some(J::Str(m)), None) => query(m),
            (None, Some(J::Arr(ms))) => ms.iter().all(|m| m.str().is_some_and(query)),
            _ => false,
        },
        _ => true,
    }
}

fn schema(props: &[(&str, J)], required: &[&str]) -> J {
    J::obj(&[
        ("type", J::s("object")),
        ("properties", J::obj(props)),
        ("required", J::Arr(required.iter().map(|r| J::s(r)).collect())),
        ("additionalProperties", J::Bool(false)),
    ])
}

fn typed(t: &str, description: &str) -> J {
    J::obj(&[("type", J::s(t)), ("description", J::s(description))])
}

fn string(description: &str) -> J {
    typed("string", description)
}

fn number(description: &str) -> J {
    typed("number", description)
}

fn integer(description: &str) -> J {
    typed("integer", description)
}

fn boolean() -> J {
    J::obj(&[("type", J::s("boolean"))])
}

fn list(items: J, description: &str) -> J {
    J::obj(&[("type", J::s("array")), ("items", items), ("description", J::s(description))])
}

fn choice(words: &[&str], description: &str) -> J {
    J::obj(&[
        ("type", J::s("string")),
        ("enum", J::Arr(words.iter().map(|w| J::s(w)).collect())),
        ("description", J::s(description)),
    ])
}

fn scpi_schema() -> J {
    schema(
        &[
            ("command", string("One SCPI program message.")),
            (
                "commands",
                list(text_item(), "SCPI program messages, one after the other; stops at the first that fails."),
            ),
        ],
        &[],
    )
}

fn text_item() -> J {
    J::obj(&[("type", J::s("string"))])
}

fn measure_schema() -> J {
    let items: Vec<&str> = crate::scpi::measurement_items().into_iter().map(|(t, _)| t).collect();
    let mut items_list = list(choice(&items, "A measurement item."), "The items to measure.");
    if let J::Obj(fields) = &mut items_list {
        fields.push(("minItems".into(), J::Num(1.0)));
        fields.push(("maxItems".into(), J::Num(ITEMS_MAX as f64)));
    }
    schema(
        &[
            ("items", items_list),
            ("source", string("CHAN1..CHAN4 or R1..R10; default: the source of the Measure menu.")),
            ("source2", string("The second source of a pair item.")),
        ],
        &["items"],
    )
}

fn waveform_schema() -> J {
    schema(
        &[
            ("source", string("CHAN1..CHAN4, R1..R10 or MATH1..MATH4; default CHAN1.")),
            ("record", choice(&["screen", "memory"], "What is drawn, or the acquisition memory; default screen.")),
            ("points", integer("Points (or min/max pairs) in the answer, 2 to 10000; default 1000.")),
            ("start", integer("First sample of the record to read, from 1; default 1.")),
        ],
        &[],
    )
}

fn decode_results_schema() -> J {
    schema(
        &[
            ("offset", integer("First row, from 0; default 0.")),
            ("max_rows", integer("Rows in the answer, 1 to 2000; default 200.")),
        ],
        &[],
    )
}

fn configure_schema() -> J {
    let channel = schema(
        &[
            ("channel", integer("1 to the number of inputs of the model.")),
            ("display", boolean()),
            ("scale", number("V/div")),
            ("offset", number("V")),
            ("coupling", choice(&["DC", "AC", "GND", "DC50"], "DC50 only where the board has it.")),
            ("probe", number("Attenuation, e.g. 10.")),
            ("bwlimit", string("OFF, 20M, AUTO or a filter name.")),
            ("invert", boolean()),
        ],
        &["channel"],
    );
    schema(
        &[
            ("channels", list(channel, "Channels to set.")),
            ("timebase", schema(&[("scale", number("s/div")), ("offset", number("Delay in s."))], &[])),
            (
                "trigger",
                schema(
                    &[
                        ("type", choice(TRIGGER_TYPES, "The trigger type.")),
                        ("source", string("CHAN1..CHAN4, R1..R10, D0..D15.")),
                        ("level", number("V")),
                        ("slope", choice(&["POSitive", "NEGative", "RFALl"], "The edge.")),
                        ("sweep", choice(&["AUTO", "NORMal", "SINGle"], "Sweep.")),
                        ("coupling", choice(&["DC", "AC", "LFReject", "HFReject"], "Trigger coupling.")),
                        ("holdoff", number("s, 0 to 10.")),
                    ],
                    &[],
                ),
            ),
            (
                "acquire",
                schema(
                    &[
                        ("memory_depth", string("AUTO or a depth the scope has, e.g. 1M.")),
                        ("type", choice(&["NORMal", "PEAK", "AVERages", "HRESolution"], "Acquisition mode.")),
                        ("averages", integer("2 to 1024, for AVERages.")),
                    ],
                    &[],
                ),
            ),
        ],
        &[],
    )
}

fn acquire_schema() -> J {
    schema(&[("action", choice(&["run", "stop", "single", "force", "autoset", "clear"], "What to do."))], &["action"])
}

fn setup_decoder_schema() -> J {
    let protocols: Vec<&str> = crate::protocols::PROTOS.iter().map(|p| p.name()).collect();
    schema(
        &[
            ("slot", integer("Decoder slot, from 1.")),
            ("protocol", choice(&protocols, "The protocol.")),
            ("input", choice(&["WAVEFORM", "LA"], "Analog sources or logic lines.")),
            ("source", string("Data: CHANn, Rn or Dn.")),
            ("source2", string("Clock or second line.")),
            ("cs", string("Chip select, or NONE.")),
            ("source4", string("Fourth role, or NONE.")),
            ("threshold", number("V")),
            ("state", boolean()),
        ],
        &[],
    )
}

/// One tool call's way to the interpreter: each command alone, its answer, and the errors it queued.
struct Run<'a> {
    ui: &'a mut Ui,
    cv: &'a Canvas,
    scpi: &'a mut Scpi,
    errors: Vec<String>,
}

impl Run<'_> {
    /// The answer of one program message, or None when it queued an error or waited (kept in `errors`).
    fn exec(&mut self, message: &str) -> Option<Vec<u8>> {
        let queued = self.scpi.queued_errors().len();
        match self.scpi.exec_message_checked(self.ui, self.cv, message) {
            Ok((reply, _)) => {
                let new = self.scpi.queued_errors().get(queued..).map(|e| e.join("; ")).unwrap_or_default();
                if new.is_empty() {
                    return Some(reply);
                }
                self.errors.push(format!("{message}: {new}"));
                None
            }
            Err(error) => {
                self.errors.push(format!("{message}: {error}"));
                None
            }
        }
    }

    /// A query's answer as text.
    fn q(&mut self, query: &str) -> Option<String> {
        self.exec(query).map(|r| String::from_utf8_lossy(&r).trim().to_string())
    }

    /// A command; false when it failed.
    fn w(&mut self, command: &str) -> bool {
        self.exec(command).is_some()
    }
}

/// The result of a tool: its JSON as `structuredContent` and as text, with the errors of its commands; an error of
/// the tool when there are any.
fn answer(run: &mut Run, data: J) -> J {
    let mut data = data;
    let failed = !run.errors.is_empty();
    if let J::Obj(fields) = &mut data {
        if failed {
            fields.push(("errors".into(), J::Arr(run.errors.drain(..).map(J::Str).collect())));
        }
    }
    let mut out = vec![
        ("content", J::Arr(vec![J::obj(&[("type", J::s("text")), ("text", J::Str(data.to_string()))])])),
        ("structuredContent", data),
    ];
    if failed {
        out.push(("isError", J::Bool(true)));
    }
    J::obj(&out)
}

/// A tool refused before any command ran: its arguments were wrong.
fn refuse(why: &str) -> J {
    J::obj(&[
        ("isError", J::Bool(true)),
        ("content", J::Arr(vec![J::obj(&[("type", J::s("text")), ("text", J::s(why))])])),
    ])
}

/// A word that may stand in a command: letters, digits and `_ - + .`, at most 32. `;`, quotes and spaces never get in.
fn word(s: &str) -> Option<&str> {
    let ok = !s.is_empty() && s.len() <= 32 && s.chars().all(|c| c.is_ascii_alphanumeric() || "_-+.".contains(c));
    ok.then_some(s)
}

/// A finite number as a command writes it: a whole number as one (a slot, a count), else in exponent form.
fn num_arg(v: f64) -> Option<String> {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        return Some(format!("{}", v as i64));
    }
    v.is_finite().then(|| format!("{v:e}"))
}

fn jnum(j: &J) -> Option<f64> {
    match j {
        J::Num(n) => Some(*n),
        _ => None,
    }
}

/// A count of an argument: a whole number from `lo` to `hi`, `default` when it is not given.
fn count(args: &J, key: &str, default: usize, lo: usize, hi: usize) -> Result<usize, String> {
    match args.get(key) {
        None | Some(J::Null) => Ok(default),
        Some(j) => jnum(j)
            .filter(|n| n.fract() == 0.0 && *n >= lo as f64 && *n <= hi as f64)
            .map(|n| n as usize)
            .ok_or_else(|| format!("{key}: a whole number from {lo} to {hi}")),
    }
}

/// An answer of the interpreter as JSON: a number (null for 9.9E37), else the word.
fn value(s: Option<String>) -> J {
    match s {
        None => J::Null,
        Some(s) => match s.parse::<f64>() {
            Ok(v) if v.is_finite() && v.abs() < NO_VALUE => J::Num(v),
            Ok(_) => J::Null,
            Err(_) => J::Str(s),
        },
    }
}

/// An answer `1` or `0` as true or false.
fn flag(s: Option<String>) -> J {
    match s.as_deref() {
        Some("1") => J::Bool(true),
        Some("0") => J::Bool(false),
        Some(other) => J::s(other),
        None => J::Null,
    }
}

/// A number cut to six significant digits, which is what a sample of the scope holds and what a model reads.
fn short(v: f64) -> J {
    J::Num(format!("{v:.5e}").parse().unwrap_or(v))
}

fn scpi_tool(run: &mut Run, args: &J) -> J {
    if let Some(messages) = args.get("commands") {
        return scpi_messages(run, messages);
    }
    let text = |s: &str| J::obj(&[("content", J::Arr(vec![J::obj(&[("type", J::s("text")), ("text", J::s(s))])]))]);
    let c = args.get("command").and_then(J::str).unwrap_or("");
    let queued = run.scpi.queued_errors().len();
    match run.scpi.exec_message_checked(run.ui, run.cv, c) {
        // a command that failed answered "ok" like one that worked, and the caller of a tool does not read the error
        // queue after every call: the errors this message queued are part of its result (they stay queued for
        // :SYSTem:ERRor?)
        Ok((r, has_reply)) if run.scpi.queued_errors().len() > queued => {
            let errors = run.scpi.queued_errors()[queued..].join("; ");
            let reply =
                if r.is_empty() && !has_reply { String::new() } else { format!("{}\n", String::from_utf8_lossy(&r)) };
            let message = J::Str(format!("{reply}error: {errors}"));
            J::obj(&[
                ("isError", J::Bool(true)),
                ("content", J::Arr(vec![J::obj(&[("type", J::s("text")), ("text", message)])])),
            ])
        }
        Ok((r, has_reply)) => {
            text(if r.is_empty() && !has_reply { "ok" } else { std::str::from_utf8(&r).unwrap_or("(binary response)") })
        }
        Err(error) => refuse(error),
    }
}

/// `commands` of the `scpi` tool: each message in its turn, until one fails.
fn scpi_messages(run: &mut Run, messages: &J) -> J {
    let J::Arr(messages) = messages else { return refuse("commands: a list of strings") };
    if messages.len() > MESSAGES_MAX {
        return refuse(&format!("commands: at most {MESSAGES_MAX}"));
    }
    if messages.iter().any(|m| m.str().is_none()) {
        return refuse("commands: a list of strings");
    }
    let mut replies = Vec::new();
    for (k, message) in messages.iter().filter_map(J::str).enumerate() {
        let Some(r) = run.exec(message) else {
            let data = J::obj(&[("ran", J::Arr(replies)), ("stopped_at", J::Num(k as f64))]);
            return answer(run, data);
        };
        let reply = J::Str(String::from_utf8_lossy(&r).trim_end().to_string());
        replies.push(J::obj(&[("command", J::s(message)), ("reply", reply)]));
    }
    answer(run, J::obj(&[("ran", J::Arr(replies))]))
}

fn scope_state(run: &mut Run) -> J {
    let mut channels = Vec::new();
    for c in 1..=oos_hal::model::platform().caps.channels {
        let mut q = |k: &str| run.q(&format!(":CHAN{c}:{k}?"));
        channels.push(J::obj(&[
            ("channel", J::Num(c as f64)),
            ("display", flag(q("DISP"))),
            ("scale", value(q("SCAL"))),
            ("offset", value(q("OFFS"))),
            ("coupling", value(q("COUP"))),
            ("probe", value(q("PROB"))),
            ("bwlimit", value(q("BWL"))),
        ]));
    }
    let mut q = |k: &str| value(run.q(k));
    let timebase = J::obj(&[("scale", q(":TIM:SCAL?")), ("offset", q(":TIM:OFFS?"))]);
    let acquire =
        J::obj(&[("sample_rate", q(":ACQ:SRAT?")), ("memory_depth", q(":ACQ:MDEP?")), ("type", q(":ACQ:TYPE?"))]);
    let trigger = J::obj(&[
        ("type", q(":TRIG:MODE?")),
        ("status", q(":TRIG:STAT?")),
        ("sweep", q(":TRIG:SWE?")),
        ("source", q(":TRIG:EDGE:SOUR?")),
        ("level", q(":TRIG:EDGE:LEV?")),
        ("slope", q(":TRIG:EDGE:SLOP?")),
        ("coupling", q(":TRIG:COUP?")),
    ]);
    J::obj(&[("channels", J::Arr(channels)), ("timebase", timebase), ("acquire", acquire), ("trigger", trigger)])
}

fn measure(run: &mut Run, args: &J) -> J {
    let known = crate::scpi::measurement_items();
    let Some(J::Arr(items)) = args.get("items") else { return refuse("items: a list of measurement items") };
    if items.is_empty() || items.len() > ITEMS_MAX {
        return refuse(&format!("items: 1 to {ITEMS_MAX}"));
    }
    let mut sources = Vec::new();
    for key in ["source", "source2"] {
        match args.get(key).map(|j| j.str().and_then(word)) {
            None => sources.push(None),
            Some(Some(s)) => sources.push(Some(s)),
            Some(None) => return refuse(&format!("{key}: a source such as CHAN1 or R1")),
        }
    }
    let mut asked = Vec::new();
    for item in items {
        match item.str().and_then(|i| known.iter().find(|(t, _)| t.eq_ignore_ascii_case(i))) {
            Some(&found) => asked.push(found),
            None => return refuse(&format!("items: not a measurement item: {item}")),
        }
    }
    let mut results = Vec::new();
    for (token, pair) in asked {
        let mut query = format!(":MEAS:ITEM? {token}");
        let roles = if pair { &sources[..] } else { &sources[..1] };
        for s in roles.iter().map_while(|s| *s) {
            query.push(',');
            query.push_str(s);
        }
        let v = value(run.q(&query));
        results.push(J::obj(&[("item", J::s(token)), ("value", v)]));
    }
    let source = sources[0].map_or(J::Null, J::s);
    answer(run, J::obj(&[("source", source), ("results", J::Arr(results))]))
}

fn waveform(run: &mut Run, args: &J) -> J {
    let source = match args.get("source") {
        None => "CHAN1",
        Some(j) => match j.str().and_then(word) {
            Some(s) => s,
            None => return refuse("source: CHAN1..CHAN4, R1..R10 or MATH1..MATH4"),
        },
    };
    let memory = match args.get("record").and_then(J::str) {
        None | Some("screen") => false,
        Some("memory") => true,
        Some(_) => return refuse("record: screen or memory"),
    };
    let points = match count(args, "points", POINTS_DEFAULT, 2, POINTS_MAX) {
        Ok(n) => n,
        Err(e) => return refuse(&e),
    };
    let start = match count(args, "start", 1, 1, usize::MAX) {
        Ok(n) => n as u64,
        Err(e) => return refuse(&e),
    };
    let saved = run.scpi.wave_selection();
    let data = read_record(run, source, memory, points, start);
    run.scpi.set_wave_selection(saved);
    answer(run, data)
}

/// The record of `source` from `start`, at most `READ_MAX` samples, as statistics and `points` points.
fn read_record(run: &mut Run, source: &str, memory: bool, points: usize, start: u64) -> J {
    let mode = if memory { "RAW" } else { "NORM" };
    let selected = run.w(&format!(":WAV:SOUR {source}"))
        && run.w(&format!(":WAV:MODE {mode}"))
        && run.w(":WAV:FORM WORD")
        && run.w(":WAV:STAR 1")
        && run.w(&format!(":WAV:STOP {}", u64::MAX));
    let total = if selected { run.q(":WAV:STOP?").and_then(|s| s.parse::<u64>().ok()) } else { None };
    let mut out = vec![
        ("source", J::s(source)),
        ("record", J::s(if memory { "memory" } else { "screen" })),
        ("total_samples", J::Num(total.unwrap_or(0) as f64)),
        ("start", J::Num(start as f64)),
    ];
    let Some(total) = total else { return J::obj(&out) };
    if total == 0 || start > total {
        run.errors.push(format!("{source}: no samples from {start} (the record holds {total})"));
        return J::obj(&out);
    }
    let end = total.min(start.saturating_add(READ_MAX - 1));
    let paged = run.w(&format!(":WAV:STAR {start}")) && run.w(&format!(":WAV:STOP {end}"));
    let pre: Option<Vec<f64>> = if paged { run.q(":WAV:PRE?") } else { None }
        .map(|p| p.split(',').map(|f| f.trim().parse().unwrap_or(0.0)).collect())
        .filter(|p: &Vec<f64>| p.len() == 10);
    let Some(pre) = pre else { return J::obj(&out) };
    let Some(block) = run.exec(":WAV:DATA?") else { return J::obj(&out) };
    let Some(payload) = block_payload(&block) else {
        run.errors.push(":WAV:DATA?: not a block".into());
        return J::obj(&out);
    };
    let (dt, t0, yinc, yorg, yref) = (pre[4], pre[5], pre[7], pre[8], pre[9]);
    let codes: Vec<u16> = payload.chunks_exact(2).map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
    let clipped = codes.iter().filter(|&&c| c == 0 || c == u16::MAX).count();
    let volts: Vec<f64> = codes.iter().map(|&c| (f64::from(c) - yorg - yref) * yinc).collect();
    let n = volts.len();
    out.push(("samples_read", J::Num(n as f64)));
    out.push(("next_start", if end < total { J::Num((end + 1) as f64) } else { J::Null }));
    out.push(("dt", short(dt)));
    out.push(("t0", short(t0)));
    out.push(("unit", J::s("V")));
    out.push(("clipped", J::Num(clipped as f64)));
    if n == 0 {
        return J::obj(&out);
    }
    let range = |v: &[f64]| v.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &x| (lo.min(x), hi.max(x)));
    let (lo, hi) = range(&volts);
    let mean = volts.iter().sum::<f64>() / n as f64;
    let rms = (volts.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt();
    let stats = [("min", short(lo)), ("max", short(hi)), ("vpp", short(hi - lo)), ("mean", short(mean))];
    let mut stats = stats.to_vec();
    stats.push(("rms", short(rms)));
    out.push(("stats", J::obj(&stats)));
    if n <= points {
        out.push(("samples", J::Arr(volts.iter().map(|&v| short(v)).collect())));
    } else {
        // `points` buckets over the samples read, each its lowest and highest sample
        let pairs: Vec<J> = (0..points)
            .map(|k| {
                let (a, b) = (k * n / points, ((k + 1) * n / points).max(k * n / points + 1));
                let (lo, hi) = range(&volts[a..b]);
                J::Arr(vec![short(lo), short(hi)])
            })
            .collect();
        out.push(("bucket_dt", short(dt * n as f64 / points as f64)));
        out.push(("minmax", J::Arr(pairs)));
    }
    J::obj(&out)
}

/// The payload of an IEEE definite-length block, `#<n><n digits of length><payload>`.
fn block_payload(b: &[u8]) -> Option<&[u8]> {
    let at = b.iter().position(|&c| c == b'#')?;
    let n = (*b.get(at + 1)? as char).to_digit(10)? as usize;
    let len: usize = std::str::from_utf8(b.get(at + 2..at + 2 + n)?).ok()?.parse().ok()?;
    b.get(at + 2 + n..at + 2 + n + len)
}

fn decode_results(run: &mut Run, args: &J) -> J {
    let offset = match count(args, "offset", 0, 0, usize::MAX) {
        Ok(o) => o,
        Err(e) => return refuse(&e),
    };
    let rows = match count(args, "max_rows", ROWS_DEFAULT, 1, ROWS_MAX) {
        Ok(r) => r,
        Err(e) => return refuse(&e),
    };
    let events = run.q(":DEC:EVEN?");
    let mut out = Vec::new();
    match events.as_deref() {
        Some("BUSY") => out.push(("pending", J::Bool(true))),
        Some(text) => {
            let all: Vec<&str> = text.lines().filter(|l| !l.is_empty()).collect();
            let shown: Vec<J> = all.iter().skip(offset).take(rows).map(|line| event_row(line)).collect();
            let next = offset + shown.len();
            out.push(("pending", J::Bool(false)));
            out.push(("total_rows", J::Num(all.len() as f64)));
            out.push(("offset", J::Num(offset as f64)));
            out.push(("rows", J::Arr(shown)));
            out.push(("next_offset", if next < all.len() { J::Num(next as f64) } else { J::Null }));
        }
        None => {}
    }
    answer(run, J::obj(&out))
}

/// A row of `:DECode:EVENts?`, `t,who,text,ok`: the text may hold commas of its own.
fn event_row(line: &str) -> J {
    let f: Vec<&str> = line.split(',').collect();
    let text = if f.len() > 3 { f[2..f.len() - 1].join(",") } else { String::new() };
    J::obj(&[
        ("t", value(f.first().map(|s| s.to_string()))),
        ("decoder", J::s(f.get(1).copied().unwrap_or(""))),
        ("text", J::Str(text)),
        ("ok", J::Bool(f.last() == Some(&"ok"))),
    ])
}

fn errors(run: &mut Run) -> J {
    let mut queue = Vec::new();
    // the queue holds at most 20 (scpi.md, Errors): a few more reads end at "0,..." in any case
    for _ in 0..24 {
        match run.q(":SYST:ERR?") {
            Some(e) if !e.starts_with("0,") => queue.push(J::Str(e)),
            _ => break,
        }
    }
    answer(run, J::obj(&[("errors_queued", J::Arr(queue))]))
}

/// What the value of a key of `configure` or `setup_decoder` has to be.
enum Kind {
    Num,
    Bool,
    Word,
}

/// The commands of an object of `configure` or `setup_decoder`, in the order of `table` (key, header, kind); an
/// unknown key, or a value of the wrong kind, refuses the whole call before any command runs. A key with an empty
/// header only names what the others set (the channel).
fn commands(obj: &J, table: &[(&str, &str, Kind)], prefix: &str, out: &mut Vec<String>) -> Result<(), String> {
    let J::Obj(fields) = obj else { return Err(format!("{prefix}: an object")) };
    if let Some((key, _)) = fields.iter().find(|(key, _)| !table.iter().any(|(k, _, _)| k == key)) {
        return Err(format!("{prefix}{key}: not known"));
    }
    for (key, header, kind) in table {
        let Some(v) = obj.get(key) else { continue };
        let arg = match (kind, v) {
            (Kind::Num, J::Num(n)) => num_arg(*n),
            (Kind::Bool, J::Bool(b)) => Some(if *b { "ON" } else { "OFF" }.into()),
            (Kind::Word, J::Str(s)) => word(s).map(str::to_string),
            (Kind::Word, J::Num(n)) => num_arg(*n),
            _ => None,
        };
        let Some(arg) = arg else { return Err(format!("{prefix}{key}: not a value it takes")) };
        if !header.is_empty() {
            out.push(format!("{header} {arg}"));
        }
    }
    Ok(())
}

/// The commands of `configure`: the acquisition, the timebase, the channels (the probe before the scale), the trigger
/// (its source before its level).
fn configure_commands(args: &J) -> Result<Vec<String>, String> {
    let mut cmds = Vec::new();
    if let J::Obj(fields) = args {
        let parts = ["channels", "timebase", "trigger", "acquire"];
        if let Some((key, _)) = fields.iter().find(|(k, _)| !parts.contains(&k.as_str())) {
            return Err(format!("{key}: not known"));
        }
    }
    if let Some(a) = args.get("acquire") {
        let table = [
            ("memory_depth", ":ACQ:MDEP", Kind::Word),
            ("type", ":ACQ:TYPE", Kind::Word),
            ("averages", ":ACQ:AVER", Kind::Num),
        ];
        commands(a, &table, "acquire.", &mut cmds)?;
    }
    if let Some(t) = args.get("timebase") {
        let table = [("scale", ":TIM:SCAL", Kind::Num), ("offset", ":TIM:OFFS", Kind::Num)];
        commands(t, &table, "timebase.", &mut cmds)?;
    }
    if let Some(chs) = args.get("channels") {
        let J::Arr(chs) = chs else { return Err("channels: a list".into()) };
        let n = oos_hal::model::platform().caps.channels;
        for ch in chs {
            let c = ch.get("channel").and_then(jnum).filter(|c| c.fract() == 0.0 && *c >= 1.0 && *c <= n as f64);
            let Some(c) = c else { return Err(format!("channels: channel from 1 to {n}")) };
            let h = |k: &str| format!(":CHAN{c}:{k}");
            let headers = [h("DISP"), h("PROB"), h("SCAL"), h("OFFS"), h("COUP"), h("BWL"), h("INV")];
            let table = [
                ("channel", "", Kind::Num),
                ("display", headers[0].as_str(), Kind::Bool),
                ("probe", headers[1].as_str(), Kind::Num),
                ("scale", headers[2].as_str(), Kind::Num),
                ("offset", headers[3].as_str(), Kind::Num),
                ("coupling", headers[4].as_str(), Kind::Word),
                ("bwlimit", headers[5].as_str(), Kind::Word),
                ("invert", headers[6].as_str(), Kind::Bool),
            ];
            commands(ch, &table, &format!("channels[{c}]."), &mut cmds)?;
        }
    }
    if let Some(t) = args.get("trigger") {
        let table = [
            ("type", ":TRIG:MODE", Kind::Word),
            ("source", ":TRIG:EDGE:SOUR", Kind::Word),
            ("slope", ":TRIG:EDGE:SLOP", Kind::Word),
            ("coupling", ":TRIG:COUP", Kind::Word),
            ("sweep", ":TRIG:SWE", Kind::Word),
            ("holdoff", ":TRIG:HOLD", Kind::Num),
            ("level", ":TRIG:EDGE:LEV", Kind::Num),
        ];
        commands(t, &table, "trigger.", &mut cmds)?;
    }
    Ok(cmds)
}

fn configure(run: &mut Run, args: &J) -> J {
    let cmds = match configure_commands(args) {
        Ok(c) => c,
        Err(e) => return refuse(&e),
    };
    let applied: Vec<J> = cmds.iter().filter(|c| run.w(c)).map(|c| J::s(c)).collect();
    let state = scope_state(run);
    answer(run, J::obj(&[("applied", J::Arr(applied)), ("state", state)]))
}

fn acquire(run: &mut Run, args: &J) -> J {
    let command = match args.get("action").and_then(J::str) {
        Some("run") => ":RUN",
        Some("stop") => ":STOP",
        Some("single") => ":SING",
        Some("force") => ":TFOR",
        Some("autoset") => ":AUT",
        Some("clear") => ":CLE",
        _ => return refuse("action: run, stop, single, force, autoset or clear"),
    };
    run.w(command);
    let status = value(run.q(":TRIG:STAT?"));
    let autoset = flag(run.q(":AUT?"));
    answer(run, J::obj(&[("done", J::s(command)), ("trigger_status", status), ("autoset_running", autoset)]))
}

/// The commands of `setup_decoder`: the slot, the protocol (it sets the roles the sources fill), then the rest.
fn decoder_commands(args: &J) -> Result<Vec<String>, String> {
    let protocol = match args.get("protocol") {
        None => None,
        Some(p) => {
            let found =
                p.str().and_then(|p| crate::protocols::PROTOS.iter().find(|q| q.name().eq_ignore_ascii_case(p)));
            Some(found.ok_or("protocol: one of the list of tools/list")?.name())
        }
    };
    if args.get("slot").is_some_and(|s| jnum(s).is_none_or(|s| s.fract() != 0.0 || s < 1.0)) {
        return Err("slot: a whole number from 1".into());
    }
    let mut rest = args.clone();
    if let J::Obj(fields) = &mut rest {
        fields.retain(|(k, _)| k != "protocol");
    }
    let table = [
        ("slot", ":DEC:SEL", Kind::Num),
        ("input", ":DEC:INP", Kind::Word),
        ("source", ":DEC:SOUR", Kind::Word),
        ("source2", ":DEC:SOUR2", Kind::Word),
        ("cs", ":DEC:CS", Kind::Word),
        ("source4", ":DEC:SOUR4", Kind::Word),
        ("threshold", ":DEC:THR", Kind::Num),
        ("state", ":DEC:STAT", Kind::Bool),
    ];
    let mut cmds = Vec::new();
    commands(&rest, &table, "", &mut cmds)?;
    if let Some(p) = protocol {
        cmds.insert(usize::from(args.get("slot").is_some()), format!(":DEC:PROT {p}"));
    }
    Ok(cmds)
}

fn setup_decoder(run: &mut Run, args: &J) -> J {
    let cmds = match decoder_commands(args) {
        Ok(c) => c,
        Err(e) => return refuse(&e),
    };
    let applied: Vec<J> = cmds.iter().filter(|c| run.w(c)).map(|c| J::s(c)).collect();
    let mut q = |k: &str| value(run.q(k));
    let decoder = J::obj(&[
        ("slot", q(":DEC:SEL?")),
        ("protocol", q(":DEC:PROT?")),
        ("state", q(":DEC:STAT?")),
        ("input", q(":DEC:INP?")),
        ("source", q(":DEC:SOUR?")),
        ("source2", q(":DEC:SOUR2?")),
        ("threshold", q(":DEC:THR?")),
    ]);
    answer(run, J::obj(&[("applied", J::Arr(applied)), ("decoder", decoder)]))
}
