use std::io::Write;

use serde::Serialize;

use crate::error::Error;
use crate::profile::{self, ModelProfile};
use crate::sysinfo::{self, SysInfo};
use crate::transport::{
    RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession, read_file,
};

const HARMONY_VENDOR_ID: u16 = 0x046D;
const HARMONY_300_350_PRODUCT_ID: u16 = 0xC124;
const HARMONY_200_PRODUCT_ID: u16 = 0xC125;

pub struct OpenedRemote {
    pub descriptor: RemoteDescriptor,
    pub sysinfo: SysInfo,
    pub session: Box<dyn RemoteSession>,
}

pub fn open_remote(backend: &mut dyn RemoteBackend) -> Result<OpenedRemote, Error> {
    let devices: Vec<RemoteDescriptor> = backend
        .list()?
        .into_iter()
        .filter(|device| device.vendor_id == HARMONY_VENDOR_ID)
        .filter(|device| {
            device.product_id == HARMONY_300_350_PRODUCT_ID
                || device.product_id == HARMONY_200_PRODUCT_ID
                || device
                    .product
                    .as_deref()
                    .is_some_and(|product| product.to_ascii_lowercase().contains("harmony"))
        })
        .collect();

    match devices.len() {
        0 => Err(Error::Device(
            "no Harmony Remote found; connect a Harmony 300 and retry".into(),
        )),
        1 => {
            let descriptor = devices
                .into_iter()
                .next()
                .ok_or_else(|| Error::Device("no Harmony Remote found".into()))?;
            let mut session = backend.open(&RemoteSelector {
                bus: descriptor.bus,
                address: descriptor.address,
            })?;
            let bytes = read_file(&mut *session, "/sys/sysinfo")?;
            let sysinfo = sysinfo::parse(&bytes);
            Ok(OpenedRemote {
                descriptor,
                sysinfo,
                session,
            })
        }
        _ => {
            let list = devices
                .iter()
                .map(|device| {
                    format!(
                        "bus {} address {} ({})",
                        device.bus,
                        device.address,
                        device.product.as_deref().unwrap_or("unknown")
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            Err(Error::Device(format!(
                "multiple Harmony Remotes found: {list}; disconnect all but one"
            )))
        }
    }
}

pub fn run(
    backend: &mut dyn RemoteBackend,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let opened = open_remote(backend)?;
    let info = &opened.sysinfo;
    let profile: Option<&ModelProfile> = profile::for_identity(info.architecture, info.skin);
    let model = profile
        .map(|profile| profile.model)
        .or_else(|| profile::recognized_model(info.architecture, info.skin))
        .unwrap_or("unknown Harmony Remote");
    let supported = profile.is_some();

    if json {
        let output = DetectOutput {
            schema_version: 1,
            supported,
            model,
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
    writeln!(stdout, "Supported: {}", if supported { "yes" } else { "no" })?;
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
    if !supported {
        writeln!(
            stdout,
            "This model is recognised but not supported in this version; no write will be performed."
        )?;
    }
    Ok(())
}

#[derive(Serialize)]
struct DetectOutput<'a> {
    schema_version: u8,
    supported: bool,
    model: &'a str,
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
struct UsbIdentity<'a> {
    vendor_id: u16,
    product_id: u16,
    bcd_device: u16,
    product: Option<&'a str>,
    serial: Option<&'a str>,
    bus: u8,
    address: u8,
}

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
