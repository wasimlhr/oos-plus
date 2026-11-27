#!/bin/sh
# Copyright (C) 2026 OOS contributors
# SPDX-License-Identifier: GPL-2.0-or-later
# build-busybox.sh - the image's static busybox (ADR 0078), in Docker, into rootfs/out/. boot/build-dev-image.sh runs
# it when it is missing or out of date (deps.sh); the sims take the same file (tools/lib/sim.sh).
# The configuration is busybox's defconfig with the lines of rootfs/busybox.config over it: no telnet and no network
# server but udhcpd, the rest as the image had it. Stops when a line of busybox.config did not take (a typo, an option of another version),
# or when the applets differ from rootfs/busybox.applets, which tools/busybox-applets.py holds the image's scripts to.
# Writes busybox, busybox.COPYING and busybox.dotconfig (the whole configuration). Version and SHA-256:
# rootfs/deps.lock (the SHA-256 is the one busybox.net publishes next to the archive; Debian's .dsc of 1.37.0 names
# the same for its orig.tar.bz2).
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out="$here/out"
. "$here/deps.sh"
. "$here/../tools/lib/docker.sh"
ver=$(dep_ver busybox)
dep_fetch busybox "$out" >/dev/null
# arm64 containers: on an x86-64 host through QEMU (boot/build-dev-image.sh says how); the results belong to the caller
docker run --rm --platform linux/arm64 -e OUT_OWNER="$(id -u):$(id -g)" -e VER="$ver" -v "$out:/out" \
	-v "$here/busybox.config:/config:ro" -v "$here/busybox.applets:/applets:ro" "$OOS_ALPINE" sh -euc '
	apk add --no-cache build-base linux-headers >/dev/null
	cd /tmp && tar -xjf /out/busybox-$VER.tar.bz2 && cd busybox-$VER
	# the version line says the date of the source, not of the build: two builds of one tree are the same file
	export SOURCE_DATE_EPOCH=$(stat -c %Y Makefile)
	make defconfig >/tmp/config.log 2>&1 || { tail -30 /tmp/config.log; exit 1; }
	# each line of busybox.config replaces the option of defconfig; oldconfig then settles what depends on it
	grep "^CONFIG_" /config > /tmp/lines
	while IFS="=" read -r k v; do
		sed -i -e "/^$k=/d" -e "/^# $k is not set\$/d" .config
		if [ "$v" = n ]; then echo "# $k is not set"; else echo "$k=$v"; fi >> .config
	done < /tmp/lines
	yes "" | make oldconfig >>/tmp/config.log 2>&1 || { tail -30 /tmp/config.log; exit 1; }
	# a line took when .config has it after oldconfig (an option busybox does not know is dropped there)
	bad=$(while IFS="=" read -r k v; do
		if [ "$v" = n ]; then l="# $k is not set"; else l="$k=$v"; fi
		grep -qxF "$l" .config || echo "$k=$v"
	done < /tmp/lines; true)
	[ -z "$bad" ] || { echo "build-busybox.sh: these lines of rootfs/busybox.config did not take:" $bad >&2; exit 1; }
	make -j"$(nproc)" >/tmp/build.log 2>&1 ||
		{ grep -m 20 -A3 "error:" /tmp/build.log || tail -30 /tmp/build.log; exit 1; }
	# the applets: the built ones are the plain lines of busybox.applets, and all of the source the lines together
	./busybox --list | sort > /tmp/built
	grep -v -e "^#" -e "^$" -e "^-" /applets | sort > /tmp/listed
	find . -name "*.c" -exec cat {} + | grep -E "^//applet:" | grep -oE "APPLET[A-Z_]*\( *[^ ,()]+ *," |
		sed -E "s/^APPLET[A-Z_]*\( *//; s/ *,\$//" | sort -u > /tmp/known
	grep -v -e "^#" -e "^$" /applets | sed "s/^-//" | sort > /tmp/all
	if ! cmp -s /tmp/built /tmp/listed || ! cmp -s /tmp/known /tmp/all; then
		echo "build-busybox.sh: rootfs/busybox.applets differs from the build (+ built, - listed):" >&2
		{ comm -13 /tmp/listed /tmp/built | sed "s/^/+/"; comm -23 /tmp/listed /tmp/built | sed "s/^/-/"; } >&2
		echo "and from the applets of busybox $VER (+ in the source, - listed):" >&2
		{ comm -13 /tmp/all /tmp/known | sed "s/^/+/"; comm -23 /tmp/all /tmp/known | sed "s/^/-/"; } >&2
		exit 1
	fi
	cp busybox /out/busybox
	cp LICENSE /out/busybox.COPYING
	cp .config /out/busybox.dotconfig
	chown "$OUT_OWNER" /out/busybox /out/busybox.COPYING /out/busybox.dotconfig
'
dep_stamp "$out/busybox" "$here/build-busybox.sh" "$here/busybox.config" "$here/busybox.applets" busybox
file "$out/busybox" 2>/dev/null || true
ls -la "$out/busybox"
