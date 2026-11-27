# Decoders and bus

Protocol decoders: the `:DECode:` subtree edits the selected decoder slot, `:BUS<n>:` addresses decoder n directly (the decoder the menu edits stays the same afterwards), and `:BUS:LANE:` is the plot bus (I2C lane). Protocol-specific rows exist only for their protocol: an option the protocol does not have is -224. Decoder parameters take the option text of the menu row, and the query answers it in capitals. A decoder on channels reads the samples the acquisition kept when those are finer than the screen record (up to 262144 points per line while running, 4 M when stopped), so `:DECode:EVENts?` and `:BUS<n>:DATA?` can list events of the whole record, also outside the screen. A UART character the record starts inside is not listed, nor the ones behind it until its bits can be read in one way only; in peak detect, roll and above 1 M points it reads the 3600 points of the screen. The events of a decoder can also be searched by the type `BUS` of [search](search.md). Reference sources may be used as roles if their native intervals overlap in physical time; no synthetic LA input is invented for a reference.

Menu location: Decode and bus (Protocol, Source / clock / CS, Threshold, Format, Events pane, protocol parameters, Plot bus, Save events).

Part of the [SCPI reference](README.md).

## Commands

### Decoder slot

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:DECode:SELect` | Set / Query | One-based decoder slot. | `1` | Decode and bus > Decoder slot select | OOS extension |
| `:DECode:PROTocol` | Set / Query | `<name>` Decoder protocol UI name: UART, I2C, SPI, CAN, CAN-FD, LIN, FlexRay and the rest of the list, matched without case, spaces or punctuation (`CAN-FD`, `canfd` and `CAN FD` are the same). The 29 names are UART, I2C, SPI, CAN, LIN, Parallel, C-PHY, DMX512, S/PDIF, USB PD, DCC, Manchester, MLT-3, UART 9-bit, Modbus RTU, Modbus ASCII, 1-Wire, DS2482, SWD, JTAG, Quad-SPI, I3C, CAN-FD, FlexRay, SENT, ARINC 429, 1553, I2S, USB. The query answers the UI name as written here. | `UART` | Decode > Protocol | OOS extension |
| `:DECode:STATe` | Set / Query | Boolean. | `0` | Decode > Decoder | OOS extension |
| `:DECode:INPut` | Set / Query | `WAVEFORM\|LA`; `ANALOG` aliases waveform. LA requires available/enabled logic inputs. | `WAVEFORM` | Decode and bus > Input | OOS extension |
| `:DECode:SOURce` | Set / Query | Main data role; `CHANn\|Rn`. | `CHAN3` | Decode > Source | OOS extension |
| `:DECode:SOURce2` | Set / Query | Clock role; `CHANn\|Rn`. | `CHAN4` | Decode > Clock | OOS extension |
| `:DECode:CS` | Set / Query | The third role (chip select for SPI); optional roles accept `NONE`. Roles depend on the protocol; for USB (low and full speed) it is **D-**: D+ is `:DECode:SOURce`, USB has no clock role (`:DECode:SOURce2` is not used), and D- is optional (`NONE` reads D+ alone). | `CHAN2` | Decode > CS | OOS extension |
| `:DECode:SOURce4` | Set / Query | Fourth role; optional, `NONE` clears it. | `NONE` | Decode and bus > Source / clock / CS / source 4 / B line | OOS extension |
| `:DECode:DIFFerential` | Set / Query | Differential (B line) role; optional, `NONE` clears it. | `NONE` | Decode and bus > Source / clock / CS / source 4 / B line | OOS extension |
| `:DECode:THReshold` | Set / Query | Configured threshold. For C-PHY this is minimum adjacent-wire level separation. `AUTO` sets it once to the middle of the signal on the decoder's source, half way between the lowest and the highest sample of the record on the screen, and each further line's to the middle of its own (-230 without a signal); the query answers the value. | `1.650000E+00` | Decode > Threshold | OOS extension |
| `:DECode:CPHY:RATE` | Set / Query | `AUTO` or known symbols/second; aliases CPHY:SRATe and DECode:SRATe. | `AUTO` | Decode and bus > C-PHY symbol rate | OOS extension |
| `:DECode:BUSY` | Query | Worker readiness, without changing acquisition. | `0` | Decode and bus > Read decoded events | OOS extension |
| `:DECode:EVENts` | Query | Physical-time event rows, `BUSY` while pending, or an error if unavailable. While the scope runs the rows are those of the record on the screen and change with the next acquisition; stop it first (`:STOP`, `:SINGle`) and ask again until the answer is not `BUSY` for a complete list. A row is `ok` or `error`; an error row ends with its reason, the text of the Error column of the Events pane ("Word count mismatch", "Incomplete packet"), so a word of a message the pane flags is an error here too. The rows come one per line without a count, so a client that reads line by line cannot tell where they end: `:BUS<n>:DATA?` gives the same rows as a block. These are always the word rows; the count of the pane's view (messages, fields or lines) is `:DECode:EVENts:COUNt`, so the two can differ. | empty | Decode and bus > Read decoded events | OOS extension |
| `:DECode:EVENts:VIEW` | Set / Query | `MESSages\|FIELds\|LINes`: what a row of the Events pane is. A message is what was sent without a pause clearly longer than usual, or from one start condition to the next; a line is a UART character stream up to its line feed (other decoders list their messages). `:DECode:EVENts?` and `:BUS<n>:DATA?` always answer a row per field. | `MESSAGES` | Events pane > Messages, Fields, Lines | OOS extension |
| `:DECode:EVENts:ERRors` | Set / Query | Boolean; the Events pane lists only the rows with a failed check. | `0` | Events pane > Errors only | OOS extension |
| `:DECode:SAVE` | Action | Save the decoded events as CSV. | - | Decode and bus > Export events | OOS extension |

### Bus and common parameters

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:BUS<n>:MODE` | Set / Query | `RS232\|UART\|IIC\|I2C\|SPI\|CAN\|LIN\|PARallel\|CPHY\|DMX512\|SPDIF\|USBPD\|DCC\|MANCHESTER\|MLT3\|UART9BIT\|MODBUSRTU\|MODBUSASCII\|1WIRE\|DS2482\|SWD\|JTAG\|QUADSPI\|I3C\|CANFD\|FLEXRAY\|SENT\|ARINC429\|1553\|I2S\|USB` Decoder protocol token (RS232, IIC, SPI, CAN, LIN, CANFD, FLEXRAY and the rest of the list), matched without case. UART is the name of RS232, I2C of IIC, PAR of PARallel. The remaining tokens are the UI name without spaces and punctuation (CAN-FD is CANFD, 1-Wire is 1WIRE, FlexRay is FLEXRAY, Quad-SPI is QUADSPI). The query answers PAR, RS232, IIC, SPI, CAN, LIN and the tokens as written here. | `RS232` | Decode > Protocol | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:DISPlay` | Set / Query | Boolean decoder visibility. | `0` | Decode > Decoder | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:FORMat` | Set / Query | `HEX\|ASCii`. | `ASC` | Decode > Format | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:LABel:CONTent` | Set / Query | The name on the band of the decoder, up to 10 characters; empty: its protocol. Minimal and Studio draw it. | (empty) | Decode > Label | OOS extension |
| `:BUS<n>:POSition` | Set / Query | Where the band stands in its window: divisions from the middle, -3.9 to 4, or `BOTTom`: at the bottom, on the bands under it. The query answers the number or `BOTT`. Minimal and Studio. | `BOTT` | Decode > Position | OOS extension (the unit is this firmware's own) |
| `:BUS<n>:EVENt` | Set / Query | Boolean; the Events pane, the second pane with the events of every decoder, on or off (the same pane as `:DISPlay:LOWer EVENTS`). It was a table over the plot. | `0` | Decode and bus > Events pane | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:THReshold` | Set / Query | Decoder threshold in source units. `<line>,<volts>` with line 2 to 4 sets the threshold of that line of the decoder alone (its second, third or fourth line, as Decode lists them), `<line>,DEF` gives it the decoder's again, and the query with a line number answers that line's. `AUTO` sets each line's once to the middle of its own signal (-230 without a signal on the main line); the query answers the value. | `1.650000E+00` | Decode > Threshold | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:DATA` | Query | Ready event rows in an IEEE block. `BUSY` while required decoding is pending; unavailable sources return an error. | - | Decode and bus > Read decoded events | Rigol-style (command family of the DHO guide) |

