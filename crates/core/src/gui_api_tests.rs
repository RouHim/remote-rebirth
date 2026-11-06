//! Behavioural tests for the typed GUI API.

use crate::gui_api::*;

use super::*;
use crate::fake::{FakeBackend, FakeDevice};
use crate::transport::RemoteDescriptor;

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
        directory.path(),
        &container_path,
        &backup_path,
        false,
        false,
        &mut |_| {},
    )
    .expect("prepare write");
    assert!(!prepared.commit);
    assert!(prepared.bytes > 0);

    let prepared = restore(
        &mut backend,
        &clock,
        directory.path(),
        &backup_path,
        false,
        false,
        &mut |_| {},
    )
    .expect("prepare restore");
    assert!(!prepared.commit);
    assert_eq!(prepared.operation, "restore");
}
