Fixed:

- Studio shows the name of a channel on its bottom bar pill also where the pill is narrow (five or more sources): the
  name is cut with a dot behind it instead of falling back to CH1 to CH4.
- A scale label at the right edge of a Studio lane moves left of a handle, so the wider handle of a named source no
  longer covers the "0 V" of its lane.

Interface:

- A math trace can be named: `:MATH<n>:LABel:CONTent` (up to 10 characters, kept in the setup as `math_label<n>`).
  Studio shows the name on the trace's handle and pill.
