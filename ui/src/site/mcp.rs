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
use oos_remote::http::Body;

mod reference;

/// Rows of the CSV file of `waveform` by default, and at most; points it puts into its answer at most.
const POINTS_DEFAULT: usize = 10_000;
const POINTS_MAX: usize = 100_000;
const INLINE_MAX: usize = 1000;
/// Where the web remote serves the CSV file of each `waveform` call, under a name of its own (`ch1-3.csv`), so that
/// the file of one call is not replaced by the next (CH1 then CH2 before either is downloaded).
const WAVEFORM_DIR: &str = "/mcp/waveform/";
/// How long the file of a `waveform` call stays to be downloaded; then it is deleted.
pub const FILE_TTL: std::time::Duration = std::time::Duration::from_secs(120);
/// Files of `waveform` kept at once (each at most a few MB): a call past it deletes the oldest.
pub const FILES_MAX: usize = 4;
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

/// One MCP JSON-RPC message; None for notifications (answered with 202 and no body). The answer of `screenshot` is
/// made later (`Body::Later`, on a sender thread of the web server), from a copy of the screen taken here (#205:
/// encoding a PNG took the frame loop's time).
pub fn mcp(body: &str, cx: &mut Ctx) -> Option<Body> {
    let (ui, cv, scpi) = (&mut *cx.ui, cx.cv, &mut *cx.scpi);
    let err = |id: J, code: f64, msg: &str| {
        J::obj(&[
            ("jsonrpc", J::s("2.0")),
            ("id", id),
            ("error", J::obj(&[("code", J::Num(code)), ("message", J::s(msg))])),
        ])
        .to_string()
    };
    let now = |s: String| Some(Body::Now(s.into_bytes()));
    let Some(req) = parse(body) else { return now(err(J::Null, -32700.0, "parse error")) };
    let id = req.get("id").cloned()?;
    let method = req.get("method").and_then(J::str).unwrap_or("");
    let params = req.get("params").cloned().unwrap_or(J::Null);
    let args = params.get("arguments").cloned().unwrap_or(J::Obj(Vec::new()));
    // Remote services > MCP (`remote.mcp`): off answers every request with an error; read only offers the tools that
    // only read, and `scpi` for queries alone
    let read = match ui.sys.remote_mcp {
        McpMode::Off => return now(err(id, -32001.0, OFF)),
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
            J::Arr(only_reads(
                read,
                vec![
                    if read {
                        tool(("name", J::s("scpi")), SCPI_QUERIES, scpi_schema(), READS)
                    } else {
                        tool(("name", J::s("scpi")), SCPI, scpi_schema(), WRITES)
                    },
                    tool(("name", J::s("screenshot")), SCREENSHOT, schema(&[], &[]), READS),
                    tool(("name", J::s("scope_state")), SCOPE_STATE, schema(&[], &[]), READS),
                    tool(("name", J::s("measure")), MEASURE, measure_schema(), READS),
                    tool(("name", J::s("measure_slot")), MEASURE_SLOT, measure_slot_schema(), WRITES),
                    tool(("name", J::s("waveform")), WAVEFORM, waveform_schema(), READS),
                    tool(("name", J::s("decode_results")), DECODE_RESULTS, decode_results_schema(), READS),
                    tool(("name", J::s("table")), TABLE, table_schema(), READS),
                    tool(("name", J::s("errors")), ERRORS, schema(&[], &[]), READS),
                    tool(("name", J::s("scpi_reference")), SCPI_REFERENCE, scpi_reference_schema(), READS),
                    tool(("name", J::s("configure")), CONFIGURE, configure_schema(), WRITES),
                    tool(("name", J::s("acquire")), ACQUIRE, acquire_schema(), WRITES),
                    tool(("name", J::s("setup_decoder")), SETUP_DECODER, setup_decoder_schema(), WRITES),
                ],
            )),
        )]),
        "tools/call" => match params.get("name").and_then(J::str) {
            name if read && !reads_only(name, &args) => refuse(READ_ONLY),
            Some("scpi") => scpi_tool(&mut run, &args),
            Some("screenshot") => {
                let shot = super::snapshot(cv);
                return Some(Body::Later(Box::new(move || {
                    let png = crate::gfx::png::png_of(&shot);
                    let result = J::obj(&[(
                        "content",
                        J::Arr(vec![J::obj(&[
                            ("type", J::s("image")),
                            ("mimeType", J::s("image/png")),
                            ("data", J::Str(crate::base64::encode(&png))),
                        ])]),
                    )]);
                    J::obj(&[("jsonrpc", J::s("2.0")), ("id", id), ("result", result)]).to_string().into_bytes()
                })));
            }
            Some("scope_state") => {
                let state = scope_state(&mut run);
                answer(&mut run, state)
            }
            Some("measure") => measure(&mut run, &args),
            Some("measure_slot") => measure_slot(&mut run, &args),
            Some("waveform") => waveform(&mut run, &args),
            Some("decode_results") => decode_results(&mut run, &args),
            Some("table") => table(&mut run, &args),
            Some("errors") => errors(&mut run),
            Some("scpi_reference") => scpi_reference(&mut run, &args),
            Some("configure") => configure(&mut run, &args),
            Some("acquire") => acquire(&mut run, &args),
            Some("setup_decoder") => setup_decoder(&mut run, &args),
            _ => return now(err(id, -32602.0, "unknown tool")),
        },
        _ => return now(err(id, -32601.0, "method not found")),
    };
    now(J::obj(&[("jsonrpc", J::s("2.0")), ("id", id), ("result", result)]).to_string())
}

