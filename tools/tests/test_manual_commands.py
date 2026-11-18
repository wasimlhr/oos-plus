# SPDX-License-Identifier: GPL-2.0-or-later

"""The command reference of the SCPI manual is what its generator writes from docs/scpi. Offline: files only."""
import copy
import datetime
import importlib.util
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


gen = load("oos_gen_commands", ROOT / "docs/manual/gen_commands.py")


class ManualCommands(unittest.TestCase):
    def test_every_page_of_the_reference_is_a_group_or_named_as_none(self):
        self.assertEqual(gen.unlisted_pages(), [], "add the page to PAGES of docs/manual/gen_commands.py")
        for page in gen.PAGES:
            self.assertTrue((gen.DOCS / f"{page}.md").is_file(), page)

    def test_the_command_file_is_what_the_generator_writes_now(self):
        why = gen.stale()
        self.assertIsNone(why, f"docs/manual/commands.json is behind docs/scpi: {why}. "
                               "Run: python docs/manual/gen_commands.py")

    def test_a_file_that_is_behind_says_what_it_lacks(self):
        want, _ = gen.build(datetime.date(2026, 1, 1))
        self.assertIsNone(gen.difference(copy.deepcopy(want), want))
        # a command the reference gained
        have = copy.deepcopy(want)
        gone = have["subsystems"][0]["commands"].pop()
        self.assertIn(f"1 not in it ({gone['header']})", gen.difference(have, want))
        # a row whose text changed, and one the reference dropped
        have = copy.deepcopy(want)
        have["subsystems"][0]["commands"][0]["desc"] += " Once more."
        have["subsystems"][1]["commands"].append({"header": ":NO:SUCH"})
        why = gen.difference(have, want)
        self.assertIn("1 no longer in the reference (:NO:SUCH)", why)
        self.assertIn(f"1 changed ({want['subsystems'][0]['commands'][0]['header']})", why)
        # a version behind the tree, and a text that is not a command
        have = copy.deepcopy(want)
        have["meta"]["version"] = "0.0.0"
        self.assertIn("it is of oos-ui 0.0.0", gen.difference(have, want))
        have = copy.deepcopy(want)
        have["intro"] = []
        self.assertIn("a text beside the commands changed", gen.difference(have, want))


if __name__ == "__main__":
    unittest.main()
