use super::*;

// ---------------------------------------------------------------------------
// Code-review findings: detection, overwrite, schema exposure, clap, profiles.
// ---------------------------------------------------------------------------

#[cfg(feature = "cli")]
fn non_harmony_device() -> FakeDevice {
    FakeDevice {
        descriptor: descriptor(1, 5, 0xC52B, 0x0100, "USB Receiver"),
        sysinfo: Vec::new(),
        usercfg: Vec::new(),
    }
}

#[cfg(feature = "cli")]
#[test]
fn detect_reports_the_family_from_bcd_device() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(
        vec![device(78, 0x1078, 0xC124)],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["family"].as_str(), Some("Harmony 300"));
    assert_eq!(value["skin"].as_u64(), Some(78));
    assert_eq!(value["supported"].as_bool(), Some(true));
}

#[cfg(feature = "cli")]
#[test]
fn detect_reports_a_non_harmony_logitech_device() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(
        vec![non_harmony_device()],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["supported"].as_bool(), Some(false));
    assert_eq!(value["model"].as_str(), Some("unknown Logitech device"));
    assert!(value["profile"].is_null());
    assert!(value["skin"].is_null());
    assert_eq!(value["usb"]["vendor_id"].as_u64(), Some(0x046D));
    assert_eq!(value["usb"]["product_id"].as_u64(), Some(0xC52B));
    assert_eq!(value["usb"]["product"].as_str(), Some("USB Receiver"));

    let (exit, stdout, _) = run_with(vec![non_harmony_device()], &["detect"], directory.path());
    assert_eq!(exit, Exit(0));
    assert!(stdout.contains("vendor 0x046D product 0xC52B"), "{stdout}");
    assert!(stdout.contains("not a Harmony Remote"), "{stdout}");
}

#[cfg(feature = "cli")]
#[test]
fn open_remote_refuses_a_non_harmony_logitech_device() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let (exit, _stdout, stderr) = run_with(
        vec![non_harmony_device()],
        &["backup", "--out", out.to_str().expect("path")],
        directory.path(),
    );

    assert_eq!(exit, Exit(3));
    assert!(stderr.contains("not a Harmony Remote"), "{stderr}");
    assert!(!out.exists());
}

#[cfg(feature = "cli")]
#[test]
fn detect_refuses_a_link_packet_length_other_than_64() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut wrong = device(79, 0x1078, 0xC124);
    wrong.sysinfo = sysinfo_with_link(79, 16, Some(63));

    let (exit, stdout, stderr) = run_with(vec![wrong], &["detect"], directory.path());

    assert_eq!(exit, Exit(3));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("link_packet_length"), "{stderr}");
    assert!(stderr.contains("63"), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn detect_refuses_a_missing_link_packet_length() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut wrong = device(79, 0x1078, 0xC124);
    wrong.sysinfo = sysinfo_with_link(79, 16, None);

    let (exit, stdout, stderr) = run_with(vec![wrong], &["detect"], directory.path());

    assert_eq!(exit, Exit(3));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("link_packet_length"), "{stderr}");
    assert!(stderr.contains("missing"), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn backup_refuses_a_configuration_above_the_region_limit() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    backend.store(0).borrow_mut().usercfg = (0..=u8::MAX)
        .cycle()
        .take(container::MAX_CONTAINER_SIZE + 1)
        .collect();
    let out = directory.path().join("backup.bin");

    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &["backup", "--out", out.to_str().expect("path"), "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(5), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("above the"), "{stderr}");
    assert!(
        !out.exists(),
        "no Backup is written for an oversized region"
    );
    assert!(
        !PathBuf::from(format!("{}.json", out.display())).exists(),
        "no sidecar is written for an oversized region"
    );
}

#[cfg(feature = "cli")]
#[test]
fn restore_refuses_a_backup_above_the_region_limit() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = directory.path().join("oversized.bin");
    let bytes = vec![0u8; container::MAX_CONTAINER_SIZE + 4];
    std::fs::write(&backup, &bytes).expect("backup");
    std::fs::write(
        format!("{}.json", backup.display()),
        serde_json::to_vec(&json!({
            "schema_version": 1,
            "model": "Harmony 300",
            "skin": 79,
            "architecture": 16,
            "size": bytes.len(),
            "sha256": container::sha256_hex(&bytes),
            "validation": "ok",
        }))
        .expect("sidecar"),
    )
    .expect("sidecar");

    let mark = backend.calls().borrow().len();
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &[
            "restore",
            "--backup",
            backup.to_str().expect("path"),
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stderr.contains("maximum is"), "{stderr}");
    let calls = backend.calls().borrow().clone();
    assert!(
        !calls[mark..]
            .iter()
            .any(|call| matches!(call, Call::OpenWrite { .. })),
        "an oversized Backup must never be written"
    );
}

