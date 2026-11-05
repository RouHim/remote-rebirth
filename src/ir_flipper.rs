//! Flipper-IRDB files: `raw` captures and parsed protocol entries.

use crate::ir_encode::raw_record;
use crate::ir_encode::{
    carrier_period, jvc_import, kaseikyo_import, nec_import, nec_import_extended, rc5_import,
    samsung_import, sirc_import,
};
use crate::ir_lirc::alternating_pulses;
use crate::ir_types::{Context, IrError, IrSource, ParsedIr};

// ---------------------------------------------------------------------------
// Flipper
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FlipperEntry {
    name: String,
    kind: String,
    protocol: Option<String>,
    address: Option<u32>,
    command: Option<u32>,
    frequency: Option<u32>,
    data: Vec<u32>,
}

pub(crate) fn parse_flipper(
    text: &str,
    source: &IrSource,
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    let wanted = source
        .command
        .as_deref()
        .ok_or_else(|| context.error("a Flipper source needs a `command` selector"))?;
    let entries = parse_flipper_entries(text, context)?;
    let entry = entries
        .iter()
        .find(|entry| entry.name.eq_ignore_ascii_case(wanted))
        .ok_or_else(|| {
            let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
            context.error(format!(
                "command \"{wanted}\" not found (have: {})",
                names.join(", ")
            ))
        })?;
    match entry.kind.as_str() {
        "raw" => flipper_raw_import(entry, context),
        "parsed" => flipper_parsed_import(entry, context, sends),
        other => Err(context.malformed(format!("unknown Flipper signal type \"{other}\""))),
    }
}

