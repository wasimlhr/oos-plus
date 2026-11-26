Changed:

- The screen is drawn at every third refresh of the panel (32.5 pictures a second on a DHO924S, whose panel runs at 97.4
  Hz) instead of on a 30 Hz timer of its own, so its pictures no longer beat against the panel; `pacing.clock = timer`
  in `/oos/system.conf` goes back to the timer, and a monitor on HDMI keeps it ([ADR
  0010](docs/adr/hardware/0010-unverified-hardware-functions-are-opt-in.md)).
