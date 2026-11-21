# Search (mark events)

Mark the events of a trigger-like condition and walk through them. The first set of a source, level or slope adds Search 1 like the Add rail does. Studio has up to three searches: `:SEARch<n>:...` with `<n>` 1 to 3 is Search n, and without a number the command is of Search 1 (`STATe` and `MODE` are the same with any number; `VALue` takes none). The first set of `:SEARch2:EDGE:SOURce` adds Search 2; `:SEARch2:EVENt` goes to an event of Search 2 and makes it the selected search of the screen, the other commands leave the screen's choice alone. A search of Studio is of one of six types, `EDGE`, `PULSe`, `SLOPe`, `TIMeout`, `RUNT` and `WINDow`, with the settings the trigger has for that type under the same names (`:SEARch:PULSe:WHEN` as `:TRIGger:PULSe:WHEN`). A search has one set of settings, as the trigger has: a set under a type makes the search of that type, a query answers the setting whatever the type is, and `:SEARch:MODE` changes the type alone. The events are found in the samples by the software trigger, with a hysteresis of a tenth of a division. The events of the decoders have their own commands in [decode](decode.md).

Menu location: Decode and bus > Mark events, Search (type, source, slope or polarity, levels, 50 %, time condition, from / to trigger, event, event list, save events, remove).

