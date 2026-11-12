//! Building an Architecture 16 Container from a validated model and its imported IR Records.
//!
//! The sections are the ones the empty configuration states (see the codec tests), with the model's Devices, key maps,
//! send lists, Watch TV Activity, Favourites, number sender and IR Records filled in. The shapes follow
//! the MIT-licensed harmony-explorations codec (`families/no-screen/*`): a send list cancels the timers,
//! sends its record and states a quantity; a device key's press in the key table selects the Device's
//! map; Watch TV's list calls each step's send list, waits and selects the channel Device's map;
//! Favourites are number loads handed to the number sender's record 0; and each IR group is a count and
//! a pointer per record, with the blocks in front of the record header.
//!
//! Every address inside an IR Record is a [`Reference`] into its own pieces rather than a number, because
//! the frame decides where a piece lands. A record is two pieces — its blocks and the first eight header
//! bytes, then its own start, group count and pointers — so that the header's address fields can name a
//! piece the frame has already indexed.

use time::OffsetDateTime;

use crate::architecture::{Instruction, TaggedEntry, action_list_bytes, tagged_list_bytes};
use crate::container::{FLASH_BASE, ValidationStatus, validation_status};
use crate::error::Error;
use crate::frame::{Layout, Piece, Placement, Reference};
use crate::ir::{ImportedCommand, IrRecord};
use crate::model::{Activity, Model};
use crate::profile::{self, ModelProfile};
use crate::sections::{
    START_UP_EMPTY, acknowledging_entry, digit_entry_table, log_area, name_tree, parameter_groups,
    send_event_entries, send_event_lists, start_up_lists_with, start_up_table, state_records,
    state_table,
};
use crate::tables::{counted_table, in_compiler_order, number_sender_table};

/// A call of an action list.
const CALL: u8 = 0x7F;
/// A send of one IR Record: the group in the operand's high byte, the record in its low byte.
const SEND: u8 = 0x7D;
/// A wait: the group in the operand's high byte, tenths of a second in its low byte.
const DELAY: u8 = 0x7C;
/// A number load into the accumulator: the channel in the operand.
const NUMBER: u8 = 0x7A;
/// A select of a key map: `0xFF00 | map`.
const SELECT_MAP: u8 = 0x1F;
const SELECT_MAP_MASK: u16 = 0xFF00;
/// The number sender record a channel is handed to, in the band's low byte.
const NUMBER_SENDER_BAND: u16 = 0xF300;
/// A key map entry's press event.
const PRESS: u8 = 0x80;
/// The key table's Watch TV tag: scan 6 with no event bits, the tag `inspect` reads an activity's
/// binding by.
const WATCH_TV_TAG: u8 = PRESS | 0x06;
/// The timers a send list cancels.
const CANCEL_TIMERS: u16 = 0xFFFB;
/// A send's quantity, in tenths: one, the reference's default for an ordinary command.
const DEVICE_QUANTITY: u8 = 1;
/// The number sender's record length.
const NUMBER_SENDER_RECORD_LENGTH: usize = 23;
/// One digit table's length: ten instructions of a `u16` operand and an opcode.
const DIGIT_TABLE_LENGTH: usize = 30;
/// The number sender's record holds three identical digit tables.
const DIGIT_TABLE_COUNT: usize = 3;

/// What one composed Container holds, in the model's Device order.
pub struct Composed {
    pub bytes: Vec<u8>,
    pub devices: Vec<ComposedDevice>,
    pub favourites: usize,
}

/// One Device's place in the composed Container.
pub struct ComposedDevice {
    /// The Device's infrared group: the Device Key's place, counted from zero.
    pub ir_group: u8,
    /// The Device's map index in raw slot 6, where the key table is 0.
    pub map_index: u8,
    /// Each Command's record index in the Device's infrared group, in the model's Command order.
    /// `None` means the Command is not sent and has no record in the Container.
    pub record_indices: Vec<Option<u8>>,
}

/// A Device's key map entry waiting for the compiler order: the tag, and the label a collision names.
struct MapEntry {
    entry: TaggedEntry,
    label: String,
}

