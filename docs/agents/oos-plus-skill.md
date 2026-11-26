# Skill: driving an OOS Plus oscilloscope (for agents)

Use this when you control an OOS Plus scope (Rigol DHO800/DHO900 running OOS Plus) over MCP or SCPI. It says what
the scope can do, the order to do it in, and the mistakes agents make most. Everything here is in the references at
the end; where this file and the SCPI reference differ, the reference holds.

## Connect

- MCP: `POST http://<scope>:8080/mcp` (tools: `scope_state`, `configure`, `acquire`, `measure`, `waveform`,
  `decode_results`, `setup_decoder`, `screenshot`, `scpi`, `scpi_reference`, `errors`). Raw SCPI: TCP 5555.
- Start with `scope_state`. Call `scpi_reference` with no arguments: it lists what the scope can do by task.
- Never guess a command. Find it with `scpi_reference` (search, then header), send it, read the value back.

## What the scope can do (beyond channels, timebase, trigger)

| Task | Subsystem | Start with |
|---|---|---|
| Arrange the screen: dock, float, move, resize, close views | Docking, Views | `:DOCK:LIST`, `:DOCK:MOVE`, `:DOCK:FLOAT`, `:DOCK:SIZE`, `:DOCK:SHOW` |
| Zoom, fit the record | Studio zoom | `:ZOOM:HERE`, `:ZOOM:BOX`, `:VIEW:FIT` |
| Simulate a signal and add it as a reference | Signal simulator | `:SIM:PRESet`, `:SIM:PLAY ON`, `:SIM:ADDRef` |
| Eye, spectrum, bode, mask, power, histogram | their own subsystems | `:EYE:...`, `:SPECtrum:...`, `:BODE:...` |
| Decode a bus and read its events | Decoders and bus | `setup_decoder`, `decode_results`, `:DECode:EVENts?` |
| Mark and find events | Search | `:SEARch:...` |
| Reference and math traces | Reference waveforms, Math | `:REFerence<n>:...`, `:MATH<n>:...` |
| Generator, logic analyser, cursors, DVM, counter | their subsystems | `:SOURce<n>:...`, `:LA:...`, `:CURSor:...` |
| Anything a finger does that no command covers | Screen controls | `:UI:ELEMents`, `:UI:TAP`, `:UI:MENU:SELect` |

After a change to the screen, take a `screenshot` and check it.

## Decoding a bus: the checklist (this is where agents fail)

A decoder needs the signal sampled finely enough. "Nothing decoded" almost always means a setting, not a fault.

1. **Sample rate.** Use at least 10 samples per bit (20 is better). A 1 Mb/s bus needs 10 to 25 MSa/s. MIL-STD-1553
   is 1 Mb/s Manchester, so use 10 MSa/s or more. A 25 kSa/s record (200 ms/div with 50 kpts) cannot decode it.
2. **Timebase.** Window = memory depth / sample rate. At 50 kpts and 10 MSa/s the window is 5 ms, about 500 us/div.
   Pick the timebase so the record holds the whole frame or message (1553: up to about 650 us).
3. **Not in roll mode.** Roll starts by itself on slow timebases (from 50 ms/div with Auto Roll on). Use a
   timebase of ms/div or faster, and Normal mode with a trigger on the bus.
4. **Threshold** between the two levels (idle 0 V and a 1553 swing of about 2.5 V: use about 1 V). Check the
   source: CH1 / CH2 or a differential math trace.
5. **Trigger** on the burst (edge level inside the signal, not in the idle noise), then `:SINGle` or `acquire`
   and wait until `scope_state` shows the trigger status STOP.
6. **Read the result** with `decode_results` when `pending` is false. While the scope runs, rows are those of the
   record on screen and change at the next acquisition: stop first for a complete list.

Typical bit rates, to choose the sample rate (check the real signal): UART up to 1 Mb/s and more, I2C 100 kb/s to
3.4 Mb/s, SPI up to tens of Mb/s, CAN 125 kb/s to 1 Mb/s (CAN-FD data phase up to 8 Mb/s), LIN 19.2 kb/s,
FlexRay 10 Mb/s, ARINC 429 12.5 or 100 kb/s, MIL-STD-1553 1 Mb/s, USB full speed 12 Mb/s. The decoder names and
what each takes are in `docs/guide.md` and `wiki/Features.md`.

## Measuring

- `:MEASure:ITEM? <item>,<source>` returns a value without adding a table row. The `measure` tool does the same.
- `:MEASure:ITEM <item>,<source>` adds a row to the measurement table. A source may be `CHANn`, `Rn` or `MATHn`
  (a math trace is read from a copy in a reference slot: save again after a new acquisition).
- Values are in base units: V, s, Hz. `9.9E37` means no value (no data, stopped scope without a record, or the
  measurement is still computing: check `:MEASure:PENDing?`).

## Waveform data

- Read samples only while the scope is stopped. Use the `waveform` tool (statistics and a CSV link, download it at
  once) instead of putting samples into the conversation. Raw SCPI: `:WAV:SOUR`, `:WAV:MODE RAW`, `:WAV:FORM BYTE`,
  `:WAV:PRE?`, `:WAV:DATA?`; volts = (byte - yorigin - yref) * yincrement.

## Rules

1. Look up, send, verify. A sent command is not a done command: a setting may be refused, rounded or limited.
2. After an error, read its hint; do not resend the same command. `:SYSTem:ERRor?` / the `errors` tool.
3. Do not change what the owner did not ask for. Read the set-up first and say what you changed.
4. If MCP is read only on the scope, do not try to change anything.

## References (absolute paths on the documentation machine)

- SCPI Programming Manual (PDF): `docs/OOS-Plus-SCPI-Programming-Manual.pdf`
- All commands as JSON (what the MCP `scpi_reference` reads): `docs/manual/commands.json`
- SCPI reference, one page per group: `docs/scpi/README.md` and the pages beside it
- User guide: `docs/guide.md`; features: `wiki/Features.md`; remote control: `wiki/Remote-Control.md`
- HTTP and MCP routes: `docs/http.md`
