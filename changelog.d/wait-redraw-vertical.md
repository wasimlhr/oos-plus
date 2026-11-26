Fixed:

- A new volts per division, offset or channel while the scope waits for a trigger draws the last record again at its
  own volts. The picture kept its old height and place before, so the trace read wrong values against the grid until
  the next trigger ([First Steps](wiki/First-Steps.md)).
