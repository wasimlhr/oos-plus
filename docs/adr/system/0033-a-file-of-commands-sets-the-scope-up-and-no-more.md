# ADR 0033: A file of SCPI commands sets the scope up and runs it, and no more

- Status: accepted
- Date: 2026-11-18
- Category: system

## Context

Storage > Run SCPI file runs the lines of a file through the one interpreter (ADR 0022). The first version gave a
file the authority of a client on the network, except for a line with the word `CONFIRM`. A review of that change
found what this leaves open:

- `:UI:TAP` and `:UI:MENU:SELect` are presses on the screen. With them a file opens the power dialog and takes its
  button, rolls an update back, or imports SSH keys from the stick it came from: everything a finger can do, also
  what a command only does with `CONFIRM`.
- `:CALibrate:STARt` starts a self-calibration without the dialog that asks to disconnect the inputs, and its result
  replaces the calibration the scope measures with.
- `:SYSTem:REMote:SERVice:TELNet ON`, `:SYSTem:WLAN:FORGet` and the clock need no `CONFIRM`.
- `:SAVE:SETup "<path>"` writes where the path says (only the vendor's data is refused, ADR 0009).

A client on the network is someone the owner let onto the bench LAN (ADR 0025, `net.allow`, the switches of the
services). A file is something else: it is run by one tap on its name, by a person who usually has not read it, and it
may come from a stick someone else filled. The checks of ADR 0025 say nothing about it.

This is known by reading the code and by host tests; none of it was tried on a unit.

## Decision

A file run on the scope sets the scope up, runs it, measures and saves into the save folder. What goes beyond that
is refused by the interpreter before any handler, said on the screen with the command, and not done; the rest of the
file runs. Refused from a file are:

- a command with the argument `CONFIRM`;
- every command of `:UI:` that is not a query;
- every command of `:SYSTem:` that is not a query, except those a set-up is written with: `:SYSTem:SET`,
  `:SYSTem:SETup` and the headers of the table of `:SYSTem:SETup:SCPI?`;
- `:CALibrate:STARt`;
- `:MMEMory:` commands that are not queries;
- a save, load, export or import with a file name that is an absolute path, has a drive, or steps up (`..`): a file
  names files of the save folder.

A query is always run: its answer goes nowhere. The file itself is read only when it is a regular file, not a link,
and no more than its size bound is read.

A client keeps the authority ADR 0025 gives it. What a file changes of a client's state is put back: the error queue,
the event status, the enable registers, and the operations the client waits for with `*OPC`.

## Alternatives

- A list of the commands a file may send. It is the stricter rule, and it was the first thought. The interpreter has
  several hundred headers that set the scope up, in tables and in older handlers; a list of them would have to be kept
  for every new command, and a missing entry breaks a recording made on the scope itself. The rule above is by
  subsystem instead, and the two subsystems that mix settings with administration (`:SYSTem:`, `:CALibrate:`) are
  closed except for what is named.
- A question on the screen for each such command ("this file wants to switch the scope off"). A person who taps Yes
  on a file they have not read taps Yes again; and a file that waits for a finger is no longer a file that runs by
  itself.
- A signature on the file, as for updates (ADR 0012). A recording made on the scope and edited by hand should run
  without a key.

## Consequences

- A recording of the SCPI recorder and a set-up saved as commands run as before: the recorder writes none of the
  refused commands.
- A script written by hand that used `:UI:TAP` to reach something without a command of its own no longer does so
  from a file. The way out is a command for the thing (ADR 0022), not the press.
- A new subsystem that can change more than the set-up (a second `:SYSTem:`) is added to `file_refuses` in the same
  pull request. A new command of `:SYSTem:` is refused from a file unless it is a row of the set-up table: that is
  the safe side.
- A file can still do what a set-up can: every key of the set-up schema, `*RST`, an auto set-up, the outputs of the
  generator. A set-up loaded from a stick could do the same before.
- Worth a second look when a command that names a file is added whose keyword is not SAVE, LOAD, EXPort or IMPort.

## References

- `ui/src/scpi/replay.rs` (`file_refuses`, `replay_start`), `ui/src/scpi.rs` (`exec_cmd`, `from_file`).
- [docs/scpi/storage.md](../../scpi/storage.md), Run SCPI file.
- ADR 0009, ADR 0022, ADR 0025.
