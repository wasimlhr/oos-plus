Fixed:

- With the zoom window on, the live channels on the external display (HDMI Layout) are drawn from the acquisition's own
  samples with sin(x)/x between them, as the panel does, instead of straight lines between the 3600 points of the whole
  screen window. Fast edges and bus signals no longer turn into triangles.
