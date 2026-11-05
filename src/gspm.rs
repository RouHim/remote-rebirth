//! Architecture-16 primitives: the constants the frame is built from, the action list encoding, the
//! tagged list encoding and the build timestamp record.
//!
//! Ported from the MIT-licensed harmony-explorations codec (`packages/codec/src/gspm.ts` and the
//! no-screen family), with the attribution the project's licensing decision requires. Only the
//! architecture-16 facts the empty no-screen configuration needs are carried here; the reader,
//! the other architectures and the catalogue data stay in the reference.
//!
//! Every constant below is a fact of the Harmony 300/350 container as measured by that reference:
//! `GSPM`, the `LWJL` marker at `0x47`, fifteen pointer slots, a trailer checksum six bytes before
//! the end and `PTYY` as the end marker at the flash base `0x020000`.

use time::{OffsetDateTime, Weekday};

use crate::error::Error;

/// The architecture the frame lays out.
pub const ARCHITECTURE: u8 = 16;
/// Pointer slots in the section table.
pub const POINTER_COUNT: usize = 15;
/// The section table starts at this byte, after the cookie, `end_addr` and the format word.
pub const SECTION_TABLE_OFFSET: usize = 0x0B;
/// `{ u8 spare; u24 address }` per slot.
pub const SECTION_ITEM_SIZE: usize = 4;
/// Where the `LWJL` marker after the table sits: `0x0B + 4 * 15`.
pub const TABLE_END_OFFSET: usize = SECTION_TABLE_OFFSET + SECTION_ITEM_SIZE * POINTER_COUNT;
/// `GSPM` and `PTYY` are four bytes; the end marker closes the container.
pub const END_MARKER_LENGTH: usize = 4;
/// The stored checksum word followed by the four byte end marker.
pub const TRAILER_LENGTH: usize = 6;

/// Base slot 1: the seven byte architecture record.
pub const ARCH_RECORD_LENGTH: usize = 7;
/// The record's fourth byte, `0x0D` on every container built from 2009 onward.
pub const ARCH_RECORD_CONSTANT: u8 = 0x0D;

/// Base slot 3: `u16 0xADDF`, seven fields, `u16 0xEFBF`, then three zero bytes.
pub const CLOCK_SECTION_LENGTH: usize = 14;
/// The seven timestamp fields sit two bytes in, after the cookie.
pub const CLOCK_FIELDS_OFFSET: usize = 2;
/// Seconds, minutes, hours, day, weekday, month, year offset from 2000.
pub const CLOCK_FIELD_COUNT: usize = 7;
pub const CLOCK_COOKIE: [u8; 2] = [0xDF, 0xAD];
pub const CLOCK_END: [u8; 2] = [0xBF, 0xEF];
const CLOCK_FIRST_YEAR: i32 = 2000;
const CLOCK_LAST_YEAR: i32 = 2255;

/// The format word states the pointer count in its second byte: `0x0F00` on architecture 16.
pub const FORMAT_WORD: u32 = (POINTER_COUNT as u32) << 8;

/// The maximum of each clock field, seconds through month. The year's maximum is that year plus
/// one, so it is not stated here.
pub const CLOCK_STATE_MAXIMA: [u16; 6] = [59, 59, 23, 30, 6, 11];

/// One action list instruction: a sixteen bit operand and an opcode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Instruction {
    pub operand: u16,
    pub opcode: u8,
}

impl Instruction {
    pub const fn new(opcode: u8, operand: u16) -> Self {
        Self { operand, opcode }
    }
}

/// One entry of a tagged list: the event tag followed by an action list instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaggedEntry {
    pub tag: u8,
    pub operand: u16,
    pub opcode: u8,
}

/// An action list: a one byte count, then each instruction's operand and opcode.
pub fn action_list_bytes(body: &[Instruction]) -> Result<Vec<u8>, Error> {
    let count = u8::try_from(body.len()).map_err(|_| {
        Error::Validation(format!(
            "an action list of {} instructions does not fit its one byte count",
            body.len()
        ))
    })?;
    let mut out = Vec::with_capacity(1 + 3 * body.len());
    out.push(count);
    for instruction in body {
        out.extend_from_slice(&instruction.operand.to_le_bytes());
        out.push(instruction.opcode);
    }
    Ok(out)
}

