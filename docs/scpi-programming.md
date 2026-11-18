# OOS Plus SCPI programmer's guide

A short, task-first guide to driving OOS Plus from a script: connect, set up, capture, read waveforms, measure,
handle errors. Each command here is in the full [SCPI reference](scpi.md), and the commands in the examples were checked against
the Windows app's demo signal. Check that reference for every parameter,
reply format and limitation.

## 1. Connect

| Interface | Port | Use it for |
| --- | --- | --- |
| Raw TCP socket | 5555 | Scripts. Simplest and recommended. |
| HiSLIP | 4880 | VISA tools that want `::INSTR` resources. |
| VXI-11 | 111 / core port | Older VISA setups. Scope image only, not advertised on Windows. |
| HTTP `POST /mcp` | web port (8080) | AI agents (JSON-RPC tool `scpi`). Not a VISA interface. |

VISA resource strings:

```text
TCPIP0::<address>::5555::SOCKET          raw socket (set the read/write termination to \n)
TCPIP0::<address>::hislip0::INSTR        HiSLIP on 4880
TCPIP0::<address>::hislip0,14880::INSTR  HiSLIP on another port
```

**Windows desktop app.** By default its services listen on this PC only, so use `127.0.0.1`. Start it with
`--lan` to reach it from another machine. Windows asks once about the firewall then. Ports can be changed with
`--scpi-port`, `--hislip-port` and `--web-port`, or through the launcher:

```powershell
.\run-windows.ps1 -Offline -ScpiPort 5555 -HislipPort 14880 -AppArgs --lan
```

**On the scope** the services are open on the LAN. Use the scope's address, shown under Utility > Network.

There is no authentication. Keep the instrument on a bench network.

## 2. Syntax in one minute

- One command per line, terminated by `\n`. Every reply ends with `\n`.
- Keywords are case-insensitive. Use the long form (`:TRIGger:EDGE:SOURce`) or the short form in capitals
  (`:TRIG:EDGE:SOUR`), nothing in between.
- Several commands on one line are separated by `;`, each with its full header starting with `:`.
  Send one query per line when its reply contains `;`.
- Numbers are in base units with plain or scientific notation: `2e-1` (V), `1e-6` (s). Not `200mV`.
  Memory depth also accepts `1M` and `10K`.
- Booleans: `ON|OFF|1|0`.
- Sources: `CHAN1` .. `CHAN4` (inputs) and `R1` .. `R10` (references, also `REF1` ..). Use `CHAN1`, not `CH1`.
- `9.9E37` means "no value" (unavailable or still being calculated). It is never a real reading.
- A query the scope cannot answer still replies, with the error as its text (`-113,"Undefined header" :FOO:BAR?`),
  so a client never hangs waiting on a bad query. Setters reply nothing; their errors go to the queue only.

## 3. A minimal client (Python, standard library only)

Save this as `oos.py` and import it from the examples below.

```python
import socket

class Scope:
    def __init__(self, host='127.0.0.1', port=5555, timeout=10):
        self.s = socket.create_connection((host, port), timeout=timeout)

    def write(self, cmd):
        self.s.sendall((cmd + '\n').encode('ascii'))

    def _exact(self, n):
        buf = bytearray()
        while len(buf) < n:
            part = self.s.recv(min(65536, n - len(buf)))
            if not part:
                raise EOFError('scope disconnected')
            buf += part
        return bytes(buf)

    def query(self, cmd):
        self.write(cmd)
        line = bytearray()
        while (ch := self._exact(1)) != b'\n':
            line += ch
        return line.decode('ascii').strip()

    def query_block(self, cmd):
        """IEEE definite-length block: #<N><length><payload>\\n."""
        self.write(cmd)
        assert self._exact(1) == b'#', 'not a block reply'
        n = int(self._exact(1))
        payload = self._exact(int(self._exact(n)))
        self._exact(1)  # trailing newline
        return payload

    def errors(self):
        """Drain the error queue; returns the list of errors (empty when all is well)."""
        out = []
        while not (e := self.query(':SYST:ERR?')).startswith('0,'):
            out.append(e)
        return out

    def close(self):
        self.s.close()
```

Never split a binary reply on newlines: read the length the block header announces, as `query_block` does.

With PyVISA, the raw socket needs explicit terminations:

