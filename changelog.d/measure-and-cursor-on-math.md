Fixed:

- `:MEASure:ITEM <item>,MATH1` adds a row for a math trace instead of refusing it, and `:CURSor:MANual:SOURce?`
  after `MATH1` answers `MATH1`, not the reference slot that holds its copy.
- The MCP tool `decode_results` says that rows read while the scope runs are those of the record on the screen, and
  that a stopped scope gives a complete list.
