use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::clock::Clock;
use crate::container::{self, ValidationStatus};
use crate::fake::{Call, FakeBackend, FakeDevice};
use crate::filewriter::{self, WritePlan};
use crate::runtime::Runtime;
use crate::transport::RemoteDescriptor;
use crate::{Exit, run};

struct FixedClock(time::OffsetDateTime);

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0
    }
}

fn descriptor(
    bus: u8,
    address: u8,
    product_id: u16,
    bcd_device: u16,
    product: &str,
) -> RemoteDescriptor {
    RemoteDescriptor {
        bus,
        address,
        vendor_id: 0x046D,
        product_id,
        bcd_device,
        product: Some(product.to_string()),
        serial: None,
    }
}

fn sysinfo(skin: u8, architecture: u8) -> Vec<u8> {
    format!(
        "arch=0x{architecture:02X}\nskin=0x{skin:02X}\nfw_ver=0x14\nfw_type=0\nhw_ver=0x05\nlink_type=hid\nlink_packet_length=64\nguid=test-guid\n"
    )
    .into_bytes()
}

fn sample_container() -> Vec<u8> {
    sample_container_sized(128)
}

fn sample_container_sized(len: usize) -> Vec<u8> {
    assert!(len > 32, "a Container needs a complete trailer");
    let mut container = vec![0u8; len];
    container[0..4].copy_from_slice(b"GSPM");
    container[0x47..0x4B].copy_from_slice(b"LWJL");
    container[4..8].copy_from_slice(&(container::FLASH_BASE + len as u32 - 4).to_le_bytes());
    container[0x08] = 0x0F;
    let checksum = container::trailer_checksum(&container).expect("checksum");
    container[len - 6..len - 4].copy_from_slice(&checksum.to_le_bytes());
    container[len - 4..].copy_from_slice(b"PTYY");
    container
}

fn device(skin: u8, bcd_device: u16, product_id: u16) -> FakeDevice {
    let container = sample_container();
    FakeDevice {
        descriptor: descriptor(1, 5, product_id, bcd_device, "Harmony Remote 0-1.4.0"),
        sysinfo: sysinfo(skin, 16),
        usercfg: container[..124].to_vec(),
    }
}

fn run_with(devices: Vec<FakeDevice>, args: &[&str], state_dir: &Path) -> (Exit, String, String) {
    let mut backend = FakeBackend::new(devices);
    run_backend(&mut backend, args, state_dir)
}

fn run_backend(
    backend: &mut FakeBackend,
    args: &[&str],
    state_dir: &Path,
) -> (Exit, String, String) {
    let clock = FixedClock(
        time::OffsetDateTime::from_unix_timestamp(1_800_000_000).expect("fixed timestamp"),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut runtime = Runtime {
        backend,
        clock: &clock,
        stdout: &mut stdout,
        stderr: &mut stderr,
        state_dir: state_dir.to_path_buf(),
    };
    let args: Vec<OsString> = std::iter::once("remote-rebirth")
        .chain(args.iter().copied())
        .map(OsString::from)
        .collect();
    let exit = run(&args, &mut runtime);
    (
        exit,
        String::from_utf8(stdout).expect("stdout utf8"),
        String::from_utf8(stderr).expect("stderr utf8"),
    )
}

#[test]
fn detect_json_reports_supported_harmony_300() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["supported"].as_bool(), Some(true));
    assert_eq!(value["model"].as_str(), Some("Harmony 300"));
    assert_eq!(value["profile"].as_str(), Some("harmony_300"));
    assert_eq!(value["architecture"].as_u64(), Some(16));
    assert_eq!(value["skin"].as_u64(), Some(79));
    assert_eq!(value["firmware_version"].as_str(), Some("1.4"));
    assert!(value["serial"].is_null());
}

#[test]
fn detect_reports_unsupported_harmony_350() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(
        vec![device(104, 0x1104, 0xC124)],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["supported"].as_bool(), Some(false));
    assert_eq!(value["model"].as_str(), Some("Harmony 350"));
    assert!(value["profile"].is_null());
}

#[test]
fn detect_without_remote_exits_3() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["detect"], directory.path());

    assert_eq!(exit, Exit(3));
    assert!(stdout.is_empty());
    assert!(stderr.contains("no Harmony Remote found"), "{stderr}");
}

#[test]
fn detect_with_multiple_remotes_exits_3() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut second = device(79, 0x1078, 0xC124);
    second.descriptor.bus = 1;
    second.descriptor.address = 6;
    let (exit, stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124), second],
        &["detect"],
        directory.path(),
    );

    assert_eq!(exit, Exit(3));
    assert!(stdout.is_empty());
    assert!(
        stderr.contains("multiple Harmony Remotes found"),
        "{stderr}"
    );
}

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
    assert_eq!(sidecar["size"].as_u64(), Some(128));
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
    assert_eq!(output["size"].as_u64(), Some(128));
}

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

