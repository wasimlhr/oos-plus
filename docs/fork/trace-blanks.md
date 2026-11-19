# Pictures without the channel traces (2026-11-18)

Report: on a DHO924S in Run, status TRIG'D throughout, Auto sweep, 5 ms/div, 100 k points, CH2 to CH4 on an SPI
burst, about 8 records and 18 pictures a second, System > Graphics = GPU: in 3 to 7 of 14 pictures read from the web
remote the channel traces were missing altogether; the graticule, the labels, the math trace and the decoder band
were there. With Graphics = CPU none of 14 was. The owner had asked why the waveform "blips".

## Cause

Not the GPU path. Three places marked the phosphor maps for clearing (`dirty_map`) whenever the answer of a
measurement that is worked out in the background came in: `poll_reference_measurements`, `poll_catalog_measurements`
and `poll_pair_measurements`, all called at the start of `Ui::step`. The same step then cleared the maps, as it does
after a change of the time base. At minimum persistence a picture is drawn only when a record arrives, so a step
that took an answer and no record left the plot without traces until the next record. With persistence the collected
picture started over at every answer.

The unit had six rows of the measurement catalogue (jitter values, ids 42 to 47), which a background job answers
for each record. The empty frames came 60 to 70 ms after a record (verified on the unit, below); that this is when
the job's answer came in is assumed from the code and the host test, the answers themselves were not logged on the
unit. With the GPU a step without a record takes about 30 ms, so the answer often came in the second step after a
record, with no new record in it (records were about 125 ms apart). Drawing on the CPU made every frame longer
(15.5 pictures a second against 18.4), and the answer then mostly came in a step that also brought a record, which
draws the maps again at once: that is why the setting seemed to decide it.

## How it was found (verified on the unit, 2026-11-18)

- 30 pictures from `GET /screen.png` in a row, Graphics = GPU: 9 without the channel traces, not even faint ones.
- No GPU fault in the kernel log. The GPU's runtime power-down (after 50 ms idle, it was suspended about 30 % of
  the time) is not it: 7 of 30 with it switched off, 9 of 30 at 5 ms, 10 of 30 back at 50 ms.
- A copy of `oos-gpu` with switches, in the unit's RAM only: every layer drawn every frame, the maps uploaded whole,
  `glFinish` after the layers, `glFinish` after the index pass: 7 to 12 of 24 each, as without them. So neither the
  rows-that-changed upload nor the order of the GPU's jobs is it.
- The same copy writing one line per frame with the number of non-zero cells of each map as it arrives in the shared
  memory: every picture without traces was a frame whose three maps arrived empty, with the rows of the last trace
  as the changed band. The helper drew what it was given. Each such frame was the second in a row without a new
  record, 60 to 70 ms after the last one; the first one still had the full maps.
- Graphics = CPU on the same build: 0 of 30 at 15.5 pictures a second.

## Fix (branch fix/meas-answer-clears-picture)

The three places no longer mark the maps. An answer changes numbers, not where a trace lies; the values are drawn
anew with every picture anyway, and STOP draws its record again in every step. Nothing else read the mark there.

Host test `a_measurement_that_finishes_leaves_the_live_picture`: a hardware source, one record, a catalogue row whose
answer comes in a later step without a record; the map must be the same after every step, at minimum and at infinite
persistence. Without the fix it fails at the step that takes the answer.

A change of settings still clears the picture until the next record (`settings_changed`): a step of the trigger
level or of the horizontal position shows an empty plot for the time one record takes. That is meant (the old
picture no longer fits the settings) and is not changed here.

## Verification on the unit

Not yet verified on a unit: the fix ran on the host only.
