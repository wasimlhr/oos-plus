# ADR 0075: A setting that is only off or on is one switch in the menus

- Status: proposed
- Date: 2026-11-22
- Category: ui-ux

Amends ADR 0008, which sets the look of the controls and says nothing of on/off settings.

## Context

A menu row draws its control from the setting's options (`Ui::ctl`, `ui/src/ui/menu_dialog.rs`): segmented buttons
when every option fits, `-` value `+` otherwise. A setting that is only off or on (Display of a channel, Invert, Fine
scale, Colour grade, Dots, the remote services and many more) became the pair `Off` `On`. Owners found the pairs heavy
(#55): two buttons for one state, the state only readable from which of the two is white, and a tap on the white one
does nothing.

## Decision

**A choice whose options are exactly Off and On is drawn as one switch (`Ctl::Switch`):** a field the width of the
control with the state as text on the left and a slide switch on the right. On, the whole field is white like a chosen
segment or an active button, the text dark, the track dark with a white knob on the right; off, the field stays dark,
the track grey with the knob on the left. A tap anywhere on the field switches the setting; the
knobs and keys step it as before. Every other choice keeps its buttons or its picker.

- The field fills the control as the buttons did, so the rows, the columns and the menu layouts stay as they are
  (ADR 0008: no empty areas).
- Colours are the named ones (`ACCENT`, `BTN`, `BTN_BORDER`, `DIM`, `DARK_TEXT`), the shapes round.
- The rule lives in `Ui::ctl` alone: a new on/off setting gets the switch without a line of its own.
- The layout lint checks that the state text and the track fit the control at every text size.

## Alternatives

- **A switch only, no text.** Smaller, but the state is then only a position, which is hard to read at a glance on the
  panel and in a screenshot.
- **Narrower controls, to fit more rows.** Would change the layout of every menu at once; the space a switch frees can
  be used later, menu by menu, where a page is crowded.
- **A check box.** Reads as a form on a computer, not as the state of an instrument; the switch is what the request
  asks for.

## Consequences

- Every on/off row of every menu changes its look at once; the comparison pictures change with this record.
- A tap on an on/off row now always switches it, also on the half where the button that was already lit stood.
- A choice with Off and On and a third option (such as `Off`, `2`, `3`, `4`) stays a row of buttons.

## References

`ui/src/ui/menu_dialog.rs` (`Ui::ctl`, `switch_parts`, `draw_switch`, `tap_menu`), `ui/src/ui.rs` (`Ctl`),
`ui/src/ui/lint/menu_layout.rs`, `ui/src/ui/menu_tests.rs` (`an_on_off_setting_is_a_switch_a_tap_switches`), issue #55,
ADR 0008.
