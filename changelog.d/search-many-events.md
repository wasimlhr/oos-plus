Fixed:

- A search with events on every pixel of the plot (64 000 edges of a 40 MHz square at 20 us/div) drew a dashed line in
  every column, which made bands across the whole plot; such events are now a strip at the top edge, and each has its
  mark again when zoomed in. Studio's Search sheet has **Event list**, which opens the table of the events; the table
  names the search's own source and writes the times with enough digits to tell two rows apart
  ([guide](docs/guide.md)).
