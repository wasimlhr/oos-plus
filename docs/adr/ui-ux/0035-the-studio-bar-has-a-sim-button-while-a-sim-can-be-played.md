# ADR 0035: the Studio bar has a SIM button while a SIM can be played, the twin of Live

- Status: proposed
- Date: 2026-11-22
- Category: ui-ux

Amends ADR 0007 (four entries and a Live button in record mode) for the Studio layout only.

## Context

SIM mode plays a signal as a live acquisition: the preset of the knobs, or the references that are shown (a loaded
file, a signal added to a slot). The Studio bar shows a chip with the preset's name while SIM is the source, and a
Live button, the way back, while any record is. Back in Live there was nothing in the bar to return to the SIM: it
is an entry of the Menu (Analysis), as ADR 0007 keeps it, so the way back was Menu, SIM, then Play reference (four
taps and a page). The owner, working on a captured signal of a faster scope that is a reference in R1, went to Live and
back several times in a session and asked on 2026-11-22 for a SIM button in the bar: "how do I go back to SIM mode
quickly".

Host only, on the pictures of the owner and on the host; the buttons were not tried on a unit.

## Decision

- **In the Studio layout, while the scope is live and a shown reference has samples, the bar has a SIM button** at the
  place where the preset's chip stands in SIM mode. One tap plays what is shown (the references, as Play reference
  does: `sim_panel_play`), and the bar turns into the SIM bar: Live and the preset's chip.
- **It is the twin of Live, not a fifth entry.** Live is there when a record is, as ADR 0007 has it; the SIM button is
  there when there is something to return to, a shown reference. Without one the bar is as before and SIM starts from
  the Menu.
- Minimal keeps its SIM button, which opens the SIM page; the Default layout is untouched.

## Alternatives

- **A SIM button always** (also with nothing to play). It would be a fifth entry for the owner's use of a captured
  reference only; with no reference it would play the knobs' signal at a tap, which is a different thing.
- **Remembering "the last SIM" as a mode** (a preset that was playing, a reference). More state to keep and to show
  for what the shown reference already says.

## Consequences

The bar's room: the button is as wide as a label of the tier, taken from the read-out of the acquisition like the
chip. A change of which references are shown changes the bar: a reference hidden or cleared takes the button away.

## References

`ui/src/ui/studio.rs` (`studio_top_tier`, `draw_studio_top`, `studio_top_tap`), `ui/src/ui/sim.rs`
(`sim_panel_play`), ADR 0007, ADR 0063.
