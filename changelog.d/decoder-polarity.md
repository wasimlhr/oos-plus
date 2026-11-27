Added:

- Decode > Polarity (Normal, Inverted, Auto) for MIL-STD-1553, ARINC 429 (Normal and Inverted), DMX512 and UART
  9-bit: a bus that is connected the wrong way round is read inverted without inverting the channel on the screen.
  Auto decodes both ways and keeps the reading with fewer failed words. A decoder that reads inverted says
  "(inverted)" on its band. Over SCPI: `:DECode:POLarity` and `:BUS<n>:POLarity`. Old setups load as Normal.
  Host: generated inverted 1553 and ARINC traffic; not yet seen on a unit ([Features](wiki/Features.md)).
