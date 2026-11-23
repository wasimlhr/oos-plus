Fixed:

- With only noise on the source, the spectrum's THD, SNR, SINAD, SFDR and ENOB were figures of a "fundamental" found
  in the noise; they now show no value, and `:SPECtrum:ITEM?` answers `9.9E37`
  ([SCPI and VXI-11](wiki/SCPI-and-VXI-11.md)).