/// A tagged list: a one byte count and four bytes per entry. An empty list is the two byte wide
/// form's zero marker, because a narrow count of zero would be read as the wide form.
pub fn tagged_list_bytes(entries: &[TaggedEntry]) -> Result<Vec<u8>, Error> {
    if entries.is_empty() {
        return Ok(vec![0, 0]);
    }
    let count = u8::try_from(entries.len()).map_err(|_| {
        Error::Validation(format!(
            "a tagged list of {} entries does not fit its one byte count",
            entries.len()
        ))
    })?;
    let mut out = Vec::with_capacity(1 + 4 * entries.len());
    out.push(count);
    for entry in entries {
        out.push(entry.tag);
        out.extend_from_slice(&entry.operand.to_le_bytes());
        out.push(entry.opcode);
    }
    Ok(out)
}

/// Base slot 1: `16 16 <skin> 0D 00 00 00`.
pub fn architecture_record(skin: u8) -> [u8; ARCH_RECORD_LENGTH] {
    [
        ARCHITECTURE,
        ARCHITECTURE,
        skin,
        ARCH_RECORD_CONSTANT,
        0,
        0,
        0,
    ]
}

/// The seven stored bytes of a build timestamp. The day of the month is stored counted from zero
/// and the weekday from Sunday, which is how the remote's clock counts both.
pub fn clock_fields(built_at: &OffsetDateTime) -> Result<[u8; CLOCK_FIELD_COUNT], Error> {
    let year = built_at.year();
    if !(CLOCK_FIRST_YEAR..=CLOCK_LAST_YEAR).contains(&year) {
        return Err(Error::Validation(format!(
            "a build timestamp of {year} cannot be stored: the clock's year is 2000 to 2255"
        )));
    }
    let month = built_at.month() as u8;
    let day = built_at.day();
    let stored_day = day - 1;
    if stored_day > last_stored_day(month - 1, year - CLOCK_FIRST_YEAR) {
        return Err(Error::Validation(format!(
            "a build timestamp of {year}-{month:02}-{day:02} is outside the date range the clock can state"
        )));
    }
    Ok([
        built_at.second(),
        built_at.minute(),
        built_at.hour(),
        stored_day,
        weekday_from_sunday(built_at.weekday()),
        month - 1,
        (year - CLOCK_FIRST_YEAR) as u8,
    ])
}

/// Base slot 3: cookie, seven timestamp fields, terminator, three zero bytes.
pub fn timestamp_section(built_at: &OffsetDateTime) -> Result<[u8; CLOCK_SECTION_LENGTH], Error> {
    let fields = clock_fields(built_at)?;
    let mut out = [0u8; CLOCK_SECTION_LENGTH];
    out[..CLOCK_COOKIE.len()].copy_from_slice(&CLOCK_COOKIE);
    out[CLOCK_FIELDS_OFFSET..CLOCK_FIELDS_OFFSET + CLOCK_FIELD_COUNT].copy_from_slice(&fields);
    out[CLOCK_FIELDS_OFFSET + CLOCK_FIELD_COUNT
        ..CLOCK_FIELDS_OFFSET + CLOCK_FIELD_COUNT + CLOCK_END.len()]
        .copy_from_slice(&CLOCK_END);
    Ok(out)
}

/// The last stored day of `month` (0 = January) as the firmware's own month-end routine states it:
/// 30 for a 31 day month, 29 for a 30 day one, 28 in a leap February and 27 otherwise. The remote's
/// `& 3` leap rule is kept even though the calendar check already rejects 29 February 2100 and 2200,
/// so the encoder's range is the decoder's by construction.
fn last_stored_day(month: u8, year_offset: i32) -> u8 {
    match month {
        1 => {
            if year_offset & 3 == 0 {
                28
            } else {
                27
            }
        }
        3 | 5 | 8 | 10 => 29,
        _ => 30,
    }
}

fn weekday_from_sunday(weekday: Weekday) -> u8 {
    weekday.number_days_from_sunday()
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time};

    use super::*;

    fn moment(year: i32) -> OffsetDateTime {
        let date = Date::from_calendar_date(year, Month::November, 5).expect("a valid date");
        let time = Time::from_hms(12, 34, 56).expect("a valid time");
        PrimitiveDateTime::new(date, time).assume_utc()
    }

    #[test]
    fn the_stored_clock_fields_count_the_day_and_weekday_from_zero() {
        let fields = clock_fields(&moment(2026)).expect("2026 is storable");
        assert_eq!(fields, [56, 34, 12, 4, 4, 10, 26]);
    }

    #[test]
    fn a_stamp_outside_the_clock_range_is_refused() {
        assert!(clock_fields(&moment(1999)).is_err());
        assert!(clock_fields(&moment(2256)).is_err());
    }
}
