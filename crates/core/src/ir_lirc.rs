//! LIRC configuration files: `raw_codes` captures and `SPACE_ENC`/`RC5` code blocks.

use crate::ir_encode::raw_record;
use crate::ir_encode::{
    carrier_period, jvc_import, kaseikyo_import, nec_import, rc5_import, samsung_import,
    sirc_import,
};
use crate::ir_types::{Context, IrError, IrSource, ParsedIr, Pulse};

// ---------------------------------------------------------------------------
// LIRC
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LircRemote {
    name: String,
    flags: Vec<String>,
    header: Option<(u32, u32)>,
    one: Option<(u32, u32)>,
    zero: Option<(u32, u32)>,
    bits: Option<u32>,
    frequency: Option<u32>,
    pre_data_bits: Option<u32>,
    post_data_bits: Option<u32>,
    codes: Vec<(String, String)>,
    raw_codes: Vec<(String, Vec<u32>)>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LircSection {
    Top,
    Codes,
    RawCodes,
}

pub(crate) fn parse_lirc(
    text: &str,
    source: &IrSource,
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    let remotes = parse_lirc_remotes(text, context)?;
    if remotes.is_empty() {
        return Err(context.malformed("the file holds no LIRC remote"));
    }
    let wanted = source
        .remote
        .as_deref()
        .ok_or_else(|| context.error("an LIRC source needs a `remote` selector"))?;
    let remote = remotes
        .iter()
        .find(|remote| remote.name.eq_ignore_ascii_case(wanted))
        .ok_or_else(|| {
            let names: Vec<&str> = remotes.iter().map(|remote| remote.name.as_str()).collect();
            context.error(format!(
                "remote \"{wanted}\" not found (have: {})",
                names.join(", ")
            ))
        })?;
    let button = source
        .button
        .as_deref()
        .ok_or_else(|| context.error("an LIRC source needs a `button` selector"))?;

    if let Some((_, timings)) = remote
        .raw_codes
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(button))
    {
        return lirc_raw_import(remote, timings, context);
    }
    if let Some((_, value)) = remote
        .codes
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(button))
    {
        return lirc_code_import(remote, value, context, sends);
    }
    Err(context.error(format!(
        "button \"{button}\" not found in remote \"{wanted}\""
    )))
}

fn parse_lirc_remotes(text: &str, context: &Context) -> Result<Vec<LircRemote>, IrError> {
    let mut remotes = Vec::new();
    let mut current: Option<LircRemote> = None;
    let mut section = LircSection::Top;
    for raw_line in text.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if lower == "begin remote" {
            if current.is_some() {
                return Err(context.malformed("LIRC remotes are nested"));
            }
            current = Some(LircRemote::default());
            section = LircSection::Top;
            continue;
        }
        if lower == "end remote" {
            if let Some(remote) = current.take() {
                remotes.push(remote);
            }
            section = LircSection::Top;
            continue;
        }
        let Some(remote) = current.as_mut() else {
            continue;
        };
        if lower == "begin codes" {
            section = LircSection::Codes;
            continue;
        }
        if lower == "end codes" {
            section = LircSection::Top;
            continue;
        }
        if lower == "begin raw_codes" {
            section = LircSection::RawCodes;
            continue;
        }
        if lower == "end raw_codes" {
            section = LircSection::Top;
            continue;
        }
        match section {
            LircSection::Top => parse_lirc_top_line(remote, line, context)?,
            LircSection::Codes => {
                let mut tokens = line.split_whitespace();
                let Some(name) = tokens.next() else {
                    continue;
                };
                let value: Vec<&str> = tokens.collect();
                if value.is_empty() {
                    return Err(context.malformed(format!("code \"{name}\" has no value")));
                }
                remote.codes.push((name.to_string(), value.join("")));
            }
            LircSection::RawCodes => {
                if let Some(name) = line.strip_prefix("name ") {
                    remote.raw_codes.push((name.trim().to_string(), Vec::new()));
                    continue;
                }
                let Some((_, timings)) = remote.raw_codes.last_mut() else {
                    return Err(context.malformed("a raw code timing appears before its name"));
                };
                for token in line.split_whitespace() {
                    let timing = token
                        .parse::<u32>()
                        .map_err(|_| context.malformed(format!("\"{token}\" is not a timing")))?;
                    timings.push(timing);
                }
            }
        }
    }
    if current.is_some() {
        return Err(context.malformed("the LIRC remote is missing `end remote`"));
    }
    Ok(remotes)
}

