# ADR 0031: every source has one handle on its zero level; a sheet keeps the names and the order of the menu

- Status: accepted
- Date: 2026-11-18
- Category: ui-ux

## Context

Studio draws its sources in one of two ways, chosen in the plot's menu: Stacked traces gives every source a lane with
its own scale, Overlay traces puts every source on one graticule of the full height, each with its own position and
scale. The owner reported on 2026-11-18 that Overlay does not overlay properly, and described what both should look
like: a handle with the name of the source on its zero level, in a lane as on the one graticule.

A walk of the Studio controls on the host (87 controls and functions over SCPI, 460 sheet rows by touch) showed where
the screen differed from that:

- A channel had a badge with its name at the left of its lane, on its ground line. A reference had a badge in the
  middle of its lane in Stacked and a box with its name, scale and rate over the trace in Overlay. A math channel had
  none. With every source on one graticule nothing said where the zero of a reference or of a math trace was.
- A badge at the bottom edge lay under the time labels, badges of sources on one level lay on top of each other, and
  the volt labels were drawn over the badges of references.

The same walk compared the sheets of Minimal and Studio with the Default menus. ADR 0030 already holds that a sheet
has every row and gives it the control of the dialog. Names and order were not held by anything: the cursor rows of
Minimal, which are one tab there, were not in the order of the menu.

Host only. A DHO924S was read (its build was older) but not changed.

## Decision

In Studio every source that is drawn (channel, reference, math channel; not a spectrum) has one handle: an outlined
badge with its name at the left edge of its lane or of the one graticule, on its own zero level, in Stacked and in
Overlay. A level outside the plot pins the handle to that edge, above the time labels. Handles on the same level in
one graticule stand side by side in the order of the sources. They are drawn after every trace. A tap on a handle
opens the sheet of its source.

A sheet row has the name of its row in the Default menu, and inside a tab the rows stand in the order of the menu,
with the links to other pages last. What differs on purpose is listed with its reason in `meant`
(`ui/src/ui/studio_parity_tests.rs`), and a test fails for anything else, in Minimal and in Studio.

## Alternatives

- Handles of sources on one level stacked with an offset down the edge. Each would then stand off its zero level,
  which is the one thing a handle says.
- The name, scale and rate of a reference in a box over the trace, as Default and Minimal have. In Studio they are on
  the reference's pill in the bottom bar, and a box covers the trace it names.
- A badge of its own for each kind of source, as before. Three places decided where a name goes, and two of them
  were not on the zero level.

## Consequences

- `:UI:ELEMents?` lists the lanes (`LANE`) and the handles (`HANDLE`), so a program can check where a zero level is.
- A new kind of source drawn in the plot gets its handle from `studio_handle_rects`, not a badge of its own.
- A row that is renamed on a sheet, or moved up inside a tab, needs its reason in `meant`.
- In a short last lane the Fit row and the time labels leave the handle little room: it is pinned above them, off
  the zero level, as it is for a level outside the plot.

## References

`ui/src/ui/studio.rs` (`studio_zero_y`, `studio_handle_rects`, `studio_draw_handles`),
`ui/src/ui/studio_trace_tests.rs`, `ui/src/ui/studio_parity_tests.rs`
(`the_sheet_rows_have_the_names_and_the_order_of_the_default_menus`), ADR 0030.
