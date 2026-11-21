Added:

- A search of Studio looks for more than edges: the row **Type** of its sheet makes it an Edge, Pulse, Slope, Timeout
  or Runt search, with the polarity, the two levels and the time condition the trigger has for that type. Set to 50 %
  places the levels of every type, and the copies to and from the trigger carry the type and its settings. Over SCPI
  `:SEARch:MODE` names the type and `:SEARch:PULSe:...`, `:SEARch:SLOPe:...`, `:SEARch:TIMeout:...` and
  `:SEARch:RUNT:...` set it ([guide](docs/guide.md), [SCPI](docs/scpi/search.md)).
