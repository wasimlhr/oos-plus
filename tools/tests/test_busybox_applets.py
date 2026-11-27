# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
"""tools/busybox-applets.py: the command words of shell text and of Rust that starts programs, a call of a left-out
applet named with its file and line, and this tree: every applet its scripts call is in the image's busybox."""
import contextlib
import importlib.util
import io
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("busybox_applets", ROOT / "tools/busybox-applets.py")
ba = importlib.util.module_from_spec(spec)
spec.loader.exec_module(ba)


def words(text):
    return [w for _, w in ba.Shell(text).calls]


class ShellWords(unittest.TestCase):
    def test_commands_not_arguments(self):
        self.assertEqual(words("grep -q x /f | sort > /tmp/o; echo done && cat /f\n"), ["grep", "sort", "echo", "cat"])

    def test_substitutions_and_sh_c(self):
        t = 'x="$(uname -r)"; y=`hostname`\n(setsid sh -c \'/bin/telnetd -l /bin/sh\' < /dev/null &)\n'
        self.assertEqual(words(t), ["uname", "hostname", "setsid", "sh", "/bin/telnetd"])

    def test_quotes_comments_case_for_heredoc(self):
        t = ("# telnetd here is a comment\necho 'httpd tc' \"telnet\"\ncase $a in\n\ttc|httpd) ls;;\n\t*) ;;\nesac\n"
             "for f in tc httpd; do wc -l $f; done\ncat <<-X\n\ttelnetd\n\tX\nsync\n")
        self.assertEqual(words(t), ["echo", "ls", "wc", "cat", "sync"])

    def test_wrappers_and_keywords(self):
        t = "if ! timeout 5 nc host 1; then exec /bin/init; fi\nwhile true; do xargs -n 1 rm < /l; done\n"
        self.assertEqual(words(t), ["timeout", "nc", "exec", "/bin/init", "true", "xargs", "rm"])

    def test_lines(self):
        s = ba.Shell("a=1\n\nfoo\nx=$(\nbar)\n")
        self.assertEqual(s.calls, [(3, "foo"), (5, "bar")])

    def test_function_shadows(self):
        s = ba.Shell("tc() { echo; }\ntc\n")
        self.assertIn("tc", s.functions)


class RustCalls(unittest.TestCase):
    def test_programs_and_shell_lines(self):
        rs = ('const LOOP: &str = "while true; do sleep 2; done";\n'
              'fn f() {\n    let sh = |c: &str| std::process::Command::new("sh").arg("-c").arg(c).status();\n'
              '    sh("lsmod | grep -q x");\n    sh(&format!("insmod {}/x.ko", m));\n'
              '    std::process::Command::new("ip").args(["link"]);\n'
              '    std::process::Command::new("setsid").args(["sh", "-c", LOOP]);\n'
              '    die("watch runs on the unit only");\n}\n')
        got = [w for _, w in ba.rust_calls(rs)]
        self.assertEqual(sorted(got), sorted(["sh", "ip", "setsid", "true", "sleep", "lsmod", "grep", "insmod"]))


class Tree(unittest.TestCase):
    def tree(self, script):
        root = Path(tempfile.mkdtemp(prefix="oos-busybox-applets-"))
        self.addCleanup(shutil.rmtree, root)
        (root / "rootfs/initramfs/bin").mkdir(parents=True)
        (root / "rootfs/busybox.applets").write_text("# list\nsh\necho\n-telnetd\n")
        (root / "rootfs/initramfs/bin/x").write_text(script)
        return str(root)

    def test_left_out_applet_is_named(self):
        root = self.tree("#!/bin/sh\necho hi\n/bin/telnetd -p 23\n")
        self.assertEqual([(f, ln, a) for f, ln, a in ba.calls(root) if a == "telnetd"],
                         [("rootfs/initramfs/bin/x", 3, "telnetd")])
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = ba.main([], root)
        self.assertEqual(code, 1)
        self.assertIn("rootfs/initramfs/bin/x:3: calls `telnetd`", out.getvalue())

    def test_this_tree(self):
        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            code = ba.main([])
        self.assertEqual(code, 0, out.getvalue())


if __name__ == "__main__":
    unittest.main()
