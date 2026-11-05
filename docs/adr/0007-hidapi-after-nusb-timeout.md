# Use hidapi over hidraw for the transport

Status: superseded by ADR-0008

The raw `nusb`/libusb path detaches `usbhid` and writes 64-byte interrupt reports directly. On a Harmony 300 the first write times out and `CLEAR_HALT` returns `EPROTO`, leaving the interface driverless; the unit does not accept the raw path before the HID class setup the kernel performs. The reference implementation's measured path is hidapi over hidraw, so the transport now uses `hidapi`: the kernel HID driver owns the interface, a write is a 65-byte buffer with a leading zero report ID that hidapi strips, and a read accepts 64 or 65 bytes.

## Consequences

- The udev rule covers both the USB device and the `hidraw` node, and grants the `wheel` group access.
- The `nusb` dependency is removed.
- The file-protocol layer is shared through `ReportLink`/`FileProtocol`, so the command and sequence logic is unchanged.
