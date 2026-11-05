//! The architecture-16 sections the empty configuration holds: the start-up table and its lists, the
//! four send events' lists and key entries, the state tables, the log area, the parameter groups, the
//! digit entry table and the name tree.
//!
//! Ported from the MIT-licensed harmony-explorations codec
//! (`families/no-screen/constants.ts`, `families/no-screen/statetables.ts` and the shared
//! `statetables.ts`/`firmwaresections.ts`). The values are the reference's measurements of all
//! thirteen Harmony 300/350 compiles it holds; no Logitech catalogue data is shipped.

use time::OffsetDateTime;

use crate::error::Error;
use crate::frame::{Piece, Reference};
use crate::gspm::{
    CLOCK_STATE_MAXIMA, END_MARKER_LENGTH, Instruction, TaggedEntry, action_list_bytes,
    clock_fields,
};

/// The start-up event table's entries that call a list of their own: entries 1 to 4. Entry 0 and
/// entries 5 to 10 call the empty list.
const START_UP_OWN_EVENTS: [(usize, u16); 4] = [(1, 0), (2, 1), (3, 2), (4, 3)];
/// The start-up event table holds eleven instructions in every compile of the family.
const START_UP_EVENT_COUNT: usize = 11;

/// The list that sets the hold time and pushes the key table.
pub const START_UP_HOLD: u16 = 4;
/// The start-up's empty list.
pub const START_UP_EMPTY: u16 = 5;
/// The empty list the four send-event bound lists call; Logitech's is not the start-up's empty list.
pub const SEND_EVENT_EMPTY: u16 = 6;
/// The send events' tags in ascending order, one per output pin.
pub const SEND_EVENT_TAGS: [u8; 4] = [0x2D, 0x2E, 0x2F, 0x30];
/// The output pin each tag's list switches: `0x2D` pin 0, `0x2E` pin 2, `0x2F` pin 1, `0x30` pin 3.
pub const SEND_EVENT_PINS: [u8; 4] = [0, 2, 1, 3];
/// The firmware event a Harmony 300 raises for a send from an infrared group above 3.
pub const UNMASKED_SEND_EVENT: u8 = 0xFF;
/// The send event whose bound list event `0xFF` is given: the event a 350 raises for a send from
/// group 3 and every group above it.
const ACKNOWLEDGED_AS: u8 = 0x30;

const CALL: u8 = 0x7F;
const STATE_BAND: u8 = 0x1F;
const PIN_BAND: u8 = 0xE9;
const CONDITION_OPCODE: u8 = 0x71;
const CONDITION_TWO_ARMS: u16 = 0x8000;
const BYTE_REGISTER_LOAD: u8 = 0xFB;
const SENDING_FLAG: u16 = 17;
const SEND_EVENT_COMPARISON: u16 = 1;
const HOLD_TIME: u16 = 10;

/// The firmware's state records 7 to 17 on the Harmony 300 and 350, as `[first, max]`: the same in
/// all thirteen compiles of the family the reference holds, none with a transition.
const FIRMWARE_STATE_RECORDS: [[u16; 2]; 11] = [
    [0, 2],
    [0, 3],
    [5, 7],
    [0, 7],
    [0, 32],
    [0, 1],
    [0, 32],
    [1, 1],
    [0, 3],
    [0, 1],
    [0, 3],
];
/// The compiler's first two-byte variable, placed after the firmware's block.
const FIRST_WIDE_VARIABLE: [u16; 2] = [0, 65277];

/// The parameter block's five groups, in table order, the same values in all thirteen compiles.
const NO_SCREEN_PARAMETERS: [&[u16]; 5] = [&[60], &[80], &[600, 602, 766, 769], &[1800], &[0]];

/// The tags a list's entry 0 binds, sorted, paired with the pin each event's list switches.
pub fn send_event_tags() -> [(u8, u8); 4] {
    std::array::from_fn(|k| (SEND_EVENT_TAGS[k], SEND_EVENT_PINS[k]))
}

/// A call of the action list at `list`.
fn call(list: u16) -> Instruction {
    Instruction::new(CALL, list)
}

/// The index of send event `k`'s bound, load and test lists.
fn send_event_lists_at(k: usize) -> (u16, u16, u16) {
    let bound = 7 + 3 * k as u16;
    (bound, bound + 1, bound + 2)
}

