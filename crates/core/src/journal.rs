use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::clock::{Clock, format_rfc3339};
use crate::error::Error;

pub const JOURNAL_FILE: &str = "journal.jsonl";

/// The append-only record of acknowledged Write and Restore steps.
///
/// Every record carries the operation identity, the command, the step, the
/// offset and byte count it covered, the reply status, and the Backup that the
/// operation can be recovered from. A failed operation therefore leaves the
/// last acknowledged step at the tail of the file.
pub struct Journal<'a> {
    file: File,
    path: PathBuf,
    clock: &'a dyn Clock,
    operation_id: String,
    command: &'a str,
    backup: String,
    backup_sha256: String,
}

impl<'a> Journal<'a> {
    pub fn create(
        state_dir: &Path,
        clock: &'a dyn Clock,
        command: &'a str,
        backup: &Path,
        backup_sha256: &str,
    ) -> Result<Self, Error> {
        fs::create_dir_all(state_dir)?;
        let path = state_dir.join(JOURNAL_FILE);
        let mut file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(&path)?;
        // An earlier run may have died mid-record. Start the first new record on its own line so a
        // truncated tail cannot swallow it.
        if file.metadata()?.len() > 0 {
            let mut last = [0u8; 1];
            file.seek(SeekFrom::End(-1))?;
            file.read_exact(&mut last)?;
            if last[0] != b'\n' {
                file.write_all(b"\n")?;
                file.sync_data()?;
            }
        }
        let operation_id = format!("{command}-{}", format_rfc3339(&clock.now())?);
        Ok(Self {
            file,
            path,
            clock,
            operation_id,
            command,
            backup: backup.display().to_string(),
            backup_sha256: backup_sha256.to_string(),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record(
        &mut self,
        step: &str,
        offset: usize,
        bytes: usize,
        reply: &str,
    ) -> Result<(), Error> {
        let record = Record {
            operation_id: &self.operation_id,
            timestamp: format_rfc3339(&self.clock.now())?,
            command: self.command,
            step,
            offset: offset as u64,
            bytes: bytes as u64,
            reply,
            backup: &self.backup,
            backup_sha256: &self.backup_sha256,
        };
        let mut line = serde_json::to_vec(&record)?;
        line.push(b'\n');
        self.file.write_all(&line)?;
        // `flush` on a File is a no-op; the page cache is lost on power failure, which is exactly
        // when the last acknowledged step has to survive.
        self.file.sync_data()?;
        Ok(())
    }
}

#[derive(Debug, Serialize)]
struct Record<'a> {
    operation_id: &'a str,
    timestamp: String,
    command: &'a str,
    step: &'a str,
    offset: u64,
    bytes: u64,
    reply: &'a str,
    backup: &'a str,
    backup_sha256: &'a str,
}
