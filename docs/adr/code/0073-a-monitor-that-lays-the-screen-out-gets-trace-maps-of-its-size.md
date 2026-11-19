# ADR 0073: A monitor that lays the screen out gets trace maps of its own size

- Status: proposed
- Date: 2026-11-19
- Category: code

## Context

Display > HDMI = Layout, and Waveform since pull request 81, draw the screen again at the monitor's size, up to 1920 x
1080. Text, grid and labels are sharp there; the traces were not. They came from the panel's phosphor maps (about
1004 x 218 to 440 cells), stretched to a plot of about 1920 x 1016: each cell a block of 1.9 x 2.3 or more monitor
pixels, nearest cell on the CPU, the cells' indices mixed by the GPU's filter. The owner's 1080p monitor (photo,
2026-11-19: Studio, zoom on, 10 ns/div at 625 MSa/s) showed the waveform "aliased and broken as in 8 bit pixel",
stair-steps of about two monitor pixels in the zoom view and in the overview. Graphics = GPU is the default, so the
filtered stretch was most likely the one on the photo; the photo does not say.

A map is filled from every acquired frame: the fast path (`Ui::fast_draw_all`, `drawpipe::draw_frame`), the deposit
workers (`drawpipe.rs`), the record path and the stopped picture (`Phosphor::draw_ex`), and cleared, decayed and halved
by persistence. Measured on the host (a PC, release build, 2026-11-19, four channels, `cost_of_the_monitor_s_maps`):
drawing a frame into the panel's map and a 1830 x 846 map costs 2.1 to 2.7 times the panel's map alone (0.14 ms ->
0.32 ms at 1 us/div with 12 500 points, 0.70 ms -> 1.56 ms at 100 ns/div with 1000); per screen the larger maps add
0.16 ms to clear, 0.24 ms for the grading's maximum and 0.31 ms per worker's maps merged. Not yet measured on the
scope.

## Decision

While a monitor pass in Layout or Waveform draws traces, the panel's maps carry a second set at the size the monitor
drew them (`Phosphor::hi`). Every clear, decay and draw of the panel's maps goes to it too; the fast path and the
deposit workers place each frame again at its size; the monitor's pass draws from it one cell a pixel, graded by its
own counts. A few steps without such a pass (no monitor, Mirror) drop it. `OOS_HDMI_TRACES=stretched` or
`hdmi.traces = stretched` in `system.conf` keeps the stretched maps.

## Alternatives

- **A filtered stretch** (bilinear on the CPU as on the GPU): the cheapest, no work per frame. It only softens the
  steps: a one-cell trace becomes two or three monitor pixels of half brightness, and the GPU already does it.
- **One set of maps at the monitor's size, reduced for the panel**: the panel's picture would change (a reduced map
  is not the map drawn at its size, ADR 0014), and the panel would pay for the larger maps with no monitor too.
- **The monitor's maps from the newest record of each screen only**: cheap, but another picture: at minimum
  persistence the panel shows every frame of the screen, graded; the monitor would show one.

## Consequences

- The monitor's traces are as sharp as the panel's: a one-pixel line is one monitor pixel (host test
  `the_monitor_draws_a_trace_one_of_its_pixels_wide`); the panel's picture is the same pixels as without a monitor.
- With a monitor in Layout or Waveform each frame costs about twice as much to draw. The fast path's take follows
  its time budget, so the screen rate should hold and fewer waveforms a second go into the picture; the workers on
  the little cores have half the throughput. Memory: about 12 MB per set of four maps at 1080p, also for the fast
  path's differences and each worker's maps. The GPU copies and uploads the larger maps for each monitor frame.
- `:SYSTem:STATe?` says `maps WxH` (or `maps panel`) after the monitor; `tools/device-test.py --tier a0`
  `frame_rate_by_graphics_and_monitor` fails when Layout or Waveform draws from the panel's maps, and reports the
  waveforms a second and the fast path's time of each mode. Worth a second look when that shows the screen rate or the
  waveform rate falling more than the monitor is worth: then the cheaper alternative above, or the maps only where
  the time base makes the stretch visible.
- Code that fills the maps another way than through `Phosphor` and `drawpipe::draw_frame` has to fill `hi` too.

## References

`ui/src/ui/hdmimaps.rs`, `ui/src/wave.rs` (`Phosphor::hi`), `ui/src/drawpipe.rs`, `ui/src/main.rs`
(`GpuLayers::monitor_quad`), [gpu/README.md](../../../gpu/README.md),
[ADR 0003](0003-gpu-drawing-in-a-helper-process.md), pull request 81 (Waveform at the monitor's resolution).
