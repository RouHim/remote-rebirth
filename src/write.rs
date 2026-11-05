use std::fs;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::clock::Clock;
use crate::container::{self, MAX_CONTAINER_SIZE};
use crate::detect;
use crate::error::Error;
use crate::filewriter::{self, WritePlan};
use crate::inspect_container::{self, ParsedContainer};
use crate::journal::Journal;
use crate::paths::sidecar_path;
use crate::profile::{self, ModelProfile};
use crate::sysinfo::SysInfo;
use crate::transport::{RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession, read_file};

/// The flags a Write or Restore shares.
#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub commit: bool,
    pub no_reboot: bool,
    pub json: bool,
}

pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    container_path: &Path,
    backup_path: &Path,
    options: Options,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let span = tracing::info_span!(
        "write",
        container = %container_path.display(),
        backup = %backup_path.display(),
        commit = options.commit
    );
    let _entered = span.enter();

    let container = read_input(container_path, "Container")?;
    let parsed = validate_container(&container)?;
    let (backup, sidecar) = load_backup(backup_path)?;

    let detect::OpenedRemote {
        descriptor,
        sysinfo,
        mut session,
    } = detect::open_remote(backend)?;
    let profile = check_remote(&sysinfo, &sidecar)?;
    check_container(&parsed, &sysinfo, profile)?;

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

    if !options.commit {
        report(
            options.json,
            stdout,
            &WriteOutput::new(
                filewriter::Operation::Write.as_str(),
                false,
                Some(container_path),
                backup_path,
                container.len(),
                &sidecar.sha256,
                write_plan,
                options.no_reboot,
                None,
            ),
        )?;
        return Ok(());
    }

    let commit = filewriter::Commit {
        operation: filewriter::Operation::Write,
        container: &container,
        backup: backup_path,
        no_reboot: options.no_reboot,
    };
    tracing::info!("committed Write starting");
    let mut journal = Journal::create(state_dir, clock, "write", backup_path, &sidecar.sha256)?;
    journal.record(
        "prepare",
        0,
        filewriter::stored_bytes(&container).len(),
        "ok",
    )?;

    commit_container(backend, &descriptor, session, &mut journal, &commit)?;
    tracing::info!("committed Write finished and verified");

    report(
        options.json,
        stdout,
        &WriteOutput::new(
            filewriter::Operation::Write.as_str(),
            true,
            Some(container_path),
            backup_path,
            container.len(),
            &sidecar.sha256,
            write_plan,
            options.no_reboot,
            Some(journal.path()),
        ),
    )
}

/// Run a commit, and on a protocol failure reopen a fresh session so the failure is not reported
/// from a poisoned handle. The recovery read proves the new session answers.
pub(crate) fn commit_container(
    backend: &mut dyn RemoteBackend,
    descriptor: &RemoteDescriptor,
    session: Box<dyn RemoteSession>,
    journal: &mut Journal<'_>,
    commit: &filewriter::Commit<'_>,
) -> Result<WritePlan, Error> {
    let mut session = session;
    match filewriter::execute(commit, &mut *session, journal) {
        Ok(plan) => Ok(plan),
        Err(error @ Error::Protocol(_)) => {
            drop(session);
            Err(recover(backend, descriptor, journal, error))
        }
        Err(error) => Err(error),
    }
}

fn recover(
    backend: &mut dyn RemoteBackend,
    descriptor: &RemoteDescriptor,
    journal: &mut Journal<'_>,
    error: Error,
) -> Error {
    tracing::error!(%error, "protocol failure; reopening a fresh session");
    if let Err(journal_error) = journal.record("failure", 0, 0, "protocol") {
        tracing::error!(%journal_error, "could not journal the failure");
    }
    let selector = RemoteSelector {
        bus: descriptor.bus,
        address: descriptor.address,
        path: descriptor.path.clone(),
    };
    let outcome = match backend.open(&selector) {
        Ok(mut fresh) => match read_file(&mut *fresh, "/sys/sysinfo") {
            Ok(_) => Ok(()),
            Err(read_error) => Err(format!(
                "the reopened session could not read /sys/sysinfo: {read_error}"
            )),
        },
        Err(open_error) => Err(format!("the Remote could not be reopened: {open_error}")),
    };
    match outcome {
        Ok(()) => {
            if let Err(journal_error) = journal.record("recover", 0, 0, "ok") {
                tracing::error!(%journal_error, "could not journal the recovery");
            }
            tracing::warn!("reopened a fresh session");
            error
        }
        Err(detail) => {
            if let Err(journal_error) = journal.record("recover", 0, 0, "failed") {
                tracing::error!(%journal_error, "could not journal the failed recovery");
            }
            Error::Protocol(format!(
                "{error}; recovery failed: {detail}. The session may be poisoned: disconnect and \
                 reconnect the Remote, then run `restore --backup <path> --commit`"
            ))
        }
    }
}

