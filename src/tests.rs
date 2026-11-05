use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::clock::Clock;
use crate::container::{self, ValidationStatus};
use crate::fake::{Call, FakeBackend, FakeDevice};
use crate::filewriter::{self, WritePlan};
use crate::protocol::{FileProtocol, REPORT_BYTES, ReportLink, chunk_reply_matches};
use crate::runtime::Runtime;
use crate::transport::{RemoteDescriptor, RemoteSession};
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
        path: None,
    }
}

fn sysinfo(skin: u8, architecture: u8) -> Vec<u8> {
    sysinfo_with_link(skin, architecture, Some(64))
}

fn sysinfo_with_link(skin: u8, architecture: u8, link: Option<u16>) -> Vec<u8> {
    let mut text = format!(
        "arch=0x{architecture:02X}\nskin=0x{skin:02X}\nfw_ver=0x14\nfw_type=0\nhw_ver=0x05\nlink_type=hid\nguid=test-guid\n"
    );
    if let Some(link) = link {
        text.push_str(&format!("link_packet_length={link}\n"));
    }
    text.into_bytes()
}

fn fixture_moment() -> time::OffsetDateTime {
    let date =
        time::Date::from_calendar_date(2026, time::Month::November, 5).expect("a valid date");
    let time = time::Time::from_hms(12, 34, 56).expect("a valid time");
    time::PrimitiveDateTime::new(date, time).assume_utc()
}

/// A genuine Architecture 16 Container: the empty Harmony 300 configuration for skin 79.
fn sample_container() -> Vec<u8> {
    crate::emit_empty(79, &fixture_moment()).expect("skin 79 is supported")
}

/// A genuine Container of exactly `len` bytes: the empty configuration with zero padding before its
/// trailer, so the last raw slot absorbs the padding.
fn sample_container_sized(len: usize) -> Vec<u8> {
    let base = sample_container();
    assert!(len > base.len() - 4, "a Container needs a complete trailer");
    let mut container = base[..base.len() - 4].to_vec();
    container.resize(len - 4, 0);
    container.extend_from_slice(b"PTYY");
    container[4..8].copy_from_slice(&(container::FLASH_BASE + len as u32 - 4).to_le_bytes());
    let checksum_at = len - 6;
    container[checksum_at..checksum_at + 2].copy_from_slice(&[0, 0]);
    let checksum = container::trailer_checksum(&container).expect("checksum");
    container[checksum_at..checksum_at + 2].copy_from_slice(&checksum.to_le_bytes());
    container
}

