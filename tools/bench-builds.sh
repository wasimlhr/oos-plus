#!/bin/sh
# Copyright (C) 2026 dekajoule
# SPDX-License-Identifier: GPL-2.0-or-later
# bench-builds.sh - the waveform performance of two builds on the same unit, side by side (device-bench.py).
#
#   tools/bench-builds.sh <ip> <ref-a> <ref-b> [device-bench options]
#
# A ref is a commit, branch or tag of this repository, or `installed` (the oos-ui on the card, nothing deployed). Each
# other ref is built in a worktree of its own under target/bench-builds/ (oos-ui and oos-fpgad for the scope, into one
# shared target directory), run from RAM with this tree's tools/device-deploy.sh (`OOS_DEPLOY_UI`, `OOS_DEPLOY_FPGAD`;
# the plan matrix it uploads is this tree's), measured, and the installed oos-ui is back after it (`--restore`). The
# results go to target/bench-builds/<ref>-<commit>.json and the table of the two to stdout. The SSH key of
# device-deploy.sh comes from OOS_SSH_IDENTITY. It writes /tmp on the scope and its settings, nothing on the card.
#
#   tools/bench-builds.sh 192.0.2.10 installed HEAD --runs 3
#   tools/bench-builds.sh 192.0.2.10 origin/main my-branch --views fork
set -eu
[ $# -ge 3 ] || { sed -n '4,16p' "$0" | sed 's/^# \{0,1\}//'; exit 2; }
ip=$1 a=$2 b=$3
shift 3
# each build's label and file are this script's
for opt in "$@"; do
	case $opt in --label* | --out*) echo "bench-builds.sh: $opt is set per build" >&2; exit 2 ;; esac
done
top=$(git rev-parse --show-toplevel)
out=$top/target/bench-builds
mkdir -p "$out"
target=aarch64-unknown-linux-musl

# name REF: its file name, the ref and its commit (`foo/bar` and `foo_bar` stay apart)
name() {
	if [ "$1" = installed ]; then
		echo installed
	else
		echo "$(printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_')-$(git -C "$top" rev-parse --short "$1^{commit}")"
	fi
}

# bench REF: deploy REF (unless `installed`), measure, restore; the results in $out/<name>.json
bench() {
	ref=$1
	shift
	name=$(name "$ref")
	if [ "$ref" = installed ]; then
		python3 "$top/tools/device-bench.py" --host "$ip" --label installed --out "$out/$name.json" "$@"
		return
	fi
	wt=$out/tree-$name
	rm -rf "$wt"
	git -C "$top" worktree add -q --detach "$wt" "$ref"
	trap 'git -C "$top" worktree remove --force "$wt" 2>/dev/null || true' EXIT
	# one target directory for every build: a second ref compiles only what differs
	cargo_dir=$out/cargo
	(cd "$wt/ui" && CARGO_TARGET_DIR=$cargo_dir cargo build --release --locked --target $target -q)
	(cd "$wt/fpgad" && CARGO_TARGET_DIR=$cargo_dir cargo build --release --locked --target $target -q)
	# from the deploy on, any way out restores the installed oos-ui (device-deploy.sh stops it before the upload); the
	# trap goes only once the restore below has run
	trap '"$top/tools/device-deploy.sh" "$ip" --restore
		git -C "$top" worktree remove --force "$wt" 2>/dev/null || true' EXIT
	(OOS_DEPLOY_UI=$cargo_dir/$target/release/oos-ui \
		OOS_DEPLOY_FPGAD=$cargo_dir/$target/release/oos-fpgad "$top/tools/device-deploy.sh" "$ip")
	sleep 8
	status=0
	python3 "$top/tools/device-bench.py" --host "$ip" --label "$ref" --out "$out/$name.json" "$@" || status=$?
	"$top/tools/device-deploy.sh" "$ip" --restore
	git -C "$top" worktree remove --force "$wt"
	trap - EXIT
	return $status
}

bench "$a" "$@"
bench "$b" "$@"
na=$(name "$a")
nb=$(name "$b")
python3 "$top/tools/device-bench.py" --compare "$out/$na.json" "$out/$nb.json"
