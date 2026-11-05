//! Behavioural tests for `compose`. They drive the public CLI seam and the public inspect seam, and
//! assert the Container's bytes, its parse back, and the resolved-model sidecar. Every IR Source is
//! inline Pronto hex generated for these tests; no Logitech catalogue data is used.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::clock::Clock;
use crate::container::{FLASH_BASE, MAX_CONTAINER_SIZE, ValidationStatus, trailer_checksum};
use crate::fake::FakeBackend;
use crate::runtime::Runtime;
use crate::{Exit, run};

// Two-pulse raw Pronto codes at the reference's 0x6D divider. Each is a distinct capture.
const RAW_TV_POWER: &str = "0000 006D 0002 0000 0157 00AB 0016 0041";
const RAW_TV_INPUT: &str = "0000 006D 0002 0000 0156 00AB 0016 0042";
const RAW_VOLUME_UP: &str = "0000 006D 0002 0000 0155 00AC 0015 0043";
const RC5_TV: &str = "5000 0000 0000 0001 0004 000C";
const NEC_CABLE: &str = "9000 0000 0000 0002 0004 00FB 0008 00F7";

struct FixedClock(time::OffsetDateTime);

impl Clock for FixedClock {
    fn now(&self) -> time::OffsetDateTime {
        self.0
    }
}

fn fixed_clock() -> FixedClock {
    FixedClock(time::OffsetDateTime::from_unix_timestamp(1_800_000_000).expect("a fixed timestamp"))
}

fn run_args(args: &[&str]) -> (Exit, String, String) {
    let mut backend = FakeBackend::new(Vec::new());
    let clock = fixed_clock();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut runtime = Runtime {
        backend: &mut backend,
        clock: &clock,
        stdout: &mut stdout,
        stderr: &mut stderr,
        state_dir: PathBuf::from("."),
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

fn write_model(directory: &Path, model: &Value) -> PathBuf {
    let path = directory.join("model.json");
    std::fs::write(&path, serde_json::to_vec_pretty(model).expect("model json"))
        .expect("write model");
    path
}

fn compose_args<'a>(model: &'a Path, out: &'a Path, extra: &[&'a str]) -> Vec<String> {
    let mut args = vec![
        "compose".to_string(),
        "--model".to_string(),
        model.display().to_string(),
        "--out".to_string(),
        out.display().to_string(),
    ];
    args.extend(extra.iter().map(|one| one.to_string()));
    args
}

fn compose(model: &Path, out: &Path, extra: &[&str]) -> (Exit, String, String) {
    let args = compose_args(model, out, extra);
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    run_args(&borrowed)
}

fn sidecar(out: &Path) -> PathBuf {
    PathBuf::from(format!("{}.model.json", out.display()))
}

fn inspect_json(path: &Path) -> Value {
    let mut stdout = Vec::new();
    crate::inspect::run(Some(path), true, false, &mut stdout)
        .expect("inspect parses the Container");
    serde_json::from_slice(&stdout).expect("inspect JSON")
}

/// Two Devices on key places 1 and 2, one Watch TV Activity and, when `favourites` is true, two
/// Favourite channels.
fn two_device_model(skin: u8, favourites: bool) -> Value {
    let mut value = json!({
        "schema_version": 1,
        "profile": "harmony_300",
        "skin": skin,
        "devices": [
            {
                "name": "TV",
                "device_key": 1,
                "commands": {
                    "power": { "source": { "format": "pronto", "hex": RAW_TV_POWER } },
                    "input": { "source": { "format": "pronto", "hex": RAW_TV_INPUT } },
                    "volume_up": { "source": { "format": "pronto", "hex": RC5_TV } }
                },
                "keys": {
                    "power": "power",
                    "input": "input",
                    "volume_up": "volume_up"
                }
            },
            {
                "name": "Cable",
                "device_key": 2,
                "commands": {
                    "power": { "source": { "format": "pronto", "hex": RAW_VOLUME_UP } },
                    "number_0": { "source": { "format": "pronto", "hex": RAW_TV_POWER } },
                    "number_1": { "source": { "format": "pronto", "hex": NEC_CABLE }, "sends": 2 }
                },
                "keys": {
                    "power": "power",
                    "number_0": "number_0",
                    "number_1": "number_1"
                }
            }
        ],
        "activity": {
            "channel_device": "Cable",
            "steps": [
                { "device": "TV", "power": "power", "input": "input", "delay_ms": 500 },
                { "device": "Cable", "power": "power", "delay_ms": 1000 }
            ]
        }
    });
    if favourites {
        value["favourites"] = json!([
            { "key": 1, "channel": "101" },
            { "key": 2, "channel": "102" }
        ]);
    }
    value
}

