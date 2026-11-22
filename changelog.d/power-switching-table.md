Changed:

- Power analysis, Switching page: the figures are a table, each with mean, minimum, maximum, standard deviation and
  count (turn-on, turn-off and conduction energy, their total, and the losses at the switching frequency), with the
  plot of the voltage against the current under it. The conduction energy and RDS(on) are no longer negative where
  the voltage stands under 0 V while the switch conducts (a zero offset of the probe). `:POWer:ITEM?` has `ETOT`,
  `PLOSS` and `FSW` ([Measurements and analysers](wiki/Measurements-and-Analysers.md); host only, not yet seen on a
  unit).