/// Raw slot 5, the start-up event table: a `u8` count and eleven calls. Entry 0 and entries 5 to 10
/// call the empty list; entries 1 to 4 call their own list.
pub fn start_up_table() -> Result<Piece, Error> {
    let mut body = Vec::with_capacity(START_UP_EVENT_COUNT);
    for entry in 0..START_UP_EVENT_COUNT {
        let list = START_UP_OWN_EVENTS
            .iter()
            .find_map(|(at, list)| (*at == entry).then_some(*list))
            .unwrap_or(START_UP_EMPTY);
        body.push(call(list));
    }
    Ok(Piece::new(
        action_list_bytes(&body)?,
        "raw-5-start-up-table",
    ))
}

/// The start-up's six lists, in index order: the four event lists, the hold list and the empty list.
/// Each event list calls the hold list and then the empty list. None selects a key map, because the
/// empty configuration has no device whose map could be selected; the reference documents this as
/// the least a start-up can do.
pub fn start_up_lists() -> Result<Vec<Piece>, Error> {
    let event = action_list_bytes(&[call(START_UP_HOLD), call(START_UP_EMPTY)])?;
    let hold = action_list_bytes(&[
        Instruction::new(0x3F, 0xF000 | HOLD_TIME),
        Instruction::new(STATE_BAND, 0xFE00),
        Instruction::new(STATE_BAND, 0xFEFE),
    ])?;
    let empty = action_list_bytes(&[])?;
    Ok(vec![
        Piece::new(event.clone(), "slot-10-list"),
        Piece::new(event.clone(), "slot-10-list"),
        Piece::new(event.clone(), "slot-10-list"),
        Piece::new(event, "slot-10-list"),
        Piece::new(hold, "slot-10-list"),
        Piece::new(empty, "slot-10-list"),
    ])
}

/// The four send events' thirteen lists, in index order: the shared empty list, then per event the
/// bound list, the load list and the test list.
pub fn send_event_lists() -> Result<Vec<Piece>, Error> {
    let mut out = vec![Piece::new(action_list_bytes(&[])?, "slot-10-list")];
    for (k, (_, pin)) in send_event_tags().iter().enumerate() {
        let (_, load, test) = send_event_lists_at(k);
        out.push(Piece::new(
            action_list_bytes(&[
                call(load),
                call(SEND_EVENT_EMPTY),
                Instruction::new(0x07, 0xFFFA),
            ])?,
            "slot-10-list",
        ));
        out.push(Piece::new(
            action_list_bytes(&[
                Instruction::new(STATE_BAND, u16::from(BYTE_REGISTER_LOAD) << 8),
                call(test),
            ])?,
            "slot-10-list",
        ));
        out.push(Piece::new(
            action_list_bytes(&[
                Instruction::new(
                    CONDITION_OPCODE,
                    CONDITION_TWO_ARMS | (SEND_EVENT_COMPARISON << 8) | SENDING_FLAG,
                ),
                Instruction::new(
                    PIN_BAND,
                    (u16::from(PIN_BAND) << 8) | (u16::from(*pin) << 4) | 2,
                ),
                Instruction::new(
                    PIN_BAND,
                    (u16::from(PIN_BAND) << 8) | (u16::from(*pin) << 4),
                ),
            ])?,
            "slot-10-list",
        ));
    }
    Ok(out)
}

/// The key table's four send-event entries, one per tag in tag order, each a call of its bound list.
pub fn send_event_entries() -> Vec<TaggedEntry> {
    send_event_tags()
        .iter()
        .enumerate()
        .map(|(k, (tag, _))| {
            let (bound, _, _) = send_event_lists_at(k);
            TaggedEntry {
                tag: *tag,
                operand: bound,
                opcode: CALL,
            }
        })
        .collect()
}

/// The key table's fifth entry: firmware event `0xFF` bound to the list the `0x30` entry calls, so a
/// Harmony 300 sends from infrared groups 4 to 7 instead of hanging at the first send.
pub fn acknowledging_entry() -> TaggedEntry {
    let k = SEND_EVENT_TAGS
        .iter()
        .position(|tag| *tag == ACKNOWLEDGED_AS)
        .unwrap_or(SEND_EVENT_TAGS.len() - 1);
    let (bound, _, _) = send_event_lists_at(k);
    TaggedEntry {
        tag: UNMASKED_SEND_EVENT,
        operand: bound,
        opcode: CALL,
    }
}

/// One base slot 13 record: `first`, `max`, a zero transition count and the byte the reference says
/// is zero in every record. The empty configuration has no transitions.
fn state_record(first: u16, max: u16) -> Piece {
    let mut bytes = Vec::with_capacity(7);
    bytes.extend_from_slice(&first.to_le_bytes());
    bytes.extend_from_slice(&max.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0]);
    Piece::new(bytes, "slot-13-record")
}