/// Build the Container for `model`, whose Commands `imports` holds per Device in the model's command
/// order. `None` marks a Command no key or Activity sends; it is left out of the Container. Every
/// check the model validation cannot make — the profile's key layout, a favourite key's scan
/// colliding with a bound key — is made here before a byte is laid out.
pub fn build(
    model: &Model,
    profile: &ModelProfile,
    imports: &[Vec<Option<ImportedCommand>>],
    built_at: &OffsetDateTime,
) -> Result<Composed, Error> {
    let plan = plan_devices(model, profile, imports)?;

    let mut layout = Layout::new(model.skin, built_at)?;

    // The lists, in index order: the start-up's six, the send events' thirteen, then one send list per
    // Command in Device order and Command name order, the Watch TV Activity's list and the Favourites'.
    let mut lists = send_lists(model, imports, &plan)?;
    let channel_device = channel_device(model)?;
    let (activity_index, activity) = activity_list(
        model,
        &lists.command,
        &plan,
        channel_device,
        lists.pieces.len(),
    )?;
    lists.pieces.push(activity);
    let favourites = favourite_lists(model, profile, &plan, channel_device, &mut lists.pieces)?;

    // The key table: the send events, event 0xFF, one selection per Device Key and Watch TV.
    let key_table = key_table(&mut layout, &plan, profile, activity_index)?;

    // Each Device's own map, in map order: a press per bound key calling that Command's send list, and
    // the Favourite presses on the channel Device.
    let map_indices = device_maps(
        &mut layout,
        model,
        profile,
        &plan,
        &lists.command,
        &favourites,
        channel_device,
    )?;

    let tables = Tables {
        key_table,
        map_indices,
        digit_lists: digit_lists(model, channel_device, &lists.command),
        favourites: favourites.len(),
    };
    assemble(
        &mut layout,
        model,
        imports,
        lists.pieces,
        built_at,
        &plan,
        tables,
    )
}

/// What the Container needs before its first piece: the skin, the Device Key order, and the map and
/// group each Device is numbered by.
struct DevicePlan<'a> {
    skin: &'a profile::SkinProfile,
    group_count: usize,
    order: Vec<usize>,
    map_of: Vec<u8>,
    group_of: Vec<u8>,
}

/// The lists so far — the start-up's, the send events' and one per sent Command — and each Command's
/// send list index per Device.
struct SendLists {
    pieces: Vec<Piece>,
    command: Vec<Vec<Option<u16>>>,
}

/// The pieces the slots name, once the key table and the Device maps are laid out.
struct Tables {
    key_table: usize,
    map_indices: Vec<usize>,
    digit_lists: [u16; 10],
    favourites: usize,
}

/// The action lists and the state records the body slot holds.
struct Body {
    list_targets: Vec<usize>,
    state_record_indices: Vec<usize>,
}

/// The infrared groups: the pieces the slot holds in front of the group table, the table's own piece,
/// and each Command's record index per Device.
struct Infrared {
    before: Vec<usize>,
    table: usize,
    record_indices: Vec<Vec<Option<u8>>>,
}

/// The piece indices the twelve slots name.
struct Slots {
    tree: usize,
    log: usize,
    infrared_before: Vec<usize>,
    infrared: usize,
    start_up: usize,
    map_indices: Vec<usize>,
    key_map: usize,
    list_table: usize,
    timers: usize,
    states: usize,
    parameter_indices: Vec<usize>,
    parameter_table: usize,
    sender_before: Vec<usize>,
    senders: usize,
    digits: usize,
}

/// The skin, the Device Key order and the map and group numbering, before any byte is laid out.
fn plan_devices<'a>(
    model: &Model,
    profile: &'a ModelProfile,
    imports: &[Vec<Option<ImportedCommand>>],
) -> Result<DevicePlan<'a>, Error> {
    let skin = profile::skin_profile(profile, model.skin).ok_or_else(|| {
        Error::Validation(format!(
            "skin {} is not accepted by profile '{}'",
            model.skin, profile.id
        ))
    })?;
    if imports.len() != model.devices.len() {
        return Err(Error::Internal(
            "the imported commands do not match the model's devices".into(),
        ));
    }
    let group_count = profile.max_ir_groups;

    // The maps are numbered in Device Key order, so the Device on the lowest key is map 1 and the
    // start-up can select it. The IR group is the place itself, counted from zero.
    let mut order: Vec<usize> = (0..model.devices.len()).collect();
    order.sort_by_key(|&index| model.devices[index].device_key);
    let mut map_of = vec![0u8; model.devices.len()];
    let mut group_of = vec![0u8; model.devices.len()];
    for (rank, &device) in order.iter().enumerate() {
        map_of[device] = u8::try_from(rank + 1)
            .map_err(|_| Error::Validation("the model declares more key maps than fit".into()))?;
        let place = usize::from(model.devices[device].device_key)
            .checked_sub(1)
            .ok_or_else(|| Error::Validation("a Device Key is place 0".into()))?;
        if place >= profile.device_key_scans.len() || place >= group_count {
            return Err(Error::Validation(format!(
                "Device '{}' uses Device Key {}, and profile '{}' has {group_count} infrared groups",
                model.devices[device].name, model.devices[device].device_key, profile.id
            )));
        }
        group_of[device] = u8::try_from(place)
            .map_err(|_| Error::Internal("a Device Key place exceeds a byte".into()))?;
    }
    Ok(DevicePlan {
        skin,
        group_count,
        order,
        map_of,
        group_of,
    })
}

