Fixed:

- SCPI: `*RST` also sets the waveform transfer back (`:WAV:SOURce`, `:MODE`, `:FORMat`, `:STARt`, `:STOP`), so a
  page left by an earlier read no longer cuts the next one ([docs/scpi.md](docs/scpi.md)).
