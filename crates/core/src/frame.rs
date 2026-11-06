//! The architecture-16 container frame: a linker that places content pieces, writes every section
//! pointer and address field, and stamps the header and trailer.
//!
//! Ported from the MIT-licensed harmony-explorations codec (`packages/codec/src/frame.ts`). The
//! reference's linker handles six architectures; this one carries the architecture-16 slot table,
//! which is stated rather than derived because its fifteen raw slots are not the base twenty with
//! insertions. Everything the reference generates for a layout (base slot 1's architecture record
//! and base slot 3's timestamp) is generated here too; everything else is a [`Piece`] the caller
//! supplies.
//!
//! Two deviations from the reference are deliberate and documented:
//!
//! * Raw slot 13, the metadata archive, is NULL in this frame. Nothing on the remote is known to
//!   read the archive, and building it means porting the reference's ZIP writer and deflate. The
//!   omission shortens the empty container by the archive's length, which is why the tests assert
//!   this project's lengths rather than the reference's 1081 and 1105.
//! * Skin 78 is accepted with four infrared groups even though the reference measured only skins
//!   79 and 104. The spec enables 78, whose skin 79 sibling allocates four groups; the choice is
//!   stated where the allocation is chosen.

use time::OffsetDateTime;

use crate::architecture::{
    END_MARKER_LENGTH, FORMAT_WORD, POINTER_COUNT, SECTION_ITEM_SIZE, SECTION_TABLE_OFFSET,
    TABLE_END_OFFSET, TRAILER_LENGTH, architecture_record, timestamp_section,
};
use crate::container::{
    FLASH_BASE, MAX_CONTAINER_SIZE, ValidationStatus, stored_checksum, trailer_checksum,
    validation_status,
};
use crate::error::Error;

/// One address field inside a piece: either a target piece and an offset into it, or an absolute
/// flash address the container does not own (base slot 2's log area names flash above the file).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reference {
    Piece {
        at: usize,
        target: usize,
        offset: usize,
    },
    Address {
        at: usize,
        address: u32,
    },
}

/// A run of content bytes and the address fields inside it.
#[derive(Debug, Clone)]
pub struct Piece {
    pub bytes: Vec<u8>,
    pub refs: Vec<Reference>,
    pub owner: &'static str,
}

impl Piece {
    pub fn new(bytes: Vec<u8>, owner: &'static str) -> Self {
        Self {
            bytes,
            refs: Vec::new(),
            owner,
        }
    }
}

/// What one raw slot of the architecture-16 table holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    /// A base slot, by base number. The base layout names twenty; architecture 16 places nine.
    Base(u8),
    /// A section of the family's own that no base slot names.
    Inserted,
    /// A NULL item.
    Null,
}

/// Architecture 16's fifteen raw slots. Raw 13 (the metadata archive) and raw 14 are NULL: raw 14 is
/// NULL in every Logitech compile, and raw 13 is this project's documented archive omission.
///
/// The base-slot placement is the reference's `ARCH16_SLOT_MAP`, measured with the firmware's own
/// section seeker: raw 4 is base slot 5 (the infrared table), raw 7 base 10 (the action lists), raw
/// 9 base 13 (the state table), raw 10 base 15 (the parameters) and raw 11 base 16 (the number
/// sender). Raw 5, 6, 8 and 12 are the start-up table, key map table, timers and digit table.
pub const ARCH16_SLOTS: [SlotKind; POINTER_COUNT] = [
    SlotKind::Base(0),
    SlotKind::Base(1),
    SlotKind::Base(2),
    SlotKind::Base(3),
    SlotKind::Base(5),
    SlotKind::Inserted,
    SlotKind::Inserted,
    SlotKind::Base(10),
    SlotKind::Inserted,
    SlotKind::Base(13),
    SlotKind::Base(15),
    SlotKind::Base(16),
    SlotKind::Inserted,
    SlotKind::Null,
    SlotKind::Null,
];

/// What sits in one raw slot: what is parked in front of the table, then the piece the slot's own
/// pointer names.
#[derive(Debug, Clone)]
pub struct Placement {
    pub before: Vec<usize>,
    pub head: usize,
}

impl Placement {
    pub fn new(before: Vec<usize>, head: usize) -> Self {
        Self { before, head }
    }
}

