use super::*;

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
fn model_accepts_unreferenced_command() {
    let mut model = valid_model();
    model.devices[0].commands.insert(
        "unused".into(),
        serde_json::from_value(serde_json::json!({
            "source": { "format": "pronto", "hex": "0000 006D 0002 0000 0020 0020 0020 0020" }
        }))
        .expect("command"),
    );
    crate::model::validate(&model, &crate::profile::HARMONY_300).expect("unused allowed");
    assert_eq!(
        crate::model::unused_commands(&model),
        vec![("TV".to_string(), "unused".to_string())]
    );
}

fn model_with_volume_bindings() -> crate::model::Model {
    let mut model = valid_model();
    for key in crate::model::Routing::VOLUME_KEYS {
        model.devices[0].commands.insert(
            (*key).into(),
            serde_json::from_value(serde_json::json!({
                "source": { "format": "pronto", "hex": "0000 006D 0002 0000 0020 0020 0020 0020" }
            }))
            .expect("command"),
        );
        model.devices[0].keys.insert((*key).into(), (*key).into());
    }
    model
}

#[test]
fn model_accepts_v2_routing() {
    let mut model = model_with_volume_bindings();
    model.schema_version = 2;
    model.routing = Some(crate::model::Routing {
        volume: Some("TV".into()),
        channel: None,
    });
    crate::model::validate(&model, &crate::profile::HARMONY_300).expect("routing");
}

#[test]
fn model_rejects_routing_in_v1() {
    let mut model = model_with_volume_bindings();
    model.routing = Some(crate::model::Routing {
        volume: Some("TV".into()),
        channel: None,
    });
    let error =
        crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("v1 routing");
    assert!(error.to_string().contains("schema_version 1"));
}

#[test]
fn model_rejects_missing_routing_target() {
    let mut model = valid_model();
    model.schema_version = 2;
    model.routing = Some(crate::model::Routing {
        volume: Some("Missing".into()),
        channel: None,
    });
    let error =
        crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("missing target");
    assert!(error.to_string().contains("missing Device 'Missing'"));
}

#[test]
fn model_rejects_target_missing_required_binding() {
    let mut model = valid_model();
    model.schema_version = 2;
    model.routing = Some(crate::model::Routing {
        volume: Some("TV".into()),
        channel: None,
    });
    let error =
        crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("missing binding");
    assert!(error.to_string().contains("does not bind key 'volume_up'"));
}

#[test]
fn model_rejects_non_target_routed_key() {
    let mut model = model_with_volume_bindings();
    model.schema_version = 2;
    model.routing = Some(crate::model::Routing {
        volume: Some("TV".into()),
        channel: None,
    });
    let mut cable = model.devices[0].clone();
    cable.name = "Cable".into();
    cable.device_key = 2;
    cable.commands.clear();
    cable.keys.clear();
    cable.commands.insert(
        "power".into(),
        serde_json::from_value(serde_json::json!({
            "source": { "format": "pronto", "hex": "0000 006D 0002 0000 0020 0020 0020 0020" }
        }))
        .expect("command"),
    );
    cable.keys.insert("volume_up".into(), "power".into());
    model.devices.push(cable);
    let error = crate::model::validate(&model, &crate::profile::HARMONY_300).expect_err("conflict");
    assert!(error.to_string().contains("routing volume assigns to 'TV'"));
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
        serde_json::from_str(include_str!("../../schema/model-v1.json")).expect("schema");
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
fn model_v2_schema_accepts_routing_and_rejects_v1_routing() {
    let schema: Value =
        serde_json::from_str(include_str!("../../schema/model-v2.json")).expect("schema");
    let validator = jsonschema::validator_for(&schema).expect("compile schema");

    let mut v2 = valid_model_json();
    v2["schema_version"] = json!(2);
    v2["routing"] = json!({ "volume": "TV" });
    validator.validate(&v2).expect("v2 routing validates");

    let mut v1 = valid_model_json();
    v1["routing"] = json!({ "volume": "TV" });
    assert!(
        validator.validate(&v1).is_err(),
        "v1 must not declare routing"
    );
}

#[cfg(feature = "cli")]
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
        serde_json::from_str(include_str!("../../schema/backup-v1.json")).expect("schema");
    let validator = jsonschema::validator_for(&schema).expect("compile schema");
    validator.validate(&sidecar).expect("sidecar validates");
}
