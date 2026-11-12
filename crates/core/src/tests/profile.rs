use super::*;

// ---------------------------------------------------------------------------
// A test-only Model Profile proves Detect, Compose and validation are data-driven.
// ---------------------------------------------------------------------------

#[cfg(feature = "cli")]
const PRONTO_POWER: &str = "0000 006D 0002 0000 0020 0020 0020 0020";

#[cfg(feature = "cli")]
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

#[cfg(feature = "cli")]
pub(super) fn compose_with_test_profile(
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

#[cfg(feature = "cli")]
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
