use std::io::Write;
use std::path::Path;

use crate::clock::Clock;
use crate::detect;
use crate::error::Error;
use crate::filewriter;
use crate::journal::Journal;
use crate::transport::{RemoteBackend, RemoteSelector};
use crate::write::{self, WriteOutput};

#[allow(clippy::too_many_arguments)]
pub fn run(
    backend: &mut dyn RemoteBackend,
    clock: &dyn Clock,
    state_dir: &Path,
    backup_path: &Path,
    commit: bool,
    no_reboot: bool,
    json: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    let (backup, sidecar) = write::load_backup(backup_path)?;
    write::validate_container(&backup)?;

    let detect::OpenedRemote {
        descriptor,
        sysinfo,
        mut session,
    } = detect::open_remote(backend)?;
    write::check_remote(&sysinfo, &sidecar)?;

    let write_plan = filewriter::plan(filewriter::stored_bytes(&backup).len());

    if !commit {
        write::report(
            json,
            stdout,
            &WriteOutput::new(
                filewriter::Operation::Restore.as_str(),
                false,
                None,
                backup_path,
                backup.len(),
                &sidecar.sha256,
                write_plan,
                no_reboot,
                None,
            ),
        )?;
        return Ok(());
    }

    let mut journal = Journal::create(state_dir, clock, "restore", backup_path, &sidecar.sha256)?;
    journal.record("prepare", 0, filewriter::stored_bytes(&backup).len(), "ok")?;

    let result = filewriter::execute(
        filewriter::Operation::Restore,
        &mut *session,
        &mut journal,
        &backup,
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

    write::report(
        json,
        stdout,
        &WriteOutput::new(
            filewriter::Operation::Restore.as_str(),
            true,
            None,
            backup_path,
            backup.len(),
            &sidecar.sha256,
            write_plan,
            no_reboot,
            Some(journal.path()),
        ),
    )
}
