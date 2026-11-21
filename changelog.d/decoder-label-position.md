Added:

- Minimal, Studio: a decoder has a name and a place of its own. Decode > Label writes a name on its band where the
  protocol stood, and Decode > Position puts the band so many divisions from the middle of its window; at the lower
  end it is at the bottom, on the bands under it, as before. Each of the four decoders keeps its own, a setup stores
  them (`dec_label`, `dec_pos`), and `:BUS<n>:LABel:CONTent` and `:BUS<n>:POSition` set them. The Default layout
  draws upstream's band and has neither row (host; not yet seen on a unit)
  ([SCPI](wiki/SCPI-and-VXI-11.md)).
