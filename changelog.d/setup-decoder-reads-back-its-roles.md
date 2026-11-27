Changed:

- The MCP tool `setup_decoder` answers with the second source only when it was asked for one, so a MIL-STD-1553 or
  UART decoder no longer shows a clock source it does not have. The docs say that `:DECode:AUTO?` only reads, that the
  automatic set-up picks the smallest record, and that `:ZOOM:BOX 0,0` is refused (`:ZOOM:CLOSe` closes the zoom).