fn parse_flipper_entries(text: &str, context: &Context) -> Result<Vec<FlipperEntry>, IrError> {
    let mut entries: Vec<FlipperEntry> = Vec::new();
    for raw_line in text.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let key = key.trim().to_ascii_lowercase();
        let value = value.trim();
        if key == "name" {
            entries.push(FlipperEntry {
                name: value.to_string(),
                ..FlipperEntry::default()
            });
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            continue;
        };
        match key.as_str() {
            "type" => entry.kind = value.to_ascii_lowercase(),
            "protocol" => entry.protocol = Some(value.to_string()),
            "address" => entry.address = Some(parse_flipper_word(value, context)?),
            "command" => entry.command = Some(parse_flipper_word(value, context)?),
            "frequency" => {
                entry.frequency =
                    Some(value.parse::<u32>().map_err(|_| {
                        context.malformed(format!("\"{value}\" is not a frequency"))
                    })?);
            }
            "data" => {
                entry.data = value
                    .split_whitespace()
                    .map(|token| {
                        token.parse::<u32>().map_err(|_| {
                            context.malformed(format!("\"{token}\" is not a raw timing"))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
            }
            _ => {}
        }
    }
    Ok(entries)
}

fn parse_flipper_word(value: &str, context: &Context) -> Result<u32, IrError> {
    let bytes: Vec<&str> = value.split_whitespace().collect();
    if bytes.len() != 4 {
        return Err(context.malformed(format!("\"{value}\" is not a four-byte word")));
    }
    let mut parsed = [0u8; 4];
    for (index, byte) in bytes.iter().enumerate() {
        parsed[index] = u8::from_str_radix(byte, 16)
            .map_err(|_| context.malformed(format!("\"{byte}\" is not a byte")))?;
    }
    Ok(u32::from_le_bytes(parsed))
}

fn flipper_raw_import(entry: &FlipperEntry, context: &Context) -> Result<ParsedIr, IrError> {
    if entry.data.is_empty() {
        return Err(context.malformed("the raw Flipper signal holds no timings"));
    }
    if entry.data.contains(&0) {
        return Err(context.malformed("the raw Flipper signal holds a zero-length timing"));
    }
    let frequency = entry.frequency.unwrap_or(38_000);
    let period_ns = carrier_period(frequency).ok_or_else(|| {
        context.malformed(format!("{frequency} Hz is not a usable carrier frequency"))
    })?;
    let pulses = alternating_pulses(&entry.data);
    Ok(ParsedIr {
        record: raw_record(period_ns, &pulses, &[])?,
        protocol: "raw".into(),
        sends: 1,
    })
}

fn flipper_parsed_import(
    entry: &FlipperEntry,
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    let protocol = entry
        .protocol
        .as_deref()
        .ok_or_else(|| context.malformed("the parsed Flipper signal has no protocol"))?;
    let address = entry
        .address
        .ok_or_else(|| context.malformed("the parsed Flipper signal has no address"))?;
    let command = entry
        .command
        .ok_or_else(|| context.malformed("the parsed Flipper signal has no command"))?;
    let unsupported = |protocol: &str| IrError::UnsupportedFlipperProtocol {
        file: context.source.clone(),
        selector: context.selector.clone(),
        protocol: protocol.to_string(),
    };
    match protocol.to_ascii_uppercase().as_str() {
        "NEC" => {
            let address = u8::try_from(address & 0xFF)
                .map_err(|_| context.malformed("the NEC address does not fit a byte"))?;
            let command = u8::try_from(command & 0xFF)
                .map_err(|_| context.malformed("the NEC command does not fit a byte"))?;
            Ok(nec_import(address, command, sends.unwrap_or(3)))
        }
        "NECEXT" => Ok(nec_import_extended(
            (address & 0xFFFF) as u16,
            (command & 0xFFFF) as u16,
            sends.unwrap_or(3),
        )),
        "SAMSUNG32" => {
            let address = u8::try_from(address & 0xFF)
                .map_err(|_| context.malformed("the Samsung32 address does not fit a byte"))?;
            let command = u8::try_from(command & 0xFF)
                .map_err(|_| context.malformed("the Samsung32 command does not fit a byte"))?;
            Ok(samsung_import(address, command, sends.unwrap_or(3)))
        }
        "JVC" => {
            let address = u8::try_from(address & 0xFF)
                .map_err(|_| context.malformed("the JVC address does not fit a byte"))?;
            let command = u8::try_from(command & 0xFF)
                .map_err(|_| context.malformed("the JVC command does not fit a byte"))?;
            Ok(jvc_import(address, command, sends.unwrap_or(3)))
        }
        "RC5" => {
            let address = u8::try_from(address & 0x1F)
                .map_err(|_| context.malformed("the RC5 address does not fit five bits"))?;
            let command = u8::try_from(command & 0x3F)
                .map_err(|_| context.malformed("the RC5 command does not fit six bits"))?;
            Ok(rc5_import(address, command, "RC5", sends.unwrap_or(1)))
        }
        "RC5X" => {
            let address = u8::try_from(address & 0x1F)
                .map_err(|_| context.malformed("the RC5X address does not fit five bits"))?;
            let command = u8::try_from(command & 0x7F)
                .map_err(|_| context.malformed("the RC5X command does not fit seven bits"))?;
            Ok(rc5_import(address, command, "RC5X", sends.unwrap_or(1)))
        }
        "SIRC" => sirc_import(
            12,
            (address & 0x1F) as u16,
            (command & 0x7F) as u8,
            sends.unwrap_or(3),
        ),
        "SIRC15" => sirc_import(
            15,
            (address & 0xFF) as u16,
            (command & 0x7F) as u8,
            sends.unwrap_or(3),
        ),
        "SIRC20" => sirc_import(
            20,
            (address & 0x1FFF) as u16,
            (command & 0x7F) as u8,
            sends.unwrap_or(3),
        ),
        "KASEIKYO" => Ok(kaseikyo_import(
            kaseikyo_flipper_payload(address, command),
            sends.unwrap_or(1),
        )),
        _ => Err(unsupported(protocol)),
    }
}

/// Rebuild the six Kaseikyo payload bytes from Flipper's packed address and
/// command fields.
fn kaseikyo_flipper_payload(address: u32, command: u32) -> [u8; 6] {
    let vendor = ((address >> 8) & 0xFFFF) as u16;
    let genre1 = ((address >> 4) & 0x0F) as u8;
    let genre2 = (address & 0x0F) as u8;
    let id = ((address >> 24) & 0xFF) as u8;
    let data = (command & 0x03FF) as u16;
    let vendor_lo = (vendor & 0xFF) as u8;
    let vendor_hi = (vendor >> 8) as u8;
    let parity = (vendor_lo ^ vendor_hi) & 0x0F;
    let byte2 = (genre1 << 4) | parity;
    let byte3 = ((data & 0x0F) as u8) << 4 | genre2;
    let byte4 = (((data >> 4) & 0x3F) as u8) | (id << 6);
    let byte5 = byte2 ^ byte3 ^ byte4;
    [vendor_lo, vendor_hi, byte2, byte3, byte4, byte5]
}
