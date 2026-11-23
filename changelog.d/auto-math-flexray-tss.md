Changed:

- Auto: every math channel on the screen (but an FFT) is fitted to its result after the set-up, as its Autoscale does,
  from the first record taken with the new set-up. Before, Math1 = CH1 - CH2 kept the scale and offset it had and
  could leave the screen (host; not yet seen on a unit).
- FlexRay: the reference says that the protocol trigger on Frame (`:TRIGger:BUS:WHEN FRAMe`) is the trigger on the
  transmission start sequence (TSS), at its end; a test holds it (host).
