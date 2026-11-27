Added:

- Decode > Polarity (Normal, Inverted, Auto) for USB low and full speed: with D+ and D- swapped every sync and PID
  failed; Inverted exchanges the two sources (with D+ alone it mirrors the line), Auto decodes both ways and keeps the
  reading with fewer failed fields. DCC reads a swapped pair as it is and has no row. Old setups load as Normal.
  Host: generated USB traffic with the pair swapped; not yet seen on a unit.
