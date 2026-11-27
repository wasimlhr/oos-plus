# Testing on a unit

`tools/device-test.py` tests a running OOS over the network, with nobody at the scope. It reads the card and
the calibration but never writes them; the build under test runs from `/tmp`
(`tools/device-deploy.sh IP`, back with `--restore`). A full run of the tiers a0 to a2 takes about 17 minutes.
Most changes need only a part of it, and a measurement on real hardware is sometimes noisy. The options below keep a
run short and its verdict sound, so that it does not have to be repeated by hand.

## A typical run for a pull request

```sh
tools/device-test.py --host IP --changed origin/main --list   # which tests, and why (no scope needed)
tools/device-deploy.sh IP                                    # the build of this tree, in RAM
tools/device-test.py --host IP --changed origin/main --plan plan.txt
tools/device-deploy.sh IP --restore                          # the installed oos-ui again
```

`plan.txt` is `OOS_CAL_DIR=local/cal tools/plan-matrix.sh fpgad/target/release/oos-fpgad > plan.txt` on this
computer, with the calibration files of the same unit.

## SSH on a bench unit

Both tools reach the scope's shell over SSH only, with a trusted key and strict host key checking; there is no telnet
(ADR 0054). `device-test.py` needs it for its read-only commands (the fingerprint of the protected areas, the build,
the log), `device-deploy.sh` for everything. Once per unit, at the scope ([ssh.md](ssh.md), network.md 7):

1. System > Network > Remote services > **Allowed**: the prefix of the bench LAN (`192.0.2.0/24`). SSH listens only
   with an address list.
2. **SSH** on. SSH settings > State says `on: password and keys`; Show password gives the password and the host key
   fingerprint.
3. On the computer: `ssh root@IP` once, compare the fingerprint it shows with the one on the scope and answer `yes`
   (the tools refuse an unknown host key), then `ssh-copy-id root@IP` with the password. A key can also come with the
   image (`OOS_SSH_AUTHORIZED_KEYS`), from a USB stick, or be written to the card on a computer (ssh.md).
4. `ssh -o BatchMode=yes root@IP true` answers without a question: the tools work. `OOS_SSH_IDENTITY` names another
   key file, `OOS_SSH_PORT` another port.

The keys, the host key and the password stay on the data partition (`/oos/ssh`) over updates and power cycles.

## What a run from RAM does not cover

`tools/device-deploy.sh` restarts `oos-ui` with the new programs; it does not start the scope. What runs once at boot
is not reached: `oos-fpgad cold-boot` and `fpga-boot` from the installed image, the scripts of the initramfs, the
kernel, an update. A change there needs the image installed and the scope started from power off, with a person at
the unit, before the label `unit-tested` is set (ADR 0052). `tools/pr-gate.sh` names the classes: `$proven`
(the comparison proves a refactoring), `$live` (a run from RAM or more).

## Power at the bench

With a switchable outlet at the bench (ADR 0070, proposed), the start from power off needs nobody there.
`tools/bench-power.sh` switches it: `status`, `on`, `off`, and `cycle`, which turns it off for 30 s and on again. The
outlet goes into `local/bench.conf` as `OOS_POWER=shelly:HOST`, `tasmota:HOST` or `cmd:PROGRAM`. The scope to ask is
`OOS_SCOPE`, with the same SSH key and port as `tools/device-deploy.sh`. Before it cuts the power, the tool takes the
scope's update lock over SSH and holds it during the cut, so no update can begin to write. It refuses when an
`oos-update` runs, when the lock is taken, and when the scope does not answer. An install started by kexec has no SSH
and must not lose power. `--force` only cuts a scope that does not answer at all. The tool is not yet verified on
hardware.

A run of the boot class with it:

1. Dry run: `oos-update install FILE`.
2. Install: `oos-update install FILE --execute`.
3. Cold start: `tools/bench-power.sh cycle`.
4. Wait until the start-up watcher confirms the start (`oos-update status`).
5. Check: `tools/device-test.py --tier a0`.

The pull request says which commit ran, and that it ran from power off.

## Only what the change can affect

`--changed BASE` takes the files the tree changes against `BASE`:
- the commits since the common ancestor;
- what is not committed yet;
- new files.

Each file is matched against the rules in `tools/device_select.py`. The first rule that matches gives its areas, and
each area names its tests. Some tests always run: identity, the fingerprint of the protected areas before and after,
the build on the unit, the kernel log, and where the generator reaches.

