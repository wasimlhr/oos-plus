# Save and load

Save and load setups, waveforms, screenshots and the fast-frame history, and fetch what was saved. A relative path of a save is taken in the save folder. The save folder is the USB stick when one is in (`:SAVE:DESTination`), else the scope's local folder, which is in memory and empty again after a power cycle: fetch a file with `:MMEMory:DATA?` or save to a stick to keep it. Setup slots are kept on the card. `*SAV` and `*RCL` (see [common](common.md)) and the setup block `:SYSTem:SETup` (see [system](system.md)) are the same setup files.

Menu location: Storage (Save destination, Setup slot, Save setup, Recall setup, Save waveform, Save screenshot, Fast-frame save / load).

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
| `:MMEMory:DATA` | Query | `"name"` The bytes of a file of the save folder as an IEEE block. A name with a directory part or a colon is refused (-224); a file that is not there is -256. | - | - | SCPI-99 |
| `:MMEMory:DELete` | Action | `"name"` Remove a file of the save folder. | - | - | SCPI-99 |
| `:SAVE:IMAGe` | Action | `["path"]` Screenshot as PNG, written after the next frame | - | Storage > Save screenshot | Rigol-compatible |
| `:SAVE:FFRame` | Action | Save the fast-frame history (`frame-NNN.bin` in the save folder). | - | Storage > Save fast-frame | OOS extension |
| `:LOAD:FFRame` | Action | Load the fast-frame history. | - | Storage > Load fast-frame | OOS extension |

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

