# ADR 0029: A Storage row asks for the name of its file, and which file to load

- Status: accepted, to be reviewed
- Date: 2026-11-17
- Category: ui-ux

## Context

The Storage menu named every file itself. Save setup wrote `setup<slot>.txt`, Save session the next
`session-NNN.oss`, the exports the next numbered or dated name; Load session and Load fast-frame took the newest file
and no other. There was no way to say what a file is called, and none to pick the one to load, although SCPI could do
both since it takes a path (`:SAVE:SETup "path"`, `:LOAD:SESSion "path"`). A test in `ui/src/ui/lint.rs` states that
the scope has no file dialog, and the scope has no keyboard.

What was there already: the on-screen keyboard (channel names, the name on the network, the WLAN password), the
list of files of Reference > Import slot, the scroll bar of ADR 0006, and in the desktop build the Open dialog of
Windows for a waveform (`ui/src/desktop/picker.rs`). Host-tested only; not yet verified on hardware.

## Decision

A Storage row that writes a file asks for its name first, and a row that reads one asks which file; the question is
asked with what the device has. On the scope the name comes from the on-screen keyboard, with the next free name
already in it, and the file is picked from a list of the folder that scrolls by bar and finger. The desktop build
asks with the Save As and Open dialogs of its system, which also choose the folder. The setup slots stay as they
are, without a question, and nothing remote ever asks.

In detail:

- The place on the scope is the row Save to (Auto, USB, Local), as before. The keyboard takes a name, not a path: a
  name with `/ \ : * ? " < > |`, or with a dot at its start or end, is refused and stays on the keyboard. The suffix
  is added. A name that is taken is said to be taken; OK once more replaces the file. The name in the keyboard is a
  proposal and stands marked: the first character typed takes its place, an arrow or a tap into it keeps it.
- Setup slot, Save setup and Recall setup are the quick way and the one `*SAV`, `*RCL` and `:SAVE:SETup` without a
  path use. A setup under a name of its own has two rows, Save setup to file and Load setup from file.
- Save to holds for a setup by name: it goes to the stick when files go there. Locally it lies beside the slots, in
  the folder that survives a power cycle, since the local save folder of the scope is in memory. The slots do not
  follow Save to: `*RCL 3` has to find its file without a stick and after a power cycle.
- A list shows the files of one kind, the newest first, told by how they begin where the suffix does not say it (a
  mask is a `.txt` like a setup, a waveform a `.bin` like a fast-frame file).
- A path given by SCPI is used as given. A command without a path (`:SAVE:IMAGe`, `:SAVE:FFRame`, `:LOAD:FFRame`,
  `:SAVE:SESSion`, `:LOAD:SESSion`, `:SAVE:WAVeform`) and the Quick key take the next free name or the newest file,
  as before: a question on the screen would leave a remote client waiting for a finger.

## Alternatives

- A file dialog of our own on the scope, with folders to walk through. The scope has two places, the stick and the
  local folder, and a row that chooses between them; a tree of folders would be a new kind of screen for that.
- A row File name in the menu, whose text the next save takes. One more row, and it leaves open what the save after
  that one is called.
- A switch between asking and numbering. It keeps the save with one tap in the menu, at the price of a row that
  changes what six others do. The Quick key is the save with one press. Worth adding when the question is in the way
  on a unit.
- Giving the slots up for names. `*SAV` and `*RCL` are numbers, in every program written for a scope.
- Asking on the desktop with the keyboard and the list too, to have one way. A computer has a file system its user
  knows, and a dialog for it; the waveform load uses it already.

## Consequences

- Every save from the Storage menu takes one tap more on the scope: OK on the keyboard.
- The test scripts press a keyboard and a list by name: `type <text>`, `ok` and `option <text|n>` (`ui/src/cmd.rs`).
- A new kind of file gets a line in `FileKind` (`ui/src/ui/files.rs`) and, for the desktop, its filter in
  `picker.rs`; the keyboard, the list and the dialog come with it.
- The list of the Default layout is a dialog that scrolls (`scroll::List::Files`). In Minimal and Studio the sheets
  show the same list their own way, in pages.
- The dialogs exist for Windows only, as the desktop build does. Another desktop gets them in its own `picker`.
- Reference > Import slot and Mask > Load mask still take their files as before; they can take the same list.

## References

`ui/src/ui/files.rs`, `ui/src/desktop/picker.rs`, `ui/src/ui/wlan.rs` (`OskFor::File`), `ui/src/ui/scroll.rs`,
`ui/tests/storage.txt`, `docs/scpi/storage.md`. ADR 0006 (lists scroll), ADR 0021 (platform crates on the desktop),
ADR 0027 (a session is one file).
