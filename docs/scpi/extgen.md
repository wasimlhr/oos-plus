# External generator

A generator beside the scope, on the USB-A port (USBTMC, no computer) or on the LAN. These commands name its model and where it is, connect to it, and switch a sine on. The Bode sweep with Generator = External sends through the same connection ([bode.md](bode.md)); a sweep that finds none opens one for itself and closes it at its end.

A model with an error query (the OWON DGE) is asked after every command: what it refuses is shown on the screen, and a Bode sweep ends with it. While a Bode sweep steps the generator, the commands that would change the generator or its connection answer -221.

How the models are known: [docs/extgen.md](../extgen.md). The OWON DGE on the USB-A port of the scope is not yet verified on hardware.

Menu location: Ext generator (Model, Connection, Address, State, Identity, Gen channel, Load, Frequency, Amplitude, Output, Connect / disconnect).

Part of the [SCPI reference](README.md).

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:EXTGen:MODel` | Set / Query | `APPL`, `BSWV`, `SCPI`, `CHANNEL`, `OWON_DGE`. | `APPL` | Ext generator > Model | OOS extension. Connect on the screen or `:EXTGen:CONNect ON` sets it from the `*IDN?` answer of a model it knows. |
| `:EXTGen:LINK` | Set / Query | `LAN`, `USB`. | `LAN` | Ext generator > Connection | OOS extension. `USB` is the first USBTMC instrument on the USB-A port. A change closes the connection. |
| `:EXTGen:ADDRess` | Set / Query | `"ip"` or `"ip:port"`, one word without blanks; port 5555 if none. | `192.168.1.100:5555` | Ext generator > Address | OOS extension. The address `:BODE:GENerator` sets. |
| `:EXTGen:CHANnel` | Set / Query | `1`, `2`. | `1` | Ext generator > Gen channel | OOS extension. Used by the models whose commands name a channel. |
| `:EXTGen:LOAD` | Set / Query | `HIZ` (also `INFinity`), `50`. | `HIZ` | Ext generator > Load | OOS extension. The load the generator is told it drives (OWON DGE): the amplitude is the one at that load. |
| `:EXTGen:FREQuency` | Set / Query | Hz. Range 1e-3 to 2e8. | `1.000000E+03` | Ext generator > Frequency | OOS extension. Sent at once while the output is on. |
| `:EXTGen:AMPLitude` | Set / Query | Volts peak to peak. Range 0.002 to 20.0. | `1.000000E+00` | Ext generator > Amplitude | OOS extension. Sent at once while the output is on. |
| `:EXTGen:OUTPut` | Set / Query | Boolean. `ON` sends load, sine, amplitude and frequency, then switches the output on. | `0` | Ext generator > Output | OOS extension. Error -221 without a connection. |
| `:EXTGen:CONNect` | Set / Query | Boolean; the query answers whether the connection is open or opening. | `0` | Ext generator > Connect / disconnect | OOS extension. Asks `*IDN?` and takes the model from the answer. |
| `:EXTGen:STATe` | Query | `OFF`, `CONNECTING`, `ON`, `FAILED`. | - | Ext generator > State | OOS extension |
| `:EXTGen:IDN` | Query | The `*IDN?` answer of the connected generator, empty without one. | - | Ext generator > Identity | OOS extension. The screen shows maker and model only. |

## Examples

An OWON DGE on the USB-A port, 1 kHz, 1 Vpp into the scope input.

```text
:EXTGen:LINK USB
:EXTGen:CONNect ON
:EXTGen:STATe?                         -> ON
:EXTGen:MODel?                         -> OWON_DGE
:EXTGen:LOAD HIZ
:EXTGen:FREQuency 1000
:EXTGen:AMPLitude 1
:EXTGen:OUTPut ON
:EXTGen:OUTPut?                        -> 1
```

A Bode sweep with that generator.

```text
:EXTGen:OUTPut OFF
:BODE:STARt 100
:BODE:STOP 1e6
:BODE:RUN ON
:BODE:RUN?                             -> 1
```

