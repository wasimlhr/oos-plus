# ADR 0080: The rear USB port is a USBTMC instrument for VISA, a FunctionFS function served by oos-ui, off by default

- Status: proposed
- Date: 2026-11-24
- Category: system (touches code: a transport of ADR 0022, the threads of ADR 0066)

## Context

Bench instruments with a USB-B port are USBTMC devices: VISA (NI, Keysight, pyvisa-py) lists them as
`USB::<vid>::<pid>::<serial>::INSTR` without any network, and many scripts and programs only know that form. ADR 0079
made the scope's rear USB-B port a configfs gadget for a network over the cable, with one list of functions so that a
FunctionFS function could join it. The kernel of the image has FunctionFS (`CONFIG_USB_CONFIGFS_F_FS`); it has no
USBTMC gadget function of its own. The scope already speaks USBTMC as a host, to a generator for the Bode plot
(`ui/system/src/usbtmc.rs`).

Facts this rests on, from the 6.12 kernel's `Documentation/usb/functionfs.rst`, `include/uapi/linux/usb/functionfs.h`,
`drivers/usb/gadget/function/f_fs.c` and `composite.c` (read, not yet verified on hardware): a FunctionFS function can
be bound only after its program wrote the descriptors and strings to `ep0`; closing all its files unbinds the whole
gadget; class requests to its interface and endpoints arrive on `ep0` as SETUP events, answered by a write (or stalled
by a read); the function's Microsoft OS descriptors are sent for the configuration linked under `os_desc`, and the
composite layer sends extended properties of the REG_SZ type but not REG_MULTI_SZ.

## Decision

**With `usb.tmc` on (System > Network > Remote services > USB instrument, off by default, ADR 0010), the gadget of
ADR 0079 gets a FunctionFS function `ffs.usbtmc` with one USBTMC/USB488 interface, and oos-ui is its program: a
transport of the one SCPI interpreter (ADR 0022) whose endpoints are served by threads of their own, so the frame loop
never waits for the computer (ADR 0066).**

