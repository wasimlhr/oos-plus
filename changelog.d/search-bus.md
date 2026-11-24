Added:

- A search of Studio can look at a bus: the type **Bus** marks the packets of the active decoder that the bus
  trigger's condition accepts, a frame, an ID or address, data or an error, with the row **Match** and, for an ID or
  data, the row **Value**. The events are listed, counted and walked through like those of the other types, and the
  copies to and from the trigger carry the condition. Over SCPI `:SEARch:MODE BUS`, `:SEARch:BUS:WHEN` and
  `:SEARch:BUS:VALue`. Host: UART records made by the signal generator; not yet seen on a unit
  ([SCPI](docs/scpi/search.md)).
