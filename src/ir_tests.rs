use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::container;
use crate::ir::{
    CLASS_STREAM, DURATION_MASK, IrError, IrRecord, IrSource, IrSourceKind, MARK_BIT, Pulse,
    import, pack_block,
};

fn pronto(hex: &str) -> IrSource {
    IrSource {
        format: IrSourceKind::Pronto,
        file: None,
        hex: Some(hex.to_string()),
        remote: None,
        button: None,
        command: None,
    }
}

fn lirc(file: &str, remote: &str, button: &str) -> IrSource {
    IrSource {
        format: IrSourceKind::Lirc,
        file: Some(file.to_string()),
        hex: None,
        remote: Some(remote.to_string()),
        button: Some(button.to_string()),
        command: None,
    }
}

fn flipper(file: &str, command: &str) -> IrSource {
    IrSource {
        format: IrSourceKind::Flipper,
        file: Some(file.to_string()),
        hex: None,
        remote: None,
        button: None,
        command: Some(command.to_string()),
    }
}

fn write_file(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, body).expect("fixture file");
    path
}

fn import_inline(source: &IrSource) -> crate::ir::ImportedCommand {
    import(source, Path::new("."), None).expect("IR source imports")
}

fn import_file(dir: &TempDir, source: &IrSource, sends: Option<u8>) -> crate::ir::ImportedCommand {
    import(source, dir.path(), sends).expect("IR source imports")
}

fn words_to_pulses(words: &[u16]) -> Vec<Pulse> {
    words
        .iter()
        .map(|word| Pulse {
            mark: word & MARK_BIT != 0,
            micros: u32::from(word & DURATION_MASK),
        })
        .collect()
}

fn count_marks_near(words: &[u16], target: u32) -> usize {
    words_to_pulses(words)
        .iter()
        .filter(|pulse| pulse.mark && pulse.micros.abs_diff(target) < 100)
        .count()
}

#[test]
fn pronto_raw_import_produces_one_class_one_pointer_group() {
    let command = import_inline(&pronto("0000 006D 0002 0000 0157 00AB 0016 0041"));

    assert_eq!(command.record.class, CLASS_STREAM);
    assert_eq!(command.record.pointer_group_count(), 1);
    assert_eq!(command.record.carrier_period_ns, 26_296);
    assert_eq!(command.record.carrier_on_ns, 13_148);
    assert_eq!(command.provenance.protocol, "raw");
    assert_eq!(command.provenance.sends, 1);
    let group = &command.record.groups[0];
    assert_eq!(
        group.once,
        vec![0xA33B, 0x1191, 0x8243, 0x06AD],
        "9019 us mark, 4497 us space, 579 us mark, 1709 us space"
    );
    assert!(group.held.is_none());
}

#[test]
fn pronto_raw_import_keeps_once_and_repeat_sequences_apart() {
    let command = import_inline(&pronto("0000 006D 0001 0001 0156 00AB 0016 0041"));

    let group = &command.record.groups[0];
    assert_eq!(group.once, vec![0xA321, 0x1191]);
    assert_eq!(group.held, Some(vec![0x8243, 0x06AD]));
}

#[test]
fn pronto_rc5_import_emits_both_toggle_groups() {
    let command = import_inline(&pronto("5000 0000 0000 0001 0004 000C"));

    assert_eq!(command.provenance.protocol, "RC5");
    assert_eq!(command.provenance.sends, 1);
    assert_eq!(command.record.pointer_group_count(), 2);
    assert_eq!(command.record.carrier_period_ns, 27_777);
    assert_ne!(
        command.record.groups[0].once, command.record.groups[1].once,
        "the second group carries the flipped toggle bit"
    );
}

#[test]
fn pronto_nec_import_sends_three_presses() {
    let command = import_inline(&pronto("9000 0000 0000 0002 0004 00FB 0008 00F7"));

    assert_eq!(command.provenance.protocol, "NEC");
    assert_eq!(command.provenance.sends, 3);
    assert_eq!(count_marks_near(&command.record.groups[0].once, 9000), 3);
    assert_eq!(
        count_marks_near(
            command.record.groups[0]
                .held
                .as_deref()
                .expect("held block"),
            9000
        ),
        1
    );
}