fn device(skin: u8, bcd_device: u16, product_id: u16) -> FakeDevice {
    let container = crate::emit_empty(skin, &fixture_moment()).expect("a supported skin");
    let stored = container[..container.len() - 4].to_vec();
    FakeDevice {
        descriptor: descriptor(1, 5, product_id, bcd_device, "Harmony Remote 0-1.4.0"),
        sysinfo: sysinfo(skin, 16),
        usercfg: stored,
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
fn container_reconstructs_trailer() {
    let full = sample_container();
    let read = &full[..full.len() - 4];
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
    assert_eq!(backend.store(0).borrow().usercfg, vec![0xFF; 670]);

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

fn valid_model_json() -> Value {
    serde_json::json!({
        "schema_version": 1,
        "profile": "harmony_300",
        "skin": 79,
        "devices": [{
            "name": "TV",
            "device_key": 1,
            "commands": {
                "power": { "source": { "format": "pronto", "hex": "0000 006D 0002 0000 0020 0020 0020 0020" } },
                "input": { "source": { "format": "pronto", "hex": "0000 006D 0002 0000 0020 0020 0020 0020" } }
            },
            "keys": { "power": "power", "input": "input" }
        }],
        "activity": {
            "channel_device": "TV",
            "steps": [{ "device": "TV", "power": "power", "input": "input" }]
        },
        "favourites": [{ "key": 1, "channel": "101" }]
    })
}

fn valid_model() -> crate::model::Model {
    serde_json::from_value(valid_model_json()).expect("valid model")
}

#[test]
fn model_accepts_valid_harmony_300_model() {
    let model = valid_model();
    crate::model::validate(&model, &crate::profile::HARMONY_300).expect("valid");
    assert_eq!(model.activity.steps[0].delay_ms, 500);
}

#[test]
fn model_rejects_duplicate_device_keys() {
    let mut model = valid_model();
    let mut second = model.devices[0].clone();
    second.name = "Cable".into();
    model.devices.push(second);
    let error =
        crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("duplicate");
    assert!(error.to_string().contains("device key 1"));
}

#[test]
fn model_rejects_unreferenced_command() {
    let mut model = valid_model();
    model.devices[0].commands.insert(
        "unused".into(),
        serde_json::from_value(serde_json::json!({
            "source": { "format": "pronto", "hex": "0000 006D 0002 0000 0020 0020 0020 0020" }
        }))
        .expect("command"),
    );
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("unused");
    assert!(error.to_string().contains("not referenced"));
}

#[test]
fn model_rejects_leading_zero_favourite() {
    let mut model = valid_model();
    model.favourites[0].channel = "01".into();
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("channel");
    assert!(error.to_string().contains("leading zero"));
}

#[test]
fn model_rejects_five_favourites_on_skin_79() {
    let mut model = valid_model();
    model.favourites = (1..=5)
        .map(|key| crate::model::Favourite {
            key,
            channel: format!("10{key}"),
        })
        .collect();
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("capacity");
    assert!(error.to_string().contains("skin 79 accepts 4"));
}

#[test]
fn model_rejects_lirc_without_button() {
    let mut model = valid_model();
    model.devices[0]
        .commands
        .get_mut("power")
        .expect("power")
        .source = serde_json::from_value(serde_json::json!({
        "format": "lirc",
        "file": "tv.conf",
        "remote": "TV"
    }))
    .expect("source");
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("lirc");
    assert!(error.to_string().contains("remote and button"));
}

#[test]
fn model_rejects_unknown_key() {
    let mut model = valid_model();
    model.devices[0]
        .keys
        .insert("unknown_key".into(), "power".into());
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("key");
    assert!(error.to_string().contains("unknown key"));
}

#[test]
fn model_rejects_unknown_json_field() {
    let value = serde_json::json!({
        "schema_version": 1,
        "profile": "harmony_300",
        "skin": 79,
        "devices": [],
        "activity": { "channel_device": "TV", "steps": [] },
        "unexpected": true
    });
    assert!(serde_json::from_value::<crate::model::Model>(value).is_err());
}

#[test]
fn model_json_validates_against_published_schema() {
    let schema: Value =
        serde_json::from_str(include_str!("../schema/model-v1.json")).expect("schema");
    let validator = jsonschema::validator_for(&schema).expect("compile schema");
    validator
        .validate(&valid_model_json())
        .expect("input model validates");
    let serialized = serde_json::to_value(valid_model()).expect("serialize model");
    validator
        .validate(&serialized)
        .expect("serialized model validates");
}

#[test]
fn backup_sidecar_validates_against_published_schema() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let (exit, _stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &["backup", "--out", out.to_str().expect("path")],
        directory.path(),
    );
    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let sidecar_path = PathBuf::from(format!("{}.json", out.display()));
    let sidecar: Value =
        serde_json::from_slice(&std::fs::read(&sidecar_path).expect("sidecar")).expect("json");
    let schema: Value =
        serde_json::from_str(include_str!("../schema/backup-v1.json")).expect("schema");
    let validator = jsonschema::validator_for(&schema).expect("compile schema");
    validator.validate(&sidecar).expect("sidecar validates");
}

// ---------------------------------------------------------------------------
// Code-review findings: detection, overwrite, schema exposure, clap, profiles.
// ---------------------------------------------------------------------------

fn non_harmony_device() -> FakeDevice {
    FakeDevice {
        descriptor: descriptor(1, 5, 0xC52B, 0x0100, "USB Receiver"),
        sysinfo: Vec::new(),
        usercfg: Vec::new(),
    }
}

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

#[test]
fn detect_labels_the_harmony_350_family_as_unsupported() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(
        vec![device(104, 0x1104, 0xC124)],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    // The family comes from bcdDevice; skin 104 is recognised but has no profile in v1.
    assert_eq!(value["family"].as_str(), Some("Harmony 350"));
    assert_eq!(value["supported"].as_bool(), Some(false));
    assert_eq!(value["model"].as_str(), Some("Harmony 350"));
}

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

#[test]
fn cli_usage_errors_exit_two() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["detect", "--unknown"], directory.path());
    assert_eq!(exit, Exit(2));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("unexpected argument"), "{stderr}");

    let (exit, _stdout, stderr) = run_with(vec![], &["backup"], directory.path());
    assert_eq!(exit, Exit(2));
    assert!(stderr.contains("--out"), "{stderr}");
}

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

// ---------------------------------------------------------------------------
// A test-only Model Profile proves Detect, Compose and validation are data-driven.
// ---------------------------------------------------------------------------

const PRONTO_POWER: &str = "0000 006D 0002 0000 0020 0020 0020 0020";