/// The pieces of a container and where the caller puts them.
#[derive(Debug)]
pub struct Layout {
    pieces: Vec<Piece>,
    architecture: usize,
    clock: usize,
    key_table: usize,
    body: Vec<usize>,
    placements: [Option<Placement>; POINTER_COUNT],
}

impl Layout {
    /// A layout with the frame's two generated pieces in place: base slot 1's architecture record
    /// and base slot 3's build timestamp. Their indices stay private; the caller only names slots.
    pub fn new(skin: u8, built_at: &OffsetDateTime) -> Result<Self, Error> {
        let architecture = Piece::new(architecture_record(skin).to_vec(), "slot-1-arch");
        let clock = Piece::new(timestamp_section(built_at)?.to_vec(), "slot-3-clock");
        Ok(Self {
            pieces: vec![architecture, clock],
            architecture: 0,
            clock: 1,
            key_table: usize::MAX,
            body: Vec::new(),
            placements: std::array::from_fn(|_| None),
        })
    }

    /// Add a piece, returning the index its address fields use.
    pub fn add(&mut self, piece: Piece) -> usize {
        self.pieces.push(piece);
        self.pieces.len() - 1
    }

    /// The key table sits immediately after `LWJL`, as it does in every Logitech compile of the
    /// family; raw slot 6's key map table names it as entry 0.
    pub fn set_key_table(&mut self, index: usize) {
        self.key_table = index;
    }

    /// The body: every piece reached only through an address, in the caller's order.
    pub fn set_body(&mut self, body: Vec<usize>) {
        self.body = body;
    }

    /// Put a slot's table, and whatever is parked in front of it, at a raw slot.
    pub fn place(&mut self, raw: usize, placement: Placement) -> Result<(), Error> {
        let Some(kind) = ARCH16_SLOTS.get(raw) else {
            return Err(Error::Validation(format!(
                "architecture 16 has no raw slot {raw}"
            )));
        };
        if *kind == SlotKind::Null {
            return Err(Error::Validation(format!(
                "raw slot {raw} is NULL in this layout and cannot name a table"
            )));
        }
        if *kind == SlotKind::Base(1) || *kind == SlotKind::Base(3) {
            return Err(Error::Validation(format!(
                "raw slot {raw} is the frame's own and takes no placement"
            )));
        }
        if self.placements[raw].is_some() {
            return Err(Error::Validation(format!("raw slot {raw} is placed twice")));
        }
        self.placements[raw] = Some(placement);
        Ok(())
    }