/// The start-up's six lists and the send events' thirteen, then one send list per Command in Device
/// order and Command name order.
fn send_lists(
    model: &Model,
    imports: &[Vec<Option<ImportedCommand>>],
    plan: &DevicePlan<'_>,
) -> Result<SendLists, Error> {
    let start_up_map = plan.order.first().map(|&device| plan.map_of[device]);
    let mut lists: Vec<Piece> = start_up_lists_with(start_up_map)?;
    lists.extend(send_event_lists()?);
    let mut command_lists: Vec<Vec<Option<u16>>> = Vec::with_capacity(model.devices.len());
    for (device, device_imports) in model.devices.iter().zip(imports) {
        if device_imports.len() != device.commands.len() {
            return Err(Error::Internal(format!(
                "Device '{}' has {} Commands but {} imported records",
                device.name,
                device.commands.len(),
                device_imports.len()
            )));
        }
        let group = u16::from(plan.group_of[command_lists.len()]);
        let mut indices = Vec::with_capacity(device.commands.len());
        let mut record = 0u16;
        for imported in device_imports {
            if imported.is_none() {
                indices.push(None);
                continue;
            }
            let index = u16::try_from(lists.len())
                .map_err(|_| Error::Validation("the action list table overflows a u16".into()))?;
            let slot = u8::try_from(record).map_err(|_| {
                Error::Validation(format!(
                    "Device '{}' declares more than 256 sent Commands",
                    device.name
                ))
            })?;
            let body = [
                Instruction::new(0x07, CANCEL_TIMERS),
                Instruction::new(SEND, (group << 8) | u16::from(slot)),
                Instruction::new(DELAY, (group << 8) | u16::from(DEVICE_QUANTITY)),
            ];
            lists.push(Piece::new(action_list_bytes(&body)?, "slot-10-list"));
            indices.push(Some(index));
            record = record
                .checked_add(1)
                .ok_or_else(|| Error::Validation("too many sent Commands".into()))?;
        }
        command_lists.push(indices);
    }
    Ok(SendLists {
        pieces: lists,
        command: command_lists,
    })
}

/// The channel Device the Watch TV Activity ends in.
fn channel_device(model: &Model) -> Result<usize, Error> {
    model
        .devices
        .iter()
        .position(|device| device.name == model.activity.channel_device)
        .ok_or_else(|| {
            Error::Validation(format!(
                "Watch TV Activity names missing channel Device '{}'",
                model.activity.channel_device
            ))
        })
}

/// The activity's list: each step's power and input sends, a wait for the step's delay, and the
/// channel Device's map selected at the end. The index is the list's place in the list table.
fn activity_list(
    model: &Model,
    command_lists: &[Vec<Option<u16>>],
    plan: &DevicePlan<'_>,
    channel_device: usize,
    lists_len: usize,
) -> Result<(u16, Piece), Error> {
    let activity_index = u16::try_from(lists_len)
        .map_err(|_| Error::Validation("the action list table overflows a u16".into()))?;
    let piece = Piece::new(
        action_list_bytes(&activity_body(
            model,
            &model.activity,
            command_lists,
            &plan.group_of,
            plan.map_of[channel_device],
        )?)?,
        "slot-10-list",
    );
    Ok((activity_index, piece))
}

