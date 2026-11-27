#!/bin/sh
# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
# upstream-sync.sh - oos-plus: what upstream OOS has that the fork lacks, and the start of its merge
# (docs/fork/upstream-sync.md). Touches no scope; `start` changes the branch it is run on.
#
#   tools/upstream-sync.sh status   the commits of upstream's main after the last sync, oldest last, and a last line
#                                   "behind N"; exit 0 with nothing new, 3 with commits to merge
#   tools/upstream-sync.sh start    on a clean branch made from the fork's main: records the last sync as merged,
#                                   merges upstream's main without committing and writes the new base. Exit 0 when
#                                   git merged every file, 4 when conflicts are left; either way the merge is open,
#                                   to be finished with `git commit -s` after the checklist
#
# A sync lands in the fork's main squashed, so git does not know afterwards that upstream's commits are in, and a
# plain merge would bring all of them again as conflicts. tools/upstream-base.txt holds the commit of upstream that
# the last sync reached; `start` first makes a merge of that commit with the strategy `ours`, which changes no file
# and only records the ancestry, and then the real merge brings what is new alone.
set -eu
cd "$(dirname "$0")/.."

url=${OOS_UPSTREAM_URL:-https://github.com/dekajoule/oos}
base=$(cat tools/upstream-base.txt)

fail() {
    echo "upstream-sync.sh: $*" >&2
    exit 1
}

git fetch -q "$url" main || fail "upstream's main could not be fetched from $url"
tip=$(git rev-parse FETCH_HEAD)
git cat-file -e "$base^{commit}" 2>/dev/null || fail "the base $base of tools/upstream-base.txt is no commit here"
git merge-base --is-ancestor "$base" "$tip" || fail "the base $base is not in upstream's main: it was rewritten"

case ${1:-} in
status)
    git log --format='%h %ad %s' --date=short "$base..$tip"
    n=$(git rev-list --count "$base..$tip")
    echo "behind $n"
    [ "$n" -eq 0 ] || exit 3
    ;;
start)
    [ "$base" != "$tip" ] || fail "nothing to merge: upstream's main is the base $base"
    [ -z "$(git status --porcelain)" ] || fail "the work tree has changes: start on a clean branch made from main"
    tree=$(git rev-parse 'HEAD^{tree}')
    git merge -q --signoff -s ours --no-edit \
        -m "Sync: record dekajoule/oos up to $(git rev-parse --short "$base") as merged" "$base"
    [ "$(git rev-parse 'HEAD^{tree}')" = "$tree" ] || fail "the record of the last sync changed files"
    rc=0
    git merge --no-commit --no-ff "$tip" || rc=4
    echo "$tip" > tools/upstream-base.txt
    git add tools/upstream-base.txt
    echo "merging $(git rev-list --count "$base..$tip") commits of upstream up to $(git rev-parse --short "$tip")"
    exit $rc
    ;;
*)
    sed -n '4,13p' "$0" >&2
    exit 2
    ;;
esac