#[test]
fn compose_writes_a_checksummed_architecture_16_container() {
    let directory = tempfile::tempdir().expect("tempdir");
    let model = write_model(directory.path(), &two_device_model(79, true));
    let out = directory.path().join("container.bin");

    let (exit, stdout, stderr) = compose(&model, &out, &["--json"]);
    assert_eq!(exit, Exit(0), "{stderr}");
    assert!(stderr.is_empty(), "{stderr}");

    let bytes = std::fs::read(&out).expect("container");
    let output: Value = serde_json::from_str(&stdout).expect("compose JSON");
    assert_eq!(output["schema_version"].as_u64(), Some(1));
    assert_eq!(
        output["container"].as_str(),
        Some(out.display().to_string().as_str())
    );
    assert_eq!(
        output["resolved_model"].as_str(),
        Some(sidecar(&out).display().to_string().as_str())
    );
    assert_eq!(output["size"].as_u64(), Some(bytes.len() as u64));
    assert_eq!(
        output["sha256"].as_str(),
        Some(crate::container::sha256_hex(&bytes).as_str())
    );
    assert_eq!(output["devices"].as_u64(), Some(2));
    assert_eq!(output["favourites"].as_u64(), Some(2));
    assert_eq!(&bytes[0..4], b"GSPM");
    assert_eq!(&bytes[0x47..0x4B], b"LWJL");
    assert_eq!(&bytes[bytes.len() - 4..], b"PTYY");
    assert_eq!(
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        FLASH_BASE + bytes.len() as u32 - 4,
        "end_addr names the PTYY marker"
    );
    assert_eq!(
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        15 << 8,
        "fifteen pointer slots"
    );
    let stored = u16::from_le_bytes([bytes[bytes.len() - 6], bytes[bytes.len() - 5]]);
    assert_eq!(
        stored,
        trailer_checksum(&bytes).expect("a complete trailer"),
        "the stored checksum covers everything before it"
    );
    assert_eq!(
        crate::container::validation_status(&bytes),
        ValidationStatus::Ok
    );
    // A golden length: this project's own Compose output for the two-Device model, the shape the
    // spec says to keep a fixture of.
    assert_eq!(bytes.len(), 1796, "the composed Container's size");
    assert!(bytes.len() < MAX_CONTAINER_SIZE);

    // The same model at the same moment is one file.
    let second = directory.path().join("second.bin");
    let (exit, _, _) = compose(&model, &second, &[]);
    assert_eq!(exit, Exit(0));
    assert_eq!(std::fs::read(&second).expect("second"), bytes);
}

