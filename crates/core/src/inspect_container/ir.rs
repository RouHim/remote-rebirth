use super::*;

pub(super) fn parse_ir_database(
    bytes: &[u8],
    slots: &[Slot; SLOT_COUNT],
) -> Result<(usize, Vec<IrRecord>), Error> {
    let slot = slots[IR_DATABASE_SLOT];
    if slot.null {
        return Ok((0, Vec::new()));
    }
    let count = u8_at(bytes, slot.offset, "infrared group count")? as usize;
    let table_end = slot.offset + 1 + 3 * count;
    if table_end > slot.offset + slot.length {
        return Err(Error::Validation(format!(
            "infrared table at 0x{:06X} declares {count} groups and overruns its section",
            slot.address
        )));
    }

    let mut records = Vec::new();
    for group in 0..count {
        let group_address = u24_at(bytes, slot.offset + 1 + 3 * group, "infrared group address")?;
        if group_address == 0 {
            return Err(Error::Validation(format!("infrared group {group} is NULL")));
        }
        let group_offset = address_offset(bytes, group_address, "infrared group")?;
        let spare = u8_at(bytes, group_offset, "infrared group spare byte")?;
        if spare != 0 {
            return Err(Error::Validation(format!(
                "infrared group {group} has spare byte 0x{spare:02X}, expected zero"
            )));
        }
        let record_count = u16_at(bytes, group_offset + 1, "infrared record count")? as usize;
        let group_end = group_offset + 3 + 3 * record_count;
        if group_end > bytes.len() {
            return Err(Error::Validation(format!(
                "infrared group {group} declares {record_count} records and overruns the container"
            )));
        }
        for index in 0..record_count {
            let pointer = u24_at(
                bytes,
                group_offset + 3 + 3 * index,
                "infrared record pointer",
            )?;
            if pointer == 0 {
                return Err(Error::Validation(format!(
                    "infrared record {index} of group {group} is NULL"
                )));
            }
            let record = parse_ir_record(bytes, group, index, pointer)?;
            records.push(record);
        }
    }
    Ok((count, records))
}

fn parse_ir_record(
    bytes: &[u8],
    group: usize,
    index: usize,
    pointer: u32,
) -> Result<IrRecord, Error> {
    let class_offset = address_offset(bytes, pointer, "infrared record pointer")?;
    let own_start = u24_at(bytes, class_offset + 1, "infrared record own start")?;
    if own_start == 0 {
        return Err(Error::Validation(format!(
            "infrared record {index} of group {group} has a NULL own start"
        )));
    }
    let offset = address_offset(bytes, own_start, "infrared record start")?;
    if own_start.checked_add(7) != Some(pointer) {
        return Err(Error::Validation(format!(
            "infrared record {index} of group {group} is named at 0x{pointer:06X} but starts \
             at 0x{own_start:06X}"
        )));
    }
    let period_ns = u24_at(bytes, offset + 1, "infrared carrier period")?;
    let on_ns = u24_at(bytes, offset + 4, "infrared carrier on time")?;
    let class = u8_at(bytes, offset + 7, "infrared record class")?;
    let stated_start = u24_at(bytes, offset + 8, "infrared record own start")?;
    if stated_start != own_start {
        return Err(Error::Validation(format!(
            "infrared record {index} of group {group} states its own start as \
             0x{stated_start:06X}, not 0x{own_start:06X}"
        )));
    }
    let group_count = u8_at(bytes, offset + 11, "infrared pointer group count")? as usize;
    if group_count == 0 || group_count > 16 {
        return Err(Error::Validation(format!(
            "infrared record {index} of group {group} declares {group_count} pointer groups"
        )));
    }
    let header_end = offset + 12 + 9 * group_count;
    if header_end > bytes.len() {
        return Err(Error::Validation(format!(
            "infrared record {index} of group {group} overruns the container"
        )));
    }

    let mut pointer_groups = Vec::with_capacity(group_count);
    for pointer_group in 0..group_count {
        let at = offset + 12 + 9 * pointer_group;
        let mut blocks = [None, None, None];
        for (slot, block) in blocks.iter_mut().enumerate() {
            let address = u24_at(bytes, at + 3 * slot, "infrared block pointer")?;
            if address == 0 {
                continue;
            }
            let block_offset = address_offset(bytes, address, "infrared block")?;
            if class == 1 {
                *block = Some(parse_ir_block(bytes, address, block_offset)?);
            }
        }
        pointer_groups.push(IrPointerGroup {
            once: blocks[0].clone(),
            held: blocks[1].clone(),
            tail: blocks[2].clone(),
        });
    }

    let first = pointer_groups.first().cloned().unwrap_or(IrPointerGroup {
        once: None,
        held: None,
        tail: None,
    });
    let toggle = pointer_groups.get(1).cloned();
    let extra_groups = pointer_groups.into_iter().skip(2).collect();

    Ok(IrRecord {
        group,
        index,
        address: own_start,
        offset,
        class,
        carrier: IrCarrier {
            period_ns,
            on_ns,
            hertz: (period_ns != 0).then_some(1_000_000_000 / period_ns),
        },
        once: first.once,
        held: first.held,
        tail: first.tail,
        toggle,
        extra_groups,
    })
}

fn parse_ir_block(bytes: &[u8], address: u32, offset: usize) -> Result<IrBlock, Error> {
    let mut at = offset;
    let mut words = 0usize;
    let mut duration_us = 0u64;
    loop {
        if at + 2 > bytes.len() {
            return Err(Error::Validation(format!(
                "infrared block at 0x{address:06X} is not terminated inside the container"
            )));
        }
        let word = u16_at(bytes, at, "infrared block word")?;
        words += 1;
        if word == 0 {
            break;
        }
        duration_us += u64::from(word & 0x7FFF);
        at += 2;
    }
    Ok(IrBlock {
        address,
        offset,
        bytes: words * 2,
        words,
        duration_us,
    })
}
