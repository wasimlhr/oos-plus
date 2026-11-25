Changed:

- MCP `scpi_reference`: an AI assistant finds a command in its own words. A search ranks the commands by how well
  they fit instead of asking for every word, and knows what is said for a word of the reference ("volts per
  division", "record length", "glitch", "i2c"); it also answers the measurement item, eye figure or error code that
  fits ("peak to peak" is `VPP` of `:MEASure:ITEM?`). New: `appendix` reads a table of the manual's appendices,
  `subsystem` adds the manual's introduction and examples of one subsystem, and `offset` goes on in a long list
  ([Remote control](wiki/Remote-Control.md#mcp)).
