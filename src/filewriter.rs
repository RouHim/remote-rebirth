use std::path::Path;

use serde::Serialize;

use crate::container;
use crate::error::Error;
use crate::journal::Journal;
use crate::protocol::DATA_PACKET_PAYLOAD;
use crate::transport::{RemoteSession, read_file};

/// At most 50 data packets travel in one write chunk.
pub const CHUNK_DATA_PACKETS: usize = 50;
/// Payload bytes carried by one write chunk at 62 bytes per data packet.
pub const CHUNK_BYTES: usize = CHUNK_DATA_PACKETS * DATA_PACKET_PAYLOAD;

const CHECK_SEED: u16 = 0x4321;
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

/// The bytes the Remote stores for a Container: everything before `PTYY`.
pub fn stored_bytes(container: &[u8]) -> &[u8] {
    let trailer = container::END_MARKER.len();
    if container.len() <= trailer {
        return &[];
    }
    &container[..container.len() - trailer]
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

/// Commit a Container to the Remote.
///
/// The sequence matches the vendor client: reset, open for writing, chunked
/// writes, checksum check, flush only on `m`, close, Read-Back, reboot. The
/// caller records the prepare step before this function runs, because opening
/// for writing erases the configuration region.
pub fn execute(
    operation: Operation,
    session: &mut dyn RemoteSession,
    journal: &mut Journal<'_>,
    container: &[u8],
    backup: &Path,
    no_reboot: bool,
) -> Result<WritePlan, Error> {
    let payload = stored_bytes(container);
    let covered_len = container.len().saturating_sub(6);
    let expected = container::stored_checksum(container)
        .ok_or_else(|| Error::Validation("Container is shorter than its trailer".into()))?;

    session.reset_filesystem()?;
    journal.record("reset", 0, 0, "ok")?;

    let handle = session.open_write(CONFIG_PATH, payload.len() as u32)?;
    journal.record("open_write", 0, payload.len(), "ok")?;

    let result = commit_chunks(
        session,
        journal,
        container,
        payload,
        handle,
        expected,
        covered_len,
        backup,
        operation,
        no_reboot,
    );
    if result.is_err() {
        // A refused check or a protocol failure leaves the session poisoned;
        // closing the handle is best effort and the caller reopens a fresh one.
        let _ = session.close(handle);
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn commit_chunks(
    session: &mut dyn RemoteSession,
    journal: &mut Journal<'_>,
    container: &[u8],
    payload: &[u8],
    handle: u8,
    expected: u16,
    covered_len: usize,
    backup: &Path,
    operation: Operation,
    no_reboot: bool,
) -> Result<WritePlan, Error> {
    let write_plan = plan(payload.len());
    let mut offset = 0usize;
    for chunk in payload.chunks(CHUNK_BYTES) {
        session.write_chunk(handle, chunk, true)?;
        journal.record("write", offset, chunk.len(), "done")?;
        offset += chunk.len();
    }

    let accepted = session.check(handle, expected, covered_len as u32)?;
    journal.record("check", 0, covered_len, if accepted { "m" } else { "u" })?;
    if !accepted {
        return Err(Error::Protocol(format!(
            "checksum check {} for a Container using seed 0x{CHECK_SEED:04X} returned u; the \
             Configuration was not flushed and the Remote may be erased; run \
             `restore --backup {} --commit`",
            expected,
            backup.display()
        )));
    }

    session.flush(handle)?;
    journal.record("flush", 0, 0, "ok")?;
    session.close(handle)?;
    journal.record("close", 0, 0, "ok")?;

    let read = read_file(session, CONFIG_PATH)?;
    let read = container::reconstruct_trailer(&read);
    if read != container {
        journal.record("read_back", 0, read.len(), "mismatch")?;
        return Err(Error::Verification(mismatch_message(
            operation, backup, &read, container,
        )));
    }
    journal.record("read_back", 0, read.len(), "ok")?;

    if no_reboot {
        journal.record("reboot", 0, 0, "skipped")?;
    } else {
        session.reboot()?;
        journal.record("reboot", 0, 0, "ok")?;
    }

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
