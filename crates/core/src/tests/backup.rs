use crate::container::ValidationStatus;

use super::*;

#[cfg(feature = "cli")]
#[test]
fn backup_writes_container_and_sidecar() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let (exit, stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &["backup", "--out", out.to_str().expect("path"), "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let expected = sample_container();
    assert_eq!(std::fs::read(&out).expect("backup"), expected);

    let sidecar_path = PathBuf::from(format!("{}.json", out.display()));
    let sidecar: Value =
        serde_json::from_slice(&std::fs::read(&sidecar_path).expect("sidecar")).expect("json");
    assert_eq!(sidecar["schema_version"].as_u64(), Some(1));
    assert_eq!(sidecar["model"].as_str(), Some("Harmony 300"));
    assert_eq!(sidecar["skin"].as_u64(), Some(79));
    assert_eq!(sidecar["architecture"].as_u64(), Some(16));
    assert_eq!(sidecar["size"].as_u64(), Some(670));
    assert_eq!(
        sidecar["sha256"].as_str(),
        Some(container::sha256_hex(&expected).as_str())
    );
    assert_eq!(sidecar["validation"].as_str(), Some("ok"));
    assert!(
        sidecar["timestamp"]
            .as_str()
            .is_some_and(|value| value.ends_with('Z'))
    );

    let output: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(output["validation"].as_str(), Some("ok"));
    assert_eq!(output["size"].as_u64(), Some(670));
}

#[cfg(feature = "cli")]
#[test]
fn backup_refuses_a_symlinked_destination() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let target = directory.path().join("target.bin");
    std::fs::write(&target, b"untouched").expect("target");
    let link = directory.path().join("backup.bin");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    let (exit, stdout, stderr) = run_backend(
        &mut backend,
        &[
            "backup",
            "--out",
            link.to_str().expect("path"),
            "--overwrite",
            "--json",
        ],
        directory.path(),
    );

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("is a symlink"), "{stderr}");
    assert_eq!(
        std::fs::read(&target).expect("target"),
        b"untouched",
        "the symlink target must not be written"
    );
}

#[cfg(feature = "cli")]
#[test]
fn a_successful_write_leaves_no_temporary_file_behind() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let out = directory.path().join("backup.bin");
    let (exit, _stdout, stderr) = run_backend(
        &mut backend,
        &["backup", "--out", out.to_str().expect("path"), "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let leftovers: Vec<String> = std::fs::read_dir(directory.path())
        .expect("read_dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "temporary files remain: {leftovers:?}"
    );
}

#[cfg(feature = "cli")]
#[test]
fn backup_refuses_overwrite() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    std::fs::write(&out, b"keep").expect("seed");
    let (exit, _stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &["backup", "--out", out.to_str().expect("path")],
        directory.path(),
    );

    assert_eq!(exit, Exit(4));
    assert_eq!(std::fs::read(&out).expect("backup"), b"keep");
    assert!(stderr.contains("already exists"), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn backup_without_remote_writes_nothing() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let (exit, _stdout, stderr) = run_with(
        vec![],
        &["backup", "--out", out.to_str().expect("path")],
        directory.path(),
    );

    assert_eq!(exit, Exit(3));
    assert!(!out.exists());
    assert!(stderr.contains("no Harmony Remote found"), "{stderr}");
}

#[cfg(feature = "cli")]
#[test]
fn backup_marks_checksum_mismatch() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let mut container = sample_container();
    container[10] ^= 0xFF;
    let mut damaged = device(79, 0x1078, 0xC124);
    damaged.usercfg = container[..container.len() - 4].to_vec();

    let (exit, _stdout, stderr) = run_with(
        vec![damaged],
        &["backup", "--out", out.to_str().expect("path"), "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let sidecar_path = PathBuf::from(format!("{}.json", out.display()));
    let sidecar: Value =
        serde_json::from_slice(&std::fs::read(&sidecar_path).expect("sidecar")).expect("json");
    assert_eq!(sidecar["validation"].as_str(), Some("checksum_mismatch"));
}

#[test]
fn sysinfo_parse_normalizes_identity() {
    let info = crate::sysinfo::parse(b"arch=0x10\nskin=0x4F\nfw_ver=0x14\nhw_ver=0x05\nguid=abc\n");

    assert_eq!(info.architecture, Some(16));
    assert_eq!(info.skin, Some(79));
    assert_eq!(info.firmware_version.as_deref(), Some("1.4"));
    assert_eq!(info.hardware_version.as_deref(), Some("0.5"));
    assert_eq!(info.guid.as_deref(), Some("abc"));
}

#[test]
fn firmware_matches_compares_the_major_and_minor_version() {
    let profile = crate::profile::by_id("harmony_300").expect("the Harmony 300 profile");

    assert!(crate::profile::firmware_matches(profile, Some("1.4")));
    assert!(crate::profile::firmware_matches(profile, Some("1.4.0")));
    assert!(!crate::profile::firmware_matches(profile, Some("1.5.0")));
    assert!(!crate::profile::firmware_matches(profile, Some("2.4.0")));
    assert!(!crate::profile::firmware_matches(profile, None));
}

#[test]
fn container_reconstructs_trailer() {
    let full = sample_container();
    let read = &full[..full.len() - 4];
    let rebuilt = container::reconstruct_trailer(read);

    assert_eq!(rebuilt, full);
    assert_eq!(container::validation_status(&rebuilt), ValidationStatus::Ok);
}

#[cfg(feature = "cli")]
#[test]
fn container_rejects_short_trailer() {
    assert!(container::trailer_checksum(&[0u8; 5]).is_err());
}