/// The Favourites, positional from 1: a number load handed to number sender record 0, bound on the
/// channel Device's own map. The number sender's digit tables call the channel Device's digit send
/// lists, which its `number_*` keys name. Returns each Favourite's scan and list index.
fn favourite_lists(
    model: &Model,
    profile: &ModelProfile,
    plan: &DevicePlan<'_>,
    channel_device: usize,
    lists: &mut Vec<Piece>,
) -> Result<Vec<(u8, u16)>, Error> {
    let skin = plan.skin;
    let mut favourites: Vec<(u8, u16)> = Vec::new();
    if !model.favourites.is_empty() {
        let channel = &model.devices[channel_device];
        for favourite in &model.favourites {
            let place = usize::from(favourite.key)
                .checked_sub(1)
                .ok_or_else(|| Error::Validation("a Favourite key is 0".into()))?;
            let scan = *skin.favourite_scans.get(place).ok_or_else(|| {
                Error::Validation(format!(
                    "Favourite key {} is outside 1 to {} for skin {}",
                    favourite.key,
                    skin.favourite_scans.len(),
                    skin.skin
                ))
            })?;
            let channel_number: u16 = favourite.channel.parse().map_err(|_| {
                Error::Validation(format!(
                    "Favourite '{}' is not a channel the number sender holds",
                    favourite.channel
                ))
            })?;
            let index = u16::try_from(lists.len())
                .map_err(|_| Error::Validation("the action list table overflows a u16".into()))?;
            lists.push(Piece::new(
                action_list_bytes(&[
                    Instruction::new(NUMBER, channel_number),
                    Instruction::new(SELECT_MAP, NUMBER_SENDER_BAND),
                ])?,
                "slot-10-list",
            ));
            // The scan must not collide with a key the channel Device binds.
            for key in channel.keys.keys() {
                if profile::key_scan(profile, key) == Some(scan) {
                    return Err(Error::Validation(format!(
                        "Favourite key {} raises scan {scan}, which Device '{}' binds to key '{key}'",
                        favourite.key, channel.name
                    )));
                }
            }
            favourites.push((scan, index));
        }
    }
    Ok(favourites)
}

/// The key table: the send events, event 0xFF, one selection per Device Key and Watch TV.
fn key_table(
    layout: &mut Layout,
    plan: &DevicePlan<'_>,
    profile: &ModelProfile,
    activity_index: u16,
) -> Result<usize, Error> {
    let mut key_entries: Vec<TaggedEntry> = send_event_entries();
    key_entries.push(acknowledging_entry());
    for &device in &plan.order {
        let place = usize::from(plan.group_of[device]);
        key_entries.push(TaggedEntry {
            tag: PRESS | profile.device_key_scans[place],
            operand: SELECT_MAP_MASK | u16::from(plan.map_of[device]),
            opcode: SELECT_MAP,
        });
    }
    key_entries.push(TaggedEntry {
        tag: WATCH_TV_TAG,
        operand: activity_index,
        opcode: CALL,
    });
    let key_table = layout.add(Piece::new(
        tagged_list_bytes(&in_compiler_order(&[], &key_entries)?)?,
        "raw-6-key-table",
    ));
    layout.set_key_table(key_table);
    Ok(key_table)
}

/// Each Device's own map, in map order: a press per bound key calling that Command's send list, the
/// routing copies, and the Favourite presses on the channel Device.
fn device_maps(
    layout: &mut Layout,
    model: &Model,
    profile: &ModelProfile,
    plan: &DevicePlan<'_>,
    command_lists: &[Vec<Option<u16>>],
    favourites: &[(u8, u16)],
    channel_device: usize,
) -> Result<Vec<usize>, Error> {
    let mut map_indices = Vec::with_capacity(plan.order.len());
    for &device in &plan.order {
        let mut entries: Vec<MapEntry> = Vec::new();
        bound_key_entries(model, profile, command_lists, device, &mut entries)?;
        routing_entries(model, profile, command_lists, device, &mut entries)?;
        favourite_entries(model, favourites, channel_device, device, &mut entries);
        let ordered = compiler_ordered(entries, &model.devices[device].name)?;
        map_indices.push(layout.add(Piece::new(tagged_list_bytes(&ordered)?, "raw-6-map")));
    }
    Ok(map_indices)
}

/// A press per key the Device binds, calling that Command's send list.
fn bound_key_entries(
    model: &Model,
    profile: &ModelProfile,
    command_lists: &[Vec<Option<u16>>],
    device: usize,
    entries: &mut Vec<MapEntry>,
) -> Result<(), Error> {
    for (key, command) in &model.devices[device].keys {
        let scan = profile::key_scan(profile, key).ok_or_else(|| {
            Error::Validation(format!(
                "Device '{}' binds key '{key}', which profile '{}' has no scan code for",
                model.devices[device].name, profile.id
            ))
        })?;
        let command_index = model.devices[device]
            .commands
            .keys()
            .position(|name| name == command)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "Device '{}' binds key '{key}' to missing Command '{command}'",
                    model.devices[device].name
                ))
            })?;
        entries.push(MapEntry {
            entry: TaggedEntry {
                tag: PRESS | scan,
                operand: command_lists[device][command_index].ok_or_else(|| {
                    Error::Internal(format!(
                        "Device '{}' binds key '{key}' to a Command with no send list",
                        model.devices[device].name
                    ))
                })?,
                opcode: CALL,
            },
            label: format!("key '{key}'"),
        });
    }
    Ok(())
}

