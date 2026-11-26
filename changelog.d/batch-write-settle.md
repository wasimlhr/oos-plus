Changed:

- Acquisition: a batch's register writes wait once after each group instead of after every write (75 waits of at
  least 10 us a batch became 2); 8 to 13 % more waveforms a second reach the screen at the short time bases
  (DHO924S, 1 channel at 5 ns/div 6,060 -> 6,830).
