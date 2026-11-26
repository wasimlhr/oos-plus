Fixed:

- A remote command over SCPI (port 5555), VXI-11 or HiSLIP is answered as soon as it arrives, not at the next picture:
  `*OPC?` in Run takes a few milliseconds instead of 12 to 30 ([SCPI and VXI-11](wiki/SCPI-and-VXI-11.md)) (unit).
