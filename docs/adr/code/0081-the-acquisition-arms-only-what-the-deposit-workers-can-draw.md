# ADR 0081: The acquisition arms only what the deposit workers can draw

- Status: proposed
- Date: 2026-11-25
- Category: code

## Context

In batch mode (ADR 0059) the acquisition thread arms a batch of up to 1000 frames, lets the FPGA's ring fill for up
to 25 ms, reads it and lends the buffer to the deposit workers (`ui/src/drawpipe.rs`, one per A53). With three
buffers, a batch read while the workers still hold the other two finds none to swap in and is dropped whole. On a
DHO924S at 1 channel and 5 ns/div (verified on the unit, 2026-11-25) the hardware delivered about 53,000 frames a
second and about 6,400 reached the picture: the four workers were each about 96 % busy, so most batches were read over
PCIe only to be dropped, and the reads and the drops took memory bandwidth and CPU from the drawing.

## Decision

The workers measure what a frame costs them (a moving average per batch, per mapping) and grant the acquisition
thread the frames they draw in about one picture (33 ms; from 1 frame to the planned batch). A batch planned with more
than `FRAME0_CHECK_MAX_FRAMES` frames arms at most the grant, and not fewer than that many plus one; before it arms,
the thread waits while the workers hold the buffers the batch would need, drawing frames for them meanwhile, for at
most 200 ms, after which the batch goes on as before.

## Alternatives

- Keep reading and dropping (before): the drops cost the time the workers lack.
- A fixed smaller batch: right for one time base and channel count only; the cost per frame differs by a factor of
  hundreds between them.

## Consequences

- `dropped` of `:SYSTem:STATe?` loses the batches dropped for want of a buffer at short time bases (expected near 0);
  frames of older settings or mappings still count there. `acquired` counts frames read, as before, so it falls
  towards `used`: frames the FPGA would have recorded while the thread waits are not recorded, as no batch is armed.
- More, shorter batches at short time bases: frames 0 and 1 of each batch are left out (issue #5), so a few per mille
  more of the frames read; batches planned with up to `FRAME0_CHECK_MAX_FRAMES` (slow time bases, SINGLE), the
  hardware average and every path without batches arm and read as before.
- Verified on the unit (DHO924S, 2026-11-25, from RAM; device-test of acquisition, deep record, roll, trigger and
  front end: 53 pass): at 1 channel 5 ns/div 8,121 frames read a second, 8,034 in the picture, 91 dropped (before
  52,552, 7,234 and 45,321); 4 channels 200 ns/div 1,772, 1,673 and 101 (before 12,663, 1,391 and 11,272); the step
  time fell by about a quarter. Worth a second look if a time base waits for the 200 ms.

## References

[ADR 0059](0059-batch-is-the-only-draw-path.md),
[ADR 0074](0074-the-board-names-no-signal-code-its-policy-is-its-own-and-the-draw-pipeline-is-handed-to-it.md),
[fork/perf-plan.md](../../fork/perf-plan.md) (item 1), `ui/src/drawpipe.rs` (`Cost`, `Pipe::grant`, `Pipe::lent`),
`ui/src/board/pipe.rs` (`Workers::grant`, `Workers::lent`), `ui/src/board/dho/hw/plan.rs` (`admitted`,
`wait_for_workers`), `ui/src/board/dho/hw.rs` (`batch_pass`).
