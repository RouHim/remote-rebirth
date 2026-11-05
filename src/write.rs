use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::container::{self, MAX_CONTAINER_SIZE};
use crate::detect;
use crate::error::Error;
use crate::filewriter::{self, WritePlan};
use crate::journal::Journal;
use crate::profile;
use crate::sysinfo::SysInfo;
use crate::transport::{RemoteBackend, RemoteSelector, read_file};

#[allow(clippy::too_many_arguments)]
pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    container_path: &Path,
    backup_path: &Path,
    commit: bool,
    no_reboot: bool,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let container = read_input(container_path, "Container")?;
    validate_container(&container)?;
    let (backup, sidecar) = load_backup(backup_path)?;

    let detect::OpenedRemote {
        descriptor,
        sysinfo,
        mut session,
    } = detect::open_remote(backend)?;
    check_remote(&sysinfo, &sidecar)?;

    let current = read_file(&mut *session, "/cfg/usercfg")?;
    let current = container::reconstruct_trailer(&current);
    if current != backup && !is_empty_configuration(&current) {
        return Err(Error::Validation(format!(
            "the Remote's current Configuration does not match Backup {}; capture it with \
             `backup --out <path>` before writing",
            backup_path.display()
        )));
    }

    let write_plan = filewriter::plan(filewriter::stored_bytes(&container).len());

    if !commit {
        report(
            json,
            stdout,
            &WriteOutput::new(
                filewriter::Operation::Write.as_str(),
                false,
                Some(container_path),
                backup_path,
                container.len(),
                &sidecar.sha256,
                write_plan,
                no_reboot,
                None,
            ),
        )?;
        return Ok(());
    }

    let mut journal = Journal::create(state_dir, clock, "write", backup_path, &sidecar.sha256)?;
    journal.record(
        "prepare",
        0,
        filewriter::stored_bytes(&container).len(),
        "ok",
    )?;

    let result = filewriter::execute(
        filewriter::Operation::Write,
        &mut *session,
        &mut journal,
        &container,
        backup_path,
        no_reboot,
    );

    if let Err(error) = result {
        if matches!(error, Error::Protocol(_)) {
            drop(session);
            let _ = backend.open(&RemoteSelector {
                bus: descriptor.bus,
                address: descriptor.address,
            });
        }
        return Err(error);
    }

    report(
        json,
        stdout,
        &WriteOutput::new(
            filewriter::Operation::Write.as_str(),
            true,
            Some(container_path),
            backup_path,
            container.len(),
            &sidecar.sha256,
            write_plan,
            no_reboot,
            Some(journal.path()),
        ),
    )
}

pub(crate) fn read_input(path: &Path, what: &str) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|error| {
        Error::Validation(format!("{what} {} cannot be read: {error}", path.display()))
    })
}

pub(crate) fn validate_container(bytes: &[u8]) -> Result<(), Error> {
    if bytes.len() > MAX_CONTAINER_SIZE {
        return Err(Error::Validation(format!(
            "Container is {} bytes; the maximum is {MAX_CONTAINER_SIZE} bytes",
            bytes.len()
        )));
    }
    if !container::has_header(bytes) || !bytes.ends_with(&container::END_MARKER[..]) {
        return Err(Error::Validation(
            "Container is not an Architecture 16 frame with a PTYY trailer".into(),
        ));
    }
    match container::validation_status(bytes) {
        container::ValidationStatus::Ok => Ok(()),
        container::ValidationStatus::ChecksumMismatch => Err(Error::Validation(format!(
            "Container checksum does not match its stored trailer word {}",
            container::stored_checksum(bytes).unwrap_or_default()
        ))),
        container::ValidationStatus::Unreadable => {
            Err(Error::Validation("Container is unreadable".into()))
        }
    }
}

pub(crate) fn is_empty_configuration(bytes: &[u8]) -> bool {
    bytes.is_empty() || bytes.iter().all(|byte| *byte == 0xFF)
}

pub(crate) fn sidecar_path(backup: &Path) -> PathBuf {
    let mut name = backup.as_os_str().to_os_string();
    name.push(".json");
    PathBuf::from(name)
}

#[derive(Debug, Deserialize)]
pub(crate) struct BackupSidecar {
    pub schema_version: u64,
    pub model: String,
    pub skin: Option<u8>,
    pub architecture: Option<u8>,
    pub size: u64,
    pub sha256: String,
    pub validation: String,
}