#[test]
fn pronto_unsupported_format_names_the_word_and_selector() {
    for word in ["6000", "900A", "5001"] {
        let error = import_inline_error(&pronto(&format!("{word} 0000 0000 0001 0000 0000")));
        assert!(
            matches!(
                &error,
                IrError::UnsupportedProntoFormat { word: found, selector, .. }
                    if *found == u16::from_str_radix(word, 16).expect("word") && selector == "inline hex"
            ),
            "{word}: {error}"
        );
        assert!(error.to_string().contains(word));
    }
}

fn import_inline_error(source: &IrSource) -> IrError {
    import(source, Path::new("."), None).expect_err("IR source must be rejected")
}

const LIRC_RAW: &str = "begin remote
  name TV
  flags RAW_CODES
  frequency 38000
  begin raw_codes
    name Power
    9000 4500 560 560 560 1690
    name Input
    2400 600 600 600
  end raw_codes
end remote
";

#[test]
fn lirc_raw_codes_import_preserves_the_capture() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.conf", LIRC_RAW);
    let command = import_file(&dir, &lirc("tv.conf", "TV", "Power"), None);

    assert_eq!(command.provenance.protocol, "raw");
    assert_eq!(command.provenance.sends, 1);
    assert_eq!(
        command.record.groups[0].once,
        vec![0xA328, 0x1194, 0x8230, 0x0230, 0x8230, 0x069A],
        "9000/4500/560/560/560/1690 us, mark bit on odd words"
    );
    assert!(
        command
            .provenance
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with("tv.conf"))
    );
}

const LIRC_CODES: &str = "begin remote
  name TV
  flags SPACE_ENC
  header 9000 4500
  one 560 1690
  zero 560 560
  bits 32
  frequency 38000
  begin codes
    Power 0x20DF10EF
    Input 0x20DF40BF
  end codes
end remote
";

#[test]
fn lirc_space_encoded_codes_import_as_nec() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.conf", LIRC_CODES);
    let command = import_file(&dir, &lirc("tv.conf", "TV", "Power"), None);

    assert_eq!(command.provenance.protocol, "NEC");
    assert_eq!(command.provenance.sends, 3);
    assert_eq!(count_marks_near(&command.record.groups[0].once, 9000), 3);
}

#[test]
fn lirc_rc5_codes_emit_both_toggle_groups() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(
        dir.path(),
        "rc5.conf",
        "begin remote
  name Philips
  flags RC5
  frequency 36000
  begin codes
    Power 0x0C0C
  end codes
end remote
",
    );
    let command = import_file(&dir, &lirc("rc5.conf", "Philips", "Power"), None);

    assert_eq!(command.provenance.protocol, "RC5");
    assert_eq!(command.provenance.sends, 1);
    assert_eq!(command.record.pointer_group_count(), 2);
    assert_ne!(command.record.groups[0].once, command.record.groups[1].once);
}

#[test]
fn lirc_unsupported_flags_name_the_flag_and_button() {
    for flag in ["RC6", "BIPHASE", "CONST_LENGTH", "NO_HEAD_REP"] {
        let dir = tempfile::tempdir().expect("temp dir");
        write_file(
            dir.path(),
            "bad.conf",
            &format!(
                "begin remote
  name TV
  flags {flag}
  header 9000 4500
  one 560 1690
  zero 560 560
  bits 32
  begin codes
    Power 0x20DF10EF
  end codes
end remote
"
            ),
        );
        let error = import(&lirc("bad.conf", "TV", "Power"), dir.path(), None)
            .expect_err("flag must be rejected");
        assert!(
            matches!(&error, IrError::UnsupportedLircFlag { flag: found, .. } if found == flag),
            "{error}"
        );
        assert!(error.to_string().contains(flag), "{error}");
        assert!(error.to_string().contains("button=Power"), "{error}");
    }
}

#[test]
fn lirc_missing_button_names_the_file_and_selector() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.conf", LIRC_RAW);
    let error = import(&lirc("tv.conf", "TV", "Missing"), dir.path(), None)
        .expect_err("missing button must be rejected");

    let message = error.to_string();
    assert!(message.contains("tv.conf"), "{message}");
    assert!(message.contains("remote=TV button=Missing"), "{message}");
    assert!(
        message.contains("button \"Missing\" not found"),
        "{message}"
    );
}

#[test]
fn lirc_raw_code_with_a_zero_timing_is_rejected() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(
        dir.path(),
        "tv.conf",
        "begin remote
  name TV
  begin raw_codes
    name Power
    9000 4500 0 560
  end raw_codes
end remote
",
    );
    let error = import(&lirc("tv.conf", "TV", "Power"), dir.path(), None)
        .expect_err("zero timing must be rejected");
    assert!(error.to_string().contains("zero-length"), "{error}");
}

