# External generator

A generator beside the scope: on the USB-A port at the front, or on the LAN. OOS names its model, connects to it and
sends it SCPI commands. The Bode sweep uses it (Analysers > Bode, Generator = External), and the page can switch a
sine on by itself, to see on the screen that the generator does what it is told. The decision behind it is ADR 0028.

Menu > Instruments > Ext generator. Over SCPI: [scpi/extgen.md](scpi/extgen.md).

## How the scope talks to it

- **USB.** A USB cable from the USB-A port of the scope to the USB device port of the generator. The scope is the
  host, as a computer would be, and speaks USBTMC to the first instrument it finds (`ui/system/src/usbtmc.rs`: no kernel
  driver, no computer). Nothing is typed in.
- **LAN.** A socket to the address of the generator (`ip` or `ip:port`, port 5555 if none). The knob moves the last
  number of the address; `:EXTGen:ADDRess` sets all of it. A generator without a network port is not reached this
  way: the OWON DGE2000 series has a USB device port only (its data sheet).

Connect opens the connection, asks `*IDN?` and shows maker and model (the serial number stays off the screen). When
the answer is that of a model the table knows, the Model row follows it. `*ESR?` is never sent.

## The page

| Row | What |
|---|---|
| Model | the command set: APPL, BSWV, SCPI, CHANnel, OWON DGE |
| Connection | LAN or USB |
| Address | the LAN address |
| State | not connected, connecting, connected to what, or why not |
| Identity | maker and model of the `*IDN?` answer |
| Gen channel | channel 1 or 2 of the generator, for the models whose commands name one |
| Load | High-Z or 50 ohms: what the generator is told it drives (OWON DGE). Its amplitude is the one at that load; a scope input is High-Z |
| Frequency, Amplitude | the sine of the page's own output |
| Output | on: load, sine, amplitude and frequency are sent, then the output is switched on. While it is on, a change is sent at once |
| Connect / disconnect | opens or closes the connection |

A model with an error query is asked after every command. What it refuses is shown as a message with the command and
the generator's answer; the connection stays. In a Bode sweep the refused step ends the sweep with that message,
instead of a curve drawn from the old setting. The sweep measures a step only after the generator is through with
its commands.

## The models

Each model is a row of `GENERATORS` in `ui/src/extgen.rs`: the commands sent once, per frequency, to switch the
output on and off, the error query if it has one, and how its `*IDN?` answer starts. The setup file keeps the index
of the row, so new models go at the end.

| Model | For | How it is known |
|---|---|---|
| APPL | Rigol DG and others that take `:SOURn:APPL:SIN` | assumed from the command set; no generator tried |
| BSWV | Siglent SDG (`Cn:BSWV`) | assumed from the command set; no generator tried |
| SCPI | generators that take `FREQ` and `VOLT` | assumed; no generator tried |
| CHANnel | UNI-T UTG900 series | commands from the UTG900 programming manual (REV 00, 2022-07-15); not yet tried with a real UTG900 |
| OWON DGE | OWON DGE2000 and DGE3000 series | headers from the DGE2000/3000 programmer manual (V1.0.0, March 2023). On a DGE2070 with firmware V1.0.4.7.0, driven from a computer over USBTMC: the commands below are accepted, a value out of range is refused with an entry in the error queue and the old value stays, `SYSTem:ERRor?` reads and removes one entry, `*ESR?` is never answered. Not yet verified on hardware: the generator on the USB-A port of the scope |

The OWON DGE commands, for channel 1 into a high impedance at 1 kHz and 1 Vpp:

```text
OUTPut1:IMPedance INFinity
SOURce1:FUNCtion:SHAPe SIN
SOURce1:VOLTage:LEVel:IMMediate:OFFSet 0mV
SOURce1:VOLTage:LEVel:IMMediate:AMPLitude 1Vpp
SOURce1:FREQuency:FIXed 1000Hz
OUTPut1:STATe ON
```

each followed by `SYSTem:ERRor?`. A sweep sends the first four once and then one `FREQuency:FIXed` per point. The
sine of a DGE2070 ends at 70 MHz and its amplitude at 20 Vpp into High-Z (measured on that unit, not a published
figure): a sweep beyond it ends at the first frequency the generator refuses.

## Trying it on a unit

1. Cable from the USB-A port of the scope to the generator, generator output to CH1.
2. Ext generator > Connection = USB, then Connect. State reads `Connected: USB <vendor:product>`, Identity the maker
   and model, and Model follows for a model the table knows. Without a device State says why.
3. Output > Output = On at 1 kHz and 1 Vpp: CH1 shows a 1 kHz sine of 1 Vpp.
4. Frequency one step up: the sine follows. A frequency above the range of the generator: a message with the
   generator's answer, and the sine stays as it was.
5. Analysers > Bode, Generator = External, Input and Output on the channels, Start.

What ran so far is in the table of the main README ("Bode with a generator on USB"): tests on the host only.

## Adding a model

A row at the end of `GENERATORS`, with where its commands are from in the comment above the table and in the table
of this page, and a test of its command lines beside the others. `{f}` is the frequency in Hz, `{a}` the amplitude
in Vpp, `{c}` the channel, `{z}` the load. A model whose commands are not verified on a unit says so here.
