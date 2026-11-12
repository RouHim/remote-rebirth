//! Parsing for the Architecture 16 Container, used by `inspect`.
//!
//! The frame and the sections this project understands follow the MIT licensed
//! harmony-explorations reference (`reference/config-format/container.md`,
//! `.../architectures/harmony-300-350/memory.md`, `.../infrared-records.md`,
//! `.../modes-and-keys.md`, `.../actions.md`, `.../state.md`). Logitech data is
//! never embedded here; every value comes from the file being inspected.

use serde::Serialize;

use crate::container::{self, FLASH_BASE, MAX_CONTAINER_SIZE};
use crate::error::Error;
use crate::profile::{self, ModelProfile};

pub const SLOT_COUNT: usize = 15;
mod derive;
mod ir;

use self::derive::{derive_devices, derive_favourites, derive_watch_tv};
use self::ir::parse_ir_database;

pub const SECTION_TABLE_OFFSET: usize = 0x0B;
pub const HEADER_MARKER_OFFSET: usize = 0x47;
pub const KEY_TABLE_OFFSET: usize = 0x4B;
pub const TRAILER_LENGTH: usize = 6;
pub const END_MARKER_LENGTH: usize = 4;

pub const IR_DATABASE_SLOT: usize = 4;
pub const KEY_MAP_SLOT: usize = 6;
pub const ACTION_LIST_SLOT: usize = 7;
pub const STATE_VARIABLE_SLOT: usize = 9;
pub const NUMBER_SENDER_SLOT: usize = 11;

const SLOT_NAMES: [&str; SLOT_COUNT] = [
    "name_tree",
    "architecture_record",
    "log_area",
    "clock_record",
    "ir_database",
    "startup_events",
    "key_map_table",
    "action_lists",
    "timer_table",
    "state_variables",
    "parameter_block",
    "number_sender",
    "digit_entry_table",
    "metadata_archive",
    "unused",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    index: usize,
    address: u32,
    offset: usize,
    length: usize,
    null: bool,
}

#[derive(Debug, Serialize)]
pub struct ContainerInfo {
    pub size: usize,
    pub end_addr: u32,
    pub base: u32,
    pub checksum: u16,
    pub slots: usize,
    pub pointer_count: usize,
    pub architecture: Option<u8>,
    pub skin: Option<u8>,
    pub sha256: String,
}

#[derive(Debug, Serialize)]
pub struct RawSlot {
    pub index: usize,
    pub name: &'static str,
    pub offset: Option<usize>,
    pub length: usize,
    pub sha256: Option<String>,
    pub known: bool,
}

#[derive(Debug, Serialize)]
pub struct KeyTable {
    pub offset: usize,
    pub count: usize,
    pub entries: Vec<KeyMapEntry>,
}

#[derive(Debug, Serialize)]
pub struct KeyMap {
    pub index: usize,
    pub address: u32,
    pub offset: usize,
    pub wide: bool,
    pub entries: Vec<KeyMapEntry>,
}

#[derive(Debug, Serialize)]
pub struct KeyMapEntry {
    pub tag: u8,
    pub event: &'static str,
    pub scan_code: u8,
    pub operand: u16,
    pub opcode: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_list: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_index: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send: Option<IrReference>,
}

#[derive(Debug, Serialize)]
pub struct Device {
    pub map_index: usize,
    pub device_key: Option<DeviceKey>,
    pub ir_group: Option<u8>,
    pub record_count: usize,
}

#[derive(Debug, Serialize)]
pub struct DeviceKey {
    pub place: u8,
    pub label: &'static str,
    pub scan_code: u8,
    pub event: &'static str,
}

#[derive(Debug, Serialize)]
pub struct WatchTvActivity {
    pub action_list: u16,
    pub selected_map: Option<u8>,
    pub instructions: Vec<ActivityInstruction>,
    pub sends: Vec<IrReference>,
    pub delays: Vec<IrDelay>,
}