pub(crate) fn read_input(path: &Path, what: &str) -> Result<Vec<u8>, Error> {
    fs::read(path).map_err(|error| {
        Error::Validation(format!("{what} {} cannot be read: {error}", path.display()))
    })
}

pub(crate) fn validate_container(bytes: &[u8]) -> Result<ParsedContainer, Error> {
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
        container::ValidationStatus::Ok => {}
        container::ValidationStatus::ChecksumMismatch => {
            return Err(Error::Validation(format!(
                "Container checksum does not match its stored trailer word {}",
                container::stored_checksum(bytes).unwrap_or_default()
            )));
        }
        container::ValidationStatus::Unreadable => {
            return Err(Error::Validation("Container is unreadable".into()));
        }
    }
    let parsed = inspect_container::parse(bytes)?;
    if parsed.container.architecture != Some(profile::HARMONY_300.architecture) {
        return Err(Error::Validation(format!(
            "Container states architecture {:?}; this tool writes Architecture 16 only",
            parsed.container.architecture
        )));
    }
    Ok(parsed)
}

pub(crate) fn is_empty_configuration(bytes: &[u8]) -> bool {
    bytes.is_empty() || bytes.iter().all(|byte| *byte == 0xFF)
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
        // A deliberate safety extension beyond the spec: Restore only trusts a Backup whose capture
        // sidecar says it was readable and checksum-clean, so a damaged file is never replayed.
        return Err(Error::Validation(format!(
            "Backup validation is {}; refusing to write from an unverified Backup",
            sidecar.validation
        )));
    }
    Ok((bytes, sidecar))
}

/// Refuse an unsupported Remote, or a Backup captured from a different model or Skin.
pub(crate) fn check_remote(
    sysinfo: &SysInfo,
    sidecar: &BackupSidecar,
) -> Result<&'static ModelProfile, Error> {
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
    Ok(profile)
}

/// Refuse a Container the connected Remote's Model Profile and Skin cannot hold.
pub(crate) fn check_container(
    parsed: &ParsedContainer,
    sysinfo: &SysInfo,
    profile: &ModelProfile,
) -> Result<(), Error> {
    if parsed.container.skin != sysinfo.skin {
        return Err(Error::Validation(format!(
            "Container is for Skin {:?}, but the connected Remote reports Skin {:?}",
            parsed.container.skin, sysinfo.skin
        )));
    }
    if parsed.devices.len() > profile.max_devices {
        return Err(Error::Validation(format!(
            "Container declares {} Devices; profile '{}' accepts at most {}",
            parsed.devices.len(),
            profile.id,
            profile.max_devices
        )));
    }
    if parsed.ir_groups > profile.max_ir_groups {
        return Err(Error::Validation(format!(
            "Container declares {} infrared groups; profile '{}' accepts at most {}",
            parsed.ir_groups, profile.id, profile.max_ir_groups
        )));
    }
    let skin = sysinfo
        .skin
        .and_then(|skin| profile::skin_profile(profile, skin));
    let skin = skin.ok_or_else(|| {
        Error::Validation(format!(
            "Container Skin {:?} is not accepted by profile '{}'",
            sysinfo.skin, profile.id
        ))
    })?;
    if parsed.favourites.len() > skin.favourite_capacity {
        return Err(Error::Validation(format!(
            "Container declares {} Favourites; Skin {} accepts at most {}",
            parsed.favourites.len(),
            skin.skin,
            skin.favourite_capacity
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
