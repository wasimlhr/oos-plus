Added:

- An empty math channel has an Add filter row: a Butterworth low-pass of the active channel, and a Filter window.
- The designed filters of the Math menu have a Filter window (Add > Windows, or Display > Windows > Lower) with the
  filter's gain, phase, impulse and step response, and
  remote commands for their settings (`:MATH<n>:FILTer:RESPonse`, `ORDer`, `PRIPple`, `SDURation`,
  `ROFactor`, `DELay`). A Tektronix session file loads the filters of its math channels (Butterworth, Bessel, Gaussian,
  delay, raised cosine, band filters; a custom filter's coefficient file is not read and a Butterworth over the same
  band stands in). Host only; not seen on a unit.
- A floating window can lie over the zoom bar. The waterfall of the Spectrum is marked "now" and "older".
- The Math sheet of a filter has a Preset row (Remove noise, Remove ringing, Remove drift) that
  follows the sample rate and the time base.
  Add > Filter makes a filter on a free math channel. The measurement chip is an arrow that slides out to "Meas . N".
  The external display is set on Display > Screen only. The web page sends a long press (`down` and `up` commands).
- The Spectrum and Filter windows have a settings button (a gear) left of the expand button. The Spectrum of a channel
  is taken from the samples the scope took when the screen record is interpolated, so its band ends at half the sample
  rate and not at 3.6 GHz. The math chips of the bottom bar show their scale a division like the channels.
- A filter can be your own: the Custom response takes the taps of a filter file (`.flt` of a Tektronix session, or any
  waveform file, loaded with Reference > Load file, a set of taps for each sample rate of the file) from the reference
  chosen as Source B, at the step of the file.
  Save to reference on the Math sheet keeps the result of a math channel as a reference, to measure and compare. The
  custom filters of a Tektronix session archive load with their taps.
- The Histogram, XY and Eye windows have a settings button too. The selected chip of the bottom bar has a thick border.
  Auto scale of a math channel waits for its result when it has none yet, so one press fits it.
- The live channels on the external display (HDMI Layout) are drawn as lines at its resolution, like the math and
  reference traces, instead of the panel's phosphor stretched to it (blocky edges). With HDMI set to Layout, Display >
  Screen > HDMI lines turns it off to get the persistence and colour grade back.
