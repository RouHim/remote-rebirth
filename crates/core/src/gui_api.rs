//! Typed core operations for the desktop GUI.
//!
//! The CLI keeps its `run(args, runtime)` adapter. The GUI calls these functions directly so it
//! never parses CLI output and never depends on stdout.

use std::path::Path;

use serde::Serialize;

use crate::backup::{self, BackupReport};
use crate::clock::Clock;
use crate::compose::{self, ComposeReport};
use crate::detect::{self, Selected};
use crate::error::Error;
use crate::model::{self, Model};
use crate::profile;
use crate::progress::Progress;
use crate::restore;
use crate::transport::{RemoteBackend, RemoteDescriptor};
use crate::write::{self, WriteReport};

pub use crate::backup::BackupReport as BackupResult;
pub use crate::compose::ComposeReport as ComposeResult;
pub use crate::write::WriteReport as WriteResult;

/// One remote's identity, read without writing.
#[derive(Debug, Clone, Serialize)]
pub struct DetectReport {
    pub supported: bool,
    pub model: String,
    pub family: Option<&'static str>,
    pub profile: Option<&'static str>,
    pub architecture: Option<u8>,
    pub skin: Option<u8>,
    pub firmware_version: Option<String>,
    pub hardware_version: Option<String>,
    pub guid: Option<String>,
    pub serial: Option<String>,
    pub usb: UsbIdentity,
}

#[derive(Debug, Clone, Serialize)]
pub struct UsbIdentity {
    pub vendor_id: u16,
    pub product_id: u16,
    pub bcd_device: u16,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub bus: u8,
    pub address: u8,
}

impl From<&RemoteDescriptor> for UsbIdentity {
    fn from(descriptor: &RemoteDescriptor) -> Self {
        Self {
            vendor_id: descriptor.vendor_id,
            product_id: descriptor.product_id,
            bcd_device: descriptor.bcd_device,
            product: descriptor.product.clone(),
            serial: descriptor.serial.clone(),
            bus: descriptor.bus,
            address: descriptor.address,
        }
    }
}

/// Identify the connected Remote, refusing nothing: an unsupported model is reported as such.
pub fn detect(backend: &mut dyn RemoteBackend) -> Result<DetectReport, Error> {
    match detect::select(backend)? {
        Selected::Other(descriptor) => Ok(DetectReport {
            supported: false,
            model: "unknown Logitech device".into(),
            family: None,
            profile: None,
            architecture: None,
            skin: None,
            firmware_version: None,
            hardware_version: None,
            guid: None,
            serial: None,
            usb: UsbIdentity::from(&descriptor),
        }),
        Selected::Harmony(opened) => {
            let info = &opened.sysinfo;
            let active = profile::for_identity(info.architecture, info.skin);
            let model = active
                .map(|profile| profile.model)
                .or_else(|| profile::recognized_model(info.architecture, info.skin))
                .unwrap_or("unknown Harmony Remote")
                .to_string();
            Ok(DetectReport {
                supported: active.is_some(),
                model,
                family: profile::family(opened.descriptor.bcd_device),
                profile: active.map(|profile| profile.id),
                architecture: info.architecture,
                skin: info.skin,
                firmware_version: info.firmware_version.clone(),
                hardware_version: info.hardware_version.clone(),
                guid: info.guid.clone(),
                serial: info.serial.clone(),
                usb: UsbIdentity::from(&opened.descriptor),
            })
        }
    }
}

pub fn backup(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    out: &Path,
    overwrite: bool,
) -> Result<BackupReport, Error> {
    backup::create(backend, clock, out, overwrite)
}

pub fn compose(
    clock: &dyn Clock,
    model_path: &Path,
    out: &Path,
    overwrite: bool,
) -> Result<ComposeReport, Error> {
    compose::create(clock, model_path, out, overwrite)
}

#[allow(clippy::too_many_arguments)]
pub fn write(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    container_path: &Path,
    backup_path: &Path,
    commit: bool,
    no_reboot: bool,
    progress: &mut dyn FnMut(Progress),
) -> Result<WriteReport, Error> {
    write::perform(
        backend,
        clock,
        state_dir,
        container_path,
        backup_path,
        commit,
        no_reboot,
        progress,
    )
}

pub fn restore(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    backup_path: &Path,
    commit: bool,
    no_reboot: bool,
    progress: &mut dyn FnMut(Progress),
) -> Result<WriteReport, Error> {
    restore::perform(
        backend,
        clock,
        state_dir,
        backup_path,
        commit,
        no_reboot,
        progress,
    )
}

pub fn load_model(path: &Path) -> Result<Model, Error> {
    model::load(path)
}

pub fn save_model(path: &Path, model: &Model) -> Result<(), Error> {
    let bytes = serde_json::to_vec_pretty(model)?;
    let mut temporary = path.as_os_str().to_os_string();
    temporary.push(".tmp");
    let temporary = std::path::PathBuf::from(temporary);
    std::fs::write(&temporary, bytes)?;
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

pub fn validate_model(model: &Model) -> Result<(), Error> {
    let active = profile::by_id(&model.profile).ok_or_else(|| {
        Error::Validation(format!(
            "model names unknown Model Profile '{}'",
            model.profile
        ))
    })?;
    model::validate(model, active)
}

/// A blank Model for editing. It is deliberately invalid until a Device and an Activity exist.
pub fn blank_model(skin: u8) -> Model {
    Model {
        schema_version: model::SCHEMA_VERSION,
        profile: profile::HARMONY_300.id.to_string(),
        skin,
        devices: Vec::new(),
        routing: None,
        activity: model::Activity {
            channel_device: String::new(),
            steps: Vec::new(),
        },
        favourites: Vec::new(),
    }
}