| A change of | runs |
|---|---|
| documents, CI, host tools, tests of the crates | the smoke tests only |
| `hal/src/dho/roll*` | roll, acquisition, register sequences |
| `hal/src/dho/la*` | logic analyser, register sequences |
| trigger, mask, zone, AUX OUT in `hal/src/dho/` | trigger, register sequences |
| AFE, ADC, calibration, PLL in `hal/src/dho/` | front end, acquisition, deep record, register sequences |
| other parts of `hal/src/dho/`, `hal/` | acquisition, deep record, register sequences |
| `ui/src/board/` | acquisition, deep record, roll, trigger, front end, register sequences |
| `fpgad/` | register sequences |
| SCPI, web, VXI-11, MCP | remote |
| storing, the logger | store |
| `dsp/`, measurements, decoders | measure, decode |
| the measurement code (`dsp/src/measure.rs`, the measurement paths of `ui/`) | measure, acquisition, deep record, roll, front end, generator, trigger |
| `ui/src/deepmeas.rs` | deep record, acquisition, measure |
| other parts of `ui/`, `gpu/` | screen, remote |
| `boot/`, `kernel/`, `rootfs/`, `hal/src/update/`, the cold boot, `Cargo.toml`/`Cargo.lock`, the test tools themselves, a file no rule knows | everything |

`--list` prints the files with their areas and the tests that would run, and stops. The changes of the last pull
requests chose between 6 tests (documents, CI) and 13 to 23 (one part of the hardware); all 90 run only when the
image, the update path, the build or the tests themselves change.

## Noise and failures that were there before

- **`--retries N`** (default 1): a test that fails runs again, up to N times. One that passes then is `FLAKY`; its
  first failure stays in the report. A test that fails every time is `FAIL` with each attempt's detail: a retry
  absorbs noise, not a fault that is always there.
- **`--known FILE`** (default `device-test/known.json`): the results of a run of main on the same unit. A failure that
  the known run has too is `KNOWN`, not a failure of the change; a difference of the register sequences and a failed
  smoke test never are. `--save-known` writes the run into that file: do it with the installed image or a build of
  main, after main changed in the hardware paths.

`FLAKY` and `KNOWN` are in the report and do not fail the run (exit code 0). A test that is `FLAKY` again and again
is a fault of its own: of the test, or of OOS. It deserves an issue, not more retries.

## The register sequences

`plan_matches_this_build` (with `--plan`) runs `plan-matrix.sh` on the unit with its calibration and compares the
output with this computer's, section by section. Three differences come from the setup and are not counted:

- where the calibration files lie;
- whether a logic probe is plugged in;
- a section that uses a calibration file one side lacks (this computer often has no `cal_afg.hex`): left out and
  named as `INFO`.

Any other difference fails the test and names the section and the first line that differs.

## When a test fails only with one build

Compare with main on the same unit, the test alone (`--only NAME`), more than once each. Deploy the build of main,
run the test, deploy the change, run it again. A test that passes with main every time and fails with the change
every time points at the change, even when the host checks see no difference: the timing on the unit is part of what
is tested.

## Waveform performance

`tools/device-bench.py` measures how fast a running OOS draws, view by view, over SCPI only: pictures a second, records
shown, acquired and dropped a second, the step and render time from `:SYSTem:STATe?`, and the time `*OPC?` and a query
take, seen from this computer. The generator's output is wired to CH1 (`--gen-ch`), the other inputs stay open; each
view starts from `*RST` with the generator's 100 kHz square of 0 to 3.3 V, settles for 6 s and is measured for 12 s.
The set `fork` holds the 18 views of the comparison with the fork yottabytesolutions/oos (#432), the set `extra` Peak
below the full rate, Average with a count of 64 and a zoom at 1 ms/div (`--list`). One run of both takes about 9
minutes; `--runs 3` shows the spread.

It is not part of the run for a pull request. It runs when a release is prepared, before its pull request
(`tools/changelog.py --release`), with the build of main against the baseline of the board, and whenever the
maintainer asks for it, for instance to compare two builds.

```sh
tools/device-bench.py --host IP --out run.json                        # the build running now
tools/device-bench.py --check run.json --baseline tools/bench/dho924s.json   # exit 1 on a step back
tools/device-bench.py --compare a.json b.json                          # two runs side by side
tools/bench-builds.sh IP installed HEAD --runs 3                       # two builds, deployed one after the other
```

A step back against the baseline is fewer than 95 % of its pictures a second or 85 % of its records shown, a step over
1.3 times its own plus 1 ms, or `*OPC?` over 1.5 times plus 2 ms. A step back found before a release is fixed or named
in the release before it goes out. `--check` also counts a view or value of the baseline that the run lacks (unless
`--subset`), and refuses a run of another model (exit 2). When the release run has none, its numbers become the new
baseline in the release's pull request (`--runs 3`, the medians): the baseline only gets better. The first release run
with the tool writes it: until then there is no baseline, and `--check` has nothing to compare with.

`tools/bench-builds.sh` builds each ref (a commit, branch or tag, or `installed`) in a worktree of its own, deploys it
to RAM, measures it, restores the installed oos-ui and prints the table of the two: the comparison of two builds on
one unit, as #432 has it for 0.1.15, main and the fork.