#[test]
fn inspect_reports_the_composed_devices_keys_activity_and_favourites() {
    let directory = tempfile::tempdir().expect("tempdir");
    let model = write_model(directory.path(), &two_device_model(79, true));
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(0), "{stderr}");

    let value = inspect_json(&out);
    assert_eq!(value["container"]["skin"].as_u64(), Some(79));
    assert_eq!(value["container"]["architecture"].as_u64(), Some(16));

    let devices = value["devices"].as_array().expect("devices");
    assert_eq!(devices.len(), 2, "one map per Device");
    assert_eq!(devices[0]["device_key"]["place"].as_u64(), Some(0));
    assert_eq!(devices[0]["device_key"]["event"].as_str(), Some("press"));
    assert_eq!(devices[0]["ir_group"].as_u64(), Some(0));
    assert_eq!(devices[0]["record_count"].as_u64(), Some(3));
    assert_eq!(devices[1]["device_key"]["place"].as_u64(), Some(1));
    assert_eq!(devices[1]["ir_group"].as_u64(), Some(1));
    assert_eq!(devices[1]["record_count"].as_u64(), Some(3));

    // The key table selects both maps and binds Watch TV to its activity list.
    let key_table = value["key_table"]["entries"].as_array().expect("key table");
    let mut selects: Vec<(u64, Option<u64>)> = key_table
        .iter()
        .filter(|entry| entry["opcode"].as_u64() == Some(0x1F))
        .map(|entry| {
            (
                entry["scan_code"].as_u64().expect("scan"),
                entry["map_index"].as_u64(),
            )
        })
        .collect();
    selects.sort();
    assert_eq!(selects, vec![(1, Some(2)), (5, Some(1))]);
    assert!(
        key_table
            .iter()
            .any(|entry| entry["tag"].as_u64() == Some(0x06)
                && entry["opcode"].as_u64() == Some(0x7F)),
        "Watch TV calls its activity list"
    );
    assert!(
        key_table
            .iter()
            .any(|entry| entry["tag"].as_u64() == Some(0xFF)),
        "event 0xFF is acknowledged"
    );

    let maps = value["key_maps"].as_array().expect("maps");
    assert_eq!(maps.len(), 3, "the key table and one map per Device");
    let scans = |map: usize| -> Vec<u64> {
        let mut scans: Vec<u64> = maps[map]["entries"]
            .as_array()
            .expect("map entries")
            .iter()
            .filter(|entry| entry["opcode"].as_u64() == Some(0x7F))
            .map(|entry| entry["scan_code"].as_u64().expect("scan"))
            .collect();
        scans.sort();
        scans
    };
    assert_eq!(
        scans(1),
        vec![3, 35, 49],
        "TV binds power, input and volume up"
    );
    assert_eq!(
        scans(2),
        vec![3, 8, 9, 20, 24],
        "Cable binds power, both digits and both Favourites"
    );

    let activity = &value["watch_tv_activity"];
    assert_eq!(activity["selected_map"].as_u64(), Some(2));
    assert_eq!(
        activity["sends"].as_array().expect("activity sends").len(),
        3,
        "two TV sends and one Cable send"
    );
    // The list's own instructions are the two steps' waits and the channel Device's map; the send
    // lists' quantity instructions are reached through the calls, so they appear in  too.
    let waits: Vec<(u64, u64)> = activity["instructions"]
        .as_array()
        .expect("activity instructions")
        .iter()
        .filter(|instruction| instruction["opcode"].as_u64() == Some(0x7C))
        .map(|instruction| {
            (
                instruction["delay"]["group"].as_u64().expect("group"),
                instruction["delay"]["tenths"].as_u64().expect("tenths"),
            )
        })
        .collect();
    assert_eq!(
        waits,
        vec![(0, 5), (1, 10)],
        "500 ms is five tenths and 1000 ms is ten"
    );
    assert!(
        activity["delays"]
            .as_array()
            .expect("activity delays")
            .iter()
            .any(|delay| delay["group"].as_u64() == Some(1)
                && delay["tenths"].as_u64() == Some(10)),
        "the waits are collected through the list"
    );

    let favourites = value["favourites"].as_array().expect("favourites");
    assert_eq!(favourites.len(), 2);
    let by_key: Vec<(u64, &str, u64)> = favourites
        .iter()
        .map(|one| {
            (
                one["key"].as_u64().expect("key"),
                one["channel"].as_str().expect("channel"),
                one["map_index"].as_u64().expect("map"),
            )
        })
        .collect();
    let mut by_key = by_key;
    by_key.sort();
    assert_eq!(by_key, vec![(1, "101", 2), (2, "102", 2)]);

    let records = value["ir_records"].as_array().expect("IR Records");
    assert_eq!(records.len(), 6, "one record per Command");
    assert!(
        records.iter().any(|record| record["toggle"].is_object()),
        "the RC5 code carries both toggle groups"
    );

    let senders = value["number_senders"].as_array().expect("number senders");
    assert_eq!(senders.len(), 1, "one number sender record");
    let tables = senders[0]["digit_tables"].as_array().expect("digit tables");
    assert_eq!(tables.len(), 3);
    for table in tables {
        assert_eq!(table["instructions"].as_array().expect("digits").len(), 10);
    }
    assert_eq!(senders[0]["flags"].as_u64(), Some(0));
    assert_eq!(senders[0]["digits"].as_u64(), Some(0));
}

