Fixed:

- `:TIMebase:DELay:SCALe` and `:OFFSet` sent while the zoom is off stay in effect when `:TIMebase:DELay:ENABle 1`
  follows; before, the enable put the zoom back at three steps below the main time base and a requested 2.5 us/div
  could end as 5 us/div. Host: tested; the same query read on a unit ([SCPI](docs/scpi.md)).
