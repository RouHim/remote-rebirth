//! `detect`: identify the connected Remote without writing.
//!
//! The Harmony 300 and 350 share product ID `0xC124`, so the family comes from the USB descriptor's
//! `bcdDevice` and the identity from `/sys/sysinfo`. A single Logitech device that is not a Harmony
//! is reported as unsupported rather than as no Remote; more than one Logitech device is an error,
//! because the CLI never guesses which unit to open.

#[cfg(feature = "cli")]
use std::io::Write;

#[cfg(feature = "cli")]
use serde::Serialize;

use crate::error::Error;
use crate::profile;
#[cfg(feature = "cli")]
use crate::profile::ModelProfile;
use crate::protocol::REPORT_BYTES;
use crate::sysinfo::{self, SysInfo};
use crate::transport::{RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession, read_file};

const LOGITECH_VENDOR_ID: u16 = 0x046D;
const HARMONY_300_350_PRODUCT_ID: u16 = 0xC124;
const HARMONY_200_PRODUCT_ID: u16 = 0xC125;

/// A Remote whose identity has been read and whose session is still open.
pub struct OpenedRemote {
    pub descriptor: RemoteDescriptor,
    pub sysinfo: SysInfo,
    pub session: Box<dyn RemoteSession>,
}

/// The single Logitech device attached: either a Harmony we opened, or another Logitech product.
pub enum Selected {
    Harmony(OpenedRemote),
    Other(RemoteDescriptor),
}

/// Select the single Logitech device and, when it is a Harmony, open it and read `/sys/sysinfo`.
pub fn select(backend: &mut dyn RemoteBackend) -> Result<Selected, Error> {
    let logitech: Vec<RemoteDescriptor> = backend
        .list()?
        .into_iter()
        .filter(|device| device.vendor_id == LOGITECH_VENDOR_ID)
        .collect();
    if logitech.is_empty() {
        return Err(Error::Device(
            "no Harmony Remote found; connect a Harmony 300 and retry".into(),
        ));
    }

    let mut harmony: Vec<RemoteDescriptor> = logitech
        .iter()
        .filter(|device| is_harmony(device))
        .cloned()
        .collect();
    match harmony.len() {
        0 if logitech.len() == 1 => {
            Ok(Selected::Other(logitech.into_iter().next().ok_or_else(
                || Error::Device("no Logitech device found".into()),
            )?))
        }
        0 => Err(Error::Device(format!(
            "multiple Logitech devices found, none is a Harmony Remote: {}; disconnect all but one",
            describe(&logitech)
        ))),
        1 => {
            let descriptor = harmony
                .pop()
                .ok_or_else(|| Error::Device("no Harmony Remote found".into()))?;
            open(backend, descriptor).map(Selected::Harmony)
        }
        _ => Err(Error::Device(format!(
            "multiple Harmony Remotes found: {}; disconnect all but one",
            describe(&harmony)
        ))),
    }
}

