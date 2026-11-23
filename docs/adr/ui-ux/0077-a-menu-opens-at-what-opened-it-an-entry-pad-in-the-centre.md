# ADR 0077: a menu opens at what opened it; an entry pad opens in the centre, large

- Status: proposed
- Date: 2026-11-23
- Category: ui-ux

## Context

Where a dialog stands was never written down. A menu hangs under its top-bar button or above its card at the bottom,
else it is centred above the cards (`menu_anchor`). Since ADR 0076 the top bar has no Measure and Cursor buttons, so
those pages lost the place they had. The number pad (issue #22) opened beside its menu, small, with the entry in the
title line ("Scale · 1.5_").

Three placements were built and compared in the simulator (2026-11-23): today's, every dialog centred, and every menu
at one fixed place above the cards on the right with the pads centred. The channel, trigger and display menus are two
columns and about 840 px wide, over 80 % of the screen, so they cover most of the trace wherever they stand. Centred,
a menu covers the middle of the plot, which is what one watches while turning a scale or a level; at the right, it
covers the trigger level mark. Only the pads were clearly better centred and larger.

## Decision

A menu opens at what opened it: under its top-bar button, above its card, or at the place its page names; the cursor
page opens under the top bar, so the cursor readouts in the strip stay in sight. An entry pad (the number pad, the
generator's frequency, a hex value of a bus trigger) opens in the centre of the plot area on a dimmed screen, with
larger keys and the entry large in a field above them; until something is typed, the field shows the row's value dim.

## Alternatives

- Every dialog in the centre: one place to look, but it covers the middle of the trace while a value changes.
- Every menu at one fixed place (right, above the cards): the left of the trace stays free for narrow menus only,
  and the trigger level mark is covered.
- The pad over the whole screen, like the keyboard: the keyboard needs the width for its keys; the pad has twenty and
  is large enough at about 80 % of the width.

## Consequences

- One reads a typed value where it is typed, in large digits; the keys are easier to hit on the 7" panel.
- A new page that has no card and no top-bar button names its anchor in `PAGES`; without one it is centred above
  the cards.
- What would make the menus' place worth a second look: narrower, one-column menus. Then a fixed place could leave
  most of the trace free.

## References

`ui/src/ui/menu_dialog.rs` (`dlg_layout`, `pick_window`, `draw_menu`), `ui/src/ui/numpad.rs` (`pad_entry`,
`pad_now`), `ui/src/ui/menu_text.rs` (`menu_title`), `ui/src/ui/pages.rs` (`TOP_CENTRE`), `ui/src/ui.rs`
(`PAD_ENTRY_H`, `SHADE`); ADR 0076; issue #22.
