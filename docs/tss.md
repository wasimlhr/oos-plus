# Session files of another instrument (`.tss`)

TekScope, the PC program of Tektronix for its scopes, saves its state as a session, one file with the suffix `.tss`.
OOS loads such a file as a session, for its analog waveforms and a few settings, so that a capture of a faster scope
can be analysed on OOS. This page says what the file looks like from outside, how each fact is known, and what OOS
reads of it and what not. It is written from looking at files, not from a manual: no command
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
  13). What they hold: in the one session of a spectrum view that was looked at, `ch1_iq.wfm` has 254748 values a
  step of 40 ns and starts at the time the settings call `:SV:IQXZERO`, and `:SV:IQTIMESPAN` (10.19 ms) is 127374
  pairs of 80 ns: so a pair is two steps of the file, I and Q one after the other (assumed: I first; which of the two
  is I was not verified), the complex baseband around `:CH1:SV:CENTERFREQUENCY` (98 MHz there) at 12.5 million pairs a
  second for the 10 MHz span (`:SV:SPAN`). Seen in that one file only.
- A file can hold many acquisitions, one after the other (seen: 3 entries, of about 100 each). `parse_wfm` reads
  the first.

## The settings OOS uses

Of the settings text OOS uses the lines below and no other. `<n>` is the number of the channel or the reference.
What a line means is taken from its name and from comparing its value with the waveform files; none was tried on
the program.

| Line | Used as | How it is known |
|---|---|---|
| `:DISPLAY:GLOBAL:CH<n>:STATE`, else `:SELECT:CH<n>` | the channel is shown (`1`) or not (`0`) | seen: 1 for all 66 channel files, 0 for the 254 channels without a file |
| `:CH<n>:SCALE`, else `:DISPLAY:WAVEVIEW1:CH<n>:VERTICAL:SCALE` | volts a division | seen: there for all 64 analog channel files; the two lines agree in all 266 channels that have both |
| `:CH<n>:POSITION`, else `:DISPLAY:WAVEVIEW1:CH<n>:VERTICAL:POSITION` | where 0 V of the trace lies, in divisions above the middle | seen, see below; the two lines agree in all 262 channels that have both |
| `:CH<n>:OFFSET` | volts taken off the signal before it is drawn | seen, see below; not zero in 9 of 64 |
| `:CH<n>:LABEL:NAME`, `:REF:REF<n>:LABEL:NAME` | the name | seen: a name in 8 channels and 57 references, empty in the others |
| `:DISPLAY:WAVEVIEW1:REF:REF<n>:VERTICAL:SCALE` and `...:POSITION` | volts a division and position of a reference | seen: there for 176 of 181 references, missing for the digital ones |
| `:DISPLAY:GLOBAL:REF<n>:STATE` | the reference is shown or not | seen: there for all 181 |
| `:HORIZONTAL:SCALE` | time a division, of a screen ten divisions wide | seen: ten times the scale is the length of the record in 39 of the 41 files with a scale and an analog channel record |
| `:HORIZONTAL:POSITION` | where the trigger lies on the screen, in percent from the left | seen: with it the middle of the screen is the middle of the record in 37 of 40 |
| `:HORIZONTAL:DELAY:MODE`, `:HORIZONTAL:DELAY:TIME` | with the mode on, the middle of the screen is that time after the trigger | assumed: the mode is 0 in 99 files and missing in 10 |

The math channels that are filters (`:MATH:MATH<n>:TYPE FILTER`, `<n>` the number of the math channel) are read from
the lines below. One archive of filters was looked at (six filters, one of each kind it offers); the names of the
kinds and responses not in it are assumed.