#[test]
fn backup_marks_checksum_mismatch() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let mut container = sample_container();
    container[10] ^= 0xFF;
    let mut damaged = device(79, 0x1078, 0xC124);
    damaged.usercfg = container[..124].to_vec();

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
fn container_reconstructs_trailer() {
    let full = sample_container();
    let read = &full[..124];
    let rebuilt = container::reconstruct_trailer(read);

    assert_eq!(rebuilt, full);
    assert_eq!(container::validation_status(&rebuilt), ValidationStatus::Ok);
}

#[test]
fn container_rejects_short_trailer() {
    assert!(container::trailer_checksum(&[0u8; 5]).is_err());
}

#[test]
fn container_marks_unreadable() {
    assert_eq!(
        container::validation_status(b"not a container"),
        ValidationStatus::Unreadable
    );
}

#[test]
fn container_detects_checksum_mismatch() {
    let mut full = sample_container();
    full[10] ^= 0xFF;
    assert_eq!(
        container::validation_status(&full),
        ValidationStatus::ChecksumMismatch
    );
}

fn make_backup(backend: &mut FakeBackend, directory: &Path) -> PathBuf {
    let path = directory.join("backup.bin");
    let (exit, stdout, stderr) = run_backend(
        backend,
        &["backup", "--out", path.to_str().expect("path"), "--json"],
        directory,
    );
    assert_eq!(exit, Exit(0), "{stdout} {stderr}");
    path
}

fn journal_records(directory: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(directory.join("journal.jsonl")).expect("journal");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("journal record"))
        .collect()
}

fn position_of(calls: &[Call], predicate: impl Fn(&Call) -> bool) -> usize {
    calls
        .iter()
        .position(predicate)
        .expect("expected call was not recorded")
}

fn no_write_opened(calls: &[Call]) -> bool {
    !calls
        .iter()
        .any(|call| matches!(call, Call::OpenWrite { .. }))
}

fn write_args<'a>(container: &'a str, backup: &'a str, commit: bool) -> Vec<&'a str> {
    let mut args = vec!["write", "--container", container, "--backup", backup];
    if commit {
        args.push("--commit");
    }
    args.push("--json");
    args
}

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
    assert_eq!(value["data_packets"].as_u64(), Some(2));
    assert_eq!(value["bytes"].as_u64(), Some(124));
    assert!(value["journal"].is_null());

    let calls = backend.calls().borrow().clone();
    assert!(no_write_opened(&calls[mark..]));
    assert_eq!(
        backend.store(0).borrow().usercfg,
        container[..container.len() - 4].to_vec()
    );
    assert!(!directory.path().join("journal.jsonl").exists());
}

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
    backend.store(0).borrow_mut().usercfg = current[..124].to_vec();

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
    assert_eq!(backend.store(0).borrow().usercfg, current[..124].to_vec());
}

#[test]
fn write_allows_empty_current_configuration() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().usercfg = vec![0xFF; 124];

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
    assert_eq!(value["data_packets"].as_u64(), Some(2));
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
        Call::OpenWrite { path, size } if path == "/cfg/usercfg" && *size == 124
    )));
    let expected = container::stored_checksum(&container).expect("checksum");
    assert!(tail.iter().any(|call| matches!(
        call,
        Call::Check { expected: recorded, covered_len, .. }
            if *recorded == expected && *covered_len == 122
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
    assert_eq!(written, container[..124].to_vec());
    assert_eq!(backend.store(0).borrow().usercfg, container[..124].to_vec());

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
    assert_eq!(value["bytes"].as_u64(), Some(6996));

    let calls = backend.calls().borrow().clone();
    let chunk_sizes: Vec<usize> = calls[mark..]
        .iter()
        .filter_map(|call| match call {
            Call::WriteChunk { data, last, .. } => {
                assert!(*last);
                Some(data.len())
            }
            _ => None,
        })
        .collect();
    assert_eq!(chunk_sizes, vec![3100, 3100, 796]);
    assert_eq!(
        backend.store(0).borrow().usercfg,
        container[..6996].to_vec()
    );
    assert!(calls[mark..].iter().any(|call| matches!(
        call,
        Call::Check { covered_len, .. } if *covered_len == 6994
    )));
}

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
    assert_eq!(backend.store(0).borrow().usercfg, vec![0xFF; 124]);

    let records = journal_records(directory.path());
    let check = records
        .iter()
        .find(|record| record["step"] == "check")
        .expect("check record");
    assert_eq!(check["reply"].as_str(), Some("u"));
}

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
    assert_eq!(backend.store(0).borrow().usercfg, vec![0xFF; 124]);
    let records = journal_records(directory.path());
    let last = records.last().expect("journal record");
    assert_eq!(last["step"].as_str(), Some("open_write"));
}

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
    assert_eq!(backend.store(0).borrow().usercfg, expected[..124].to_vec());
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