/// The routing copies: each global function's keys call the target Device's send list.
fn routing_entries(
    model: &Model,
    profile: &ModelProfile,
    command_lists: &[Vec<Option<u16>>],
    device: usize,
    entries: &mut Vec<MapEntry>,
) -> Result<(), Error> {
    if let Some(routing) = &model.routing {
        for (function, target, keys) in [
            (
                "volume",
                routing.volume.as_deref(),
                crate::model::Routing::VOLUME_KEYS,
            ),
            (
                "channel",
                routing.channel.as_deref(),
                crate::model::Routing::CHANNEL_KEYS,
            ),
        ] {
            let Some(target) = target else { continue };
            let target_device = routing_target(model, function, target)?;
            if target_device == device {
                continue;
            }
            for key in keys {
                entries.push(routing_entry(
                    model,
                    profile,
                    command_lists,
                    function,
                    target,
                    target_device,
                    key,
                )?);
            }
        }
    }
    Ok(())
}

/// The Device a routing function names.
fn routing_target(model: &Model, function: &str, target: &str) -> Result<usize, Error> {
    model
        .devices
        .iter()
        .position(|candidate| candidate.name == target)
        .ok_or_else(|| {
            Error::Validation(format!(
                "routing {function} names missing Device '{target}'"
            ))
        })
}

/// One routing key: the target Device's send list called under the key's scan.
fn routing_entry(
    model: &Model,
    profile: &ModelProfile,
    command_lists: &[Vec<Option<u16>>],
    function: &str,
    target: &str,
    target_device: usize,
    key: &str,
) -> Result<MapEntry, Error> {
    let command = model.devices[target_device].keys.get(key).ok_or_else(|| {
        Error::Validation(format!(
            "routing {function} targets Device '{target}', which does not bind \
             key '{key}'"
        ))
    })?;
    let command_index = model.devices[target_device]
        .commands
        .keys()
        .position(|name| name == command)
        .ok_or_else(|| {
            Error::Validation(format!(
                "routing {function} targets Device '{target}', whose key '{key}' \
                 names missing Command '{command}'"
            ))
        })?;
    let scan = profile::key_scan(profile, key).ok_or_else(|| {
        Error::Validation(format!(
            "routing {function} names key '{key}', which profile '{}' has no scan \
             code for",
            profile.id
        ))
    })?;
    Ok(MapEntry {
        entry: TaggedEntry {
            tag: PRESS | scan,
            operand: command_lists[target_device][command_index].ok_or_else(|| {
                Error::Internal(format!(
                    "routing {function} targets Device '{target}', whose key \
                     '{key}' has no send list"
                ))
            })?,
            opcode: CALL,
        },
        label: format!("key '{key}' inherited from '{target}'"),
    })
}

/// The Favourite presses, bound on the channel Device's own map.
fn favourite_entries(
    model: &Model,
    favourites: &[(u8, u16)],
    channel_device: usize,
    device: usize,
    entries: &mut Vec<MapEntry>,
) {
    if device == channel_device {
        for (favourite, &(scan, list)) in model.favourites.iter().zip(favourites) {
            entries.push(MapEntry {
                entry: TaggedEntry {
                    tag: PRESS | scan,
                    operand: list,
                    opcode: CALL,
                },
                label: format!("Favourite key {}", favourite.key),
            });
        }
    }
}

/// The body, the tables and the slots, then the check that the finished Container parses.
fn assemble(
    layout: &mut Layout,
    model: &Model,
    imports: &[Vec<Option<ImportedCommand>>],
    lists: Vec<Piece>,
    built_at: &OffsetDateTime,
    plan: &DevicePlan<'_>,
    tables: Tables,
) -> Result<Composed, Error> {
    let body = body(layout, lists, built_at)?;
    let tree = layout.add(name_tree());
    let log = layout.add(log_area());
    let infrared = infrared(layout, model, imports, plan)?;

    let start_up = layout.add(start_up_table()?);
    let mut key_map_targets = Vec::with_capacity(tables.map_indices.len() + 1);
    key_map_targets.push(tables.key_table);
    key_map_targets.extend_from_slice(&tables.map_indices);
    let key_map = layout.add(counted_table(1, &key_map_targets, "raw-6-table")?);
    let list_table = layout.add(counted_table(2, &body.list_targets, "raw-7-table")?);
    let timers = layout.add(counted_table(1, &[], "raw-8-table")?);
    let states = layout.add(state_table(&body.state_record_indices)?);

    let (parameter_indices, parameter_table) = parameters(layout)?;
    let (sender_before, senders, digits) = digit_tables(layout, model, &tables.digit_lists)?;

    place_slots(
        layout,
        Slots {
            tree,
            log,
            infrared_before: infrared.before,
            infrared: infrared.table,
            start_up,
            map_indices: tables.map_indices,
            key_map,
            list_table,
            timers,
            states,
            parameter_indices,
            parameter_table,
            sender_before,
            senders,
            digits,
        },
    )?;
    finish(
        layout,
        model,
        plan,
        &infrared.record_indices,
        tables.favourites,
    )
}