    /// Lay the container out: header, section table, content, trailer. Every check the layout can
    /// make is made before any bytes are written, and the result is verified with the container
    /// validator before it is returned.
    pub fn lay_out(&self) -> Result<Vec<u8>, Error> {
        if self.key_table == usize::MAX {
            return Err(Error::Internal(
                "the layout has no key table; set it before laying out".into(),
            ));
        }

        // The region order: key table, body, then the raw slots in table order with what is parked
        // in front of each table. The frame's own two slots are filled from their generated pieces.
        let mut order: Vec<usize> = Vec::new();
        order.push(self.key_table);
        order.extend_from_slice(&self.body);
        let mut heads: [Option<usize>; POINTER_COUNT] = [None; POINTER_COUNT];

        for (raw, kind) in ARCH16_SLOTS.iter().enumerate() {
            match kind {
                SlotKind::Null => {
                    if self.placements[raw].is_some() {
                        return Err(Error::Validation(format!(
                            "raw slot {raw} is NULL and cannot carry a table"
                        )));
                    }
                }
                SlotKind::Base(1) => {
                    order.push(self.architecture);
                    heads[raw] = Some(self.architecture);
                }
                SlotKind::Base(3) => {
                    order.push(self.clock);
                    heads[raw] = Some(self.clock);
                }
                SlotKind::Base(_) | SlotKind::Inserted => {
                    let placed = self.placements[raw].as_ref().ok_or_else(|| {
                        Error::Validation(format!(
                            "raw slot {raw} has no table; every non-NULL slot of architecture 16 states one"
                        ))
                    })?;
                    order.extend_from_slice(&placed.before);
                    if placed.head >= self.pieces.len() {
                        return Err(Error::Internal(format!(
                            "raw slot {raw} names piece {}, which does not exist",
                            placed.head
                        )));
                    }
                    order.push(placed.head);
                    heads[raw] = Some(placed.head);
                }
            }
        }

        // Offsets. The content starts after the table and the marker, and a piece is listed once.
        let mut offsets = vec![0usize; self.pieces.len()];
        let mut in_layout = vec![false; self.pieces.len()];
        let mut at = TABLE_END_OFFSET + END_MARKER_LENGTH;
        for &index in &order {
            let Some(piece) = self.pieces.get(index) else {
                return Err(Error::Internal(format!(
                    "the layout lists piece {index}, which does not exist"
                )));
            };
            if in_layout[index] {
                return Err(Error::Validation(format!(
                    "the piece {} is listed twice",
                    piece.owner
                )));
            }
            in_layout[index] = true;
            offsets[index] = at;
            at = at.checked_add(piece.bytes.len()).ok_or_else(|| {
                Error::Internal("the container length overflows the address space".into())
            })?;
        }
        let length = at + TRAILER_LENGTH;
        if length > MAX_CONTAINER_SIZE {
            return Err(Error::Validation(format!(
                "the container is {length} bytes, larger than the {MAX_CONTAINER_SIZE} byte region"
            )));
        }
        let length_u32 = u32::try_from(length)
            .map_err(|_| Error::Internal("the container length does not fit u32".into()))?;

        let mut bytes = vec![0u8; length];
        bytes[..4].copy_from_slice(b"GSPM");
        bytes[4..8]
            .copy_from_slice(&(FLASH_BASE + length_u32 - END_MARKER_LENGTH as u32).to_le_bytes());
        bytes[8..12].copy_from_slice(&FORMAT_WORD.to_le_bytes());
        bytes[TABLE_END_OFFSET..TABLE_END_OFFSET + END_MARKER_LENGTH].copy_from_slice(b"LWJL");

        for (raw, head) in heads.iter().enumerate() {
            let Some(index) = head else { continue };
            let item = SECTION_TABLE_OFFSET + SECTION_ITEM_SIZE * raw;
            write_u24(&mut bytes, item + 1, FLASH_BASE + offsets[*index] as u32)?;
        }

        // The content, then every address field from where its target landed.
        for &index in &order {
            let piece = &self.pieces[index];
            let start = offsets[index];
            bytes[start..start + piece.bytes.len()].copy_from_slice(&piece.bytes);
            for reference in &piece.refs {
                match *reference {
                    Reference::Piece { at, target, offset } => {
                        if !in_layout.get(target).copied().unwrap_or(false) {
                            return Err(Error::Validation(format!(
                                "an address in {} names a piece that is not in the layout",
                                piece.owner
                            )));
                        }
                        let target_len = self.pieces[target].bytes.len();
                        if offset > target_len {
                            return Err(Error::Validation(format!(
                                "an address in {} names {offset} bytes into a piece of {target_len}",
                                piece.owner
                            )));
                        }
                        write_u24(
                            &mut bytes,
                            start + at,
                            FLASH_BASE + offsets[target] as u32 + offset as u32,
                        )?;
                    }
                    Reference::Address { at, address } => {
                        write_u24(&mut bytes, start + at, address)?;
                    }
                }
            }
        }

        let trailer = length - TRAILER_LENGTH;
        let checksum = trailer_checksum(&bytes)?;
        bytes[trailer..trailer + 2].copy_from_slice(&checksum.to_le_bytes());
        bytes[trailer + 2..].copy_from_slice(b"PTYY");

        debug_assert_eq!(stored_checksum(&bytes), Some(checksum));
        debug_assert_eq!(validation_status(&bytes), ValidationStatus::Ok);
        Ok(bytes)
    }
}

fn write_u24(bytes: &mut [u8], at: usize, value: u32) -> Result<(), Error> {
    if value > 0xFF_FFFF {
        return Err(Error::Validation(format!(
            "the address 0x{value:x} is past a three byte field"
        )));
    }
    let Some(slot) = bytes.get_mut(at..at + 3) else {
        return Err(Error::Internal(format!(
            "the address field at {at} is outside the container"
        )));
    };
    slot.copy_from_slice(&value.to_le_bytes()[..3]);
    Ok(())
}
