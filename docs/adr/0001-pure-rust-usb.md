# Use nusb for USB access

The Harmony 300 exposes vendor-defined HID over 64-byte interrupt reports. We use `nusb` rather than `rusb` (libusb) or `hidapi`: it is pure Rust, provides blocking calls with read timeouts, exposes endpoint discovery and kernel-driver detach, and adds no C dependency. `hidapi` was rejected because its report-ID handling can wedge the OUT endpoint on a 63-byte packet; `rusb` was rejected because libusb adds a system dependency without a capability we need.

## Considered Options

- `nusb`: pure Rust, blocking `.wait()`, explicit endpoints, timeouts.
- `rusb`: libusb binding, matches the original assumption, C dependency.
- `hidapi`: closest to the reference implementation, report-ID footgun.

## Consequences

Linux udev permissions are still required. `nusb`'s async-shaped API is used in blocking mode only.
