//! Class-1 IR Record encoding and decoding, and duration-word packing.

use crate::ir_types::{
    BLOCK_TERMINATOR, CLASS_STREAM, EncodedIrRecord, IrError, IrPointerGroup, IrRecord, MARK_BIT,
    MAX_DURATION_US, MAX_POINTER_GROUPS, Pulse, RECORD_POINTER_BIAS,
};

impl IrRecord {
    pub fn pointer_group_count(&self) -> usize {
        self.groups.len()
    }

    /// Serialize the record so its blocks sit in front of its header and a
    /// pointer at `header_addr + 7` lands on the class byte.
    pub fn encode_at(&self, header_addr: u32) -> Result<EncodedIrRecord, IrError> {
        if self.class != CLASS_STREAM {
            return Err(IrError::Record {
                detail: format!("only class 1 records are encoded, not class {}", self.class),
            });
        }
        if self.groups.is_empty() || self.groups.len() > MAX_POINTER_GROUPS {
            return Err(IrError::Record {
                detail: format!(
                    "a record carries one or two pointer groups, not {}",
                    self.groups.len()
                ),
            });
        }
        if self.carrier_period_ns > 0x00FF_FFFF || self.carrier_on_ns > 0x00FF_FFFF {
            return Err(IrError::Record {
                detail: "carrier timings exceed the header's 24-bit fields".into(),
            });
        }

        let mut blocks: Vec<Vec<u8>> = Vec::new();
        let mut group_blocks: Vec<[Option<usize>; 3]> = Vec::new();
        for group in &self.groups {
            let mut slots = [None, None, None];
            for (slot, block) in [Some(&group.once), group.held.as_ref(), group.tail.as_ref()]
                .into_iter()
                .enumerate()
            {
                if let Some(words) = block {
                    if words.is_empty() {
                        return Err(IrError::Record {
                            detail: "a record block carries no durations".into(),
                        });
                    }
                    if words.contains(&BLOCK_TERMINATOR) {
                        return Err(IrError::Record {
                            detail: "a duration word is zero, which would close the block".into(),
                        });
                    }
                    slots[slot] = Some(blocks.len());
                    let mut bytes = Vec::with_capacity(words.len() * 2 + 2);
                    for word in words {
                        bytes.extend_from_slice(&word.to_le_bytes());
                    }
                    bytes.extend_from_slice(&BLOCK_TERMINATOR.to_le_bytes());
                    blocks.push(bytes);
                }
            }
            group_blocks.push(slots);
        }

        let block_length: usize = blocks.iter().map(Vec::len).sum();
        let block_length_u32 = u32::try_from(block_length).map_err(|_| IrError::Record {
            detail: "record blocks are too large".into(),
        })?;
        if header_addr < block_length_u32 {
            return Err(IrError::Record {
                detail: format!("record address 0x{header_addr:06X} leaves no room for its blocks"),
            });
        }
        let base_addr = header_addr - block_length_u32;
        let pointer = header_addr + RECORD_POINTER_BIAS as u32;
        if pointer > 0x00FF_FFFF {
            return Err(IrError::Record {
                detail: "record pointer exceeds the 24-bit address space".into(),
            });
        }

        let header_offset = block_length;
        let mut bytes = Vec::with_capacity(block_length + 12 + 9 * self.groups.len());
        for block in &blocks {
            bytes.extend_from_slice(block);
        }
        bytes.push(0);
        bytes.extend_from_slice(&u24(self.carrier_period_ns));
        bytes.extend_from_slice(&u24(self.carrier_on_ns));
        bytes.push(self.class);
        bytes.extend_from_slice(&u24(header_addr));
        bytes.push(self.groups.len() as u8);
        let mut next_block = 0usize;
        let mut addresses = Vec::new();
        for block in &blocks {
            addresses.push(
                base_addr
                    + u32::try_from(next_block).map_err(|_| IrError::Record {
                        detail: "record blocks are too large".into(),
                    })?,
            );
            next_block += block.len();
        }
        for slots in &group_blocks {
            for slot in slots {
                let address = slot.map_or(0, |index| addresses[index]);
                bytes.extend_from_slice(&u24(address));
            }
        }

        Ok(EncodedIrRecord {
            bytes,
            header_offset,
            base_addr,
            pointer,
        })
    }

