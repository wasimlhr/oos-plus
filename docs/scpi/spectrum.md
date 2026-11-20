# Spectrum

The Spectrum analyser page: setup, peak and limit actions, and the figure readout `:SPECtrum:ITEM?`. A change restarts the averaging and the waterfall like the menu does. Sources take `CHANn` or `Rn`.

Menu location: Spectrum (Source, Centre, Span, RBW, Window, Trace mode, Ref level, Waterfall, THD / SNR, Peak list, ACPR, Limit line).

Part of the [SCPI reference](README.md).

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SPECtrum:PEAK:CENTer` | Action | Centre the spectrum on its strongest line | - | Spectrum > Centre on peak | OOS extension |
| `:SPECtrum:LIMit:SET` | Action | Make a limit line from the trace. | - | Spectrum > Limit line | OOS extension |
| `:SPECtrum:LIMit:CLEar` | Action | Clear the limit line. | - | Spectrum > Limit line | OOS extension |
| `:SPECtrum:LIMit:MARGin` | Set / Query | DB, 0 to 60 | `1.000000E+01` | Spectrum > Limit margin | OOS extension |
| `:SPECtrum:SOURce` | Set / Query | Source | `CH1` | Spectrum > Source | name not verified against the DHO guide |
| `:SPECtrum:CENTer` | Set / Query | Centre frequency, Hz. Range 0.0 to 1e12. | `0.000000E+00` | Spectrum > Centre | name not verified against the DHO guide |
| `:SPECtrum:SPAN` | Set / Query | Span, Hz; 0 is full / automatic. Range 0.0 to 1e12. | `0.000000E+00` | Spectrum > Span | name not verified against the DHO guide |
| `:SPECtrum:RBW` | Set / Query | Resolution bandwidth, Hz. Range 0.0 to 1e12. | `0.000000E+00` | Spectrum > RBW | name not verified against the DHO guide |
| `:SPECtrum:RBW:ACTual` | Query | The resolution bandwidth, Hz, of the spectrum of the record now: the one the page shows while it runs, else computed for the query with the page's settings; `9.9E37` without a record. The record on the screen limits how fine it can be, so it can be coarser than `:SPECtrum:RBW` asks for: make the time base slower for a finer one. | - | Spectrum > RBW in the title of the plot | OOS extension |
| `:SPECtrum:WINDow` | Set / Query | Option text of the menu row (the query answers it in capitals). | `BLACKMAN` | Spectrum > Window | name not verified against the DHO guide |
| `:SPECtrum:TRACe` | Set / Query | Option text of the menu row (the query answers it in capitals). | `NORMAL` | Spectrum > Trace | name not verified against the DHO guide |
| `:SPECtrum:RLEVel` | Set / Query | DBV Range -300.0 to 300.0. | `1.000000E+01` | Spectrum > Ref level | name not verified against the DHO guide |
| `:SPECtrum:WATerfall` | Set / Query | Boolean. | `0` | Spectrum > Waterfall | name not verified against the DHO guide |
| `:SPECtrum:DISTortion` | Set / Query | Boolean. | `0` | Spectrum > THD / SNR | OOS extension |
| `:SPECtrum:PEAK:HARMonics` | Set / Query | Option text of the Harmonics row | `10` | Spectrum > Peak list | name not verified against the DHO guide |
| `:SPECtrum:ACPR:WIDTh` | Set / Query | Hz. Range 1.0 to 1e12. | `1.000000E+06` | Spectrum > Ch width | name not verified against the DHO guide |
| `:SPECtrum:ACPR:SPACing` | Set / Query | Hz. Range 1.0 to 1e12. | `2.000000E+06` | Spectrum > Ch space | name not verified against the DHO guide |
| `:SPECtrum:ITEM` | Query | `<item>[,source]` `F1`, `THD`, `THDPCT`, `THDN`, `SNR`, `SINAD`, `SFDR`, `ENOB`, `ACPR`, `ACPRLO`, `ACPRHI`, `FLOOR`. `F1` to `ENOB` answer `9.9E37` when the record has no fundamental (noise alone: no line stands 20 dB above the median of the spectrum); `ACPR` and `FLOOR` need none. | - | Spectrum > Read spectrum figures (THD, SNR, SFDR, ...) | OOS extension |
| `:SPECtrum:PEAK:LIST` | Query | The lines of the spectrum of the record now, in the order of the peak table (by frequency or by level): `frequency,level;...`, Hz and dBV: the lines found above the noise within the page's span (the fundamental's harmonics, and others within the spur range of the strongest), strongest first, up to the Harmonics row (at least 8; All: no limit), also those the table has no room for; a harmonic below the noise is not listed. Computed for the query when the page is not shown; `9.9E37` when there is none (also when the span holds no line). | - | Spectrum > Peak list | OOS extension |

## Examples

Centre, span and resolution bandwidth. `:SPECtrum:RBW:ACTual?` answers the RBW the page computed with, which is
coarser than the one asked for when the record on the screen is short.

```text
:SPECtrum:SOURce CHAN1
:SPECtrum:CENTer 1e6
:SPECtrum:SPAN 2e6
:SPECtrum:RBW 1e3
:SPECtrum:CENTer?                      -> 1.000000E+06
:SPECtrum:SPAN?                        -> 2.000000E+06
:SPECtrum:RBW:ACTual?                  -> 1.000000E+03
```

The peak table: frequency and level of each line, in the order of the table.

```text
:SPECtrum:PEAK:LIST?                   -> 4.000100E+07,-2.070000E+01;7.999900E+07,-5.020000E+01;...
```

Window and limit line.

```text
:SPECtrum:WINDow?                      -> BLACKMAN
:SPECtrum:LIMit:MARGin 6
:SPECtrum:LIMit:SET
:SPECtrum:LIMit:MARGin?                -> 6.000000E+00
:SPECtrum:LIMit:CLEar
```