### UART, I2C and SPI

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:BUS<n>:RS232:TX` | Set / Query | Role source `CHANn\|Rn`. UART decoding uses the configured primary input. | `CHAN3` | Decode and bus > Source | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:RS232:RX` | Set / Query | Role source `CHANn\|Rn`. UART decoding uses the configured primary input. | `CHAN4` | Decode and bus > Source | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:RS232:BAUD` | Set / Query | Supported baud ladder entry (the nearest one is used). | `115200` | Decode > Baud | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:RS232:PARity` | Set / Query | Parity. | `NONE` | Decode > Parity | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:RS232:DBITs` | Set / Query | Data-bit count. | `8` | Decode > Data bits | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:IIC:SCLK:SOURce` | Set / Query | I2C clock role `CHANn\|Rn`. | `CHAN4` | Decode and bus > Source / clock / CS | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:IIC:SDA:SOURce` | Set / Query | I2C data role `CHANn\|Rn`. | `CHAN3` | Decode and bus > Source / clock / CS | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:SPI:SCLK:SOURce` | Set / Query | SPI role `CHANn\|Rn`. | `CHAN4` | Decode and bus > Source / clock / CS | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:SPI:MOSI:SOURce` | Set / Query | SPI role `CHANn\|Rn`. | `CHAN3` | Decode and bus > Source / clock / CS | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:SPI:SS:SOURce` | Set / Query | SPI select role `CHANn\|Rn`; `OFF` or `NONE` disables it. | `CHAN2` | Decode and bus > Source / clock / CS | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:SPI:DBITs` | Set / Query | Supported SPI word length. | `8` | Decode > Data bits | Rigol-style (command family of the DHO guide) |
| `:BUS<n>:SPI:MODE` | Set / Query | `0` to `3` (clock polarity and phase); an SPI decoder only. | `0` | Decode and bus > Mode | Rigol-compatible |

### CAN, SENT, USB and protocol options

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:BUS<n>:CAN:BAUD` | Set / Query | Bit rate, option text of the menu row (a CAN decoder only). | `500_KBIT/S` | Decode > Bit rate | Rigol-compatible |
| `:BUS<n>:FLEXray:BAUD` | Set / Query | FlexRay bit rate: `10`, `5` or `2.5` Mbit/s (option text of the menu row, `10 Mbit/s`; the same as `:BUS<n>:OPTion1`; a FlexRay decoder only, -224 otherwise). A deploy or `*RST` puts the decoder back to 10 Mbit/s, so set it again after one. | `10 Mbit/s` | Decode > Bit rate | OOS extension |
| `:BUS<n>:CAN:SIGNal` | Set / Query | `RX/TX` or `CAN_H` (a CAN decoder only). | `RX/TX` | Decode and bus > Signal | Rigol-compatible |
| `:BUS<n>:CAN:FD:BAUD` | Set / Query | FD data rate, option text of the menu row. | `2_M` | Decode and bus > FD data rate | Rigol-compatible |
| `:BUS<n>:CAN:FD:ABAUd` | Set / Query | FD arbitration rate, option text of the menu row. | `AUTO` | Decode and bus > Arbitration rate | Rigol-compatible |
| `:BUS<n>:SENT:TICK` | Set / Query | SENT tick, option text of the menu row. | `3_µS` | Decode and bus > Tick | OOS extension |
| `:BUS<n>:SENT:NIBBles` | Set / Query | Nibbles, 1 to 6. | `6` | Decode and bus > Nibbles | OOS extension |
| `:BUS<n>:USB:SPEed` | Set / Query | USB speed, option text of the menu row (a USB decoder only). | `AUTO` | Decode and bus > Speed | OOS extension |
| `:BUS<n>:OPTion1` | Set / Query | Option of the protocol module, in the order of the menu rows (FlexRay: its only option row, Bit rate, 10 Mbit/s, 5 Mbit/s or 2.5 Mbit/s, default 10 Mbit/s; FlexRay has no header of its own); -224 when the protocol has no such option. | empty | Decode and bus > Option | OOS extension |
| `:BUS<n>:OPTion2` | Set / Query | Option of the protocol module, in the order of the menu rows; -224 when the protocol has no such option. | empty | Decode and bus > Option | OOS extension |
| `:BUS<n>:OPTion3` | Set / Query | Option of the protocol module, in the order of the menu rows; -224 when the protocol has no such option. | empty | Decode and bus > Option | OOS extension |
| `:BUS<n>:OPTion4` | Set / Query | Option of the protocol module, in the order of the menu rows; -224 when the protocol has no such option. | empty | Decode and bus > Option | OOS extension |

