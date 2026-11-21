Fixed:

- `:STOP`, `:RUN` and `:SINGle` keep the measurement statistics, as the Run/Stop key does (a `:STOP` started them
  again).
- `:SPECtrum:RBW:ACTual?` and `:SPECtrum:PEAK:LIST?` answer the record now also with the Spectrum page closed (they
  answered the page's last result), and the list holds up to as many lines as the Harmonics row asks, not 8 at most.