#[test]
fn the_resolved_model_records_each_commands_provenance() {
    let directory = tempfile::tempdir().expect("tempdir");
    let model = write_model(directory.path(), &two_device_model(79, true));
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(0), "{stderr}");

    let resolved: Value =
        serde_json::from_slice(&std::fs::read(sidecar(&out)).expect("sidecar")).expect("json");
    assert_eq!(resolved["schema_version"].as_u64(), Some(1));
    assert_eq!(resolved["profile"].as_str(), Some("harmony_300"));
    assert_eq!(resolved["skin"].as_u64(), Some(79));

    let tv = &resolved["devices"][0];
    assert_eq!(tv["name"].as_str(), Some("TV"));
    assert_eq!(tv["ir_group"].as_u64(), Some(0));
    assert_eq!(tv["map_index"].as_u64(), Some(1));

    let power = &tv["commands"]["power"];
    assert_eq!(power["provenance"]["format"].as_str(), Some("pronto"));
    assert_eq!(power["provenance"]["path"], Value::Null);
    assert_eq!(power["provenance"]["selector"].as_str(), Some("inline hex"));
    assert_eq!(power["provenance"]["protocol"].as_str(), Some("raw"));
    assert_eq!(power["provenance"]["sends"].as_u64(), Some(1));
    assert_eq!(
        power["provenance"]["sha256"].as_str(),
        Some(crate::container::sha256_hex(RAW_TV_POWER.as_bytes()).as_str()),
        "the digest covers the source bytes"
    );
    assert_eq!(power["record"]["group"].as_u64(), Some(0));
    assert_eq!(
        power["record"]["index"].as_u64(),
        Some(1),
        "the TV Commands sort input, power, volume_up"
    );
    assert_eq!(power["record"]["class"].as_u64(), Some(1));
    assert_eq!(power["record"]["pointer_groups"].as_u64(), Some(1));

    let volume = &tv["commands"]["volume_up"];
    assert_eq!(volume["provenance"]["protocol"].as_str(), Some("RC5"));
    assert_eq!(volume["record"]["pointer_groups"].as_u64(), Some(2));
    assert_eq!(volume["record"]["index"].as_u64(), Some(2));

    let cable = &resolved["devices"][1];
    assert_eq!(cable["ir_group"].as_u64(), Some(1));
    assert_eq!(cable["map_index"].as_u64(), Some(2));
    let one = &cable["commands"]["number_1"];
    assert_eq!(one["provenance"]["protocol"].as_str(), Some("NEC"));
    assert_eq!(
        one["provenance"]["sends"].as_u64(),
        Some(2),
        "the model's sends override the protocol's default"
    );
}

