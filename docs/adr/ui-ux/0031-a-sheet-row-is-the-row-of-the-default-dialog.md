# ADR 0031: a sheet row is the row of the Default dialog; every source has one handle on its zero level

- Status: accepted
- Date: 2026-11-18
- Category: ui-ux

## Context

Minimal and Studio are other screens on the same scope: the model, the menus and the commands are those of the Default
layout. ADR 0030 made sure every row of a Default menu can be reached in Studio. Two things had still drifted, and the
owner named both on 2026-11-18.

**How a row is set.** A sheet had its own kinds of row. A setting with a list of options was a value that opened the
list, without the minus and the plus the Default dialog has, so one step of the record length or of the bandwidth
limit took two taps and a look at a list. A choice was buttons only up to four options and only when they fit a column
of 224 px, so the four acquisition modes and Expand around were lists where Default shows four buttons. A walk of the
47 menus (476 rows, the states that change rows included) found 22 rows whose control differed from the dialog's for
these two reasons.

**How a source is marked in the plot.** A channel had a badge with its name at the left of its lane, on its ground
line. A reference had a badge in the middle of its lane in Stacked and a box with its name, scale and rate over the
trace in Overlay, and a math channel had none. With every source on one graticule (Overlay) nothing said where the
zero of a reference or of a math trace was; a badge at the bottom edge lay under the time labels, and the volt labels
were drawn over the badges of references. Host only; the unit was read (its build was older) but not changed.

## Decision

A row of a sheet has the control `Ui::ctl` gives its item, the same function the Default dialog asks: segments
wherever they fit the row (the column beside the labels, or for a choice that needs more, the room its own label
leaves), however many; where they do not fit, the picker of the dialog (minus, plus, and the value opens the list);
minus, value and plus for a number; a switch for Off and On. A sheet never shows a list alone. Inside a tab the rows
stand in the order of the menu, with the links to other pages last. What differs on purpose is listed with its reason
in `meant` (`ui/src/ui/studio_parity_tests.rs`), and a test fails for anything else.

In Studio every source that is drawn (channel, reference, math channel; not a spectrum) has one handle: an outlined
badge with its name at the left edge of its lane or of the one graticule, on its own zero level, in Stacked and in
Overlay. A level outside the plot pins the handle to that edge, above the time labels. Handles on the same level in
one graticule stand side by side in the order of the sources. A tap on a handle opens the sheet of its source.

## Alternatives

- The list-only row, kept. It is one control instead of three and looks calmer, but it is the drift the owner named:
  the same setting is set in two ways, and the common step costs a tap more.
- A wider sheet, so more choices fit as buttons. It takes the plot's width for every page, for the sake of a few rows.
- Handles of sources on one level stacked with an offset down the edge. Each would then stand off its zero level,
  which is the one thing a handle says.
- The name, scale and rate of a reference in a box over the trace, as Default and Minimal have. In Studio they are on
  the reference's pill in the bottom bar, and a box covers the trace it names.

## Consequences

- A sheet looks more like the dialog and less like a form of its own; the row with minus and plus is the usual one.
- Trigger > Type (Edge, Pulse, Slope, More), the swatches of a reference's colour and the one Shown switch of the
  reference slots stay as they are and are named as meant. Trigger > Coupling stays a picker: its four names do not fit
  the row.
- Acquire keeps its two tabs (ADR 0030). With averaging on a unit the first tab already has seven rows (Mode,
  Averages, HW average, Memory, ADC cores, Draw path, Aliasing; read from the code, not seen on a unit), so Segments
  cannot stand third, as it does in the dialog, without the page falling back to the dialog.
- `:UI:ELEMents?` lists the lanes (`LANE`) and the handles (`HANDLE`), so a program can check where a zero level is.
- A new kind of source drawn in the plot gets its handle from `studio_handle_rects`, not a badge of its own.

## References

`ui/src/ui/sheet.rs` (`sheet_row_kind`, `sheet_ctrl_w`), `ui/src/ui/studio.rs` (`studio_handle_rects`,
`studio_draw_handles`), `ui/src/ui/studio_parity_tests.rs` (`a_sheet_row_is_the_row_of_the_default_dialog`,
`drift_table`), `ui/src/ui/studio_trace_tests.rs`, ADR 0030.
