//! Typed core operations for the desktop GUI.
//!
//! The CLI keeps its `run(args, runtime)` adapter. The GUI calls these functions directly so it
//! never parses CLI output and never depends on stdout.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::backup::{self, BackupReport};
use crate::clock::Clock;
use crate::compose::{self, ComposeReport};
use crate::detect::{self, Selected};
use crate::error::Error;
use crate::model::{self, Model};
use crate::profile;
use crate::progress::Progress;
use crate::restore;
pub use crate::restore::RestoreRequest;
use crate::transport::{RemoteBackend, RemoteDescriptor};
pub use crate::write::WriteRequest;
use crate::write::{self, WriteReport};

pub use crate::backup::BackupReport as BackupResult;
pub use crate::compose::ComposeReport as ComposeResult;
pub use crate::write::WriteReport as WriteResult;

/// The detect report's schema version, stamped into every JSON output.
pub const DETECT_SCHEMA_VERSION: u8 = 1;

/// One remote's identity, read without writing.
#[derive(Debug, Clone, Serialize)]
pub struct DetectReport {
    pub schema_version: u8,
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
            schema_version: DETECT_SCHEMA_VERSION,
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
                schema_version: DETECT_SCHEMA_VERSION,
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

pub fn write(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    request: WriteRequest<'_>,
    progress: &mut dyn FnMut(Progress),
) -> Result<WriteReport, Error> {
    write::perform(backend, clock, request, progress)
}

pub fn restore(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    request: RestoreRequest<'_>,
    progress: &mut dyn FnMut(Progress),
) -> Result<WriteReport, Error> {
    restore::perform(backend, clock, request, progress)
}

pub fn load_model(path: &Path) -> Result<Model, Error> {
    model::load(path)
}

pub fn save_model(path: &Path, model: &Model) -> Result<(), Error> {
    write_atomic(path, &serde_json::to_vec_pretty(model)?)
}

/// Write `bytes` to `path` through a sibling temporary file, then rename it into place, so a
/// partial write never replaces the previous contents and a symlink is never followed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    crate::output::write_file(path, bytes)
}

/// The sidecar path that belongs to a Backup, so the GUI never rebuilds the convention by hand.
pub fn backup_sidecar(backup: &Path) -> PathBuf {
    crate::paths::sidecar_path(backup)
}

/// What the GUI shows for one Backup without parsing the sidecar as untyped JSON.
#[derive(Debug, Clone, Serialize)]
pub struct BackupSummary {
    pub backup: PathBuf,
    pub sidecar: PathBuf,
    pub model: String,
    pub skin: Option<u8>,
    pub architecture: Option<u8>,
    pub firmware_version: Option<String>,
    pub size: u64,
    pub sha256: String,
    pub timestamp: String,
    pub validation: String,
}

#[derive(Deserialize)]
struct SidecarView {
    model: String,
    #[serde(default)]
    skin: Option<u8>,
    #[serde(default)]
    architecture: Option<u8>,
    #[serde(default)]
    firmware_version: Option<String>,
    size: u64,
    sha256: String,
    timestamp: String,
    validation: String,
}

/// Read one Backup sidecar. The Backup's own path is derived from the sidecar's name, so a listing
/// never guesses at the convention.
pub fn backup_summary(sidecar: &Path) -> Result<BackupSummary, Error> {
    let raw = std::fs::read(sidecar).map_err(|error| {
        Error::Validation(format!(
            "Backup sidecar {} cannot be read: {error}",
            sidecar.display()
        ))
    })?;
    let view: SidecarView = serde_json::from_slice(&raw).map_err(|error| {
        Error::Validation(format!(
            "Backup sidecar {} is not valid JSON: {error}",
            sidecar.display()
        ))
    })?;
    let backup = sidecar.with_extension("");
    Ok(BackupSummary {
        backup,
        sidecar: sidecar.to_path_buf(),
        model: view.model,
        skin: view.skin,
        architecture: view.architecture,
        firmware_version: view.firmware_version,
        size: view.size,
        sha256: view.sha256,
        timestamp: view.timestamp,
        validation: view.validation,
    })
}

/// The commands one local Flipper file offers, validated the way an import validates them.
pub fn list_flipper_commands(path: &Path) -> Result<Vec<String>, Error> {
    let text = std::fs::read_to_string(path).map_err(|error| {
        Error::Validation(format!("{} cannot be read: {error}", path.display()))
    })?;
    let context = crate::ir_types::Context {
        source: path.display().to_string(),
        selector: String::new(),
    };
    crate::ir_flipper::command_names(&text, &context)
        .map_err(|error| Error::Validation(error.to_string()))
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
