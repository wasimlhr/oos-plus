# ADR 0032: Studio has the zoom row and the zoom window of the Default layout

- Status: accepted
- Date: 2026-11-18
- Category: ui-ux

## Context

Zoom shows an overview of the record on top and the zoomed part under it. The Default layout has one row between the
two: the text "Zoom", the time per division, the position and the vertical factor at the left, then the buttons H-,
H+, V-, V+, Box, Reset and Close. The window in the overview is a slim frame with a handle at each end.

Studio had controls of its own: a small tab on the zoomed lanes ("Zoom 10x") that opened a toolbar over them when it
was tapped or pulled down and shut again after four seconds (a field for the time per division, plus, minus, Y plus,
Y minus, a read-out, Box, Center trigger, Fit record, a menu named Zoom & navigation, close); as a second setting the
same toolbar in the title strip of the waveform; a row in Horizontal to choose between the two; and in the overview a
shaded window with wide grips.

The owner compared the two on 2026-11-18 and prefers the Default one: "its cleaner and the box is slimmer". It is
also the case ADR 0030 is about: the same function was set in two ways.

The row, the window and the Center trigger row of the plot's menu were walked on a DHO924S over SCPI (every button
pressed, the answer read back).

## Decision

Studio draws the zoom row of the Default layout, in the same place and with the same code (`zoom_controls`,
`draw_zoom_readout`), and the same window in the overview. The pull-down tab, the toolbar in the title strip, the menu
Zoom & navigation and the Zoom bar row of Horizontal are removed.

What only Studio has stays beside the row, because the row has no place for it:

| Studio only | Where it is |
|---|---|
| The height of the overview | its grip at the lower edge of the overview |
| The zoom source and the zoom of one lane | the menu of the Zoom button (hold or right click) |
| The Y zoom of one lane | a tap on the lane's handle while zoomed |
| Fit record, Reset zoom, Close zoom | also in the plot's menu |
| Center trigger | a row of the plot's menu while zoom is on (the owner, 2026-11-18); `:TIMebase:DELay:CENTer` |

## Alternatives

- The Default row as a third setting of Zoom bar. Three forms of one control, and the owner would still meet the
  other two.
- Studio's toolbar with the labels of the Default row. It kept the tab that hides the controls until it is pulled,
  which is what made the Default row the cleaner one.
- Center trigger as an eighth button of the row. The row would differ from the Default one again; it is a row of
  the plot's menu, beside Reset zoom and Close zoom.

## Consequences

- The zoomed lanes lose 26 px to the row (it was 6 px with the tab), as in the Default layout.
- `studio_zoom_bar` only names the buttons of the row for the code that needs them (the grip that keeps clear of
  them, the tests).
- The setup key `studio_zoom_tab` is still read and written, so a set-up of an older build loads; it changes nothing.
- A test compares the row of Studio with the row of Default pixel for pixel
  (`studio_has_the_zoom_row_and_the_window_of_the_default_layout`).
- The view column of the plot's menu has nine rows while zoom is on, Center trigger among them
  (`the_plot_menu_centres_the_trigger_in_the_zoom_window` checks that the menu fits the screen).
- The toolbar of the pull-down tab had been made a finger high (44 px) the day before. The buttons of the zoom row
  are 28 px high, as in the Default layout. If they are to be a finger high, they grow in every layout.

## References

`ui/src/ui.rs` (`zoom_controls`, `zoom_readout`, `draw_zoom_readout`, `split_zoom`), `ui/src/ui/studio.rs`
(`studio_zoom_bar`), `ui/src/ui/studioctx.rs` (Center trigger), `ui/src/ui/studio_trace_tests.rs`, ADR 0030.
