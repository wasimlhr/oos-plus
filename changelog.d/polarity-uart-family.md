Added:

- Decode > Polarity (Normal, Inverted, Auto) for UART, Modbus RTU and Modbus ASCII: a line idle low, or a pair wired
  the wrong way round, is read inverted without inverting the channel; Auto decodes both ways and keeps the reading
  with fewer failed characters, parities and checksums. On UART the decode menu's copy of the serial trigger's
  Polarity row is replaced by it (the decoder still follows the trigger's setting; Inverted reverses that). Over
  SCPI: `:DECode:POLarity` and `:BUS<n>:POLarity`. Old setups load as Normal.
  Host: generated inverted UART, Modbus RTU and ASCII lines; not yet seen on a unit.