Part of the [SCPI reference](README.md).

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SEARch:STATe` | Set / Query | Mark the events of the trigger condition | `0` | Trigger > Mark events | name not verified against the DHO guide |
| `:SEARch:MODE` | Set / Query | `EDGE`, `PULSe`, `SLOPe`, `TIMeout`, `RUNT`, `WINDow`: the type of the search. `SLOPe`, `RUNT` and `WINDow` place their two levels on the signal when the search had one level. Without a search of Studio the answer is `EDGE` | `EDGE` | Search > Type | name not verified against the DHO guide |
| `:SEARch:EDGE:SOURce` | Set / Query | Search source; without a search of its own the trigger's. The first set of a source, level or slope adds Search 1 like the Add rail does | `CHAN1` | Decode and bus > Search source | name not verified against the DHO guide |
| `:SEARch:EDGE:SLOPe` | Set / Query | `POSitive`, `NEGative`, `RFALl` | `POS` | Decode and bus > Search slope | name not verified against the DHO guide |
| `:SEARch:EDGE:LEVel` | Set / Query | Volts | `1.500000E+00` | Decode and bus > Search level | name not verified against the DHO guide |
| `:SEARch:EDGE:LEVel:AUTO` | Action | Level to the middle of the source | - | Decode and bus > Level 50 % | name not verified against the DHO guide |
| `:SEARch:PULSe:SOURce` | Set / Query | Search source; the set makes the search a pulse search | `CHAN1` | Search > Source | name not verified against the DHO guide |
| `:SEARch:PULSe:POLarity` | Set / Query | `POSitive`, `NEGative` | `POS` | Search > Polarity | name not verified against the DHO guide |
| `:SEARch:PULSe:WHEN` | Set / Query | `GREater`, `LESS`, `EQUal` and `UNEQual` (within 5 % of the limit, or not), `RANGe` (inside the two limits), `OUTside` | `LESS` | Search > When | name not verified against the DHO guide |
| `:SEARch:PULSe:LWIDth` | Set / Query | Seconds, above 0. The limit, or the lower one of `RANGe` and `OUTside` | `2.000000E-05` | Search > Time | name not verified against the DHO guide |
| `:SEARch:PULSe:UWIDth` | Set / Query | Seconds, above 0. The upper limit of `RANGe` and `OUTside`; with one limit the same as `LWIDth` | `4.000000E-05` | Search > Upper limit | name not verified against the DHO guide |
| `:SEARch:PULSe:LEVel` | Set / Query | Volts | `1.500000E+00` | Search > Level | name not verified against the DHO guide |
| `:SEARch:SLOPe:SOURce` | Set / Query | Search source; the set makes the search a slope search (the time of an edge between two levels) | `CHAN1` | Search > Source | name not verified against the DHO guide |
| `:SEARch:SLOPe:SLOPe` | Set / Query | `POSitive`, `NEGative`, `RFALl` | `POS` | Search > Slope | name not verified against the DHO guide |
| `:SEARch:SLOPe:WHEN` | Set / Query | `GREater`, `LESS`, `EQUal` and `UNEQual` (within 5 % of the limit, or not), `RANGe` (inside the two limits), `OUTside` | `LESS` | Search > When | name not verified against the DHO guide |
| `:SEARch:SLOPe:TLOWer` | Set / Query | Seconds, above 0. The limit, or the lower one | `2.000000E-05` | Search > Time | name not verified against the DHO guide |
| `:SEARch:SLOPe:TUPPer` | Set / Query | Seconds, above 0. The upper limit; with one limit the same as `TLOWer` | `4.000000E-05` | Search > Upper limit | name not verified against the DHO guide |
| `:SEARch:SLOPe:ALEVel` | Set / Query | Volts, the upper level | `1.500000E+00` | Search > Upper | name not verified against the DHO guide |
| `:SEARch:SLOPe:BLEVel` | Set / Query | Volts, the lower level; not above the upper one | `5.000000E-01` | Search > Lower | name not verified against the DHO guide |
| `:SEARch:TIMeout:SOURce` | Set / Query | Search source; the set makes the search a timeout search | `CHAN1` | Search > Source | name not verified against the DHO guide |
| `:SEARch:TIMeout:SLOPe` | Set / Query | `POSitive`, `NEGative`, `RFALl`: the edges the time runs from | `POS` | Search > Slope | name not verified against the DHO guide |
| `:SEARch:TIMeout:TIME` | Set / Query | Seconds, above 0, without an edge | `2.000000E-05` | Search > Timeout | name not verified against the DHO guide |
| `:SEARch:TIMeout:LEVel` | Set / Query | Volts | `1.500000E+00` | Search > Level | name not verified against the DHO guide |
| `:SEARch:RUNT:SOURce` | Set / Query | Search source; the set makes the search a runt search | `CHAN1` | Search > Source | name not verified against the DHO guide |
| `:SEARch:RUNT:POLarity` | Set / Query | `POSitive`, `NEGative` | `POS` | Search > Polarity | name not verified against the DHO guide |
| `:SEARch:RUNT:WHEN` | Set / Query | `NONE` (any width), `GREater`, `LESS`, `EQUal` and `UNEQual` (within 5 % of the limit, or not), `RANGe` (inside the two limits), `OUTside` | `LESS` | Search > When | name not verified against the DHO guide |
| `:SEARch:RUNT:WLOWer` | Set / Query | Seconds, above 0. The limit, or the lower one | `2.000000E-05` | Search > Time | name not verified against the DHO guide |
| `:SEARch:RUNT:WUPPer` | Set / Query | Seconds, above 0. The upper limit; with one limit the same as `WLOWer` | `4.000000E-05` | Search > Upper limit | name not verified against the DHO guide |
| `:SEARch:RUNT:ALEVel` | Set / Query | Volts, the upper level | `1.500000E+00` | Search > Upper | name not verified against the DHO guide |
| `:SEARch:RUNT:BLEVel` | Set / Query | Volts, the lower level; not above the upper one | `5.000000E-01` | Search > Lower | name not verified against the DHO guide |
| `:SEARch:WINDow:SOURce` | Set / Query | Search source; the set makes the search a window search (the signal between two levels) | `CHAN1` | Search > Source | name not verified against the DHO guide |
| `:SEARch:WINDow:POSition` | Set / Query | `ENTer` (the signal comes between the levels), `EXIT` (it leaves them), `TIMe` (it was between them for a time that meets `WHEN`; the mark is where it leaves) | `ENT` | Search > Position | name not verified against the DHO guide |
| `:SEARch:WINDow:WHEN` | Set / Query | `GREater`, `LESS`, `EQUal`, `UNEQual`, `RANGe`, `OUTside`; a set makes the position `TIMe` | `LESS` | Search > When | name not verified against the DHO guide |
| `:SEARch:WINDow:TIME` | Set / Query | Seconds, above 0. The limit, or the lower one of `RANGe` and `OUTside` | `2.000000E-05` | Search > Time | name not verified against the DHO guide |
| `:SEARch:WINDow:TUPPer` | Set / Query | Seconds, above 0. The upper limit; with one limit the same as `TIME` | `4.000000E-05` | Search > Upper limit | name not verified against the DHO guide |
| `:SEARch:WINDow:ALEVel` | Set / Query | Volts, the upper level | `1.500000E+00` | Search > Upper | name not verified against the DHO guide |
| `:SEARch:WINDow:BLEVel` | Set / Query | Volts, the lower level; not above the upper one | `5.000000E-01` | Search > Lower | name not verified against the DHO guide |
| `:SEARch:DISPlay` | Set / Query | The marks of the search are drawn (`1`) or hidden (`0`); a hidden search keeps its events, its count, its badge and its list | `1` | Search > Marks | name not verified against the DHO guide |
| `:SEARch:COPY:TO` | Action | `TRIGger` `TRIGger`: copy the search to the trigger. | - | Decode and bus > To trigger | name not verified against the DHO guide |
| `:SEARch:COPY:FROM` | Action | `TRIGger` `TRIGger`: copy the trigger to the search. | - | Decode and bus > From trigger | name not verified against the DHO guide |
| `:SEARch:COUNt` | Query | Number of events in the record shown | `1` | - | name not verified against the DHO guide |
| `:SEARch:EVENt` | Set / Query | Go to event `1`..count (the view moves there); the query answers the current one | `1` | Decode and bus > Event | name not verified against the DHO guide |
| `:SEARch:DATA` | Query | Rows `time,source #n,type,ok`, one per line, like `:DECode:EVENts?` | - | Decode and bus > Event list | name not verified against the DHO guide |
| `:SEARch:SAVE` | Action | Save the events as CSV (`events-*.csv` in the save folder) | - | Decode and bus > Save events | name not verified against the DHO guide |
| `:SEARch:REMove` | Action | Remove the search (Search 1 without a number) | - | Decode and bus > Remove | name not verified against the DHO guide |
| `:SEARch:VALue` | Query | Time of event `n` in seconds (`n` empty: the event last gone to) | - | - | fork command; name not verified against the DHO guide |

