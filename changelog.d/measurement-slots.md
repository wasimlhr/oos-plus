Added:

- Numbered measurement slots `MEAS1` to `MEAS99`: `:MEASure:SLOT<n> <item>,<source>` adds a measurement, which is then
  computed and is a row of the measurement table with the badge M<n>, and `:MEASure:SLOT<n>?` reads it back by its
  number. An empty slot is an error that says so, not 9.9E37; the number of a row stays when other rows are removed.
  `:MEASure:ITEM?` is unchanged ([SCPI and VXI-11](wiki/SCPI-and-VXI-11.md)).
- The MCP tool `measure_slot` adds a measurement to a slot and reads it back, so a value an assistant uses is also on
  the screen ([Remote Control](wiki/Remote-Control.md)).
