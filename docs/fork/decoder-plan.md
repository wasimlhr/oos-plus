# Decoder plan (2026-11-19)

**State, 2026-11-19:** step 0 and step 1 are done and merged (pull requests 75 and 76 of the fork, with the SIM bus
presets). Step 2 is done and merged (pull request 78): a message is a frame of the protocol, and the Events pane
has the columns of the protocol. Of step 3, what was missing in a decode is done on the host for SENT (slow
channel, two channels), CAN and CAN-FD (error flags, the length in bytes) and Manchester (the rate of the signal,
the other convention). The order from here on is the owner's (step 3): the buses that are live on his scope
first, one after the other; the decoders that can only be checked on stored waveforms last.

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
| UART | capture: text and times agree; unit: 236 characters, none wrong | band, bit row, lines | a failed character on the unit |
| SPI | capture: 13-bit words, MOSI/MISO, frames agree; unit: 441 words, none wrong | band, frames, columns, Format | MISO on the unit |
| I2C | capture: bytes, errors and 10-bit addresses agree (one address differs, below) | band, bit row, Format | unit |
| CAN | capture: 97 of 97 frames agree; unit: 93 to 97 frames in 40 ms, every CRC right; error flags host only | band, columns | an error frame on the unit |
| CAN-FD | capture: frames agree, length in bytes; error flags host only | band, columns | unit |
| LIN | capture: frames, errors and the rate from the sync agree | band | unit |
| FlexRay | capture: frames agree at 10 Mbit/s; 9 CRC errors that repeat with the cycle, not compared | band, columns | unit |
| I2S | capture: samples agree with the word length set | band, columns | - |
| SENT | capture: fast and slow channel and both values agree | band, columns | unit |
| MIL-STD-1553 | capture: 71 words agree (2 marked: the capture's own fault); command and status told apart | band, columns | unit |
| ARINC 429 | capture: agrees, no errors | band | - |
| USB | capture: full speed packets agree | band | - |
| 1-Wire | capture: transactions agree | band, columns, ROM command names | - |
| I3C | capture: commands and bytes as on the lines; one read differs from the reference | band, command names | - |
| C-PHY | capture: 64 bursts as packets, every checksum right; 64 low-power messages as the instrument lists them | band, columns | - |
| Manchester | capture: nine packets, cut into the fields its instrument lists in the five rows of its picture | band, packets | - |
| the 13 others | host only: no capture | band | captures |

The 13 others are Parallel, DCC, SWD, JTAG, DMX512, MLT-3, DS2482, Quad-SPI, UART 9-bit, Modbus RTU, Modbus ASCII,
S/PDIF and USB PD. The Example module is a template and is not in the list. SMBus was read with the I2C decoder
(bytes agree); it has no layer of its own (protocol type, PEC).

The captures are 16 demo sessions of a PC scope program of another manufacturer, on the owner's computer. They stay
outside the tree (`local/captures`, ignored), with the scripts that run a decoder of the host UI over them
(`oos-ui --serve --open <file>`, the decoder on reference sources, `:DECode:EVENts?`).

A decoder can also be tried on the scope with nothing connected: SIM > Preset has a bus for every protocol whose
traffic the scope can make (28 of them: every decoder), which it generates into references and decodes. That checks the screen
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
3. **The buses that are live on the owner's scope, one after the other.** His direction (2026-11-19): the signals
   that can be decoded live on the scope come first; the decoders that can only be checked on stored waveforms
   come at the end. For each live bus the same list, on the unit:
   - the decode is right: what the scope lists is what is known of the signal (its sender's data, or a second
     instrument);
   - the band at a slow, a middle and a fast time base, and the bit row where the protocol has one;
   - the Events pane: a row for each frame, the columns of the protocol, the Fields view, Errors only;
   - the thresholds found by Threshold to middle, and the protocol trigger holding a frame still.
   What it shows is fixed and verified on the unit before the next bus.
   1. SPI: on CH4 (data), CH3 (clock) and CH2 (select) of his bench; one word a frame every 500 us, the words a
      sine of 64 steps, and a frame of 16 words now and then. Done on the unit (2026-11-19): the decode is right
      (441 words in 200 ms, none wrong, down to 2.5 samples a bit), the band at 10 us, 100 us, 1 ms and 5 ms/div,
      the Events pane (a row a frame, Fields, a tap on a row), the thresholds from the signals. Three flaws it
      showed are fixed, each seen right on the unit afterwards: the MOSI column cut a frame of several words; the
      bus trigger never fired where the chip select has a threshold of its own; a record with too few samples for
      the clock listed words that were none. The bus trigger finds its word in the records the edge on the chip
      select gives it: every frame holds still at once, a word that comes every 150 ms after some seconds.
      Open: MISO (the bench bus has one data line), and the SPI trigger in the receiver, which is off by default
      and not verified on hardware. Noticed, not changed: `:BUS<n>:DATA?` answers from the newest record while
      the screen holds the last one the trigger took; after a change of the time base in Normal the band and the
      math trace of the old record stay while its channels are gone.
   2. UART: on CH1 of his bench (9600 baud, " Tektronix \n Enabling Innovation \n", a pause of one character
      before the second line feed). Done on the unit (2026-11-19): 236 characters in 800 ms, none wrong; the band
      beside the SPI band, the Events pane with both decoders, the Lines view, which has the two lines of the
      text, and the bus trigger holding a character still. Nothing to fix. Open: a failed character, which the
      bench signal has none of.
   3. CAN: on CH1 (CAN_H) and CH3 (CAN_L) of his demo board, 500 kbit/s. Done on the unit (2026-11-19): the
      decode is right from CAN_H, from CAN_L and from the difference of the two (93 to 97 frames in 40 ms, 21
      identifiers, every CRC right; the traffic of the capture), the thresholds from the signals, the band at
      2 ms, 200 us, 50 us and 5 us/div, the Events pane with its columns. The trigger on an identifier held
      the frame at 200 us/div and not at 50 us or 5 us/div; three faults behind that are fixed (the changelog
      has them: a frame shown at the hardware point and called triggered, the hardware point on the same few
      messages of a bus that repeats, a frame cut off by the search). After them, on the unit: a new frame in
      15 of 29 seconds at 50 us/div in Normal, the end of the identifier at the trigger point (before: none in
      70 s). The CAN-FD decoder reads the same bus (its frames are classic ones) at 200 us/div and gave
      nothing at 2 ms/div, where its data rate has too few samples: it reads what stays at the nominal rate
      now (unit: 88 frames, 21 identifiers, none wrong). Connected a second time the same day, CAN_H on CH1
      with a slow way back to recessive and CAN_L on CH2: the decoders resynchronised on that slow edge and
      read fields wrong; the bit timing follows the edges to dominant alone now, and the unit reads the line
      without a wrong field (93 to 96 frames in 40 ms, 21 identifiers). The trigger on an identifier, measured
      again there: a new frame in 15 of 29 seconds at 50 us/div, in 18 of 19 at 200 us/div, none at 5 us/div,
      where the record is shorter than the frame. The owner decided that the decoders give what is whole of a
      frame the record cuts off (start, identifier, data bytes; the last frame of a record then shows without
      its CRC), and with Auto memory the record of a protocol trigger has a frame of room on each side of the
      screen. After both, on the unit: a new frame in 17 of 24 seconds at 5 us/div and in 19 of 24 at 10 us/div
      and 20 us/div. Open: an error frame, which the
      board does not send. CH2 was not read: its high level sags to within 8 mV of the threshold in the middle,
      and the hysteresis of a tenth of a division (5 mV at 50 mV/div) lets it toggle; a record of it reads right
      on the host with 20 mV. And the unit left the network about 100 s after the start of build
      20261119-190204, in the run that measured the trigger, cause not known; it was back later the same day
      after a restart, the build confirmed by its trial start, its log of that start with the Ethernet without
      an address for a time and nothing of the run before it.
   4. The other buses of his demo board, as he connects them: CAN-FD, LIN, MIL-STD-1553, SENT, FlexRay, ARINC
      429, USB.
      10BASE-T (2026-11-19, the negative wire on CH1 and the positive one on CH2, 10 Mbit/s): the Manchester
      decoder read it on the unit from CH1 with "A one is" on Falling, a ping of 102 bytes with the right
      frame check and the frames the board changes on purpose, the same in every record. It brought the parts
      of an Ethernet frame with a marked frame check and a bit clock that follows the line (host-tested; not
      on the unit, where installs are paused at the owner's word). Open: CH2 at a time base fast enough, the
      Events pane and the trigger on a frame with a wrong check.
      Before they are connected (2026-11-19), each of the eight ran on the unit as its SIM preset: the scope
      makes the traffic, decodes it and shows it. That checks the screen and the path of the decoder on the
      unit, not the decode of a real signal. All eight decode without an error; the band and the columns of the
      Events pane are as meant for each (1553: RT, T/R, SA, WC/MC, Data, Status; USB: Packet, Address, Endpoint,
      Data, CRC; SENT without its slow columns, since the traffic has no slow channel). Three flaws the pass
      showed are fixed: a simulated line ran down into the band and its label covered the name of the band
      (seen right on the unit afterwards); the start of a CAN-FD frame read "S" beside CAN's "SOF"; a narrow box
      of a USB packet read "SYNC FS..." (it begins with the packet's type now, seen on the unit). What is open
      for every one of them is the real signal.
4. **Decoders with a capture and no live source:** I2C, I2S, 1-Wire, I3C, Manchester, C-PHY. They are checked on
   their captures and with the SIM bus presets, and say "capture" or "host", not "unit". Done here: C-PHY reads
   its capture (the levels from the record, the symbol order of the wire, the second sync word of a burst),
   1-Wire and I3C write the names of their commands, the words of a C-PHY burst are read as its packet and its
   low-power data as a command and bytes. A Manchester line that sends packets is read packet by
   packet: the capture has 13 bits every 14.5 bit times, and the decoder took the record for one stream, half a
   bit off in every second packet. A packet is cut into fields by settings of the
   decoder (Sync bits, Header bits, Data bits; what is left is the trailer), as the instrument of the capture has
   them: with 2, 3 and 6 the capture reads as that instrument lists it. Nothing of this step is open on the
   captures; what none of it has is a unit with the real signal.
   The same day the other twelve presets ran on the unit (UART, I2C, SPI, I2S, 1-Wire, DCC, SWD, DMX512, UART
   9-bit, Modbus RTU, S/PDIF, USB PD): every one decodes its traffic without an error. That is the first run on a
   unit for seven of the decoders of step 5. It showed that a preset left the lines of the preset before it on
   the screen, which is fixed and seen fixed on the unit, as is the text of a USB packet. Manchester, MLT-3
   and Modbus ASCII got traffic of their own and ran on the unit likewise, and so did the DS2482 bridge, JTAG
   and a C-PHY lane with a short packet; Quad-SPI has frames on one line each way, which its four roles carry:
   27 presets, and every one of the 27 has played and decoded on the unit without an error (the last with build
   20261119-143504). Parallel reads a bank of stored lines as it reads a bank of channels, so it has a preset
   too (a counter of three bits with its clock, which the unit reads as 00 to 07 with build 20261119-150142):
   28, one for every decoder, and every one has run on the unit.
5. **The 13 decoders without a capture:** captures from the net (the owner is asked before a download) or from his
   bench; until then they are host only and say so.

## Decisions that are the owner's

- The height of a row of the Events pane: 26 px as in the other tables (5 rows in the docked pane), or a finger's
  40 px (3 rows).
- Whether SMBus gets a decoder of its own on top of I2C.
- Manchester packets (taken without waiting, since it adds settings and changes nothing while they are 0): the
  widths of sync and header are two rows of the Decode menu, the word is Data bits, the trailer what is left. If
  a few fixed structures to choose from would serve better than numbers, the rows can become that.
