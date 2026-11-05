//! The no-screen empty configuration: a complete architecture-16 container from a skin and a moment
//! and nothing else.
//!
//! Ported from the MIT-licensed harmony-explorations codec
//! (`packages/codec/src/families/no-screen/empty.ts`). This is the reference's
//! `buildEmptyNoScreenConfiguration`, with the metadata archive left out: nothing on the remote is
//! known to read raw slot 13, and porting its ZIP/deflate writer would add a large subsystem for a
//! section the firmware does not seek. The archive's absence is the only intentional byte-level
//! difference, and it is why [`emit_empty`]'s result is shorter than the reference's 1081/1105 bytes.
//!
//! The container holds the four send events' lists and key entries, the entry binding firmware event
//! `0xFF` to the list event `0x30` calls, the start-up table and its six lists, the name tree, the
//! log area, the model's allocation of empty infrared groups, the action list table, the timer and
//! digit entry tables, the state table and its records, the parameter block and the number sender
//! table.

use time::OffsetDateTime;

use crate::error::Error;
use crate::frame::{Layout, Piece, Placement};
use crate::gspm::tagged_list_bytes;
use crate::sections::{
    acknowledging_entry, digit_entry_table, log_area, name_tree, parameter_groups,
    send_event_entries, send_event_lists, start_up_lists, start_up_table, state_records,
    state_table,
};
use crate::tables::{counted_table, in_compiler_order, number_sender_table};

/// The empty configuration for `skin`, built at `built_at`, complete with `PTYY` and its trailer
/// checksum.
///
/// Skins 79 and 104 are the two the reference measured, with four and eight infrared groups.
/// **Skin 78 is accepted with four groups**: the reference refuses it because no configuration
/// stating it has been read, but the Model Profile enables it and its skin 79 sibling, the European
/// Harmony 300, allocates four. The choice is stated here rather than derived, because nothing in
/// the container settles it.
pub fn emit_empty(skin: u8, built_at: &OffsetDateTime) -> Result<Vec<u8>, Error> {
    let group_count = infrared_groups(skin)?;
    let mut layout = Layout::new(skin, built_at)?;

    // The key table, immediately after `LWJL`: the four send event entries and event 0xFF, stored in
    // the compiler's hash order. Raw slot 6's key map table names this list as entry 0.
    let mut entries = send_event_entries();
    entries.push(acknowledging_entry());
    let ordered = in_compiler_order(&[], &entries)?;
    let key_table = layout.add(Piece::new(tagged_list_bytes(&ordered)?, "raw-6-key-table"));
    layout.set_key_table(key_table);

    // The action lists: the start-up's six, numbered 0 to 5, then the send events' thirteen, 6 to 18.
    let mut lists = start_up_lists()?;
    lists.extend(send_event_lists()?);
    let mut list_indices = Vec::with_capacity(lists.len());
    for piece in lists {
        list_indices.push(layout.add(piece));
    }

    // The state records, numbered 0 to 18: the clock, the firmware's eleven, the wide variable.
    let mut records = state_records(built_at)?;
    let mut record_indices = Vec::with_capacity(records.len());
    for piece in records.drain(..) {
        record_indices.push(layout.add(piece));
    }

    // The body: every piece reached only through an address, in the reference's order.
    let mut body = list_indices.clone();
    body.extend_from_slice(&record_indices);
    layout.set_body(body);

    let tree = layout.add(name_tree());
    let log = layout.add(log_area());

    let mut groups = Vec::with_capacity(group_count);
    for _ in 0..group_count {
        groups.push(layout.add(Piece::new(vec![0, 0, 0], "slot-5-group")));
    }
    let infrared = layout.add(counted_table(1, &groups, "raw-4-table")?);
    let start_up = layout.add(start_up_table()?);
    let key_map = layout.add(counted_table(1, &[key_table], "raw-6-table")?);
    let list_table = layout.add(counted_table(2, &list_indices, "raw-7-table")?);
    let timers = layout.add(counted_table(1, &[], "raw-8-table")?);
    let states = layout.add(state_table(&record_indices)?);

    let parameters = parameter_groups()?;
    let mut parameter_indices = Vec::with_capacity(parameters.len());
    for piece in parameters {
        parameter_indices.push(layout.add(piece));
    }
    let parameter_table = layout.add(counted_table(1, &parameter_indices, "raw-10-table")?);
    let senders = layout.add(number_sender_table(&[])?);
    let digits = layout.add(digit_entry_table());

    layout.place(0, Placement::new(Vec::new(), tree))?;
    layout.place(2, Placement::new(Vec::new(), log))?;
    layout.place(4, Placement::new(groups, infrared))?;
    layout.place(5, Placement::new(Vec::new(), start_up))?;
    layout.place(6, Placement::new(Vec::new(), key_map))?;
    layout.place(7, Placement::new(Vec::new(), list_table))?;
    layout.place(8, Placement::new(Vec::new(), timers))?;
    layout.place(9, Placement::new(Vec::new(), states))?;
    layout.place(10, Placement::new(parameter_indices, parameter_table))?;
    layout.place(11, Placement::new(Vec::new(), senders))?;
    layout.place(12, Placement::new(Vec::new(), digits))?;

    layout.lay_out()
}

/// How many infrared groups a skin's configurations allocate.
///
/// Skins 78 and 79 are the Harmony 300's two printings, with four groups; skin 104 is the Harmony
/// 350, with eight. Any other skin is refused with the skins that are known, rather than guessed.
fn infrared_groups(skin: u8) -> Result<usize, Error> {
    match skin {
        78 | 79 => Ok(4),
        104 => Ok(8),
        other => Err(Error::Validation(format!(
            "skin {other} is not supported; the empty configuration is built for skins 78, 79 and 104"
        ))),
    }
}
