# System

Everything under the System menu: identification text, the key = value setup schema, the whole-setup block, date, time and NTP, Ethernet and WLAN, the remote services switches, the scope name, touch lock, the quick key and session, and the commands that power off, restart, update, roll back or change the boot mode. Destructive commands take a literal `CONFIRM` argument and are refused with -221 without it; a command that would change nothing (the boot mode it already has) needs none. `:SYSTem:SETup` (long form) is the setup block; the short form `:SYST:SET` stays the older key = value command. The key list behind `:SYSTem:SET` is in [setup keys](setup-keys.md).

Menu location: System (About, System info, Date and time, Network, Remote services, Update, Power), Run control > Power off / restart, Display and layout > Graphics.

Part of the [SCPI reference](README.md).

## Commands

### Diagnostics, schema and setup block

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SYSTem:SETup` | Set / Query | Query: the whole setup (the file Save setup writes) as an IEEE block. Set: apply a setup block (or the plain text); the acquisition state stays as it is. The short form `:SYST:SET` is the key = value command. | - | Storage > Recall setup | OOS extension |
| `:SYSTem:SETup:SCPI` | Query | The setup as SCPI commands, as an IEEE block of lines: `*RST`, then one command for every setting that differs from the Default setup, each with the answer of its own query as the value. A setting without a header of its own is a `:SYSTem:SET` line; the headers are in the Equivalent header column of [setup keys](setup-keys.md). Sent line by line to a scope, it brings that scope to this setup. Reference waveforms and notes are not part of it. A value with a `;` in it (a floating panel of the Studio layout) cannot travel in a line: it is left out and error -221 names its key. `SETup` is spelled out, as for `:SYSTem:SETup`. | - | Storage > Save setup as SCPI | OOS extension. The first part of the SCPI recorder. The menu row writes the same lines to a new file where Storage saves, `setup-NNN.scpi` (`setup-<date>-<time>.scpi` once the clock is set). |
| `:SYSTem:RECorder:STATe` | Set / Query | Boolean. The SCPI recorder on or off, taken at the next frame. While it is on, what is set at the scope (touch, keys, knobs, menus) is written as commands: a setting is written when it has stood still for 0.6 s, so a knob that was turned is one line with the value it was left at. What a client sets by SCPI is not recorded: a comment in the file says where a client set something, and the file does not bring that. What the web remote's keys and touches do is recorded, as the front panel's. The lines go into a new file `recording-NNN.scpi` where Storage saves, written again after every step, so a recording is not lost when the program ends while it is on (a power cut is another matter: the local folder is in memory, and on a stick the file is certain only once the recorder was switched off); it ends by itself at 20000 lines. A key or a row that is an action is written as its command, with what was set before it first (before Default that is only the text size and the Quick key action, which Default leaves alone): Run / Stop (`:RUN`, `:STOP`), Single (`:SINGle`), Force (`:TFORce`), Clear (`:CLEar`), Default (`*RST`), Autoset (`:AUToscale`), and the Storage rows Save setup, Save waveform and Save screenshot (`*SAV <n>`, `:SAVE:WAVeform`, `:SAVE:IMAGe`). Save setup is written with its slot, which is not part of the setup: a replay writes the same slot and leaves the Setup slot row at it. `*OPC?` stands after `:SINGle` and `:AUToscale`. The settings an action changes are its own and are not written as settings; an auto set-up is the command alone, so a replay finds what the signal then has. Recall setup and a loaded session are written as the settings they bring. | `0` | Storage > SCPI recorder | OOS extension. The proof of every step is the one of `:SYSTem:SETup:SCPI?`. |
| `:SYSTem:RECorder:DATA` | Query | The lines of the recording that runs, else of the last one, as an IEEE block: comment lines (`#`), the setup at the start as commands (`*RST` and what differs from the Default setup), then the recorded commands, each step under a comment with its time (`# +12.4 s`). A setting that has not stood still yet is not in it. Empty before the first recording. | - | - | OOS extension |
| `:SYSTem:LOCK` | Set / Query | Touch lock on/off | `0` | - | name not verified against the DHO guide |
| `:SESSion` | Set / Query | `LIVE`, `SIM` or `REF`; setting `LIVE` leaves SIM and offline sessions. | `LIVE` | System > Live | OOS extension |
| `:SYSTem:INFO` | Query | The System info page in one line: each line of the page a quoted string, `"label,value"` or `"# heading"`, separated by commas | - | System > System info | OOS extension |
| `:SYSTem:GRAPhics` | Set / Query | `CPU` or `GPU` drawing; the query answers what is in use (whether the GPU really draws is the `gpu` part of `:SYSTem:STATe?`). | `GPU` | System > Graphics | OOS extension |
| `:SYSTem:QUICk` | Set / Query | The quick key action, by its option text | `SCREENSHOT` | System > Quick key | OOS extension |
| `:IDENtify` | Query | The app's signal identification text. | - | Run control > Identify signal | OOS extension |
| `:SYSTem:STATe` | Query | Diagnostic state text of the app (includes the `gpu` part). | - | - | OOS extension |
| `:SYSTem:GET` | Query | `<key>` Value of a key of the setup schema, empty when the key is unknown. See [setup keys](setup-keys.md). | - | - | OOS extension |
| `:SYSTem:SET` | Set | `<key> = <value>` Write a key of the setup file on the live settings. Unknown keys fail with -224 after the fact. See [setup keys](setup-keys.md). | - | - | OOS extension. `:SYSTem:SETup` (long form) is a different command. |
| `:SYSTem:ERRor` | Query | Remove and return the oldest error as `code,"message"`; `0,"No error"` when empty. `:SYSTem:ERRor:NEXT?` is the same query. | - | - | Rigol-compatible |
| `:SYSTem:VERSion` | Query | The SCPI version the syntax follows. | `1999.0` | - | SCPI-99 |