/// What `initialize` tells the client's model, as short rules: a small local model follows a numbered list better than
/// a paragraph. Each rule is a trap seen with such models (a guessed command, samples asked into the context, a single
/// shot taken as done, the same failed command sent again).
const INSTRUCTIONS: &str = "OOS oscilloscope (Rigol DHO800/DHO900). Rules:
1. Start with scope_state.
2. Use the typed tools first: configure, acquire, measure, waveform, decode_results. Use scpi only for what they lack.
3. Look up, send, verify. Never guess a SCPI command: find it with scpi_reference (search, then header), send it with
scpi, then check the value read back in its answer. Sent is not done: a setting may be refused, rounded or limited.
4. waveform gives statistics and a CSV link: download the file at once (deleted after 2 minutes); never ask for
samples inline.
5. acquire does not wait: after single, ask scope_state until trigger.status is STOP.
6. After an error, read its hint. Do not resend the same command.
7. If a tool says MCP is read only, do not try to change the scope.
8. The scope does more than the typed tools: it arranges its screen, simulates signals,
opens plots and analysers, presses menus. Before you say something cannot be done, call scpi_reference with no
arguments: it lists what the scope can do by task.
9. After a change to the screen, check it with screenshot.
10. A value for the screen: measure_slot adds it to the measurement table; measure shows nothing.
11. Decoder empty or failing: decode_results diagnosis; setup_decoder auto:true.
Values in base units: V, s, Hz.";

const SCPI: &str = "Run SCPI on the oscilloscope: `command`, one program message (commands separated by ';'), answers \
    the response of its queries; or `commands`, a list of messages run in order that stops at the first error. The \
    commands are those of OOS's SCPI reference (Rigol-style: :TIM:SCAL 1e-3, :CHAN1:SCAL 0.5, :MEAS:ITEM? VPP,CHAN1): \
    look a command up with scpi_reference before sending it, do not guess its name. Every command that sets something \
    and has a query form is read back after the message (`read back :CHAN1:SCAL? -> 5.000000E-1`): compare it with \
    what was sent.";
const SCPI_QUERIES: &str = "Run SCPI queries on the oscilloscope (MCP is read only on this scope): `command`, one \
    program message whose every command is a query (its header ends in '?', commands separated by ';'), or \
    `commands`, a list of such messages. The queries are those of OOS's SCPI reference (:TIM:SCAL?, \
    :MEAS:ITEM? VPP,CHAN1).";
/// What the answer of `scpi_reference` without arguments says about its list of tasks.
const TASKS_HINT: &str = "what_the_scope_can_do: pick the task, then list its subsystem (scpi_reference with     `subsystem`) or search by what it should do. A task is not a typed tool: send its commands with scpi.";
/// What `decode_results` adds while the decoders compute.
const DECODE_PENDING: &str = "The decoders are still computing: ask again in a moment. While the scope runs, the rows     are those of the record on the screen and change with the next acquisition: stop it (acquire stop, or single)     first for a stable, complete list.";
const OFF: &str = "MCP is off on this scope (System > Network > Remote services > MCP)";
const READ_ONLY: &str = "MCP is read only on this scope (System > Network > Remote services > MCP): this tool, or \
    a command that is not a query, is refused";
const SCREENSHOT: &str = "The current oscilloscope screen as a PNG image.";
const SCOPE_STATE: &str = "The whole set-up in one answer: every channel (display, scale V/div, offset V, coupling, \
    probe, bandwidth limit), the timebase (s/div, delay s), the acquisition (sample rate, memory depth, mode) and the \
    trigger (type, status STOP|TD|AUTO|WAIT, sweep, source, level, slope, coupling). Ask it after `acquire` to see \
    whether a single shot stopped.";
