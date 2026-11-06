//! The libusb HID transport: hidapi claims interface 0 directly and moves 64-byte reports.
//!
//! The kernel `usbhid` driver refuses to bind this unit after the raw claim (`ENODEV`), so the
//! hidraw backend cannot be used. hidapi's libusb backend does not need `usbhid`: it opens the
//! interface, writes a 65-byte buffer with a leading zero report ID, and reads the 64-byte reply.

use std::fs;

use hidapi::{HidApi, HidDevice};

use crate::error::Error;
use crate::protocol::{FileProtocol, REPORT_BYTES, ReportLink};
use crate::transport::{RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession};

const READ_TIMEOUT_MS: i32 = 40_000;

pub struct HidBackend;

impl HidBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for HidBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteBackend for HidBackend {
    fn list(&mut self) -> Result<Vec<RemoteDescriptor>, Error> {
        let api = HidApi::new()
            .map_err(|error| Error::Device(format!("HID enumeration failed: {error}")))?;
        let mut descriptors = Vec::new();
        for (index, info) in api.device_list().enumerate() {
            let path = info.path().to_string_lossy().into_owned();
            let fallback = u8::try_from(index + 1).unwrap_or(u8::MAX);
            let (bus, address) = location(&path, fallback);
            descriptors.push(RemoteDescriptor {
                bus,
                address,
                vendor_id: info.vendor_id(),
                product_id: info.product_id(),
                bcd_device: info.release_number(),
                product: info.product_string().map(str::to_owned),
                serial: info.serial_number().map(str::to_owned),
                path: Some(path),
            });
        }
        Ok(descriptors)
    }

    fn open(&mut self, selector: &RemoteSelector) -> Result<Box<dyn RemoteSession>, Error> {
        let api = HidApi::new()
            .map_err(|error| Error::Device(format!("HID enumeration failed: {error}")))?;
        let info = api
            .device_list()
            .find(|info| {
                selector
                    .path
                    .as_deref()
                    .is_some_and(|path| info.path().to_string_lossy().as_ref() == path)
            })
            .ok_or_else(|| {
                Error::Device(format!(
                    "the HID device at bus {} address {} is no longer connected",
                    selector.bus, selector.address
                ))
            })?;
        let device = info
            .open_device(&api)
            .map_err(|error| Error::Device(format!("cannot open HID device: {error}")))?;
        let mut link = HidLink { device };
        link.drain();
        Ok(Box::new(FileProtocol::new(link)))
    }
}

/// The sysfs name of the USB device a hidapi-libusb path names, e.g. `1-2` from `1-2:1.0`.
pub(crate) fn sysfs_device(path: &str) -> Option<&str> {
    let device = path.split(':').next()?;
    (device.contains('-') && !device.starts_with('/')).then_some(device)
}

fn location(path: &str, fallback: u8) -> (u8, u8) {
    let Some(device) = sysfs_device(path) else {
        return (0, fallback);
    };
    let read = |name: &str| {
        fs::read_to_string(format!("/sys/bus/usb/devices/{device}/{name}"))
            .ok()
            .and_then(|value| value.trim().parse::<u8>().ok())
    };
    (
        read("busnum").unwrap_or(0),
        read("devnum").unwrap_or(fallback),
    )
}

struct HidLink {
    device: HidDevice,
}

impl HidLink {
    /// Remove reports left queued by an earlier session before the first request is sent.
    fn drain(&mut self) {
        let mut buffer = [0u8; REPORT_BYTES + 1];
        for _ in 0..64 {
            match self.device.read_timeout(&mut buffer, 0) {
                Ok(read) if read > 0 => {}
                _ => break,
            }
        }
    }
}

impl ReportLink for HidLink {
    fn send_report(&mut self, report: &[u8; REPORT_BYTES]) -> Result<(), Error> {
        let mut buffer = [0u8; REPORT_BYTES + 1];
        buffer[1..].copy_from_slice(report);
        let written = self
            .device
            .write(&buffer)
            .map_err(|error| Error::Protocol(format!("HID write failed: {error}")))?;
        if written != REPORT_BYTES && written != REPORT_BYTES + 1 {
            return Err(Error::Protocol(format!(
                "HID write returned {written} bytes, expected {} or {}",
                REPORT_BYTES,
                REPORT_BYTES + 1
            )));
        }
        Ok(())
    }

    fn receive_report(&mut self) -> Result<[u8; REPORT_BYTES], Error> {
        let mut buffer = [0u8; REPORT_BYTES + 1];
        let read = self
            .device
            .read_timeout(&mut buffer, READ_TIMEOUT_MS)
            .map_err(|error| Error::Protocol(format!("HID read failed: {error}")))?;
        match read {
            0 => Err(Error::Protocol(format!(
                "no reply from Remote within {READ_TIMEOUT_MS} ms"
            ))),
            REPORT_BYTES => {
                let mut report = [0u8; REPORT_BYTES];
                report.copy_from_slice(&buffer[..REPORT_BYTES]);
                Ok(report)
            }
            n if n == REPORT_BYTES + 1 => {
                let mut report = [0u8; REPORT_BYTES];
                report.copy_from_slice(&buffer[1..]);
                Ok(report)
            }
            n => Err(Error::Protocol(format!(
                "HID read returned {n} bytes, expected {REPORT_BYTES} or {}",
                REPORT_BYTES + 1
            ))),
        }
    }
}
