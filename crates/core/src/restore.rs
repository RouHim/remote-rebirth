#[cfg(feature = "cli")]
use std::io::Write;
use std::path::Path;

use crate::clock::Clock;
use crate::detect;
use crate::error::Error;
use crate::filewriter;
use crate::journal::Journal;
use crate::progress::Progress;
use crate::transport::RemoteBackend;
#[cfg(feature = "cli")]
use crate::write::WriteOutput;
use crate::write::{self, WriteReport};

/// The paths and flags that identify one Restore.
#[derive(Debug, Clone, Copy)]
pub struct RestoreRequest<'a> {
    pub state_dir: &'a Path,
    pub backup_path: &'a Path,
    pub commit: bool,
    pub no_reboot: bool,
}

/// A prepared or committed Restore without any CLI formatting.
pub fn perform(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    request: RestoreRequest<'_>,
    progress: &mut dyn FnMut(Progress),
) -> Result<WriteReport, Error> {
    let RestoreRequest {
        state_dir,
        backup_path,
        commit,
        no_reboot,
    } = request;
    let span = tracing::info_span!(
        "restore",
        backup = %backup_path.display(),
        commit
    );
    let _entered = span.enter();

    progress(Progress::Step("load"));
    let (backup, sidecar) = write::load_backup(backup_path)?;
    let parsed = write::validate_container(&backup)?;

    progress(Progress::Step("detect"));
    let detect::OpenedRemote {
        descriptor,
        sysinfo,
        session,
    } = detect::open_remote(backend)?;
    let profile = write::check_remote(&sysinfo, &sidecar)?;
    write::check_container(&parsed, &sysinfo, profile)?;

    let write_plan = filewriter::plan(backup.len());

    if !commit {
        return Ok(write::restore_report(
            &request,
            false,
            &sidecar.sha256,
            backup.len(),
            write_plan,
            None,
        ));
    }

    let commit_request = filewriter::Commit {
        operation: filewriter::Operation::Restore,
        container: &backup,
        backup: backup_path,
        no_reboot,
    };
    progress(Progress::Step("prepare"));
    let mut journal = Journal::create(state_dir, clock, "restore", backup_path, &sidecar.sha256)?;
    journal.record("prepare", 0, backup.len(), "ok")?;

    tracing::info!("committed Restore starting");
    progress(Progress::Step("commit"));
    write::commit_container(
        backend,
        &descriptor,
        session,
        &mut journal,
        &commit_request,
        progress,
    )?;
    tracing::info!("committed Restore finished and verified");

    Ok(write::restore_report(
        &request,
        true,
        &sidecar.sha256,
        backup.len(),
        write_plan,
        Some(journal.path()),
    ))
}

#[cfg(feature = "cli")]
pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    request: RestoreRequest<'_>,
    json: bool,
    stdout: &mut dyn Write,
    progress: &mut dyn FnMut(crate::Progress),
) -> Result<(), Error> {
    let outcome = perform(backend, clock, request, progress)?;
    write::report(json, stdout, &WriteOutput::from_report(&outcome))
}