const MEASURE: &str = "Measure items of the measurement list on a source, as numbers in base units (V, s, Hz, %, \
    degrees); null where the scope has no value (no edge on screen, still computing). Pair items (delay, phase, skew, \
    setup, hold) take `source` and `source2`. Does not add rows to the screen's measurement strip: for a number that     is to be on the screen use `measure_slot`.";
const MEASURE_SLOT: &str = "Numbered measurement slots MEAS1..MEAS99: a slot holds one measurement that is a row of     the screen's measurement table (with the badge M<n>) and is read back by its number. With `item` (and `source`,     for a pair `source2`): add it to slot `slot`, then answer what it measures and its value (null while it is     computing). Without `item`: read the slot; an empty slot is an error that says so. `clear`: empty the slot and     take its row off the table. Adding to a slot that is in use replaces its measurement. A source is CHAN1..CHAN4,     R1..R10 or MATH1..MATH4. Use this, not `measure`, for a value the user is to see on the screen.";
const WAVEFORM: &str = "A record of a source: its statistics (min, max, mean, rms, peak-to-peak in volts, clipped \
    samples) and a link to a CSV file of it, `points` rows (the samples, or min/max per bucket when the record is \
    longer). The samples are not in the answer: download the file to the computer (curl -o wave.csv <url>) and work \
    on it there instead of reading it into the conversation; each call has its own file (the last 4 are kept), \
    deleted 2 minutes after the call (`expires_in_s`), so download it at once. `inline` puts up to \
    1000 points into the answer as well. `record` screen reads what is drawn, memory the acquisition memory, 1 Mi \
    samples per call from `start` (the answer gives `next_start`). The waveform selection of other SCPI clients is \
    left as it was.";
const DECODE_RESULTS: &str = "The events of the decoders that are on, as rows (time s, decoder, text, ok), \
    `max_rows` from `offset`. `pending` while the decoders are still computing: ask again.";
const DECODE_TABLE_HINT: &str = "These are the rows of the Events pane (the decode table under the waveform), whose     view is `view`. To see it on the screen: :DISPlay:LOWer EVENTS. As a file: :TABLe:SAVE EVENTS. The rows of     every field: table `FIELDS`.";
const TABLE_HINT: &str = "Read a table with `name` (Index is the first column of every table), `max_rows` from     `offset`. As a CSV file on the scope: :TABLe:SAVE <name>; the whole table over SCPI: :TABLe:DATA? <name>.";
const TABLE: &str = "Every table the screen shows (decode table, fields, search events, segments, spectrum peaks,     measurements) by name, with its Index in the first column; without `name` the list of tables.";
const ERRORS: &str = "Empty the SCPI error queue and answer its entries, oldest first.";
const SCPI_REFERENCE: &str = "OOS's SCPI reference (the command chapter of its programming manual), to find a command \
    instead of guessing it: `search`, what it should do in your own words (\"volts per division\", \"trigger \
    level\", \"measure rise time\"), answers the commands that fit best, best first, with their syntax, and as \
    `items` the measurement items, eye figures or error codes that fit; `header`, a command as it is sent \
    (:CHAN1:SCAL, :TRIG:EDGE:LEV?), answers it whole: parameters with their ranges, what the query returns, an \
    example; `subsystem`, part of a subsystem's title, lists its commands, and for one subsystem what the manual \
    says about it and its examples; `appendix`, part of an appendix's title, lists its rows (the items `:MEAS:ITEM` \
    takes, the error codes). `offset` goes on in a long list. Without arguments: the subsystems and the appendices. \
    A header that is not here is not a command of this scope.";
/// What a search adds when no command and no item fits any of its words.
const NOTHING_FITS: &str = "No command fits these words: say what it should do in other words, or list a subsystem.";
/// What an answer adds when a command was not known (-113): the way to the right one.
const UNDEFINED: &str = "Undefined header: the command is not in OOS's SCPI reference. Find the right one with the \
    tool scpi_reference (search: what it should do) instead of guessing another name.";
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
        Some("configure" | "acquire" | "setup_decoder" | "measure_slot") => false,
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

