# ADR 0082: a measurement slot is a number for a row of the table; an empty slot is an error

- Status: proposed
- Date: 2026-11-26
- Category: code

## Context

`:MEASure:ITEM? VPP,CHAN1` computes a value and leaves no trace: nothing is added to the measurement table, so the
person at the scope never sees a number an assistant (over MCP) or a script has used. `:MEASure:ITEM VPP,CHAN1` adds the
row, but the row is then found by its place in `:MEASure:STRip?`, which moves when another row is removed, and by
`:MEASure:ITEM? VPP,CHAN1` again, which computes it a second time. The owner asked on 2026-11-26 for the logic of a
measurement slot: a measurement is added to a slot first, so it is computed and is a row of the table on the screen,
and it is read back from the slot by its number. Host only: the slots were not tried on a unit.

Rows are stored as lists of what they measure: `Model::meas` (item, source), `meas2` (pair item, A, B) and `cat`
(catalogue rows). Per-row settings (`row_cfg`) are already kept against a `RowKey`, the identity of a row by what it
measures; the setup file writes them for the rows that are still selected.

## Decision

`MEAS<n>` (n = 1 to 99) is a number kept in `Model::meas_slots`, a list of `(n, RowKey)`, one per view like the rows.
A row has at most one number and a number at most one row.

- The number is bound to what the row measures, not to its place, so it stays when other rows come and go. A row that
  leaves the table (removed, cleared, `:MEASure:CLEar`) takes its number with it; a slot whose row is gone reads as
  empty even before the list is tidied.
- `:MEASure:SLOT<n> <item>[,<A>[,<B>]]` adds the row as `:MEASure:ITEM` does (same items, sources, errors) and gives
  it the number. A slot that is in use gives its old row up: the row leaves the table, so a slot is always one
  measurement. The row shows the badge `M<n>` in front of its name in every table that lists rows.
- `:MEASure:SLOT<n>?` answers the value of the row as the table shows it; `:MEASure:SLOT<n>:ITEM?` what it measures
  (`VPP,CHAN1`); `:MEASure:SLOT<n>:CLEar` empties the slot and removes the row; `:MEASure:SLOT:LIST?` lists the slots
  in use.
- A read of an empty slot is an error (`-221`, saying which slot and how to fill it), never `9.9E37`, which keeps its
  meaning "a row that exists has no value yet" (`:MEASure:PENDing?`). A query that fails answers with its error text in
  this interpreter, so a client sees the reason at once.
- `:MEASure:ITEM? <item>,<source>` is unchanged: it still computes without a row.
- The setup key `meas_slots` (`n|row`, as `row_cfg` writes a row) keeps the numbers of the rows that are selected.
  Catalogue rows have no slot yet (a catalogue row is not an `:ITEM` item).
- The MCP tool `measure_slot` is the typed way: add (`slot`, `item`, `source`, `source2`), read (`slot` alone) and
  `clear`. It can change the scope, so MCP in its read-only mode does not offer it. `measure` stays and says it adds no
  row.

## Alternatives

A slot as a position in the strip (`STRip` index) was rejected: the number would change when a row above it is removed.
A slot as a second list of its own with its own copy of the item and sources was rejected: two lists would have to be
kept equal to the table, and "appears as a row of the table" would be a promise instead of a fact. A read of an empty
slot that answers `9.9E37` was rejected because the callers cannot tell it from a measurement that is still computing.

## Consequences

Rows added the old way (`:MEASure:ITEM`, the menu) have no number, and a row can be given one later by
`:MEASure:SLOT<n>` with the same item and sources (the row stays, with its statistics). A math source is a row of the
reference slot that holds a copy of the math result, so `:MEASure:SLOT<n>:ITEM?` answers `R<k>` for it, as
`:MEASure:STRip?` does. Worth a second look when catalogue rows are to be numbered too.

## References

`ui/src/ui/slots.rs`, `ui/src/scpi/measure.rs` (`meas_slot_cmd`), `ui/src/site/mcp.rs` (`measure_slot`),
`docs/scpi/measure.md`, ADR 0074.
