# ADR 0075: the session archive of another instrument is read with an own small zip reader

- Status: proposed
- Date: 2026-11-22
- Category: code

## Context

A capture of a faster scope is worth analysing on OOS: its decoders, measurements and analysers work on a loaded
waveform as on a live one. One PC scope program saves a capture as a session file `.tss`
([tss.md](../../tss.md)): a zip archive with the record of each channel as a waveform file, and a second zip
archive inside with the settings as text and the reference waveforms. OOS already reads that waveform format from
single files (`parse_wfm`), so what stood between such a session and the screen was the archive, and unpacking it by
hand on a PC first: up to four channel files and eight references of one session, one by one.

[ADR 0027](0027-a-session-is-one-file-of-setup-record-and-references.md) decided against an archive for OOS's own
session file: it needs a dependency or an own reader, for no gain over three length-prefixed parts. That holds for a
format OOS chooses. Here the format is chosen by someone else, and the question is only whether OOS reads it.

The crates depend on `libc` alone ([ADR 0013](../process/0013-no-vendor-code-only-libc.md)), and the parts of a
reader were there: an inflate with a limit on its output (`oos-store`, for RIGOL's waveform files) and a CRC-32 (the
PNG writer). Missing was the reading of the list of entries. Counted over 109 session files of that program in
2026-11: entries are stored or deflated only, no archive is zip64, encrypted or in several parts, an archive has at
most 7 entries and the one inside at most 10, and the largest file has 45 MB. Host-tested only; not yet run on a
unit.

## Decision

OOS reads a session archive with a zip reader of its own in `oos-store` (`ui/store/src/zip.rs`): the end record,
the list of entries, and the bytes of one entry, stored or deflated, with its CRC-32 checked. It reads only what
such sessions need and refuses the rest by name. It never writes an archive, and OOS's own session file stays the
one of ADR 0027.

## Alternatives

- A crate for zip files. It would be the first dependency beside `libc`, with its own dependencies for the
  compression, for a file format of which one small subset is ever met.
- A tool on the PC that turns a session into OOS's session file. Nothing new on the scope, but the user at a scope
  with a stick has no PC tool there, and the conversion would have to be kept in step with two formats.
- Unpacking by hand and loading each waveform file. It works today and loses the names, the scales and the order
  of the waveforms, which the settings text has.

## Consequences

- The limits are part of the decision. Refused, each with a message: zip64, an archive in several parts, an
  encrypted entry, a method other than stored and deflate, more than 256 entries, an entry over the import limit
  of 128 MiB, a name that is absolute or has a `..` step, and every length or offset that the bytes do not hold.
  Sizes come from the list of entries and are checked before memory is taken; an entry must inflate to exactly its
  size.
- Nothing is unpacked to a file and no name of an entry is ever used as a path: a name is compared, no more.
- The archive in the archive is opened once. A reader that follows archives as deep as they go is a different
  thing and is not wanted.
- The reader is not a general one and is not offered as one: no other part of OOS opens zip files with it without
  a look at these limits first. A use that needs zip64 or another method is a reason for a second look at a
  dependency.
- CRC-32 exists once for `oos-ui`, `oos-store` and the decoders: the table version of the PNG writer moved to
  `oos-dsp` (`decode::crc32`), the lowest crate all three reach, and the CRC-32 of USB PD is that function.
  `oos-hal` keeps its own few lines: it depends on `libc` alone and on no crate of the workspace
  ([ADR 0001](0001-three-crates-with-pure-hal-and-dsp.md)).
- The inflate reads its codes a bit at a time. The largest session seen (45 MB, three records of 12.5 million
  points) loads in under a second on a desktop PC; how long it takes on a scope is not measured. Worth a second
  look when that time is felt.
- The tests build their archives themselves (`testkit::zip`, with the deflate of the PNG writer and a writer of
  dynamic codes); no file of an instrument is in the tree.

## References

`ui/store/src/zip.rs`, `ui/store/src/tss.rs`, `ui/store/src/tss_tests.rs`, `ui/src/store/hostile_tests.rs`,
[docs/tss.md](../../tss.md).