/// Base slot 13's records, in index order: the seven clock records stamped with `built_at`, the
/// firmware's eleven, then the compiler's two-byte variable.
pub fn state_records(built_at: &OffsetDateTime) -> Result<Vec<Piece>, Error> {
    let fields = clock_fields(built_at)?;
    let mut records = Vec::with_capacity(7 + FIRMWARE_STATE_RECORDS.len() + 1);
    for (index, first) in fields.iter().enumerate() {
        let max = if index == 6 {
            u16::from(*first) + 1
        } else {
            CLOCK_STATE_MAXIMA[index]
        };
        records.push(state_record(u16::from(*first), max));
    }
    for [first, max] in FIRMWARE_STATE_RECORDS {
        records.push(state_record(first, max));
    }
    records.push(state_record(FIRST_WIDE_VARIABLE[0], FIRST_WIDE_VARIABLE[1]));
    Ok(records)
}

/// Base slot 13's table: count, the first two-byte variable's index, the count of two-byte
/// variables, the same index again, and a pointer per record.
pub fn state_table(records: &[usize]) -> Result<Piece, Error> {
    let count = u16::try_from(records.len())
        .map_err(|_| Error::Validation("the state table does not fit a u16 count".into()))?;
    // The firmware owns records 0 to 17 in the empty configuration; record 18 is the compiler's
    // first two-byte variable, and it is the only two-byte one.
    let narrow = count
        .checked_sub(1)
        .ok_or_else(|| Error::Validation("the state table holds no records".into()))?;
    let wide = 1u16;
    let mut bytes = Vec::with_capacity(8 + 3 * records.len());
    for word in [count, narrow, wide, narrow] {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    let refs = records
        .iter()
        .enumerate()
        .map(|(index, target)| Reference::Piece {
            at: 8 + 3 * index,
            target: *target,
            offset: 0,
        })
        .collect();
    bytes.resize(8 + 3 * records.len(), 0);
    Ok(Piece {
        bytes,
        refs,
        owner: "slot-13-table",
    })
}

/// Base slot 0, the name tree: `0xFEED`, the length, the Root and State nodes, `0xBEEF`. No
/// variable is named, since the empty configuration has none of its own.
pub fn name_tree() -> Piece {
    let mut body = Vec::new();
    for (index, name) in ["Root", "State"].iter().enumerate() {
        body.push(0xA7);
        body.extend_from_slice(&(4 + name.len() as u16).to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        body.extend_from_slice(&(index as u16).to_le_bytes());
        body.extend_from_slice(name.as_bytes());
    }
    let length = 5 + body.len() as u32;
    let mut bytes = Vec::with_capacity(2 + 3 + body.len() + END_MARKER_LENGTH);
    bytes.extend_from_slice(&[0xED, 0xFE]);
    bytes.extend_from_slice(&length.to_le_bytes()[..3]);
    bytes.extend_from_slice(&body);
    bytes.extend_from_slice(&[0xEF, 0xBE]);
    Piece::new(bytes, "slot-0-tree")
}

/// Base slot 2, the log area: the record capacity and the region's two ends. The firmware never
/// seeks this slot on architecture 16; the region is what all thirteen compiles state.
pub fn log_area() -> Piece {
    const CAPACITY: u16 = 8192;
    const START: u32 = 0x07_0000;
    const LIMIT: u32 = 0x08_0000;
    let mut bytes = Vec::with_capacity(8);
    bytes.extend_from_slice(&CAPACITY.to_le_bytes());
    bytes.extend_from_slice(&[0, 0, 0]);
    bytes.extend_from_slice(&[0, 0, 0]);
    Piece {
        bytes,
        refs: vec![
            Reference::Address {
                at: 2,
                address: START,
            },
            Reference::Address {
                at: 5,
                address: LIMIT,
            },
        ],
        owner: "slot-2-log",
    }
}

/// Base slot 15's five parameter groups, in table order.
pub fn parameter_groups() -> Result<Vec<Piece>, Error> {
    NO_SCREEN_PARAMETERS
        .iter()
        .map(|values| crate::tables::parameter_group(values, "slot-15-group"))
        .collect()
}

/// Raw slot 12, the digit entry table: a `u16` count of zero, as every compile holds it.
pub fn digit_entry_table() -> Piece {
    Piece::new(vec![0, 0], "raw-12-table")
}