```python
import pyvisa
rm = pyvisa.ResourceManager()
scope = rm.open_resource('TCPIP0::127.0.0.1::5555::SOCKET', read_termination='\n', write_termination='\n', timeout=10000)
print(scope.query('*IDN?'))
raw = scope.query_binary_values(':WAV:DATA?', datatype='B', container=bytes)  # BYTE format
```

## 4. Identify and reset

```text
*IDN?            -> OOS,<model>,<serial>,oos-ui <version>
*RST             default configuration (changes the whole workspace)
*CLS             clear error queue and status
:SYST:ERR?       -> 0,"No error"  or  <code>,"<text>"
```

`*IDN?` says `OOS`, not `RIGOL`. Scripts that check the vendor string need adjusting.

## 5. Set up channels, timebase and trigger

```text
:CHAN1:DISP ON
:CHAN1:COUP DC
:CHAN1:PROB 10
:CHAN1:SCAL 0.5              0.5 V/div (nearest supported scale)
:CHAN1:OFFS 0
:TIM:SCAL 1e-6               1 us/div (nearest supported)
:TIM:OFFS 0
:ACQ:MDEP 1M
:TRIG:MODE EDGE
:TRIG:EDGE:SOUR CHAN1
:TRIG:EDGE:SLOP POS          POS | NEG | RFAL
:TRIG:EDGE:LEV 0.8
:TRIG:SWE NORM               AUTO | NORMal | SINGle
```

Setters snap to the nearest supported value. Read the setting back (`:CHAN1:SCAL?`) when the exact value matters,
and check `:SYST:ERR?` after a setup block.

## 6. Capture

| Goal | Commands |
| --- | --- |
| Free run | `:RUN` |
| Freeze | `:STOP` |
| One triggered capture | `:SING`, then wait (below) |
| Capture now, trigger or not | `:TFOR` |
| Autoset | `:AUT`, then `*OPC?` |

Wait for a Single in either of two ways:

```python
scope.write(':SING')
scope.query('*OPC?')                   # blocks until the capture is done; returns '1'
# or poll, which lets you give up:
import time
deadline = time.time() + 5
while scope.query(':TRIG:STAT?') != 'STOP':
    if time.time() > deadline:
        scope.write(':TFOR')           # nothing triggered: force one
    time.sleep(0.05)
```

`*OPC?` waits as long as it takes. With no trigger event that can be forever, so set the socket timeout
accordingly or poll `:TRIG:STAT?` (`STOP | TD | AUTO | WAIT`). `*OPC?` covers Autoset, Single and SIM generation.
It does not wait for continuous Run or for measurements, Math, decode or Eye to finish calculating.

## 7. Read a waveform

```python
from oos import Scope
import struct

scope = Scope()
scope.write(':STOP')                   # data and preamble from the same capture
scope.write(':WAV:SOUR CHAN1')
scope.write(':WAV:MODE RAW')           # NORMal = screen record, RAW = full memory
scope.write(':WAV:FORM WORD')          # BYTE | WORD | ASCii
pre = [float(x) for x in scope.query(':WAV:PRE?').split(',')]
fmt, typ, points, count, xinc, xorg, xref, yinc, yorg, yref = pre
data = scope.query_block(':WAV:DATA?')
codes = struct.unpack(f'<{int(points)}H', data)    # WORD is little-endian u16
volts = [(c - yorg - yref) * yinc for c in codes]
times = [(k - xref) * xinc + xorg for k in range(int(points))]
print(int(points), 'points,', xinc, 's/pt, first', volts[0], 'V')
print(scope.errors())
```

- **Conversion:** `volts = (code - yorigin - yreference) * yincrement` and
  `time = (k - xreference) * xincrement + xorigin`.
- **BYTE** is one unsigned byte per point (`'B'` in `struct`). **WORD** is unsigned 16-bit **little-endian**
  (Rigol scripts that assume big-endian need changing). **ASCii** is comma-separated volts, with no scaling to apply.
- **Large records:** read a page at a time with `:WAV:STAR <first>` and `:WAV:STOP <last>`, counting from 1.
  A reply over 64 MiB is refused with error -223.
- **References:** use `:WAV:SOUR R1` to read loaded references the same way, with their native sample rate. Read
  it from the preamble, not from `:ACQ:SRAT?`.

