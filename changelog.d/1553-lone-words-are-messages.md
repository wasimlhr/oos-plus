Fixed:

- A MIL-STD-1553 word that follows another after more than 14 us of quiet bus is a message of its own in the Events
  pane and on the band. A bus that sends a word every few milliseconds was shown as one long message with a
  "Word count mismatch" and a data word seconds after its command; each word now stands as its own message
  ("Incomplete packet" or "Unframed"). Host: tests with words 2 ms apart; not yet seen on a unit.