#[cfg(feature = "cli")]
#[test]
fn a_state_directory_that_cannot_be_created_exits_one() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, sample_container()).expect("container");
    // A regular file where the state directory should be makes Journal::create fail with an I/O
    // error, which is the internal-error exit code.
    let state_path = directory.path().join("state");
    std::fs::write(&state_path, b"not a directory").expect("state file");

    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &[
            "write",
            "--container",
            container_path.to_str().expect("path"),
            "--backup",
            backup.to_str().expect("path"),
            "--commit",
            "--json",
        ],
        &state_path,
    );

    assert_eq!(exit, Exit(1), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
}

#[cfg(feature = "cli")]
#[test]
fn backup_refuses_an_existing_sidecar_without_overwrite() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let sidecar = PathBuf::from(format!("{}.json", out.display()));
    std::fs::write(&sidecar, b"keep").expect("seed sidecar");

    let (exit, _stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &["backup", "--out", out.to_str().expect("path")],
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert!(!out.exists(), "the Container is not written either");
    assert_eq!(std::fs::read(&sidecar).expect("sidecar"), b"keep");
    assert!(stderr.contains("already exists"), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn backup_overwrite_replaces_both_files() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let sidecar = PathBuf::from(format!("{}.json", out.display()));
    std::fs::write(&out, b"old").expect("seed");
    std::fs::write(&sidecar, b"old").expect("seed");

    let (exit, _stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &[
            "backup",
            "--out",
            out.to_str().expect("path"),
            "--overwrite",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    assert_eq!(std::fs::read(&out).expect("backup"), sample_container());
    let value: Value =
        serde_json::from_slice(&std::fs::read(&sidecar).expect("sidecar")).expect("json");
    assert_eq!(value["validation"].as_str(), Some("ok"));
}

#[cfg(feature = "cli")]
#[test]
fn backup_prints_its_embedded_schema_without_a_remote() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["backup", "--print-schema"], directory.path());

    assert_eq!(exit, Exit(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("schema json");
    assert_eq!(
        value["title"].as_str(),
        Some("Remote Rebirth Backup sidecar")
    );
}

#[cfg(feature = "cli")]
#[test]
fn compose_prints_its_model_and_resolved_schemas_without_inputs() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["compose", "--print-schema"], directory.path());
    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("schema json");
    assert_eq!(value["title"].as_str(), Some("Remote Rebirth model"));

    let (exit, stdout, stderr) = run_with(
        vec![],
        &["compose", "--print-resolved-schema"],
        directory.path(),
    );
    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("schema json");
    assert_eq!(
        value["title"].as_str(),
        Some("Remote Rebirth resolved model")
    );
}

#[cfg(feature = "cli")]
#[test]
fn cli_help_and_version_are_successful_stdout() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["--help"], directory.path());
    assert_eq!(exit, Exit(0));
    assert!(stdout.contains("Usage"), "{stdout}");
    assert!(stderr.is_empty(), "{stderr}");

    let (exit, stdout, stderr) = run_with(vec![], &["--version"], directory.path());
    assert_eq!(exit, Exit(0));
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
    assert!(stderr.is_empty(), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn cli_usage_errors_exit_two() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["detect", "--unknown"], directory.path());
    assert_eq!(exit, Exit(2));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(!stderr.trim().is_empty(), "{stderr}");

    let (exit, _stdout, stderr) = run_with(vec![], &["backup"], directory.path());
    assert_eq!(exit, Exit(2));
    assert!(!stderr.trim().is_empty(), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn the_exit_code_taxonomy_covers_one_to_six() {
    use crate::error::Error;

    assert_eq!(Error::Internal("x".into()).exit_code(), 1);
    assert_eq!(Error::Io(std::io::Error::other("x")).exit_code(), 1);
    assert_eq!(Error::Usage("x".into()).exit_code(), 2);
    assert_eq!(Error::Device("x".into()).exit_code(), 3);
    assert_eq!(Error::Validation("x".into()).exit_code(), 4);
    assert_eq!(Error::Protocol("x".into()).exit_code(), 5);
    assert_eq!(Error::Verification("x".into()).exit_code(), 6);
}