### Name, date, time and NTP

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SYSTem:DATE` | Set / Query | `yyyy,mm,dd` in the time zone of the settings; setting one part keeps the other part of the clock. | `2026,11,16` | System > Date and time > Set the clock | Rigol-compatible |
| `:SYSTem:TIME` | Set / Query | `hh,mm,ss` in the time zone of the settings. | `16,58,59` | System > Date and time > Set the clock | Rigol-compatible |
| `:SYSTem:TZONe` | Set / Query | The time zone, a POSIX-style string (`CET-1CEST,M3.5.0,M10.5.0/3`) | `UTC0` | Date and time > Time zone | OOS extension |
| `:SYSTem:NTP:ENABle` | Set / Query | NTP on/off | `0` | Date and time > Time from NTP | OOS extension |
| `:SYSTem:NTP:SERVer` | Set / Query | Server name or address, or `DHCP` | `dhcp` | Date and time > NTP server | OOS extension |
| `:SYSTem:NTP:STATus` | Query | The NTP state text | `Off` | System > Date and time > NTP state | OOS extension |
| `:SYSTem:NTP:SYNC` | Action | Sync now | - | System > Date and time > Sync now | OOS extension |
| `:SYSTem:NAME` | Set / Query | The scope's name on the network (letters, digits and `-`, up to 63) | `dho924s` | System > Name | OOS extension |

### Network

Joining a WLAN network needs a password and stays on the screen.

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SYSTem:COMMunicate:LAN:IPADdress` | Query | Address, MAC address or link text (query only). | empty | System > Network > Ethernet | Rigol-compatible |
| `:SYSTem:COMMunicate:LAN:MAC` | Query | Address, MAC address or link text (query only). | `00:15:5d:6d:dc:93` | System > Network > Ethernet | Rigol-compatible |
| `:SYSTem:COMMunicate:LAN:STATus` | Query | Address, MAC address or link text (query only). | `link up, no address yet` | System > Network > Ethernet | Rigol-compatible |
| `:SYSTem:COMMunicate:LAN:RETRy10` | Set / Query | 10 Mbit retry on/off | `1` | Network > 10 Mbit retry | Rigol-compatible |
| `:SYSTem:WLAN:STATus` | Query | WLAN state text. | `no stick` | System > Network > WLAN | OOS extension |
| `:SYSTem:WLAN:SCAN` | Action | Start a scan. | - | System > Network > WLAN | OOS extension |
| `:SYSTem:WLAN:NETWorks` | Query | The scanned networks, each a quoted string, separated by commas in one line. | `Other network ...` | System > Network > WLAN | OOS extension |
| `:SYSTem:WLAN:SAVed` | Query | The saved networks, each a quoted string, separated by commas in one line; empty without any. | empty | System > Network > WLAN | OOS extension |
| `:SYSTem:WLAN:FORGet` | Action | `<n>` Forget saved network `<n>`. Joining a network needs a password and stays on the screen. | - | System > Network > WLAN | OOS extension |

