use std::path::{Path, PathBuf};

use crate::clock::Clock;
use crate::journal::{JOURNAL_FILE, Journal};

struct FixedClock(time::OffsetDateTime);

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0
    }
}

fn clock() -> FixedClock {
    FixedClock(time::OffsetDateTime::from_unix_timestamp(1_800_000_000).expect("a fixed timestamp"))
}

fn backup() -> PathBuf {
    PathBuf::from("/backups/2026-11-05-original.bin")
}

fn records(state_dir: &Path) -> Vec<serde_json::Value> {
    let text = std::fs::read_to_string(state_dir.join(JOURNAL_FILE)).expect("journal");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("every complete line is a record"))
        .collect()
}

#[test]
fn journal_appends_every_operation_without_rewriting_earlier_records() {
    let directory = tempfile::tempdir().expect("tempdir");
    let clock = clock();
    {
        let mut journal =
            Journal::create(directory.path(), &clock, "write", &backup(), "d1b78d77a279")
                .expect("journal");
        journal.record("prepare", 0, 0, "ok").expect("prepare");
        journal.record("chunk", 0, 3200, "ok").expect("chunk");
    }
    {
        let mut journal = Journal::create(
            directory.path(),
            &clock,
            "restore",
            &backup(),
            "d1b78d77a279",
        )
        .expect("journal");
        journal
            .record("read_back", 0, 3200, "mismatch")
            .expect("read_back");
    }

    let records = records(directory.path());
    assert_eq!(records.len(), 3, "one record per acknowledged step");
    assert_eq!(records[0]["command"], "write");
    assert_eq!(records[0]["step"], "prepare");
    assert_eq!(records[1]["step"], "chunk");
    assert_eq!(records[1]["bytes"], 3200);
    assert_eq!(records[2]["command"], "restore");
    assert_eq!(records[2]["reply"], "mismatch");
    assert_ne!(
        records[0]["operation_id"], records[2]["operation_id"],
        "each operation gets its own identity"
    );
}

#[test]
fn journal_starts_a_new_line_after_a_truncated_tail() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = directory.path().join(JOURNAL_FILE);
    std::fs::write(&path, b"{\"operation_id\":\"write-partial\",\"step\":").expect("partial tail");
    let clock = clock();

    let mut journal = Journal::create(directory.path(), &clock, "write", &backup(), "d1b78d77a279")
        .expect("journal");
    journal.record("prepare", 0, 0, "ok").expect("prepare");

    let text = std::fs::read_to_string(&path).expect("journal");
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2, "the truncated tail stays on its own line");
    assert_eq!(lines[0], "{\"operation_id\":\"write-partial\",\"step\":");
    let appended: serde_json::Value = serde_json::from_str(lines[1]).expect("a complete record");
    assert_eq!(appended["step"], "prepare");
}

#[test]
fn journal_starts_a_new_line_after_a_complete_record() {
    let directory = tempfile::tempdir().expect("tempdir");
    let clock = clock();
    {
        let mut journal =
            Journal::create(directory.path(), &clock, "write", &backup(), "d1b78d77a279")
                .expect("journal");
        journal.record("prepare", 0, 0, "ok").expect("prepare");
    }
    let mut journal = Journal::create(directory.path(), &clock, "write", &backup(), "d1b78d77a279")
        .expect("journal");
    journal.record("chunk", 0, 3200, "ok").expect("chunk");

    let records = records(directory.path());
    assert_eq!(records.len(), 2);
    assert_eq!(records[1]["step"], "chunk");
}
