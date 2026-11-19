# Decoder plan (2026-11-19)

Goal: every protocol decoder is usable on the 1024x600 screen the way a bench scope shows a bus: a box per field on
the plot, a list of messages beside it, settings that are found, and a decode that is right on real signals. The
order follows what can be checked: first against captures of real buses on the host, then on the unit.

How a statement here is known is said with it: **unit** (seen on the owner's DHO924S), **capture** (decoded on the
host from a capture of a real bus and compared with the decode of the instrument that took it), **host** (tests on
synthetic signals only), or **not checked**.

## Where it stands

Shared UI, for every decoder (pull requests 71, 72 and 73 of the fork):

- the band on the plot: a box per field, a box per message when fields have no room, a stream when messages have
  none; a bit row at a time base that shows single bits; hidden with its source channel (unit, for UART and SPI);
- the Events pane: Messages, Fields, Lines, Errors only; fits its width; a tap marks a row and, stopped, goes to it;
  the one list of decoder events (unit, for UART and SPI);
- a threshold for each line, Threshold to middle, a pill per decoder in Studio's bottom bar, the Decode sheet with
  the lines of a protocol together (unit, for SPI).

Per decoder:

| Decoder | Decode on real signals | Shown as | Open |
|---|---|---|---|
| UART | capture: text and times agree; unit | band, bit row, lines | a failed character on the unit |
| SPI | capture: 13-bit words, MOSI/MISO, frames agree; unit | band, frames, Format | MISO on the unit |
| I2C | capture: bytes, errors and 10-bit addresses agree (one address differs, below) | band, bit row, Format | unit |
| CAN | capture: 97 of 97 frames agree | band | DLC, frame type as columns |
| CAN-FD | capture: frames agree; error frame not listed; DLC as its code | band | error frames, DLC in bytes |
| LIN | capture: frames, errors and the rate from the sync agree | band | unit |
| FlexRay | capture: frames agree; 9 CRC errors not checked | band | bit rate, channel A/B |
| I2S | capture: samples agree with the word length set | band | left and right as one row |
| SENT | capture: fast channel agrees | band | slow channel, two channels, status |
| MIL-STD-1553 | capture: **37 of 71 words misread** (bus 4 % slow, no resync; assumed) | band | resync on edges |
| ARINC 429 | capture: agrees, no errors | band | - |
| USB | capture: full speed packets agree | band | - |
| 1-Wire | capture: transactions agree | band | command names |
| I3C | capture: commands agree; **a 5-byte read loses its end** | band | fix; command names |
| C-PHY | capture: **nothing decoded** at 50 Msym/s with low-power states | band | low rates, LP states |
| Manchester | capture: not comparable (125 kbit/s not in the list, no packet structure) | band | rate, structure |
| the 13 others | host only: no capture | band | captures |

The 13 others are Parallel, DCC, SWD, JTAG, DMX512, MLT-3, DS2482, Quad-SPI, UART 9-bit, Modbus RTU, Modbus ASCII,
S/PDIF and USB PD. The Example module is a template and is not in the list. SMBus was read with the I2C decoder
(bytes agree); it has no layer of its own (protocol type, PEC).

The captures are 16 demo sessions of a PC scope program of another manufacturer, on the owner's computer. They stay
outside the tree (`local/captures`, ignored), with the scripts that run a decoder of the host UI over them
(`oos-ui --serve --open <file>`, the decoder on reference sources, `:DECode:EVENts?`).

## Steps

0. **Done.** Every decoder that has a capture run against it; the table above.
1. **Wrong decodes first**, each with a test that fails before the fix (a short synthetic signal of the same shape;
   the captures stay outside the tree) and a rerun on its capture:
   1. I2C: the NAK that ends a read is not an error; 10-bit addresses. **Done** (host and capture): of 393
      messages 162 have an error where 351 had one, and the rows the reference decode marks are the ones marked.
      Two differences are left as they are: the read after a repeated start shows the ten bits of the address
      written before it ("152 R") where the reference shows its one byte (79), and a general call (00 W) that
      nobody acknowledged is an error here and unmarked there. The SMBus capture has no error (96 before).
   2. LIN: the bit rate from the sync field; a frame with a bad sync is shown with its error; a byte after the
      frame's longest time is not its checksum; the checksum version as an option. **Done** (host and capture):
      the 37 frames of the capture read the same at every Baud setting, the frame with the bad sync is listed, and
      with Checksum = Enhanced the frames with a classic checksum are marked as the reference marks them (it also
      marks the diagnostic frame 3C, which keeps the classic checksum by the standard and passes here).
   3. MIL-STD-1553: the bit clock follows the mid-bit edges.
   4. I3C: the read that loses its end.
2. **Two hooks for every module**, so the rest is not built protocol by protocol:
   - where a frame starts. Today I2C, SPI, CAN, CAN-FD, LIN and I3C say so; FlexRay, SENT, USB, I2S, ARINC 429,
     1553 and 1-Wire are split into messages by idle time alone. The start marks then leave the Fields view (an
     "S" row per SPI frame today);
   - columns for the Events pane (CAN: ID, DLC, data, CRC; LIN: ID, data, checksum; I2C: address, data), in place
     of one text.
3. **Families, in the order they can be checked:** the bench buses on the unit (UART, SPI, I2C); the vehicle buses
   on their captures (CAN, CAN-FD, LIN, FlexRay, SENT); the avionics buses (1553, ARINC 429); USB, 1-Wire, I2S,
   I3C; then what is missing in a decode (SENT slow channel, CAN-FD error frames, a free bit rate for LIN and
   Manchester, C-PHY at low rates).
4. **The 13 decoders without a capture:** captures from the net (the owner is asked before a download) or from his
   bench; until then they are host only and say so.

## Decisions that are the owner's

- The height of a row of the Events pane: 26 px as in the other tables (5 rows in the docked pane), or a finger's
  40 px (3 rows).
- Whether SMBus gets a decoder of its own on top of I2C.
