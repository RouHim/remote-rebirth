//! `inspect --file <path> [--json] [--print-schema]`.
//!
//! The JSON is versioned by `schema_version` and described by the embedded
//! `schema/inspect-v1.json`, so another tool can validate it.

use std::io::Write;
use std::path::Path;

use crate::error::Error;
use crate::inspect_container::{self, ParsedContainer};

pub const SCHEMA: &str = include_str!("../schema/inspect-v1.json");

pub fn run(
    file: Option<&Path>,
    json: bool,
    print_schema: bool,
    stdout: &mut dyn Write,
) -> Result<(), Error> {
    if print_schema {
        stdout.write_all(SCHEMA.as_bytes())?;
        return Ok(());
    }
    let Some(path) = file else {
        return Err(Error::Usage(
            "inspect requires --file <path> unless --print-schema is given".into(),
        ));
    };
    let span = tracing::info_span!("inspect", file = %path.display());
    let _entered = span.enter();

    let bytes = std::fs::read(path)?;
    let parsed = inspect_container::parse(&bytes)?;
    tracing::info!(
        size = parsed.container.size,
        devices = parsed.devices.len(),
        favourites = parsed.favourites.len(),
        "parsed Container"
    );

    if json {
        let encoded = serde_json::to_vec(&parsed)?;
        stdout.write_all(&encoded)?;
        stdout.write_all(b"\n")?;
    } else {
        write_summary(stdout, path, &parsed)?;
    }
    Ok(())
}

fn write_summary(
    stdout: &mut dyn Write,
    path: &Path,
    parsed: &ParsedContainer,
) -> Result<(), Error> {
    let container = &parsed.container;
    writeln!(stdout, "file: {}", path.display())?;
    writeln!(
        stdout,
        "Architecture 16 Container: {} bytes, base 0x{:06X}, end_addr 0x{:06X}, checksum 0x{:04X}, {} slots",
        container.size, container.base, container.end_addr, container.checksum, container.slots
    )?;
    if let Some(skin) = container.skin {
        writeln!(stdout, "Skin: {skin}")?;
    }
    writeln!(stdout, "SHA-256: {}", container.sha256)?;
    writeln!(stdout, "Devices: {}", parsed.devices.len())?;
    for device in &parsed.devices {
        match &device.device_key {
            Some(key) => writeln!(
                stdout,
                "  Device Key {} (place {}, scan {}, {}) -> map {}, IR group {}, {} IR Records",
                key.label,
                key.place,
                key.scan_code,
                key.event,
                device.map_index,
                device
                    .ir_group
                    .map(|group| group.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                device.record_count
            )?,
            None => writeln!(
                stdout,
                "  map {} -> no Device Key binding, IR group {}, {} IR Records",
                device.map_index,
                device
                    .ir_group
                    .map(|group| group.to_string())
                    .unwrap_or_else(|| "unknown".into()),
                device.record_count
            )?,
        }
    }

    match &parsed.watch_tv_activity {
        Some(activity) => {
            writeln!(
                stdout,
                "Watch TV Activity: action list {}, selects map {}, {} sends, {} delays",
                activity.action_list,
                activity
                    .selected_map
                    .map(|map| map.to_string())
                    .unwrap_or_else(|| "none".into()),
                activity.sends.len(),
                activity.delays.len()
            )?;
        }
        None => writeln!(stdout, "Watch TV Activity: none")?,
    }

    writeln!(stdout, "Favourites: {}", parsed.favourites.len())?;
    for favourite in &parsed.favourites {
        writeln!(
            stdout,
            "  key {} -> channel {} (map {}, action list {})",
            favourite
                .key
                .map(|key| key.to_string())
                .unwrap_or_else(|| "?".into()),
            favourite.channel,
            favourite.map_index,
            favourite.action_list
        )?;
    }

    writeln!(stdout, "IR Records: {}", parsed.ir_records.len())?;
    for record in &parsed.ir_records {
        writeln!(
            stdout,
            "  group {} record {}: class {}, carrier {} ns, once {}, held {}, tail {}",
            record.group,
            record.index,
            record.class,
            record.carrier.period_ns,
            record.once.is_some(),
            record.held.is_some(),
            record.tail.is_some()
        )?;
    }

    if let Some(state) = &parsed.state_variables {
        writeln!(
            stdout,
            "State variables: {} ({} narrow, {} wide)",
            state.count, state.narrow, state.wide
        )?;
    }

    let occupied = parsed
        .raw_slots
        .iter()
        .filter(|slot| slot.offset.is_some())
        .count();
    writeln!(
        stdout,
        "Key table entries: {}; Key maps: {}; Raw slots: {} of {} occupied",
        parsed.key_table.count,
        parsed.key_maps.len(),
        occupied,
        parsed.raw_slots.len()
    )?;
    Ok(())
}