### Remote services

The switches of the Remote services page. `SCPI OFF` needs `,CONFIRM`.

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SYSTem:REMote:SERVice:SCPI` | Set / Query | Boolean switch of the Remote services page; `SCPI OFF` needs `,CONFIRM`. | `1` | System > Remote services > Remote services | OOS extension |
| `:SYSTem:REMote:SERVice:VXI11` | Set / Query | Boolean switch of the Remote services page; `SCPI OFF` needs `,CONFIRM`. | `1` | Remote services > VXI-11 | OOS extension |
| `:SYSTem:REMote:SERVice:TELNet` | Set / Query | Boolean switch of the Remote services page; `SCPI OFF` needs `,CONFIRM`. | `1` | Remote services > telnet | OOS extension |
| `:SYSTem:REMote:SERVice:WEB` | Set / Query | Boolean switch of the Remote services page; `SCPI OFF` needs `,CONFIRM`. | `1` | Remote services > Web remote | OOS extension |
| `:SYSTem:REMote:SERVice:MDNS` | Set / Query | Boolean switch of the Remote services page; `SCPI OFF` needs `,CONFIRM`. | `1` | Remote services > mDNS | OOS extension |

### Power, update and boot

These take an explicit `CONFIRM` argument and are refused with -221 without it. Power off and restart are refused while an update runs.

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SYSTem:POWer:OFF` | Action | `CONFIRM` `CONFIRM` required. Power off; refused while an update runs. | - | System > Update and power > Power dialog | OOS extension |
| `:SYSTem:POWer:RESTart` | Action | `CONFIRM` `CONFIRM` required. Restart; refused while an update runs. | - | System > Update and power > Power dialog | OOS extension |
| `:SYSTem:UPDate:STATus` | Query | The Update page in one line: each of its lines a quoted string, separated by commas | `Update` | System > Update and power > Update | OOS extension |
| `:SYSTem:UPDate:ROLLback` | Action | `CONFIRM` Roll back to the kept image | - | System > Update and power > Roll back update | OOS extension |
| `:SYSTem:UPDate:INSTall` | Action | `"path",CONFIRM` Install a signed package file (same checks as the web page) | - | System > Update and power > Update | OOS extension |
| `:SYSTem:BOOT` | Set / Query | `ONEShot`, `PERSist`; a change needs `,CONFIRM` | `ONES` | System > Update and power > Boot mode | OOS extension |
| `:SYSTem:OVERclock` | Set / Query | ADC clock in Hz, one of the board's steps; a change needs `,CONFIRM` | `1.250000E+09` | System > Update and power > Overclock | OOS extension |
| `:SYSTem:STOCk` | Action | `CONFIRM` What Back to stock does: says whether the next power-on is stock | - | System > Update and power > Back to stock | OOS extension |

## Examples

Date, time and time zone.

```text
:SYSTem:DATE 2026,11,16
:SYSTem:TIME 12,30,0
:SYSTem:DATE?                          -> 2026,11,16
:SYSTem:TZONe?                         -> UTC0
```

Remote services; the destructive commands need CONFIRM.

```text
:SYSTem:REMote:SERVice:SCPI?           -> 1
:SYSTem:REMote:SERVice:WEB ON
:SYSTem:REMote:SERVice:SCPI OFF
:SYSTem:ERRor?                         -> -221,"Settings conflict" Switching SCPI off needs the argument CONFIRM
:SYSTem:POWer:OFF
:SYSTem:ERRor?                         -> -221,"Settings conflict" Power off needs the argument CONFIRM
```

Whole setup and a key.

```text
:SYSTem:SET tdiv = 5
:SYSTem:GET? tdiv                      -> 5
:SYSTem:NAME "bench-scope"
:SYSTem:NAME?                          -> bench-scope
:SYSTem:BOOT?                          -> ONES
```

