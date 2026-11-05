use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::container;
use crate::inspect;
use crate::runtime::Runtime;
use crate::{Exit, run};

const BASE: u32 = container::FLASH_BASE;

fn set_u24(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at] = (value & 0xFF) as u8;
    bytes[at + 1] = ((value >> 8) & 0xFF) as u8;
    bytes[at + 2] = ((value >> 16) & 0xFF) as u8;
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u24(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&[value as u8, (value >> 8) as u8, (value >> 16) as u8]);
}

fn write_instruction(bytes: &mut [u8], at: usize, tag: u8, operand: u16, opcode: u8) {
    bytes[at] = tag;
    bytes[at + 1..at + 3].copy_from_slice(&operand.to_le_bytes());
    bytes[at + 3] = opcode;
}

fn seal(bytes: &mut [u8]) {
    let end_addr = BASE + (bytes.len() - 4) as u32;
    bytes[4..8].copy_from_slice(&end_addr.to_le_bytes());
    let checksum_at = bytes.len() - 6;
    bytes[checksum_at..checksum_at + 2].copy_from_slice(&[0, 0]);
    let checksum = container::trailer_checksum(bytes).expect("checksum");
    bytes[checksum_at..checksum_at + 2].copy_from_slice(&checksum.to_le_bytes());
}

