# Run control and acquisition

Run, stop, single, autoset and clear; the acquisition type, averages, memory depth and the segmented frame count; and the history of stored acquisitions. There is one acquisition context: a loaded reference beside live channels is a passive overlay, and selecting a reference as the primary trigger source enters stored-record acquisition. `:RUN` and `:STOP` are assignments, so repeating either does not toggle it.

Menu location: Run control (Run / Stop, Single, Autoset, Clear), Acquisition (Mode, Averages, Memory depth, Segments), Run control > History.

Part of the [SCPI reference](README.md).

## Commands

### Run control

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:RUN` | Action | Start continuous acquisition. Idempotent assignment; also starts stored replay and SIM. After a Single the sweep returns to what it was before it (Auto or Normal). | - | Run control > Run / Stop | Rigol-compatible |
| `:STOP` | Action | Stop acquisition. Idempotent assignment; also stops stored replay and a SIM generation. | - | Run control > Run / Stop | Rigol-compatible |
| `:SINGle` | Action | Arm once: wait for a qualified capture, then stop. | - | Run control > Single | Rigol-compatible |
| `:TFORce` | Action | Accept a capture without a trigger match (software force on stored/SIM acquisition, one hardware pulse live). | - | Run control > Force trigger | Rigol-compatible. Alias of `:TRIGger:FORCe`. |
| `:AUToscale` | Set / Query | Start ADC Autoset; the query answers `1` while it runs, else `0`. Autoset does not fit a reference. | `0` | Run control > Autoset | Rigol-compatible |
| `:CLEar` | Action | Clear display persistence and measurement statistics (the Clear action). | - | Run control > Clear display / stats | Rigol-compatible |
| `:RECord:CURRent` | Set / Query | History frame on screen, `1` (oldest) to the count; a set stops the acquisition | newest | Acquire > History | fork command; name not verified against the DHO guide |
| `:RECord:WREPlay:FCURrent` | Set / Query | The same frame as `:RECord:CURRent` | newest | Acquire > History | fork command; name not verified against the DHO guide |
| `:RECord:WREPlay:FMAX` | Query | Number of stored frames | - | - | fork command; name not verified against the DHO guide |
| `:NAVigate:FRAMe:{NEXT\|BACK\|STARt\|END}` | Action | Step through the stored frames | - | Acquire > History | fork command; name not verified against the DHO guide |

### Acquisition

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes | What it does | See also |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `:ACQuire:SRATe` | Query | ADC/model sample rate, not a loaded reference's native rate. | `2.500000E+08` | Acquisition > Memory depth (read-out) | Rigol-compatible | - | - |
| `:ACQuire:MDEPth` | Set / Query | Set `AUTO` or a supported menu depth, e.g. `1000000`, `1M` (1k, 10k, 100k, 1M, 10M, 25M, 50M, 62.5M, 100M, 125M; what the unit has is cut to its memory). In roll mode (Auto Roll on and 200 ms/div or slower; stock rolls from 50) the record is the stream's points of the screen whatever the depth: the depth is kept for the faster time bases and the set is answered with -221 that names the depth, the reason and the way out (`:TIMebase:ROLL OFF`, or a time base faster than 200 ms/div). A value that is none of them is -224, and the error names the depths this unit takes (`5M` is none of them). Query returns the numeric configured/effective acquisition depth: the depth of the record the scope works with, so a depth set while stopped, or in roll (which has its own depth), reads back only once an acquisition with it has run. | `5.000000E+05` | Acquire > Memory | Rigol-compatible | - | - |
| `:ACQuire:TYPE` | Set / Query | `NORMal\|PEAK\|AVERages\|HRESolution`. | `NORM` | Acquire > Mode | Rigol-style (command family of the DHO guide) | - | - |
| `:ACQuire:AVERages` | Set / Query | Requested average count, mapped to supported powers of two. Read back the selected count. | `16` | Acquire > Averages | Rigol-style (command family of the DHO guide) | - | - |
| `:ACQuire:CORes` | Set / Query | `MATCh\|RAW` (also `ON`, `OFF`, `1`, `0`); the query answers `MATC` or `RAW`. Matched takes the offsets of the interleaved ADC cores off in software; Raw leaves the words as they come. | `MATC` | Acquire > ADC cores | OOS extension | Choose whether the offsets of the interleaved ADC cores are removed in software (matched) or the raw words are kept. | `:ACQuire:SRATe` |
| `:ACQuire:AVERages:HARDware` | Set / Query | `ON\|OFF` (`1`, `0`); the query answers `1` or `0`. In Average mode the FPGA averages where the board can, else the software does. Experimental, not verified on a unit. | `0` | Acquire > HW average | OOS extension | Let the FPGA do the averaging in Average mode where the board can. | `:ACQuire:TYPE`, `:ACQuire:AVERages` |
| `:ACQuire:SEGMents` | Set / Query | `<n>\|OFF` segmented recording: the segment count without the menu's steps. More than 100000 is set to 100000, `OFF` or `0` switches it off, another value is -224. The query answers the count, `0` when off. Where the segments are recorded is `:ACQuire:SEGMents:HARDware`. `:ACQuire:ULTRa:MAXFrame` sets the same count to the nearest menu step. | `0` | Acquire > Segments | OOS extension | Record a number of segments (frames) one after the other, or switch segmented recording off. | `:ACQuire:SEGMents:HARDware`, `:ACQuire:SEGMents:INTerval`, `:ACQuire:SEGMents:TIMes`, `:ACQuire:ULTRa:MAXFrame` |
| `:ACQuire:SEGMents:HARDware` | Set / Query | `ON\|OFF` (`1`, `0`); the query answers `1` or `0`. Segments are recorded by the FPGA instead of the software history. Experimental, not verified on a unit. | `0` | Acquire > HW segments | OOS extension | Record the segments in the FPGA instead of the software history. | `:ACQuire:SEGMents` |
| `:ACQuire:SEGMents:INTerval` | Set / Query | Seconds, 0 to 10: the pause after each hardware segment. Not verified on a unit. | `0` | Acquire > Segment interval | OOS extension | Set the pause after each hardware segment, in seconds. | `:ACQuire:SEGMents` |
| `:ACQuire:SEGMents:TIMes` | Query | Comma-separated seconds: the trigger time of each recorded hardware segment since the first one, `9.9E37` for a segment without a time stamp; empty before segments are recorded. Not verified on a unit. | - | - | OOS extension | Read the trigger time of each recorded hardware segment. | `:ACQuire:SEGMents`, `:ACQuire:SEGMents:HARDware` |
| `:ACQuire:ULTRa:MAXFrame` | Set / Query | Supported segmented frame count. This does not claim the stock UltraAcquire capture rate. | `0` | Acquire > Segments | Rigol-style (command family of the DHO guide) | - | - |

### History

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:ACQuire:HISTory:ENABle` | Set / Query | `ON` shows the newest stored record (stops the acquisition), `OFF` goes back to live (Run). -221 when nothing is stored. | `0` | Run control > History / Live | name not verified against the DHO guide |
| `:ACQuire:HISTory:FRAMe` | Set / Query | Record `1`..count; `0` when not in history | `0` | Run control > History, Back, Forward | name not verified against the DHO guide |
| `:ACQuire:HISTory:COUNt` | Query | Number of stored records | `200` | - | name not verified against the DHO guide |
| `:ACQuire:HISTory:NEXT` | Action | One record on (the Display menu's Forward). | - | Run control > History: newer | name not verified against the DHO guide |
| `:ACQuire:HISTory:PREVious` | Action | One record back (the Display menu's Back). | - | Run control > History: older | name not verified against the DHO guide |

## Examples

Stop, pick averaging and read it back.

```text
:STOP
:ACQuire:TYPE AVERages
:ACQuire:AVERages 16
:ACQuire:TYPE?                         -> AVER
:ACQuire:AVERages?                     -> 16
```

Memory depth and sample rate.

```text
:ACQuire:MDEPth?                       -> 5.000000E+05
:ACQuire:MDEPth 1M
:ACQuire:MDEPth?                       -> 1.000000E+06
:ACQuire:SRATe?                        -> 2.500000E+08
```

Walk the acquisition history.

```text
:STOP
:ACQuire:HISTory:COUNt?                -> 200
:ACQuire:HISTory:ENABle ON
:ACQuire:HISTory:FRAMe?                -> 200
:ACQuire:HISTory:PREVious
:ACQuire:HISTory:FRAMe?                -> 199
:ACQuire:HISTory:ENABle OFF
```

