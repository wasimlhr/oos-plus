Fixed:

- The FlexRay decoder shows all digits of a CRC: the frame CRC as six hex digits and the header CRC as three. A frame CRC
  of 0x02F5A6 read "CRC 2F5A6" and looked like a value of 20 bits; the CRC itself was read right ([Features](wiki/Features.md)).
- Studio shows the name a channel was given (`:CHANnel<n>:LABel:CONTent`, up to 10 characters) on its handle at the right
  edge of the plot and, where the pill is wide, on its pill in the bottom bar; they read CH1 to CH4 whatever the name was.