/// The body: every action list by index, then the state records.
fn body(layout: &mut Layout, lists: Vec<Piece>, built_at: &OffsetDateTime) -> Result<Body, Error> {
    let list_count = lists.len();
    let mut body = Vec::with_capacity(list_count);
    for piece in lists {
        body.push(layout.add(piece));
    }
    let list_targets = body[..list_count].to_vec();
    let records = state_records(built_at)?;
    let mut state_record_indices = Vec::with_capacity(records.len());
    for piece in records {
        state_record_indices.push(layout.add(piece));
    }
    body.extend_from_slice(&state_record_indices);
    layout.set_body(body);
    Ok(Body {
        list_targets,
        state_record_indices,
    })
}

/// The infrared groups: one piece per group holding its record count and pointers, one two-piece
/// record per Command, then the group table the slot names.
fn infrared(
    layout: &mut Layout,
    model: &Model,
    imports: &[Vec<Option<ImportedCommand>>],
    plan: &DevicePlan<'_>,
) -> Result<Infrared, Error> {
    let mut group_records: Vec<Vec<(usize, usize)>> = vec![Vec::new(); plan.group_count];
    let mut record_indices: Vec<Vec<Option<u8>>> = vec![Vec::new(); model.devices.len()];
    let mut record_pieces = Vec::new();
    for (device, device_imports) in imports.iter().enumerate() {
        let mut record = 0u16;
        for imported in device_imports {
            let Some(imported) = imported else {
                record_indices[device].push(None);
                continue;
            };
            let slot = u8::try_from(record).map_err(|_| {
                Error::Validation(format!(
                    "Device '{}' declares more than 256 sent Commands",
                    model.devices[device].name
                ))
            })?;
            let (data, header, header_offset) = add_record(layout, &imported.record)?;
            group_records[usize::from(plan.group_of[device])].push((data, header_offset));
            record_pieces.push(data);
            record_pieces.push(header);
            record_indices[device].push(Some(slot));
            record = record
                .checked_add(1)
                .ok_or_else(|| Error::Validation("too many sent Commands".into()))?;
        }
    }
    let mut group_indices = Vec::with_capacity(plan.group_count);
    for records in &group_records {
        group_indices.push(layout.add(group_piece(records)?));
    }
    let mut before = group_indices.clone();
    before.extend_from_slice(&record_pieces);
    let table = layout.add(counted_table(1, &group_indices, "raw-4-table")?);
    Ok(Infrared {
        before,
        table,
        record_indices,
    })
}

/// The parameter block: one piece per group, then the table the slot names.
fn parameters(layout: &mut Layout) -> Result<(Vec<usize>, usize), Error> {
    let parameters = parameter_groups()?;
    let mut parameter_indices = Vec::with_capacity(parameters.len());
    for piece in parameters {
        parameter_indices.push(layout.add(piece));
    }
    let parameter_table = layout.add(counted_table(1, &parameter_indices, "raw-10-table")?);
    Ok((parameter_indices, parameter_table))
}

/// The number sender: an empty table, or one record and its three digit tables when Favourites exist.
fn digit_tables(
    layout: &mut Layout,
    model: &Model,
    digit_lists: &[u16; 10],
) -> Result<(Vec<usize>, usize, usize), Error> {
    let mut sender_before = Vec::new();
    let senders = if model.favourites.is_empty() {
        layout.add(number_sender_table(&[])?)
    } else {
        let table_piece = digit_table_bytes(digit_lists);
        let mut tables = Vec::with_capacity(DIGIT_TABLE_COUNT);
        for _ in 0..DIGIT_TABLE_COUNT {
            tables.push(layout.add(Piece::new(table_piece.clone(), "slot-16-digit-table")));
        }
        let record = layout.add(number_sender_record(&tables)?);
        sender_before.extend_from_slice(&tables);
        sender_before.push(record);
        layout.add(number_sender_table(&[record])?)
    };
    let digits = layout.add(digit_entry_table());
    Ok((sender_before, senders, digits))
}

