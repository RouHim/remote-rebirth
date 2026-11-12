use super::*;

pub(super) fn derive_devices(
    key_maps: &[KeyMap],
    ir_records: &[IrRecord],
    profile: &ModelProfile,
) -> Vec<Device> {
    let Some(first) = key_maps.first() else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for map in key_maps.iter().skip(1) {
        let binding = first.entries.iter().find_map(|entry| {
            let place = profile
                .device_key_scans
                .iter()
                .position(|scan| *scan == entry.scan_code)?;
            if entry.map_index? as usize != map.index || entry.event == "none" {
                return None;
            }
            Some((place, entry))
        });
        let (device_key, ir_group) = match binding {
            Some((place, entry)) => {
                let group = if entry.event == "held" {
                    4 + place as u8
                } else {
                    place as u8
                };
                (
                    Some(DeviceKey {
                        place: place as u8,
                        label: profile
                            .device_key_labels
                            .get(place)
                            .copied()
                            .unwrap_or("Device Key"),
                        scan_code: entry.scan_code,
                        event: entry.event,
                    }),
                    Some(group),
                )
            }
            None => (None, None),
        };
        let record_count = ir_group
            .map(|group| {
                ir_records
                    .iter()
                    .filter(|record| record.group == group as usize)
                    .count()
            })
            .unwrap_or(0);
        devices.push(Device {
            map_index: map.index,
            device_key,
            ir_group,
            record_count,
        });
    }
    devices
}

pub(super) fn derive_watch_tv(
    key_table: &KeyTable,
    key_maps: &[KeyMap],
    action_lists: &[Vec<Instruction>],
) -> Result<Option<WatchTvActivity>, Error> {
    let entries = key_maps
        .first()
        .map(|map| map.entries.as_slice())
        .unwrap_or(key_table.entries.as_slice());
    let Some(entry) = entries
        .iter()
        .find(|entry| matches!(entry.tag, 0x06 | 0x86) && entry.opcode == 0x7F)
    else {
        return Ok(None);
    };
    let action_list = entry.operand;
    let list = action_lists.get(action_list as usize).ok_or_else(|| {
        Error::Validation(format!(
            "Watch TV Activity calls action list {action_list}, which the action list table does not hold"
        ))
    })?;

    let mut walk = ActivityWalk::new(action_lists);
    walk.collect(action_list, 0);
    let ActivityWalk {
        sends,
        delays,
        selected_map,
        ..
    } = walk;

    let instructions = list
        .iter()
        .map(|instruction| ActivityInstruction {
            opcode: instruction.opcode,
            operand: instruction.operand,
            action_list: (instruction.opcode == 0x7F).then_some(instruction.operand),
            send: send_of(instruction),
            delay: delay_of(instruction),
            select_map: select_map_of(instruction),
            number: (instruction.opcode == 0x7A).then_some(instruction.operand),
        })
        .collect();

    Ok(Some(WatchTvActivity {
        action_list,
        selected_map,
        instructions,
        sends,
        delays,
    }))
}

/// What one Action List walk collects.
struct ActivityWalk<'a> {
    action_lists: &'a [Vec<Instruction>],
    visited: Vec<bool>,
    sends: Vec<IrReference>,
    delays: Vec<IrDelay>,
    selected_map: Option<u8>,
}

impl<'a> ActivityWalk<'a> {
    fn new(action_lists: &'a [Vec<Instruction>]) -> Self {
        Self {
            action_lists,
            visited: vec![false; action_lists.len()],
            sends: Vec::new(),
            delays: Vec::new(),
            selected_map: None,
        }
    }

    fn collect(&mut self, action_list: u16, depth: usize) {
        if depth > 16
            || self
                .visited
                .get(action_list as usize)
                .copied()
                .unwrap_or(true)
        {
            return;
        }
        self.visited[action_list as usize] = true;
        let Some(list) = self.action_lists.get(action_list as usize) else {
            return;
        };
        for instruction in list {
            if let Some(send) = send_of(instruction) {
                self.sends.push(send);
            }
            if let Some(delay) = delay_of(instruction) {
                self.delays.push(delay);
            }
            if let Some(map) = select_map_of(instruction) {
                self.selected_map = Some(map);
            }
            if instruction.opcode == 0x7F {
                self.collect(instruction.operand, depth + 1);
            }
        }
        self.visited[action_list as usize] = false;
    }
}

fn send_of(instruction: &Instruction) -> Option<IrReference> {
    (instruction.opcode == 0x7D).then_some(IrReference {
        group: (instruction.operand >> 8) as u8,
        record: (instruction.operand & 0xFF) as u8,
    })
}

fn delay_of(instruction: &Instruction) -> Option<IrDelay> {
    (instruction.opcode == 0x7C).then_some(IrDelay {
        group: (instruction.operand >> 8) as u8,
        tenths: (instruction.operand & 0xFF) as u8,
    })
}

fn select_map_of(instruction: &Instruction) -> Option<u8> {
    (instruction.opcode == 0x1F && instruction.operand >> 8 == 0xFF)
        .then_some((instruction.operand & 0xFF) as u8)
}

pub(super) fn derive_favourites(
    key_maps: &[KeyMap],
    action_lists: &[Vec<Instruction>],
    profile: &ModelProfile,
    skin: u8,
) -> Vec<Favourite> {
    let mut favourites = Vec::new();
    for map in key_maps {
        for entry in &map.entries {
            if entry.opcode != 0x7F {
                continue;
            }
            let Some(list) = action_lists.get(entry.operand as usize) else {
                continue;
            };
            let Some(channel) = favourite_channel(list) else {
                continue;
            };
            favourites.push(Favourite {
                key: profile::favourite_key(profile, skin, entry.scan_code),
                scan_code: entry.scan_code,
                map_index: map.index,
                action_list: entry.operand,
                channel: channel.to_string(),
            });
        }
    }
    favourites
}

fn favourite_channel(list: &[Instruction]) -> Option<u16> {
    let mut number = None;
    for instruction in list {
        if instruction.opcode == 0x7A {
            number = Some(instruction.operand);
        }
        if instruction.opcode == 0x1F && instruction.operand >> 8 == 0xF3 {
            return number;
        }
    }
    None
}
