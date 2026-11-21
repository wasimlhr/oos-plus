# ADR 0034: a decoder is a bus of its own, with its settings in one place

- Status: proposed
- Date: 2026-11-21
- Category: ui-ux

## Context

A decoder had its settings on four tabs of the Decode sheet of Studio: Decode (which one, on, protocol, lines),
Settings (what the protocol needs, and the two buttons for its results), Levels (the thresholds) and Bus. The last
one held the I2C lane of the plot (ADR 0030 placed the rows of Menu > Bus there), which is another thing than the
decoder that is open. On 2026-11-21 the owner had MIL-STD-1553 on a unit and compared the sheet with a bus dialog
that holds everything of one bus on one panel: he did not find the settings, took the tab "Bus" for them, and read
the decoder as a part of its channel, since the pill and the band name the channel beside the protocol. For that
protocol the Settings tab was empty but for the two buttons (host, the build of main of that day).

A sheet has seven rows a page and two actions under them (ADR 0030), and its rows are those of the menu in the
menu's order (ADR 0031). A UART has eleven rows, so one page for every decoder needs more than a new plan.

## Decision

A decoder is a bus: an object of its own that names channels as its inputs, and what belongs to it is reached from
its sheet without a tab that belongs to something else. It is done in steps, each a pull request:

1. The Decode sheet holds the decoder alone; the I2C lane has its own sheet. The results (Events pane, Export
   events) are under the first page. The thresholds come to the first page where they fit, and a page without a row
   is no tab: a decoder with few rows is one page. (This record's pull request.)
2. A label and a position of its band for each decoder, as settings with their rows in the Decode menu.
3. The pill and the band name the bus first (its number and protocol) and its inputs second.
4. The lines of a decoder as one group: source, threshold and polarity of a line side by side, and how the band
   shows its values.

## Alternatives

- A sheet that scrolls, so that every decoder is one page. A sheet never moves while it is used (the head of
  `sheet.rs`), and a row out of sight is a row not found; the steps above make the first page hold what is used first.
- The dialog of the Default layout in Studio for the decoders. It covers the plot (ADR 0030).
- Leaving the lane's rows in the Decode sheet under another tab name. The lane is no setting of a decoder; with its
  own sheet Menu > Bus opens what it names.

## Consequences

Menu > Bus opens the lane's sheet in Studio and no longer the Decode sheet. Which tabs the Decode sheet has depends
on the protocol: one page for MIL-STD-1553, three for a UART (with step 2 the label and the position of the band are
on Settings, so MIL-STD-1553 has two). Steps 2 to 4 add settings (ADR 0046: a row of
`ui/src/settings/`, its menu row, its remote command) and change what the pill and the band write; stored
identifiers (`dec_*` keys, `:DECode` and `:BUS<n>` commands) keep their names.

## References

`ui/src/ui/sheet.rs` (`sheet_plan`, `sheet_spec`), `ui/src/ui/sheet_tests.rs`
(`the_i2c_lane_has_its_own_sheet_and_the_gear_opens_a_short_main_sheet`), ADR 0030, ADR 0031 (ui-ux).
