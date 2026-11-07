//! Behavioural tests for the typed GUI API.

use crate::gui_api::*;

use super::*;
use crate::fake::{FakeBackend, FakeDevice};
use crate::restore::RestoreRequest;
use crate::transport::RemoteDescriptor;
use crate::write::WriteRequest;

fn fake_harmony_300() -> FakeBackend {
    let moment = time::OffsetDateTime::from_unix_timestamp(1_800_000_000).expect("time");
    FakeBackend::new(vec![FakeDevice {
            descriptor: RemoteDescriptor {
                bus: 1,
                address: 2,
                vendor_id: 0x046D,
                product_id: 0xC124,
                bcd_device: 0x1078,
                product: Some("Harmony Remote 0-1.4.0".into()),
                serial: None,
                path: None,
            },
            sysinfo: b"arch=0x10\nskin=0x4F\nfw_ver=0x14\nfw_type=0\nhw_ver=0x05\nlink_type=hid\nguid=test-guid\nlink_packet_length=64\n".to_vec(),
            usercfg: crate::emit_empty(79, &moment).expect("empty container"),
        }])
}

#[test]
fn detect_reports_a_supported_harmony_300() {
    let mut backend = fake_harmony_300();
    let report = detect(&mut backend).expect("detect");
    assert!(report.supported);
    assert_eq!(report.model, "Harmony 300");
    assert_eq!(report.profile, Some("harmony_300"));
    assert_eq!(report.skin, Some(79));
    assert_eq!(report.usb.product_id, 0xC124);
}

#[test]
fn backup_and_compose_return_typed_reports() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = fake_harmony_300();
    let clock = crate::SystemClock;
    let backup_path = directory.path().join("backup.bin");
    let backup = backup(&mut backend, &clock, &backup_path, false).expect("backup");
    assert_eq!(
        backup.size,
        crate::emit_empty(79, &clock.now()).expect("empty").len()
    );
    assert!(backup.container.exists());
    assert!(backup.sidecar.exists());

    let model_path = directory.path().join("model.json");
    let mut model = blank_model(79);
    model.devices.push(crate::model::Device {
        name: "TV".into(),
        device_key: 1,
        commands: [(
            "power".into(),
            crate::model::Command {
                source: crate::model::IrSource {
                    format: "pronto".into(),
                    file: None,
                    hex: Some("0000 006D 0002 0000 0020 0020 0020 0020".into()),
                    remote: None,
                    button: None,
                    command: None,
                },
                sends: None,
            },
        )]
        .into_iter()
        .collect(),
        keys: [("power".into(), "power".into())].into_iter().collect(),
    });
    model.activity = crate::model::Activity {
        channel_device: "TV".into(),
        steps: vec![crate::model::ActivityStep {
            device: "TV".into(),
            power: Some("power".into()),
            input: None,
            delay_ms: 500,
        }],
    };
    save_model(&model_path, &model).expect("save");
    let container_path = directory.path().join("container.bin");
    let report = compose(&clock, &model_path, &container_path, true).expect("compose");
    assert_eq!(report.devices, 1);
    assert!(report.size > 0);
    assert!(report.container.exists());
    assert!(report.resolved_model.exists());

    let prepared = write(
        &mut backend,
        &clock,
        WriteRequest {
            state_dir: directory.path(),
            container_path: &container_path,
            backup_path: &backup_path,
            commit: false,
            no_reboot: false,
        },
        &mut |_| {},
    )
    .expect("prepare write");
    assert!(!prepared.commit);
    assert!(prepared.bytes > 0);

    let prepared = restore(
        &mut backend,
        &clock,
        RestoreRequest {
            state_dir: directory.path(),
            backup_path: &backup_path,
            commit: false,
            no_reboot: false,
        },
        &mut |_| {},
    )
    .expect("prepare restore");
    assert!(!prepared.commit);
    assert_eq!(prepared.operation, "restore");
}

#[test]
fn list_flipper_commands_matches_what_an_import_accepts() {
    let directory = tempfile::tempdir().expect("tempdir");
    let good = directory.path().join("TV.ir");
    std::fs::write(
        &good,
        "Filetype: IR signals file\nVersion: 1\nname: Power\ntype: raw\nfrequency: 38000\ndata: 9000 4500 560 560\nname: Vol Up\ntype: raw\ndata: 9000 4500 560 1690\n",
    )
    .expect("library file");
    let commands = list_flipper_commands(&good).expect("a valid Flipper file");
    assert_eq!(commands, ["Power", "Vol Up"]);

    // A `name` with no `type` is not a command an import could encode, so it is not offered.
    let bad = directory.path().join("Broken.ir");
    std::fs::write(
        &bad,
        "name: Power\nname: Volume\ntype: raw\ndata: 9000 4500\n",
    )
    .expect("broken file");
    let commands = list_flipper_commands(&bad).expect("a parseable file");
    assert_eq!(
        commands,
        ["Volume"],
        "an entry without a type is not listed"
    );

    // A malformed timing word fails the whole file instead of hiding the problem.
    let malformed = directory.path().join("Malformed.ir");
    std::fs::write(
        &malformed,
        "name: Power\ntype: raw\ndata: 9000 4500 not-a-timing\n",
    )
    .expect("malformed file");
    assert!(list_flipper_commands(&malformed).is_err());
}

#[test]
fn backup_summary_is_typed_and_derives_the_backup_path() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut backend = fake_harmony_300();
    let backup_path = directory.path().join("2026-11-05.bin");
    let clock = SystemClock;
    let report = backup(&mut backend, &clock, &backup_path, false).expect("backup");

    let summary = backup_summary(&report.sidecar).expect("sidecar");
    assert_eq!(summary.backup, backup_path);
    assert_eq!(summary.sidecar, report.sidecar);
    assert_eq!(summary.model, "Harmony 300");
    assert_eq!(summary.skin, Some(79));
    assert_eq!(summary.validation, "ok");
    assert!(summary.size > 0);
    assert_eq!(summary.sha256.len(), 64);

    assert_eq!(
        backup_sidecar(&backup_path),
        report.sidecar,
        "the path helper matches where the sidecar was written"
    );
}