fn parse_lirc_top_line(
    remote: &mut LircRemote,
    line: &str,
    context: &Context,
) -> Result<(), IrError> {
    let mut tokens = line.split_whitespace();
    let Some(key) = tokens.next() else {
        return Ok(());
    };
    let key = key.to_ascii_lowercase();
    let rest: Vec<&str> = tokens.collect();
    match key.as_str() {
        "name" => remote.name = rest.join(" "),
        "flags" => {
            remote.flags = rest
                .iter()
                .flat_map(|token| token.split(['|', ',']))
                .filter(|token| !token.is_empty())
                .map(|token| token.to_ascii_uppercase())
                .collect();
        }
        "header" => remote.header = Some(parse_pair(&rest, context)?),
        "one" => remote.one = Some(parse_pair(&rest, context)?),
        "zero" => remote.zero = Some(parse_pair(&rest, context)?),
        "bits" => remote.bits = Some(parse_u32(&rest, context)?),
        "frequency" => remote.frequency = Some(parse_u32(&rest, context)?),
        "pre_data_bits" => remote.pre_data_bits = Some(parse_u32(&rest, context)?),
        "post_data_bits" => remote.post_data_bits = Some(parse_u32(&rest, context)?),
        _ => {}
    }
    Ok(())
}

fn parse_pair(values: &[&str], context: &Context) -> Result<(u32, u32), IrError> {
    if values.len() < 2 {
        return Err(context.malformed("a timing pair needs two values"));
    }
    let first = values[0]
        .parse::<u32>()
        .map_err(|_| context.malformed(format!("\"{}\" is not a timing", values[0])))?;
    let second = values[1]
        .parse::<u32>()
        .map_err(|_| context.malformed(format!("\"{}\" is not a timing", values[1])))?;
    Ok((first, second))
}

fn parse_u32(values: &[&str], context: &Context) -> Result<u32, IrError> {
    let value = values
        .first()
        .ok_or_else(|| context.malformed("a numeric setting has no value"))?;
    value
        .parse::<u32>()
        .map_err(|_| context.malformed(format!("\"{value}\" is not a number")))
}

fn lirc_raw_import(
    remote: &LircRemote,
    timings: &[u32],
    context: &Context,
) -> Result<ParsedIr, IrError> {
    if timings.is_empty() {
        return Err(context.malformed("the raw code holds no timings"));
    }
    if timings.contains(&0) {
        return Err(context.malformed("the raw code holds a zero-length timing"));
    }
    let frequency = remote.frequency.unwrap_or(38_000);
    let period_ns = carrier_period(frequency).ok_or_else(|| {
        context.malformed(format!("{frequency} Hz is not a usable carrier frequency"))
    })?;
    let pulses = alternating_pulses(timings);
    Ok(ParsedIr {
        record: raw_record(period_ns, &pulses, &[])?,
        protocol: "raw".into(),
        sends: 1,
    })
}

