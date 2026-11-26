Fixed:

- The Eye of a bus that talks in bursts (MIL-STD-1553 words, packets) no longer folds the idle line between the bursts
  into the picture: it was the brightest line of the eye and the One and Zero levels it measured. The clock takes the
  phase of each burst at once, and one period from all of them, so the words of a 1553 capture line up in one eye.
