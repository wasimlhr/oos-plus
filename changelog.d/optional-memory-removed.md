Removed:

- System > Opt. memory and Probe optional memory: hardware segments above 256 MiB read back the samples of the
  segment 256 MiB below them, so the upper half is not usable. Hardware segments use the lower 256 MiB again, as
  before 0.1.15
  ([horizontal-config.md 1.8](docs/protocols/horizontal-config.md#18-frame-memory-above-256-mib-not-usable)).
  A `mem.upper` line left in `/oos/system.conf` is kept and does nothing (unit)
