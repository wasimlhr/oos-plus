# Save and load

Save and load setups, waveforms, screenshots and the fast-frame history, and fetch what was saved. A relative path of a save is taken in the save folder. The save folder is the USB stick when one is in (`:SAVE:DESTination`), else the scope's local folder, which is in memory and empty again after a power cycle: fetch a file with `:MMEMory:DATA?` or save to a stick to keep it. Setup slots are kept on the card. `*SAV` and `*RCL` (see [common](common.md)) and the setup block `:SYSTem:SETup` (see [system](system.md)) are the same setup files.

Menu location: Storage (Save to, Setup slot, Save setup, Recall setup, Save setup to file, Load setup from file, Save waveform, Fast-frame save / load, Save session, Load session, Save screenshot, Save setup as SCPI, SCPI recorder, Run SCPI file). Save setup as SCPI writes the lines of [`:SYSTem:SETup:SCPI?`](system.md) to a file. SCPI recorder writes what is set at the scope as commands while it is on and saves them as a file when it is switched off ([`:SYSTem:RECorder:STATe`](system.md)). Run SCPI file runs a file of commands on this scope: a recording, a setup saved as commands, or lines written by hand. The scope lists the `.scpi` files where Storage saves, the newest first; the desktop app asks with its Open dialog. The lines are run like a client's, one command a frame; a line that waits for an operation (`*OPC?`) is waited for, for at most 30 s; comment lines (`#`) are no commands. While a file runs the row reads Stop SCPI file. At the end the screen says how many commands were run, and the first one that was refused if any was; those errors do not reach a client: its error queue, event status and last error are as they were. A file sets the scope up, runs it and saves into the save folder, and no more ([ADR 0033](../adr/system/0033-a-file-of-commands-sets-the-scope-up-and-no-more.md)): refused from a file, and said as refused with -203, are a command with `CONFIRM` (the word would be the author's and not yours), every `:UI:` command that is not a query (a file does not press the screen), every `:SYSTem:` command that is not a query except `:SYSTem:SET`, `:SYSTem:SETup` and the ones a setup is written with (`:SYSTem:LOCK`, `:SYSTem:AOUTput`, `:SYSTem:PIPeline`, `:SYSTem:LOG:INTerval`), `:CALibrate:STARt`, `:MMEMory:DELete`, and a save or load whose file name is an absolute path or steps up with `..`: a file names files of the save folder. The rest of the file runs. A file saved by an editor with a byte order mark, or with CR LF or CR line ends, runs; a link or anything else that is not a regular file is not read. There is no command for this row: a client sends the lines itself.

In the menu, a row that writes a file asks for its name first and a row that reads one asks which file ([ADR 0029](../adr/ui-ux/0029-a-storage-row-asks-for-the-name-of-its-file.md)): the on-screen keyboard and a list of the folder on the scope, the Save As and Open dialogs on the desktop build. The setup slots do not ask; Save setup to file and Load setup from file are `:SAVE:SETup` and `:LOAD:SETup` with a path. A command never asks: a path it is given is used as given, and without one it takes the next free name (a save) or the newest numbered file (a load), as the table says. A setup saved by name from the menu goes to the stick when files go there, else beside the slots (on the scope `/oos/setups`); `:LOAD:SETup` loads it by its full path.

Part of the [SCPI reference](README.md).

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes |
| --- | --- | --- | --- | --- | --- |
| `:SAVE:DESTination` | Set / Query | `AUTO`, `USB`, `DESKtop` (the local storage directory) | `AUTO` | Storage > Save to | OOS extension |
| `:SAVE:SLOT` | Set / Query | Setup slot 1 to 9 | `1` | Storage > Setup slot | OOS extension |
| `:SAVE:SETup` | Action | `["path"]` Save the current slot, or `"path"`. | - | Storage > Save setup | Rigol-compatible |
| `:LOAD:SETup` | Action | `["path"]` Recall the current slot, or `"path"`. | - | Storage > Recall setup | Rigol-compatible |
| `:SAVE:WAVeform` | Action | `["path"[,CSV or BIN]]` Newest record to a file (default CSV, in the save folder) | - | Storage > Save waveform | Rigol-compatible |
| `:SAVE:MEMory:WAVeform` | Action | `["path"[,CSV or BIN]]` Every sample the acquisition kept of the channels that are on, where `:SAVE:WAVeform` writes the 3600 points of the screen. The format follows the file name (`.csv`), else BIN; CSV is refused above 1 M points per channel (-221); a file that cannot be written is -250. In peak detect, roll and above 1 M points there are no single samples and the screen points are written. | - | - | name not verified against the DHO guide |
| `:MMEMory:CATalog` | Query | The files of the save folder: bytes used, bytes free, then `"name,,size"` for each. | `0,0` | - | SCPI-99 |
| `:MMEMory:DATA` | Query | `"name"` The bytes of a file of the save folder as an IEEE block. A name with a directory part or a colon is refused (-224); a session saved under a relative name lies beside the setups, not in this folder, so save it without a name to fetch it; a file that is not there is -256. | - | - | SCPI-99 |
| `:MMEMory:DELete` | Action | `"name"` Remove a file of the save folder. | - | - | SCPI-99 |
| `:SAVE:IMAGe` | Action | `["path"]` Screenshot as PNG, written after the next frame | - | Storage > Save screenshot | Rigol-compatible |
| `:SAVE:FFRame` | Action | Save the fast-frame history (the next `frame-NNN.bin` in the save folder; the menu row asks for the name). | - | Storage > Save fast-frame | OOS extension |
| `:LOAD:FFRame` | Action | Load the fast-frame history (the newest `frame-NNN.bin` of the save folder; the menu row lists the files). | - | Storage > Load fast-frame | OOS extension |
| `:SAVE:SESSion` | Action | `["path"]` The setup, the record (every sample the acquisition kept, up to 1 M points; else the points of the screen) and the loaded reference slots as one file (`session-NNN.oss` in the save folder, or `"path"`). Always OOS's own file, also after a session of another instrument was loaded. | - | Storage > Save session | OOS extension |
| `:LOAD:SESSion` | Action | `["path"]` Load a session (the newest `session-NNN.oss`, or `"path"`): the scope stops and shows it; reference slots the file does not hold are emptied. A path can name the session file of another instrument (`.tss`, known by its first bytes): its channel records become the record, its references go into their slots, and of its settings the scales and the time base are taken ([tss.md](../tss.md)). A file that cannot be read is -224 and changes nothing. | - | Storage > Load session | OOS extension |

## Examples

Save to a slot and recall.

```text
:SAVE:DESTination AUTO
:SAVE:SLOT 3
:SAVE:SETup
:LOAD:SETup
:SAVE:SLOT?                            -> 3
```

Save the acquisition memory on the scope and fetch the file.

```text
:STOP
:SAVE:MEMory:WAVeform "capture.bin"
:MMEMory:CATalog?                      -> 400036,1990000000,"capture.bin,,400036"
:MMEMory:DATA? "capture.bin"           -> #9000400036OOSW...
:MMEMory:DELete "capture.bin"
```

A missing file of `:LOAD:SETup` or `:MMEMory:DATA?` is error -256.

Save the newest waveform as CSV.

```text
:STOP
:SAVE:WAVeform
:SAVE:IMAGe
```

