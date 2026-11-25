# Channels

The vertical settings of the analogue inputs `CHANnel<n>` (n = 1 to the number of channels of the model), and the selected channel. Scales are chosen at the nearest supported step and the probe factor applies. Waveform names in scripts are `CHAN1` to `CHAN4`, not the UI label `CH1`. Probe deskew (`:CHANnel<n>:TCALibrate`) is separate from the analysis deskew in [calibration](calibration.md).

Menu location: Channels (Display, Scale, Offset, Coupling, Bandwidth limit, Probe ratio, Invert, Unit, Fine scale, Deskew, Zero bias, Expand around, Name).

Part of the [SCPI reference](README.md).

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes | What it does | See also |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `:CHANnel<n>:DISPlay` | Set / Query | Boolean. | `1` | Channel > Channel | Rigol-compatible | Switches the analogue input on or off. An input that is off is not acquired, and what is computed from it (measurements, decoders, math) goes with it. | `:CHANnel<n>:HIDE` |
| `:CHANnel<n>:SCALe` | Set / Query | Volts/div; setter chooses the nearest supported scale. Probe factor applies. | `1.000000E+00` | Channel > Scale | Rigol-compatible | Sets the vertical scale of the input in volts per division at the probe tip. The scope takes the nearest scale it has, so read the value back. | `:CHANnel<n>:OFFSet`, `:CHANnel<n>:PROBe`, `:CHANnel<n>:VERNier`, `:CHANnel<n>:EXPand` |
| `:CHANnel<n>:OFFSet` | Set / Query | Volts. | `-1.500000E+00` | Channel > Offset | Rigol-compatible | Sets the vertical offset of the input in volts. | `:CHANnel<n>:SCALe`, `:CHANnel<n>:POSition`, `:CHANnel<n>:BIAS` |
| `:CHANnel<n>:COUPling` | Set / Query | `DC\|AC\|GND`; `DC50` only on a board supporting it. | `DC` | Channel > Coupling | Rigol-compatible | Sets the input coupling: DC, AC or ground, and DC at 50 ohms on a board that has it. | `:CHANnel<n>:BWLimit` |
| `:CHANnel<n>:BWLimit` | Set / Query | `OFF\|20M\|AUTO` or a supported filter name. OOS filtering extends stock choices. | `AUTO` | Channel > Bandwidth | Rigol-compatible | Sets the bandwidth limit of the input, the filter after the ADC: off, 20 MHz, automatic, or a filter by its name. | `:CHANnel<n>:COUPling` |
| `:CHANnel<n>:PROBe` | Set / Query | Positive attenuation factor. | `1.000000E+00` | Channel > Probe | Rigol-compatible | Sets the attenuation factor of the probe on the input (10 for a 10x probe). The scale is then in volts at the probe tip. | `:CHANnel<n>:SCALe`, `:CHANnel<n>:UNITs` |
| `:CHANnel<n>:INVert` | Set / Query | Boolean. | `0` | Channel > Invert | Rigol-compatible | Inverts the trace of the input, or shows it as it is. | - |
| `:CHANnel<n>:HIDE` | Set / Query | Boolean. The channel stays on without its trace: it is acquired, measured, decoded and the input of a math trace as before, and has no trace, lane or ground mark. `:CHANnel<n>:DISPlay OFF` switches the channel off, and what is computed from it goes with it. | `0` | Channel > Hide trace | OOS extension | Hides the trace of an input that stays on: it is still acquired, measured, decoded and usable as the input of a math trace. | `:CHANnel<n>:DISPlay` |
| `:CHANnel<n>:CLIPping?` | Query | `1` while the record on screen reaches the end of the ADC range on this channel (4.35 divisions above or below the middle of the screen), else `0`. `0` for a channel that is off. | `0` | the red Clipping note in the plot | OOS extension | Asks whether the record on screen reaches the end of the ADC range on this input: `1` when it clips, else `0`. | `:CHANnel<n>:SCALe`, `:CHANnel<n>:OFFSet` |
| `:CHANnel<n>:POSition` | Set / Query | The same value as `:CHANnel<n>:OFFSet`, in volts (not in divisions). | `-1.500000E+00` | Channel > Offset | Rigol-style (command family of the DHO guide) | Sets the vertical offset of the input in volts: the same setting as `:CHANnel<n>:OFFSet` under its second name. | `:CHANnel<n>:OFFSet` |
| `:CHANnel<n>:UNITs` | Set / Query | Supported channel unit. Use the configured unit when interpreting cursor/readout values. | `VOLT` | Channel more > Unit | Rigol-style (command family of the DHO guide) | Sets the unit the values of the input are shown and answered in: `VOLTage`, `AMPere`, `WATT` or `UNKNown`. | `:CHANnel<n>:PROBe` |
| `:CHANnel<n>:VERNier` | Set / Query | Boolean fine vertical control. | `0` | Channel > Fine scale | Rigol-style (command family of the DHO guide) | Switches the fine vertical scale on or off, for every input at once. With it on, `:CHANnel<n>:SCALe` takes the first calibrated scale at or above the value instead of the nearest step of the 1-2-5 ladder. | `:CHANnel<n>:SCALe` |
| `:CHANnel<n>:TCALibrate` | Set / Query | Channel deskew in seconds; native transfer metadata remains unchanged. | `0.000000E+00` | Channel more > Deskew | Rigol-style (command family of the DHO guide) | Sets the deskew of the input in seconds, to line up probes of different delay. A change is refused with -224 on a time base slower than 10 us/div. | - |
| `:CHANnel<n>:BIAS` | Set / Query | Software channel zero-bias correction. This is not a calibration procedure. | `0.000000E+00` | Channel more > Zero bias | Rigol-style (command family of the DHO guide) | Sets the zero correction of the input in volts, which is subtracted from what the input reads. | `:CHANnel<n>:BIAS:ZERO`, `:CHANnel<n>:ZERO`, `:CHANnel<n>:OFFSet` |
| `:CHANnel<n>:EXPand` | Set / Query | Vertical expansion reference supported by the model. | `CENT` | Channel more > Expand around | Rigol-style (command family of the DHO guide) | Chooses what stays in place when the vertical scale changes, the ground line (`GND`) or the centre of the screen (`CENTer`): one switch for every input. | `:CHANnel<n>:SCALe` |
| `:CHANnel<n>:LABel:SHOW` | Set / Query | Boolean label visibility. | `0` | Channels > Name (label) | Rigol-style (command family of the DHO guide) | Shows or hides the name label of the input on the screen. | `:CHANnel<n>:LABel:CONTent` |
| `:CHANnel<n>:LABel:CONTent` | Set / Query | Label text. | empty | Channels > Name (label) | Rigol-style (command family of the DHO guide) | Sets the text of the name label of the input. | `:CHANnel<n>:LABel:SHOW` |
| `:CHANnel<n>:BIAS:ZERO` | Action | The mean of the last record becomes part of the zero bias | - | Channels > Zero this input | OOS extension | Takes the mean of the last record into the zero bias of the input, so that the level it sees now reads zero. | `:CHANnel<n>:BIAS`, `:CHANnel<n>:ZERO` |
| `:CHANnel:ACTive` | Set / Query | The selected channel, 1 to the number of channels | `1` | Channels > active channel | OOS extension | Selects the input the channel controls of the screen act on, by its number. | - |
| `:CHANnel<n>:ZERO` | Action | The mean of the record shown becomes the zero correction of the channel; refused without a waveform | - | Channel > Zero | fork command; name not verified against the DHO guide | Makes the mean of the record shown the zero correction of the input. Refused when there is no waveform. | `:CHANnel<n>:BIAS`, `:CHANnel<n>:BIAS:ZERO` |

## Examples

Scale, offset and coupling of channel 1.

```text
:CHANnel1:DISPlay ON
:CHANnel1:SCALe 0.5
:CHANnel1:OFFSet 0.25
:CHANnel1:COUPling AC
:CHANnel1:SCALe?                       -> 5.000000E-01
:CHANnel1:COUPling?                    -> AC
```

Probe ratio, unit, label.

```text
:CHANnel2:PROBe 10
:CHANnel2:UNITs AMPere
:CHANnel2:LABel:CONTent "Iload"
:CHANnel2:LABel:SHOW ON
:CHANnel2:UNITs?                       -> AMP
:CHANnel2:LABel:CONTent?               -> Iload
```

Select the active channel.

```text
:CHANnel:ACTive 2
:CHANnel:ACTive?                       -> 2
```

