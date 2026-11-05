//! Tests for the count-and-address tables.

use crate::architecture::TaggedEntry;
use crate::tables::*;

fn entry(tag: u8) -> TaggedEntry {
    TaggedEntry {
        tag,
        operand: 0,
        opcode: 0x7F,
    }
}

#[test]
fn a_key_table_of_the_five_empty_entries_is_in_slot_order() {
    // Hand-computed from Java's hash at sixteen slots: 0xFF is bucket 1, 0x30 bucket 3, 0x2E
    // bucket 12, 0x2F bucket 13 and 0x2D bucket 15.
    let added = [0x2D, 0x2E, 0x2F, 0x30, 0xFF].map(entry);
    let ordered = in_compiler_order(&[], &added).expect("five distinct tags");
    assert_eq!(
        ordered.iter().map(|one| one.tag).collect::<Vec<_>>(),
        vec![0xFF, 0x30, 0x2E, 0x2F, 0x2D]
    );
}

#[test]
fn a_tag_given_twice_is_refused() {
    assert!(in_compiler_order(&[entry(0x2D)], &[entry(0x2D)]).is_err());
}
