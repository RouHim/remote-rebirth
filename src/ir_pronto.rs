//! Pronto formats: raw captures, RC5 and NEC shorthand.

use crate::ir_encode::raw_record;
use crate::ir_encode::{nec_import, rc5_import};
use crate::ir_types::{Context, IrError, ParsedIr, Pulse};

/// Pronto's frequency-divider constant, in hertz.
const PRONTO_CLOCK_HZ: u64 = 4_145_146;

// ---------------------------------------------------------------------------
// Pronto
// ---------------------------------------------------------------------------

pub(crate) fn parse_pronto(
    text: &str,
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    let words = parse_pronto_words(text, context)?;
    match words[0] {
        0x0000 | 0x0100 => parse_pronto_raw(&words, context),
        0x5000 => parse_pronto_rc5(&words, context, sends),
        0x9000 => parse_pronto_nec(&words, context, sends),
        word => Err(IrError::UnsupportedProntoFormat {
            file: context.source.clone(),
            selector: context.selector.clone(),
            word,
        }),
    }
}

fn parse_pronto_words(text: &str, context: &Context) -> Result<Vec<u16>, IrError> {
    let mut words = Vec::new();
    for token in text.split_whitespace() {
        let token = token.trim_matches(',');
        if token.is_empty() {
            continue;
        }
        if token.len() > 4 || !token.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(context.malformed(format!("\"{token}\" is not a four-digit Pronto word")));
        }
        let word = u16::from_str_radix(token, 16)
            .map_err(|_| context.malformed(format!("\"{token}\" is not a Pronto word")))?;
        words.push(word);
    }
    if words.is_empty() {
        return Err(context.malformed("the Pronto hex is empty"));
    }
    Ok(words)
}

fn parse_pronto_raw(words: &[u16], context: &Context) -> Result<ParsedIr, IrError> {
    if words.len() < 4 {
        return Err(context.malformed("the raw Pronto header is truncated"));
    }
    let divider = u32::from(words[1]);
    if divider == 0 {
        return Err(context.malformed("the Pronto frequency divider is zero"));
    }
    let once_pairs = usize::from(words[2]);
    let repeat_pairs = usize::from(words[3]);
    let expected = 4 + 2 * (once_pairs + repeat_pairs);
    if words.len() < expected {
        return Err(context.malformed(format!(
            "the raw Pronto code declares {expected} words but holds {}",
            words.len()
        )));
    }
    if once_pairs + repeat_pairs == 0 {
        return Err(context.malformed("the raw Pronto code holds no burst pairs"));
    }

    let mut pairs = words[4..expected].as_chunks::<2>().0.iter();
    let mut once = Vec::with_capacity(once_pairs * 2);
    let mut repeat = Vec::with_capacity(repeat_pairs * 2);
    for index in 0..once_pairs + repeat_pairs {
        let pair = pairs.next().ok_or_else(|| {
            context.malformed("the raw Pronto code ends in the middle of a burst pair")
        })?;
        let mark = pronto_micros(pair[0], divider);
        let space = pronto_micros(pair[1], divider);
        if mark == 0 || space == 0 {
            return Err(context.malformed("the raw Pronto code holds a zero-length burst"));
        }
        let target = if index < once_pairs {
            &mut once
        } else {
            &mut repeat
        };
        target.push(Pulse::mark(mark));
        target.push(Pulse::space(space));
    }

    let (once_pulses, held_pulses) = if once.is_empty() {
        (std::mem::take(&mut repeat), Vec::new())
    } else {
        (once, repeat)
    };
    let period_ns =
        u32::try_from((u64::from(divider) * 1_000_000_000 + PRONTO_CLOCK_HZ / 2) / PRONTO_CLOCK_HZ)
            .map_err(|_| context.malformed("the Pronto carrier period does not fit"))?;
    let record = raw_record(period_ns, &once_pulses, &held_pulses);
    Ok(ParsedIr {
        record: record?,
        protocol: "raw".into(),
        sends: 1,
    })
}

fn pronto_micros(count: u16, divider: u32) -> u32 {
    let product = u64::from(count) * u64::from(divider) * 1_000_000;
    u32::try_from((product + PRONTO_CLOCK_HZ / 2) / PRONTO_CLOCK_HZ).unwrap_or(u32::MAX)
}

fn parse_pronto_rc5(
    words: &[u16],
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    if words.len() < 6 {
        return Err(context.malformed("the RC5 Pronto code needs device and command words"));
    }
    let address = words[4];
    let command = words[5];
    if address > 0x1F || command > 0x7F {
        return Err(context.malformed(format!(
            "the RC5 Pronto code states device 0x{address:04X} command 0x{command:04X}"
        )));
    }
    Ok(rc5_import(
        address as u8,
        command as u8,
        "RC5",
        sends.unwrap_or(1),
    ))
}

fn parse_pronto_nec(
    words: &[u16],
    context: &Context,
    sends: Option<u8>,
) -> Result<ParsedIr, IrError> {
    if words.len() < 8 {
        return Err(context.malformed("the NEC Pronto code needs four code words"));
    }
    let value = |index: usize| u32::from(words[index]);
    if value(4) > 0xFF || value(6) > 0xFF {
        return Err(context.malformed("the NEC Pronto code words must be bytes"));
    }
    if (value(4) ^ value(5)) & 0xFF != 0xFF || (value(6) ^ value(7)) & 0xFF != 0xFF {
        return Err(context.malformed("the NEC Pronto code words are not each other's complements"));
    }
    Ok(nec_import(
        value(4) as u8,
        value(6) as u8,
        sends.unwrap_or(3),
    ))
}