- **A switch of its own**, next to USB network, independent of it: an instrument for VISA and a network adapter are
  different wishes, and a computer that only wants VISA gets no network adapter. The gadget is built when either is
  on. The cable reaches one computer, which someone plugged in at the bench (ADR 0079's treatment): no `net.allow`
  check, but the switch ADR 0025 asks of a service that can change the scope.
- **The interface:** class 0xFE, subclass 0x03, protocol 0x01; Bulk-OUT, Bulk-IN (64 bytes at full speed, 512 at high
  speed, no super speed: the port is USB 2.0) and Interrupt-IN (2 bytes, 1 ms). In both configurations of the gadget,
  after RNDIS in the first and ECM in the second, so it is there whichever configuration a computer takes; with the
  network off, one configuration with this interface alone.
- **USBTMC 1.0 and USB488 1.0**, the parts a VISA uses: DEV_DEP_MSG_OUT up to its EOM is one program message;
  REQUEST_DEV_DEP_MSG_IN gets DEV_DEP_MSG_IN with its bTag, at most its TransferSize (and at most 1 MiB, so a deep
  record or a screenshot of tens of MB goes in many transfers), EOM on the last; INITIATE_CLEAR, both aborts and their
  checks; GET_CAPABILITIES; READ_STATUS_BYTE with the byte on Interrupt-IN (MAV, and the error queue as SCPI's EAV).
  Not offered, and so stalled: TermChar, INDICATOR_PULSE, REN_CONTROL, GO_TO_LOCAL, LOCAL_LOCKOUT, TRIGGER, service
  requests. A transfer that would end on a packet boundary gets four zero bytes more, so it ends with a short packet.
- **Windows binds WinUSB by itself:** the function's MS OS 1.0 descriptors give the compatible ID `WINUSB` and
  `DeviceInterfaceGUID` (REG_SZ) `{D904622F-1F2F-469B-AE0F-FC6A5CD8F693}`, which never changes.
- **IDs:** with the function, the gadget takes vendor ID 0x1209 (pid.codes, for open source projects) and its test
  product ID 0x0001, written in one place (`usbtmc_pid` in `/bin/oos-gadget`); bcdDevice 0x0101 alone, 0x0102 with the
  network. The network alone keeps ADR 0079's 1d6b:0104. The serial number stays the gadget's (from the SD card's CID).
- **Bind order:** the script mounts the FunctionFS at `/dev/usbtmc-ffs` and builds the gadget; it binds when the
  descriptors are there, else oos-ui runs `oos-gadget bind` once it wrote them, also after oos-ui started again. A lock
  folder keeps the script's runs one at a time.
- **The code:** the protocol without endpoints (descriptor blob, control requests, bulk state with tags, chunking,
  clear and abort) in `oos_remote::usbtmc_device`, tested by feeding bytes; the files of the mount and the threads in
  its submodule `ffs`, plain reads and writes, no ioctl and no `unsafe`; the bulk headers in one place for host and
  device (`oos_system::usbtmc`). Threads: `usbtmc` writes the descriptors and reads the events of `ep0` (opened
  non-blocking, looked at every 10 ms, so it notices the switch), one thread blocks on each endpoint; the frame loop
  hands messages to the interpreter and the answers back without waiting (`Server::poll_async`).

## Alternatives

- **USBTMC always on with USB network:** a computer that wants only one of them would get both, and the network's
  RNDIS confuses Windows machines that only want VISA. Two switches cost one row.
- **A kernel gadget function for USBTMC:** there is none in mainline; writing one means a kernel patch to keep, while
  FunctionFS is built already and keeps the protocol in Rust next to the interpreter.
- **No OS descriptors, so NI-VISA's USBTMC driver binds on Windows:** then a Windows computer without NI-VISA or
  Keysight's IO libraries has no driver at all, and pyvisa-py and the desktop client could not reach the scope without
  Zadig or an `.inf`. WinUSB serves those without an install; NI-VISA can still be given the device by hand.
- **The Linux Foundation's IDs with USBTMC:** 1d6b's IDs are for Linux's own gadgets; a VISA resource string with them
  says nothing about the instrument, and a pid.codes ID can become OOS's own.
- **Blocking reads of `ep0` and of the endpoints from the frame loop, or AIO:** a blocking read stops the screen
  (ADR 0066), and FunctionFS's asynchronous I/O needs `io_submit` through `unsafe`; a transfer blocks the thread it is
  on, which is why each endpoint has its own.

## Consequences

- VISA, pyvisa-py and the desktop client reach the scope over a cable without any network, with the same commands as
  on port 5555. On Windows NI-VISA and Keysight VISA do not list it as `USB::...::INSTR` until their driver is chosen
  for it in the Device Manager; pyvisa-py (libusb over WinUSB) and the desktop client (nusb over WinUSB) need no
  install. Linux binds its `usbtmc` kernel driver (`/dev/usbtmcN`) or libusb takes the interface; macOS uses libusb or
  nusb.
- **The product ID must change before a release:** pid.codes' test PID is for private testing and must not be on
  devices that are distributed. The maintainer asks pid.codes for a PID; it goes into the one place in the script, and
  docs/usb-device.md and the wiki follow. A computer that saw the test PID then sees a new device.
- When oos-ui ends or starts again, the kernel unbinds the gadget, and the network over the cable goes with it for that
  moment; the next oos-ui binds it again. A change of either switch rebuilds the gadget (ADR 0079).
- A slow or absent host never holds the frame loop: a Bulk-IN write waits on its own thread until the host reads, a
  clear or an abort, or the unbind. A host that never reads Interrupt-IN leaves one notification waiting there.
- How it is known (ADR 0015): the protocol in host tests, fed byte by byte; a round trip through oos-ui's interpreter
  with the scope's own USBTMC host code; the script's start, bind, rebind and stop in an arm64 container with a folder
  for configfs. Not yet verified on hardware: FunctionFS taking the descriptors, the bind, the enumeration and binding
  on Windows, macOS and Linux, and VISA over the cable. Worth a second look after that run, when a real PID is there,
  or when SRQ (USB488 SR1) or TermChar is asked for.

## References

`ui/remote/src/usbtmc_device.rs`, `ui/remote/src/usbtmc_device/ffs.rs`, `ui/system/src/usbtmc.rs`,
`ui/system/src/gadget.rs`, `rootfs/initramfs/bin/oos-gadget`, `ui/src/main.rs`, `ui/src/ui/remote.rs`,
[usb-device.md](../../usb-device.md), [scpi.md](../../scpi.md); USBTMC 1.0 and USB488 1.0 (USB-IF); Linux 6.12
`Documentation/usb/functionfs.rst`, `include/uapi/linux/usb/functionfs.h`; https://pid.codes; ADR 0010, ADR 0022,
ADR 0025, ADR 0066, ADR 0079.
