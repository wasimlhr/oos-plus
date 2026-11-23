# ADR 0076: The top bar holds Display and Storage; Measure and Cursor are keys and start-menu entries

- Status: proposed
- Date: 2026-11-22
- Category: ui-ux

Supersedes the list of entries of ADR 0007; the rest of ADR 0007 (a function belongs to its menu, the Live button in
record mode) and ADR 0063 (the status boxes) stay as they are.

## Context

ADR 0007 kept four entries in the top bar: Measure, Cursor, Display and Storage. Measure and Cursor have their own
keys on the front panel (the keys are there on the DHO800/DHO900; their codes 0x0f and 0x10 are taken from
[frontpanel.md](../../protocols/frontpanel.md) and not yet verified on hardware) and their own entries in the start
menu, so the bar showed them a third time, on the scarcest
strip of a 1024 x 600 panel. The Display and Storage menus have no key, which is why they are in the bar at all.

The Measure button also had a press-and-hold: a short menu (`MenuId::MeasHold`) whose one row, Add meas, added the
next usual measurement when the finger was lifted on the button.

## Decision

**The top bar holds Display and Storage** (and the Live button before them in record mode, as before). Measure and
Cursor are reached with their front-panel keys and from the start menu.

**The press-and-hold moves to the Measure key:** held on the front panel for the long press of the channel keys
(`LONG_PRESS`), it adds the next usual measurement at once (Vpp, Freq, Period, Vrms, Vavg, Rise, then the rest), and
its release does nothing more. A short press opens the measurement list as before. The short menu and its row are
removed.

## Alternatives

- **Keep the four entries.** The bar stays as crowded as it is, for two functions that are a key press away.
- **Keep the press-and-hold on the screen** (on the measurement strip). A long touch on the strip already has other
  uses there; the key is where the measurements are opened anyway.

## Consequences

- The bar has room for its caption and the status boxes; the buttons left are wider at the larger text sizes.
- Measure and Cursor are one tap further away on the screen (Menu, then the entry); on the panel they are not.
- The long press of a key is one mechanism for the channel keys and Measure (`panel_key`, `ch_hold_poll`); another
  key with a long press is one line there.
- Scripts and the web remote press keys without a hold: the long press of Measure is a front-panel action only.
- Until the Measure key's code is verified on a unit, the start menu is the sure way to the measurement page; the
  long press is not yet verified on hardware either.

## References

`ui/src/ui/bars.rs` (`top_buttons`), `ui/src/ui/input.rs` (`panel_key`, `held_release`, `ch_hold_poll`),
`ui/src/ui/menu_dialog.rs` (`add_usual_meas`), `ui/src/ui/menu_tests.rs`
(`measure_key_lists_and_its_long_press_adds`), `ui/src/ui/lint/screen_layout.rs`, ADR 0007, ADR 0063.
