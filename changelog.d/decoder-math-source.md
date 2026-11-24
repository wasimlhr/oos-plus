Added:

- A Math trace is a decoder source: choose M1..M8 (a channel that is on and not an FFT) in the Source rows of the
  Decode menu, or `:DECode:SOURce MATH1` over SCPI, and a differential pair such as CH1 - CH2 is decoded as the
  difference (FlexRay BP/BM, RS-485, CAN H/L). The setup stores the source as its number
  ([SCPI](wiki/SCPI-and-VXI-11.md)).
