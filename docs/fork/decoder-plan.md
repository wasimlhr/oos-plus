# Decoder plan (2026-11-19)

**State, 2026-11-19:** step 0 and step 1 are done and merged (pull requests 75 and 76 of the fork, with the SIM bus
presets). Step 2 is done and merged (pull request 78): a message is a frame of the protocol, and the Events pane
has the columns of the protocol. Of step 3, what was missing in a decode is done on the host for SENT (slow
channel, two channels), CAN and CAN-FD (error flags, the length in bytes) and Manchester (the rate of the signal,
the other convention). Open: C-PHY at low rates, the packet structure of a Manchester line, and each bus on the
unit as the owner connects it.

Goal: every protocol decoder is usable on the 1024x600 screen the way a bench scope shows a bus: a box per field on
the plot, a list of messages beside it, settings that are found, and a decode that is right on real signals. The
order follows what can be checked: first against captures of real buses on the host, then on the unit.

How a statement here is known is said with it: **unit** (seen on the owner's DHO924S), **capture** (decoded on the
host from a capture of a real bus and compared with the decode of the instrument that took it), **host** (tests on
synthetic signals only), or **not checked**.

## Where it stands

Shared UI, for every decoder (pull requests 71, 72, 73 and 76 of the fork):

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
| CAN | capture: 97 of 97 frames agree; error flags host only | band, columns | unit |
| CAN-FD | capture: frames agree, length in bytes; error flags host only | band, columns | unit |
| LIN | capture: frames, errors and the rate from the sync agree | band | unit |
| FlexRay | capture: frames agree; 9 CRC errors not checked | band | bit rate, channel A/B |
| I2S | capture: samples agree with the word length set | band | left and right as one row |
| SENT | capture: fast and slow channel and both values agree | band, columns | unit |
| MIL-STD-1553 | capture: 71 words agree (2 marked: the capture's own fault) | band | word types as columns; unit |
| ARINC 429 | capture: agrees, no errors | band | - |
| USB | capture: full speed packets agree | band | - |
| 1-Wire | capture: transactions agree | band | command names |
| I3C | capture: commands and bytes as on the lines; one read differs from the reference | band | command names |
| C-PHY | capture: **nothing decoded** at 50 Msym/s with low-power states | band | low rates, LP states |
| Manchester | capture: the bits agree at the rate of the signal; no packet structure | band | structure |
| the 13 others | host only: no capture | band | captures |

The 13 others are Parallel, DCC, SWD, JTAG, DMX512, MLT-3, DS2482, Quad-SPI, UART 9-bit, Modbus RTU, Modbus ASCII,
S/PDIF and USB PD. The Example module is a template and is not in the list. SMBus was read with the I2C decoder
(bytes agree); it has no layer of its own (protocol type, PEC).

The captures are 16 demo sessions of a PC scope program of another manufacturer, on the owner's computer. They stay
outside the tree (`local/captures`, ignored), with the scripts that run a decoder of the host UI over them
(`oos-ui --serve --open <file>`, the decoder on reference sources, `:DECode:EVENts?`).

A decoder can also be tried on the scope with nothing connected: SIM > Preset has a bus for every protocol whose
traffic the scope can make (20 of them), which it generates into references and decodes. That checks the screen
and the decoder's path on the unit, not the decode of a real signal: the traffic is the one the tests decode.

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
   3. MIL-STD-1553: the bit clock follows the mid-bit edges. **Done** (host and capture): the words of the capture
      are 20.8 us long (a bus 4 % slow, which was the assumed cause and is the measured one); 2 of 71 are marked
      where 37 were, and the first 13 are the command, status and data words the reference decode lists.
   4. I3C: the read that loses its end. **No fault found.** Read sample by sample, the lines of that read hold
      three bytes AC with T = 1, then 59 with T = 0, the end of the read, as the decoder lists them; eight more
      bits (AC) are clocked before the stop, which the decoder leaves out, and the reference decode shows five
      bytes there. The capture is a generated one (a clock of 200 MHz); a real I3C bus is needed to say more.
2. **Two hooks for every module**, so the rest is not built protocol by protocol:
   - where a frame starts. **Done** (host; SENT and USB on their captures): a module names the field a frame
     begins with (`frame_start`), and a message is a frame whatever pauses it has inside. I2C, I3C, CAN, CAN-FD,
     LIN, FlexRay, SENT, USB, I2S, ARINC 429, 1-Wire, SWD and DCC do; the UART family, SPI without a chip select,
     Modbus and 1553 are split by idle time as before (a 1553 command and a status word have the same sync). The
     SENT capture, one message before, lists its 118 frames. The mark an SPI frame begins with is no row of the
     Fields view any more;
   - columns for the Events pane, in place of one text. **Done** (host; CAN and LIN on their captures): a module
     gives a table of titles and what each column holds of a frame (`columns`). CAN and CAN-FD: ID, DLC, Data,
     CRC; LIN: ID, Data, Checksum; I2C: Address, Data; SPI: MOSI, MISO; SENT: Status, Data, CRC; FlexRay: ID,
     Cycle, Data, CRC; ARINC 429: Label, SDI, Data, SSM; USB: Packet, Address, Endpoint, Data, CRC; I2S: Left,
     Right. The pane uses them when it lists messages, is at least 560 px wide and every decoder that is on has
     the same columns; else a row is its text as before.
   What was missing in a decode, done on the host: the SENT slow channel (the message of the capture reads
   identifier 07, data 075, CRC 01, as the reference has it, and its CRC check passes) and the two 12-bit values;
   CAN and CAN-FD error flags (on a generated bus: the "error frame" the reference lists first in the CAN-FD
   capture is the frame the record begins in the middle of, not a flag on the bus) and the length in bytes;
   Manchester at the rate of its signal, with either convention for a one.
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
