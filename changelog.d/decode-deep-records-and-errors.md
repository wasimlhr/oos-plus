Fixed:

- A decoder of a deep record says why it decoded nothing: "Deep record is read compressed" while a record over 1 M
  points runs on the screen, "Record too long for the decoder: N samples, limit 32000000" above 32 M samples; after a
  STOP it reads every sample of the record the stop read, not every n-th
  ([Measurements](wiki/Measurements-and-Analysers.md)).
  Host: a 6 M record of 150 bursts decoded in full; not yet seen on a unit.
- `:ACQuire:MDEPth` in roll mode (Auto Roll on, 200 ms/div and slower) is answered with -221 naming the reason and the
  way out; `:DECode:EVENts?` and the FIELDS table mark a word an error, with the reason, where the Events pane does;
  the MCP `configure` names the keys it knows and takes `acquisition` for `acquire`; `scope_state` shows `auto_roll`.
- The MIL-STD-1553 Messages view has the columns of the Tektronix table: Type, RT, T/R, SA/mode, Mode code, Status, WC,
  Data, C/S payload and Error ([Measurements](wiki/Measurements-and-Analysers.md)).