| Line | Used as | How it is known |
|---|---|---|
| `:MATH:MATH<n>:FILTER:TYPE` | `LPASS`, `HPASS`, `BPASS`, `BSTOP`: low-pass, high-pass, band-pass, band-stop; `APASS`: a delay; `RC`: a raised cosine | seen: `LPASS`, `APASS`, `RC`, `BPASS`; assumed: `HPASS`, `BSTOP` |
| `:MATH:MATH<n>:FILTER:RESPONSE` | `BUTTERWORTH`, `BESSEL`, `GAUSSIAN` the response of the designer; `CUSTOM` has its coefficients in a file | seen |
| `:MATH:MATH<n>:FILTER:ORDER` | the order of Butterworth and Bessel (1 to 16); that of a custom filter is its number of taps | seen: 5 for both in the archive |
| `:MATH:MATH<n>:FILTER:CFREQ` | the cut-off of a low-pass or high-pass | seen; whether it is the -3 dB point of a Gaussian is assumed |
| `:MATH:MATH<n>:FILTER:LCFREQ`, `...:HCFREQ` | the band edges of a band filter | seen |
| `:MATH:MATH<n>:FILTER:DELAY` | the delay of `APASS` | seen: 1.6 ms |
| `:MATH:MATH<n>:FILTER:SDURATION`, `...:ROFACTOR` | the symbol time and roll-off of `RC` | seen: 5 us and 0.3 |
| `:MATH:MATH<n>:FILTER:SOURCE` | what is filtered: `CH<n>`, `REF<n>`, `MATH<n>` | seen: `REF1` in all six |
| `:DISPLAY:WAVEVIEW1:MATH:MATH<n>:VERTICAL:SCALE`, `...:POSITION`, `...:STATE` | volts a division, position in divisions, shown or not | seen; the position is taken as the offset of the lane in divisions (assumed) |
| `:DISPLAY:SELECT:MATH` | the math channel the Math menu shows | seen |

The coefficient file of a custom filter (`filterCoeff<k>_math<n>.flt` beside the settings: comment lines of parameters
behind `//` and a line `<sample rate>;<tap>,<tap>,...`, seen in two of the six filters of the archive, a lowpass with
3001 taps and a custom band-pass with 5087) is read as the taps of the filter: they become a reference, which is
Source B of the math channel, whose response is Custom. The taps are read at their own step and the filter keeps its
gain at the step of the record. A custom filter without its file becomes a Butterworth over the same band, and the
message of the load says so. The other keys of a filter (the saved response, the attenuation, the standard deviation) are not
used.

A line can be missing (two channel files have no vertical lines: the digital ones; one file has no horizontal
scale). OOS then takes the value from the waveform itself and invents none.

The vertical lines were tried against the samples: a sample of `v` volts is taken to stand
`(v - offset) / scale + position` divisions above the middle of the screen. Seen: of the 100 waveforms with a
position or an offset other than 0 (13 channel records, 87 references; records up to 3 million points) every one
then lies within ten divisions of height and 80 within eight. With the position counted downwards 46 of the 87
references leave ten divisions, with the offset added 3 of the 8 channel records that have one do. So the other
screen is taken to have ten divisions of height (assumed) where OOS has eight: a trace that used the outer two
reaches past the grid of OOS until its volts a division are changed.

## What OOS reads

**As a session.** A `.tss` file is listed under **Storage > Load session** beside the sessions of OOS
(`session-NNN.oss`), in the dialog of the Default layout and on the sheet of Studio and Minimal; on a stick it is
found wherever it lies, three folders deep. `:LOAD:SESSion "name.tss"`, `--open` and a file dropped on the window
of the desktop build load it the same way: the file is known by its first bytes, not by its name. A session is
the whole bench, so the setup and every reference slot are replaced.

- The records of channels 1 to 4 become **the record on CH1 to CH4, at full length** and with their time step,
  and the scope stops on it, as after loading a session of OOS. The channels of a record of OOS share one time
  axis: a channel file whose start, step or length differs from the first one's is left out and counted (not seen
  in the 16 files with two or more analog channel files).
- A channel is on when the settings show it, with their volts a division, and with the trace where the settings
  put it: OOS adds `position * scale - offset` volts before it draws. A scale that is no step of OOS's ladder is
  the step under it and a fine factor. The probe factor is 1: the samples are volts or amperes at the probe tip
  already. The name becomes the label of the channel (ten characters).
