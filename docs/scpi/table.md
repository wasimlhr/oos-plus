# Tables

Every table the screen shows, by name, with its **Index** in the first column: the number of the row in the list of
the record, from 1, the same in the pane, in the CSV file and in the answer. One mechanism for all of them: list the
names, count the rows, read the table, save it as a CSV file. The measurement strip is the table `MEASURE`.

Menu location: the Save (CSV) buttons of the tables; Decode and bus > Export events (the decode table).

Part of the [SCPI reference](README.md).

## Tables

| Name | What it is | Columns after Index |
| --- | --- | --- |
| `EVENTS` | The decode table, the Events pane: a row per message, field or line (`:DECode:EVENts:VIEW`). | Time (s), End (s), Source, Len, then the columns of the protocol (1553 fields: Type, RT, T/R, SA, Mode code / words, Data, Status, Status bits, C/S payload) or Text and Hex, then Error. |
| `FIELDS` | A row per field of every decoder, the rows of `:DECode:EVENts?`. | Time (s), Source, Event, Ok. |
| `SEARCH` | The events of the search. | Time (s), Source, Event. |
| `SEGMENTS` | The hardware segments read back. | Time since first (s), Step. |
| `PEAKS` | The peak table of the spectrum. | Frequency (Hz), Level (dBV). |
| `MEASURE` | The measurement strip. | Item, Source, Value. |

## Commands

| Command | Set/Query | Parameters and range | Default | Menu equivalent | Notes | What it does | See also |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `:TABLe:LIST` | Query | The names of the tables, a line each: `NAME,what it is`. | `EVENTS,decode table: ...` | Tables > list | OOS extension | Lists the tables the screen shows, to read or save as a list of rows with an index. | `:TABLe:DATA`, `:TABLe:COUNt` |
| `:TABLe:COUNt` | Query | `<name>`: a table of the list. The number of its rows; `BUSY` while a decoder computes. | `0` | Tables > rows | OOS extension | How many rows a table has, such as "36 packets decoded" in the decode table. | `:TABLe:DATA`, `:DECode:EVENts:COUNt` |
| `:TABLe:DATA` | Query | `<name>`: a table of the list. A definite-length block of CSV text: the titles, then a row per line, Index first; a cell with a comma is in quotes. `BUSY` while a decoder computes. | - | Tables > CSV | OOS extension | Reads a whole table as CSV, with the row number (index) first: the decode table, the search events, the measurements, the peaks. | `:TABLe:SAVE`, `:TABLe:LIST` |
| `:TABLe:SAVE` | Action | `<name>[,"path"]`: writes the table as CSV into the save folder, as `<name>-*.csv`, or to the path. | - | Tables > Save as CSV | OOS extension | Saves a table of the screen as a CSV file with its index column. | `:TABLe:DATA`, `:DECode:SAVE` |
| `:DECode:EVENts:COUNt` | Query | The rows the Events pane lists in its view (messages, fields or lines), without the "Errors only" filter; `BUSY` while pending. | `0` | Events pane > count line | OOS extension | How many messages, words or fields the decode table holds ("N packets decoded"). | `:DECode:EVENts`, `:DECode:EVENts:VIEW` |

## Examples

```
:DECode:EVENts:VIEW FIELds
:DECode:EVENts:COUNt?
:TABLe:DATA? EVENTS
:TABLe:SAVE EVENTS
:TABLe:SAVE MEASURE,"/rigol/data/measure.csv"
```
