# ADR 0074: MCP offers typed tools, each a sequence of commands for the one SCPI interpreter

- Status: proposed
- Date: 2026-11-19
- Category: code

A record of oos-plus (README: a new record of the fork takes a number upstream has not used). Touches the remote
interface (ADR 0022, ADR 0025, ADR 0055).

## Context

The web remote answers MCP on `POST /mcp` (`ui/src/site.rs`, docs/http.md) with two tools: `scpi`, one program message
for the interpreter, and `screenshot`. An assistant that drives the scope through them has to know the dialect of
[scpi.md](../../scpi.md) by heart, guess names (it does: `:CHAN1:VDIV`, `:MEAS:VPP?`), read the error queue on its own,
and turn the binary block of `:WAVeform:DATA?` into numbers it cannot parse from a text item. A long record does not fit
into a model's context, and a picture of the screen is the only way it has to see the signal.

What MCP itself offers for this (specification 2025-06-18): a tool has an input schema (with `enum` for a closed set of
words), annotations (`readOnlyHint`, `destructiveHint`, `idempotentHint`), and may answer `structuredContent`, JSON
beside its text.

What the screen allows: one request is answered in one pass of the frame loop (ADR 0066), so a tool cannot wait for a
trigger; the interpreter's waveform selection (`:WAVeform:SOURce`, `MODE`, `FORMat`, `STARt`, `STOP`) is one for every
client.

## Decision

MCP offers typed tools beside `scpi` and `screenshot`. Each one is a fixed sequence of commands for the one interpreter
(`Scpi::exec_message_checked`), with its arguments checked against a schema before a command is built, the errors each
command queued in its answer, and JSON (`structuredContent` and the same as text) instead of SCPI text. A tool adds no
behaviour that a command of the interpreter does not have; a fact a tool needs and no command answers gets its command
first.

The tools of this record:

| Tool | Changes the scope | What it is |
|---|---|---|
| `scope_state` | no | channels, timebase, trigger and acquisition in one answer |
| `measure` | no | items of the measurement list for a source (pairs: two), as numbers; pending values as `null` |
| `waveform` | no | the statistics of a record and a link to a CSV of it (samples or min/max pairs); the interpreter's waveform selection is left as it was |
| `decode_results` | no | the events of a decoder slot as rows, a limited number per call |
| `errors` | no (empties the error queue) | the error queue as a list |
| `scpi_reference` | no | a command of the SCPI reference, found by what it does or by its header, with its syntax and parameters |
| `configure` | yes | channels, timebase, trigger and acquisition from one object; answers what was applied, read back |
| `acquire` | yes | run, stop, single, force, autoset, clear; answers the trigger status at once, it does not wait |
| `setup_decoder` | yes | protocol, input, sources, threshold and state of a decoder slot |
| `scpi` | yes | as before, and a list of messages that stops at the first error |
| `screenshot` | no | as before |

`scpi_reference` is the one tool that does not ask the interpreter: it answers from the command chapter of the
programming manual (`docs/manual/commands.json`, written from docs/scpi/ and checked against it), built into oos-ui. A
model that does not know a command guesses one that looks right (`:CHAN1:VDIV`); the reference is what the dialect is,
and `tools/scpi-coverage.py` keeps it and the interpreter alike. A command answered -113 makes a tool name
`scpi_reference` in its answer.

Every tool carries the annotations of MCP: `readOnlyHint` for those that do not change the scope. The words a tool takes
from a closed set (measurement items, protocols, trigger types) are an `enum` of its schema, read from the tables of the
interpreter, so a client sees them in `tools/list` and does not guess.

Limits, so that one call cannot take the scope's memory or a model's context: `waveform` puts no samples into its
answer by default (at most 1000 when asked), but a link to a CSV of up to 100000 rows (10000 by default) that the web
remote serves (`GET /mcp/waveform/<file>`, a file per call, the last 4 kept for 2 minutes) and a client downloads to its computer; it reads at most 1 Mi
samples of a record per call, saying where the next call starts. A record in the answer fills the context of a small
local model at once (seen with a 4B model and 500 min/max pairs);
`decode_results` at most 2000 rows (200 by default). MCP as a whole answers only a peer in `net.allow` (ADR 0025).

What MCP offers is a setting of the scope, Remote services > MCP (`remote.mcp`): **Full** (the default, every tool),
**Read only** (the tools that only read, and `scpi` for messages whose every command is a query), or **Off**. It is a
setting on the scope and not an argument of a call, because a client chooses its arguments: the owner lends the bench
to an assistant in Read only and knows that it changes nothing, whatever the assistant sends.

## Alternatives

- **Only `scpi`, with better instructions.** Leaves the client to know the dialect and to decode binary blocks; the
  errors that come from guessed names are the most common failure seen when an assistant drives the scope.
- **Tools that call the model directly (`Ui`), not the interpreter.** A second remote interface with its own meaning of
  each setting, and a second place to keep in step with scpi.md (ADR 0022 rules it out).
- **A tool per command (`set_channel_scale`, ...).** Hundreds of tools; a model chooses worse from a long list than
  from ten tools with schemas.
- **A server on a computer that speaks SCPI to the scope.** Works with any firmware, but every user needs it installed;
  the knowledge layer (search in the reference, recipes) belongs there, the typed tools belong next to the interpreter.

## Consequences

- The names and arguments of these tools are part of the remote interface: docs/http.md lists each with its mark
  (ADR 0055), `tools/http-coverage.py` checks that every tool has its entry, and a released name does not change.
- A tool's answer follows the interpreter: when a command's answer changes, the comparison of the SCPI script shows it,
  and the tool's tests show what the tool makes of it.
- `acquire` does not wait: a client asks `scope_state` (its `trigger.status`) until a single shot stopped. Waiting
  would need an answer that leaves the pass of the frame loop, which the transport does not have.
- The tools are offered to upstream; until they are taken, oos-plus carries them in `ui/src/site/mcp.rs`, apart from
  `site.rs`, so that a merge from upstream meets them in one place.

## References

- `ui/src/site/mcp.rs`, `ui/src/site.rs`, docs/http.md (MCP), [scpi.md](../../scpi.md)
- ADR 0022, ADR 0025, ADR 0055, ADR 0066
- Model Context Protocol, specification 2025-06-18: tools, annotations, structured content