/// Place every slot the composed Container states: the pieces the slot holds, then the table itself.
fn place_slots(layout: &mut Layout, slots: Slots) -> Result<(), Error> {
    layout.place(0, Placement::new(Vec::new(), slots.tree))?;
    layout.place(2, Placement::new(Vec::new(), slots.log))?;
    layout.place(4, Placement::new(slots.infrared_before, slots.infrared))?;
    layout.place(5, Placement::new(Vec::new(), slots.start_up))?;
    layout.place(6, Placement::new(slots.map_indices.clone(), slots.key_map))?;
    layout.place(7, Placement::new(Vec::new(), slots.list_table))?;
    layout.place(8, Placement::new(Vec::new(), slots.timers))?;
    layout.place(9, Placement::new(Vec::new(), slots.states))?;
    layout.place(
        10,
        Placement::new(slots.parameter_indices, slots.parameter_table),
    )?;
    layout.place(11, Placement::new(slots.sender_before, slots.senders))?;
    layout.place(12, Placement::new(Vec::new(), slots.digits))?;
    Ok(())
}

/// Lay the pieces out, check the trailer, and parse the result back.
fn finish(
    layout: &mut Layout,
    model: &Model,
    plan: &DevicePlan<'_>,
    record_indices: &[Vec<Option<u8>>],
    favourites: usize,
) -> Result<Composed, Error> {
    let bytes = layout.lay_out()?;
    if validation_status(&bytes) != ValidationStatus::Ok {
        return Err(Error::Internal(
            "the composed Container does not pass its trailer check".into(),
        ));
    }
    crate::inspect_container::parse(&bytes).map_err(|error| {
        Error::Internal(format!("the composed Container did not parse: {error}"))
    })?;

    Ok(Composed {
        bytes,
        devices: model
            .devices
            .iter()
            .enumerate()
            .map(|(device, _model)| ComposedDevice {
                ir_group: plan.group_of[device],
                map_index: plan.map_of[device],
                record_indices: record_indices[device].clone(),
            })
            .collect(),
        favourites,
    })
}

/// The Watch TV Activity's list: each step's power then input send list called, a wait of the step's
/// delay, and the channel Device's map selected at the end.
fn activity_body(
    model: &Model,
    activity: &Activity,
    command_lists: &[Vec<Option<u16>>],
    group_of: &[u8],
    channel_map: u8,
) -> Result<Vec<Instruction>, Error> {
    let mut body = Vec::new();
    for step in &activity.steps {
        let device = model
            .devices
            .iter()
            .position(|device| device.name == step.device)
            .ok_or_else(|| {
                Error::Validation(format!(
                    "Watch TV Activity names missing Device '{}'",
                    step.device
                ))
            })?;
        for command in [step.power.as_ref(), step.input.as_ref()]
            .into_iter()
            .flatten()
        {
            let index = model.devices[device]
                .commands
                .keys()
                .position(|name| name == command)
                .ok_or_else(|| {
                    Error::Validation(format!(
                        "Watch TV Activity step for Device '{}' names missing Command '{command}'",
                        step.device
                    ))
                })?;
            body.push(Instruction::new(
                CALL,
                command_lists[device][index].ok_or_else(|| {
                    Error::Internal(format!(
                        "Watch TV Activity step for Device '{}' names Command '{command}' with no \
                         send list",
                        step.device
                    ))
                })?,
            ));
        }
        if step.delay_ms > 0 {
            // The clamp bounds this to 1..=255, so the u8 conversion cannot fail.
            let tenths = (step.delay_ms.saturating_add(99) / 100).clamp(1, 255) as u8;
            body.push(Instruction::new(
                DELAY,
                (u16::from(group_of[device]) << 8) | u16::from(tenths),
            ));
        }
    }
    body.push(Instruction::new(
        SELECT_MAP,
        SELECT_MAP_MASK | u16::from(channel_map),
    ));
    Ok(body)
}

/// The action list each digit 0 to 9 calls: the channel Device's send list for its `number_*` key, or
/// the start-up's empty list where the Device binds none.
fn digit_lists(
    model: &Model,
    channel_device: usize,
    command_lists: &[Vec<Option<u16>>],
) -> [u16; 10] {
    let device = &model.devices[channel_device];
    std::array::from_fn(|digit| {
        let key = format!("number_{digit}");
        device
            .keys
            .get(&key)
            .and_then(|command| device.commands.keys().position(|name| name == command))
            .and_then(|index| command_lists[channel_device][index])
            .unwrap_or(START_UP_EMPTY)
    })
}

