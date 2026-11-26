Fixed:

- The Eye of a bus that talks in bursts (MIL-STD-1553 words, packets) no longer folds the idle line between the bursts
  into the picture: it was the brightest line of the eye and the One and Zero levels it measured. The clock takes the
  phase of each burst at once, and one period from all of them, so the words of a 1553 capture line up in one eye.
- The Eye's automatic rate of such a bus is read from where the signal is: a 2 Mb/s 1553 word in a long idle record was
  read as 9.7 Mb/s (and, from other records, as 22 or 1 Mb/s), because the three windows it looked at lay at the start,
  middle and end of the record.
