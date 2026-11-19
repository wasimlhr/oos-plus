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
0.16 ms to clear, 0.24 ms for the grading's maximum and 0.31 ms per worker's maps merged.

A first version drew every frame into both sets. On a DHO924S (verified on the unit, 2026-11-19: 1080p monitor,
Graphics = GPU, Studio, two channels, zoom, a Manchester decoder, 100 ns/div at 625 MSa/s, running) the monitor's
traces were sharp ("Yes, sharp", the owner), but the scope paid for them: Mirror 9.7 screens a second and about 460
waveforms a second; Layout 9.5 before, 5.0 to 5.6 and 240 to 260 waveforms a second with the second maps (the step's
drawing 54 -> 130 ms); Waveform 4.7 to 5.8 and about 215. The helper's GPU time for the monitor's frame was 44 to
54 ms. Stopped, every mode kept 30 screens a second. The owner's rule: the monitor must not cost the scope's own
picture its rate; what the monitor does not get within that is dropped.

## Decision

While a monitor pass in Layout or Waveform draws traces, the panel's maps carry a second set at the size the monitor
drew them (`Phosphor::hi`). Every clear and decay of the panel's maps goes to it too; the monitor's pass draws from it
one cell a pixel, graded by its own counts. A few steps without such a pass (no monitor, Mirror) drop it.

Frames go into it within a **time budget of 3 ms a step** (`hdmimaps::BUDGET`), not all of them: the first record of
a step always; further records while the step's time on these maps is under the budget; of the fast path's frames,
the newest that fit the rest of the budget at the pace it measured (one while it is not known; a pace above the
budget is tried again now and then); the deposit workers draw the panel's maps alone. The fast path's own take
follows its time without the monitor's maps. So the panel takes every frame, as without a monitor; the monitor's
picture takes a sample of them and is graded by its own count of frames. `OOS_HDMI_TRACES=stretched` or
`hdmi.traces = stretched` in `system.conf` keeps the stretched maps.

## Alternatives

- **A filtered stretch** (bilinear on the CPU as on the GPU): the cheapest, no work per frame. It only softens the
  steps: a one-cell trace becomes two or three monitor pixels of half brightness, and the GPU already does it.
- **One set of maps at the monitor's size, reduced for the panel**: the panel's picture would change (a reduced map
  is not the map drawn at its size, ADR 0014), and the panel would pay for the larger maps with no monitor too.
- **Every frame into both sets** (the first version): the monitor's picture the panel's exactly, at about half the
  screens and waveforms a second on the unit (Context). Against the owner's rule.
- **The monitor's maps from the newest record of each screen only**: cheapest, but at minimum persistence one frame
  against the panel's whole screen of them. The budget gives it that one at least, and more where they fit.

## Consequences

- The monitor's traces are as sharp as the panel's: a one-pixel line is one monitor pixel (host test
  `the_monitor_draws_a_trace_one_of_its_pixels_wide`); the panel's picture is the same pixels as without a monitor.
- With many frames a second the monitor's picture holds fewer of them than the panel's: at minimum persistence a
  sparser picture of a jittering or rare event, with persistence it fills more slowly. A steady signal looks the same.
- What a step still pays: up to the budget (more when its first record alone takes longer), clearing and grading
  the larger maps (about 0.4 ms on a PC), memory (about 12 MB per set of four maps at 1080p, and the fast path's
  differences), and on the GPU the larger maps copied and uploaded with each monitor frame, which the budget does not
  cover. On a PC (release, four channels, a fast-path screen of 32 frames) the monitor's maps add 1.0 to 2.2 ms to it
  and take 2.7 to 6.8 of the frames. Not yet measured on the unit with the budget.
- `:SYSTem:STATe?` says `maps WxH` (or `maps panel`) after the monitor; `tools/device-test.py --tier a0`
  `frame_rate_by_graphics_and_monitor` fails when Layout or Waveform draws from the panel's maps, and reports the
  screens and waveforms a second and the fast path's time of each mode. If Layout and Waveform still fall below
  Mirror there, or below 15 screens a second where Mirror reaches them, the owner's rule drops the second maps: then
  `hdmi.traces = stretched` becomes the default, or the record is superseded.
- Code that fills the maps another way than through `Phosphor` and `drawpipe::draw_frame` has to fill `hi` too, or
  set it aside (`Ui::hi_hold`).

## References

`ui/src/ui/hdmimaps.rs`, `ui/src/wave.rs` (`Phosphor::hi`), `ui/src/drawpipe.rs`, `ui/src/main.rs`
(`GpuLayers::monitor_quad`), [gpu/README.md](../../../gpu/README.md),
[ADR 0003](0003-gpu-drawing-in-a-helper-process.md), pull request 81 (Waveform at the monitor's resolution).