#[test]
fn compose_refuses_to_overwrite_without_the_flag() {
    let directory = tempfile::tempdir().expect("tempdir");
    let model = write_model(directory.path(), &two_device_model(79, true));
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(0), "{stderr}");
    let first = std::fs::read(&out).expect("container");
    let first_sidecar = std::fs::read(sidecar(&out)).expect("sidecar");

    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(4), "an existing output is a validation refusal");
    assert!(stderr.contains("already exists"), "{stderr}");
    assert!(stderr.contains("--overwrite"), "{stderr}");
    assert_eq!(std::fs::read(&out).expect("container"), first);
    assert_eq!(
        std::fs::read(sidecar(&out)).expect("sidecar"),
        first_sidecar
    );

    let (exit, _, stderr) = compose(&model, &out, &["--overwrite"]);
    assert_eq!(exit, Exit(0), "{stderr}");
}

#[test]
fn compose_refuses_a_container_larger_than_the_256_kib_region() {
    let directory = tempfile::tempdir().expect("tempdir");
    // Eighteen records of 7616 duration words each: about 274 KiB of blocks, past the region.
    let pairs = 3808usize;
    let mut hex = format!("0000 006D {pairs:04X} 0000 ");
    for _ in 0..pairs {
        hex.push_str("0020 0020 ");
    }
    let keys = [
        "power",
        "volume_up",
        "volume_down",
        "volume_mute",
        "channel_up",
        "channel_down",
        "number_0",
        "number_1",
        "number_2",
        "number_3",
        "number_4",
        "number_5",
        "number_6",
        "number_7",
        "number_8",
        "number_9",
        "menu",
        "exit",
    ];
    let mut commands = serde_json::Map::new();
    let mut bindings = serde_json::Map::new();
    for (index, key) in keys.iter().enumerate() {
        let name = format!("command_{index}");
        commands.insert(
            name.clone(),
            json!({ "source": { "format": "pronto", "hex": hex } }),
        );
        bindings.insert(key.to_string(), Value::String(name.clone()));
    }
    let model = write_model(
        directory.path(),
        &json!({
            "schema_version": 1,
            "profile": "harmony_300",
            "skin": 79,
            "devices": [{
                "name": "TV",
                "device_key": 1,
                "commands": Value::Object(commands),
                "keys": Value::Object(bindings)
            }],
            "activity": {
                "channel_device": "TV",
                "steps": [{ "device": "TV", "power": "command_0" }]
            }
        }),
    );
    let out = directory.path().join("container.bin");

    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(
        exit,
        Exit(4),
        "an oversized Container is a validation refusal"
    );
    assert!(stderr.contains("262144"), "{stderr}");
    assert!(stderr.contains("bytes"), "{stderr}");
    assert!(
        !out.exists(),
        "nothing is written when the Container is refused"
    );
    assert!(!sidecar(&out).exists());
}

#[test]
fn compose_rejects_a_model_the_profile_refuses() {
    let directory = tempfile::tempdir().expect("tempdir");
    let out = directory.path().join("container.bin");

    let mut five = two_device_model(79, false);
    five["favourites"] = json!(
        (1..=5)
            .map(|key| json!({ "key": key, "channel": format!("10{key}") }))
            .collect::<Vec<_>>()
    );
    let model = write_model(directory.path(), &five);
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("skin 79 accepts 4"), "{stderr}");
    assert!(!out.exists());

    let mut duplicate = two_device_model(79, false);
    duplicate["devices"][1]["device_key"] = json!(1);
    let model = write_model(directory.path(), &duplicate);
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("device key 1"), "{stderr}");
    assert!(!out.exists());

    let mut leading_zero = two_device_model(79, true);
    leading_zero["favourites"][0]["channel"] = json!("01");
    let model = write_model(directory.path(), &leading_zero);
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("leading zero"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn compose_composes_five_favourites_for_skin_78() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut model = two_device_model(78, false);
    model["favourites"] = json!([
        { "key": 1, "channel": "101" },
        { "key": 2, "channel": "102" },
        { "key": 3, "channel": "103" },
        { "key": 4, "channel": "104" },
        { "key": 5, "channel": "105" }
    ]);
    let model = write_model(directory.path(), &model);
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(0), "{stderr}");

    let value = inspect_json(&out);
    assert_eq!(value["container"]["skin"].as_u64(), Some(78));
    let favourites = value["favourites"].as_array().expect("favourites");
    assert_eq!(favourites.len(), 5);
    let mut by_key: Vec<(u64, String)> = favourites
        .iter()
        .map(|one| {
            (
                one["key"].as_u64().expect("key"),
                one["channel"].as_str().expect("channel").to_string(),
            )
        })
        .collect();
    by_key.sort();
    assert_eq!(
        by_key,
        vec![
            (1, "101".to_string()),
            (2, "102".to_string()),
            (3, "103".to_string()),
            (4, "104".to_string()),
            (5, "105".to_string())
        ],
        "the five positional keys of the US printing"
    );
}

