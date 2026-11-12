use super::*;

#[cfg(feature = "cli")]
#[test]
fn restore_dry_run_validates_without_opening_for_write() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &[
            "restore",
            "--backup",
            backup.to_str().expect("path"),
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["operation"].as_str(), Some("restore"));
    assert_eq!(value["commit"].as_bool(), Some(false));
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
    assert!(!directory.path().join("journal.jsonl").exists());
}

#[cfg(feature = "cli")]
#[test]
fn restore_refuses_backup_from_another_skin_before_writing() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    backend.store(0).borrow_mut().sysinfo = sysinfo(78, 16);

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &[
            "restore",
            "--backup",
            backup.to_str().expect("path"),
            "--commit",
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("Skin"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}

#[cfg(feature = "cli")]
#[test]
fn restore_commit_writes_backup_and_reboots() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let expected = sample_container();

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &[
            "restore",
            "--backup",
            backup.to_str().expect("path"),
            "--commit",
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["commit"].as_bool(), Some(true));
    assert_eq!(backend.store(0).borrow().usercfg, expected);
    let calls = backend.calls().borrow().clone();
    assert!(
        calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::Reboot))
    );
    let records = journal_records(directory.path());
    assert!(
        records
            .iter()
            .all(|record| record["command"].as_str() == Some("restore"))
    );
    assert_eq!(
        records.first().and_then(|record| record["step"].as_str()),
        Some("prepare")
    );
}

#[cfg(feature = "cli")]
#[test]
fn restore_commit_with_no_reboot_skips_reboot() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let expected = sample_container();

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &[
            "restore",
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
    assert_eq!(value["commit"].as_bool(), Some(true));
    assert_eq!(value["reboot"].as_bool(), Some(false));
    assert_eq!(backend.store(0).borrow().usercfg, expected);
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::Reboot)),
        "--no-reboot must suppress the reboot"
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
fn restore_commit_read_back_mismatch_exits_6() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    backend.store(0).borrow_mut().faults.read_back_mismatch = true;

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &[
            "restore",
            "--backup",
            backup.to_str().expect("path"),
            "--commit",
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(6), "{stderr}");
    assert!(stderr.contains("Read-Back mismatch"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::Reboot))
    );
}