/// An Architecture 16 Container with one device map, one IR group, one Watch TV
/// activity, one favourite and two state variables.
fn sample_container() -> Vec<u8> {
    // Header through the key table after `LWJL`. The key table is entry 0 of the
    // key map table.
    let mut bytes = vec![0u8; 0x54];
    bytes[0..4].copy_from_slice(b"GSPM");
    bytes[8] = 0;
    bytes[9] = 15;
    bytes[0x47..0x4B].copy_from_slice(b"LWJL");
    bytes[0x4B] = 2;
    write_instruction(&mut bytes, 0x4C, 0x85, 0xFF01, 0x1F);
    write_instruction(&mut bytes, 0x50, 0x06, 0, 0x7F);

    let mut slot_addresses = [0u32; 15];

    // Raw slot 0: the 0xFEED name tree, not parsed by inspect.
    slot_addresses[0] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[0xED, 0xFE, 0, 0, 0xEF]);

    // Raw slot 1: architecture twice, skin, the constant.
    slot_addresses[1] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[16, 16, 79, 0x0D, 0, 0, 0]);

    // Raw slot 2: the log area.
    slot_addresses[2] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[0x00, 0x20, 0x00, 0x00, 0x07, 0x00, 0x00, 0x08]);

    // Raw slot 3: the 0xADDF framed clock record.
    slot_addresses[3] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[0xDF, 0xAD, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xBF, 0xEF]);

    // Raw slot 4: the infrared database: a count and the group addresses.
    slot_addresses[4] = BASE + bytes.len() as u32;
    bytes.push(1);
    let ir_group_at = bytes.len();
    bytes.extend_from_slice(&[0, 0, 0]);

    // Raw slot 5: the start up event table, empty.
    slot_addresses[5] = BASE + bytes.len() as u32;
    bytes.push(0);

    // Raw slot 6: the key map table. Entry 0 is the key table after LWJL.
    slot_addresses[6] = BASE + bytes.len() as u32;
    bytes.push(2);
    bytes.extend_from_slice(&(BASE + 0x4B).to_le_bytes()[..3]);
    let key_map_one_at = bytes.len();
    bytes.extend_from_slice(&[0, 0, 0]);

    // Raw slot 7: the action list table.
    slot_addresses[7] = BASE + bytes.len() as u32;
    push_u16(&mut bytes, 2);
    let action_list_0_at = bytes.len();
    push_u24(&mut bytes, 0);
    let action_list_1_at = bytes.len();
    push_u24(&mut bytes, 0);

    // Raw slot 8: the timer table, empty.
    slot_addresses[8] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[0, 0]);

    // Raw slot 9: the state variable table, two variables.
    slot_addresses[9] = BASE + bytes.len() as u32;
    push_u16(&mut bytes, 2); // count
    push_u16(&mut bytes, 1); // narrow
    push_u16(&mut bytes, 1); // wide
    push_u16(&mut bytes, 1); // narrow again
    let state_record_0_at = bytes.len();
    push_u24(&mut bytes, 0);
    let state_record_1_at = bytes.len();
    push_u24(&mut bytes, 0);

    // Raw slot 10: the parameter block, empty.
    slot_addresses[10] = BASE + bytes.len() as u32;
    bytes.push(0);

    // Raw slot 11: the number sender table, empty.
    slot_addresses[11] = BASE + bytes.len() as u32;
    bytes.push(0);

    // Raw slot 12: the digit entry table, empty.
    slot_addresses[12] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[0, 0]);

    // Raw slot 13: the metadata archive, left raw.
    slot_addresses[13] = BASE + bytes.len() as u32;
    bytes.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF]);

    // The IR group array and its record, after the section starts.
    let ir_block_at = bytes.len();
    push_u16(&mut bytes, 0x8000 | 9000);
    push_u16(&mut bytes, 4500);
    push_u16(&mut bytes, 0);
    let ir_record_start = bytes.len();
    bytes.push(0); // spare
    push_u24(&mut bytes, 25_000); // carrier period
    push_u24(&mut bytes, 12_500); // carrier on time
    bytes.push(1); // class
    push_u24(&mut bytes, BASE + ir_record_start as u32);
    bytes.push(1); // pointer groups
    push_u24(&mut bytes, BASE + ir_block_at as u32);
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // held and tail NULL

    let ir_group_at_end = bytes.len();
    bytes.push(0); // spare
    push_u16(&mut bytes, 1); // count
    push_u24(&mut bytes, BASE + ir_record_start as u32 + 7);
    set_u24(&mut bytes, ir_group_at, BASE + ir_group_at_end as u32);

    // A wide key map, selected by the release of the first favourite key.
    let key_map_one = bytes.len();
    bytes.extend_from_slice(&[0, 1, 0, 0x88, 0x01, 0x00, 0x7F]);

    // Action list 0: the Watch TV activity, a send and the channel device's map.
    let action_list_0 = bytes.len();
    bytes.push(2);
    push_u16(&mut bytes, 0x0000);
    bytes.push(0x7D);
    push_u16(&mut bytes, 0xFF01);
    bytes.push(0x1F);

    // Action list 1: a favourite channel, the number 101 and the number sender.
    let action_list_1 = bytes.len();
    bytes.push(2);
    push_u16(&mut bytes, 101);
    bytes.push(0x7A);
    push_u16(&mut bytes, 0xF300);
    bytes.push(0x1F);

    // The state records.
    let state_record_0 = bytes.len();
    bytes.extend_from_slice(&[0, 0, 1, 0, 0, 0, 0]);
    let state_record_1 = bytes.len();
    bytes.extend_from_slice(&[0, 0, 1, 0, 1, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 0]);

    // Trailer.
    bytes.extend_from_slice(&[0, 0]);
    bytes.extend_from_slice(b"PTYY");

    // Patch the section table.
    for (index, address) in slot_addresses.iter().enumerate() {
        let at = 0x0B + 4 * index;
        bytes[at] = 0;
        set_u24(&mut bytes, at + 1, *address);
    }
    set_u24(&mut bytes, key_map_one_at, BASE + key_map_one as u32);
    set_u24(&mut bytes, action_list_0_at, BASE + action_list_0 as u32);
    set_u24(&mut bytes, action_list_1_at, BASE + action_list_1 as u32);
    set_u24(&mut bytes, state_record_0_at, BASE + state_record_0 as u32);
    set_u24(&mut bytes, state_record_1_at, BASE + state_record_1 as u32);

    seal(&mut bytes);
    bytes
}

fn run_inspect(file: &Path, extra: &[&str]) -> (Exit, String, String) {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut backend = crate::fake::FakeBackend::new(Vec::new());
    let clock = crate::clock::SystemClock;
    let mut runtime = Runtime {
        backend: &mut backend,
        clock: &clock,
        stdout: &mut stdout,
        stderr: &mut stderr,
        state_dir: PathBuf::from("."),
    };
    let mut args: Vec<OsString> = vec![
        OsString::from("remote-rebirth"),
        OsString::from("inspect"),
        OsString::from("--file"),
        OsString::from(file),
    ];
    args.extend(extra.iter().map(OsString::from));
    let exit = run(&args, &mut runtime);
    (
        exit,
        String::from_utf8(stdout).expect("stdout utf8"),
        String::from_utf8(stderr).expect("stderr utf8"),
    )
}

fn write_container(directory: &tempfile::TempDir, bytes: &[u8]) -> PathBuf {
    let path = directory.path().join("configuration.bin");
    std::fs::write(&path, bytes).expect("container");
    path
}

