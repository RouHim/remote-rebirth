use std::collections::VecDeque;
#[cfg(feature = "cli")]
use std::ffi::OsString;
#[cfg(feature = "cli")]
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

#[cfg(feature = "cli")]
use crate::clock::Clock;
use crate::container;
#[cfg(feature = "cli")]
use crate::fake::{Call, FakeBackend, FakeDevice};
use crate::filewriter::{self, WritePlan};
#[cfg(feature = "cli")]
use crate::progress::Progress;
use crate::protocol::{FileProtocol, REPORT_BYTES, ReportLink, chunk_reply_matches};
#[cfg(feature = "cli")]
use crate::runtime::Runtime;
#[cfg(feature = "cli")]
use crate::transport::RemoteDescriptor;
use crate::transport::RemoteSession;
#[cfg(feature = "cli")]
use crate::{Exit, run};

#[cfg(feature = "cli")]
struct FixedClock(time::OffsetDateTime);

#[cfg(feature = "cli")]
impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0
    }
}

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
fn sysinfo(skin: u8, architecture: u8) -> Vec<u8> {
    sysinfo_with_link(skin, architecture, Some(64))
}

#[cfg(feature = "cli")]
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
#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
fn device(skin: u8, bcd_device: u16, product_id: u16) -> FakeDevice {
    let container = crate::emit_empty(skin, &fixture_moment()).expect("a supported skin");
    let stored = container[..container.len() - 4].to_vec();
    FakeDevice {
        descriptor: descriptor(1, 5, product_id, bcd_device, "Harmony Remote 0-1.4.0"),
        sysinfo: sysinfo(skin, 16),
        usercfg: stored,
    }
}

#[cfg(feature = "cli")]
fn run_with(devices: Vec<FakeDevice>, args: &[&str], state_dir: &Path) -> (Exit, String, String) {
    let mut backend = FakeBackend::new(devices);
    run_backend(&mut backend, args, state_dir)
}

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
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
    // The family comes from bcdDevice; skin 104 is recognised but has no profile in v1.
    assert_eq!(value["family"].as_str(), Some("Harmony 350"));
    assert!(value["profile"].is_null());
}

#[cfg(feature = "cli")]
#[test]
fn detect_without_remote_exits_3() {
    let directory = tempfile::tempdir().expect("tempdir");
    let (exit, stdout, stderr) = run_with(vec![], &["detect"], directory.path());

    assert_eq!(exit, Exit(3));
    assert!(stdout.is_empty());
    assert!(stderr.contains("no Harmony Remote found"), "{stderr}");
}

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
fn journal_records(directory: &Path) -> Vec<Value> {
    let text = std::fs::read_to_string(directory.join("journal.jsonl")).expect("journal");
    text.lines()
        .map(|line| serde_json::from_str(line).expect("journal record"))
        .collect()
}

#[cfg(feature = "cli")]
fn position_of(calls: &[Call], predicate: impl Fn(&Call) -> bool) -> usize {
    calls
        .iter()
        .position(predicate)
        .expect("expected call was not recorded")
}

#[cfg(feature = "cli")]
fn no_write_opened(calls: &[Call]) -> bool {
    !calls
        .iter()
        .any(|call| matches!(call, Call::OpenWrite { .. }))
}

#[cfg(feature = "cli")]
fn write_args<'a>(container: &'a str, backup: &'a str, commit: bool) -> Vec<&'a str> {
    let mut args = vec!["write", "--container", container, "--backup", backup];
    if commit {
        args.push("--commit");
    }
    args.push("--json");
    args
}

mod backup;
mod model;
#[cfg(feature = "cli")]
mod profile;
mod protocol;
#[cfg(feature = "cli")]
mod refusal;
#[cfg(feature = "cli")]
mod restore;
#[cfg(feature = "cli")]
mod review;
#[cfg(feature = "cli")]
mod sidecar;
mod write;