fn lirc_code_import(
    remote: &LircRemote,
    value: &str,
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    const UNSUPPORTED_FLAGS: &[&str] = &[
        "RC6",
        "RCMM",
        "BIPHASE",
        "CONST_LENGTH",
        "NO_HEAD_REP",
        "NO_FOOT_REP",
        "REPEAT_HEADER",
    ];
    for flag in &remote.flags {
        if UNSUPPORTED_FLAGS.contains(&flag.as_str()) {
            return Err(IrError::UnsupportedLircFlag {
                file: context.source.clone(),
                selector: context.selector.clone(),
                flag: flag.clone(),
            });
        }
    }
    if remote.pre_data_bits.unwrap_or(0) > 0 || remote.post_data_bits.unwrap_or(0) > 0 {
        return Err(context.error("LIRC pre_data and post_data are not supported"));
    }

    let code = parse_lirc_value(value, context)?;
    if remote.flags.iter().any(|flag| flag == "RC5") {
        let code = code & 0x0FFF;
        let address = ((code >> 6) & 0x1F) as u8;
        let command = (code & 0x3F) as u8;
        return Ok(rc5_import(address, command, "RC5", sends.unwrap_or(1)));
    }
    if !remote.flags.iter().any(|flag| flag == "SPACE_ENC") {
        return Err(IrError::UnsupportedLircProtocol {
            file: context.source.clone(),
            selector: context.selector.clone(),
            protocol: if remote.flags.is_empty() {
                "(no flags)".into()
            } else {
                remote.flags.join("|")
            },
        });
    }

    let header = remote
        .header
        .ok_or_else(|| context.malformed("a SPACE_ENC remote needs a header"))?;
    let one = remote
        .one
        .ok_or_else(|| context.malformed("a SPACE_ENC remote needs a one timing"))?;
    let zero = remote
        .zero
        .ok_or_else(|| context.malformed("a SPACE_ENC remote needs a zero timing"))?;
    let bits = remote
        .bits
        .ok_or_else(|| context.malformed("a SPACE_ENC remote needs its bit count"))?;

    if (bits == 12 || bits == 15 || bits == 20)
        && timing_close(header, (2400, 600))
        && timing_close(one, (1200, 600))
        && timing_close(zero, (600, 600))
    {
        let width = bits - 7;
        let command = (code & 0x7F) as u8;
        let address = ((code >> 7) & ((1u64 << width) - 1)) as u16;
        return sirc_import(bits as u8, address, command, sends.unwrap_or(3));
    }
    if timing_close(header, (9000, 4500)) {
        if bits != 32 {
            return Err(context.error("NEC timing declared with a bit count other than 32"));
        }
        return Ok(nec_import(
            (code & 0xFF) as u8,
            ((code >> 16) & 0xFF) as u8,
            sends.unwrap_or(3),
        ));
    }
    if timing_close(header, (4500, 4500)) {
        if bits != 32 {
            return Err(context.error("Samsung32 timing declared with a bit count other than 32"));
        }
        return Ok(samsung_import(
            (code & 0xFF) as u8,
            ((code >> 16) & 0xFF) as u8,
            sends.unwrap_or(3),
        ));
    }
    if timing_close(header, (8400, 4200)) {
        if bits != 16 {
            return Err(context.error("JVC timing declared with a bit count other than 16"));
        }
        return Ok(jvc_import(
            (code & 0xFF) as u8,
            ((code >> 8) & 0xFF) as u8,
            sends.unwrap_or(3),
        ));
    }
    if timing_close(header, (3456, 1728)) {
        if bits != 48 {
            return Err(context.error("Kaseikyo timing declared with a bit count other than 48"));
        }
        let mut payload = [0u8; 6];
        for (index, byte) in payload.iter_mut().enumerate() {
            *byte = ((code >> (index * 8)) & 0xFF) as u8;
        }
        return Ok(kaseikyo_import(payload, sends.unwrap_or(1)));
    }
    Err(context.error(format!(
        "unsupported SPACE_ENC timing profile: header {} {}",
        header.0, header.1
    )))
}

fn timing_close(actual: (u32, u32), expected: (u32, u32)) -> bool {
    fn close(actual: u32, expected: u32) -> bool {
        let slack = (expected / 10).max(200);
        actual.abs_diff(expected) <= slack
    }
    close(actual.0, expected.0) && close(actual.1, expected.1)
}

fn parse_lirc_value(value: &str, context: &Context) -> Result<u64, IrError> {
    let digits: String = value
        .split_whitespace()
        .map(|token| token.trim_start_matches("0x").trim_start_matches("0X"))
        .collect();
    if digits.is_empty() || digits.len() > 16 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(context.malformed(format!("\"{value}\" is not a LIRC code value")));
    }
    u64::from_str_radix(&digits, 16)
        .map_err(|_| context.malformed(format!("\"{value}\" is not a LIRC code value")))
}

pub(crate) fn alternating_pulses(timings: &[u32]) -> Vec<Pulse> {
    timings
        .iter()
        .enumerate()
        .map(|(index, micros)| {
            if index % 2 == 0 {
                Pulse::mark(*micros)
            } else {
                Pulse::space(*micros)
            }
        })
        .collect()
}