- The time base is the window of the settings when the record covers it, else the whole record on the screen.
- A reference goes into the slot of its number (R1 to R10) with its scale, position, name and whether it is
  shown. A session without a channel record opens in the offline view with its first reference filling the
  screen, on the time base of that reference: the horizontal settings are those of the acquisition, and fit the
  references in only 20 of the 65 such files.
- The filters of the math channels become the same math channels of OOS (Response, Order, cut-off or band, source,
  scale, whether shown), as the lines above say; the one the archive selects is the one the Math menu shows.
- Everything else is the default setup of OOS. **Not read: the trigger, the buses and their decoders, the
  measurements, maths other than filters, cursors, zoom, the unit and the invert switch of a channel, the logic
  channels, spectrum views and everything of the analysis options.** The message of the load says so in one line:
  "Loaded name.tss: waveforms, scales, time base and filters, without its trigger, buses,
  measurements, other maths".
- Save session writes OOS's own file. There is no way back into a `.tss`.

**As waveforms.** Where a waveform file is taken and no session is meant, a `.tss` gives its analog waveforms to
reference slots: **Reference > Load file**, `:REFerence:LOAD` and the upload of the web page. They go into the
slots from the chosen one on: the records of the channels first, in the order of the channels, then the
references in the order of their numbers, each named as the settings name it, else after its file. Nothing of
the setup changes.

**The I/Q record of a spectrum view** (`ch<n>_iq.wfm`) is read where the settings give `:CH<n>:SV:CENTERFREQUENCY`: it
is interpolated to a rate of 2.2 times the highest frequency of the record (the centre plus half the I/Q rate) and
mixed up to the centre, so it becomes the real signal around that frequency (a spectrum of it from 93 to 103 MHz shows
the signal of a view centred on 98 MHz). It goes after the records of the channels, as a reference in a session. A
record whose centre would need more samples than the import allows, or that cannot be read, is left out.

**Left out either way**, and said in the message ("not read: 2 digital and 1 IQ waveforms"): logic waveforms, the
`_iq` files without a centre frequency in the settings, a waveform file that is neither the record of a channel nor a reference the list names ("other"), a
reference whose file is not in the archive ("missing"); as a session also a channel record on another time axis
("differently timed") and a channel above 4 or a reference above 10 ("surplus"). A reference that is a text of
values is read as the waveform files of that kind are. Of a file with many acquisitions the first is read.

Limits, each refused with its reason: the file and every entry at most 128 MiB, a waveform at most 32 million
samples, all waveforms of one file together at most 128 million samples, at most 256 entries in an archive, and
the refusals of the zip reader below. A file that cannot be read leaves the bench as it was.

Over the 109 files (host, 2026-11-22; not run on a scope): 108 load as a session, 42 with a record of channels and
66 with references only, 14 of them with a waveform left out; one holds an `_iq` file and nothing else and is
refused with that reason. The longest record has 12.5 million points a channel; the largest file (45 MB, three
such channels) loads in about a second on a desktop PC. On a scope that time is not measured, and neither is the
memory: three channels of 12.5 million points are 150 MB as samples, beside the 45 MB of the file while it is
read.

## The zip reader

OOS has its own small reader (`ui/store/src/zip.rs`,
[ADR 0075](adr/code/0075-a-foreign-session-archive-is-read-with-an-own-zip-reader.md)): it finds the end record,
reads the list of entries, and gives the bytes of one entry, stored or inflated, after checking their CRC-32. It
unpacks nothing to a file and writes nothing. It refuses: zip64, an archive in several parts, an encrypted entry,
a method other than stored and deflate, more than 256 entries, an entry larger than the limit, a name that is
absolute or has a `..` step, and every length or offset the bytes of the file do not hold. The archive in the
archive is opened once; what should be the settings text and is an archive again is refused.
