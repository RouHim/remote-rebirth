//! Record construction and the six protocol encoders.

use crate::ir_record::pack_block;
use crate::ir_types::{
    CLASS_STREAM, IrError, IrPointerGroup, IrRecord, LEAD_IN_US, ParsedIr, ProtocolFrame, Pulse,
};

// ---------------------------------------------------------------------------
// Record construction and protocol encoders
// ---------------------------------------------------------------------------

pub(crate) fn carrier_period(frequency_hz: u32) -> Option<u32> {
    if frequency_hz == 0 {
        return None;
    }
    let period = 1_000_000_000u64 / u64::from(frequency_hz);
    u32::try_from(period).ok().filter(|period| *period > 0)
}

pub(crate) fn raw_record(
    carrier_period_ns: u32,
    once: &[Pulse],
    held: &[Pulse],
) -> Result<IrRecord, IrError> {
    let once_words = pack_block(once);
    if once_words.is_empty() {
        return Err(IrError::Record {
            detail: "a raw record holds no durations".into(),
        });
    }
    let held_words = pack_block(held);
    Ok(IrRecord {
        carrier_period_ns,
        carrier_on_ns: carrier_period_ns / 2,
        class: CLASS_STREAM,
        groups: vec![IrPointerGroup {
            once: once_words,
            held: if held_words.is_empty() {
                None
            } else {
                Some(held_words)
            },
            tail: None,
        }],
    })
}

fn record_from_frames(frames: &[ProtocolFrame], sends: u8, protocol: &str) -> ParsedIr {
    let frequency_hz = frames.first().map_or(38_000, |frame| frame.frequency_hz);
    let period_ns = carrier_period(frequency_hz).unwrap_or(26_315);
    let mut groups = Vec::with_capacity(frames.len());
    for frame in frames {
        let once = press_block(&frame.pulses, sends, true);
        let held = press_block(&frame.pulses, 1, false);
        groups.push(IrPointerGroup {
            once: pack_block(&once),
            held: Some(pack_block(&held)),
            tail: None,
        });
    }
    ParsedIr {
        record: IrRecord {
            carrier_period_ns: period_ns,
            carrier_on_ns: period_ns / 2,
            class: CLASS_STREAM,
            groups,
        },
        protocol: protocol.into(),
        sends,
    }
}

fn press_block(pulses: &[Pulse], sends: u8, lead_in: bool) -> Vec<Pulse> {
    let mut block = Vec::new();
    if lead_in {
        block.push(Pulse::space(LEAD_IN_US));
    }
    for _ in 0..sends {
        block.extend_from_slice(pulses);
    }
    if let Some(index) = block.len().checked_sub(1)
        && !block[index].mark
    {
        if block[index].micros > 1 {
            block[index].micros -= 1;
        } else {
            block.truncate(index);
        }
    }
    block.push(Pulse::mark(1));
    block
}

fn bits_lsb(bytes: &[u8]) -> Vec<bool> {
    let mut bits = Vec::with_capacity(bytes.len() * 8);
    for byte in bytes {
        for bit in 0..8 {
            bits.push((byte >> bit) & 1 == 1);
        }
    }
    bits
}

struct PulseDistance {
    header_mark: u32,
    header_space: u32,
    bit_mark: u32,
    zero_space: u32,
    one_space: u32,
    trailer_mark: u32,
    gap: u32,
}

fn pulse_distance_frame(
    timings: &PulseDistance,
    bits: &[bool],
    frequency_hz: u32,
) -> ProtocolFrame {
    let mut pulses = Vec::with_capacity(bits.len() * 2 + 4);
    pulses.push(Pulse::mark(timings.header_mark));
    pulses.push(Pulse::space(timings.header_space));
    for bit in bits {
        pulses.push(Pulse::mark(timings.bit_mark));
        pulses.push(Pulse::space(if *bit {
            timings.one_space
        } else {
            timings.zero_space
        }));
    }
    pulses.push(Pulse::mark(timings.trailer_mark));
    pulses.push(Pulse::space(timings.gap));
    ProtocolFrame {
        frequency_hz,
        pulses,
    }
}

pub(crate) fn nec_import(address: u8, command: u8, sends: u8) -> ParsedIr {
    let payload = [address, !address, command, !command];
    let frame = pulse_distance_frame(
        &PulseDistance {
            header_mark: 9000,
            header_space: 4500,
            bit_mark: 560,
            zero_space: 560,
            one_space: 1690,
            trailer_mark: 560,
            gap: 40_000,
        },
        &bits_lsb(&payload),
        38_000,
    );
    record_from_frames(&[frame], sends, "NEC")
}

pub(crate) fn nec_import_extended(address: u16, command: u16, sends: u8) -> ParsedIr {
    let payload = [
        (address & 0xFF) as u8,
        (address >> 8) as u8,
        (command & 0xFF) as u8,
        (command >> 8) as u8,
    ];
    let frame = pulse_distance_frame(
        &PulseDistance {
            header_mark: 9000,
            header_space: 4500,
            bit_mark: 560,
            zero_space: 560,
            one_space: 1690,
            trailer_mark: 560,
            gap: 40_000,
        },
        &bits_lsb(&payload),
        38_000,
    );
    record_from_frames(&[frame], sends, "NECext")
}

