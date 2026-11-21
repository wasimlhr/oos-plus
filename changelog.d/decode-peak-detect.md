Fixed:

- The decoders and the protocol trigger read a Peak detect record: its points are minimum and maximum in turn, so
  every edge passed the threshold three times. A LIN frame at 2.5 MSa/s came out with no sync measured and every byte
  wrong, while the same line decoded in Normal (the fault seen on a unit; the fix host-tested, not yet run on a unit)
  ([Features](wiki/Features.md)).
