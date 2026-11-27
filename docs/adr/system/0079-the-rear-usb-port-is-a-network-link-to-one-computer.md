# ADR 0079: The rear USB port is a network link to one computer, off by default

- Status: proposed
- Date: 2026-11-24
- Category: system

## Context

The scope's rear USB-B port is the device side of the RK3399's first USB controller (DWC3 at `fe800000`, `dr_mode =
"otg"` in the OOS device tree, high speed). Without a gadget it does nothing in OOS. A user without Ethernet at the
bench asked to reach the scope over that cable (proposed in #235, tried on a DHO804 with a configfs gadget: a serial
console and Ethernet over USB). The kernel of the image has configfs gadgets with RNDIS, ECM, ACM and FunctionFS
built in; NCM is not built. Windows has a driver of its own for RNDIS and binds it without an `.inf` when the device
answers Microsoft OS descriptors; macOS has a driver for CDC ECM and none for RNDIS; Linux has both. That the port is
in device mode at the start and enumerates is reported for a DHO804; on a DHO900 it is not yet verified on hardware.

## Decision

- **One gadget, built by one script.** `/bin/oos-gadget` builds it in configfs (`/sys/kernel/config/usb_gadget/oos`),
  binds it to the device controller it finds under `/sys/class/udc` and takes it down again (start, stop, apply,
  status). The functions come from one list in the script, each with its set-up before the bind and after it.
- **Network over the cable, two configurations.** Configuration 1 RNDIS, linked under `os_desc` with the Microsoft OS
  descriptors (`use` 1, vendor code 0xcd, `MSFT100`, compatible ID `RNDIS`, sub-compatible ID `5162001`), so Windows
  takes it and binds its own driver; configuration 2 CDC ECM for macOS and Linux. No NCM (not in the kernel).
- **One fixed address, DHCP for the computer, nothing routed.** Each function has its interface (`usb0`, `usb1`); the
  kernel has no bridge (`CONFIG_BRIDGE` off), so both get the same address, 192.168.235.1/32, with a route to the one
  address the computer gets on it: 192.168.235.2 over RNDIS, .3 over ECM. busybox `udhcpd` runs on these interfaces
  only and hands out that address with a /29 mask, no router and no DNS server. The prefix is 192.168.235.0/29 by
  default (a corner of the private range that home and office networks seldom use), and `usb.net.prefix` in the
  system settings names another /29.
- **No passthrough.** The scope is never a DHCP client on the cable and gets no way out through the computer: an
  internet connection of the scope (NTP, say) wants a decision of its own.
- **Off by default.** `usb.net` (System > Network > Remote services > USB network), read by oos-ui and by the script
  at the start; off until the gadget ran on a unit (ADR 0010).
- **The computer on the cable is let in like loopback.** The firewall accepts the USB prefix from the gadget's
  interfaces (`iifname "usb*"`), and its DHCP request, whatever `net.allow` says; oos-ui's second check lets the
  prefix in while the network over the cable is up. A USB cable reaches one computer, which someone plugged into the
  scope: that is access at the bench, as at the screen. Everything else stays as ADR 0025 says: the web remote, SCPI
  and VXI-11 are open on the cable as on a listed network, and SSH needs its switch and a non-empty `net.allow` as on
  Ethernet.
- **No console over USB.** The shell over the cable is SSH over this link: SSH stays the only shell (ADR 0054).

## Alternatives

- **A serial console (ACM) with a getty on `ttyGS0`**, as proposed in #235: either a root shell without a login, the
  telnet ADR 0054 removed in another form on a port that is reachable without opening the case, or a second login
  next to SSH with the same password. SSH over the USB link already gives an authenticated shell, with keys, `scp`
  and `sftp`.
- **Two subnets, one per interface** (as some boards do): the scope's address would depend on the computer's driver
  (RNDIS or ECM, and Linux takes either). One address on both interfaces is the same on every computer.
- **One interface for all, ECM or NCM only:** Windows has no ECM driver, NCM is not in the kernel and would need a
  kernel change first.
- **`passthrough`, the scope as a DHCP client of a computer that shares its connection** (#235): left out, see above.
- **The USB prefix only through `net.allow`:** a user who limited the LAN would have to know and add a prefix to use a
  cable at the bench; the cable gives nothing a person at the scope does not have already.

## Consequences

- A computer without Ethernet reaches every network service of the scope over one cable, at `192.168.235.1` and by
  its `.local` name (the mDNS responder answers on every interface with an address).
- More functions can join the same gadget later (a FunctionFS USBTMC function, so VISA finds the scope over USB, is the
  next one): one more line in the script's list, its set-up before the bind (FunctionFS mounted and its daemon's
  descriptors written) and after it. The gadget, the configurations and the OS descriptors stay as they are.
- The gadget takes the IDs of the Linux Foundation's multifunction composite gadget (1d6b:0104), as many open boards
  do. Windows keeps what it read of a device by its IDs: a computer that saw another gadget with these IDs without the
  OS descriptors may need the device removed in the Device Manager once.
- How it is known (ADR 0015): the script's network part (both interfaces with one address, the routes, the leases
  without router and DNS, the firewall's rules for the cable, the teardown) ran in an arm64 container with the image's
  busybox and `nft`, with veth pairs in place of the gadget's interfaces and a folder in place of configfs; the
  settings, the menu rows and the address check of oos-ui in host tests. The gadget itself, the enumeration on
  Windows, macOS and Linux and the DHCP lease over the cable are not yet verified on hardware.
- Worth a second look when a unit shows that macOS or Linux pick configuration 1, or when the kernel gets a bridge or
  NCM.

## References

`rootfs/initramfs/bin/oos-gadget`, `rootfs/initramfs/bin/oos-firewall`, `rootfs/initramfs/etc/rcS`,
`ui/system/src/gadget.rs`, `ui/system/src/netacl.rs`, `ui/system/src/sysconf.rs`, `ui/src/ui/remote.rs`,
`ui/src/main.rs`, [usb-device.md](../../usb-device.md), [network.md](../../network.md) 4 and 6, Linux
`Documentation/usb/gadget_configfs.rst` and `Documentation/usb/gadget-testing.rst`, ADR 0010, ADR 0025, ADR 0054,
issue #235.
