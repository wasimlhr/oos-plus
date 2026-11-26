# Probewright skills mapped to OOS Plus

Probewright (`C:\pw\Probewright-MCP-Master-src\skills`) has 26 core skills (vendor-neutral) and a folder per vendor: Tektronix 73, Rohde & Schwarz 29, Keithley 23, Keysight 22, Siglent 14, and for Rigol only 6 (DHO and DS1000Z, three each: scope, measurement-pass-fail, troubleshooting). No skill mentions OOS Plus by name. Its command catalogue has an OOS-plus family (`public/commands/oosplus_dho.json`), but agents have no playbook for it: that is why they do not know what OOS Plus can do.

## 1. The 26 core skills and what each needs for OOS Plus

| Core skill | Lines | Needs | What to add for OOS Plus |
|---|---|---|---|
| mcp-mastery | 549 | OOS overlay | OOS has its own MCP on :8080 (scope_state, configure, acquire, measure, measure_slot, waveform, decode_results, setup_decoder, scpi_reference, screenshot, scpi, errors) next to Probewright's command_router: say which to use and what read-only mode does |
| probewright-agent-discipline | 142 | as is | none |
| instrument-operating-baseline | 209 | as is | add: write to a scope only when the owner asked in this session |
| probewright-mcp-direct-access | 129 | as is | none |
| scpi_gotchas | 82 | OOS overlay | OOS parses short and long forms and the <n> suffix, limits on *OPC?, the -113 hint, read back after every set |
| ieee488-2-common | 120 | as is | *IDN? identifies OOS, not Rigol; *OPT? is hardware options only (LA, AFG) |
| instrument-error-recovery | 95 | OOS overlay | :SYSTem:ERRor?, the error-code appendix, -221 reasons, BUSY answers |
| scope-setup | 97 | OOS overlay | channel and timebase rows, roll mode from 50 ms/div, sample rate = memory / window |
| scope-triggering | 92 | OOS overlay | the 13 trigger types, :TRIGger:BUS and the protocol trigger of each decoder |
| waveform-transfer | 92 | OOS overlay | :WAV: preamble scaling, only while stopped, the MCP waveform CSV link, MATH and R sources |
| scope-screenshot-save | 75 | OOS overlay | :DISPlay:DATA?, MCP screenshot, save and load of setups, references and waveforms |
| measurements | 80 | OOS overlay | :MEASure:ITEM set vs query, gate, levels 10/90 or 20/80, statistics, MATH sources, slots MEAS1..99 |
| scope-analysis-tools | 89 | OOS overlay | cursors (manual, track, XY), zoom, history, math and references, search marks, statistics |
| serial-bus-decode | 78 | OOS overlay | :DECode: and :BUS<n>: per protocol, sample-rate rule, events, BUSY |
| usb-pd-decode | 109 | OOS overlay | the USB PD decoder on CC: sample rate and threshold |
| mask_test | 63 | OOS overlay | :MASK: plus histogram and zones |
| fft_analysis | 76 | OOS overlay | :SPECtrum:, FFT math, window and span rules |
| jitter_ssc | 67 | OOS overlay | :EYE: jitter figures, TIE, RJ/DJ, SSC, the burst-bus eye |
| highspeed-serial-compliance | 80 | OOS overlay | simulator lanes (PCIe 4.0 ...), eye, mask; what OOS can and cannot certify |
| power-analysis | 206 | OOS overlay | :POWer: Power, Magnetic, Inverter, Double pulse |
| power-sequencing | 152 | as is | loads and supplies, not the scope |
| am_modulation | 56 | as is | use the measurement and FFT skills |
| emi-precompliance | 93 | as is | spectrum analyser work; OOS has the scope FFT only |
| engineering-report | 119 | as is | none |
| pymeasure-drivers | 81 | as is | none (no driver for OOS) |
| script-packages | 61 | as is | PyVISA over raw TCP 5555 works |

## 2. New OOS Plus skills (a Tektronix-sized set): 35

Proposed folder: `skills/vendors/rigol/` with the prefix `oos-plus-` (or a new `oos-plus` vendor folder), the same frontmatter as `rigol-dho-scope` (name, description, triggers, vendors, families, instrument_types, version). Each is written from the documents named in the last column, never from memory.