pub(crate) fn samsung_import(address: u8, command: u8, sends: u8) -> ParsedIr {
    let payload = [address, address, command, !command];
    let frame = pulse_distance_frame(
        &PulseDistance {
            header_mark: 4500,
            header_space: 4500,
            bit_mark: 560,
            zero_space: 560,
            one_space: 1690,
            trailer_mark: 560,
            gap: 40_000,
        },
        &bits_lsb(&payload),
        38_000,
    );
    record_from_frames(&[frame], sends, "Samsung32")
}

pub(crate) fn jvc_import(address: u8, command: u8, sends: u8) -> ParsedIr {
    let payload = [address, command];
    let frame = pulse_distance_frame(
        &PulseDistance {
            header_mark: 8400,
            header_space: 4200,
            bit_mark: 525,
            zero_space: 525,
            one_space: 1575,
            trailer_mark: 525,
            gap: 45_000,
        },
        &bits_lsb(&payload),
        38_000,
    );
    record_from_frames(&[frame], sends, "JVC")
}

pub(crate) fn kaseikyo_import(payload: [u8; 6], sends: u8) -> ParsedIr {
    let frame = pulse_distance_frame(
        &PulseDistance {
            header_mark: 3456,
            header_space: 1728,
            bit_mark: 432,
            zero_space: 432,
            one_space: 1296,
            trailer_mark: 432,
            gap: 74_000,
        },
        &bits_lsb(&payload),
        37_000,
    );
    record_from_frames(&[frame], sends, "Kaseikyo")
}

pub(crate) fn sirc_import(
    bits: u8,
    address: u16,
    command: u8,
    sends: u8,
) -> Result<ParsedIr, IrError> {
    let address_bits = match bits {
        12 => 5,
        15 => 8,
        20 => 13,
        other => {
            return Err(IrError::Record {
                detail: format!("SIRC supports 12, 15 or 20 bits, not {other}"),
            });
        }
    };
    if command > 0x7F {
        return Err(IrError::Record {
            detail: "the SIRC command does not fit seven bits".into(),
        });
    }
    if address >= (1u16 << address_bits) {
        return Err(IrError::Record {
            detail: format!("the SIRC address does not fit {address_bits} bits"),
        });
    }
    let mut payload = Vec::with_capacity(bits as usize);
    for bit in 0..7 {
        payload.push((command >> bit) & 1 == 1);
    }
    for bit in 0..address_bits {
        payload.push((address >> bit) & 1 == 1);
    }

    let mut pulses = Vec::with_capacity(payload.len() * 2 + 2);
    pulses.push(Pulse::mark(2400));
    pulses.push(Pulse::space(600));
    for bit in &payload {
        pulses.push(Pulse::mark(if *bit { 1200 } else { 600 }));
        pulses.push(Pulse::space(600));
    }
    let elapsed: u32 = pulses.iter().map(|pulse| pulse.micros).sum();
    if let Some(last) = pulses.last_mut()
        && elapsed < 45_000
    {
        last.micros += 45_000 - elapsed;
    }
    Ok(record_from_frames(
        &[ProtocolFrame {
            frequency_hz: 40_000,
            pulses,
        }],
        sends,
        &format!("SIRC{bits}"),
    ))
}

pub(crate) fn rc5_import(address: u8, command: u8, protocol: &str, sends: u8) -> ParsedIr {
    let frames = [
        rc5_frame(address, command, false),
        rc5_frame(address, command, true),
    ];
    let mut parsed = record_from_frames(&frames, sends, protocol);
    parsed.record.groups.truncate(2);
    parsed
}

fn rc5_frame(address: u8, command: u8, toggle: bool) -> ProtocolFrame {
    const HALF_BIT: u32 = 889;
    const FRAME_PERIOD: u32 = 128 * HALF_BIT;
    let address = address & 0x1F;
    let command = command & 0x7F;

    let mut bits = Vec::with_capacity(14);
    bits.push(true);
    bits.push(command <= 0x3F);
    bits.push(toggle);
    for bit in (0..5).rev() {
        bits.push((address >> bit) & 1 == 1);
    }
    for bit in (0..6).rev() {
        bits.push((command >> bit) & 1 == 1);
    }

    let mut levels = Vec::with_capacity(28);
    for bit in bits {
        if bit {
            levels.push(false);
            levels.push(true);
        } else {
            levels.push(true);
            levels.push(false);
        }
    }
    let first_mark = levels.iter().position(|level| *level).unwrap_or(0);
    let mut pulses = Vec::new();
    let mut index = first_mark;
    while index < levels.len() {
        let level = levels[index];
        let mut run = 0u32;
        while index < levels.len() && levels[index] == level {
            run += 1;
            index += 1;
        }
        let micros = run * HALF_BIT;
        if level {
            pulses.push(Pulse::mark(micros));
        } else {
            pulses.push(Pulse::space(micros));
        }
    }
    let elapsed: u32 = pulses.iter().map(|pulse| pulse.micros).sum();
    let gap = FRAME_PERIOD.saturating_sub(elapsed).max(HALF_BIT);
    if let Some(last) = pulses.last_mut() {
        if last.mark {
            pulses.push(Pulse::space(gap));
        } else {
            last.micros += gap;
        }
    } else {
        pulses.push(Pulse::space(gap));
    }
    ProtocolFrame {
        frequency_hz: 36_000,
        pulses,
    }
}
