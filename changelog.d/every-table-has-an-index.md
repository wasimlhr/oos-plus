Changed:

- Every table has an Index (the number of the row in the list of the record, from 1) and can be read and saved the
  same way: `:TABLe:LIST?`, `:TABLe:COUNt? <name>`, `:TABLe:DATA? <name>` (CSV in a block) and `:TABLe:SAVE <name>`
  for the decode table, the fields, the search events, the segments, the spectrum peaks and the measurements; the
  MCP tool `table` reads them. The Events pane's count line says "36 words decoded" (`:DECode:EVENts:COUNt?`), its
  CSV file and the rows of `:DECode:EVENts?` and `:BUS<n>:DATA?` begin with the Index.
- The Events pane lists MIL-STD-1553 as the Tektronix decode table does: the status word as bits (000-000-00001) and
  the 11 bits of a command or status word (C/S payload) beside the flag names, and an Error in words per message:
  incomplete packet, unframed, word count mismatch, noncontiguous data, Manchester or parity error. The MCP tool
  `decode_results` answers the same rows and columns, and `setup_decoder` says how to open the table
  (`:DISPlay:LOWer EVENTS`). Host: the 1553 demo; not yet seen on a unit.
