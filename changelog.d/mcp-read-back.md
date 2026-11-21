Changed:

- MCP `scpi` reads back every setting it sent that has a query (`read back :CHAN1:SCAL? -> 5.000000E-1`), and the
  rules MCP gives an assistant say look up, send, verify: a command that was sent is not yet one that worked
  ([Remote control](wiki/Remote-Control.md#mcp)).