pub(crate) fn load_backup(path: &Path) -> Result<(Vec<u8>, BackupSidecar), Error> {
    let bytes = read_input(path, "Backup")?;
    let sidecar_path = sidecar_path(path);
    let raw = read_input(&sidecar_path, "Backup sidecar")?;
    let sidecar: BackupSidecar = serde_json::from_slice(&raw).map_err(|error| {
        Error::Validation(format!(
            "Backup sidecar {} is not valid JSON: {error}",
            sidecar_path.display()
        ))
    })?;
    if sidecar.schema_version != 1 {
        return Err(Error::Validation(format!(
            "Backup sidecar {} has unsupported schema version {}",
            sidecar_path.display(),
            sidecar.schema_version
        )));
    }
    if sidecar.size != bytes.len() as u64 {
        return Err(Error::Validation(format!(
            "Backup sidecar records {} bytes but the Backup has {}",
            sidecar.size,
            bytes.len()
        )));
    }
    let digest = container::sha256_hex(&bytes);
    if sidecar.sha256 != digest {
        return Err(Error::Validation(format!(
            "Backup sidecar digest does not match {}",
            path.display()
        )));
    }
    if sidecar.validation != "ok" {
        return Err(Error::Validation(format!(
            "Backup validation is {}; refusing to write from an unverified Backup",
            sidecar.validation
        )));
    }
    Ok((bytes, sidecar))
}

/// Refuse an unsupported Remote, or a Backup captured from a different model or Skin.
pub(crate) fn check_remote(sysinfo: &SysInfo, sidecar: &BackupSidecar) -> Result<(), Error> {
    let profile = profile::for_identity(sysinfo.architecture, sysinfo.skin).ok_or_else(|| {
        Error::Device(format!(
            "the connected Remote is recognised but not supported in this version \
             (architecture {:?}, Skin {:?}); no Write will be performed",
            sysinfo.architecture, sysinfo.skin
        ))
    })?;
    if sidecar.architecture != sysinfo.architecture
        || sidecar.skin != sysinfo.skin
        || sidecar.model != profile.model
    {
        return Err(Error::Validation(format!(
            "Backup was captured from {} (architecture {:?}, Skin {:?}), but the connected \
             Remote is {} (Skin {:?})",
            sidecar.model, sidecar.architecture, sidecar.skin, profile.model, sysinfo.skin
        )));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub(crate) struct WriteOutput<'a> {
    schema_version: u8,
    operation: &'a str,
    commit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    container: Option<String>,
    backup: String,
    size: usize,
    backup_sha256: &'a str,
    bytes: usize,
    chunks: usize,
    data_packets: usize,
    reboot: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    journal: Option<String>,
}

impl<'a> WriteOutput<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        operation: &'a str,
        commit: bool,
        container: Option<&Path>,
        backup: &Path,
        size: usize,
        backup_sha256: &'a str,
        plan: WritePlan,
        no_reboot: bool,
        journal: Option<&Path>,
    ) -> Self {
        Self {
            schema_version: 1,
            operation,
            commit,
            container: container.map(|path| path.display().to_string()),
            backup: backup.display().to_string(),
            size,
            backup_sha256,
            bytes: plan.bytes,
            chunks: plan.chunks,
            data_packets: plan.data_packets,
            reboot: !no_reboot,
            journal: journal.map(|path| path.display().to_string()),
        }
    }
}

pub(crate) fn report(
    json: bool,
    stdout: &mut dyn Write,
    output: &WriteOutput<'_>,
) -> Result<(), Error> {
    if json {
        serde_json::to_writer_pretty(&mut *stdout, output)?;
        writeln!(stdout)?;
        return Ok(());
    }
    let container = output
        .container
        .as_deref()
        .map(|path| format!(" {path}"))
        .unwrap_or_default();
    let completed = if output.operation == "restore" {
        "Restored"
    } else {
        "Wrote"
    };
    let attempted = if output.operation == "restore" {
        "restore"
    } else {
        "write"
    };
    if output.commit {
        writeln!(
            stdout,
            "{completed}{container} {} bytes in {} chunk(s); checksum m; Read-Back verified",
            output.bytes, output.chunks
        )?;
        writeln!(
            stdout,
            "Backup {} (sha256 {})",
            output.backup, output.backup_sha256
        )?;
        if output.reboot {
            writeln!(stdout, "Reboot requested")?;
        } else {
            writeln!(stdout, "Reboot suppressed by --no-reboot")?;
        }
        if let Some(journal) = &output.journal {
            writeln!(stdout, "Journal: {journal}")?;
        }
    } else {
        writeln!(
            stdout,
            "Dry Run: would {attempted}{container} {} bytes in {} chunk(s) ({} data packets) to /cfg/usercfg",
            output.bytes, output.chunks, output.data_packets
        )?;
        writeln!(
            stdout,
            "Backup {} (sha256 {})",
            output.backup, output.backup_sha256
        )?;
        writeln!(stdout, "Re-run with --commit to write")?;
    }
    Ok(())
}
