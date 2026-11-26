#!/usr/bin/env python3
# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
"""busybox-applets.py: every busybox applet the image's scripts and programs call is in the image's busybox.

    tools/busybox-applets.py           exit 1, naming the file, the line and the applet, for each call of an applet
                                       that rootfs/busybox.applets marks as left out
    tools/busybox-applets.py --list    the applets that are called, each with its first place

rootfs/busybox.applets lists every applet of the pinned busybox: a plain line is in the image, `-name` is left out by
rootfs/busybox.config (ADR 0078; rootfs/build-busybox.sh stops when the build differs from the list). The calls are
read from the shell scripts of the image (rootfs/initramfs, rootfs/board/*: every file whose first line starts a
shell), etc/inittab, and the Rust programs for the scope that start other programs (the files of hal/src, fpgad/src,
ui/src and ui/system/src with `Command::new`): the program of each `Command::new("...")`, and the command line given
to `sh -c` (see rust_calls).

The shell scripts are read as commands, not as text: the first word of each command, also inside `$(...)`, `` `...` ``,
`sh -c '...'` and `trap '...'`, and the command after `exec`, `setsid`, `nohup`, `timeout`, `xargs`, `busybox` and the
like. Quoted text, comments, the words of `case` patterns, `for` lists and here documents are not. A path names the
applet of its last part (`/bin/telnetd`). A shell function of the same file shadows an applet of its name.

Python's standard library; reads files, touches no scope. Layer `text` of tools/check.sh.
"""
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
APPLETS = "rootfs/busybox.applets"
SCRIPT_DIRS = ("rootfs/initramfs", "rootfs/board")
RUST_DIRS = ("hal/src", "fpgad/src", "ui/src", "ui/system/src")

# words that start a command list, after which the next word is a command again
STARTERS = {"if", "then", "else", "elif", "do", "while", "until", "!", "{", "time"}
# words after which the next word is no command, until the end of the command
ENDERS = {"fi", "done", "esac", "}"}
# commands whose first word that is not an option is a command; the value is the number of words to pass over first
# (`timeout 5 cmd`), options and their values aside
WRAPPERS = {"exec": 0, "command": 0, "nohup": 0, "setsid": 0, "busybox": 0, "xargs": 0, "env": 0, "nice": 0,
            "ionice": 0, "chroot": 1, "timeout": 1, "taskset": 1, "chrt": 1, "cttyhack": 0}
# options of the wrappers that take a value as the next word
VALUED = {"-n", "-I", "-P", "-s", "-L", "-E", "-d", "-c", "-p", "-k", "-t", "-u"}
SHELLS = {"sh", "ash"}


