Fixed:

- `:MEASure:STATistic:ITEM? AVERages,VPP,MATH1` (and MAXimum, MINimum, DEViation, CURRent, CNT) reads the row that
  `:MEASure:ITEM VPP,MATH1` added on the math copy; it was `-224` and 9.9E37. A statistic of a measurement with no row
  is `-221` with the command that adds it, not 9.9E37.
- `:ACQuire:SRATe?` and `:ACQuire:MDEPth?` answer the setting just made (explicit depth in Normal mode, or a time base
  just changed with Auto depth) at once, not the record of the acquisition before it for seconds.
  Host only: not yet seen on a unit.