#[derive(Debug, Serialize)]
pub struct ActivityInstruction {
    pub opcode: u8,
    pub operand: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_list: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub send: Option<IrReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<IrDelay>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub select_map: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number: Option<u16>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IrReference {
    pub group: u8,
    pub record: u8,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IrDelay {
    pub group: u8,
    pub tenths: u8,
}

#[derive(Debug, Serialize)]
pub struct Favourite {
    pub key: Option<u8>,
    pub scan_code: u8,
    pub map_index: usize,
    pub action_list: u16,
    pub channel: String,
}

#[derive(Debug, Serialize)]
pub struct IrRecord {
    pub group: usize,
    pub index: usize,
    pub address: u32,
    pub offset: usize,
    pub class: u8,
    pub carrier: IrCarrier,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub once: Option<IrBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held: Option<IrBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<IrBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toggle: Option<IrPointerGroup>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra_groups: Vec<IrPointerGroup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrCarrier {
    pub period_ns: u32,
    pub on_ns: u32,
    pub hertz: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrBlock {
    pub address: u32,
    pub offset: usize,
    pub bytes: usize,
    pub words: usize,
    pub duration_us: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct IrPointerGroup {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub once: Option<IrBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub held: Option<IrBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tail: Option<IrBlock>,
}

#[derive(Debug, Serialize)]
pub struct StateVariables {
    pub count: u16,
    pub narrow: u16,
    pub wide: u16,
    pub narrow_again: u16,
    pub variables: Vec<StateVariable>,
}

#[derive(Debug, Serialize)]
pub struct StateVariable {
    pub index: usize,
    pub width: &'static str,
    pub first: u16,
    pub max: u16,
    pub transitions: Vec<StateTransition>,
}

#[derive(Debug, Serialize)]
pub struct StateTransition {
    pub from: i16,
    pub to: i16,
    pub operand: u16,
    pub opcode: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_list: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct NumberSender {
    pub index: usize,
    pub flags: u8,
    pub base: u32,
    pub digits: u8,
    pub prologue: InstructionRef,
    pub epilogue: InstructionRef,
    pub prefix: InstructionRef,
    pub digit_tables: Vec<DigitTable>,
}

#[derive(Debug, Serialize)]
pub struct InstructionRef {
    pub operand: u16,
    pub opcode: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_list: Option<u16>,
}

#[derive(Debug, Serialize)]
pub struct DigitTable {
    pub address: u32,
    pub instructions: Vec<InstructionRef>,
}

#[derive(Debug, Serialize)]
pub struct ParsedContainer {
    pub schema_version: u8,
    pub container: ContainerInfo,
    pub raw_slots: Vec<RawSlot>,
    /// The infrared group count from the infrared table. Not serialized: `ir_records` states each
    /// record's group, so JSON consumers need no separate count, while Write and Restore check it.
    #[serde(skip)]
    pub ir_groups: usize,
    pub key_table: KeyTable,
    pub key_maps: Vec<KeyMap>,
    pub devices: Vec<Device>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watch_tv_activity: Option<WatchTvActivity>,
    pub favourites: Vec<Favourite>,
    pub ir_records: Vec<IrRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_variables: Option<StateVariables>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_senders: Option<Vec<NumberSender>>,
}

#[derive(Debug, Clone)]
struct Instruction {
    operand: u16,
    opcode: u8,
}

pub fn parse(bytes: &[u8]) -> Result<ParsedContainer, Error> {
    if bytes.len() > MAX_CONTAINER_SIZE {
        return Err(Error::Validation(format!(
            "Container is {} bytes, above the 256 KiB limit ({} bytes)",
            bytes.len(),
            MAX_CONTAINER_SIZE
        )));
    }
    if bytes.get(0..4) != Some(b"GSPM".as_slice()) {
        return Err(Error::Validation(
            "not an Architecture 16 Container: expected GSPM at offset 0".into(),
        ));
    }
    if bytes.len() < KEY_TABLE_OFFSET + 1 + TRAILER_LENGTH {
        return Err(Error::Validation(format!(
            "Container is {} bytes, too short to hold an Architecture 16 header and trailer",
            bytes.len()
        )));
    }
    if &bytes[HEADER_MARKER_OFFSET..HEADER_MARKER_OFFSET + 4] != b"LWJL" {
        return Err(Error::Validation(format!(
            "not an Architecture 16 Container: expected LWJL at offset 0x{HEADER_MARKER_OFFSET:X}"
        )));
    }

    let trailer_offset = bytes.len() - END_MARKER_LENGTH;
    if &bytes[trailer_offset..] != b"PTYY" {
        return Err(Error::Validation(format!(
            "Container does not end with PTYY at offset 0x{trailer_offset:X}"
        )));
    }

    let end_addr = u32_at(bytes, 4, "end_addr")?;
    let expected_end = FLASH_BASE + trailer_offset as u32;
    if end_addr != expected_end {
        return Err(Error::Validation(format!(
            "end_addr 0x{end_addr:06X} does not point at the PTYY end marker at 0x{expected_end:06X}"
        )));
    }

    let stored = u16_at(bytes, bytes.len() - TRAILER_LENGTH, "trailer checksum")?;
    let computed = container::trailer_checksum(bytes)?;
    if stored != computed {
        return Err(Error::Validation(format!(
            "trailer checksum mismatch: stored 0x{stored:04X}, computed 0x{computed:04X}"
        )));
    }

    let mut slots = [Slot {
        index: 0,
        address: 0,
        offset: 0,
        length: 0,
        null: true,
    }; SLOT_COUNT];

    let mut previous: Option<(usize, u32)> = None;
    for (index, slot) in slots.iter_mut().enumerate() {
        let at = SECTION_TABLE_OFFSET + 4 * index;
        let spare = u8_at(bytes, at, "section table spare byte")?;
        let address = u24_at(bytes, at + 1, "section table address")?;
        if spare != 0 {
            return Err(Error::Validation(format!(
                "section slot {index} has spare byte 0x{spare:02X}, expected zero"
            )));
        }
        if address == 0 {
            slot.index = index;
            continue;
        }
        if address < FLASH_BASE || address >= FLASH_BASE + trailer_offset as u32 {
            return Err(Error::Validation(format!(
                "section slot {index} address 0x{address:06X} is outside the container \
                 (base 0x{FLASH_BASE:06X}, end 0x{expected_end:06X})"
            )));
        }
        if let Some((previous_index, previous_address)) = previous
            && address <= previous_address
        {
            return Err(Error::Validation(format!(
                "section slot {index} starts at 0x{address:06X}, not after slot \
                 {previous_index} at 0x{previous_address:06X}; section pointers must ascend"
            )));
        }
        previous = Some((index, address));
        *slot = Slot {
            index,
            address,
            offset: (address - FLASH_BASE) as usize,
            length: 0,
            null: false,
        };
    }

    let mut section_ends = [trailer_offset; SLOT_COUNT];
    for index in 0..SLOT_COUNT {
        if slots[index].null {
            continue;
        }
        let end = (index + 1..SLOT_COUNT)
            .find_map(|next| (!slots[next].null).then_some(slots[next].offset))
            .unwrap_or(trailer_offset);
        if end < slots[index].offset {
            return Err(Error::Validation(format!(
                "section slot {index} at 0x{:06X} runs past the section after it",
                slots[index].address
            )));
        }
        section_ends[index] = end;
    }
    for (index, slot) in slots.iter_mut().enumerate() {
        if !slot.null {
            slot.length = section_ends[index] - slot.offset;
        }
    }

    let key_table = parse_key_table(bytes, trailer_offset)?;
    let (architecture, skin) = parse_architecture_record(bytes, &slots)?;

    let (ir_groups, ir_records) = parse_ir_database(bytes, &slots)?;
    let key_maps = parse_key_maps(bytes, &slots)?;
    let action_lists = parse_action_lists(bytes, &slots)?;
    let key_profile = profile::for_skin(skin).unwrap_or(&profile::HARMONY_300);
    let devices = derive_devices(&key_maps, &ir_records, key_profile);
    let watch_tv_activity = derive_watch_tv(&key_table, &key_maps, &action_lists)?;
    let favourites = derive_favourites(&key_maps, &action_lists, key_profile, skin);
    let state_variables = parse_state_variables(bytes, &slots)?;
    let number_senders = parse_number_senders(bytes, &slots)?;

    let mut raw_slots = Vec::with_capacity(SLOT_COUNT);
    for slot in slots.iter() {
        let (offset, length, sha256) = if slot.null {
            (None, 0, None)
        } else {
            let bytes = &bytes[slot.offset..slot.offset + slot.length];
            (
                Some(slot.offset),
                slot.length,
                Some(container::sha256_hex(bytes)),
            )
        };
        raw_slots.push(RawSlot {
            index: slot.index,
            name: SLOT_NAMES[slot.index],
            offset,
            length,
            sha256,
            known: true,
        });
    }

    Ok(ParsedContainer {
        schema_version: 1,
        container: ContainerInfo {
            size: bytes.len(),
            end_addr,
            base: FLASH_BASE,
            checksum: stored,
            slots: SLOT_COUNT,
            pointer_count: SLOT_COUNT,
            architecture: Some(architecture),
            skin: Some(skin),
            sha256: container::sha256_hex(bytes),
        },
        raw_slots,
        ir_groups,
        key_table,
        key_maps,
        devices,
        watch_tv_activity,
        favourites,
        ir_records,
        state_variables,
        number_senders,
    })
}

fn parse_architecture_record(bytes: &[u8], slots: &[Slot; SLOT_COUNT]) -> Result<(u8, u8), Error> {
    let slot = slots[1];
    if slot.null {
        return Err(Error::Validation(
            "raw slot 1 (architecture record) is NULL; not an Architecture 16 Container".into(),
        ));
    }
    if slot.length < 4 {
        return Err(Error::Validation(format!(
            "raw slot 1 (architecture record) is {} bytes, too short to state an architecture",
            slot.length
        )));
    }
    let offset = slot.offset;
    let first = u8_at(bytes, offset, "architecture record")?;
    let second = u8_at(bytes, offset + 1, "architecture record")?;
    if first != second {
        return Err(Error::Validation(format!(
            "architecture record states {first} and {second}; it must state one architecture twice"
        )));
    }
    if first != 16 {
        return Err(Error::Validation(format!(
            "architecture record states architecture {first}, expected 16"
        )));
    }
    Ok((first, u8_at(bytes, offset + 2, "skin")?))
}

fn parse_key_table(bytes: &[u8], trailer_offset: usize) -> Result<KeyTable, Error> {
    let count = u8_at(bytes, KEY_TABLE_OFFSET, "key table count")? as usize;
    let end = KEY_TABLE_OFFSET + 1 + 4 * count;
    if end > trailer_offset {
        return Err(Error::Validation(format!(
            "key table at 0x{KEY_TABLE_OFFSET:X} declares {count} entries and overruns the trailer"
        )));
    }
    let entries = (0..count)
        .map(|index| {
            let at = KEY_TABLE_OFFSET + 1 + 4 * index;
            parse_key_map_entry(bytes, at)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(KeyTable {
        offset: KEY_TABLE_OFFSET,
        count,
        entries,
    })
}

fn parse_key_map_entry(bytes: &[u8], at: usize) -> Result<KeyMapEntry, Error> {
    let tag = u8_at(bytes, at, "key map tag")?;
    let operand = u16_at(bytes, at + 1, "key map operand")?;
    let opcode = u8_at(bytes, at + 3, "key map opcode")?;
    Ok(KeyMapEntry {
        tag,
        event: event_name(tag),
        scan_code: tag & 0x3F,
        operand,
        opcode,
        action_list: (opcode == 0x7F).then_some(operand),
        map_index: (opcode == 0x1F && operand >> 8 == 0xFF).then_some((operand & 0xFF) as u8),
        send: (opcode == 0x7D).then_some(IrReference {
            group: (operand >> 8) as u8,
            record: (operand & 0xFF) as u8,
        }),
    })
}

fn event_name(tag: u8) -> &'static str {
    match tag & 0xC0 {
        0x40 => "release",
        0x80 => "press",
        0xC0 => "held",
        _ => "none",
    }
}

fn parse_key_maps(bytes: &[u8], slots: &[Slot; SLOT_COUNT]) -> Result<Vec<KeyMap>, Error> {
    let slot = slots[KEY_MAP_SLOT];
    if slot.null {
        return Ok(Vec::new());
    }
    let count = u8_at(bytes, slot.offset, "key map count")? as usize;
    let table_end = slot.offset + 1 + 3 * count;
    if table_end > slot.offset + slot.length {
        return Err(Error::Validation(format!(
            "key map table at 0x{:06X} declares {count} maps and overruns its section",
            slot.address
        )));
    }
    let mut maps = Vec::with_capacity(count);
    for index in 0..count {
        let address = u24_at(bytes, slot.offset + 1 + 3 * index, "key map address")?;
        if address == 0 {
            return Err(Error::Validation(format!("key map {index} is NULL")));
        }
        let offset = address_offset(bytes, address, "key map")?;
        let (wide, entries) = parse_tagged_list(bytes, offset, address)?;
        maps.push(KeyMap {
            index,
            address,
            offset,
            wide,
            entries,
        });
    }
    Ok(maps)
}

fn parse_tagged_list(
    bytes: &[u8],
    offset: usize,
    address: u32,
) -> Result<(bool, Vec<KeyMapEntry>), Error> {
    let first = u8_at(bytes, offset, "tagged list count")?;
    let wide = first == 0;
    let (count, header, stride) = if wide {
        (
            u8_at(bytes, offset + 1, "tagged list count")? as usize,
            2,
            5,
        )
    } else {
        (first as usize, 1, 4)
    };
    let length = header + stride * count;
    if offset + length > bytes.len() {
        return Err(Error::Validation(format!(
            "tagged list at 0x{address:06X} declares {count} entries and overruns the container"
        )));
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let at = offset + header + stride * index;
        if wide {
            entries.push(parse_key_map_entry(bytes, at + 1)?);
        } else {
            entries.push(parse_key_map_entry(bytes, at)?);
        }
    }
    Ok((wide, entries))
}

fn parse_action_lists(
    bytes: &[u8],
    slots: &[Slot; SLOT_COUNT],
) -> Result<Vec<Vec<Instruction>>, Error> {
    let slot = slots[ACTION_LIST_SLOT];
    if slot.null {
        return Ok(Vec::new());
    }
    let count = u16_at(bytes, slot.offset, "action list count")? as usize;
    let table_end = slot.offset + 2 + 3 * count;
    if table_end > slot.offset + slot.length {
        return Err(Error::Validation(format!(
            "action list table at 0x{:06X} declares {count} lists and overruns its section",
            slot.address
        )));
    }
    let mut lists = Vec::with_capacity(count);
    for index in 0..count {
        let address = u24_at(bytes, slot.offset + 2 + 3 * index, "action list address")?;
        if address == 0 {
            return Err(Error::Validation(format!("action list {index} is NULL")));
        }
        let offset = address_offset(bytes, address, "action list")?;
        let length = u8_at(bytes, offset, "action list length")? as usize;
        let end = offset + 1 + 3 * length;
        if end > bytes.len() {
            return Err(Error::Validation(format!(
                "action list {index} at 0x{address:06X} declares {length} instructions and \
                 overruns the container"
            )));
        }
        let mut instructions = Vec::with_capacity(length);
        for at in (offset + 1..end).step_by(3) {
            instructions.push(Instruction {
                operand: u16_at(bytes, at, "action list operand")?,
                opcode: u8_at(bytes, at + 2, "action list opcode")?,
            });
        }
        lists.push(instructions);
    }
    Ok(lists)
}

fn parse_state_variables(
    bytes: &[u8],
    slots: &[Slot; SLOT_COUNT],
) -> Result<Option<StateVariables>, Error> {
    let slot = slots[STATE_VARIABLE_SLOT];
    if slot.null {
        return Ok(None);
    }
    let count = u16_at(bytes, slot.offset, "state variable count")?;
    let narrow = u16_at(bytes, slot.offset + 2, "narrow state variable count")?;
    let wide = u16_at(bytes, slot.offset + 4, "wide state variable count")?;
    let narrow_again = u16_at(
        bytes,
        slot.offset + 6,
        "repeated narrow state variable count",
    )?;
    let table_end = slot.offset + 8 + 3 * count as usize;
    if table_end > slot.offset + slot.length {
        return Err(Error::Validation(format!(
            "state variable table at 0x{:06X} declares {count} variables and overruns its section",
            slot.address
        )));
    }
    let mut variables = Vec::with_capacity(count as usize);
    for index in 0..count as usize {
        let address = u24_at(
            bytes,
            slot.offset + 8 + 3 * index,
            "state variable record address",
        )?;
        if address == 0 {
            return Err(Error::Validation(format!(
                "state variable record {index} is NULL"
            )));
        }
        let offset = address_offset(bytes, address, "state variable record")?;
        let first = u16_at(bytes, offset, "state variable first value")?;
        let max = u16_at(bytes, offset + 2, "state variable maximum")?;
        let transition_count = u16_at(bytes, offset + 4, "state transition count")? as usize;
        let end = offset + 7 + 8 * transition_count;
        if end > bytes.len() {
            return Err(Error::Validation(format!(
                "state variable record {index} at 0x{address:06X} declares \
                 {transition_count} transitions and overruns the container"
            )));
        }
        let mut transitions = Vec::with_capacity(transition_count);
        for transition in 0..transition_count {
            let at = offset + 7 + 8 * transition;
            let from = u16_at(bytes, at + 1, "state transition from")? as i16;
            let to = u16_at(bytes, at + 3, "state transition to")? as i16;
            let operand = u16_at(bytes, at + 5, "state transition operand")?;
            let opcode = u8_at(bytes, at + 7, "state transition opcode")?;
            transitions.push(StateTransition {
                from,
                to,
                operand,
                opcode,
                action_list: (opcode == 0x7F).then_some(operand),
            });
        }
        variables.push(StateVariable {
            index,
            width: if index < narrow as usize {
                "narrow"
            } else {
                "wide"
            },
            first,
            max,
            transitions,
        });
    }
    Ok(Some(StateVariables {
        count,
        narrow,
        wide,
        narrow_again,
        variables,
    }))
}

fn parse_number_senders(
    bytes: &[u8],
    slots: &[Slot; SLOT_COUNT],
) -> Result<Option<Vec<NumberSender>>, Error> {
    let slot = slots[NUMBER_SENDER_SLOT];
    if slot.null {
        return Ok(None);
    }
    let count = u8_at(bytes, slot.offset, "number sender count")? as usize;
    let table_end = slot.offset + 1 + 3 * count;
    if table_end > slot.offset + slot.length {
        return Err(Error::Validation(format!(
            "number sender table at 0x{:06X} declares {count} records and overruns its section",
            slot.address
        )));
    }
    let mut records = Vec::with_capacity(count);
    for index in 0..count {
        let address = u24_at(bytes, slot.offset + 1 + 3 * index, "number sender address")?;
        if address == 0 {
            return Err(Error::Validation(format!(
                "number sender record {index} is NULL"
            )));
        }
        let offset = address_offset(bytes, address, "number sender record")?;
        if offset + 23 > bytes.len() {
            return Err(Error::Validation(format!(
                "number sender record {index} at 0x{address:06X} overruns the container"
            )));
        }
        let mut tables = Vec::with_capacity(3);
        for table in [14usize, 17, 20] {
            let table_address = u24_at(bytes, offset + table, "digit table address")?;
            let table_offset = address_offset(bytes, table_address, "digit table")?;
            if table_offset + 30 > bytes.len() {
                return Err(Error::Validation(format!(
                    "digit table at 0x{table_address:06X} overruns the container"
                )));
            }
            let mut instructions = Vec::with_capacity(10);
            for digit in 0..10 {
                let at = table_offset + 3 * digit;
                instructions.push(instruction_ref(
                    u16_at(bytes, at, "digit instruction operand")?,
                    u8_at(bytes, at + 2, "digit instruction opcode")?,
                ));
            }
            tables.push(DigitTable {
                address: table_address,
                instructions,
            });
        }
        records.push(NumberSender {
            index,
            flags: u8_at(bytes, offset, "number sender flags")?,
            base: u24_at(bytes, offset + 1, "number sender base")?,
            digits: u8_at(bytes, offset + 4, "number sender minimum digits")?,
            prologue: instruction_ref(
                u16_at(bytes, offset + 5, "number sender prologue operand")?,
                u8_at(bytes, offset + 7, "number sender prologue opcode")?,
            ),
            epilogue: instruction_ref(
                u16_at(bytes, offset + 8, "number sender epilogue operand")?,
                u8_at(bytes, offset + 10, "number sender epilogue opcode")?,
            ),
            prefix: instruction_ref(
                u16_at(bytes, offset + 11, "number sender prefix operand")?,
                u8_at(bytes, offset + 13, "number sender prefix opcode")?,
            ),
            digit_tables: tables,
        });
    }
    Ok(Some(records))
}

fn instruction_ref(operand: u16, opcode: u8) -> InstructionRef {
    InstructionRef {
        operand,
        opcode,
        action_list: (opcode == 0x7F).then_some(operand),
    }
}

fn address_offset(bytes: &[u8], address: u32, what: &str) -> Result<usize, Error> {
    if address < FLASH_BASE {
        return Err(Error::Validation(format!(
            "{what} address 0x{address:06X} is below the container base 0x{FLASH_BASE:06X}"
        )));
    }
    let offset = (address - FLASH_BASE) as usize;
    if offset >= bytes.len() {
        return Err(Error::Validation(format!(
            "{what} address 0x{address:06X} is outside the container"
        )));
    }
    Ok(offset)
}

fn u8_at(bytes: &[u8], at: usize, what: &str) -> Result<u8, Error> {
    bytes.get(at).copied().ok_or_else(|| {
        Error::Validation(format!(
            "{what} at offset 0x{at:X} is outside the container"
        ))
    })
}

fn u16_at(bytes: &[u8], at: usize, what: &str) -> Result<u16, Error> {
    let slice = bytes.get(at..at + 2).ok_or_else(|| {
        Error::Validation(format!(
            "{what} at offset 0x{at:X} is outside the container"
        ))
    })?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u24_at(bytes: &[u8], at: usize, what: &str) -> Result<u32, Error> {
    let slice = bytes.get(at..at + 3).ok_or_else(|| {
        Error::Validation(format!(
            "{what} at offset 0x{at:X} is outside the container"
        ))
    })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], 0]))
}

fn u32_at(bytes: &[u8], at: usize, what: &str) -> Result<u32, Error> {
    let slice = bytes.get(at..at + 4).ok_or_else(|| {
        Error::Validation(format!(
            "{what} at offset 0x{at:X} is outside the container"
        ))
    })?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}
