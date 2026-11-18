# Common commands

The IEEE 488.2 common commands: identification, reset, the status registers (`*STB?`, `*ESE`, `*SRE`, `*ESR?`) and the operation-complete fence, plus `*SAV` and `*RCL`, which save and recall a numbered setup slot. `*OPC?` waits only for finite operations (Autoset, a submitted SIM generation, an armed Single); continuous Run and background analysis are not drained. `*SAV` and `*RCL` use the same files as Storage > Setup slot, so they are cross-linked from [storage](storage.md).

Menu location: Run control > Default setup; Storage > Save setup, Recall setup.

Part of the [SCPI reference](README.md).

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `*SAV` | Set | `<n>` Save setup slot `n` (the file `setup<n>.txt`). | - | Run control > Save setup | Rigol-compatible |
| `*RCL` | Set | `<n>` Recall setup slot `n` (the file `setup<n>.txt`). | - | Run control > Recall setup | Rigol-compatible |
| `*IDN` | Query | `OOS,<model>,<serial>,oos-ui <version>`; the desktop serial can be `0`. | `OOS,DHO924S,0,oos-ui 0.1.12` | System > About | Rigol-compatible. Not a Rigol identity string. |
| `*RST` | Action | Restore the default configuration (the Default key): the set-up, the notes on the signals, the vertical zoom and the waveform read-out (`CHAN1`, `NORMal`, `BYTE`, the whole record). A new set-up is in the Default layout with the stock bars. The text size and the Quick key stay, as do `*ESE` and `*SRE`. | - | Run control > Default setup | Rigol-compatible |
| `*CLS` | Action | Clear the error queue, the event status and the pending `*OPC` registrations. Waveform memory is kept. | - | - | Rigol-compatible |
| `*OPC` | Set / Query | Set: register completion of the finite operations (Autoset, a submitted SIM generation, an armed Single) in standard-event bit 0. Query: wait for them, then reply `1`. | `1` | - | Rigol-compatible. See the operation completion section in the [overview](README.md). |
| `*ESR` | Query | Read and clear the event bits: OPC 0, query error 2, device error 3, execution error 4, command error 5. | `0` | - | Rigol-compatible. Starts clear; there is no power-on bit and no SRQ. |
| `*ESE` | Set / Query | `0` to `255`: which bits of `*ESR` set the event summary bit (5) of `*STB?`. | `0` | - | IEEE 488.2 |
| `*SRE` | Set / Query | `0` to `255`: which bits of `*STB?` set its master summary bit (6); bit 6 itself reads 0. A socket has no service request line, so nothing else follows from it. | `0` | - | IEEE 488.2 |
| `*STB` | Query | Status byte: bit 2 an error is queued, bit 5 an enabled `*ESR` bit is set, bit 6 an enabled bit of this byte is set. Not cleared by reading. There is no message-available bit. | `0` | - | IEEE 488.2 |
| `*PSC` | Set / Query | Power-on status clear flag, `0` or `1`. The enable registers start at 0 either way; the flag is kept for drivers that set it. | `1` | - | IEEE 488.2 |
| `*TST` | Query | `0`: there is no self test to fail. | `0` | - | IEEE 488.2 |
| `*OPT` | Query | `0`: no options are reported. | `0` | - | IEEE 488.2 |
| `*WAI` | Action | Accepted and does nothing: a command is finished when the next one is read. Wait for an armed Single with `*OPC?`. | - | - | IEEE 488.2 |
| `*TRG` | Action | Force a trigger, as `:TFORce`. | - | Run control > Force | IEEE 488.2 |

## Examples

Identify, reset, and fence an operation.

```text
*IDN?                                  -> OOS,DHO924S,0,oos-ui 0.1.12
*CLS
*RST
*OPC?                                  -> 1
```

Save a setup slot, change it, recall it.

```text
:CHANnel1:SCALe 0.5
*SAV 2
:CHANnel1:SCALe 2
*RCL 2
:CHANnel1:SCALe?                       -> 5.000000E-01
```

Errors latch the event status; reading them clears the queue.

```text
:NOSUCH:HEADER
*ESR?                                  -> 32
:SYSTem:ERRor?                         -> -113,"Undefined header" :NOSUCH:HEADER
:SYSTem:ERRor?                         -> 0,"No error"
```