| Skill | Kind | Covers | Written from |
|---|---|---|---|
| oos-plus-scope | master playbook | identify, connect (raw 5555, HiSLIP 4880 desktop app only, VXI-11, HTTP/MCP 8080), a subset of Rigol not all of it, *RST, *OPT? | docs/scpi/README.md, common.md, system.md |
| oos-plus-mcp-tools | tool routing | each MCP tool, when to use which, read-only mode, the task list of scpi_reference | ui/src/site/mcp.rs, docs/http.md |
| oos-plus-vs-stock-rigol | differences | which Rigol commands are not implemented, OOS extensions, name differences | docs/scpi/README.md (compatibility limits) |
| oos-plus-vertical-timebase | setup | channels, probe, offset; timebase, roll, delay, sample rate and memory | docs/scpi/channel.md, timebase.md |
| oos-plus-run-control | acquisition | run, stop, single, autoset, modes, averaging, history, segments | docs/scpi/acquire.md |
| oos-plus-triggering | triggers | 13 trigger types, holdoff, sweep, bus trigger | docs/scpi/trigger.md |
| oos-plus-waveform-transfer | data | :WAV: and MCP waveform, raw vs screen, scaling | docs/scpi/waveform.md |
| oos-plus-measurements | measure | items, gating, levels, statistics, pairs, MATH sources | docs/scpi/measure.md |
| oos-plus-measure-slots | measure | MEAS1..MEAS99: add, read back, table badge | PR 215, docs/scpi/measure.md |
| oos-plus-measurement-pass-fail | limits | limit test on measurements | docs/scpi/measure.md, mask.md |
| oos-plus-cursors-dvm-counter | cursors | manual, track and XY cursors, DVM, counter | docs/scpi/cursor.md |
| oos-plus-math-reference | traces | math operators, filters, FFT, reference slots, save and load | docs/scpi/math.md, reference.md |
| oos-plus-search-mark | search | search types, marks, navigation, events table | docs/scpi/search.md |
| oos-plus-decode-setup | decode core | slots, source, threshold, the sample-rate rule, events, BUSY, protocol trigger, four decoders at once | docs/scpi/decode.md, protocol-modules.md |
| oos-plus-decode-uart | decoder | UART, UART 9-bit, RS-485 differential, baud, parity | docs/scpi/decode.md |
| oos-plus-decode-i2c-spi | decoder | I2C, SPI, Quad-SPI, I3C, 1-Wire, DS2482 | docs/scpi/decode.md |
| oos-plus-decode-can-lin-flexray | decoder | CAN, CAN-FD, LIN (rate from sync), FlexRay (TSS trigger), SENT | docs/scpi/decode.md |
| oos-plus-decode-1553-arinc | decoder | MIL-STD-1553 (1 Mb/s, word table), ARINC 429 | docs/scpi/decode.md, guide.md |
| oos-plus-decode-usb-pd | decoder | USB low/full speed, USB PD | docs/scpi/decode.md |
| oos-plus-decode-audio-industrial | decoder | I2S, S/PDIF, DMX512, Modbus RTU and ASCII, Manchester, MLT-3, parallel, C-PHY | docs/scpi/decode.md |
| oos-plus-decode-debug-dcc | decoder | SWD, JTAG, NMRA DCC with the layout view | docs/scpi/decode.md |
| oos-plus-eye-jitter | analysis | eye, TIE, RJ/DJ, bathtub, burst buses, clock recovery | docs/scpi/eye.md, reference-analysis.md |
| oos-plus-spectrum | analysis | spectrum, FFT, markers, window, span | docs/scpi/spectrum.md |
| oos-plus-mask-histogram-zones | analysis | mask test, histogram, zones | docs/scpi/mask.md |
| oos-plus-power-analysis | analysis | power, magnetic, inverter, double pulse | docs/scpi/power.md |
| oos-plus-bode-bandwidth | analysis | bode plot, bandwidth test | docs/scpi/bode.md, docs/bandwidth-test.md |
| oos-plus-generator | source | built-in generator, external generator | docs/scpi/generator.md, extgen.md |
| oos-plus-simulator | source | SIM presets, patterns, jitter, spread, add to reference | docs/scpi/sim.md |
| oos-plus-logic-analyser | LA | LA lines, thresholds, buses | docs/scpi/logic.md, wiki/Logic-Analyser.md |
| oos-plus-screen-layout | UI | dock, float, move, zoom, views, right-click menu, :UI: finger driving | docs/scpi/layout.md, display.md |
| oos-plus-display-screenshot | display | persistence, grid, labels, screenshot | docs/scpi/display.md |
| oos-plus-storage | files | save and load setups, waveforms, screens, SD card | docs/scpi/storage.md |
| oos-plus-logger-calibration | service | data logger, calibration and deskew | docs/scpi/logger.md, calibration.md |
| oos-plus-network-remote | remote | LAN, WLAN, Bluetooth, remote services switches, update | docs/network.md, scpi/system.md |
| oos-plus-errors-troubleshooting | recovery | error codes, -113, -221, nothing decoded, 9.9E37, BUSY | docs/scpi/README.md, wiki/Troubleshooting.md |

## 3. Also needed

- The decoder names: 29 decoders, so the decode skills say per protocol which lines (roles) and options it takes and the minimum sample rate.
- The 3 existing Rigol DHO skills must say OOS Plus is a different dialect and point to `oos-plus-vs-stock-rigol`.
- A Probewright test that every OOS Plus subsystem in `oosplus_dho.json` is named by at least one skill (the same idea as the OOS Plus test that every decoder is complete).
- The MCP side: PR 214 (`scpi_reference` with no arguments lists tasks), PR 215 (`measure_slot`).