fn describe(devices: &[RemoteDescriptor]) -> String {
    devices
        .iter()
        .map(|device| {
            format!(
                "bus {} address {} ({}, vendor 0x{:04X}, product 0x{:04X})",
                device.bus,
                device.address,
                device.product.as_deref().unwrap_or("unknown product"),
                device.vendor_id,
                device.product_id
            )
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Open the connected Harmony, refusing anything that is not one.
pub fn open_remote(backend: &mut dyn RemoteBackend) -> Result<OpenedRemote, Error> {
    match select(backend)? {
        Selected::Harmony(opened) => Ok(opened),
        Selected::Other(descriptor) => Err(Error::Device(format!(
            "the connected Logitech device (vendor 0x{:04X}, product 0x{:04X}) is not a Harmony \
             Remote; connect a Harmony 300 and retry",
            descriptor.vendor_id, descriptor.product_id
        ))),
    }
}

fn is_harmony(descriptor: &RemoteDescriptor) -> bool {
    descriptor.product_id == HARMONY_300_350_PRODUCT_ID
        || descriptor.product_id == HARMONY_200_PRODUCT_ID
        || descriptor
            .product
            .as_deref()
            .is_some_and(|product| product.to_ascii_lowercase().contains("harmony"))
}

fn open(
    backend: &mut dyn RemoteBackend,
    descriptor: RemoteDescriptor,
) -> Result<OpenedRemote, Error> {
    let mut session = backend.open(&RemoteSelector {
        bus: descriptor.bus,
        address: descriptor.address,
        path: descriptor.path.clone(),
    })?;
    session.ping()?;

    let bytes = read_file(&mut *session, "/sys/sysinfo")?;
    let sysinfo = sysinfo::parse(&bytes);
    let packet_length = sysinfo.link_packet_length;
    if packet_length != Some(REPORT_BYTES as u16) {
        return Err(Error::Device(format!(
            "the Remote reports link_packet_length {}, but this tool speaks {REPORT_BYTES}-byte \
             reports; no Write will be performed",
            packet_length.map_or_else(|| "missing".to_string(), |value| value.to_string())
        )));
    }
    tracing::info!(
        bus = descriptor.bus,
        address = descriptor.address,
        bcd_device = format_args!("0x{:04X}", descriptor.bcd_device),
        family = profile::family(descriptor.bcd_device).unwrap_or("unknown"),
        skin = ?sysinfo.skin,
        "opened Remote"
    );
    Ok(OpenedRemote {
        descriptor,
        sysinfo,
        session,
    })
}

#[cfg(feature = "cli")]
pub fn run(
    backend: &mut dyn RemoteBackend,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let span = tracing::info_span!("detect");
    let _entered = span.enter();

    match select(backend)? {
        Selected::Other(descriptor) => report_other(&descriptor, json, stdout),
        Selected::Harmony(opened) => report_harmony(&opened, json, stdout),
    }
}

#[cfg(feature = "cli")]
fn report_harmony(opened: &OpenedRemote, json: bool, stdout: &mut dyn Write) -> Result<(), Error> {
    let info = &opened.sysinfo;
    let profile: Option<&ModelProfile> = profile::for_identity(info.architecture, info.skin);
    let model = profile
        .map(|profile| profile.model)
        .or_else(|| profile::recognized_model(info.architecture, info.skin))
        .unwrap_or("unknown Harmony Remote");
    let supported = profile.is_some();
    let family = profile::family(opened.descriptor.bcd_device);
    tracing::info!(
        model,
        supported,
        profile = profile.map(|profile| profile.id).unwrap_or("none"),
        "identified Remote"
    );

    if json {
        let output = DetectOutput {
            schema_version: 1,
            supported,
            model,
            family,
            profile: profile.map(|profile| profile.id),
            architecture: info.architecture,
            skin: info.skin,
            firmware_version: info.firmware_version.as_deref(),
            hardware_version: info.hardware_version.as_deref(),
            guid: info.guid.as_deref(),
            serial: info.serial.as_deref(),
            usb: UsbIdentity::from(&opened.descriptor),
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
        return Ok(());
    }

    writeln!(stdout, "Model: {model}")?;
    writeln!(
        stdout,
        "Supported: {}",
        if supported { "yes" } else { "no" }
    )?;
    if let Some(family) = family {
        writeln!(stdout, "Family: {family}")?;
    }
    if let Some(architecture) = info.architecture {
        writeln!(stdout, "Architecture: {architecture}")?;
    }
    if let Some(skin) = info.skin {
        writeln!(stdout, "Skin: {skin}")?;
    }
    if let Some(version) = &info.firmware_version {
        writeln!(stdout, "Firmware: {version}")?;
    }
    if let Some(version) = &info.hardware_version {
        writeln!(stdout, "Hardware: {version}")?;
    }
    if let Some(guid) = &info.guid {
        writeln!(stdout, "GUID: {guid}")?;
    }
    if let Some(serial) = &info.serial {
        writeln!(stdout, "Serial: {serial}")?;
    }
    write_usb(stdout, &opened.descriptor)?;
    if !supported {
        writeln!(
            stdout,
            "This model is recognised but not supported in this version; no write will be performed."
        )?;
    }
    Ok(())
}

#[cfg(feature = "cli")]
fn report_other(
    descriptor: &RemoteDescriptor,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    tracing::warn!(
        vendor_id = format_args!("0x{:04X}", descriptor.vendor_id),
        product_id = format_args!("0x{:04X}", descriptor.product_id),
        "the connected Logitech device is not a Harmony Remote"
    );
    if json {
        let output = DetectOutput {
            schema_version: 1,
            supported: false,
            model: "unknown Logitech device",
            family: None,
            profile: None,
            architecture: None,
            skin: None,
            firmware_version: None,
            hardware_version: None,
            guid: None,
            serial: None,
            usb: UsbIdentity::from(descriptor),
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
        return Ok(());
    }

    writeln!(stdout, "Model: unknown Logitech device")?;
    writeln!(stdout, "Supported: no")?;
    write_usb(stdout, descriptor)?;
    writeln!(
        stdout,
        "This Logitech device is not a Harmony Remote; connect a Harmony 300 and retry."
    )?;
    Ok(())
}

#[cfg(feature = "cli")]
fn write_usb(stdout: &mut dyn Write, descriptor: &RemoteDescriptor) -> Result<(), Error> {
    writeln!(
        stdout,
        "USB: vendor 0x{:04X} product 0x{:04X} bcdDevice 0x{:04X} ({}), bus {} address {}",
        descriptor.vendor_id,
        descriptor.product_id,
        descriptor.bcd_device,
        descriptor.product.as_deref().unwrap_or("unknown product"),
        descriptor.bus,
        descriptor.address
    )?;
    Ok(())
}

#[derive(Serialize)]
#[cfg(feature = "cli")]
struct DetectOutput<'a> {
    schema_version: u8,
    supported: bool,
    model: &'a str,
    family: Option<&'static str>,
    profile: Option<&'static str>,
    architecture: Option<u8>,
    skin: Option<u8>,
    firmware_version: Option<&'a str>,
    hardware_version: Option<&'a str>,
    guid: Option<&'a str>,
    serial: Option<&'a str>,
    usb: UsbIdentity<'a>,
}

#[derive(Serialize)]
#[cfg(feature = "cli")]
struct UsbIdentity<'a> {
    vendor_id: u16,
    product_id: u16,
    bcd_device: u16,
    product: Option<&'a str>,
    serial: Option<&'a str>,
    bus: u8,
    address: u8,
}

#[cfg(feature = "cli")]
impl<'a> From<&'a RemoteDescriptor> for UsbIdentity<'a> {
    fn from(descriptor: &'a RemoteDescriptor) -> Self {
        Self {
            vendor_id: descriptor.vendor_id,
            product_id: descriptor.product_id,
            bcd_device: descriptor.bcd_device,
            product: descriptor.product.as_deref(),
            serial: descriptor.serial.as_deref(),
            bus: descriptor.bus,
            address: descriptor.address,
        }
    }
}
