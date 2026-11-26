Changed:

- While the scope runs, the acquisition thread runs at real-time priority and the fast cores and the GPU do not drop
  to their lowest clocks; STOP gives the clocks back. 2 to 7 % more waveforms a second at the short time bases
  (DHO924S, 1 channel at 5 ns/div 7,230 -> 7,710). `perf.fifo = off` and `perf.clocks = off` in system.conf turn
  them off ([performance plan](docs/fork/perf-plan.md)).
