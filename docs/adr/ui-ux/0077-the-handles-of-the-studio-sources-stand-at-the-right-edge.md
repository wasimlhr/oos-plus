# ADR 0077: the handles of the Studio sources stand at the right edge

- Status: proposed
- Date: 2026-11-23
- Category: ui-ux

Supersedes the place of the handle in ADR 0031 (the left edge); the rest of ADR 0031 (one handle per source, on its
zero level, a tap opens the sheet, the names and the order of a sheet) holds.

## Context

The handles stood at the left edge of the plot, on the zero level of each source. On the owner's unit (2026-11-23) they
covered the volt labels there, and sources with the same zero level stood side by side to the right of the first, over
the beginning of the traces. The owner asked for them at the right edge. Host only: seen on a DHO924S, not yet
verified on hardware beyond that picture.

## Decision

A handle is an outlined badge with the name of its source at the right edge of its lane or of the one graticule, on its
zero level. A handle that would cover another handle, a corner button with its word, the trigger tag with its value or
a grip of the dock moves left until it is clear; sources on one level stand side by side, the first at the edge. A level
outside the plot pins the handle to the edge, above the time labels, as before. The volt labels at the left keep all
their room.

## Consequences

The right edge also carries the trigger tag, the corner buttons and the cursor tags, so a handle may stand a few badges
in from the edge where one of them is on its level. `studio_handle_rects` (`ui/src/ui/handles.rs`) is still the one
place that decides, and hit testing and dragging read it. A walk of a unit with five or more sources on one level
would show whether the row of badges needs a limit.

## References

`ui/src/ui/handles.rs`, `ui/src/ui/studio_trace_tests.rs`, ADR 0031.
