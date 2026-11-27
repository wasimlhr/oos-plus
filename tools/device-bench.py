#!/usr/bin/env python3
# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
"""device-bench.py: the waveform performance of a running OOS, view by view, over SCPI only.

  tools/device-bench.py --host <ip> --out main.json              every view, once
  tools/device-bench.py --host <ip> --runs 3 --out main.json     three runs a view: median and spread
  tools/device-bench.py --host <ip> --views 4ch-1us-peak,4ch-1us-zoom
  tools/device-bench.py --compare installed.json main.json       the two side by side, as a table
  tools/device-bench.py --check main.json --baseline tools/bench/dho924s.json   exit 1 on a step back
  tools/device-bench.py --list                                   the views

The wiring: the generator's output on CH1 (`--gen-ch`), the other inputs open. Each view starts from `*RST`, as
`device-test.py` does, with the generator's 100 kHz square of 0 to 3.3 V, settles (`--settle`, 6 s) and is measured
(`--measure`, 12 s) from the counters of `:SYSTem:STATe?`: pictures a second, records shown (`used`), acquired and
dropped a second, the median step and render time; then the median and p90 of `*OPC?` and of a query
(`:TIMebase:SCALe?`), timed on this computer. The 18 views of the set `fork` are those of the comparison with the fork
yottabytesolutions/oos (issue #432); the set `extra` adds Peak below the full rate and Average with its count.
Back to `*RST` with the generator off at the end. It writes settings, nothing else. It runs when a release is
prepared and on request, not for every pull request (docs/device-testing.md, "Waveform performance").
"""
import argparse
import json
import math
import re
import statistics
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from oosclient import Link  # noqa: E402

# (name, channels on, s/div, acquisition type, zoom s/div or None, depth or None, extra commands)
VIEWS = {
    "fork": [
        ("1ch-5ns", 1, 5e-9, "NORM", None, None, ()),
        ("1ch-50ns", 1, 50e-9, "NORM", None, None, ()),
        ("1ch-200ns", 1, 200e-9, "NORM", None, None, ()),
        ("1ch-1us", 1, 1e-6, "NORM", None, None, ()),
        ("1ch-10us", 1, 10e-6, "NORM", None, None, ()),
        ("1ch-100us", 1, 100e-6, "NORM", None, None, ()),
        ("1ch-1ms", 1, 1e-3, "NORM", None, None, ()),
        ("2ch-1us", 2, 1e-6, "NORM", None, None, ()),
        ("4ch-5ns", 4, 5e-9, "NORM", None, None, ()),
        ("4ch-200ns", 4, 200e-9, "NORM", None, None, ()),
        ("4ch-1us", 4, 1e-6, "NORM", None, None, ()),
        ("4ch-10us", 4, 10e-6, "NORM", None, None, ()),
        ("4ch-1us-peak", 4, 1e-6, "PEAK", None, None, ()),
        ("4ch-1us-hires", 4, 1e-6, "HRES", None, None, ()),
        ("4ch-1us-average", 4, 1e-6, "AVER", None, None, ()),
        ("4ch-1us-zoom", 4, 1e-6, "NORM", 2e-7, None, ()),
        ("4ch-1us-measure", 4, 1e-6, "NORM", None, None,
         (":MEAS:ITEM VPP,CHAN1", ":MEAS:ITEM FREQ,CHAN1", ":MEAS:ITEM VRMS,CHAN1", ":MEAS:ITEM PDUT,CHAN1")),
        ("4ch-1us-again", 4, 1e-6, "NORM", None, None, ()),
    ],
    "extra": [
        ("4ch-100us-peak-1k", 4, 100e-6, "PEAK", None, "1k", ()),
        ("4ch-100us-peak-10k", 4, 100e-6, "PEAK", None, "10k", ()),
        ("4ch-1us-average-64", 4, 1e-6, "AVER", None, None, (":ACQ:AVER 64",)),
        ("1ch-1ms-zoom", 1, 1e-3, "NORM", 1e-5, None, ()),
    ],
}

# the measured fields, and how a step back is judged against a baseline: (field, worse when, tolerance as a factor,
# tolerance added in the field's unit)
FIELDS = ["pics_s", "used_s", "acq_s", "dropped_s", "step_ms", "render_ms", "opc_ms", "opc_p90_ms", "query_ms"]
LIMITS = [("pics_s", "lower", 0.95, 0.0), ("used_s", "lower", 0.85, 0.0), ("step_ms", "higher", 1.3, 1.0),
          ("opc_ms", "higher", 1.5, 2.0)]


def all_views():
    return [v for vs in VIEWS.values() for v in vs]


def pick(spec):
    """The views of `spec`: a set name (`fork`, `extra`, `all`) or view names, comma-separated."""
    out = []
    for part in spec.split(","):
        if part == "all":
            out += all_views()
        elif part in VIEWS:
            out += VIEWS[part]
        else:
            found = [v for v in all_views() if v[0] == part]
            if not found:
                raise SystemExit(f"device-bench: no view {part!r} (--list names them)")
            out += found
    return out


