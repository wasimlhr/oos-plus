Fixed:

- A remote command that exists but gets a value it does not take (`:TRIGger:HOLDoff 11`, `:CHANnel1:PROBe 0`,
  `:ACQuire:MDEPth 7`, `:TIMebase:SCALe x`, ...) leaves `-224,"Illegal parameter value"` with the value in
  `:SYSTem:ERRor?`, no longer `-113,"Undefined header"`, which now only means the command is not known
  ([SCPI and VXI-11](wiki/SCPI-and-VXI-11.md)).
