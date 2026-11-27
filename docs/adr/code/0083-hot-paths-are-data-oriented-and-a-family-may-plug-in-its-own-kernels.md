# ADR 0083: Hot paths are data-oriented, and a family may plug in its own kernels next to the portable reference

- Status: accepted
- Date: 2026-11-27
- Category: code

Amends ADR 0001 (what `oos-dsp` takes as input).

## Context

ADR 0001 keeps the hardware layer and the signal processing pure, and gives `oos-dsp` one input: `&[f32]` volts with a
fixed `dt`. That keeps the code portable and testable on any host, but it puts a conversion in front of every loop:
each sample of a record is turned from its ADC code into an `f32` before it is drawn, measured or decoded. The loops
that run over every sample of every frame (unpacking the words of the FPGA, peak detection per screen column,
deposits into the phosphor map, measurements over deep records) are the ones that set the frame rate on the RK3399,
and there the layout of the data counts for more than the arithmetic: a 16-bit code is half the memory traffic of an
`f32`, an 8-bit code a quarter.

The tree already goes this way in places: `ui/src/fastdraw.rs` finds the minimum and maximum of a column with NEON on
aarch64, and the draw pipeline hands codes with their layout (`board::pipe::CodeLayout`) to the deposit workers
(ADR 0074, ADR 0081). Each of these grew by itself, inside the screen's code, with no rule for where such a path lives,
how it is tested against the plain code, or how a family with another native layout (the DHO800/DHO900 stock image,
the MHO900, an acquisition image of our own with its own record format) brings its own.

## Decision

1. **Hot paths are written data-oriented.** A loop over every sample keeps the data in the layout it arrives in (codes,
   with their scale and offset beside them) as long as it can, works on contiguous slices in batches, without dynamic
   dispatch, allocation or a branch on the model per sample, and turns codes into volts at the end, where a value is
   shown or reported.
2. **Kernels are a small, named set.** Such a loop is a kernel: a function on plain slices of integers or floats with a
   described layout (for example: unpack words into codes; minimum and maximum per column; deposit into the phosphor
   map; sum and sum of squares over a range). Each kernel has one **portable reference** in `oos-dsp`: safe Rust, no
   `cfg` of the target, the code every host runs. Kernels name no register and no board; a layout is data, not
   hardware, so `oos-dsp` stays free of hardware as ADR 0001 wants.
3. **A family may plug in its own kernels.** A board family (ADR 0067), or a backend for a family's FPGA image, may
   register faster versions of any kernel for its native layout, for example with NEON intrinsics or with a layout
   that only its image produces. The registration sits in the family's module (`ui/src/board/<family>/`,
   `hal/src/<family>/`); the pipeline calls the kernels through one table that the family fills when it starts, and
   falls back to the reference for every kernel the family leaves out. Above the table nothing names a family.
4. **Every plugged-in kernel is proven against its reference.** A test runs both on the same inputs (the family's
   recorded frames and generated edge cases: empty, one sample, the limits of the code range, lengths that are not a
   multiple of the vector width) and requires identical output; for a floating-point kernel the record of the kernel
   states the bound. The comparison of pictures (ADR 0014, ADR 0032) holds as before.
5. **The other rules stay.** `libc` is the only dependency (ADR 0013): vector code uses `core::arch`, not a crate.
   `unsafe` is counted (ADR 0037): a kernel first tries safe code the compiler vectorises (`chunks_exact`, fixed-size
   arrays); intrinsics that need `unsafe` stay inside the kernel's module, each block with its `SAFETY` comment, and
   raising the count is the maintainer's decision in the pull request that needs it.

## Alternatives

- **Keep `&[f32]` everywhere and only optimise the conversion.** The conversion stays a pass over every sample and the
  traffic of four bytes per sample stays; it is the cost this record removes.
- **One generic vector library for all families.** It would be a dependency (ADR 0013) or a second implementation of
  what `core::arch` gives, and it cannot know the native layouts of the families.
- **Fast paths free-standing in the screen's code, as so far.** Each one needs its own reason in review, and nothing
  shows that it computes the same as the plain code on the next family.
- **`cfg(target_arch)` in shared code.** Spreads target conditions over the tree; the reference would no longer be the
  same code on every host.

## Consequences

- A port starts with the references and is correct, if slow; its fast kernels come one by one, each with its test.
- The tests of an aarch64 kernel only run on an aarch64 host (macOS on Apple silicon, Linux on Arm, the scope); CI
  needs an Arm runner for them, or they run in the scope's test run of the pull request.
- The existing NEON loops of `ui/src/fastdraw.rs` move behind the table as kernels with references, in refactorings
  with `Behaviour: unchanged`.
- Worth a second look: when a kernel's reference and its fast versions differ in a way the test cannot pin down (for
  example rounding in a float sum over a deep record), or when the table grows past a handful of kernels.

## References

ADR 0001, ADR 0013, ADR 0014, ADR 0032, ADR 0037, ADR 0067, ADR 0074, ADR 0081; `ui/src/fastdraw.rs`,
`ui/src/board/pipe.rs`, `ui/src/compress.rs`, `dsp/src/`.
