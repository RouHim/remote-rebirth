use std::path::Path;

use serde::Serialize;

use crate::container;
use crate::error::Error;
use crate::journal::Journal;
use crate::progress::Progress;
use crate::protocol::DATA_PACKET_PAYLOAD;
use crate::transport::{RemoteSession, read_file};

/// At most 50 data packets travel in one write chunk.
pub const CHUNK_DATA_PACKETS: usize = 50;
/// Payload bytes carried by one write chunk at 62 bytes per data packet.
pub const CHUNK_BYTES: usize = CHUNK_DATA_PACKETS * DATA_PACKET_PAYLOAD;

const CONFIG_PATH: &str = "/cfg/usercfg";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WritePlan {
    pub bytes: usize,
    pub chunks: usize,
    pub data_packets: usize,
}

pub fn plan(bytes: usize) -> WritePlan {
    WritePlan {
        bytes,
        chunks: bytes.div_ceil(CHUNK_BYTES),
        data_packets: bytes.div_ceil(DATA_PACKET_PAYLOAD),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Write,
    Restore,
}

impl Operation {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Restore => "restore",
        }
    }
}

/// Everything one committed Write or Restore states besides the session and the Journal.
#[derive(Debug, Clone, Copy)]
pub struct Commit<'a> {
    pub operation: Operation,
    /// The Container, complete with its `PTYY` marker.
    pub container: &'a [u8],
    /// The recovery Backup a failure names.
    pub backup: &'a Path,
    pub no_reboot: bool,
}

/// Commit a Container to the Remote.
///
/// The sequence matches the vendor client: reset, open for writing, chunked
/// writes, checksum check, flush only on `m`, close, Read-Back, reboot. The
/// caller records the prepare step before this function runs, because opening
/// for writing erases the configuration region.
/// Commit a Container, reporting each protocol stage to `progress`.
pub fn execute_with_progress(
    commit: &Commit<'_>,
    session: &mut dyn RemoteSession,
    journal: &mut Journal<'_>,
    progress: &mut dyn FnMut(Progress),
) -> Result<WritePlan, Error> {
    let covered_len = commit.container.len().saturating_sub(6);
    let expected = container::stored_checksum(commit.container)
        .ok_or_else(|| Error::Validation("Container is shorter than its trailer".into()))?;

    progress(Progress::Step("reset"));
    session.reset_filesystem()?;
    journal.record("reset", 0, 0, "ok")?;

    progress(Progress::Step("open"));
    let handle = session.open_write(CONFIG_PATH, commit.container.len() as u32)?;
    journal.record("open_write", 0, commit.container.len(), "ok")?;

    let result = commit_chunks(
        commit,
        session,
        journal,
        handle,
        expected,
        covered_len,
        progress,
    );
    if result.is_err() {
        // A refused check or a protocol failure leaves the session poisoned;
        // closing the handle is best effort and the caller reopens a fresh one.
        let _ = session.close(handle);
    }
    result
}

fn commit_chunks(
    commit: &Commit<'_>,
    session: &mut dyn RemoteSession,
    journal: &mut Journal<'_>,
    handle: u8,
    expected: u16,
    covered_len: usize,
    progress: &mut dyn FnMut(Progress),
) -> Result<WritePlan, Error> {
    let write_plan = plan(commit.container.len());
    let chunks: Vec<&[u8]> = commit.container.chunks(CHUNK_BYTES).collect();
    let total = commit.container.len();
    let mut offset = 0usize;
    for (index, chunk) in chunks.iter().enumerate() {
        let last = index + 1 == chunks.len();
        session.write_chunk(handle, chunk, last)?;
        journal.record("write", offset, chunk.len(), "done")?;
        offset += chunk.len();
        progress(Progress::Written {
            bytes: offset,
            total,
        });
    }

    progress(Progress::Step("check"));
    let accepted = session.check(handle, expected, covered_len as u32)?;
    journal.record("check", 0, covered_len, if accepted { "m" } else { "u" })?;
    if !accepted {
        return Err(Error::Protocol(format!(
            "checksum check {expected} for a Container using seed 0x{:04X} returned u; the \
             Configuration was not flushed and the Remote may be erased; run \
             `restore --backup {} --commit`",
            container::TRAILER_CHECKSUM_SEED,
            commit.backup.display()
        )));
    }

    progress(Progress::Step("flush"));
    session.flush(handle)?;
    journal.record("flush", 0, 0, "ok")?;
    progress(Progress::Step("close"));
    session.close(handle)?;
    journal.record("close", 0, 0, "ok")?;

    progress(Progress::Step("read-back"));
    let read = read_file(session, CONFIG_PATH)?;
    let read = container::reconstruct_trailer(&read);
    if read != commit.container {
        journal.record("read_back", 0, read.len(), "mismatch")?;
        return Err(Error::Verification(mismatch_message(
            commit.operation,
            commit.backup,
            &read,
            commit.container,
        )));
    }
    journal.record("read_back", 0, read.len(), "ok")?;

    if commit.no_reboot {
        journal.record("reboot", 0, 0, "skipped")?;
    } else {
        progress(Progress::Step("reboot"));
        session.reboot()?;
        journal.record("reboot", 0, 0, "ok")?;
    }

    progress(Progress::Step("done"));
    Ok(write_plan)
}

fn mismatch_message(operation: Operation, backup: &Path, read: &[u8], expected: &[u8]) -> String {
    match operation {
        Operation::Write => format!(
            "Read-Back mismatch after writing: the Remote holds {} bytes instead of {}; the \
             reboot was skipped. The Backup {} is intact; run `restore --backup {} --commit` \
             to return to it",
            read.len(),
            expected.len(),
            backup.display(),
            backup.display()
        ),
        Operation::Restore => format!(
            "Read-Back mismatch after restoring: the Remote holds {} bytes instead of {}; the \
             reboot was skipped. Backup {} is intact; re-run `restore --backup {} --commit` \
             after inspecting the device",
            read.len(),
            expected.len(),
            backup.display(),
            backup.display()
        ),
    }
}
