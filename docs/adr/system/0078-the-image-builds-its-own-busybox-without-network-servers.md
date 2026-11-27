# ADR 0078: The image builds its own busybox, without telnet and the network servers it does not run

- Status: proposed
- Date: 2026-11-24
- Category: system (touches process: ADR 0013, ADR 0045)

Amends ADR 0054 (its consequence "the busybox of the image still holds the `telnetd` applet").

## Context

ADR 0054 took telnet away: nothing starts `telnetd`, and the firewall and `net.allow` limit what reaches the scope.
The applet itself stayed, because the image's busybox was Ubuntu's `busybox-static` package, copied out of the arm64
build container. That package also holds `httpd`, the telnet client and `nc` with a listen mode (`nc -l -e`), none of
which the image uses; a root shell, or a script that a user runs, can start any of them. Taking them out needs a
busybox built from source with a configuration of the tree (issue #331). The other programs of the image (Dropbear,
nft, e2fsck, kexec, wpa_supplicant, BlueZ) are already built that way: from a pinned archive of `rootfs/deps.lock`, in
an arm64 Alpine container, static with musl.

## Decision

The image's busybox is built by `rootfs/build-busybox.sh` from the busybox archive of `rootfs/deps.lock`, with
busybox's defconfig and the lines of `rootfs/busybox.config` over it: no `telnetd`, no telnet client, no `httpd`, no
`nc -l`, and none of the servers Ubuntu's package already left out (ftpd, tftpd, inetd, dnsd, tcpsvd, udpsvd,
fakeidentd, ntpd, dhcprelay, lpd). Everything else keeps the choices of the package the image had, and
`rootfs/busybox.applets` lists the applets that come out. `tools/busybox-applets.py` (layer `text`) fails when a
script or a program of the image calls an applet that list leaves out.

## Alternatives

- **Keep Ubuntu's package and delete the applets' links:** the code stays in the binary, and `busybox telnetd`
  still runs it.
- **busybox's defconfig with the servers taken out:** fewer lines to keep, but some 130 applets more than before, and
  other behaviour: the small `modprobe` without `modules.dep`, the shell no longer running its applets before
  `PATH`. Every script of the image would have to be tried on a unit again for nothing it needs.
- **The configuration as a whole `.config`:** about 1 200 lines in which the few that matter are hard to see;
  defconfig and some 230 lines, in groups with their reason, say the same.
- **busybox 1.38.0:** busybox calls it unstable, and a newer version is a change of its own; 1.37.0 is the version
  the image had.

## Consequences

- `busybox --list` of the image has 271 applets: Ubuntu's 276 without `telnetd`, `telnet`, `httpd`, `tc` (it does
  not build against Alpine's kernel headers, and nothing calls it) and `static-sh` (an alias of Debian's patches).
  `nc` connects but no longer listens. `udhcpd` stays: it answers only on the interfaces it is started for, and the
  network over the USB device port (issue #235) needs it, as it needs `getty` and `login`.
- The C library is musl instead of glibc, static as before. A release carries `busybox-1.37.0.tar.bz2` and, with the
  tree at its tag, the configuration and the build script, instead of Ubuntu's source packages of busybox and of the
  glibc it was built with (`tools/release-sources.sh`, THIRD-PARTY-LICENSES.md). The licence text is in
  `/etc/licenses/busybox`.
- A change of busybox's version or configuration is a pull request with its build: `rootfs/build-busybox.sh` stops
  when a line of `busybox.config` did not take or when the applets differ from `busybox.applets`, so the list in the
  tree is always the one of the image. The image's build container no longer installs `busybox-static`, and the sims
  take the same binary (`tools/lib/sim.sh`).
- How it is known (ADR 0015): the build, the list of applets and that the applets the scripts call are there are
  checked on the host (arm64 container, 2026-11-24); built either way, defconfig with `busybox.config` and the whole
  configuration of Ubuntu's package with the same changes give the same binary. A start of the image, its network
  and its scripts with this busybox are not yet verified on hardware.

## References

`rootfs/build-busybox.sh`, `rootfs/busybox.config`, `rootfs/busybox.applets`, `rootfs/deps.lock`,
`tools/busybox-applets.py`, `boot/build-dev-image.sh`, `tools/release-sources.sh`,
[THIRD-PARTY-LICENSES.md](../../../THIRD-PARTY-LICENSES.md), [deps.md](../../fork/deps.md), ADR 0054, issue #331.
