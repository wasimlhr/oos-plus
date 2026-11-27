# Task: reproduce a MIL-STD-1553 decode view on the OOS Plus scope

A Tektronix scope shows a MIL-STD-1553 capture like this (described from the owner's screenshot). Reproduce the same
view, with the same information, on the OOS Plus scope (Rigol DHO924S running OOS Plus) with the 1553 signal that is
on its CH1 (and CH2, which carries the same bus with the other polarity).

## What the Tektronix view shows

1. **A long record**: the whole capture is on screen as an overview at the top with a box marking the part that is
   zoomed below. The overview holds about 70 packets (bursts of words), so the record is long (many ms to seconds),
   not one burst.
2. **The zoomed waveform**: CH1 at 650 mV/div, horizontal about 5 us/div in the zoom (200 us/div for the record),
   showing the bit waveform of one word.
3. **A decode lane** under the waveform: boxes for the fields of the words ("Status:000-000-00001", "P:0", "Data",
   "Data:0008h", "P:0"), on the time of the bits they cover.
4. **A decode table** ("Bus Decode Results", "70 packets decoded") under the decode lane: one row per word, columns
   Index, Start Time, Type (Command, Status, Data, Command/Status, Unknown), Remote Terminal Address, Transmit/
   Receive, Sub Address/Mode, Mode Code, Status, Data Word Count, Data (hex), C/S payload, Error (for example
   "Incomplete packet; Unframed", "Data: Noncontiguous Data"). The table is readable without the waveform.
5. A single acquisition (stopped), trigger on the burst.

## Done means

- A screenshot of the OOS Plus screen (the `screenshot` tool) shows: a long record with a zoom window on part of
  it, the decode lane with field boxes over the zoomed bits, and the decode table with many rows and the 1553
  columns, with several packets, not one.
- `:DECode:EVENts?` (stopped) answers more than one word row, and the number of rows is stated.
- Say honestly which of the four items you could not reproduce and why, with the exact error text you got. Do not
  claim a view you did not see on a screenshot.

## How you work

- Control the scope only through its MCP at `http://192.168.1.158:8080/mcp` (JSON-RPC `initialize`, `tools/list`,
  `tools/call`; use `curl`) and the skills of the repository `C:\pw\Probewright-MCP-Master-src` (folder
  `skills/vendors/rigol/`: the `oos-plus-*` and `oosplus-*` skills, and the core skills). Read the skills first.
  Do not read the OOS Plus source code or ask the person who wrote this task for hints: the point is to see whether
  the instructions and the MCP are enough.
- The owner's own setup is on the scope: read it first (`scope_state`) and put back what you changed when you
  finish (or say what you left changed).
- Never write to anything but the scope's set-up (no firmware update, no files on the scope).