def parse_state(text):
    """The counters of a `:SYSTem:STATe?` line (NaN for one that is missing)."""
    def num(pattern):
        m = re.search(pattern, text)
        return float(m[1]) if m else math.nan
    return {"t": time.time(), "fps": num(r"fps ([\d.]+)"), "frames": num(r"frames (\d+)"),
            "acq": num(r"acquired (\d+)"), "used": num(r"acquired \d+ used (\d+)"),
            "dropped": num(r"acquired \d+ used \d+ dropped (\d+)"), "step": num(r"step ([\d.]+) ms"),
            "render": num(r"render ([\d.]+) ms")}


def rates(a, b, steps, renders):
    """One measurement from the counters `a` before and `b` after, and the step and render times read meanwhile."""
    dt = max(b["t"] - a["t"], 1e-9)
    med = lambda v: statistics.median(v) if v else math.nan  # noqa: E731
    return {"pics_s": (b["frames"] - a["frames"]) / dt, "used_s": (b["used"] - a["used"]) / dt,
            "acq_s": (b["acq"] - a["acq"]) / dt, "dropped_s": (b["dropped"] - a["dropped"]) / dt,
            "step_ms": med(steps), "render_ms": med(renders)}


def p90(v):
    s = sorted(v)
    return s[min(len(s) - 1, int(0.9 * len(s)))] if s else math.nan


def timed(link, command, n, pause=0.03):
    """Round trips of `command`, timed on this computer (ms)."""
    out = []
    for i in range(n):
        t = time.perf_counter()
        link.query(command)
        out.append((time.perf_counter() - t) * 1e3)
        time.sleep(pause * (i % 4))
    return out


def setup(link, gen_ch, view):
    name, nch, tdiv, typ, zoom, depth, extra = view
    for c in ["*RST", "*CLS", ":SOUR:FUNC SQU", ":SOUR:FREQ 100000", ":SOUR:VOLT 3.3", ":SOUR:VOLT:OFFS 1.65",
              ":SOUR:OUTP ON", f":CHAN{gen_ch}:SCAL 1", f":CHAN{gen_ch}:OFFS -1.65", f":TRIG:EDGE:SOUR CHAN{gen_ch}",
              ":TRIG:EDGE:LEV 1.65", ":TRIG:EDGE:SLOP POS", ":TRIG:SWE AUTO"]:
        link.send(c)
    on = [gen_ch] + [c for c in range(1, 5) if c != gen_ch][: nch - 1]
    for c in range(1, 5):
        link.send(f":CHAN{c}:DISP {'ON' if c in on else 'OFF'}")
    link.send(f":ACQ:TYPE {typ}")
    link.send(f":ACQ:MDEP {depth or 'AUTO'}")
    link.send(f":TIM:SCAL {tdiv}")
    link.send(f":TIM:DEL:ENAB {'ON' if zoom else 'OFF'}")
    if zoom:
        link.send(f":TIM:DEL:SCAL {zoom}")
    for c in extra:
        link.send(c)
    link.send(":RUN")
    link.query("*OPC?")


def measure(link, view, gen_ch, settle, length):
    setup(link, gen_ch, view)
    time.sleep(settle)
    a = parse_state(link.query(":SYST:STAT?"))
    steps, renders = [], []
    end = time.time() + length
    while time.time() < end:
        time.sleep(1)
        s = parse_state(link.query(":SYST:STAT?"))
        steps.append(s["step"])
        renders.append(s["render"])
    r = rates(a, parse_state(link.query(":SYST:STAT?")), steps, renders)
    opc = timed(link, "*OPC?", 40)
    query = timed(link, ":TIM:SCAL?", 20)
    r.update(opc_ms=statistics.median(opc), opc_p90_ms=p90(opc), query_ms=statistics.median(query))
    r.update(srate=link.query(":ACQ:SRAT?"), mdep=link.query(":ACQ:MDEP?"))
    return r


def summary(runs):
    """Median, low and high of each field over the runs of a view."""
    out = {}
    for k in FIELDS:
        v = [r[k] for r in runs if not math.isnan(r[k])]
        out[k] = {"median": statistics.median(v), "low": min(v), "high": max(v)} if v else None
    return out


def build_of(link):
    """`*IDN?` without the serial number: the model and the program's version."""
    p = link.query("*IDN?").split(",")
    return {"model": p[1] if len(p) > 1 else "?", "version": p[3] if len(p) > 3 else "?"}


