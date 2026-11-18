# ADR 0028: an external generator is a row of a table, one connection and a page of its own

- Status: accepted, to be reviewed
- Date: 2026-11-17
- Category: code

## Context

The Bode sweep could drive a generator beside the scope: on the LAN, or on the USB-A port over USBTMC. All of it lived
in the Bode page: the command sets as presets of its Generator row, the connection and the address as two more rows,
the thread that sends as a part of the sweep. Three things did not fit any more.

- The connection opened when a sweep started. Whether the generator was there, and which one, showed only after
  Start, as a sweep that stopped.
- The thread only wrote. A generator that refuses a value keeps its old one (an OWON DGE2070 does, and logs it in its
  error queue: seen on that unit with firmware V1.0.4.7.0, driven from a computer over USBTMC), so a sweep past its
  range drew a curve of a frequency that never changed. The sweep also measured without knowing whether the generator
  had its command yet, which a write to a socket hid and a query over USB does not.
- Other sweeps are to use the same generator. Each would have had to bring its own connection.

Not yet verified on hardware: any generator on the USB-A port of the scope. The USBTMC host side is tested on the
host against a simulated device.

## Decision

A generator model is a row of one table (`extgen::GENERATORS`): its command templates, its error query, how its
`*IDN?` answer starts. One thread owns the connection and sends batches of commands for whoever asks; after each
command it reads the error query of the model, and it counts the batches it is through with. The generator has a
page of its own, Menu > Instruments > Ext generator, with the model, the place, Connect, what the generator says it
is, and an output of its own. The Bode page keeps one choice: Built-in, External, or a free-running sweep.

## Alternatives

- **More rows in the Bode page** (Connect, state, identity, load, channel). Five more rows in a page that is about
  a sweep, and the next sweep would repeat them.
- **A driver trait with one type per model.** The models differ in the text of five commands. A table of templates
  says that in one line each and is tested by comparing strings; a trait would be code per model for the same text.
- **`*ESR?` after every command**, as SCPI has it. The OWON DGE never answers it: the read times out and the
  generator logs an error of its own. The error query is a field of the model instead, empty for a model that has
  none or whose queue is not known.
- **Reading every setting back.** It is what tells a refused value from an accepted one for sure, and it costs a
  query per setting and a comparison of numbers in the generator's format. The error queue answers the same question
  on the models that have one; a read-back can be added per model when one needs it.

## Consequences

- The setup file keeps the index of the row, as it did for the presets: rows are added at the end and never moved.
  A file from before the page names the external generator in its `bode` line only, and is read as that model.
- A sweep waits for the thread to be through with the commands of a step, then counts its settling records. A slow
  generator makes the sweep slower, not wrong.
- A sweep that finds no connection opens one and closes it at its end, as before; one opened by Connect stays open.
- The other models (APPL, BSWV, SCPI, CHANnel) have no error query: nothing is known about their queues from a
  unit. They behave as before, and a value they refuse is still not noticed.
- The page talks to an instrument that is not the scope. It is a client: it opens no port, so it has no switch under
  Remote services (ADR 0025).
- Worth a second look once a generator has run on the USB-A port of a unit: the pause between commands, whether the
  error queue is enough, and whether more instruments than a generator want this connection.

## References

`ui/src/extgen.rs` (models, connection, thread), `ui/src/ui/extgen.rs` (page), `ui/src/ui/bode.rs` (sweep),
`ui/src/usbtmc.rs`, [docs/extgen.md](../../extgen.md), [docs/scpi/extgen.md](../../scpi/extgen.md).