/// One digit table: ten calls of the lists in `digit_lists`.
fn digit_table_bytes(digit_lists: &[u16; 10]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(DIGIT_TABLE_LENGTH);
    for list in digit_lists {
        bytes.extend_from_slice(&list.to_le_bytes());
        bytes.push(CALL);
    }
    bytes
}

/// The number sender's record: flags, base, minimum, prologue, epilogue and prefix all zero, then the
/// three digit tables' addresses.
fn number_sender_record(tables: &[usize]) -> Result<Piece, Error> {
    if tables.len() != DIGIT_TABLE_COUNT {
        return Err(Error::Internal(
            "the number sender holds three digit tables".into(),
        ));
    }
    let bytes = vec![0u8; NUMBER_SENDER_RECORD_LENGTH];
    let refs = tables
        .iter()
        .enumerate()
        .map(|(index, target)| Reference::Piece {
            at: 14 + 3 * index,
            target: *target,
            offset: 0,
        })
        .collect();
    Ok(Piece {
        bytes,
        refs,
        owner: "slot-16-record",
    })
}

/// Order a map's entries in the compiler's slot order, refusing a tag twice with the two labels that
/// collided.
fn compiler_ordered(entries: Vec<MapEntry>, device: &str) -> Result<Vec<TaggedEntry>, Error> {
    let mut tags: Vec<(u8, &str)> = entries
        .iter()
        .map(|one| (one.entry.tag, one.label.as_str()))
        .collect();
    tags.sort_by_key(|one| one.0);
    for pair in tags.windows(2) {
        if pair[0].0 == pair[1].0 {
            return Err(Error::Validation(format!(
                "Device '{device}' binds {} and {} to the same scan code {}",
                pair[0].1,
                pair[1].1,
                pair[0].0 & 0x3F
            )));
        }
    }
    let plain: Vec<TaggedEntry> = entries.into_iter().map(|one| one.entry).collect();
    in_compiler_order(&[], &plain)
}

/// Add one record as two pieces: its blocks and the first eight header bytes, then its own start, group
/// count and block pointers. Returns the blocks' piece index, the header's piece index and the header's
/// offset inside the first piece.
fn add_record(layout: &mut Layout, record: &IrRecord) -> Result<(usize, usize, usize), Error> {
    let encoded = record.encode_at(FLASH_BASE)?;
    let header_offset = encoded.header_offset;

    let mut block_offsets: Vec<Option<usize>> = Vec::new();
    let mut at = 0usize;
    for group in &record.groups {
        for block in [Some(&group.once), group.held.as_ref(), group.tail.as_ref()] {
            match block {
                Some(words) => {
                    block_offsets.push(Some(at));
                    at = at.checked_add(words.len() * 2 + 2).ok_or_else(|| {
                        Error::Internal("a record's blocks overflow the address space".into())
                    })?;
                }
                None => block_offsets.push(None),
            }
        }
    }
    if at != header_offset {
        return Err(Error::Internal(
            "the encoded record's blocks do not meet its header".into(),
        ));
    }

    let (prefix, rest) = encoded.bytes.split_at(header_offset + 8);
    let data = layout.add(Piece::new(prefix.to_vec(), "raw-4-record"));
    let mut refs = vec![Reference::Piece {
        at: 0,
        target: data,
        offset: header_offset,
    }];
    for (index, offset) in block_offsets.iter().enumerate() {
        if let Some(offset) = offset {
            refs.push(Reference::Piece {
                at: 4 + 3 * index,
                target: data,
                offset: *offset,
            });
        }
    }
    let header = layout.add(Piece {
        bytes: rest.to_vec(),
        refs,
        owner: "raw-4-record",
    });
    Ok((data, header, header_offset))
}

/// One infrared group: a spare byte, a `u16` record count and a pointer per record.
fn group_piece(records: &[(usize, usize)]) -> Result<Piece, Error> {
    let count = u16::try_from(records.len())
        .map_err(|_| Error::Validation("an infrared group holds more than 65535 records".into()))?;
    let mut bytes = vec![0u8; 3 + 3 * records.len()];
    bytes[1..3].copy_from_slice(&count.to_le_bytes());
    let refs = records
        .iter()
        .enumerate()
        .map(|(index, (data, header_offset))| Reference::Piece {
            at: 3 + 3 * index,
            target: *data,
            offset: header_offset + 7,
        })
        .collect();
    Ok(Piece {
        bytes,
        refs,
        owner: "raw-4-group",
    })
}
