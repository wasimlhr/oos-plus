#!/usr/bin/env python3
# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
"""one-way.py: the ideas the four crates implement in more than one place (goal C1 of issue 120: one helper per
idea).

    tools/one-way.py [ROOT]            one line: the ideas implemented more than once, and the places too many
    tools/one-way.py --list [ROOT]     each idea with the files and functions that implement it

ROOT is the top of a tree (default: this tree). Read from it as it is: the Rust files below hal/src, dsp/src, ui/src,
ui/system/src, ui/remote/src, ui/model/src, ui/store/src and fpgad/src, without tests, as tools/duplicates.py reads
them (its functions are used here), and without comments.

The ideas are declared in IDEAS below: a key, what the idea is, and its rules. A rule is a pattern for the name of a
function (`Type::name` in an `impl`, `outer::name` in another function, as tools/code-metrics.py names them; None for
any name) and patterns for its code, all of which have to occur in it (none: the name alone). A function that matches
one rule implements the idea; of a function and one inside it that both match, the inner one is named. A place is a
file with at least one such function: the widths of one reader (`u16`, `u32` next to each other), or a helper and the
function that calls it in one file, are one implementation, not several. A file's functions are listed with it.

The measure is the places too many: for each idea its places but one, summed (goal: 0). It counts what merging is
left to do, so taking one of four CRC engines away shows; the summary also gives the ideas with more than one place.
Some ideas can not get down to one place as the crates are cut: oos-hal and oos-dsp must not name each other
(tools/layers.py), so an idea both need has a place in each until a crate below both holds it.

A report only: it exits 0 whatever it counts. Python's standard library; reads files, touches no scope.
"""
import argparse
import importlib.util
import os
import re
import sys

TOOLS = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(TOOLS)

# (key, what, rules); a rule is (pattern of the function's name or None, (patterns of its code, all of them)). The
# patterns are Python regular expressions, searched in the name and in the function's code without comments.
IDEAS = (
    ("crc", "a CRC register shifted bit by bit or by table",
     [(r"(?i)crc", (r"(<<|>>)\s*(1|8)\b",))]),
    ("hex-encode", "bytes as hex text, two digits each",
     [(None, (r"(\.map\(\|&?\w+\|\s*|push_str\(&|extend\()format!\(\"\{:02[xX]\}\"",))]),
    ("hex-decode", "two hex digits back to a byte",
     [(None, (r"to_digit\(16\)|from_str_radix\(&\w+\[[^\]]*\+\s*2\]",))]),
    ("hex-number", "a number written as `0x...` or decimal",
     [(None, (r"(strip_prefix|trim_start_matches)\(\"0[xX]\"\)",))]),
    ("finite-number", "a finite number from text (no nan, no inf)",
     [(None, (r"parse::<f(32|64)>\(\)\s*\.ok\(\)\s*\.filter\(\|\w+\|\s*\w+\.is_finite\(\)",))]),
    ("si-prefix", "a factor and its SI prefix letter, either way",
     [(r"(^|::)si_prefix$", ()),
      (None, (r"\(1e-?\d+,\s*\"(\\u\{b5\}|µ|[pnumkMGT])\"\)",)),
      (None, (r"Some\('[kKMG]'\)\s*=>\s*\(.*,\s*1e\d",))]),
    ("trim-zeros", "a decimal number without its trailing zeros",
     [(None, (r"trim_end_matches\('0'\)\s*\.trim_end_matches\('\.'\)",))]),
    ("key-value-lines", "the `key = value` lines of a text",
     [(None, (r"\.lines\(\)", r"split_once\('='\)"))]),
    ("key-value-pairs", "the `key=value; key=value` pairs of one line",
     [(None, (r"split\(';'\)", r"split_once\('='\)"))]),
    ("int-at", "a fixed-width integer read from bytes at an offset",
     [(r"(^|::)((u|i|f)(8|16|32|64)(_at|le|be)?|(le|be)(16|32|64)(_le)?|be|bytes_at)$",
       (r"from_(le|be)_bytes|array::from_fn",))]),
    ("civil-date", "the calendar date of a count of days since 1970",
     [(None, (r"719_?468",))]),
    ("unix-time", "a system time as seconds since 1970",
     [(None, (r"duration_since\((std::time::)?UNIX_EPOCH\)",))]),
    ("xorshift", "a xorshift random number generator",
     [(None, (r"\^=\s*[\w.]+\s*<<\s*13\b",))]),
    ("data-partition", "whether the data partition /oos is there",
     [(None, (r"Some\(\"/oos\"\)|\"/oos/setups\"",))]),
    ("atomic-write", "a file written to a new name and renamed over the old one",
     [(None, (r"fs::rename\(",))]),
    ("base64", "base64 (RFC 4648)",
     [(None, (r"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789\+/",))]),
    ("sha", "a SHA-1 or SHA-2 digest",
     [(r"(^|::)sha(1|224|256|384|512)$", ())]),
    ("json-escape", "a string escaped for JSON",
     [(None, (r"\\\\u\{:0?4[xX]\}",))]),
    ("xml-escape", "a string escaped for XML or HTML",
     [(None, (r"\"&amp;\"",))]),
)


