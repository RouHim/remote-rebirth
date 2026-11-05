# Use hidapi's libusb backend when usbhid refuses the device

The hidraw backend needs the kernel `usbhid` driver. On this Harmony 300, after the raw `nusb` claim was
cleared by a full controller reset, `usbhid`'s bind still returned `ENODEV` even though the HID report
descriptor was readable and both interrupt endpoints worked. hidapi's libusb backend opened interface 0
directly (path `1-2:1.0`), wrote 65-byte buffers with a leading zero report ID, read 64-byte replies, and
completed `detect`, `backup`, committed `write` with Read-Back, and `restore`.

## Consequences

- The build links libusb and the udev rule must cover the USB device node; the rule's hidraw line is
  harmless but no hidraw node is needed.
- The first request after opening the USB device can be answered late or not at all. The session sends a
  bare ping, drains stale reports, and matches replies by command and sequence rather than trusting the
  first report.
- Reads use a 40 s timeout because the open for writing erases six 64 KiB blocks before it answers.
