Fixed:

- `:CHANnel<n>:OFFSet` and `:SCALe` keep the offset within what the scale can reach at once: a query in the same line
  answers the offset in force, and a line of commands gives the same offset whether or not a frame ran between them.
  Found by W4R's OOS Plus fork ([SCPI and VXI-11](wiki/SCPI-and-VXI-11.md)).
