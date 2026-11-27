Added:

- A decoder that decoded nothing, or mostly errors, says why and what to change: a sample rate too low for the bit
  rate ("sample rate 15.6 kSa/s is too low for 1.00 Mb/s: use at least ..."), a threshold outside the signal, a flat
  line, a record shorter than a frame, a bit rate the edges do not show, and a bus that is inverted ("all 21 words
  fail their check, but the inverted line decodes with 0 errors: set Polarity to Auto or Inverted"). The band and the
  empty Events pane show the first sentence in place of "Nothing decoded". `:DECode:DIAGnosis?` (and `:BUS<n>:`, and
  `:FIGures?` for the measured bit rate, the levels and the share of failed fields) answer it, and the MCP tools
  `decode_results` and `setup_decoder` carry `diagnosis` and `signal`. Host: generated 1553 and UART signals and the
  reference decoder; not yet seen on a unit ([Features](wiki/Features.md)).