## Examples

Mark rising edges and walk through them.

```text
:STOP
:SEARch:EDGE:SOURce CHAN1
:SEARch:EDGE:SLOPe POSitive
:SEARch:EDGE:LEVel:AUTO
:SEARch:STATe ON
:SEARch:COUNt?                         -> 1
:SEARch:EVENt 1
:SEARch:EVENt?                         -> 1
```

Copy the trigger condition into the search and back.

```text
:SEARch:COPY:FROM TRIGger
:SEARch:EDGE:SOURce?                   -> CHAN1
:SEARch:COPY:TO TRIGger
:SEARch:REMove
```

Find the pulses narrower than 100 us, then the runts of any width.

```text
:SEARch:PULSe:SOURce CHAN1
:SEARch:PULSe:POLarity POSitive
:SEARch:PULSe:WHEN LESS
:SEARch:PULSe:UWIDth 100E-6
:SEARch:COUNt?                         -> 1
:SEARch:MODE RUNT
:SEARch:RUNT:WHEN NONE
:SEARch:COUNt?                         -> 0
:SEARch:REMove
```

A second search on the same channel, for the falling edges only.

```text
:SEARch2:EDGE:SOURce CHAN1
:SEARch2:EDGE:SLOPe NEGative
:SEARch2:COUNt?                        -> 1
:SEARch2:EVENt 1
:SEARch2:REMove
```