#[test]
fn inspect_json_reports_architecture_sixteen_container() {
    let directory = tempfile::tempdir().expect("tempdir");
    let bytes = sample_container();
    let path = write_container(&directory, &bytes);
    let (exit, stdout, stderr) = run_inspect(&path, &["--json"]);

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    let value: Value = serde_json::from_str(&stdout).expect("json");
    assert_eq!(value["schema_version"].as_u64(), Some(1));
    assert_eq!(
        value["container"]["size"].as_u64(),
        Some(bytes.len() as u64)
    );
    assert_eq!(
        value["container"]["end_addr"].as_u64(),
        Some((BASE + bytes.len() as u32 - 4) as u64)
    );
    assert_eq!(value["container"]["base"].as_u64(), Some(BASE as u64));
    assert_eq!(value["container"]["slots"].as_u64(), Some(15));
    assert!(value["container"]["checksum"].as_u64().is_some());
    assert_eq!(value["container"]["skin"].as_u64(), Some(79));

    let slots = value["raw_slots"].as_array().expect("raw slots");
    assert_eq!(slots.len(), 15);
    assert_eq!(slots[0]["index"].as_u64(), Some(0));
    assert_eq!(slots[0]["known"].as_bool(), Some(true));
    assert!(slots[0]["offset"].as_u64().is_some());
    assert_eq!(slots[0]["length"].as_u64(), Some(5));
    assert_eq!(
        slots[0]["sha256"].as_str(),
        Some(
            container::sha256_hex(
                &bytes[slots[0]["offset"].as_u64().expect("offset") as usize..][..5]
            )
            .as_str()
        )
    );
    assert!(slots[14]["offset"].is_null());
    assert_eq!(slots[14]["length"].as_u64(), Some(0));
    assert!(slots[14]["known"].as_bool().expect("known"));

    let devices = value["devices"].as_array().expect("devices");
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0]["map_index"].as_u64(), Some(1));
    assert_eq!(devices[0]["ir_group"].as_u64(), Some(0));
    assert_eq!(devices[0]["device_key"]["place"].as_u64(), Some(0));
    assert_eq!(devices[0]["device_key"]["label"].as_str(), Some("TV"));
    assert_eq!(devices[0]["device_key"]["scan_code"].as_u64(), Some(5));
    assert_eq!(devices[0]["record_count"].as_u64(), Some(1));

    let maps = value["key_maps"].as_array().expect("key maps");
    assert_eq!(maps.len(), 2);
    assert_eq!(maps[0]["address"].as_u64(), Some((BASE + 0x4B) as u64));
    assert_eq!(maps[0]["entries"][0]["map_index"].as_u64(), Some(1));
    assert_eq!(maps[1]["wide"].as_bool(), Some(true));
    assert_eq!(maps[1]["entries"][0]["action_list"].as_u64(), Some(1));

    assert_eq!(value["watch_tv_activity"]["action_list"].as_u64(), Some(0));
    assert_eq!(value["watch_tv_activity"]["selected_map"].as_u64(), Some(1));
    assert_eq!(
        value["watch_tv_activity"]["sends"][0]["group"].as_u64(),
        Some(0)
    );

    let favourites = value["favourites"].as_array().expect("favourites");
    assert_eq!(favourites.len(), 1);
    assert_eq!(favourites[0]["channel"].as_str(), Some("101"));
    assert_eq!(favourites[0]["key"].as_u64(), Some(1));
    assert_eq!(favourites[0]["scan_code"].as_u64(), Some(8));

    let records = value["ir_records"].as_array().expect("IR records");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["group"].as_u64(), Some(0));
    assert_eq!(records[0]["index"].as_u64(), Some(0));
    assert_eq!(records[0]["class"].as_u64(), Some(1));
    assert_eq!(records[0]["carrier"]["period_ns"].as_u64(), Some(25_000));
    assert_eq!(records[0]["carrier"]["hertz"].as_u64(), Some(40_000));
    assert_eq!(records[0]["once"]["words"].as_u64(), Some(3));
    assert!(records[0]["held"].is_null());
    assert!(records[0]["tail"].is_null());

    assert_eq!(value["state_variables"]["count"].as_u64(), Some(2));
    assert_eq!(value["state_variables"]["narrow"].as_u64(), Some(1));
    assert_eq!(
        value["state_variables"]["variables"][1]["max"].as_u64(),
        Some(1)
    );
    assert!(
        value["number_senders"]
            .as_array()
            .expect("senders")
            .is_empty()
    );
}

