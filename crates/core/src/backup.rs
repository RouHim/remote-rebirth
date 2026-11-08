#[cfg(feature = "cli")]
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::clock::{Clock, format_rfc3339};
use crate::container::{self, ValidationStatus};
use crate::detect;
use crate::error::Error;
use crate::paths::sidecar_path;
use crate::profile;
use crate::transport::{RemoteBackend, read_file};

/// The Backup sidecar schema, embedded so `backup --print-schema` needs no installed files.
#[cfg(feature = "cli")]
pub const SCHEMA: &str = include_str!("../schema/backup-v1.json");

/// A captured Backup, without any CLI formatting.
#[derive(Debug, Clone)]
pub struct BackupReport {
    pub container: PathBuf,
    pub sidecar: PathBuf,
    pub model: String,
    pub skin: Option<u8>,
    pub architecture: Option<u8>,
    pub firmware_version: Option<String>,
    pub guid: Option<String>,
    pub serial: Option<String>,
    pub size: usize,
    pub sha256: String,
    pub timestamp: String,
    pub validation: ValidationStatus,
}

pub fn create(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    out: &Path,
    overwrite: bool,
) -> Result<BackupReport, Error> {
    let mut opened = detect::open_remote(backend)?;
    let architecture = opened.sysinfo.architecture;
    let skin = opened.sysinfo.skin;
    let firmware_version = opened.sysinfo.firmware_version.clone();
    let guid = opened.sysinfo.guid.clone();
    let serial = opened.sysinfo.serial.clone();

    if architecture != Some(16) {
        return Err(Error::Device(format!(
            "Backup supports Architecture 16 only; the connected Remote reports architecture {architecture:?}"
        )));
    }
    let sidecar_file = sidecar_path(out);
    if !overwrite {
        let mut existing = Vec::new();
        for path in [out, sidecar_file.as_path()] {
            if path.exists() {
                existing.push(path.display().to_string());
            }
        }
        if !existing.is_empty() {
            return Err(Error::Validation(format!(
                "{} already exists; pass --overwrite to replace it",
                existing.join(" and ")
            )));
        }
    }

    let read = read_file(&mut *opened.session, "/cfg/usercfg")?;
    let container = container::reconstruct_trailer(&read);
    if container.len() > container::MAX_CONTAINER_SIZE {
        return Err(Error::Validation(format!(
            "Configuration is {} bytes; the 256 KiB region is the maximum",
            container.len()
        )));
    }
    let validation = container::validation_status(&container);
    let digest = container::sha256_hex(&container);
    let timestamp = format_rfc3339(&clock.now())?;
    let model = model_name(architecture, skin);

    crate::output::write_file(out, &container)?;

    let sidecar = Sidecar {
        schema_version: 1,
        file: out
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        model: model.clone(),
        skin,
        architecture,
        firmware_version: firmware_version.clone(),
        guid: guid.clone(),
        serial: serial.clone(),
        size: container.len() as u64,
        sha256: digest.clone(),
        timestamp: timestamp.clone(),
        validation,
    };
    crate::output::write_file(&sidecar_file, &serde_json::to_vec_pretty(&sidecar)?)?;
    tracing::info!(
        size = container.len(),
        validation = validation.as_str(),
        sha256 = %digest,
        sidecar = %sidecar_file.display(),
        "wrote Backup"
    );

    Ok(BackupReport {
        container: out.to_path_buf(),
        sidecar: sidecar_file,
        model,
        skin,
        architecture,
        firmware_version,
        guid,
        serial,
        size: container.len(),
        sha256: digest,
        timestamp,
        validation,
    })
}

#[cfg(feature = "cli")]
pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    out: &Path,
    overwrite: bool,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let span = tracing::info_span!("backup", out = %out.display());
    let _entered = span.enter();
    let report = create(backend, clock, out, overwrite)?;

    if json {
        let output = BackupOutput {
            schema_version: 1,
            backup: report.container.display().to_string(),
            sidecar: report.sidecar.display().to_string(),
            size: report.size as u64,
            sha256: report.sha256,
            validation: report.validation,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
        return Ok(());
    }

    writeln!(
        stdout,
        "Wrote Backup {} ({} bytes, sha256 {})",
        report.container.display(),
        report.size,
        report.sha256
    )?;
    writeln!(stdout, "Wrote sidecar {}", report.sidecar.display())?;
    if report.validation != ValidationStatus::Ok {
        writeln!(
            stdout,
            "Warning: the Backup validation status is {}",
            report.validation.as_str()
        )?;
    }
    Ok(())
}

fn model_name(architecture: Option<u8>, skin: Option<u8>) -> String {
    profile::recognized_model(architecture, skin)
        .unwrap_or("unknown Harmony Remote")
        .to_string()
}

#[derive(Serialize)]
struct Sidecar {
    schema_version: u8,
    file: String,
    model: String,
    skin: Option<u8>,
    architecture: Option<u8>,
    firmware_version: Option<String>,
    guid: Option<String>,
    serial: Option<String>,
    size: u64,
    sha256: String,
    timestamp: String,
    validation: ValidationStatus,
}

#[cfg(feature = "cli")]
#[derive(Serialize)]
struct BackupOutput {
    schema_version: u8,
    backup: String,
    sidecar: String,
    size: u64,
    sha256: String,
    validation: ValidationStatus,
}
