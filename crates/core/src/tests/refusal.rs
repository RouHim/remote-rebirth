use super::*;

use super::profile::compose_with_test_profile;

// ---------------------------------------------------------------------------
// Write and Restore refuse a Container the connected Remote cannot hold.
// ---------------------------------------------------------------------------

/// The absolute address a raw section slot names.
#[cfg(feature = "cli")]
fn slot_address(container: &[u8], raw: usize) -> u32 {
    let at = 0x0B + 4 * raw + 1;
    u32::from(container[at])
        | u32::from(container[at + 1]) << 8
        | u32::from(container[at + 2]) << 16
}

/// Rewrite a Container's Skin and, optionally, its infrared group count, then reseal it.
#[cfg(feature = "cli")]
fn patch_container(container: &mut [u8], skin: u8, ir_groups: Option<u8>) {
    let architecture = (slot_address(container, 1) - container::FLASH_BASE) as usize;
    let infrared = ir_groups.map(|_| (slot_address(container, 4) - container::FLASH_BASE) as usize);
    container[architecture + 2] = skin;
    if let (Some(count), Some(infrared)) = (ir_groups, infrared) {
        container[infrared] = count;
    }
    let len = container.len();
    container[4..8].copy_from_slice(&(container::FLASH_BASE + len as u32 - 4).to_le_bytes());
    let checksum_at = len - 6;
    container[checksum_at..checksum_at + 2].copy_from_slice(&[0, 0]);
    let checksum = container::trailer_checksum(container).expect("checksum");
    container[checksum_at..checksum_at + 2].copy_from_slice(&checksum.to_le_bytes());
}

#[cfg(feature = "cli")]
fn expect_write_refused(container: Vec<u8>, message: &str) {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container_path = directory.path().join("candidate.bin");
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

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains(message), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_skin_78_container_on_a_skin_79_remote() {
    let container = crate::emit_empty(78, &fixture_moment()).expect("skin 78");
    expect_write_refused(container, "Skin");
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_container_with_too_many_infrared_groups() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut container, _) = compose_with_test_profile(directory.path(), 1, 0);
    patch_container(&mut container, 79, None);
    expect_write_refused(container, "infrared groups");
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_container_with_too_many_devices() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut container, _) = compose_with_test_profile(directory.path(), 5, 0);
    patch_container(&mut container, 79, Some(4));
    expect_write_refused(container, "Devices");
}

#[cfg(feature = "cli")]
#[test]
fn write_refuses_a_container_with_too_many_favourites() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut container, _) = compose_with_test_profile(directory.path(), 1, 5);
    patch_container(&mut container, 79, Some(4));
    expect_write_refused(container, "Favourites");
}

#[cfg(feature = "cli")]
fn crafted_backup(directory: &Path, bytes: &[u8], declared_skin: u8) -> PathBuf {
    let path = directory.join("crafted-backup.bin");
    std::fs::write(&path, bytes).expect("backup");
    let sidecar = json!({
        "schema_version": 1,
        "file": "crafted-backup.bin",
        "model": "Harmony 300",
        "skin": declared_skin,
        "architecture": 16,
        "size": bytes.len() as u64,
        "sha256": container::sha256_hex(bytes),
        "timestamp": "2026-11-05T12:34:56Z",
        "validation": "ok"
    });
    let sidecar_path = PathBuf::from(format!("{}.json", path.display()));
    std::fs::write(
        &sidecar_path,
        serde_json::to_vec_pretty(&sidecar).expect("sidecar"),
    )
    .expect("write sidecar");
    path
}

#[cfg(feature = "cli")]
#[test]
fn restore_refuses_a_container_whose_skin_differs_from_the_remote() {
    let directory = tempfile::tempdir().expect("tempdir");
    let bytes = crate::emit_empty(78, &fixture_moment()).expect("skin 78");
    // The sidecar claims the connected Remote's skin, so only the Container/Skin check can refuse.
    let backup = crafted_backup(directory.path(), &bytes, 79);
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);

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

    assert_eq!(exit, Exit(4), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("Skin"), "{stderr}");
    assert!(no_write_opened(&backend.calls().borrow()[mark..]));
}
