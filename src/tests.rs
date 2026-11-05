use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::clock::Clock;
use crate::container::{self, ValidationStatus};
use crate::fake::{FakeBackend, FakeDevice};
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
    let mut container = vec![0u8; 128];
    container[0..4].copy_from_slice(b"GSPM");
    container[0x47..0x4B].copy_from_slice(b"LWJL");
    container[4..8].copy_from_slice(&(container::FLASH_BASE + 124).to_le_bytes());
    container[0x08] = 0x0F;
    let checksum = container::trailer_checksum(&container).expect("checksum");
    container[122..124].copy_from_slice(&checksum.to_le_bytes());
    container[124..128].copy_from_slice(b"PTYY");
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
    let clock = FixedClock(
        time::OffsetDateTime::from_unix_timestamp(1_800_000_000).expect("fixed timestamp"),
    );
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut runtime = Runtime {
        backend: &mut backend,
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
    assert!(stderr.contains("multiple Harmony Remotes found"), "{stderr}");
}

#[test]
fn backup_writes_container_and_sidecar() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("backup.bin");
    let (exit, stdout, stderr) = run_with(
        vec![device(79, 0x1078, 0xC124)],
        &[
            "backup",
            "--out",
            out.to_str().expect("path"),
            "--json",
        ],
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
    assert_eq!(
        sidecar["validation"].as_str(),
        Some("checksum_mismatch")
    );
}

#[test]
fn sysinfo_parse_normalizes_identity() {
    let info = crate::sysinfo::parse(
        b"arch=0x10\nskin=0x4F\nfw_ver=0x14\nhw_ver=0x05\nguid=abc\n",
    );

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
    assert_eq!(
        container::validation_status(&rebuilt),
        ValidationStatus::Ok
    );
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

fn valid_model() -> crate::model::Model {
    serde_json::from_value(serde_json::json!({
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
    }))
    .expect("valid model")
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
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("duplicate");
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
    model.devices[0].commands.get_mut("power").expect("power").source =
        serde_json::from_value(serde_json::json!({
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
