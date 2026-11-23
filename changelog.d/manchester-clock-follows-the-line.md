Fixed:

- The Manchester decoder reads a long packet whose sender's clock is a little off the rate set. Its bit clock now
  follows the edge in the middle of every bit, not the first edge of the packet: a packet of 1500 bytes at 200 ppm from
  the rate went wrong from its byte 234 before, as a frame of 10BASE-T (880 bits) did on a unit with a wrong byte and
  a wrong frame check. A rate set 8 % off still reads the line.
