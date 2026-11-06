#[cfg(feature = "cli")]
use std::io::Write;
use std::path::Path;

use crate::clock::Clock;
use crate::detect;
use crate::error::Error;
use crate::filewriter;
#[cfg(feature = "cli")]
use crate::filewriter::WritePlan;
use crate::journal::Journal;
use crate::progress::Progress;
#[cfg(feature = "cli")]
use crate::progress::ignore;
use crate::transport::RemoteBackend;
use crate::write::{self, WriteReport};
#[cfg(feature = "cli")]
use crate::write::{Options, WriteOutput};

/// A prepared or committed Restore without any CLI formatting.
pub fn perform(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    backup_path: &Path,
    commit: bool,
    no_reboot: bool,
    progress: &mut dyn FnMut(Progress),
) -> Result<WriteReport, Error> {
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
        return Ok(WriteReport::new(
            filewriter::Operation::Restore.as_str(),
            false,
            None,
            backup_path,
            backup.len(),
            &sidecar.sha256,
            write_plan,
            no_reboot,
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

    Ok(WriteReport::new(
        filewriter::Operation::Restore.as_str(),
        true,
        None,
        backup_path,
        backup.len(),
        &sidecar.sha256,
        write_plan,
        no_reboot,
        Some(journal.path()),
    ))
}

#[cfg(feature = "cli")]
pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    backup_path: &Path,
    options: Options,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let outcome = perform(
        backend,
        clock,
        state_dir,
        backup_path,
        options.commit,
        options.no_reboot,
        &mut ignore,
    )?;
    let plan = WritePlan {
        bytes: outcome.bytes,
        chunks: outcome.chunks,
        data_packets: outcome.data_packets,
    };
    write::report(
        options.json,
        stdout,
        &WriteOutput::new(
            outcome.operation,
            outcome.commit,
            None,
            &outcome.backup,
            outcome.size,
            &outcome.backup_sha256,
            plan,
            !outcome.reboot,
            outcome.journal.as_deref(),
        ),
    )
}