class Shell:
    """The command words of a shell script: a small reader of the shell's syntax, enough for the scripts of the
    image. `calls` holds (line, word) for each command, `functions` the names of the functions the text defines."""

    def __init__(self, text, line=1):
        self.t = text
        self.i = 0
        self.base = line
        self.calls = []
        self.functions = set()
        self.run()

    def peek(self, k=0):
        j = self.i + k
        return self.t[j] if j < len(self.t) else ""

    def sub(self, text, line):
        s = Shell(text, line)
        self.calls += s.calls
        self.functions |= s.functions

    def matching(self, open_, close):
        """From just after `open_`, the index of its `close` (quotes and nesting followed)."""
        depth, j, t = 1, self.i, self.t
        while j < len(t):
            c = t[j]
            if c == "\\":
                j += 2
                continue
            if c == "'":
                j = t.find("'", j + 1)
                if j < 0:
                    return len(t)
            elif c == '"':
                j += 1
                while j < len(t) and t[j] != '"':
                    j += 2 if t[j] == "\\" else 1
            elif c == open_:
                depth += 1
            elif c == close:
                depth -= 1
                if depth == 0:
                    return j
            j += 1
        return len(t)

    def substitution(self):
        """At `$(`, `$((`, `${` or a backquote: read it, the commands inside to `calls`."""
        t, line = self.t, self.line_at(self.i)
        if t.startswith("$((", self.i):
            self.i += 3
            end = self.matching("(", ")")
            self.i = end + 2
        elif t.startswith("$(", self.i):
            self.i += 2
            end = self.matching("(", ")")
            self.sub(t[self.i:end], line)
            self.i = end + 1
        elif t.startswith("${", self.i):
            self.i += 2
            end = self.matching("{", "}")
            body = t[self.i:end]
            for m in re.finditer(r"\$\(", body):
                k = m.end()
                e = Shell.__new__(Shell)
                e.t, e.i = body, k
                e = e.matching("(", ")")
                self.sub(body[k:e], line)
            self.i = end + 1
        else:
            end = t.find("`", self.i + 1)
            end = len(t) if end < 0 else end
            self.sub(t[self.i + 1:end], line)
            self.i = end + 1

    def line_at(self, j):
        return self.base + self.t.count("\n", 0, j)

    def word(self):
        """One word: (its text with the quotes taken off, whether any of it was quoted, its line)."""
        t = self.t
        out, quoted, line = [], False, self.line_at(self.i)
        while self.i < len(t):
            c = t[self.i]
            if c in " \t\n;&|()<>":
                break
            if c == "\\":
                if self.peek(1) != "\n":
                    out.append(self.peek(1))
                self.i += 2
            elif c == "'":
                end = t.find("'", self.i + 1)
                end = len(t) if end < 0 else end
                out.append(t[self.i + 1:end])
                quoted = True
                self.i = end + 1
            elif c == '"':
                quoted = True
                self.i += 1
                while self.i < len(t) and t[self.i] != '"':
                    if t[self.i] == "\\":
                        out.append(self.peek(1))
                        self.i += 2
                    elif t[self.i] in "$`" and (t[self.i] == "`" or self.peek(1) in "({"):
                        self.substitution()
                        out.append("$")
                    else:
                        out.append(t[self.i])
                        self.i += 1
                self.i += 1
            elif c in "$`" and (c == "`" or self.peek(1) in "({"):
                self.substitution()
                out.append("$")
            else:
                out.append(c)
                self.i += 1
        return "".join(out), quoted, line

    def run(self):
        t = self.t
        cmd = True          # the next word is a command
        wrap = None         # [words to pass over] after a wrapper, until its command
        skip_value = False  # the next word is the value of a wrapper's option
        redirect = False    # the next word is the target of a redirection
        args = []           # the words of the current command after its name
        name = None
        heredocs = []
        case = []           # per open `case`: True while its patterns are read
        listing = False     # the words of `for x in ...`
        while self.i < len(t):
            c = t[self.i]
            if c == "\n":
                self.i += 1
                for delim in heredocs:
                    # the body: every line up to the delimiter, tabs before it allowed (`<<-`)
                    while self.i < len(t):
                        end = t.find("\n", self.i)
                        end = len(t) if end < 0 else end
                        ln = t[self.i:end]
                        self.i = end + 1
                        if ln.lstrip("\t") == delim:
                            break
                heredocs = []
                cmd, wrap, name, args, listing, redirect = True, None, None, [], False, False
                continue
            if c in " \t":
                self.i += 1
                continue
            if c == "#":
                end = t.find("\n", self.i)
                self.i = len(t) if end < 0 else end
                continue
            if c == "\\" and self.peek(1) == "\n":
                self.i += 2
                continue
            if t.startswith(";;", self.i):
                self.i += 2
                if case:
                    case[-1] = True
                cmd, wrap, name, args, listing = False, None, None, [], False
                continue
            if c in ";&|(":
                self.i += 1
                if c == "(" and name is not None and not args and not cmd:
                    # `name()`: a function
                    self.functions.add(name)
                cmd, wrap, name, args, listing, redirect = True, None, None, [], False, False
                continue
            if c == ")":
                self.i += 1
                if case and case[-1]:
                    case[-1] = False
                cmd, wrap, name, args, listing = True, None, None, [], False
                continue
            if c in "<>":
                if t.startswith("<<", self.i):
                    self.i += 2
                    if self.peek() == "-":
                        self.i += 1
                    while self.peek() in " \t":
                        self.i += 1
                    delim, _, _ = self.word()
                    heredocs.append(delim)
                    continue
                self.i += 1
                while self.peek() in "<>&|":
                    self.i += 1
                if self.peek().isdigit() or self.peek() == "-":
                    # `2>&1`, `>&-`
                    while self.peek().isdigit() or self.peek() == "-":
                        self.i += 1
                else:
                    redirect = True
                continue
            start = self.i
            w, quoted, line = self.word()
            if not w:
                self.i += 1 if self.i < len(t) and t[self.i] not in " \t\n;&|()<>" else 0
                continue
            if redirect:
                redirect = False
                continue
            if case and case[-1]:
                if w == "esac" and not quoted:
                    case.pop()
                continue
            if listing:
                if w == "do":
                    listing, cmd = False, True
                continue
            if args == [] and name is None and re.fullmatch(r"[0-9]+", w) and self.peek() in "<>":
                continue  # the file descriptor of a redirection
            if cmd:
                if not quoted and w in STARTERS:
                    continue
                if not quoted and w in ENDERS:
                    cmd = False
                    if w == "esac" and case:
                        case.pop()
                    continue
                if not quoted and w == "case":
                    case.append(False)
                    cmd = False
                    name = "case"
                    continue
                if not quoted and w == "for":
                    listing, cmd = True, False
                    continue
                if re.match(r"[A-Za-z_][A-Za-z0-9_]*=", t[start:]):
                    continue  # an assignment before the command
                if wrap is not None:
                    if skip_value:
                        skip_value = False
                        continue
                    if w.startswith("-") or (name == "env" and "=" in w):
                        skip_value = w in VALUED and name != "env"
                        continue
                    if wrap[0] > 0:
                        wrap[0] -= 1
                        continue
                    wrap = None
                cmd = False
                name, args = w, []
                if not w.startswith("$"):
                    self.calls.append((line, w))
                base = w.rsplit("/", 1)[-1]
                if base in WRAPPERS:
                    wrap, cmd = [WRAPPERS[base]], True
                continue
            if name == "case" and w == "in":
                case[-1] = True
                continue
            args.append(w)
            base = (name or "").rsplit("/", 1)[-1]
            # the text of `sh -c TEXT`, and the commands of `trap TEXT SIGNALS`
            if (base in SHELLS and len(args) >= 2 and args[-2] == "-c") or (base == "trap" and len(args) == 1):
                self.sub(w, line)


