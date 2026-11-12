use super::*;

#[cfg(feature = "cli")]
#[test]
fn write_dry_run_validates_without_opening_for_write() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            false,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["operation"].as_str(), Some("write"));
    assert_eq!(value["commit"].as_bool(), Some(false));
    assert_eq!(value["chunks"].as_u64(), Some(1));
    assert_eq!(value["data_packets"].as_u64(), Some(11));
    assert_eq!(value["bytes"].as_u64(), Some(670));
    assert!(value["journal"].is_null());

    let calls = backend.calls().borrow().clone();
    assert!(no_write_opened(&calls[mark..]));
    assert_eq!(
        backend.store(0).borrow().usercfg,
        container[..container.len() - 4].to_vec()
    );
    assert!(!directory.path().join("journal.jsonl").exists());
}

#[cfg(feature = "cli")]
#[test]
fn write_dry_run_rejects_invalid_container_checksum() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let mut container = sample_container();
    container[10] ^= 0xFF;
    let container_path = directory.path().join("broken.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            false,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("checksum"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}

#[cfg(feature = "cli")]
#[test]
fn write_rejects_oversized_container() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container_sized(container::MAX_CONTAINER_SIZE + 1);
    let container_path = directory.path().join("large.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("maximum"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_unexpected_current_configuration() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    let mut current = sample_container();
    current[10] = 0x42;
    backend.store(0).borrow_mut().usercfg = current[..current.len() - 4].to_vec();

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("does not match Backup"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
    assert_eq!(
        backend.store(0).borrow().usercfg,
        current[..current.len() - 4].to_vec()
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_allows_empty_current_configuration() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().usercfg = vec![0xFF; 666];

    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            false,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_backup_from_another_skin() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().sysinfo = sysinfo(78, 16);

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("Skin"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_unsupported_remote() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(104, 0x1104, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(3));
    assert!(stderr.contains("not supported"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_writes_checks_flushes_reads_back_and_reboots() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["commit"].as_bool(), Some(true));
    assert_eq!(value["chunks"].as_u64(), Some(1));
    assert_eq!(value["data_packets"].as_u64(), Some(11));
    assert!(
        value["journal"]
            .as_str()
            .is_some_and(|path| path.ends_with("journal.jsonl"))
    );

    let calls = backend.calls().borrow().clone();
    let tail = &calls[mark..];
    let reset = position_of(tail, |call| matches!(call, Call::Reset));
    let open = position_of(tail, |call| matches!(call, Call::OpenWrite { .. }));
    let write = position_of(tail, |call| matches!(call, Call::WriteChunk { .. }));
    let check = position_of(tail, |call| matches!(call, Call::Check { .. }));
    let flush = position_of(tail, |call| matches!(call, Call::Flush { .. }));
    let close = tail
        .iter()
        .position(|call| matches!(call, Call::Close { .. }))
        .expect("close");
    let write_close = tail
        .iter()
        .skip(flush)
        .position(|call| matches!(call, Call::Close { .. }))
        .expect("close after flush")
        + flush;
    let reboot = position_of(tail, |call| matches!(call, Call::Reboot));
    assert!(reset < open && open < write && write < check);
    assert!(check < flush && flush < write_close && write_close < reboot);
    assert!(close < reset);

    assert!(tail.iter().any(|call| matches!(
        call,
        Call::OpenWrite { path, size } if path == "/cfg/usercfg" && *size == 670
    )));
    let expected = container::stored_checksum(&container).expect("checksum");
    assert!(tail.iter().any(|call| matches!(
        call,
        Call::Check { expected: recorded, covered_len, .. }
            if *recorded == expected && *covered_len == 664
    )));
    let written: Vec<u8> = tail
        .iter()
        .filter_map(|call| match call {
            Call::WriteChunk { data, last, .. } => {
                assert!(*last, "every chunk ends with the done packet");
                assert!(data.len() <= filewriter::CHUNK_BYTES);
                Some(data.clone())
            }
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(written, container);
    assert_eq!(backend.store(0).borrow().usercfg, container);

    let records = journal_records(directory.path());
    let steps: Vec<&str> = records
        .iter()
        .filter_map(|record| record["step"].as_str())
        .collect();
    assert_eq!(
        steps,
        vec![
            "prepare",
            "reset",
            "open_write",
            "write",
            "check",
            "flush",
            "close",
            "read_back",
            "reboot",
        ]
    );
    for record in &records {
        assert_eq!(record["command"].as_str(), Some("write"));
        assert!(
            !record["operation_id"]
                .as_str()
                .unwrap_or_default()
                .is_empty()
        );
        assert!(
            record["timestamp"]
                .as_str()
                .is_some_and(|value| value.ends_with('Z'))
        );
        assert_eq!(record["backup"].as_str(), backup.to_str());
        assert_eq!(
            record["backup_sha256"].as_str(),
            Some(container::sha256_hex(&sample_container()).as_str())
        );
        assert!(record["offset"].is_u64());
        assert!(record["bytes"].is_u64());
    }
    let check_record = records
        .iter()
        .find(|record| record["step"] == "check")
        .expect("check record");
    assert_eq!(check_record["reply"].as_str(), Some("m"));
    let read_back = records
        .iter()
        .find(|record| record["step"] == "read_back")
        .expect("read_back record");
    assert_eq!(read_back["reply"].as_str(), Some("ok"));
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_splits_a_large_container_into_chunks() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container_sized(7000);
    let container_path = directory.path().join("large.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["chunks"].as_u64(), Some(3));
    assert_eq!(value["data_packets"].as_u64(), Some(113));
    assert_eq!(value["bytes"].as_u64(), Some(7000));

    let calls = backend.calls().borrow().clone();
    let chunk_sizes: Vec<(usize, bool)> = calls[mark..]
        .iter()
        .filter_map(|call| match call {
            Call::WriteChunk { data, last, .. } => Some((data.len(), *last)),
            _ => None,
        })
        .collect();
    assert_eq!(chunk_sizes, vec![(3100, false), (3100, false), (800, true)]);
    assert_eq!(backend.store(0).borrow().usercfg, container);
    assert!(calls[mark..].iter().any(|call| matches!(
        call,
        Call::Check { covered_len, .. } if *covered_len == 6994
    )));
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_with_no_reboot_skips_reboot() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &[
            "write",
            "--container",
            container_path.to_str().expect("path"),
            "--backup",
            backup.to_str().expect("path"),
            "--commit",
            "--no-reboot",
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["reboot"].as_bool(), Some(false));
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::Reboot))
    );
    let records = journal_records(directory.path());
    let reboot = records
        .iter()
        .find(|record| record["step"] == "reboot")
        .expect("reboot record");
    assert_eq!(reboot["reply"].as_str(), Some("skipped"));
}

#[cfg(feature = "cli")]
#[test]
fn write_run_reports_each_stage_to_its_progress_sink() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, sample_container()).expect("container");
    let mut stdout = Vec::new();
    let mut events: Vec<Progress> = Vec::new();

    crate::write::run(
        &mut backend,
        &FixedClock(time::OffsetDateTime::from_unix_timestamp(1_800_000_000).unwrap()),
        crate::write::WriteRequest {
            state_dir: directory.path(),
            container_path: &container_path,
            backup_path: &backup,
            commit: false,
            no_reboot: false,
        },
        true,
        &mut stdout,
        &mut |event| events.push(event),
    )
    .expect("dry run");

    assert!(stdout.starts_with(b"{"), "the report still goes to stdout");
    assert_eq!(events.first(), Some(&Progress::Step("load")));
    assert!(
        events
            .iter()
            .any(|event| matches!(event, Progress::Step("detect"))),
        "detect is reported: {events:?}"
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_checksum_refusal_skips_flush() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().faults.check_refused = true;

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(5), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("not flushed"), "{stderr}");
    assert!(stderr.contains("may be erased"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    let tail = &calls[mark..];
    assert!(!tail.iter().any(|call| matches!(call, Call::Flush { .. })));
    assert!(!tail.iter().any(|call| matches!(call, Call::Reboot)));
    assert_eq!(backend.store(0).borrow().usercfg, vec![0xFF; 670]);

    let records = journal_records(directory.path());
    let check = records
        .iter()
        .find(|record| record["step"] == "check")
        .expect("check record");
    assert_eq!(check["reply"].as_str(), Some("u"));
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_read_back_mismatch_exits_6_and_skips_reboot() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().faults.read_back_mismatch = true;

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(6), "{stderr}");
    assert!(stderr.contains("Read-Back mismatch"), "{stderr}");
    assert!(stderr.contains("restore --backup"), "{stderr}");
    assert!(stderr.contains(backup.to_str().expect("path")), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::Reboot))
    );
    let records = journal_records(directory.path());
    let read_back = records
        .iter()
        .find(|record| record["step"] == "read_back")
        .expect("read_back record");
    assert_eq!(read_back["reply"].as_str(), Some("mismatch"));
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_disconnect_journals_and_reopens_session() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().faults.disconnect = true;

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(5), "{stderr}");
    let calls = backend.calls().borrow().clone();
    let tail = &calls[mark..];
    let opens = tail
        .iter()
        .filter(|call| matches!(call, Call::Open { .. }))
        .count();
    assert_eq!(opens, 2, "a protocol failure reopens a fresh session");
    assert!(!tail.iter().any(|call| matches!(call, Call::Flush { .. })));
    assert_eq!(backend.store(0).borrow().usercfg, vec![0xFF; 670]);
    let records = journal_records(directory.path());
    let steps: Vec<&str> = records
        .iter()
        .filter_map(|record| record["step"].as_str())
        .collect();
    assert_eq!(
        steps,
        vec!["prepare", "reset", "open_write", "failure", "recover"],
        "the failure and the reopened session are both journalled"
    );
    assert_eq!(
        records
            .iter()
            .find(|record| record["step"] == "failure")
            .and_then(|record| record["reply"].as_str()),
        Some("protocol")
    );
    assert_eq!(
        records
            .iter()
            .find(|record| record["step"] == "recover")
            .and_then(|record| record["reply"].as_str()),
        Some("ok")
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_commit_short_read_back_exits_5() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().faults.short_read = true;

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            backup.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(5), "{stderr}");
    assert!(stderr.contains("short read"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::Reboot))
    );
}

#[test]
fn write_plan_covers_chunk_and_sequence_boundaries() {
    assert_eq!(
        filewriter::plan(0),
        WritePlan {
            bytes: 0,
            chunks: 0,
            data_packets: 0
        }
    );
    assert_eq!(
        filewriter::plan(124),
        WritePlan {
            bytes: 124,
            chunks: 1,
            data_packets: 2
        }
    );
    assert_eq!(filewriter::plan(3100).chunks, 1);
    assert_eq!(filewriter::plan(3101).chunks, 2);
    assert_eq!(filewriter::plan(3101).data_packets, 51);
    assert_eq!(filewriter::plan(62).data_packets, 1);
    assert_eq!(filewriter::plan(63).data_packets, 2);

    let mut sequence = crate::protocol::Sequence::new();
    let values: Vec<u8> = (0..130).map(|_| sequence.take()).collect();
    assert_eq!(values[0], 0);
    assert_eq!(values[63], 63);
    assert_eq!(values[64], 0);
    assert_eq!(values[127], 63);
    assert_eq!(values[128], 0);
}
