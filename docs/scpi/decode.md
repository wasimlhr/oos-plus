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
| `:DECode:EVENts` | Query | Physical-time event rows, `BUSY` while pending, or an error if unavailable. While the scope runs the rows are those of the record on the screen and change with the next acquisition; stop it first (`:STOP`, `:SINGle`) and ask again until the answer is not `BUSY` for a complete list. The rows come one per line without a count, so a client that reads line by line cannot tell where they end: `:BUS<n>:DATA?` gives the same rows as a block. | empty | Decode and bus > Read decoded events | OOS extension |
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

