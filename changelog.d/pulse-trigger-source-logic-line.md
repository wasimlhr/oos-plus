Changed:

- SCPI: `:TRIGger:PULSe:SOURce D0`..`D15` selects a logic line for the pulse trigger while the logic analyser is on,
  as `:TRIGger:EDGE:SOURce` does; it was refused with "Invalid trigger source". The pulse trigger's commands (source,
  polarity, when, widths, level) are tested set and read back on the host, and the wiki and the reference list them
  (host; not yet seen on a unit).