def load(name):
    """tools/<name>.py as a module, by its path: `python3 -I` puts no folder of the tools on the path."""
    spec = importlib.util.spec_from_file_location(name.replace("-", "_"), os.path.join(TOOLS, name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


dup = load("duplicates")  # the reading of the tree and the tokens of Rust
met = load("code-metrics")  # the functions of a file, with their names


def without_comments(text):
    """The text with every comment blanked; strings stay, and so do the lines (a comment's line breaks are kept)."""
    out, pos, n = [], 0, len(text)
    while pos < n:
        m = dup.TOKEN.match(text, pos)
        kind = m.lastgroup if m.lastgroup != "hashes" else "raw"
        end = m.end()
        if kind == "block":
            depth, end = 1, m.end()
            while end < n and depth:
                if text.startswith("/*", end):
                    depth, end = depth + 1, end + 2
                elif text.startswith("*/", end):
                    depth, end = depth - 1, end + 2
                else:
                    end += 1
        elif kind == "raw":
            close = '"' + m.group("hashes")
            i = text.find(close, m.end())
            end = n if i < 0 else i + len(close)
        piece = text[pos:end]
        out.append(re.sub(r"[^\n]", " ", piece) if kind in ("line", "block") else piece)
        pos = end
    return "".join(out)


def compiled():
    """IDEAS with their patterns compiled."""
    return [(key, what, [(re.compile(name) if name else None, [re.compile(c) for c in code]) for name, code in rules])
            for key, what, rules in IDEAS]


def implementations(texts, ideas):
    """{key: {path: [(first line, name)]}} of the functions that implement each idea."""
    found = {key: {} for key, _, _ in ideas}
    for rel in sorted(texts):
        text = texts[rel]
        lines = without_comments(text).split("\n")
        spans = [(first, first + n - 1, name) for n, first, name in met.functions(text)]
        for key, _, rules in ideas:
            hits = []
            for first, last, name in spans:
                body = "\n".join(lines[first - 1:last])
                if any((not pn or pn.search(name)) and all(c.search(body) for c in code) for pn, code in rules):
                    hits.append((first, last, name))
            # of a function and one inside it that both match, the inner one
            inner = [h for h in hits if not any(o != h and h[0] <= o[0] and o[1] <= h[1] for o in hits)]
            if inner:
                found[key][rel] = sorted((first, name) for first, _, name in inner)
    return found


def count(found):
    """(ideas with more than one place, places too many) of implementations()."""
    many = [key for key, places in found.items() if len(places) > 1]
    return len(many), sum(len(places) - 1 for places in found.values() if places)


def main(argv):
    ap = argparse.ArgumentParser(description="ideas implemented in more than one place (a report, exit 0)")
    ap.add_argument("root", nargs="?", default=ROOT, help="top of the tree (default: this tree)")
    ap.add_argument("--list", action="store_true", help="each idea with the files and functions that implement it")
    args = ap.parse_args(argv)
    ideas = compiled()
    found = implementations(dup.read_tree(args.root), ideas)
    if args.list:
        for key, what, _ in ideas:
            places = found[key]
            print("%s: %d %s (%s)" % (key, len(places), "place" if len(places) == 1 else "places", what))
            for rel, fns in places.items():
                print("  %s: %s" % (rel, ", ".join("%s:%d" % (name, first) for first, name in fns)))
    many, surplus = count(found)
    print("one-way: %d places too many, %d of %d ideas implemented more than once (--list names them)"
          % (surplus, many, len(ideas)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
