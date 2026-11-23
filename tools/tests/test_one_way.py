# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
"""tools/one-way.py on small made-up trees: places by file, rules by name and code, comments and tests left out,
the inner of two nested functions; and on this tree: every idea of its table still finds its place."""
import contextlib
import importlib.util
import io
import re
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("one_way", ROOT / "tools/one-way.py")
ow = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ow)

CRC = ("pub fn crc8(d: &[u8]) -> u8 {\n    let mut c = 0u8;\n    for &b in d {\n        c = (c << 1) ^ b;\n"
       "    }\n    c\n}\n")


class OneWay(unittest.TestCase):
    def tree(self, files):
        root = Path(tempfile.mkdtemp(prefix="oos-one-way-"))
        self.addCleanup(shutil.rmtree, root)
        for crate in ("hal", "dsp", "ui", "fpgad"):
            (root / crate / "src").mkdir(parents=True)
        for rel, text in files.items():
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            (root / rel).write_text(text)
        return str(root)

    def found(self, root, ideas=None):
        return ow.implementations(ow.dup.read_tree(root), ideas or ow.compiled())

    def run_tool(self, *argv):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = ow.main(list(argv))
        self.assertEqual(code, 0, "a report: exit 0 whatever it counts")
        return out.getvalue()

    def test_an_idea_in_two_files_is_one_place_too_many(self):
        root = self.tree({"hal/src/a.rs": CRC, "ui/src/b.rs": CRC.replace("crc8", "crc_png")})
        found = self.found(root)
        self.assertEqual(found["crc"], {"hal/src/a.rs": [(1, "crc8")], "ui/src/b.rs": [(1, "crc_png")]})
        self.assertEqual(ow.count(found), (1, 1))
        self.assertIn("one-way: 1 places too many, 1 of %d ideas" % len(ow.IDEAS), self.run_tool(root))
        listed = self.run_tool("--list", root)
        self.assertIn("crc: 2 places (", listed)
        self.assertIn("  hal/src/a.rs: crc8:1\n  ui/src/b.rs: crc_png:1\n", listed)
        self.assertIn("base64: 0 places (", listed)

    def test_functions_of_one_file_are_one_place(self):
        # the widths of one reader side by side: one implementation
        reader = "".join("fn %s(b: &[u8], o: usize) -> %s {\n"
                         "    %s::from_le_bytes(b[o..o + 4].try_into().unwrap())\n}\n"
                         % (n, t, t) for n, t in (("u32_at", "u32"), ("i32_at", "i32")))
        found = self.found(self.tree({"ui/src/font.rs": reader}))
        self.assertEqual(found["int-at"], {"ui/src/font.rs": [(1, "u32_at"), (4, "i32_at")]})
        self.assertEqual(ow.count(found), (0, 0))

    def test_the_name_and_every_code_pattern_have_to_match(self):
        root = self.tree({
            # the name says CRC, but nothing is shifted: a set of parameters for an engine elsewhere
            "dsp/src/a.rs": "pub fn modbus_crc(b: &[u8]) -> u16 {\n    Crc::reflected(16).bytes(b) as u16\n}\n",
            # shifted, but no CRC
            "dsp/src/b.rs": "fn half(x: u32) -> u32 {\n    x >> 1\n}\n",
            # `=` split, but of one argument, not of the lines of a text
            "ui/src/c.rs": "fn arg(a: &str) -> Option<(&str, &str)> {\n    a.split_once('=')\n}\n",
            "ui/src/d.rs": "fn conf(t: &str) {\n    for l in t.lines() {\n        let _ = l.split_once('=');\n"
                           "    }\n}\n",
        })
        found = self.found(root)
        self.assertEqual(found["crc"], {})
        self.assertEqual(found["key-value-lines"], {"ui/src/d.rs": [(1, "conf")]})

    def test_comments_and_tests_are_not_read(self):
        root = self.tree({
            "hal/src/lib.rs": "// fn crc(c: u8) -> u8 { c << 1 }\n"
                              "/// the alphabet ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/\n"
                              "pub fn f() {\n    /* c >> 1 */\n}\n"
                              "#[cfg(test)]\nmod tests {\n" + CRC + "}\n",
            "ui/src/tests.rs": CRC,
            "dsp/src/b64.rs": 'fn encode() {\n'
                              '    let t = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";\n}\n',
        })
        found = self.found(root)
        self.assertEqual(found["crc"], {})
        self.assertEqual(found["base64"], {"dsp/src/b64.rs": [(1, "encode")]}, "a string is code, a comment is not")

    def test_of_two_nested_functions_the_inner_one_is_named(self):
        text = ("fn outer() {\n    fn crc_step(c: u8) -> u8 {\n        c << 1\n    }\n}\n"
                "fn crc_all() {\n    x >> 8;\n}\n")
        found = self.found(self.tree({"hal/src/a.rs": text}))
        self.assertEqual(found["crc"], {"hal/src/a.rs": [(2, "outer::crc_step"), (6, "crc_all")]})

    def test_a_method_is_named_with_its_type(self):
        text = "impl Crc {\n    pub fn bits(self) -> u32 {\n        self.r >> 1\n    }\n}\n"
        self.assertEqual(self.found(self.tree({"dsp/src/crc.rs": text}))["crc"], {"dsp/src/crc.rs": [(2, "Crc::bits")]})

    def test_every_rule_compiles_and_the_keys_are_unique(self):
        keys = [key for key, _, _ in ow.IDEAS]
        self.assertEqual(len(keys), len(set(keys)))
        for key, what, rules in ow.IDEAS:
            self.assertTrue(re.fullmatch(r"[a-z0-9-]+", key), key)
            self.assertTrue(what and rules, key)
            for name, code in rules:
                self.assertTrue(name or code, "%s: a rule without a name needs code" % key)

    def test_every_idea_finds_its_place_in_this_tree(self):
        # a rule that finds nothing any more has rotted (a renamed helper, a changed idiom): fix the rule, or take
        # the idea out of the table when the tree no longer has it
        found = self.found(str(ROOT))
        self.assertEqual([key for key, places in found.items() if not places], [])


if __name__ == "__main__":
    unittest.main()