fn test_profile_model(devices: usize, favourites: usize) -> Value {
    let devices_json: Vec<Value> = (0..devices)
        .map(|index| {
            json!({
                "name": format!("Device {index}"),
                "device_key": index + 1,
                "commands": {
                    "power": { "source": { "format": "pronto", "hex": PRONTO_POWER } }
                },
                "keys": { "power": "power" }
            })
        })
        .collect();
    let mut model = json!({
        "schema_version": 1,
        "profile": "harmony_350_test",
        "skin": 105,
        "devices": devices_json,
        "activity": {
            "channel_device": "Device 0",
            "steps": [{ "device": "Device 0", "power": "power" }]
        }
    });
    if favourites > 0 {
        model["favourites"] = Value::Array(
            (1..=favourites)
                .map(|key| json!({ "key": key, "channel": format!("10{key}") }))
                .collect(),
        );
    }
    model
}

fn compose_with_test_profile(
    directory: &Path,
    devices: usize,
    favourites: usize,
) -> (Vec<u8>, Value) {
    let name = format!("model-{devices}-{favourites}.json");
    let model_path = directory.join(&name);
    std::fs::write(
        &model_path,
        serde_json::to_vec_pretty(&test_profile_model(devices, favourites)).expect("model"),
    )
    .expect("write model");
    let out = directory.join(format!("container-{devices}-{favourites}.bin"));
    let (exit, stdout, stderr) = run_with(
        vec![],
        &[
            "compose",
            "--model",
            model_path.to_str().expect("path"),
            "--out",
            out.to_str().expect("path"),
            "--json",
        ],
        directory,
    );
    assert_eq!(exit, Exit(0), "{stdout} {stderr}");
    (
        std::fs::read(&out).expect("container"),
        serde_json::from_str(&stdout).expect("json"),
    )
}

#[test]
fn a_test_profile_drives_detect_compose_and_validation_without_codec_changes() {
    let directory = tempfile::tempdir().expect("tempdir");

    let (exit, stdout, stderr) = run_with(
        vec![device(105, 0x1104, 0xC124)],
        &["detect", "--json"],
        directory.path(),
    );
    assert_eq!(exit, Exit(0), "{stderr}");
    let detected: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(detected["supported"].as_bool(), Some(true));
    assert_eq!(detected["profile"].as_str(), Some("harmony_350_test"));
    assert_eq!(detected["model"].as_str(), Some("Harmony 350"));
    assert_eq!(detected["family"].as_str(), Some("Harmony 350"));

    let model: crate::model::Model =
        serde_json::from_value(test_profile_model(5, 5)).expect("model");
    crate::model::validate(&model, &crate::profile::HARMONY_350_TEST)
        .expect("the test profile accepts five Devices and five Favourites");

    let (bytes, output) = compose_with_test_profile(directory.path(), 5, 5);
    assert_eq!(output["devices"].as_u64(), Some(5));
    assert_eq!(output["favourites"].as_u64(), Some(5));
    let parsed = crate::inspect_container::parse(&bytes).expect("the composed Container parses");
    assert_eq!(parsed.container.skin, Some(105));
    assert_eq!(parsed.devices.len(), 5);
    assert_eq!(parsed.ir_groups, 5);
}

// ---------------------------------------------------------------------------
// Write and Restore refuse a Container the connected Remote cannot hold.
// ---------------------------------------------------------------------------

/// The absolute address a raw section slot names.
fn slot_address(container: &[u8], raw: usize) -> u32 {
    let at = 0x0B + 4 * raw + 1;
    u32::from(container[at])
        | u32::from(container[at + 1]) << 8
        | u32::from(container[at + 2]) << 16
}

/// Rewrite a Container's Skin and, optionally, its infrared group count, then reseal it.
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

#[test]
fn write_refuses_a_skin_78_container_on_a_skin_79_remote() {
    let container = crate::emit_empty(78, &fixture_moment()).expect("skin 78");
    expect_write_refused(container, "Skin");
}

#[test]
fn write_refuses_a_container_with_too_many_infrared_groups() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut container, _) = compose_with_test_profile(directory.path(), 1, 0);
    patch_container(&mut container, 79, None);
    expect_write_refused(container, "infrared groups");
}

#[test]
fn write_refuses_a_container_with_too_many_devices() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut container, _) = compose_with_test_profile(directory.path(), 5, 0);
    patch_container(&mut container, 79, Some(4));
    expect_write_refused(container, "Devices");
}

#[test]
fn write_refuses_a_container_with_too_many_favourites() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (mut container, _) = compose_with_test_profile(directory.path(), 1, 5);
    patch_container(&mut container, 79, Some(4));
    expect_write_refused(container, "Favourites");
}

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

// ---------------------------------------------------------------------------
// Malformed-packet recovery.
// ---------------------------------------------------------------------------

