# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
"""device-bench.py without a scope: the counters of a state line, the rates, the summary of runs, the comparison table
and the steps back against a baseline."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("device_bench", ROOT / "tools/device-bench.py")
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)

STATE = ("fps 29.9 frames 1623 records 270 run Run status Auto t 56.4 step 2.2 ms (acq 0.8 draw 0.8) render 4.8 ms "
         "present 0.0 ms pipeline Batch interp Exact acquired 289 used 281 dropped 7 measured 0 compress k 1")


def run(label, **views):
    """A result as `bench` writes it: per view the medians given (low = high = median)."""
    return {"label": label, "views": {v: {"summary": {k: {"median": x, "low": x, "high": x} for k, x in f.items()}}
                                     for v, f in views.items()}}


class Bench(unittest.TestCase):
    def test_a_state_line_gives_its_counters(self):
        s = bench.parse_state(STATE)
        self.assertEqual((s["frames"], s["acq"], s["used"], s["dropped"]), (1623, 289, 281, 7))
        self.assertEqual((s["step"], s["render"], s["fps"]), (2.2, 4.8, 29.9))
        self.assertTrue(bench.math.isnan(bench.parse_state("fps 30")["used"]), "a missing counter is NaN")

    def test_rates_are_counts_over_the_time_between(self):
        a = {"t": 10.0, "frames": 100, "used": 1000, "acq": 2000, "dropped": 50}
        b = {"t": 12.0, "frames": 160, "used": 8000, "acq": 9000, "dropped": 1050}
        r = bench.rates(a, b, [4.0, 5.0, 6.0], [2.0])
        self.assertEqual((r["pics_s"], r["used_s"], r["acq_s"], r["dropped_s"]), (30, 3500, 3500, 500))
        self.assertEqual((r["step_ms"], r["render_ms"]), (5.0, 2.0))

    def test_runs_summarise_as_median_and_spread(self):
        runs = [{k: float(i) for k in bench.FIELDS} for i in (3, 1, 2)]
        s = bench.summary(runs)["pics_s"]
        self.assertEqual((s["median"], s["low"], s["high"]), (2, 1, 3))

    def test_every_view_has_a_unique_name_and_sets_pick_them(self):
        names = [v[0] for v in bench.all_views()]
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual(len(bench.pick("fork")), 18, "the 18 views of the comparison with the fork (#432)")
        self.assertEqual([v[0] for v in bench.pick("1ch-1ms,4ch-1us-zoom")], ["1ch-1ms", "4ch-1us-zoom"])
        with self.assertRaises(SystemExit):
            bench.pick("no-such-view")

    def test_the_comparison_shows_both_values_and_the_ratio(self):
        a = run("installed", **{"4ch-1us-peak": {"pics_s": 13.2, "used_s": 632}})
        b = run("main", **{"4ch-1us-peak": {"pics_s": 29.9, "used_s": 3642}})
        table = bench.compare(a, b, fields=("pics_s", "used_s"))
        self.assertIn("| 4ch-1us-peak | 13.2 / 29.9 (2.27x) | 632 / 3642 (5.76x) |", table)
        self.assertIn("pics_s installed / main (main ÷ installed)", table, "the header names the ratio's direction")

    def test_a_view_the_run_lacks_is_a_step_back_unless_it_is_a_subset(self):
        base = run("base", v={"pics_s": 30.0}, w={"pics_s": 30.0})
        part = run("part", v={"pics_s": 30.0})
        self.assertEqual(bench.regressions(part, base), ["w: not in the run"])
        self.assertEqual(bench.regressions(part, base, subset=True), [])
        no_field = run("nf", v={"used_s": 3600}, w={"pics_s": 30.0})
        self.assertEqual(bench.regressions(no_field, base), ["v: no pics_s in the run"])

    def test_a_step_back_is_found_and_noise_is_not(self):
        base = run("base", v={"pics_s": 30.0, "used_s": 3600, "step_ms": 5.0, "opc_ms": 1.0})
        same = run("same", v={"pics_s": 29.5, "used_s": 3200, "step_ms": 6.0, "opc_ms": 2.5})
        self.assertEqual(bench.regressions(same, base), [], "within the limits")
        worse = run("worse", v={"pics_s": 12.0, "used_s": 600, "step_ms": 80.0, "opc_ms": 30.0})
        found = bench.regressions(worse, base)
        self.assertEqual([x.split(":")[1].split()[0] for x in found], ["pics_s", "used_s", "step_ms", "opc_ms"])

    def test_check_against_a_baseline_ends_with_one_on_a_step_back(self):
        base = run("base", v={"pics_s": 30.0})
        with tempfile.TemporaryDirectory() as d:
            for name, r in (("base", base), ("good", run("good", v={"pics_s": 30.0})),
                            ("bad", run("bad", v={"pics_s": 10.0}))):
                Path(d, name + ".json").write_text(json.dumps(r))
            call = lambda r: subprocess.run(  # noqa: E731
                [sys.executable, str(ROOT / "tools/device-bench.py"), "--check", str(Path(d, r + ".json")),
                 "--baseline", str(Path(d, "base.json"))], capture_output=True, text=True)
            self.assertEqual(call("good").returncode, 0)
            bad = call("bad")
            self.assertEqual(bad.returncode, 1)
            self.assertIn("STEP BACK v: pics_s", bad.stdout)
            other = run("other", v={"pics_s": 30.0})
            other["model"] = "DHO804"
            Path(d, "other.json").write_text(json.dumps(other))
            self.assertEqual(call("other").returncode, 2, "a run of another model is not checked against this one")

    def test_bench_builds_sets_label_and_file_itself(self):
        call = lambda *a: subprocess.run(["sh", str(ROOT / "tools/bench-builds.sh"), *a],  # noqa: E731
                                         capture_output=True, text=True)
        self.assertEqual(call().returncode, 2, "without ip and refs: the usage")
        refused = call("192.0.2.1", "a", "b", "--out", "x.json")
        self.assertEqual(refused.returncode, 2)
        self.assertIn("is set per build", refused.stderr)


if __name__ == "__main__":
    unittest.main()
