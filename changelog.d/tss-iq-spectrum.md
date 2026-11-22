Added:

- A session file of another instrument (`.tss`) with the I/Q record of a spectrum view (`ch<n>_iq.wfm`) loads it as the
  real signal around the centre frequency its settings give, so the spectrum of it shows the span of the view (a view
  centred on 98 MHz shows 93 to 103 MHz; it was left out as "not read: 1 IQ waveform" before). Which of the two
  values of a pair is I is assumed ([tss.md](docs/tss.md); host only, not yet seen on a unit).
