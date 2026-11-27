Added:

- `:DECode:AUTO` (and `auto: true` of the MCP tool `setup_decoder`) sets the selected decoder up for its signal in
  one call: the threshold between the levels of the signal, Polarity Auto where the decoder has it, the time base and
  memory depth that give 10 samples a bit and hold a whole frame (kept when they do; where no depth of the unit fits,
  it says so and cuts nothing), and one acquisition. Every change is named with its old value in the report
  (`:DECode:AUTO?`), followed by the diagnosis of the decoder. Host: a UART record and the simulated unit; not yet
  seen on a unit ([Features](wiki/Features.md)).
