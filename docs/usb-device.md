# The rear USB port as a network link

The USB-B port on the back is the device side of the first USB controller of the RK3399 (DWC3, `fe800000.usb`, OTG in
the OOS device tree, high speed). **System > Network > Remote services > USB network** (`usb.net`, off by default)
makes it a network adapter for the computer on the other end of the cable, and every service of
[network.md](network.md) 4 works over it as on Ethernet. The decisions are in ADR 0079.

**Verified on the unit** (DHO924S, 2026-11-24): with a Mac, the gadget at power-on and the network (a lease, the web
remote and SSH over it); with Windows 11, the network. **Not yet verified on hardware**: the network on Linux, the
switch toggled while a host is connected.

## The gadget

`/bin/oos-gadget` builds it: rcS starts the script after the firewall, oos-ui runs `oos-gadget apply` when the switch
changes; log `/tmp/oos-gadget.log`. It is built in configfs (`/sys/kernel/config/usb_gadget/oos`) and bound to the
device controller under `/sys/class/udc` (the script waits up to 10 s for it):

| | |
|---|---|
| IDs | 1d6b:0104 (Linux Foundation, multifunction composite gadget), bcdDevice 0x0100, class 0xef/0x02/0x01 (interface association) |
| Strings | `OOS`, `OOS oscilloscope (<name>)` with the scope's name ([network.md](network.md) 4.1), a serial number from the SD card's CID |
| Configuration 1 | RNDIS, for Windows: Microsoft OS descriptors (vendor code 0xcd, `MSFT100`, compatible ID `RNDIS`, sub-compatible ID `5162001`) make Windows bind its own RNDIS driver, no `.inf` |
| Configuration 2 | CDC ECM, for macOS and Linux. Linux with its RNDIS driver may take configuration 1; both work |
| MAC addresses | fixed per SD card, from the card's CID as eth0's ([network.md](network.md) 1), with another first byte for each side |

The functions come from one list in the script (`functions`), each type with its set-up before the bind (`make_<type>`)
and after it (`up_<type>`): another function joins the same gadget as one more line there.

## Addresses

The scope has 192.168.235.1 on the interface of each function (`usb0` RNDIS, `usb1` ECM: the kernel has no bridge, so
both carry the same address, as /32, with a route to the computer's one address). busybox `udhcpd`, on these
interfaces only, gives the computer 192.168.235.2 (RNDIS) or .3 (ECM) with the mask 255.255.255.248, for a day, with
**no router and no DNS server**: the computer keeps its own internet connection, and nothing is routed between the
cable and the scope's other networks.

**USB address** on the Remote services page shows the scope's address while the link is up, else what the script said
(`off: no USB device port`, `off: failed, see /tmp/oos-gadget.log`).

## Settings

Two keys of `/oos/system.conf` ([network.md](network.md) 5), read by oos-ui and by the script through `oos-conf`, the
image default `/etc/oos/system.conf` first ([network.md](network.md) 6):

| Key | Default | What |
|---|---|---|
| `usb.net` | `off` | the switch on the page; oos-ui writes it |
| `usb.net.prefix` | `192.168.235.0/29` | set by hand when that range is in use: a /29 network, e.g. `10.9.9.8/29` (the scope then has .9, the computer .10 or .11). A value that is not a /29 network keeps the default and is logged |

## Reaching the scope

`http://192.168.235.1:8080` for the web remote, `TCPIP::192.168.235.1::INSTR` for VISA, port 5555 for SCPI, and the
scope's `.local` name: the mDNS responder ([network.md](network.md) 4.1) answers on every interface that is up and has
an address, the USB ones too. SSH ([network.md](network.md) 7) is the shell over the cable: `ssh root@192.168.235.1`,
with `remote.ssh = on` and an address list in Allowed as on Ethernet (any list: the cable is let in besides it, so
`192.168.235.0/29` alone allows the cable and nothing else). There is no serial console on the port (ADR 0079).

## Firewall and `net.allow`

The computer on the cable is let in like loopback: the firewall accepts its DHCP request and its addresses of the USB
prefix from the gadget's interfaces (`iifname "usb*"`) whatever `net.allow` says, and oos-ui's second check
([network.md](network.md) 6) lets the prefix in while the link is up (`/tmp/oos-gadget.net`). The same prefix from
Ethernet or WLAN is not let in.

**Switched off**, or before the switch is on: no gadget, no interface, no DHCP server; the port is as before. Switching
off takes the gadget down at once: the computer sees the adapter go, and its connections end.

## How it is known

Verified on the host: in an arm64 container with the image's busybox and `nft`, veth pairs in place of the two
interfaces and a folder in place of configfs: the attributes and links the script writes, one address on both
interfaces, a lease of .2 and .3 without router and DNS, a service reached from both through the firewall with
`net.allow` set, another address on the cable refused, the teardown (DHCP stopped, the prefix file gone), a wrong
`usb.net.prefix` refused; the switch, the rows and the address check of oos-ui (`sysconf`, `gadget`, `ui::remote`
tests). Verified on the unit (DHO924S, #566): the gadget at power-on, a Mac's lease of .3 over ECM, the web remote and
SSH over the cable with `net.allow` set to another network; Windows 11 (a VM): the RNDIS adapter without an install,
a lease of .2, the web remote. Not yet verified on hardware: Linux, mDNS over the cable.
