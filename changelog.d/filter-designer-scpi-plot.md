Added:

- An empty math channel has an Add filter row: a Butterworth low-pass of the active channel, and a Filter window.
- The designed filters of the Math menu have a Filter window (Add > Windows, or Display > Windows > Lower) with the
  filter's gain, phase, impulse and step response, and
  remote commands for their settings (`:MATH<n>:FILTer:RESPonse`, `ORDer`, `PRIPple`, `SDURation`,
  `ROFactor`, `DELay`). A Tektronix session file loads the filters of its math channels (Butterworth, Bessel, Gaussian,
  delay, raised cosine, band filters; a custom filter's coefficient file is not read and a Butterworth over the same
  band stands in). Host only; not seen on a unit.
- A floating window can lie over the zoom bar. The waterfall of the Spectrum is marked "now" and "older".