const FLIPPER_FILE: &str = "Filetype: IR signals file
Version: 1
#
name: Power
type: parsed
protocol: NEC
address: 04 00 00 00
command: 08 00 00 00
#
name: Custom
type: raw
frequency: 38000
duty_cycle: 0.330000
data: 9000 4500 560 560 560 1690
";

#[test]
fn flipper_raw_import_preserves_the_capture() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.ir", FLIPPER_FILE);
    let command = import_file(&dir, &flipper("tv.ir", "Custom"), None);

    assert_eq!(command.provenance.protocol, "raw");
    assert_eq!(command.provenance.sends, 1);
    assert_eq!(
        command.record.groups[0].once,
        vec![0xA328, 0x1194, 0x8230, 0x0230, 0x8230, 0x069A]
    );
}

#[test]
fn flipper_parsed_nec_imports_and_sends_three() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.ir", FLIPPER_FILE);
    let command = import_file(&dir, &flipper("tv.ir", "Power"), None);

    assert_eq!(command.provenance.protocol, "NEC");
    assert_eq!(command.provenance.sends, 3);
    assert_eq!(command.provenance.selector, "command=Power");
}

#[test]
fn flipper_parsed_rc5x_is_accepted_with_both_toggle_groups() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(
        dir.path(),
        "rc5.ir",
        "Filetype: IR signals file
Version: 1
#
name: Volume_up
type: parsed
protocol: RC5X
address: 10 00 00 00
command: 41 00 00 00
",
    );
    let command = import_file(&dir, &flipper("rc5.ir", "Volume_up"), None);

    assert_eq!(command.provenance.protocol, "RC5X");
    assert_eq!(command.provenance.sends, 1);
    assert_eq!(command.record.pointer_group_count(), 2);
}

#[test]
fn flipper_unsupported_protocols_name_the_protocol_and_command() {
    for protocol in ["NEC42", "NEC42ext", "RCA", "RC6"] {
        let dir = tempfile::tempdir().expect("temp dir");
        write_file(
            dir.path(),
            "bad.ir",
            &format!(
                "Filetype: IR signals file
Version: 1
#
name: Power
type: parsed
protocol: {protocol}
address: 04 00 00 00
command: 08 00 00 00
"
            ),
        );
        let error = import(&flipper("bad.ir", "Power"), dir.path(), None)
            .expect_err("protocol must be rejected");
        assert!(
            matches!(&error, IrError::UnsupportedFlipperProtocol { protocol: found, .. } if found == protocol),
            "{error}"
        );
        assert!(error.to_string().contains("command=Power"), "{error}");
        assert!(error.to_string().contains(protocol), "{error}");
    }
}

#[test]
fn flipper_parsed_protocols_carry_their_default_press_counts() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(
        dir.path(),
        "all.ir",
        "Filetype: IR signals file
Version: 1
#
name: Nec
type: parsed
protocol: NEC
address: 04 00 00 00
command: 08 00 00 00
#
name: Samsung
type: parsed
protocol: Samsung32
address: 07 00 00 00
command: 02 00 00 00
#
name: Jvc
type: parsed
protocol: JVC
address: 03 00 00 00
command: 0C 00 00 00
#
name: Sony
type: parsed
protocol: SIRC
address: 01 00 00 00
command: 15 00 00 00
#
name: Pana
type: parsed
protocol: Kaseikyo
address: AC 02 20 00
command: D1 03 00 00
#
name: Philips
type: parsed
protocol: RC5
address: 00 00 00 00
command: 0C 00 00 00
",
    );

    let expected = [
        ("Nec", "NEC", 3),
        ("Samsung", "Samsung32", 3),
        ("Jvc", "JVC", 3),
        ("Sony", "SIRC12", 3),
        ("Pana", "Kaseikyo", 1),
        ("Philips", "RC5", 1),
    ];
    for (command, protocol, sends) in expected {
        let imported = import_file(&dir, &flipper("all.ir", command), None);
        assert_eq!(imported.provenance.protocol, protocol);
        assert_eq!(imported.provenance.sends, sends, "{command}");
        assert_eq!(imported.provenance.format, IrSourceKind::Flipper);
    }
}

