Fixed:

- An SPI decoder on a record with too few samples for its clock no longer lists words that are none. At 200 kbit/s in a
  record of 125 kSa/s a frame of 16 words read "05 05 05 05" and the frames of one word around it gave nothing. Where
  more chip-select frames have clock edges and no whole word than frames with a word, the band now says "Frames shorter
  than a word: too few samples for the clock, or Data bits", and so it does for Data bits above the frame's length.
