# ADR 0030: Studio offers what Default offers; the Add panel is the short way into the Measure page

- Status: accepted
- Date: 2026-11-18
- Category: ui-ux

## Context

Studio is a layout of its own: bars with pills, views that dock, and every menu as a sheet at the right of the plot.
A sheet is filled from a plan that says which tab each row of the menu goes to, and a row the plan leaves out is not
on the screen. Studio also has its own panel to add a measurement, and its own top bar.

An audit on 2026-11-18 compared the two layouts: the bars by hand, and the menus by a test that walks the 47 menus of
the Default layout and their 508 rows, in the states that change the rows. Row by row Studio was nearly complete (one
row was missing, the line that says where aliasing starts). The gaps were at the entrances:

- The Measure page of ADR 0005 could not be opened in Studio. With it went the only way into the measurement
  settings (reference levels, parameters, pairs, statistics, Clear all), the sources B, C and D of a measurement, and
  the list of what is selected.
- The trigger pill named the type only for the types without an edge, and never the sweep. The time pill had no delay
  and no zoom scale. The cards of the Default layout say all of it.
- The acquisition read-out opened the Acquire page, as in Default, but it was plain text, and nobody took it for a
  control.
- The Acquire sheet holds seven rows. Averaging on the hardware and hardware segments add more, and the page then
  opens as the dialog instead (read from the code, not seen on a unit).
- The Studio bar had a SIM button, which ADR 0007 took out of the Default bar.

## Decision

Every menu of the Default layout and every row of it can be reached in Studio: as a row of the sheet of that menu, in
the same dialog, or in a place that is named. A test holds this (`ui/src/ui/studio_parity_tests.rs`): a row that a
plan leaves out fails it until the row is placed or its other place is written down there.

What a bar of the Default layout says, the Studio bar says too: the read-out has the rate, the depth and the mode and
is a button; the time pill has the delay or the zoom scale; the trigger pill has the type and the sweep.

There is one Measure page for every layout, Studio included. Studio's Add panel stays as the short way to add a
measurement on one source with the plot in view: Add > Measurement and the Measure key open it, a button beside its
close button opens the page, and a button in the same place on the page leads back. Menu > Measure opens the page.

ADR 0007 holds for the Studio bar: what belongs to one menu is an entry of the Menu, so SIM has no button there.

## Alternatives

- Measure, Cursor and System buttons in the Studio bar, as in Default. The bar is the scarcest place, and it already
  shortens its labels to keep the read-out; Add and the gear lead to the three, and SIM leaves the bar for this
  reason.
- The Measure page in place of the Add panel. The page covers the plot (ADR 0005), and the panel is what Studio is
  for: the waveform stays in view while a measurement is added and shows at once as a badge.
- The LAN, WLAN and USB boxes in the Studio bar. Not built: they would take the room of the view buttons and the
  search. The state and the address are on System > Connectivity.

## Consequences

- A new row of a menu needs a place in the sheet's plan, or the test fails. That is the point.
- The pills have a second line only while both fonts fit a pill (the Normal text size). At the larger sizes the
  trigger pill has the type in front of the source instead, and the time pill keeps the scale alone.
- The Acquire sheet has two tabs, Acquire and Segments, and the way on to Horizontal is pinned under both: it is one
  tap further for the segments, and the sheet no longer changes into a dialog.
- A layout that is added later starts from the same rule, and the test is the place to add it.

## References

`ui/src/ui/studio_parity_tests.rs`, `ui/src/ui/studio.rs` (`studio_acq_chip`, `studio_trig_detail`,
`studio_time_detail`), `ui/src/ui/sheet.rs` (`sheet_plan`, `sheet_add_expand`), `ui/src/ui/meascatalog.rs`
(`catalog_lay`), ADR 0005, ADR 0007.