def applets(root):
    """(in the image, left out) from rootfs/busybox.applets."""
    keep, out = set(), set()
    with open(os.path.join(root, APPLETS), encoding="utf-8") as f:
        for ln in f:
            ln = ln.strip()
            if not ln or ln.startswith("#"):
                continue
            (out if ln.startswith("-") else keep).add(ln.lstrip("-"))
    return keep, out


def shell_files(root):
    for d in SCRIPT_DIRS:
        for dirpath, dirs, files in os.walk(os.path.join(root, d)):
            dirs.sort()
            for f in sorted(files):
                p = os.path.join(dirpath, f)
                try:
                    with open(p, encoding="utf-8") as fh:
                        first = fh.readline()
                        rest = fh.read()
                except (UnicodeDecodeError, OSError):
                    continue
                rel = os.path.relpath(p, root)
                if re.match(r"#!.*[/ ](ba|da|a)?sh( |$)", first):
                    yield rel, first + rest
                elif rel.endswith("etc/inittab"):
                    # id:runlevels:action:process; the process is a command line
                    yield rel, "\n".join(ln.split(":", 3)[3] if ln.count(":") >= 3 else "" for ln in
                                         (first + rest).split("\n"))


STRING = r'"((?:[^"\\]|\\.)*)"'


def unescape(s):
    return re.sub(r"\\\n\s*", "", s).replace('\\"', '"').replace("\\\\", "\\")


def rust_calls(text):
    """(line, word) of the programs a Rust file starts: the program of each `Command::new("x")`, and the commands of
    each command line given to `sh -c`: a string after `"-c"` (or the `const` named there), or a string passed to a
    closure of the file that runs `sh -c` (`let sh = |c: &str| Command::new("sh").arg("-c").arg(c)...`)."""
    out = []

    def at(m):
        return text.count("\n", 0, m.start()) + 1

    for m in re.finditer(r"Command::new\(\s*" + STRING, text):
        out.append((at(m), unescape(m.group(1))))
    consts = {m.group(1): unescape(m.group(2)) for m in
              re.finditer(r"const\s+(\w+)\s*:\s*&(?:'static\s+)?str\s*=\s*" + STRING, text)}
    for m in re.finditer(r'"-c"\s*\)?\s*[,.]\s*(?:arg\(\s*)?(?:' + STRING + r"|(\w+))", text):
        line = unescape(m.group(1)) if m.group(1) is not None else consts.get(m.group(2))
        if line is not None:
            out += Shell(line, at(m)).calls
    # a closure on one statement: `;` only at the end of a line ends it
    closure = r'let\s+(\w+)\s*=\s*\|[^|]*\|([^;]|;(?!\n))*?Command::new\(\s*"sh"\s*\)([^;]|;(?!\n))*?"-c"'
    closures = [m.group(1) for m in re.finditer(closure, text)]
    for name in closures:
        for m in re.finditer(r"\b" + name + r"\(\s*&?(?:format!\(\s*)?" + STRING, text):
            out += Shell(unescape(m.group(1)), at(m)).calls
    return out


def rust_files(root):
    for d in RUST_DIRS:
        for dirpath, dirs, files in os.walk(os.path.join(root, d)):
            dirs[:] = sorted(x for x in dirs if x != "tests")
            for f in sorted(files):
                if not f.endswith(".rs") or f == "tests.rs":
                    continue
                p = os.path.join(dirpath, f)
                with open(p, encoding="utf-8") as fh:
                    text = fh.read()
                if "Command::new" in text:
                    yield os.path.relpath(p, root), text


def calls(root):
    """(file, line, applet) of every call of a busybox applet, the image's or a left-out one."""
    keep, out = applets(root)
    names = keep | out
    found = []
    for rel, text in shell_files(root):
        s = Shell(text)
        for line, w in s.calls:
            a = w.rsplit("/", 1)[-1]
            if a in names and a not in s.functions:
                found.append((rel, line, a))
    for rel, text in rust_files(root):
        for line, w in rust_calls(text):
            a = w.rsplit("/", 1)[-1]
            if a in names:
                found.append((rel, line, a))
    return found


def main(argv, root=ROOT):
    keep, out = applets(root)
    found = calls(root)
    if "--list" in argv:
        first = {}
        for rel, line, a in found:
            first.setdefault(a, f"{rel}:{line}")
        for a in sorted(first):
            print(f"{a:20} {first[a]}{'' if a in keep else '  (left out)'}")
        return 0
    bad = [(rel, line, a) for rel, line, a in found if a not in keep]
    for rel, line, a in bad:
        print(f"{rel}:{line}: calls `{a}`, which the image's busybox leaves out ({APPLETS}, rootfs/busybox.config)")
    if bad:
        return 1
    print(f"busybox-applets: {len({a for _, _, a in found})} applets called, all in the image's busybox "
          f"({len(keep)} of {len(keep) + len(out)})")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
