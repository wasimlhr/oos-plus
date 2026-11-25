Added:

- `:REFerence:SOURce [<slot>,]MATHn` makes `:REFerence:SAVE` copy the result of a math channel into a reference slot. The
  Eye / Jitter analyser and the decoders can then read a differential trace (`CH1 - CH2`) from that slot. The copy is a
  stored record: save again after a new acquisition.