def bench(o):
    link = Link(o.host, o.port, timeout=30)
    views = pick(o.views)
    result = {"label": o.label, "date": time.strftime("%Y-%m-%d %H:%M"), **build_of(link),
              "settle_s": o.settle, "measure_s": o.measure, "runs": o.runs, "views": {}}
    try:
        for view in views:
            runs = []
            for _ in range(o.runs):
                runs.append(measure(link, view, o.gen_ch, o.settle, o.measure))
            result["views"][view[0]] = {"runs": runs, "summary": summary(runs)}
            s = result["views"][view[0]]["summary"]
            print(f"{view[0]:22} " + "  ".join(f"{k} {fmt(s[k])}" for k in ("pics_s", "used_s", "step_ms", "opc_ms")),
                  flush=True)
    finally:
        link.send("*RST")
        link.send(":SOUR:OUTP OFF")
        link.query("*OPC?")
    if o.out:
        Path(o.out).write_text(json.dumps(result, indent=1) + "\n")
    return result


def fmt(s):
    if s is None:
        return "-"
    m = s["median"]
    text = f"{m:,.0f}" if abs(m) >= 100 else f"{m:.1f}"
    return text + (f" ({s['low']:.3g}-{s['high']:.3g})" if s["high"] != s["low"] else "")


def value(r, view, field):
    s = r["views"].get(view, {}).get("summary", {}).get(field)
    return None if s is None else s["median"]


def compare(a, b, fields=("pics_s", "used_s", "step_ms", "opc_ms")):
    """A markdown table of `a` against `b`, view by view: both values and the ratio b / a."""
    la, lb = a["label"], b["label"]
    head = "| View | " + " | ".join(f"{f} {la} / {lb} ({lb} ÷ {la})" for f in fields) + " |"
    rows = [head, "|" + "---|" * (len(fields) + 1)]
    for view in [v for v in a["views"] if v in b["views"]]:
        cells = []
        for f in fields:
            x, y = value(a, view, f), value(b, view, f)
            ratio = f"{y / x:.2f}x" if x and y is not None else "-"
            cells.append(f"{x if x is None else round(x, 1)} / {y if y is None else round(y, 1)} ({ratio})")
        rows.append(f"| {view} | " + " | ".join(cells) + " |")
    return "\n".join(rows)


def regressions(run, base, subset=False):
    """The steps back of `run` against `base`: one line each, by the limits of `LIMITS`. A view or value of the
    baseline the run lacks is one too, unless `subset` says the run measures a part of the views on purpose."""
    out = []
    for view in base["views"]:
        if view not in run["views"]:
            if not subset:
                out.append(f"{view}: not in the run")
            continue
        for field, worse, factor, add in LIMITS:
            b, r = value(base, view, field), value(run, view, field)
            if b is None:
                continue
            if r is None:
                out.append(f"{view}: no {field} in the run")
                continue
            if worse == "lower" and r < b * factor - add:
                out.append(f"{view}: {field} {r:.1f} below {b * factor - add:.1f} (baseline {b:.1f})")
            if worse == "higher" and r > b * factor + add:
                out.append(f"{view}: {field} {r:.1f} above {b * factor + add:.1f} (baseline {b:.1f})")
    return out


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--host")
    ap.add_argument("--port", type=int, default=5555)
    ap.add_argument("--views", default="fork,extra", help="sets (fork, extra, all) or view names, comma-separated")
    ap.add_argument("--runs", type=int, default=1)
    ap.add_argument("--settle", type=float, default=6.0)
    ap.add_argument("--measure", type=float, default=12.0)
    ap.add_argument("--gen-ch", type=int, default=1, help="the input the generator's output is wired to")
    ap.add_argument("--label", default="run")
    ap.add_argument("--out")
    ap.add_argument("--compare", nargs=2, metavar=("A", "B"))
    ap.add_argument("--check", metavar="RUN")
    ap.add_argument("--baseline")
    ap.add_argument("--subset", action="store_true", help="--check: the run measures a part of the views on purpose")
    ap.add_argument("--list", action="store_true")
    o = ap.parse_args(argv)
    if o.list:
        for set_name, vs in VIEWS.items():
            for v in vs:
                print(f"{set_name:6} {v[0]}")
        return 0
    if o.compare:
        a, b = (json.loads(Path(p).read_text()) for p in o.compare)
        print(compare(a, b))
        return 0
    run = json.loads(Path(o.check).read_text()) if o.check else None
    if run is None:
        if not o.host:
            ap.error("--host, --compare, --check or --list")
        run = bench(o)
    if o.baseline:
        base = json.loads(Path(o.baseline).read_text())
        if run.get("model") != base.get("model"):
            print(f"device-bench: the run is of a {run.get('model')}, the baseline of a {base.get('model')}")
            return 2
        bad = regressions(run, base, o.subset)
        for line in bad:
            print("STEP BACK", line)
        print(f"device-bench: {len(bad)} steps back against {o.baseline}")
        return 1 if bad else 0
    return 0


if __name__ == "__main__":
    sys.exit(main())
