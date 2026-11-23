Fixed:

- The decoders and the protocol trigger read a record taken in Peak detect. Its points are minimum and maximum in
  turn, so every edge passed the threshold three times: a LIN frame at 2.5 MSa/s came out with no sync measured and
  every byte wrong, while the same line decoded in Normal. They now read the mean of every point and its neighbour
  ([Features](wiki/Features.md)).
