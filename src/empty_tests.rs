//! Behavioural tests for the empty no-screen configuration. They drive the public `emit_empty`
//! seam and assert the container's bytes, not any private helper.

use time::{Date, Month, OffsetDateTime, PrimitiveDateTime, Time};

use crate::container::{
    FLASH_BASE, ValidationStatus, stored_checksum, trailer_checksum, validation_status,
};
use crate::emit_empty;

const SKIN_300: u8 = 79;
const SKIN_350: u8 = 104;

fn moment() -> OffsetDateTime {
    let date = Date::from_calendar_date(2026, Month::November, 5).expect("a valid date");
    let time = Time::from_hms(12, 34, 56).expect("a valid time");
    PrimitiveDateTime::new(date, time).assume_utc()
}

fn u16_at(bytes: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([bytes[at], bytes[at + 1]])
}

fn u24_at(bytes: &[u8], at: usize) -> u32 {
    u32::from(bytes[at]) | u32::from(bytes[at + 1]) << 8 | u32::from(bytes[at + 2]) << 16
}

/// The blob offset a raw slot's pointer names, or `None` for a NULL item.
fn section(bytes: &[u8], raw: usize) -> Option<usize> {
    let address = u24_at(bytes, 0x0B + 4 * raw + 1);
    (address != 0).then(|| (address - FLASH_BASE) as usize)
}

fn key_entries(bytes: &[u8]) -> Vec<(u8, u16, u8)> {
    let count = bytes[0x4B] as usize;
    assert_eq!(count, 5, "the empty key table holds five entries");
    (0..count)
        .map(|k| {
            let at = 0x4C + 4 * k;
            (bytes[at], u16_at(bytes, at + 1), bytes[at + 3])
        })
        .collect()
}

#[test]
fn an_empty_harmony_300_container_is_the_documented_size_and_passes_the_trailer_check() {
    let bytes = emit_empty(SKIN_300, &moment()).expect("skin 79 is supported");

    // 1081 bytes in the reference, less the 411 byte metadata archive this project omits.
    assert_eq!(bytes.len(), 670);
    assert_eq!(&bytes[..4], b"GSPM");
    assert_eq!(&bytes[0x47..0x4B], b"LWJL");
    assert_eq!(&bytes[bytes.len() - 4..], b"PTYY");
    assert_eq!(
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        FLASH_BASE + bytes.len() as u32 - 4,
        "end_addr names the end marker"
    );
    assert_eq!(
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        15 << 8,
        "the format word states fifteen pointer slots"
    );
    assert_eq!(
        stored_checksum(&bytes),
        Some(trailer_checksum(&bytes).expect("the container is longer than its trailer"))
    );
    assert_eq!(validation_status(&bytes), ValidationStatus::Ok);

    let architecture = section(&bytes, 1).expect("base slot 1 states the architecture record");
    assert_eq!(
        &bytes[architecture..architecture + 7],
        &[16, 16, SKIN_300, 0x0D, 0, 0, 0]
    );
}

#[test]
fn an_empty_harmony_350_allocates_eight_infrared_groups_and_is_eight_bytes_longer() {
    let bytes = emit_empty(SKIN_350, &moment()).expect("skin 104 is supported");

    // 1105 bytes in the reference, less the same 411 byte archive. The four groups above skin 79's
    // allocation cost six bytes each, a three byte empty group and its three byte pointer.
    assert_eq!(bytes.len(), 694);
    assert_eq!(validation_status(&bytes), ValidationStatus::Ok);

    let architecture = section(&bytes, 1).expect("base slot 1 states the architecture record");
    assert_eq!(bytes[architecture + 2], SKIN_350);
}

#[test]
fn the_empty_configuration_has_fifteen_slots_and_four_or_eight_empty_infrared_groups() {
    for (skin, groups, length) in [(SKIN_300, 4usize, 670usize), (SKIN_350, 8, 694)] {
        let bytes = emit_empty(skin, &moment()).expect("a supported skin");
        assert_eq!(bytes.len(), length);
        assert_eq!(0x0B + 4 * 15, 0x47, "the table ends where LWJL starts");
        assert_eq!(section(&bytes, 14), None, "raw slot 14 is NULL");

        let infrared = section(&bytes, 4).expect("base slot 5 states the infrared table");
        assert_eq!(bytes[infrared] as usize, groups, "the model's allocation");
        let mut previous = 0usize;
        for group in 0..groups {
            let at = infrared + 1 + 3 * group;
            let address = u24_at(&bytes, at);
            let offset = (address - FLASH_BASE) as usize;
            assert!(offset > previous, "the group pointers ascend");
            assert_eq!(
                &bytes[offset..offset + 3],
                &[0, 0, 0],
                "an unused group is empty"
            );
            previous = offset;
        }
    }
}

