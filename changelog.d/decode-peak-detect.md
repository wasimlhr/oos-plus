Fixed:

- The decoders and the protocol trigger read a Peak detect record: its points are minimum and maximum in turn, so
  every edge passed the threshold three times. A LIN frame at 2.5 MSa/s came out with no sync measured and every byte
  wrong, while the same line decoded in Normal. Fixed for the record of a channel and for the screen record (unit:
  LIN at 19.2 kBd in Peak detect at 2.5 MSa/s, 40 records, only the bus's own three error frames in error, and the
  trigger on a frame, an identifier, a data byte and an error). In Peak detect a bus needs about ten points a bit:
  at five (100 kSa/s) a third of the fields is still wrong, where Normal reads them all
  ([Features](wiki/Features.md)).
