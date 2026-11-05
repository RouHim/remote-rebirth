use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::clock::{Clock, format_rfc3339};
use crate::container::{self, ValidationStatus};
use crate::detect;
use crate::error::Error;
use crate::profile;
use crate::transport::{RemoteBackend, read_file};

pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    out: &Path,
    overwrite: bool,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
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
    if out.exists() && !overwrite {
        return Err(Error::Validation(format!(
            "{} already exists; pass --overwrite to replace it",
            out.display()
        )));
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
    let sidecar_path = sidecar_path(out);

    fs::write(out, &container)?;

    let sidecar = Sidecar {
        schema_version: 1,
        file: out
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_string(),
        model: model_name(architecture, skin),
        skin,
        architecture,
        firmware_version,
        guid,
        serial,
        size: container.len() as u64,
        sha256: digest.clone(),
        timestamp,
        validation,
    };
    fs::write(&sidecar_path, serde_json::to_vec_pretty(&sidecar)?)?;

    if json {
        let output = BackupOutput {
            schema_version: 1,
            backup: out.display().to_string(),
            sidecar: sidecar_path.display().to_string(),
            size: container.len() as u64,
            sha256: digest,
            validation,
        };
        serde_json::to_writer_pretty(&mut *stdout, &output)?;
        writeln!(stdout)?;
        return Ok(());
    }

    writeln!(
        stdout,
        "Wrote Backup {} ({} bytes, sha256 {})",
        out.display(),
        container.len(),
        digest
    )?;
    writeln!(stdout, "Wrote sidecar {}", sidecar_path.display())?;
    if validation != ValidationStatus::Ok {
        writeln!(
            stdout,
            "Warning: the Backup validation status is {}",
            validation.as_str()
        )?;
    }
    Ok(())
}

fn sidecar_path(out: &Path) -> PathBuf {
    let mut name = out.as_os_str().to_os_string();
    name.push(".json");
    PathBuf::from(name)
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

#[derive(Serialize)]
struct BackupOutput {
    schema_version: u8,
    backup: String,
    sidecar: String,
    size: u64,
    sha256: String,
    validation: ValidationStatus,
}
