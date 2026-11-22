# Session files of another instrument (`.tss`)

TekScope, the PC program of Tektronix for its scopes, saves its state as a session, one file with the suffix `.tss`.
OOS reads the analog waveforms of such a file. This page says what the file looks like from outside, how each fact is
known, and what OOS reads of it and what not. It is written from looking at files, not from a manual: no command
table and no text of the vendor is here, and no session file is in the tree (the tests build their own).

**How the facts are known.** "Seen" means counted over 109 session files of one PC scope program, all demo captures
that came with it, in 2026-11; the number says in how many of the 109, or of how many entries. "Assumed" means it fits
what was seen and was not tested against the program. Nothing here was verified by writing a file and loading it in
that program, and no file of a scope itself (saved on the instrument, not by the PC program) was looked at.

## The container

- The file is a zip archive (seen: 109 of 109). Its entries are stored or deflated, nothing else (seen: 17 stored,
  308 deflated). No archive is zip64, none is encrypted, none is in several parts (seen: 109).
- The sizes and the check sum of an entry are often written after its bytes, and the header in front of the bytes
  has zeros in their place (seen: 86 of 325 entries). The list of entries at the end of the archive has them always,
  so a reader takes them from there.
- The entries: one picture `.png` of the screen (seen: 109), one settings archive `.set` (seen: 109), the record of
  each channel that was on as `ch<n>.wfm` with `n` from 1 on (seen: 42 files have one to four), `ch<n>_iq.wfm` beside
  it when a spectrum view was open (seen: 13 entries), and in some a `manifest.json` (seen: 26). The `.png` and the
  `.set` are not named after the file: they are found by their suffix.
- The entries can lie under one folder, with an entry for the folder itself (seen: 5 files). Their names then have
  the folder and a `/` in front.
- One file holds another `.tss` as an entry (seen: 1). OOS does not open it.

## The settings archive

The `.set` is a zip archive again (seen: 109), of the same kind: stored and deflated entries, sizes after the bytes
(seen: on 110 of 389 entries). It holds:

- **One text of settings** with the suffix `.set` (seen: 109), in UTF-8 with line feeds and no byte order mark
  (seen: 109). It is a list of remote commands, one a line, each a header that begins with `:` and whose parts are
  parted by `:`, a space and an argument. The first line has several commands parted by `;` and begins with a reset
  command (seen: 109). An argument in double quotes can run over several lines (seen: 69 files) and can hold `;`.
  A space in a name is written `%20` (seen: in the names of 12 files). Whether the text lists every setting or only
  those that differ from the state after a reset is not known; a reader has to live with a line that is missing
  (assumed: only the differing ones, because the number of lines varies much between files).
- **A list of the reference files**, a text with the suffix `.ini` of `key=value` lines (seen: 74 files have one or
  two such texts). The line `recallSubsystem\refMgr\ref<n>\sourceFileName=<name>` says that reference `n` was loaded
  from the entry `<name>` of this archive (seen: 181 lines in 69 files). The settings text does not name the file.
- **The reference waveforms** the list names, as `.wfm` files (seen: 179 of the 181; no `.wfm` entry is left
  unnamed). Two references are a text of comma-separated values under another suffix.
- Other entries of filters, masks and network data (suffixes `.flt`, `.s4p`, `.dat`, `.json`, `.csv`; seen: 11
  files). OOS reads none of them.

## The waveform files

Every `.wfm` entry is a little-endian file with the mark `:WFM#003` (seen: 258 of 258), the format of single
waveform files that OOS already opens (`parse_wfm`, `ui/store/src/import.rs`): a header with the scale and the
offset of the samples, the time between samples and the time of the first one, then the samples.

- The samples are 16-bit integers (seen: 193), 32-bit floats (seen: 46) or 8-bit integers (seen: 19), in volts or
  amperes after the scale of the header (seen: unit `V` in 206, `A` in 45).
- A record has 860 to 12.5 million points (seen). The channel files of one session have the same time between
  samples, the same start and the same length (seen: 16 of the 16 files with two or more analog channel files).
- A waveform of logic lines has 6 in the 32-bit field at byte 122, where an analog one has 2, and no unit (seen: 7
  entries with 6, each one a reference the settings call digital, or a channel file without a vertical scale in the
  settings; 251 with 2). How its samples hold the lines is not known.
- The `_iq` files have 1 in the field that says how the samples are stored, where the others have 0 (seen: 13 of
  13). What they hold is not known beyond their name.
- A file can hold many acquisitions, one after the other (seen: 3 entries, of about 100 each). `parse_wfm` reads
  the first.

## What OOS reads

A `.tss` file is taken wherever a waveform file is: **Reference > Load file**, `:REFerence:LOAD`, the upload of
the web page, `--open` and a file dropped on the window of the desktop build. Its analog waveforms go into reference
slots, from the chosen slot on (or the first empty ones): the records of the channels first, in the order of the
channels, then the references in the order of their numbers. A waveform is named as the settings name it (the
argument of `:CH<n>:LABEL:NAME` or `:REF:REF<n>:LABEL:NAME`), else after its file. A reference that is a text of
values is read as the waveform files of that kind are.

Not read, and said in the message of the load ("not read: 2 digital and 1 IQ waveforms"): logic waveforms, the `_iq`
files, a waveform file that is neither the record of a channel nor a reference the list names, and a reference whose
file is not in the archive. Everything else of the settings text is not used by this load. Of a file with many
acquisitions the first is read.

Limits, each refused with its reason: the file and every entry at most 128 MiB, a waveform at most 32 million
samples, all waveforms of one file together at most 128 million samples, at most 256 entries in an archive, and
the refusals of the zip reader below. Over the 109 files: 108 load, 14 of them with a part left out; one holds an
`_iq` file and nothing else and is refused with that reason. The longest record has 12.5 million points; the largest
file (45 MB) loads in 0.7 s on a desktop PC (host; not measured on a scope).

## The zip reader

OOS has its own small reader (`ui/store/src/zip.rs`,
[ADR 0075](adr/code/0075-a-foreign-session-archive-is-read-with-an-own-zip-reader.md)): it finds the end record, reads the list of entries, and gives the bytes of one entry, stored or inflated, after
checking their CRC-32. It unpacks nothing to a file and writes nothing. It refuses: zip64, an archive in several
parts, an encrypted entry, a method other than stored and deflate, more than 256 entries, an entry larger than the
limit, a name that is absolute or has a `..` step, and every length or offset the bytes of the file do not hold.
The archive in the archive is opened once; what should be the settings text and is an archive again is refused.