#[test]
fn compose_imports_a_file_source_relative_to_the_model() {
    let directory = tempfile::tempdir().expect("tempdir");
    std::fs::create_dir(directory.path().join("irs")).expect("irs directory");
    std::fs::write(
        directory.path().join("irs/tv.conf"),
        "begin remote\n  name TV\n  flags SPACE_ENC\n  header 9000 4500\n  one 560 1690\n  zero 560 560\n  bits 32\n  frequency 38000\n  begin codes\n    Input 0x20DF40BF\n  end codes\nend remote\n",
    )
    .expect("lirc file");
    let model = write_model(
        directory.path(),
        &json!({
            "schema_version": 1,
            "profile": "harmony_300",
            "skin": 79,
            "devices": [{
                "name": "TV",
                "device_key": 1,
                "commands": {
                    "power": { "source": { "format": "pronto", "hex": RAW_TV_POWER } },
                    "input": { "source": { "format": "lirc", "file": "irs/tv.conf", "remote": "TV", "button": "Input" } }
                },
                "keys": { "power": "power", "input": "input" }
            }],
            "activity": {
                "channel_device": "TV",
                "steps": [{ "device": "TV", "power": "power", "input": "input" }]
            }
        }),
    );
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(0), "{stderr}");
    inspect_json(&out);

    let resolved: Value =
        serde_json::from_slice(&std::fs::read(sidecar(&out)).expect("sidecar")).expect("json");
    let input = &resolved["devices"][0]["commands"]["input"];
    assert_eq!(input["provenance"]["format"].as_str(), Some("lirc"));
    assert_eq!(input["provenance"]["protocol"].as_str(), Some("NEC"));
    assert_eq!(input["provenance"]["sends"].as_u64(), Some(3));
    assert!(
        input["provenance"]["path"]
            .as_str()
            .is_some_and(|path| path.ends_with("irs/tv.conf")),
        "the resolved path names the file beside the model"
    );
    assert_eq!(
        input["provenance"]["selector"].as_str(),
        Some("remote=TV button=Input")
    );
}

#[test]
fn compose_refuses_a_key_the_profile_cannot_bind() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut model = two_device_model(79, false);
    model["devices"][0]["keys"]["live"] = json!("power");
    let model = write_model(directory.path(), &model);
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("'live'"), "{stderr}");
    assert!(stderr.contains("no scan code"), "{stderr}");
    assert!(!out.exists());
}

#[test]
fn compose_refuses_a_favourite_whose_scan_the_channel_device_binds() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut model = two_device_model(78, false);
    model["devices"][1]["commands"]["teletext"] =
        json!({ "source": { "format": "pronto", "hex": RAW_TV_INPUT } });
    model["devices"][1]["keys"]["teletext"] = json!("teletext");
    model["favourites"] = json!([
        { "key": 1, "channel": "101" },
        { "key": 2, "channel": "102" },
        { "key": 3, "channel": "103" }
    ]);
    let model = write_model(directory.path(), &model);
    let out = directory.path().join("container.bin");
    let (exit, _, stderr) = compose(&model, &out, &[]);
    assert_eq!(exit, Exit(4));
    assert!(stderr.contains("scan 11"), "{stderr}");
    assert!(stderr.contains("teletext"), "{stderr}");
    assert!(!out.exists());
}
