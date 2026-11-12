use super::*;

/// Run `write --commit` against a Backup whose sidecar was rewritten by `mutate`.
#[cfg(feature = "cli")]
fn assert_write_refuses_mutated_sidecar(mutate: fn(&mut Value), expected: &str) {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, sample_container()).expect("container");
    let sidecar_path = PathBuf::from(format!("{}.json", backup.display()));
    let mut sidecar: Value =
        serde_json::from_slice(&std::fs::read(&sidecar_path).expect("sidecar")).expect("json");
    mutate(&mut sidecar);
    std::fs::write(
        &sidecar_path,
        serde_json::to_vec_pretty(&sidecar).expect("json"),
    )
    .expect("sidecar");

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

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains(expected), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::OpenWrite { .. })),
        "a refused Backup must never open the Configuration for writing"
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_sidecar_with_an_unsupported_schema_version() {
    assert_write_refuses_mutated_sidecar(
        |sidecar| sidecar["schema_version"] = 2.into(),
        "unsupported schema version",
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_sidecar_that_records_the_wrong_size() {
    assert_write_refuses_mutated_sidecar(|sidecar| sidecar["size"] = 1.into(), "records 1 bytes");
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_sidecar_whose_digest_does_not_match() {
    assert_write_refuses_mutated_sidecar(
        |sidecar| sidecar["sha256"] = "00".repeat(32).into(),
        "digest does not match",
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_an_unverified_backup() {
    assert_write_refuses_mutated_sidecar(
        |sidecar| sidecar["validation"] = "checksum_mismatch".into(),
        "refusing to write from an unverified Backup",
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_missing_backup() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, sample_container()).expect("container");
    let missing = directory.path().join("absent.bin");

    let mark = backend.calls().borrow().len();
    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &write_args(
            container_path.to_str().expect("path"),
            missing.to_str().expect("path"),
            true,
        ),
        directory.path(),
    );

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("Backup"), "{stderr}");
    assert!(stderr.contains("cannot be read"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::OpenWrite { .. }))
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_missing_sidecar() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, sample_container()).expect("container");
    std::fs::remove_file(format!("{}.json", backup.display())).expect("remove sidecar");

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

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stderr.contains("Backup sidecar"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::OpenWrite { .. }))
    );
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_sidecar_that_is_not_json() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, sample_container()).expect("container");
    std::fs::write(format!("{}.json", backup.display()), b"{ not a sidecar }").expect("sidecar");

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

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stderr.contains("is not valid JSON"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::OpenWrite { .. }))
    );
}