fn scpi_reference_schema() -> J {
    schema(
        &[
            ("search", string("What the command should do, in your own words; the commands that fit best come first.")),
            (
                "header",
                string("A command as it is sent, short or long form, e.g. :CHAN1:SCAL or :TRIGger:EDGE:LEVel?."),
            ),
            ("subsystem", string("Part of a subsystem's title, e.g. Channels, Trigger, Measure.")),
            ("appendix", string("Part of an appendix's title, e.g. Measurement Items, Eye Figures, Error Codes.")),
            ("max_results", integer("Commands or rows in the answer, 1 to 50; default 20.")),
            ("offset", integer("Commands or rows to skip: the next_offset of the answer before; default 0.")),
        ],
        &[],
    )
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

fn measure_slot_schema() -> J {
    let items: Vec<&str> = crate::scpi::measurement_items().into_iter().map(|(t, _)| t).collect();
    schema(
        &[
            ("slot", integer("The slot, 1 to 99.")),
            ("item", choice(&items, "The measurement to add to the slot; leave out to read the slot.")),
            ("source", string("CHAN1..CHAN4, R1..R10 or MATH1..MATH4; default: the source of the Measure menu.")),
            ("source2", string("The second source of a pair item.")),
            ("clear", boolean()),
        ],
        &["slot"],
    )
}

fn waveform_schema() -> J {
    schema(
        &[
            ("source", string("CHAN1..CHAN4, R1..R10 or MATH1..MATH4; default CHAN1.")),
            ("record", choice(&["screen", "memory"], "What is drawn, or the acquisition memory; default screen.")),
            ("points", integer("Rows of the CSV file (samples, or min/max buckets), 2 to 100000; default 10000.")),
            ("inline", integer("Points (or min/max pairs) also in the answer, 0 to 1000; default 0.")),
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

fn table_schema() -> J {
    schema(
        &[
            ("name", string("EVENTS, FIELDS, SEARCH, SEGMENTS, PEAKS or MEASURE; without it the list of tables.")),
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
            (
                "auto",
                J::obj(&[
                    ("type", J::s("boolean")),
                    (
                        "description",
                        J::s("Set the decoder up for its signal: threshold between the levels, Polarity Auto, the time base and depth that give 10 samples a bit and hold a frame, then one acquisition; the answer names every change."),
                    ),
                ]),
            ),
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
        if run.errors.iter().any(|e| e.contains("-113")) {
            fields.push(("hint".into(), J::s(UNDEFINED)));
        }
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
            let hint = if errors.contains("-113") { format!("\n{UNDEFINED}") } else { String::new() };
            let message = J::Str(format!("{reply}error: {errors}{hint}"));
            J::obj(&[
                ("isError", J::Bool(true)),
                ("content", J::Arr(vec![J::obj(&[("type", J::s("text")), ("text", message)])])),
            ])
        }
        Ok((r, has_reply)) => {
            let reply =
                if r.is_empty() && !has_reply { "ok" } else { std::str::from_utf8(&r).unwrap_or("(binary response)") };
            let checks = read_back(run, c);
            if checks.is_empty() {
                return text(reply);
            }
            let lines: Vec<String> = checks.iter().map(|(q, v)| format!("read back {q} -> {v}")).collect();
            text(&format!("{}\n{}", reply.trim_end(), lines.join("\n")))
        }
        Err(error) => refuse(error),
    }
}

/// Lookup, send, verify: each command of `message` that sets something and has a query form in the reference
/// (`:CHAN1:SCAL 0.5` -> `:CHAN1:SCAL?`) asked back, with what the scope answers; a command whose query fails reads
/// back as its error. A setting that was taken but changed (rounded to a step, limited to a range) shows here.
fn read_back(run: &mut Run, message: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for query in message.split(';').filter_map(reference::read_back) {
        let queued = run.scpi.queued_errors().len();
        let value = match run.scpi.exec_message_checked(run.ui, run.cv, &query) {
            Ok(_) if run.scpi.queued_errors().len() > queued => {
                format!("error: {}", run.scpi.queued_errors()[queued..].join("; "))
            }
            Ok((r, _)) => String::from_utf8_lossy(&r).trim().to_string(),
            Err(e) => format!("error: {e}"),
        };
        out.push((query, value));
    }
    out
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
        let checks = read_back(run, message);
        let mut item = vec![("command", J::s(message)), ("reply", reply)];
        if !checks.is_empty() {
            let checks = checks.into_iter().map(|(q, v)| J::obj(&[("query", J::Str(q)), ("value", J::Str(v))]));
            item.push(("read_back", J::Arr(checks.collect())));
        }
        replies.push(J::obj(&item));
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
        J::obj(&[
            ("sample_rate", q(":ACQ:SRAT?")),
            ("memory_depth", q(":ACQ:MDEP?")),
            ("type", q(":ACQ:TYPE?")),
            // roll: on the time bases from 200 ms/div the record is the stream's, not the depth asked for
            ("auto_roll", q(":TIM:ROLL?")),
        ]);
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

/// `measure_slot`: add to a slot, read it, or clear it (`:MEASure:SLOT<n>`, ui/slots.rs).
fn measure_slot(run: &mut Run, args: &J) -> J {
    let n = match count(args, "slot", 0, 1, crate::ui::slots::SLOTS as usize) {
        Ok(0) => return refuse("slot: a whole number from 1 to 99"),
        Ok(n) => n,
        Err(e) => return refuse(&e),
    };
    let clear = matches!(args.get("clear"), Some(J::Bool(true)));
    if clear && args.get("item").is_some() {
        return refuse("clear: not with item");
    }
    let known = crate::scpi::measurement_items();
    let mut command = None;
    if let Some(item) = args.get("item") {
        let Some(&(token, pair)) = item.str().and_then(|i| known.iter().find(|(t, _)| t.eq_ignore_ascii_case(i)))
        else {
            return refuse(&format!("item: not a measurement item: {item}"));
        };
        let mut c = format!(":MEAS:SLOT{n} {token}");
        for (key, used) in [("source", true), ("source2", pair)] {
            match args.get(key).map(|j| j.str().and_then(word)) {
                None => break,
                Some(Some(s)) if used => {
                    c.push(',');
                    c.push_str(s);
                }
                Some(Some(_)) => return refuse("source2: only for a pair item"),
                Some(None) => return refuse(&format!("{key}: a source such as CHAN1, R1 or MATH1")),
            }
        }
        command = Some(c);
    }
    if clear {
        run.w(&format!(":MEAS:SLOT{n}:CLE"));
        return answer(run, J::obj(&[("slot", J::Num(n as f64)), ("cleared", J::Bool(true))]));
    }
    if command.is_some_and(|c| !run.w(&c)) {
        return answer(run, J::obj(&[("slot", J::Num(n as f64))]));
    }
    let item = run.q(&format!(":MEAS:SLOT{n}:ITEM?"));
    let v = value(run.q(&format!(":MEAS:SLOT{n}?")));
    answer(run, J::obj(&[("slot", J::Num(n as f64)), ("item", item.map_or(J::Null, J::Str)), ("value", v)]))
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
    let inline = match count(args, "inline", 0, 0, INLINE_MAX) {
        Ok(n) => n,
        Err(e) => return refuse(&e),
    };
    let start = match count(args, "start", 1, 1, usize::MAX) {
        Ok(n) => n as u64,
        Err(e) => return refuse(&e),
    };
    let saved = run.scpi.wave_selection();
    let data = read_record(run, source, memory, (points, inline), start);
    run.scpi.set_wave_selection(saved);
    answer(run, data)
}

/// The record of `source` from `start`, at most `READ_MAX` samples, as statistics, a CSV file of `points` rows kept
/// under a name of its own for `GET /mcp/waveform/<file>`, and `inline` points in the answer.
fn read_record(run: &mut Run, source: &str, memory: bool, (points, inline): (usize, usize), start: u64) -> J {
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
        // a source without a record (a channel that is off) keeps the stop that was asked, u64::MAX: no samples, and
        // :WAV:DATA? below names why
        ("total_samples", J::Num(total.filter(|&t| t != u64::MAX).unwrap_or(0) as f64)),
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
    // bucket `k` of `m` over the samples read: its first sample and the one after its last
    let bucket = |k: usize, m: usize| (k * n / m, ((k + 1) * n / m).max(k * n / m + 1));
    let t = |i: usize| t0 + dt * i as f64;
    let (columns, rows) = if n <= points {
        ("time_s,v", volts.iter().enumerate().map(|(i, v)| format!("{:.9e},{:.6e}\n", t(i), v)).collect::<String>())
    } else {
        let rows = (0..points).map(|k| {
            let (a, b) = bucket(k, points);
            let (lo, hi) = range(&volts[a..b]);
            format!("{:.9e},{:.6e},{:.6e}\n", t(a), lo, hi)
        });
        ("time_s,min_v,max_v", rows.collect())
    };
    let csv = format!("{columns}\n{rows}").into_bytes();
    run.scpi.mcp_calls = run.scpi.mcp_calls.wrapping_add(1);
    let name: String = source.chars().filter(char::is_ascii_alphanumeric).collect();
    let name = format!("{}-{}.csv", name.to_ascii_lowercase(), run.scpi.mcp_calls);
    let url = match run.ui.net.addr.as_str() {
        "" => J::Str(format!("{WAVEFORM_DIR}{name}")),
        net => J::Str(format!("http://{net}:8080{WAVEFORM_DIR}{name}")),
    };
    out.push((
        "file",
        J::obj(&[
            ("url", url),
            ("columns", J::s(columns)),
            ("rows", J::Num(n.min(points) as f64)),
            ("bytes", J::Num(csv.len() as f64)),
            ("expires_in_s", J::Num(FILE_TTL.as_secs() as f64)),
        ]),
    ));
    let files = &mut run.scpi.mcp_files;
    files.push((name, std::time::Instant::now(), csv));
    if files.len() > FILES_MAX {
        files.drain(..files.len() - FILES_MAX);
    }
    if inline > 0 && n <= inline {
        out.push(("samples", J::Arr(volts.iter().map(|&v| short(v)).collect())));
    } else if inline > 0 {
        let pairs: Vec<J> = (0..inline)
            .map(|k| {
                let (a, b) = bucket(k, inline);
                let (lo, hi) = range(&volts[a..b]);
                J::Arr(vec![short(lo), short(hi)])
            })
            .collect();
        out.push(("bucket_dt", short(dt * n as f64 / inline as f64)));
        out.push(("minmax", J::Arr(pairs)));
    }
    J::obj(&out)
}

/// `GET /mcp/waveform/<file>`: the CSV of the `waveform` call that named it, for `FILE_TTL` after it; refused while
/// MCP is off, for a name no call gave, after that time and once `FILES_MAX` later calls have pushed it out.
pub fn waveform_csv(ui: &Ui, scpi: &Scpi, name: &str) -> Result<Vec<u8>, String> {
    if ui.sys.remote_mcp == McpMode::Off {
        return Err(OFF.into());
    }
    match scpi.mcp_files.iter().find(|(n, _, _)| n == name) {
        Some((_, made, csv)) if made.elapsed() < FILE_TTL => Ok(csv.clone()),
        _ => Err(format!(
            "No waveform file {name}: call the MCP tool waveform, then download the url of its answer within 2 minutes"
        )),
    }
}

/// The files of `waveform` deleted once `FILE_TTL` has passed (each pass of the web server, `Site::pass`): each is a
/// file for one client to fetch at once, not a store.
pub fn expire_files(scpi: &mut Scpi) {
    scpi.mcp_files.retain(|(_, made, _)| made.elapsed() < FILE_TTL);
}

/// The payload of an IEEE definite-length block, `#<n><n digits of length><payload>`.
fn block_payload(b: &[u8]) -> Option<&[u8]> {
    let at = b.iter().position(|&c| c == b'#')?;
    let n = (*b.get(at + 1)? as char).to_digit(10)? as usize;
    let len: usize = std::str::from_utf8(b.get(at + 2..at + 2 + n)?).ok()?.parse().ok()?;
    b.get(at + 2 + n..at + 2 + n + len)
}

/// `scpi_reference`: a header looked up whole, an appendix, a search, a subsystem's commands, or what there is.
fn scpi_reference(run: &mut Run, args: &J) -> J {
    let page = count(args, "max_results", reference::RESULTS_DEFAULT, 1, reference::RESULTS_MAX);
    let (max, offset) = match (page, count(args, "offset", 0, 0, usize::MAX)) {
        (Ok(max), Ok(offset)) => (max, offset),
        (Err(e), _) | (_, Err(e)) => return refuse(&e),
    };
    let mut text = [None; 4];
    for (k, key) in ["header", "appendix", "search", "subsystem"].into_iter().enumerate() {
        match args.get(key) {
            None => {}
            Some(J::Str(s)) if !s.trim().is_empty() && s.len() <= 200 => text[k] = Some(s.as_str()),
            Some(_) => return refuse(&format!("{key}: a text of 1 to 200 characters")),
        }
    }
    let r = reference::reference();
    // What a list that goes on adds: where the next call starts.
    let more = |total: usize| (offset + max < total).then(|| ("next_offset", J::Num((offset + max) as f64)));
    let appendices = || {
        let one = |a: &reference::Appendix| J::obj(&[("title", J::s(&a.title)), ("rows", J::Num(a.rows.len() as f64))]);
        J::Arr(r.appendices.iter().map(one).collect())
    };
    let data = match text {
        [Some(header), ..] => {
            let found: Vec<J> = reference::lookup(header).into_iter().map(reference::full).collect();
            let mut out = vec![("header", J::s(header)), ("found", J::Num(found.len() as f64))];
            if found.is_empty() {
                out.push(("hint", J::s("No command of this scope has this header: search by what it should do.")));
            }
            out.push(("commands", J::Arr(found)));
            J::obj(&out)
        }
        [None, Some(title), ..] => match reference::appendices(title)[..] {
            [a] => {
                let rows: Vec<J> = a.rows.iter().skip(offset).take(max).map(|(row, _)| row.clone()).collect();
                let total = J::Num(a.rows.len() as f64);
                let mut out = vec![("appendix", J::s(&a.title)), ("about", J::s(&a.about)), ("total", total)];
                out.push(("rows", J::Arr(rows)));
                out.extend(more(a.rows.len()));
                J::obj(&out)
            }
            _ => J::obj(&[("hint", J::s("Name one appendix by a part of its title.")), ("appendices", appendices())]),
        },
        [None, None, None, None] => {
            let n = |k: usize| r.commands.iter().filter(|(s, _, _)| *s == k).count() as f64;
            let titles = r.titles.iter().enumerate();
            let list = titles.map(|(k, t)| J::obj(&[("title", J::s(t)), ("commands", J::Num(n(k)))])).collect();
            let task = |(what, parts, start): &(&str, &[&str], &str)| {
                J::obj(&[
                    ("task", J::s(what)),
                    ("subsystems", J::Arr(parts.iter().map(|p| J::s(p)).collect())),
                    ("start", J::s(start)),
                ])
            };
            J::obj(&[
                ("what_the_scope_can_do", J::Arr(reference::TASKS.iter().map(task).collect())),
                ("hint", J::s(TASKS_HINT)),
                ("subsystems", J::Arr(list)),
                ("appendices", appendices()),
            ])
        }
        [None, None, words, subsystem] => {
            let found = reference::search(words.unwrap_or(""), subsystem);
            let list: Vec<J> = found.iter().skip(offset).take(max).map(|e| reference::brief(e)).collect();
            let mut out = vec![("total", J::Num(found.len() as f64)), ("commands", J::Arr(list))];
            let items = words.map(reference::items).unwrap_or_default();
            match (words, found.first()) {
                // The item a command takes (a measurement, an eye figure, an error code), with the appendix's text.
                (Some(_), _) if !items.is_empty() => out.push(("items", J::Arr(items))),
                (Some(_), None) => out.push(("hint", J::s(NOTHING_FITS))),
                // One subsystem listed: what the manual says before its commands, and its examples.
                (None, Some((s, _, _))) if found.iter().all(|(other, _, _)| other == s) => {
                    out.push(("about", J::s(&r.about[*s][0])));
                    out.push(("examples", J::s(&r.about[*s][1])));
                }
                _ => {}
            }
            if let Some(next) = more(found.len()) {
                out.push(next);
                out.push(("hint", J::s("More commands fit: ask again with offset = next_offset, or add words.")));
            }
            J::obj(&out)
        }
    };
    answer(run, data)
}

/// One line of CSV as its cells (a cell in quotes may hold commas, and "" is a quote).
fn csv_cells(line: &str) -> Vec<String> {
    let (mut cells, mut cell, mut quoted) = (Vec::new(), String::new(), false);
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', _) => quoted = !quoted,
            (',', false) => cells.push(std::mem::take(&mut cell)),
            (c, _) => cell.push(c),
        }
    }
    cells.push(cell);
    cells
}

/// The text of an IEEE definite-length block (`#<n><length><data>`), or the answer itself when it is none.
fn block_text(answer: &str) -> &str {
    let digits = answer.strip_prefix('#').and_then(|r| r.chars().next()).and_then(|c| c.to_digit(10));
    match digits {
        Some(n) if n > 0 => answer.get(2 + n as usize..).unwrap_or(""),
        _ => answer,
    }
}

/// The rows of the table `name` as the answer of a tool: Index first in every row, `max_rows` from `offset`.
fn table_rows(run: &mut Run, name: &str, offset: usize, rows: usize, extra: Vec<(&'static str, J)>) -> J {
    let text = run.q(&format!(":TABL:DATA? {name}"));
    let mut out = vec![("table", J::s(name))];
    match text.as_deref() {
        Some("BUSY") => {
            out.push(("pending", J::Bool(true)));
            out.push(("hint", J::s(DECODE_PENDING)));
        }
        Some(text) => {
            let mut lines = block_text(text).lines().filter(|l| !l.is_empty());
            let head = lines.next().map(csv_cells).unwrap_or_default();
            let all: Vec<Vec<String>> = lines.map(csv_cells).collect();
            let shown: Vec<J> = all
                .iter()
                .skip(offset)
                .take(rows)
                .map(|cells| {
                    let mut fields: Vec<(&str, J)> =
                        head.iter().zip(cells).map(|(h, c)| (h.as_str(), J::Str(c.clone()))).collect();
                    // a row with an Error is not ok: the flag follows the column of the Events pane
                    if let Some(e) = head.iter().position(|h| h == "Error") {
                        fields.push(("ok", J::Bool(cells.get(e).is_none_or(|c| c.is_empty()))));
                    }
                    J::obj(&fields)
                })
                .collect();
            let next = offset + shown.len();
            out.push(("pending", J::Bool(false)));
            out.extend(extra);
            out.push(("columns", J::Arr(head.iter().map(|h| J::s(h)).collect())));
            out.push(("total_rows", J::Num(all.len() as f64)));
            out.push(("offset", J::Num(offset as f64)));
            out.push(("rows", J::Arr(shown)));
            out.push(("next_offset", if next < all.len() { J::Num(next as f64) } else { J::Null }));
        }
        None => {}
    }
    answer(run, J::obj(&out))
}

/// What the selected decoder found out about its line, for `decode_results` and `setup_decoder`: `diagnosis`, the
/// sentences that say why it decoded nothing or mostly errors and what to change (empty when it is fine), and
/// `signal`, what it measured (the bit rate the edges suggest, the lowest and highest sample, the share of fields that
/// failed a check). Nothing while the decoder is still working.
fn diagnosis_fields(run: &mut Run) -> Vec<(&'static str, J)> {
    let Some(text) = run.q(":DEC:DIAG?").filter(|t| t != "BUSY") else { return Vec::new() };
    let lines: Vec<J> = text.lines().filter(|l| !l.is_empty()).map(J::s).collect();
    let figures = run.q(":DEC:DIAG:FIG?").unwrap_or_default();
    let n: Vec<J> = figures.split(',').map(|v| v.trim().parse::<f64>().map_or(J::Null, J::Num)).collect();
    let at = |k: usize| n.get(k).cloned().unwrap_or(J::Null);
    let signal = J::obj(&[
        ("bit_rate_hz", at(0)),
        ("min_v", at(1)),
        ("max_v", at(2)),
        ("error_fraction", at(3)),
    ]);
    vec![("diagnosis", J::Arr(lines)), ("signal", signal)]
}

fn decode_results(run: &mut Run, args: &J) -> J {
    let (offset, rows) =
        match (count(args, "offset", 0, 0, usize::MAX), count(args, "max_rows", ROWS_DEFAULT, 1, ROWS_MAX)) {
            (Ok(o), Ok(r)) => (o, r),
            (Err(e), _) | (_, Err(e)) => return refuse(&e),
        };
    let view = run.q(":DEC:EVEN:VIEW?").unwrap_or_default().to_ascii_lowercase();
    let mut extra = vec![("view", J::Str(view)), ("table_hint", J::s(DECODE_TABLE_HINT))];
    extra.extend(diagnosis_fields(run));
    table_rows(run, "EVENTS", offset, rows, extra)
}

fn table(run: &mut Run, args: &J) -> J {
    let (offset, rows) =
        match (count(args, "offset", 0, 0, usize::MAX), count(args, "max_rows", ROWS_DEFAULT, 1, ROWS_MAX)) {
            (Ok(o), Ok(r)) => (o, r),
            (Err(e), _) | (_, Err(e)) => return refuse(&e),
        };
    let Some(name) = args.get("name").and_then(J::str) else {
        let list = run.q(":TABL:LIST?").unwrap_or_default();
        let tables: Vec<J> = list
            .lines()
            .filter_map(|l| l.split_once(','))
            .map(|(n, what)| J::obj(&[("name", J::s(n)), ("what", J::s(what))]))
            .collect();
        return answer(run, J::obj(&[("tables", J::Arr(tables)), ("hint", J::s(TABLE_HINT))]));
    };
    table_rows(run, name, offset, rows, Vec::new())
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
        let keys: Vec<&str> = table.iter().map(|(k, _, _)| *k).collect();
        return Err(format!("{prefix}{key}: not known; the keys are {}", keys.join(", ")));
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
        // "acquisition" is taken for "acquire": the name an assistant tries first
        let parts = ["channels", "timebase", "trigger", "acquire", "acquisition"];
        if let Some((key, _)) = fields.iter().find(|(k, _)| !parts.contains(&k.as_str())) {
            return Err(format!(
                "{key}: not known; the keys are channels, timebase, trigger, acquire (acquisition is taken for it), and nothing else"
            ));
        }
    }
    if let Some(a) = args.get("acquire").or_else(|| args.get("acquisition")) {
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
        fields.retain(|(k, _)| k != "protocol" && k != "auto");
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
    let hint = J::s("The decode table is the Events pane: open it with :DISPlay:LOWer EVENTS (the scope: scpi tool); its rows with decode_results or table EVENTS.");
    let mut out = vec![("applied", J::Arr(applied)), ("decoder", decoder), ("decode_table", hint)];
    // auto: what it set up, then what the decoder says of its line (the acquisition it arms has not finished here: ask
    // decode_results, and scope_state for the trigger status)
    if matches!(args.get("auto"), Some(J::Bool(true))) && run.w(":DEC:AUTO") {
        let report = run.q(":DEC:AUTO?").unwrap_or_default();
        out.push(("auto", J::Arr(report.lines().map(J::s).collect())));
    }
    out.extend(diagnosis_fields(run));
    answer(run, J::obj(&out))
}
