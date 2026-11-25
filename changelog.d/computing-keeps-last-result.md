Fixed:

- With a large record (1 Mpts) and the scope running, a math trace and a decoder band no longer say "Computing" for ever
  on the screen and on the external display: they show the last finished result, one record old, until the new one is
  ready. Remote answers (`:BUS<n>:DATA?`, the waveform of a math channel) still report the exact result or "BUSY".
