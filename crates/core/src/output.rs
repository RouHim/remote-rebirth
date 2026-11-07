//! Durable file output that neither follows symlinks nor leaves a partial file behind.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::Error;

static NEXT_TEMPORARY: AtomicU64 = AtomicU64::new(0);

/// Write `bytes` to `path`, replacing it only once the whole file is on disk.
///
/// A symlink at `path` is refused instead of followed, so a user-run command cannot be tricked
/// into overwriting an unrelated file through a pre-created link. The temporary file is created
/// with `create_new` in the destination directory and renamed into place, which keeps the old
/// contents when anything fails.
pub(crate) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    if std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(Error::Validation(format!(
            "{} is a symlink; refusing to write through it",
            path.display()
        )));
    }
    let temporary = temporary_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|error| {
            Error::Io(std::io::Error::new(
                error.kind(),
                format!("{} cannot be written: {error}", path.display()),
            ))
        })?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error.into());
    }
    // Make the rename itself durable: the file's contents are synced, but the directory entry
    // only survives a power loss once the parent directory is synced too. The destination has
    // already been replaced, so this is best-effort: a directory that is write-only (or otherwise
    // unreadable) still holds the new contents, and reporting an error would claim the old file
    // survived when it did not.
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if let Err(error) = std::fs::File::open(parent).and_then(|directory| directory.sync_all()) {
        tracing::warn!(
            path = %path.display(),
            error = %error,
            "wrote the file but could not sync its parent directory"
        );
    }
    Ok(())
}

/// A sibling path that does not exist yet, so `create_new` cannot clobber another writer.
fn temporary_path(path: &Path) -> PathBuf {
    let name = path.file_name().unwrap_or_default();
    loop {
        let serial = NEXT_TEMPORARY.fetch_add(1, Ordering::Relaxed);
        let mut candidate = name.to_os_string();
        candidate.push(format!(".{}.{serial}.tmp", std::process::id()));
        let candidate = path.with_file_name(candidate);
        if std::fs::symlink_metadata(&candidate).is_err() {
            return candidate;
        }
    }
}
