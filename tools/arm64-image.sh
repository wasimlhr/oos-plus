#!/bin/sh
# Copyright (C) 2026 Julian Decker
# SPDX-License-Identifier: GPL-2.0-or-later
# arm64-image.sh - an arm64 Docker image, whatever the host and whatever Docker's builder:
#
#   tools/arm64-image.sh TAG BASE COMMAND     -> TAG: BASE with COMMAND run in it (sh), for linux/arm64
#
# Nothing happens when TAG already is arm64. `docker build --platform linux/arm64` needs BuildKit. Docker's legacy
# builder (Docker without the buildx plugin, as Ubuntu's docker.io in WSL) ignores --platform for a base image it already
# has and builds an x86-64 image without a word; its programs would then end up in the image for the scope. So the
# result is checked, and when it is not arm64 the image is made again with `docker run --platform linux/arm64` and
# `docker commit`, which work with either builder (through QEMU on an x86-64 host).
set -eu
[ $# = 3 ] || { echo "usage: $0 TAG BASE COMMAND" >&2; exit 2; }
tag=$1
base=$2
cmd=$3
arch() { docker image inspect -f '{{.Architecture}}' "$1" 2>/dev/null || true; }
[ "$(arch "$tag")" = arm64 ] && exit 0
printf 'FROM %s\nRUN %s\n' "$base" "$cmd" | docker build --platform linux/arm64 -t "$tag" - >&2
[ "$(arch "$tag")" = arm64 ] && exit 0
echo "$tag: docker build made a $(arch "$tag") image, not arm64 (no BuildKit?): building it again with docker run and commit" >&2
docker rmi -f "$tag" >/dev/null 2>&1 || true
c="oos-arm64-image-$$"
docker rm -f "$c" >/dev/null 2>&1 || true
if ! docker run --platform linux/arm64 --name "$c" "$base" sh -euc "$cmd" >&2; then
	docker rm -f "$c" >/dev/null 2>&1 || true
	echo "$tag: building in an arm64 container failed" >&2
	exit 1
fi
docker commit "$c" "$tag" >/dev/null
docker rm "$c" >/dev/null
[ "$(arch "$tag")" = arm64 ] || { echo "$tag: is $(arch "$tag"), not arm64" >&2; exit 1; }