#[test]
fn inspect_rejects_a_file_that_is_not_a_container() {
    let directory = tempfile::tempdir().expect("tempdir");
    let path = write_container(&directory, b"not a container");
    let (exit, stdout, stderr) = run_inspect(&path, &["--json"]);

    assert_eq!(exit, Exit(4));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("GSPM"), "{stderr}");
}

#[test]
fn inspect_rejects_a_checksum_mismatch() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut bytes = sample_container();
    let checksum_at = bytes.len() - 6;
    bytes[checksum_at] ^= 0x01;
    let path = write_container(&directory, &bytes);
    let (exit, stdout, stderr) = run_inspect(&path, &["--json"]);

    assert_eq!(exit, Exit(4));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("checksum"), "{stderr}");
}

#[test]
fn inspect_rejects_a_section_outside_the_container() {
    let directory = tempfile::tempdir().expect("tempdir");
    let mut bytes = sample_container();
    let at = 0x0B + 4 * 5 + 1;
    let outside = BASE + bytes.len() as u32 + 0x1000;
    set_u24(&mut bytes, at, outside);
    seal(&mut bytes);
    let path = write_container(&directory, &bytes);
    let (exit, stdout, stderr) = run_inspect(&path, &["--json"]);

    assert_eq!(exit, Exit(4));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("slot 5"), "{stderr}");
    assert!(stderr.contains("outside"), "{stderr}");
}

#[test]
fn inspect_rejects_a_container_above_the_size_limit() {
    let directory = tempfile::tempdir().expect("tempdir");
    let bytes = vec![0u8; container::MAX_CONTAINER_SIZE + 1];
    let path = write_container(&directory, &bytes);
    let (exit, stdout, stderr) = run_inspect(&path, &["--json"]);

    assert_eq!(exit, Exit(4));
    assert!(stdout.is_empty(), "{stdout}");
    assert!(stderr.contains("256 KiB"), "{stderr}");
}

#[test]
fn inspect_without_json_prints_a_human_summary() {
    let directory = tempfile::tempdir().expect("tempdir");
    let bytes = sample_container();
    let path = write_container(&directory, &bytes);
    let (exit, stdout, stderr) = run_inspect(&path, &[]);

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty(), "{stderr}");
    assert!(!stdout.trim_start().starts_with('{'));
    assert!(stdout.contains("Architecture 16"));
    assert!(stdout.contains("Device Key TV"));
    assert!(stdout.contains("Watch TV Activity"));
    assert!(stdout.contains("IR Records: 1"));
    assert!(stdout.contains("Favourites: 1"));
}

#[test]
fn inspect_print_schema_prints_the_embedded_schema_without_a_file() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut backend = crate::fake::FakeBackend::new(Vec::new());
    let clock = crate::clock::SystemClock;
    let mut runtime = Runtime {
        backend: &mut backend,
        clock: &clock,
        stdout: &mut stdout,
        stderr: &mut stderr,
        state_dir: PathBuf::from("."),
    };
    let args: Vec<OsString> = ["remote-rebirth", "inspect", "--print-schema"]
        .iter()
        .map(OsString::from)
        .collect();
    let exit = run(&args, &mut runtime);

    assert_eq!(exit, Exit(0));
    assert!(stderr.is_empty());
    let stdout = String::from_utf8(stdout).expect("stdout utf8");
    assert_eq!(stdout, inspect::SCHEMA);
    let schema: Value = serde_json::from_str(&stdout).expect("schema json");
    assert!(schema["$id"].as_str().is_some());
    assert!(schema["properties"]["schema_version"].is_object());
}

#[test]
fn inspect_requires_a_file_without_print_schema() {
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let mut backend = crate::fake::FakeBackend::new(Vec::new());
    let clock = crate::clock::SystemClock;
    let mut runtime = Runtime {
        backend: &mut backend,
        clock: &clock,
        stdout: &mut stdout,
        stderr: &mut stderr,
        state_dir: PathBuf::from("."),
    };
    let args: Vec<OsString> = ["remote-rebirth", "inspect"]
        .iter()
        .map(OsString::from)
        .collect();
    let exit = run(&args, &mut runtime);

    assert_eq!(exit, Exit(2));
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("--file"));
}
