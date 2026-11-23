Added:

- The Manchester decoder shows the frame check of an Ethernet frame (10BASE-T): the last four bytes of the frame as a
  check, marked where the CRC-32 over the frame is another. As bytes alone a good frame and one with a byte changed
  looked alike. A frame the record cuts off, or one shorter than a frame can be, has no check.