## 8. Measure

```text
:MEAS:GAT NONE               NONE = whole record, SCREEN = what is on screen
:MEAS:ITEM? VPP,CHAN1        -> 1.234000E+00
:MEAS:ITEM? FREQUENCY,CHAN1
:MEAS:ITEM? RTIME,CHAN1
:MEAS:ITEM? RRDELAY,CHAN1,CHAN2     pair item: two sources
:MEAS:ITEM VPP,CHAN1         add it to the on-screen strip (with statistics)
:MEAS:STRIP?                 every strip row: ITEM,SOURCE,value;...
:MEAS:CLE                    clear the strip and statistics
```

The item names are listed in [scpi.md](scpi.md#scalar-identifiers): `VPP VMAX VMIN VAVG VRMS VAMP FREQUENCY
PERIOD RTIME FTIME PWIDTH NWIDTH PDUTY NDUTY OVERSHOOT ...`, and pairs `RRDELAY FFDELAY RRPHASE SETUP HOLD ...`.
Treat `9.9E37` as "no reading": the edge is missing or the analysis is still busy. Poll again.

Analysis pages have their own families: `:SPEC:ITEM? THD,CHAN1`, `:POW:ITEM? PF,CHAN1,CHAN2`, `:EYE:ITEM? HEIGHT`
and more ([scpi.md](scpi.md#measurements-and-analysis)).

## 9. Screenshot

```python
png = scope.query_block(':DISP:DATA? PNG')    # default without an argument: 24-bit BMP
open('screen.png', 'wb').write(png)
```

## 10. Errors

Read `:SYST:ERR?` until it returns `0,"No error"`. Each read removes one entry, and the queue holds 20.

| Code | Meaning |
| --- | --- |
| -113 | Undefined header: the command does not exist here |
| -221 | Settings conflict, such as `*OPC?` over a transport that cannot wait |
| -223 | Too much data: transfer a smaller page |
| -224 | Illegal parameter value |
| -230 | Data not available, such as no analysis result |
| -241 | Hardware missing on this model |
| -350 | Queue overflow |
| -410 | Query interrupted: a waiting `*OPC?` was cancelled by a re-arm |

A good habit: after each configuration block, call `errors()` and stop on anything but an empty list.

## 11. Complete example

```python
from oos import Scope

scope = Scope('127.0.0.1')
print(scope.query('*IDN?'))
scope.write('*CLS')
for cmd in (':CHAN1:DISP ON', ':CHAN1:SCAL 0.5', ':TIM:SCAL 1e-6', ':MEAS:GAT NONE',
            ':TRIG:MODE EDGE', ':TRIG:EDGE:SOUR CHAN1', ':TRIG:SWE NORM', ':RUN'):
    scope.write(cmd)
assert not scope.errors(), 'setup failed'

# a level that is surely crossed: halfway between the signal's extremes
vmax = float(scope.query(':MEAS:ITEM? VMAX,CHAN1'))
vmin = float(scope.query(':MEAS:ITEM? VMIN,CHAN1'))
assert vmax < 1e37 and vmin < 1e37, 'no signal on CHAN1'
scope.write(f':TRIG:EDGE:LEV {(vmax + vmin) / 2:.4e}')

scope.s.settimeout(30)                 # *OPC? waits until the Single has triggered
scope.write(':SING')
scope.query('*OPC?')
print('Vpp ', scope.query(':MEAS:ITEM? VPP,CHAN1'))
print('Freq', scope.query(':MEAS:ITEM? FREQUENCY,CHAN1'))
open('shot.png', 'wb').write(scope.query_block(':DISP:DATA? PNG'))
print('errors:', scope.errors())
scope.close()
```

## 12. Coming from a Rigol DHO script

- `*IDN?` reports `OOS`.
- WORD data is little-endian.
- `:WAV:MODE RAW` works while running. Stop first anyway when the preamble and data must match.
- References (`R1`..`R10`) can be trigger and measurement sources. They are stored records, not ADC inputs.
- Relative headers after `;` are not supported: give each command its full `:` path.
- Not every stock trigger subtree or status/SRQ register exists. Check errors after each block.
- Settings without a dedicated command are reachable through the setup schema:
  `:SYST:GET? <key>` / `:SYST:SET <key> = <value>` (keys as in a saved setup file).