    /// Read a class-1 record whose header is at `base_addr + header_offset`.
    pub fn decode(bytes: &[u8], base_addr: u32, header_offset: usize) -> Result<Self, IrError> {
        if header_offset > bytes.len() {
            return Err(IrError::Record {
                detail: "record header offset is outside the record area".into(),
            });
        }
        let header = &bytes[header_offset..];
        if header.len() < 12 {
            return Err(IrError::Record {
                detail: "record header is truncated".into(),
            });
        }
        let carrier_period_ns = read_u24(&header[1..4]);
        let carrier_on_ns = read_u24(&header[4..7]);
        let class = header[7];
        let own_start = read_u24(&header[8..11]);
        let expected_start =
            base_addr
                .checked_add(header_offset as u32)
                .ok_or_else(|| IrError::Record {
                    detail: "record address overflows".into(),
                })?;
        if own_start != expected_start {
            return Err(IrError::Record {
                detail: format!(
                    "record states its start as 0x{own_start:06X}, not 0x{expected_start:06X}"
                ),
            });
        }
        let count = header[11] as usize;
        if count == 0 || count > MAX_POINTER_GROUPS {
            return Err(IrError::Record {
                detail: format!("unsupported pointer-group count {count}"),
            });
        }
        if header.len() < 12 + 9 * count {
            return Err(IrError::Record {
                detail: "record pointer groups are truncated".into(),
            });
        }
        let mut groups = Vec::with_capacity(count);
        for group in 0..count {
            let start = 12 + 9 * group;
            let pointers = [
                read_u24(&header[start..start + 3]),
                read_u24(&header[start + 3..start + 6]),
                read_u24(&header[start + 6..start + 9]),
            ];
            let once =
                read_block(bytes, base_addr, pointers[0])?.ok_or_else(|| IrError::Record {
                    detail: "the once pointer is NULL".into(),
                })?;
            let held = read_block(bytes, base_addr, pointers[1])?;
            let tail = read_block(bytes, base_addr, pointers[2])?;
            groups.push(IrPointerGroup { once, held, tail });
        }
        Ok(Self {
            carrier_period_ns,
            carrier_on_ns,
            class,
            groups,
        })
    }
}

fn read_u24(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

fn u24(value: u32) -> [u8; 3] {
    [
        (value & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        ((value >> 16) & 0xFF) as u8,
    ]
}

fn read_block(bytes: &[u8], base_addr: u32, pointer: u32) -> Result<Option<Vec<u16>>, IrError> {
    if pointer == 0 {
        return Ok(None);
    }
    let offset = pointer
        .checked_sub(base_addr)
        .map(|value| value as usize)
        .ok_or_else(|| IrError::Record {
            detail: format!("block pointer 0x{pointer:06X} precedes the record area"),
        })?;
    let mut words = Vec::new();
    let mut at = offset;
    loop {
        if at + 2 > bytes.len() {
            return Err(IrError::Record {
                detail: "record block runs past the record area".into(),
            });
        }
        let word = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        at += 2;
        if word == BLOCK_TERMINATOR {
            break;
        }
        words.push(word);
        if words.len() > 8192 {
            return Err(IrError::Record {
                detail: "record block does not close".into(),
            });
        }
    }
    Ok(Some(words))
}

/// Pack pulses into duration words: bit 15 marks a mark, bits 14..0 are
/// microseconds, and a long interval is split so no word asks for more than
/// 32767 microseconds.
pub fn pack_block(pulses: &[Pulse]) -> Vec<u16> {
    let mut merged: Vec<Pulse> = Vec::new();
    for pulse in pulses.iter().copied().filter(|pulse| pulse.micros > 0) {
        if let Some(last) = merged.last_mut()
            && last.mark == pulse.mark
        {
            last.micros = last.micros.saturating_add(pulse.micros);
            continue;
        }
        merged.push(pulse);
    }

    let mut words = Vec::new();
    for pulse in merged {
        for chunk in split_duration(pulse.micros) {
            words.push(chunk | if pulse.mark { MARK_BIT } else { 0 });
        }
    }
    words
}

fn split_duration(micros: u32) -> Vec<u16> {
    let mut words = Vec::new();
    let mut remaining = micros;
    while remaining > MAX_DURATION_US {
        if remaining - MAX_DURATION_US < MAX_DURATION_US / 2 {
            let first = remaining / 2;
            let second = remaining - first;
            words.push(first as u16);
            words.push(second as u16);
            return words;
        }
        words.push(MAX_DURATION_US as u16);
        remaining -= MAX_DURATION_US;
    }
    if remaining > 0 {
        words.push(remaining as u16);
    }
    words
}
