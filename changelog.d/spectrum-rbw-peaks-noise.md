Added:

- Two remote queries for the spectrum: `:SPECtrum:RBW:ACTual?` answers the resolution bandwidth the spectrum really
  uses (a short time base makes it coarser than the one set), and `:SPECtrum:PEAK:LIST?` answers the peak table as
  frequency and level of each line ([SCPI and VXI-11](wiki/SCPI-and-VXI-11.md)).
