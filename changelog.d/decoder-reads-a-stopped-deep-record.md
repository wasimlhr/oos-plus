Changed:

- A decoder now works on a stopped deep record (above 1 M points): the stop reads the raw window of the screen, every
  channel that is on, and a window over the budget of one read is read as its largest part around the centre. The
  message for a stopped record that is still on pairs no longer says "stop it". Host only; the read of the window is
  not yet seen on a unit.
