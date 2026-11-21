Fixed:

- The eye and its jitter figures time the edges at the middle of Top and Base as Measure finds them. On a fast, noisy
  record the eye took the middle of the extremes, which one undershoot moved off the crossing: on a 2.5 Gb/s record
  TIE rms read 25.8 ps where it is about 9 ps, with a duty-cycle distortion of 46 ps and a narrow, lopsided bathtub
  ([Measurements and Analysers](wiki/Measurements-and-Analysers.md)).
