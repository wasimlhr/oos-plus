Added:

- An assistant that connects over MCP is told what the scope can do beyond the typed tools (arrange the screen,
  simulate signals, open plots and analysers, press menus): `scpi_reference` without arguments lists it by task,
  with the subsystem and the command to start with, and a search for "move a window" or "simulate a signal" finds
  `:DOCK:MOVE` and `:SIM:PLAY`.
