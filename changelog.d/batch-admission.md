Changed:

- Batch mode: at short time bases the scope arms only as many frames as the screen's workers can draw in one picture
  and waits for them, instead of reading batches it then drops; `acquired` and `dropped` of `:SYSTem:STATe?` fall
  towards `used` ([SCPI](docs/scpi.md)). 11 to 20 % more waveforms a second reach the screen (DHO924S, 1 channel at
  5 ns/div 7,230 -> 8,030; 4 channels at 200 ns/div 1,390 -> 1,670).