#[test]
fn sends_override_replaces_the_protocol_press_count() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.ir", FLIPPER_FILE);
    let command = import_file(&dir, &flipper("tv.ir", "Power"), Some(2));

    assert_eq!(command.provenance.sends, 2);
    assert_eq!(count_marks_near(&command.record.groups[0].once, 9000), 2);
}

#[test]
fn zero_sends_is_rejected() {
    let error = import(
        &pronto("5000 0000 0000 0001 0004 000C"),
        Path::new("."),
        Some(0),
    )
    .expect_err("zero sends must be rejected");
    assert!(error.to_string().contains("at least 1"), "{error}");
}

#[test]
fn duration_packing_sets_the_mark_bit_and_caps_words_at_32767_us() {
    assert_eq!(pack_block(&[Pulse::mark(32_767)]), vec![0xFFFF]);
    assert_eq!(pack_block(&[Pulse::space(32_768)]), vec![0x4000, 0x4000]);
    assert_eq!(
        pack_block(&[Pulse::mark(50_000)]),
        vec![0xFFFF, 0xC351],
        "32767 then 17233"
    );
    assert_eq!(
        pack_block(&[Pulse::space(40_222)]),
        vec![0x4E8F, 0x4E8F],
        "a short remainder balances across the last two words"
    );
    assert_eq!(
        pack_block(&[Pulse::mark(100), Pulse::mark(200)]),
        vec![0x812C],
        "adjacent same-kind durations are one interval"
    );
}

#[test]
fn raw_record_round_trips_through_container_encoding() {
    let command = import_inline(&pronto("0000 006D 0001 0001 0156 00AB 0016 0041"));
    let encoded = command.record.encode_at(0x021000).expect("record encodes");

    let decoded = IrRecord::decode(&encoded.bytes, encoded.base_addr, encoded.header_offset)
        .expect("record decodes");

    assert_eq!(decoded, command.record);
}

#[test]
fn record_pointer_lands_on_the_class_byte_with_blocks_before_the_header() {
    let command = import_inline(&pronto("5000 0000 0000 0001 0004 000C"));
    let encoded = command.record.encode_at(0x021000).expect("record encodes");

    assert!(
        encoded.header_offset > 0,
        "blocks sit in front of the header"
    );
    assert_eq!(
        encoded.bytes[encoded.header_offset + 7],
        CLASS_STREAM,
        "the class byte is seven bytes into the header"
    );
    assert_eq!(encoded.pointer, 0x021000 + 7);
    assert_eq!(
        encoded.pointer - encoded.base_addr,
        encoded.header_offset as u32 + 7
    );
    assert_eq!(
        u32::from_le_bytes([
            encoded.bytes[encoded.header_offset + 1],
            encoded.bytes[encoded.header_offset + 2],
            encoded.bytes[encoded.header_offset + 3],
            0,
        ]),
        command.record.carrier_period_ns
    );
    assert_eq!(encoded.bytes[encoded.header_offset + 11], 2);
}

#[test]
fn provenance_records_format_path_selector_digest_and_protocol() {
    let dir = tempfile::tempdir().expect("temp dir");
    write_file(dir.path(), "tv.conf", LIRC_CODES);
    let command = import_file(&dir, &lirc("tv.conf", "TV", "Power"), None);

    assert_eq!(command.provenance.format, IrSourceKind::Lirc);
    assert_eq!(command.provenance.selector, "remote=TV button=Power");
    assert_eq!(command.provenance.protocol, "NEC");
    assert_eq!(
        command.provenance.sha256,
        container::sha256_hex(LIRC_CODES.as_bytes())
    );
    assert_eq!(command.provenance.sha256.len(), 64);
    assert!(
        command
            .provenance
            .path
            .as_deref()
            .is_some_and(|path| path.ends_with("tv.conf")),
        "{:?}",
        command.provenance.path
    );
}

#[test]
fn ir_source_deserializes_from_the_model_shape() {
    let source: IrSource = serde_json::from_str(
        r#"{"format":"lirc","file":"irs/tv.conf","remote":"TV","button":"Input"}"#,
    )
    .expect("model source");

    assert_eq!(source.format, IrSourceKind::Lirc);
    assert_eq!(source.file.as_deref(), Some("irs/tv.conf"));
    assert_eq!(source.remote.as_deref(), Some("TV"));
    assert_eq!(source.button.as_deref(), Some("Input"));
    assert!(source.hex.is_none());
}