### Polarity

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes | What it does | See also |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `:DECode:POLarity` | Set / Query | `NORMal\|INVerted\|AUTO`: how the selected decoder reads its line. `INVerted` reads it as if its polarity were swapped, without touching the channel; `AUTO` decodes both ways and keeps the reading with fewer failed fields (a tie keeps `NORMal`). Offered by UART, UART 9-bit, Modbus RTU, Modbus ASCII, CAN, CAN-FD, LIN, FlexRay, USB, 1553, ARINC 429 (`NORMal` and `INVerted` only: a swapped pair keeps the parity of its words) and DMX512 (the reason for every other decoder is in docs/protocol-modules.md#polarity); on UART it is relative to the serial trigger's polarity, which the decoder follows; -224 for another decoder, and for `AUTO` where it is not offered. The decoder's label says "(inverted)" while the line is read inverted. | `NORMAL` | Decode > Polarity | OOS extension | Reads a decoder's line inverted, or finds out which way is right: for a bus connected the wrong way round, where every word fails its parity, without inverting the channel on the screen. | `:BUS<n>:POLarity`, `:CHANnel<n>:INVert` |
| `:BUS<n>:POLarity` | Set / Query | The same for decoder n; the decoder the menu edits stays the same. | `NORMAL` | Decode > Polarity | OOS extension | The polarity of decoder n: normal, inverted or automatic, so a bus wired the wrong way round decodes without inverting its channel. | `:DECode:POLarity` |

### Diagnosis

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes | What it does | See also |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `:DECode:DIAGnosis` | Query | Why the selected decoder decoded nothing, or mostly errors, in short sentences one a line, each naming the setting to change ("sample rate 15.6 kSa/s is too low for 1.00 Mb/s: ...", "all 21 words fail their check, but the inverted line decodes with 0 errors in 21: ... set Polarity to Auto or Inverted ...", a threshold outside the signal, a flat line, a record shorter than 20 bit times, a bit rate the edges do not show). A record the decoder cannot read says why in one line (the limit of a deep record, a source that is off). A record above 1 M points is read as min/max pairs while the scope runs and says "stop it"; stopped (or after SINGLE) the raw window of the record's screen, every channel that is on, is read once and decoded; a window above the budget of one read (24 MiB) is read as its largest part around the centre (about 6 M points with two channels on). When that read is missing the text says so ("Stopped deep record could not be read raw: press RUN and STOP again") or that raw reads are off (`meas.deep_raw`). Empty when the decode is fine; `BUSY` while decoding. | empty | Events pane, when it is empty | OOS extension | Says why a decoder shows nothing or errors: the cause and the setting to change, so a bus is not left undecoded without a reason. | `:DECode:DIAGnosis:FIGures`, `:BUS<n>:DIAGnosis`, `:DECode:POLarity`, `:DECode:EVENts` |
| `:DECode:DIAGnosis:FIGures` | Query | What the diagnosis measured, four numbers: the bit rate the edges of the line suggest (Hz; twice the bit rate for a Manchester line, whose edge is in the middle of a bit), the lowest and highest sample (V) and the share of decoded fields that failed their check; `NAN` where unknown. | `NAN,NAN,NAN,0.000000E+00` | - | OOS extension | The bit rate, the signal levels and the error share the diagnosis is made from. | `:DECode:DIAGnosis` |
| `:BUS<n>:DIAGnosis` | Query | The same for decoder n (`:BUS<n>:DIAGnosis:FIGures` for its figures). | empty | Events pane, when it is empty | OOS extension | The diagnosis of decoder n: why it decoded nothing or mostly errors. | `:DECode:DIAGnosis` |

### Automatic set-up

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes | What it does | See also |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `:DECode:AUTO` | Action | Sets the selected decoder up for its signal in one step: the threshold to the middle of the signal on the screen (half the peak for a bipolar line such as ARINC 429), Polarity Auto where the decoder has it, the smallest time base whose screen holds a frame and the smallest memory depth that gives at least 10 samples a bit at it (kept when they do already; never cut: where no depth of the unit fits it says so and leaves them), then one acquisition (a single, when the scope is stopped; a running scope decodes its next record). Nothing else is changed; every change is named in the report. -221 does not occur: a setting that cannot be made is reported, not refused. | - | Decode > Threshold to middle | OOS extension | Sets a decoder up for its signal in one call, and tells what it changed. | `:DECode:DIAGnosis`, `:DECode:POLarity`, `:ACQuire:MDEPth` |
| `:DECode:AUTO?` | Query | The report of the last `:DECode:AUTO`, one sentence a line: each setting it changed with its old value, each it kept, what it could not do, and below it the diagnosis of the decoder now (lines that begin with `diagnosis:`). The query only reads: it changes nothing and arms nothing; it shows what the last `:DECode:AUTO` (or `setup_decoder auto:true`) did. The automatic set-up chooses the smallest time base and depth that hold one frame: for a view of many packets set a longer time base and a deep memory yourself (`:TIMebase:SCALe`, `:ACQuire:MDEPth`), then `:SINGle`. The diagnosis is of the record on the screen: ask again when the single acquisition has stopped (`:TRIGger:STATus?` is STOP). | empty | - | OOS extension | What the automatic decoder set-up did and what the decoder says about its signal now. | `:DECode:DIAGnosis` |

### Plot bus

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:BUS:LANE:STATe` | Set / Query | Boolean; the plot bus (the I2C lane) on or off. | `0` | Bus > Bus | OOS extension |
| `:BUS:LANE:LABel` | Set / Query | Label text, up to 12 characters. | `I2C` | Decode and bus > Plot bus | OOS extension |
| `:BUS:LANE:POSition` | Set / Query | Position in divisions, -3.6 to 3.6. | `-3.000000E+00` | Bus > Position | OOS extension |
| `:BUS:LANE:CLOCk:SOURce` | Set / Query | Source `CHANn\|Rn`. | `CH1` | Bus > Clock | OOS extension |
| `:BUS:LANE:DATA:SOURce` | Set / Query | Source `CHANn\|Rn`. | `CH2` | Bus > Data | OOS extension |
| `:BUS:LANE:CLOCk:THReshold` | Set / Query | Volts. | `1.650000E+00` | Bus > Clock threshold | OOS extension |
| `:BUS:LANE:DATA:THReshold` | Set / Query | Volts. | `1.650000E+00` | Bus > Data threshold | OOS extension |
| `:BUS:LANE:RW` | Set / Query | `LEFT_OUT`, `INCLUDED` (the read/write bit in the address) | `LEFT_OUT` | Bus > R/W bit | OOS extension |

## Examples

UART decoder on channel 1.

```text
:DECode:SELect 1
:DECode:PROTocol UART
:DECode:SOURce CHAN1
:DECode:THReshold 1.5
:BUS1:RS232:BAUD 115200
:BUS1:RS232:PARity NONE
:DECode:STATe ON
:DECode:PROTocol?                      -> UART
```

I2C decoder.

```text
:DECode:PROTocol I2C
:BUS1:IIC:SCLK:SOURce CHAN1
:BUS1:IIC:SDA:SOURce CHAN2
:DECode:STATe ON
:BUS1:FORMat HEX
:BUS1:FORMat?                          -> HEX
:DECode:BUSY?                          -> 0
```

Plot bus lane.

```text
:BUS:LANE:STATe ON
:BUS:LANE:LABel "I2C"
:BUS:LANE:POSition 2
:BUS:LANE:STATe?                       -> 1
:BUS:LANE:LABel?                       -> I2C
```

### What a stopped deep record gives each reader

A record above 1 M points, stopped:

- The decoder (bands, Events, `:DECode:EVENts?`) reads the raw window of the stop: its source's samples across the screen, the centre part within the budget.
- Measurements and the cursor readout read the same window (Mean, RMS, Std dev and Area from samples).
- `:WAVeform:DATA?` in RAW is the same window, not all of the memory; in NORMal and in the picture it is the screen record (3600 points of min/max).

Running, a deep record is read as pairs: the decoder says "stop it", and the mean-type measurements keep the last raw window.