#[test]
fn write_recovers_a_fresh_session_after_a_malformed_packet() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut backend, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");
    backend.store(0).borrow_mut().faults.fail_first_chunk = true;

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
    assert_eq!(
        tail.iter()
            .filter(|call| matches!(call, Call::Open { .. }))
            .count(),
        2,
        "a malformed packet reopens a fresh session"
    );
    assert_eq!(
        tail.iter()
            .filter(|call| matches!(call, Call::OpenRead { path } if path == "/sys/sysinfo"))
            .count(),
        2,
        "the reopened session is used: the initial Detect read and the recovery read"
    );
    assert!(!tail.iter().any(|call| matches!(call, Call::Flush { .. })));

    let records = journal_records(directory.path());
    let steps: Vec<&str> = records
        .iter()
        .filter_map(|record| record["step"].as_str())
        .collect();
    assert_eq!(
        steps,
        vec!["prepare", "reset", "open_write", "failure", "recover"]
    );
    assert_eq!(
        records
            .iter()
            .find(|record| record["step"] == "recover")
            .and_then(|record| record["reply"].as_str()),
        Some("ok")
    );
}

#[test]
fn write_reports_a_combined_error_when_recovery_fails() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut capture = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    let backup = make_backup(&mut capture, directory.path());
    let container = sample_container();
    let container_path = directory.path().join("new.bin");
    std::fs::write(&container_path, &container).expect("container");

    // A fresh backend holds no earlier Open, so the first Open succeeds and only the recovery's
    // reopen is refused.
    let mut backend = FakeBackend::new(vec![device(79, 0x1078, 0xC124)]);
    {
        let store = backend.store(0);
        let mut store = store.borrow_mut();
        store.faults.fail_first_chunk = true;
        store.faults.refuse_reopen = true;
    }

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
    assert!(stderr.contains("recovery failed"), "{stderr}");
    assert!(stderr.contains("cannot be reopened"), "{stderr}");
    let records = journal_records(directory.path());
    assert_eq!(
        records
            .iter()
            .find(|record| record["step"] == "recover")
            .and_then(|record| record["reply"].as_str()),
        Some("failed")
    );
}

#[test]
fn detect_prefers_a_harmony_over_another_logitech_device() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut mouse = FakeDevice {
        descriptor: descriptor(1, 6, 0xC077, 0x0100, "USB Optical Mouse"),
        sysinfo: Vec::new(),
        usercfg: Vec::new(),
    };
    mouse.descriptor.vendor_id = 0x046D;
    let (exit, stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124), mouse],
        &["detect", "--json"],
        directory.path(),
    );

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["supported"].as_bool(), Some(true));
    assert_eq!(value["model"].as_str(), Some("Harmony 300"));
}

#[test]
fn a_write_chunk_reply_is_read_at_the_sequence_offset() {
    let mut reply = [0u8; REPORT_BYTES];
    reply[0] = 0xFF;
    reply[1] = 0x03;
    reply[2] = 0xFE;
    assert!(chunk_reply_matches(&reply, 0, true));

    reply[2] = 0x80 | 5;
    assert!(chunk_reply_matches(&reply, 5, false));

    reply[0] = 0xFE;
    reply[2] = 0xFE;
    assert!(!chunk_reply_matches(&reply, 0, true));
}

struct ScriptedLink {
    replies: VecDeque<[u8; REPORT_BYTES]>,
}

impl ReportLink for ScriptedLink {
    fn send_report(&mut self, _report: &[u8; REPORT_BYTES]) -> Result<(), crate::error::Error> {
        Ok(())
    }

    fn receive_report(&mut self) -> Result<[u8; REPORT_BYTES], crate::error::Error> {
        self.replies
            .pop_front()
            .ok_or_else(|| crate::error::Error::Protocol("scripted link ran out of replies".into()))
    }
}

#[test]
fn a_read_accepts_a_flagged_data_packet_size() {
    let mut replies = VecDeque::new();
    let mut ack = [0u8; REPORT_BYTES];
    ack[0] = 0xFF;
    ack[1] = 0x04;
    ack[2] = 0x80;
    replies.push_back(ack);

    let mut first = [0u8; REPORT_BYTES];
    first[0] = 0x82;
    first[1] = 0xBE;
    for (index, byte) in first[2..].iter_mut().enumerate() {
        *byte = index as u8;
    }
    replies.push_back(first);

    let mut second = [0u8; REPORT_BYTES];
    second[0] = 0x83;
    second[1] = 0xA9;
    for (index, byte) in second[2..2 + 41].iter_mut().enumerate() {
        *byte = 0xA0 + index as u8;
    }
    replies.push_back(second);

    let mut done = [0u8; REPORT_BYTES];
    done[0] = 0xFE;
    replies.push_back(done);

    let mut protocol = FileProtocol::new(ScriptedLink { replies });
    let data = protocol.read(0x06, 32).expect("flagged packet size");
    assert_eq!(data.len(), 124);
    assert_eq!(&data[..3], &[0, 1, 2]);
    assert_eq!(&data[62..65], &[0xA0, 0xA1, 0xA2]);
}
