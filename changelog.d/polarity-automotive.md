Added:

- Decode > Polarity (Normal, Inverted, Auto) for CAN, CAN-FD, LIN and FlexRay: a bus probed as CAN_H where the decoder
  expects RX/TX, an isolator that inverts LIN, or a swapped FlexRay pair is read inverted without inverting the
  channel; Auto decodes both ways and keeps the reading with fewer stuffing, checksum and CRC errors. On CAN the row
  is relative to the Signal row. SENT, S/PDIF, USB PD and MLT-3 carry their data in the transitions and read a
  swapped pair as it is. Old setups load as Normal.
  Host: generated inverted CAN, CAN-FD, LIN and FlexRay lines; not yet seen on a unit.