#[test]
fn the_empty_key_table_binds_event_ff_to_the_list_event_30_calls() {
    for skin in [SKIN_300, SKIN_350] {
        let bytes = emit_empty(skin, &moment()).expect("a supported skin");
        let entries = key_entries(&bytes);
        assert_eq!(
            entries.iter().map(|one| one.0).collect::<Vec<_>>(),
            vec![0xFF, 0x30, 0x2E, 0x2F, 0x2D],
            "the four send events plus 0xFF in the compiler's hash order"
        );
        let unmasked = entries
            .iter()
            .find(|one| one.0 == 0xFF)
            .expect("event 0xFF is bound");
        let thirty = entries
            .iter()
            .find(|one| one.0 == 0x30)
            .expect("event 0x30 is bound");
        assert_eq!(unmasked.1, thirty.1, "0xFF calls what 0x30 calls");
        assert_eq!(unmasked.2, 0x7F, "the entry is a call of an action list");

        // Raw slot 6's table names the key table as its first entry, at the marker's end.
        let key_map = section(&bytes, 6).expect("raw slot 6 states the key map table");
        assert_eq!(bytes[key_map], 1);
        assert_eq!(u24_at(&bytes, key_map + 1) - FLASH_BASE, 0x4B);
    }
}

#[test]
fn the_empty_configuration_carries_every_table_the_reference_attributes() {
    let bytes = emit_empty(SKIN_300, &moment()).expect("skin 79 is supported");

    let start_up = section(&bytes, 5).expect("raw slot 5 states the start-up table");
    assert_eq!(bytes[start_up], 11, "eleven start-up events");
    let lists = section(&bytes, 7).expect("base slot 10 states the action list table");
    assert_eq!(
        u16_at(&bytes, lists),
        19,
        "six start-up and thirteen send-event lists"
    );
    let timers = section(&bytes, 8).expect("raw slot 8 states the timer table");
    assert_eq!(bytes[timers], 0);
    let states = section(&bytes, 9).expect("base slot 13 states the state table");
    assert_eq!(
        u16_at(&bytes, states),
        19,
        "seven clock, eleven firmware and one wide record"
    );
    let parameters = section(&bytes, 10).expect("base slot 15 states the parameter block");
    assert_eq!(bytes[parameters], 5);
    let senders = section(&bytes, 11).expect("base slot 16 states the number sender");
    assert_eq!(bytes[senders], 0);
    let digits = section(&bytes, 12).expect("raw slot 12 states the digit entry table");
    assert_eq!(&bytes[digits..digits + 2], &[0, 0]);
    // The metadata archive is the one section this project leaves out for v1.
    assert_eq!(section(&bytes, 13), None);
}

#[test]
fn two_builds_at_one_moment_are_one_file() {
    let first = emit_empty(SKIN_350, &moment()).expect("skin 104 is supported");
    let second = emit_empty(SKIN_350, &moment()).expect("skin 104 is supported");
    assert_eq!(first, second);
}

#[test]
fn skin_78_is_accepted_with_the_harmony_300_allocation() {
    let bytes = emit_empty(78, &moment()).expect("the spec enables skin 78");
    assert_eq!(bytes.len(), 670);
    let infrared = section(&bytes, 4).expect("base slot 5 states the infrared table");
    assert_eq!(bytes[infrared], 4);
    assert_eq!(validation_status(&bytes), ValidationStatus::Ok);
}

#[test]
fn a_skin_without_a_measured_allocation_is_refused_by_name() {
    let error = emit_empty(80, &moment()).expect_err("skin 80 is not known");
    let message = error.to_string();
    assert!(
        message.contains("78, 79 and 104"),
        "the message names the skins: {message}"
    );
}

#[test]
fn a_build_stamp_the_clock_cannot_hold_is_refused() {
    let date = Date::from_calendar_date(2256, Month::January, 1).expect("a valid date");
    let time = Time::from_hms(0, 0, 0).expect("a valid time");
    let stamp = PrimitiveDateTime::new(date, time).assume_utc();
    assert!(emit_empty(SKIN_350, &stamp).is_err());
}

/// The MIT codec's own empty configuration, generated for this project's fixed moment. These are
/// generated bytes, not Logitech data: no device, command or catalogue entry is in an empty setup.
const REFERENCE_79: &[u8] = include_bytes!("../tests/fixtures/reference-empty-79.bin");
const REFERENCE_104: &[u8] = include_bytes!("../tests/fixtures/reference-empty-104.bin");

#[test]
fn the_empty_configuration_matches_the_reference_codec_byte_for_byte() {
    for (skin, reference, length) in [
        (SKIN_300, REFERENCE_79, 1081usize),
        (SKIN_350, REFERENCE_104, 1105),
    ] {
        assert_eq!(reference.len(), length, "the reference's own length");
        let archive = section(reference, 13).expect("the reference states the archive");
        assert_eq!(
            archive,
            length - 411 - 6,
            "the archive sits between the content and the trailer"
        );

        // This project's deviation: the archive is absent, so its pointer is NULL and the trailer
        // moves back to where the archive started. Every other content byte is the reference's.
        let mut expected = reference[..archive].to_vec();
        expected[0x3F..0x43].fill(0);
        let our_length = expected.len() + 6;
        expected[4..8].copy_from_slice(&(FLASH_BASE + our_length as u32 - 4).to_le_bytes());
        expected.resize(our_length, 0);
        let checksum =
            trailer_checksum(&expected).expect("the container is longer than its trailer");
        expected[our_length - 6..our_length - 4].copy_from_slice(&checksum.to_le_bytes());
        expected[our_length - 4..].copy_from_slice(b"PTYY");

        assert_eq!(
            emit_empty(skin, &moment()).expect("a supported skin"),
            expected
        );
    }
}
