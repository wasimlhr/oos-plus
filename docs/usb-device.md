# The rear USB port

The USB-B port on the back is the device side of the first USB controller of the RK3399 (DWC3, `fe800000.usb`, OTG in
the OOS device tree, high speed). Two switches of **System > Network > Remote services** make it a device for the
computer on the other end of the cable, each on its own or both together:

- **USB network** (`usb.net`, off by default) makes it a network adapter, and every service of
  [network.md](network.md) 4 works over it as on Ethernet. The decisions are in ADR 0079.
- **USB instrument** (`usb.tmc`, off by default) makes it a USBTMC instrument (USB488) for VISA,
  `USB::0x1209::0x0001::<serial>::INSTR`, with the SCPI interpreter of [scpi.md](scpi.md) and no network
  ([below](#the-usb-instrument-usbtmc)). The decisions are in ADR 0080.

**Verified on the unit** (DHO924S, 2026-11-24): with a Mac, the gadget at power-on, the network (a lease, the web
remote and SSH over it) and the instrument (pyvisa-py: `*IDN?`, sets, a PNG, a RAW record); with Linux, the
instrument through the kernel's `usbtmc` driver; with Windows 11, the network alone and the instrument alone. **On
Windows the two do not go together:** with both switches on, Windows sees only the network (below, and #570). **Not
yet verified on hardware**: NI-VISA, the network on Linux, answers over several transfers on Windows (#571), the
switches toggled while a host is connected.

## The gadget

`/bin/oos-gadget` builds it: rcS starts the script after the firewall, oos-ui runs `oos-gadget apply` when a switch
changes; log `/tmp/oos-gadget.log`. It is built in configfs (`/sys/kernel/config/usb_gadget/oos`) and bound to the
device controller under `/sys/class/udc` (the script waits up to 10 s for it). One run of the script at a time (the
lock folder `/tmp/oos-gadget.lock`):

| | |
|---|---|
| IDs, network alone | 1d6b:0104 (Linux Foundation, multifunction composite gadget), bcdDevice 0x0100, class 0xef/0x02/0x01 (interface association) |
| IDs with the USB instrument | 1209:0001 ([pid.codes](https://pid.codes), its test product ID, below), bcdDevice 0x0101 alone and 0x0102 with the network; class 0xef/0x02/0x01 with the network, else 0 (the class is the interface's) |
| Strings | `OOS`, `OOS oscilloscope (<name>)` with the scope's name ([network.md](network.md) 4.1), a serial number from the SD card's CID |
| Configuration 1 | RNDIS, for Windows: Microsoft OS descriptors (vendor code 0xcd, `MSFT100`, compatible ID `RNDIS`, sub-compatible ID `5162001`) make Windows bind its own RNDIS driver, no `.inf`. The USBTMC interface after it. The instrument alone: only this configuration, named `USBTMC` |
| Configuration 2 | CDC ECM, for macOS and Linux, and the USBTMC interface after it. Linux with its RNDIS driver may take configuration 1; both work |
| MAC addresses | fixed per SD card, from the card's CID as eth0's ([network.md](network.md) 1), with another first byte for each side |

The functions come from one list in the script (`functions`), each type with its set-up before the bind (`make_<type>`)
and after it (`up_<type>`).

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

## The USB instrument (USBTMC)

With `usb.tmc` on, the gadget has a FunctionFS function `ffs.usbtmc`, mounted at `/dev/usbtmc-ffs`, and oos-ui is its
program (`ui/remote/src/usbtmc_device.rs`). One interface, class 0xFE, subclass 0x03, protocol 0x01 (USBTMC 1.0 with
USB488 1.0), string `OOS USBTMC`, three endpoints:

| Endpoint | Full speed | High speed | What |
|---|---|---|---|
| Bulk-OUT | 64 | 512 | DEV_DEP_MSG_OUT (commands), REQUEST_DEV_DEP_MSG_IN (a request for the answer) |
| Bulk-IN | 64 | 512 | DEV_DEP_MSG_IN (the answer) |
| Interrupt-IN | 2, every 1 ms | 2, every 8 microframes | the status byte after READ_STATUS_BYTE |

**Start and bind.** A FunctionFS function can only be bound once its program wrote the descriptors to `ep0`. The
script mounts the FunctionFS and builds the gadget; when oos-ui's descriptors are there already it binds it, else it
leaves it built (state `waiting for oos-ui (USB instrument)`) and oos-ui runs `oos-gadget bind` after writing them.
When oos-ui ends (or starts again after an update), the kernel unbinds the whole gadget as its files close, and the
network over the cable goes with it for that moment; the next oos-ui writes the descriptors again and binds it. When a
switch changes, `oos-gadget apply` unbinds first, which ends oos-ui's transfers, waits until oos-ui closed the
function's files (up to 10 s), unmounts and builds again.

**Messages.** A DEV_DEP_MSG_OUT message up to the transfer with EOM is one program message of the interpreter, as one
line on port 5555 (several commands with `;` or newlines; at most 4 MiB, a longer one halts Bulk-OUT and is dropped).
Its reply, with a newline, is returned to the REQUEST_DEV_DEP_MSG_IN transfers that follow: each DEV_DEP_MSG_IN has the
request's bTag and at most its TransferSize of data (and at most 1 MiB), EOM on the one that ends the reply. A message
without a query has no reply. The next message runs once the reply before it is out, and while a `*OPC?`
waits the messages after it wait too, as on port 5555. Transfers are padded to a multiple of 4; one that would end on a packet boundary gets
four zero bytes more, so it ends with a short packet. A header whose bTag and inverse do not match, a bTag of 0 or an
unknown MsgID halts Bulk-OUT; the next header starts a new transfer. TRIGGER and vendor-specific OUT messages are taken
and ignored.

**Control requests** (GET_CAPABILITIES says which):

| Request | Answer |
|---|---|
| GET_CAPABILITIES | USBTMC 1.00 and USB488 1.00; no INDICATOR_PULSE, no TermChar (a request's TermChar is ignored), not talk-only or listen-only; a 488.2 interface that speaks SCPI; no REN_CONTROL, GO_TO_LOCAL, LOCAL_LOCKOUT, TRIGGER, service request (SR0), remote/local (RL0) or device trigger (DT0) |
| INITIATE_CLEAR, CHECK_CLEAR_STATUS | drops what came in and what was to go out; a `*OPC?` that waits is dropped too. Pending (with bmClear bit 0) while a Bulk-IN transfer is still written |
| INITIATE_ABORT_BULK_OUT, CHECK_... | the transfer being received and its message dropped (SUCCESS for its bTag, TRANSFER_NOT_IN_PROGRESS for another, FAILED when none is); the check gives the bytes received |
| INITIATE_ABORT_BULK_IN, CHECK_... | the transfer being written ends with a short packet, the rest of its reply is dropped; the check is pending (bmAbortBulkIn bit 0) until the transfer ended, then gives the bytes sent. A request still waiting for its reply: FAILED, and the request is dropped |
| READ_STATUS_BYTE | SUCCESS with the bTag in the answer, the status byte on Interrupt-IN (`0x80` or the bTag, then the byte): MAV (bit 4) while a reply waits, the error queue not empty (bit 2) as of the last pass of the frame loop. No service request is sent by itself |
| the others (INDICATOR_PULSE, REN_CONTROL, GO_TO_LOCAL, LOCAL_LOCKOUT, vendor requests) | stalled |

**Windows.** The function's Microsoft OS descriptors give the interface the compatible ID `WINUSB` and the property
`DeviceInterfaceGUID` = `{D904622F-1F2F-469B-AE0F-FC6A5CD8F693}` (REG_SZ; Linux's composite gadget does not send the
REG_MULTI_SZ form). Windows takes configuration 1, the one the OS descriptors are linked to, and binds WinUSB to the
interface without an install: pyvisa-py over libusb and the desktop client (nusb) reach it. NI-VISA and Keysight VISA
have their own USBTMC driver, which Windows then does not pick by itself; the Device Manager can assign it by hand.

**Windows with both switches on (#570):** Windows takes the device as one function, the RNDIS network of
configuration 1, and the instrument does not appear (verified on the unit). The reason is **assumed** from Microsoft's
documentation of its composite driver, which it loads only for a device with a single configuration; with the network
on the gadget has two (RNDIS for Windows, ECM for macOS and Linux). On Windows, switch USB network off to use the
instrument. macOS takes configuration 2 and gets both (verified on the unit); Linux is expected to do the same, **not
yet verified on hardware**. A single configuration with CDC NCM in place of RNDIS and ECM would give Windows 10 and
11 both; that is open in #570.
**Linux** binds its `usbtmc` driver (`/dev/usbtmcN`, `CONFIG_USB_TMC` on the computer) or libusb takes the interface;
**macOS** has no USBTMC driver, libusb and nusb take it.

**The product ID** 0x0001 is pid.codes' "Test PID", which is for private testing only and must not be on a device that
is distributed. Before a release carries this function the maintainer asks pid.codes for a product ID of OOS's own; it
goes into `usbtmc_pid` at the top of `/bin/oos-gadget`, the one place it is written (docs and the wiki name it as an
example).

**On the menu**, the row **USBTMC** next to the switch says `starting` (no FunctionFS yet, or its descriptors not
taken), `no computer` (written, no computer configured the device), `connected`, or `off`.

## How it is known

The USB instrument: the protocol in host tests (`usbtmc_device::tests`: the descriptor blob as FunctionFS reads it,
GET_CAPABILITIES, an answer over several requests smaller than it, a wrong bTag, aborts and a clear in the middle,
messages larger than a packet in reads of any size, EOM, READ_STATUS_BYTE), and a round trip through oos-ui's
interpreter with the scope's own USBTMC host code (`scpi::tests::transports`); the script's start, bind, rebind and
stop with the instrument, the IDs and the lock in an arm64 container with busybox, a folder in place of configfs and
stand-ins for `mount` and `ip`. Verified on the unit (DHO924S, macOS host, pyvisa-py): FunctionFS took the
descriptors and the gadget bound at power-on with both functions; macOS took configuration 2; GET_CAPABILITIES and a
clear answered; `*IDN?`, a set and its query, a PNG of 91 KB in 0.05 s and a RAW record of 3.75 MB at 14.4 MB/s, a
query after a read that timed out. That run found that a read of the header alone lost the message in the same
packet; Bulk-OUT is read in whole packets since. Verified on the unit with Linux (an Alpine VM with the LTS kernel,
the device passed through): the `usbtmc` driver binds, `*IDN?` through `/dev/usbtmc0`, READ_STATUS_BYTE with MAV and
EAV, USBTMC_IOCTL_CLEAR in the middle of a BMP of 1.84 MB and the whole BMP after it. With Windows 11 (a VM, the
device passed through): WinUSB bound without a prompt, `*IDN?`, a set and its query, INITIATE_CLEAR with Bulk-IN read
empty; an answer over several transfers stopped after the first (#571, not yet known whether the device or the VM's
passthrough). Not yet verified on hardware: NI-VISA, the aborts from a real host, the switches toggled while a host
is connected.

The network: verified on the host: in an arm64 container with the image's busybox and `nft`, veth pairs in place of the two
interfaces and a folder in place of configfs: the attributes and links the script writes, one address on both
interfaces, a lease of .2 and .3 without router and DNS, a service reached from both through the firewall with
`net.allow` set, another address on the cable refused, the teardown (DHCP stopped, the prefix file gone), a wrong
`usb.net.prefix` refused; the switch, the rows and the address check of oos-ui (`sysconf`, `gadget`, `ui::remote`
tests). Verified on the unit (DHO924S, #566): the gadget at power-on, a Mac's lease of .3 over ECM, the web remote and
SSH over the cable with `net.allow` set to another network; Windows 11 (a VM): the RNDIS adapter without an install,
a lease of .2, the web remote. Not yet verified on hardware: Linux, mDNS over the cable.
