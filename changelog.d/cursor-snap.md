Added:

- Time cursors can snap to edges: **Cursor > Snap to edge** (Off, Rising, Falling, Either) puts a time cursor on the
  nearest crossing of the source's mid-level when it is dropped, and **Snap cursors now** does it for both at once, so
  the time between two edges reads as the delta time and 1/delta. The same by `:CURSor:SNAP:EDGE` and
  `:CURSor:SNAP:EXECute`. Mode **XY** keeps time and voltage cursor pairs on together, with both read-outs
  ([Features](wiki/Features.md), [Remote control](wiki/SCPI-and-VXI-11.md)).
