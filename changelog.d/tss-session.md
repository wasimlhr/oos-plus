Added:

- A Tektronix session file (`.tss`) loads as a session, so a capture of a faster scope can be analysed on OOS:
  Storage > Load session lists it beside the sessions of OOS (on a stick wherever it lies), and
  `:LOAD:SESSion "name.tss"`, `--open` and a file dropped on the desktop window take it. Its channel records become
  the record on CH1 to CH4 at full length (12.5 million points a channel were tried), with their scales, positions,
  names and the time base; its references go into the slots of their numbers. Its trigger, buses, measurements and
  maths other than its filters are not read, and the message of the load says so. Reference > Load file still puts the waveforms of such a
  file into reference slots ([tss.md](docs/tss.md); host, not yet seen on a unit).
