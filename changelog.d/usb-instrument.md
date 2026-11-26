Added:

- The rear USB port can make the scope a USB instrument: **System > Network > Remote services > USB instrument** (off
  by default; tried on one scope with a Mac, not yet with Linux and Windows). VISA and pyvisa find it as
  `USB::0x1209::0x0001::<serial>::INSTR` (USBTMC with USB488), with the same SCPI as port 5555 and no network needed;
  Linux binds its `usbtmc` driver, Windows its WinUSB driver without an install
  ([Remote control](wiki/Remote-Control.md#usb-instrument-usbtmc)).
