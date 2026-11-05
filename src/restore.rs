use std::io::Write;
use std::path::Path;

use crate::clock::Clock;
use crate::detect;
use crate::error::Error;
use crate::filewriter;
use crate::journal::Journal;
use crate::transport::RemoteBackend;
use crate::write::{self, Options, WriteOutput};

pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    backup_path: &Path,
    options: Options,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let span = tracing::info_span!(
        "restore",
        backup = %backup_path.display(),
        commit = options.commit
    );
    let _entered = span.enter();

    let (backup, sidecar) = write::load_backup(backup_path)?;
    let parsed = write::validate_container(&backup)?;

    let detect::OpenedRemote {
        descriptor,
        sysinfo,
        session,
    } = detect::open_remote(backend)?;
    let profile = write::check_remote(&sysinfo, &sidecar)?;
    write::check_container(&parsed, &sysinfo, profile)?;

    let write_plan = filewriter::plan(filewriter::stored_bytes(&backup).len());

    if !options.commit {
        write::report(
            options.json,
            stdout,
            &WriteOutput::new(
                filewriter::Operation::Restore.as_str(),
                false,
                None,
                backup_path,
                backup.len(),
                &sidecar.sha256,
                write_plan,
                options.no_reboot,
                None,
            ),
        )?;
        return Ok(());
    }

    let commit = filewriter::Commit {
        operation: filewriter::Operation::Restore,
        container: &backup,
        backup: backup_path,
        no_reboot: options.no_reboot,
    };
    let mut journal = Journal::create(state_dir, clock, "restore", backup_path, &sidecar.sha256)?;
    journal.record("prepare", 0, filewriter::stored_bytes(&backup).len(), "ok")?;

    tracing::info!("committed Restore starting");
    write::commit_container(backend, &descriptor, session, &mut journal, &commit)?;
    tracing::info!("committed Restore finished and verified");

    write::report(
        options.json,
        stdout,
        &WriteOutput::new(
            filewriter::Operation::Restore.as_str(),
            true,
            None,
            backup_path,
            backup.len(),
            &sidecar.sha256,
            write_plan,
            options.no_reboot,
            Some(journal.path()),
        ),
    )
}
