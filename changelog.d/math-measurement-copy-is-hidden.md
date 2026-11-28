Fixed:

- A measurement or a cursor on a math channel (`:MEASure:SLOT1 VPP,MATH1`, the MCP tools) saved a copy of the math
  trace into the first free reference slot and showed it: an R1 appeared on the plot and Studio offered a SIM button
  beside Run, which played that copy when tapped. The copy is now hidden; the measurement still reads it, and a copy
  you show yourself stays shown. Host: test with a hidden copy; not yet seen on a unit.
