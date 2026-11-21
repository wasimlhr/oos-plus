Changed:

- In the Default layout the steppers and the number pad no longer stop at five divisions. This applies to the trigger
  level, channel offset, horizontal delay, math offset, decoder and bus thresholds, and reference offset. They take
  any value, as upstream OOS does; the Minimal and Studio layouts keep the limits.
- SCPI setters take the number they are sent, as upstream's do. For example, `:TRIGger:EDGE:LEVel 100` reads back 100
  (it read back 5), and `:CHANnel1:OFFSet 100` reads back 100 (it read back 56). The front end still keeps a channel's
  offset within what it can shift at every step, and the trigger hardware keeps the level within its range.
