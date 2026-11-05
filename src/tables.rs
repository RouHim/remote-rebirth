//! The count-and-address tables of architecture 16 and the compiler's key order.
//!
//! Ported from the MIT-licensed harmony-explorations codec (`packages/codec/src/frame.ts`'s
//! `countedTable`, `packages/codec/src/tables.ts`'s `numberSenderTable`, and
//! `families/no-screen/devices.ts`'s `inCompilerOrder`). The catalogue readers and the six other
//! architectures are not ported; the empty configuration needs only the writers.

use crate::error::Error;
use crate::frame::{Piece, Reference};
use crate::gspm::TaggedEntry;

/// A table of a count and three byte addresses: `u8`, `u16` or `u24` count, then one address per
/// target. The frame fills each address once the target is placed.
pub fn counted_table(
    count_width: usize,
    targets: &[usize],
    owner: &'static str,
) -> Result<Piece, Error> {
    if !(1..=3).contains(&count_width) {
        return Err(Error::Internal(format!(
            "a counted table's count is one to three bytes, not {count_width}"
        )));
    }
    let capacity = 1usize << (8 * count_width);
    if targets.len() >= capacity {
        return Err(Error::Validation(format!(
            "{} entries do not fit a {count_width} byte count",
            targets.len()
        )));
    }
    let count = u32::try_from(targets.len())
        .map_err(|_| Error::Internal("the table count does not fit u32".into()))?;
    let mut bytes = vec![0u8; count_width + 3 * targets.len()];
    bytes[..count_width].copy_from_slice(&count.to_le_bytes()[..count_width]);
    let refs = targets
        .iter()
        .enumerate()
        .map(|(k, target)| Reference::Piece {
            at: count_width + 3 * k,
            target: *target,
            offset: 0,
        })
        .collect();
    Ok(Piece { bytes, refs, owner })
}

/// Base slot 16: a one byte count and an address per number sender. No favourites is a count of
/// zero, not a NULL slot.
pub fn number_sender_table(records: &[usize]) -> Result<Piece, Error> {
    counted_table(1, records, "slot-16-table")
}

/// One base slot 15 group: a one byte count, then that many values as `u16`.
pub fn parameter_group(values: &[u16], owner: &'static str) -> Result<Piece, Error> {
    let count = u8::try_from(values.len()).map_err(|_| {
        Error::Validation(format!(
            "a parameter group of {} values does not fit its one byte count",
            values.len()
        ))
    })?;
    let mut bytes = Vec::with_capacity(1 + 2 * values.len());
    bytes.push(count);
    for value in values {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(Piece::new(bytes, owner))
}

/// Java 6 and 7's `HashMap` supplemental hash, all four terms. Keys below 4096 only reach the last
/// two terms, which is the form the reference pins the ordering with.
pub fn compiler_hash(key: u8) -> u32 {
    let key = u32::from(key);
    let h = key ^ (key >> 20) ^ (key >> 12);
    h ^ (h >> 7) ^ (h >> 4)
}

/// A tagged list with `added` put into `existing`, in the order Logitech's compiler would store it:
/// a Java 6 `HashMap` whose state is `existing` as stored. The table starts at sixteen slots, doubles
/// when an insertion finds it three quarters full, puts a new entry at the head of its slot, and
/// moves each old slot's entries over one at a time to the head of their new slot.
///
/// The order changes nothing the remote does (a tag appears once and lookup stops at the first
/// match), but keeping it makes the key table comparable entry for entry with Logitech's.
pub fn in_compiler_order(
    existing: &[TaggedEntry],
    added: &[TaggedEntry],
) -> Result<Vec<TaggedEntry>, Error> {
    let mut tags: Vec<u8> = existing
        .iter()
        .chain(added)
        .map(|entry| entry.tag)
        .collect();
    tags.sort_unstable();
    if tags.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(Error::Validation(
            "a tagged list cannot hold a tag twice".into(),
        ));
    }

    let slot = |tag: u8, size: usize| compiler_hash(tag) as usize & (size - 1);
    let stored = |size: usize| {
        let mut into = vec![Vec::new(); size];
        for entry in existing {
            into[slot(entry.tag, size)].push(*entry);
        }
        into
    };

    // The table's size: the smallest the stored entries fit at three quarters full, or a larger one
    // where the stored order is that size's slot order and not the smaller's. A Java `HashMap` never
    // shrinks, so a list that held more tags and lost some keeps the size it grew to.
    let mut size = 16;
    while existing.len() * 4 > size * 3 {
        size *= 2;
    }
    let largest = 32 * existing.len().max(1);
    let mut slots = stored(size);
    loop {
        let flat: Vec<TaggedEntry> = slots.iter().flatten().copied().collect();
        if flat == existing {
            break;
        }
        size *= 2;
        if size > largest {
            return Err(Error::Validation(
                "a tagged list here is not stored in the compiler's slot order".into(),
            ));
        }
        slots = stored(size);
    }

    for (count, entry) in (existing.len()..).zip(added) {
        slots[slot(entry.tag, size)].insert(0, *entry);
        if count * 4 >= size * 3 {
            let mut doubled = vec![Vec::new(); 2 * size];
            for entries in &slots {
                for entry in entries {
                    doubled[slot(entry.tag, 2 * size)].insert(0, *entry);
                }
            }
            slots = doubled;
            size *= 2;
        }
    }
    Ok(slots.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
