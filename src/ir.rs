//! IR Sources and IR Records.
//!
//! An [`IrSource`] names a user-supplied file and selector that supplies one
//! command's timings or protocol fields. Timing-bearing sources become class-1
//! pulse records directly; sources that carry protocol fields are encoded with
//! one of the six protocol encoders. [`IrRecord`] is the stored form: carrier,
//! class, and pointer groups of mark/space duration words.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::container;
use crate::error::Error;

/// Bit 15 of a duration word: set when the pulse is a mark.
pub const MARK_BIT: u16 = 0x8000;
/// Bits 14..0 of a duration word: microseconds.
pub const DURATION_MASK: u16 = 0x7FFF;
/// The longest interval one duration word can carry.
pub const MAX_DURATION_US: u32 = 32767;
/// The lead-in silence a compiled first block opens with.
pub const LEAD_IN_US: u32 = 50_000;
/// A word of zero closes a class-1 block.
pub const BLOCK_TERMINATOR: u16 = 0;
/// The only record class this module writes or reads.
pub const CLASS_STREAM: u8 = 1;
/// A record carries one pointer group, or two for a toggling code.
pub const MAX_POINTER_GROUPS: usize = 2;
/// A record pointer lands this many bytes into its header, on the class byte.
pub const RECORD_POINTER_BIAS: usize = 7;
/// Pronto's frequency-divider constant, in hertz.
const PRONTO_CLOCK_HZ: u64 = 4_145_146;

/// The formats an IR Source can be written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum IrSourceKind {
    Pronto,
    Lirc,
    Flipper,
}

impl IrSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pronto => "pronto",
            Self::Lirc => "lirc",
            Self::Flipper => "flipper",
        }
    }
}

/// A user-supplied IR Source: a file or inline hex plus a selector.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrSource {
    pub format: IrSourceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub button: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

impl IrSource {
    /// A compact, stable description of what the source selects.
    pub fn selector(&self) -> String {
        match self.format {
            IrSourceKind::Pronto => {
                if self.hex.is_some() {
                    "inline hex".to_string()
                } else {
                    "file".to_string()
                }
            }
            IrSourceKind::Lirc => format!(
                "remote={} button={}",
                self.remote.as_deref().unwrap_or("(none)"),
                self.button.as_deref().unwrap_or("(none)")
            ),
            IrSourceKind::Flipper => {
                format!("command={}", self.command.as_deref().unwrap_or("(none)"))
            }
        }
    }
}

/// Why an IR Source or record could not be imported or encoded.
#[derive(Debug, thiserror::Error)]
pub enum IrError {
    #[error("{file}: {selector}: unsupported Pronto format 0x{word:04X}")]
    UnsupportedProntoFormat {
        file: String,
        selector: String,
        word: u16,
    },
    #[error("{file}: {selector}: unsupported LIRC flag {flag}")]
    UnsupportedLircFlag {
        file: String,
        selector: String,
        flag: String,
    },
    #[error("{file}: {selector}: unsupported LIRC protocol {protocol}")]
    UnsupportedLircProtocol {
        file: String,
        selector: String,
        protocol: String,
    },
    #[error("{file}: {selector}: unsupported Flipper protocol {protocol}")]
    UnsupportedFlipperProtocol {
        file: String,
        selector: String,
        protocol: String,
    },
    #[error("{file}: {selector}: {detail}")]
    Source {
        file: String,
        selector: String,
        detail: String,
    },
    #[error("IR record: {detail}")]
    Record { detail: String },
}

impl From<IrError> for Error {
    fn from(value: IrError) -> Self {
        Error::Validation(value.to_string())
    }
}

/// How an imported command was produced: format, path, selector, digest,
/// protocol, and press count.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub format: IrSourceKind,
    pub path: Option<String>,
    pub selector: String,
    pub sha256: String,
    pub protocol: String,
    pub sends: u8,
}

/// One imported command: its IR Record and its provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedCommand {
    pub record: IrRecord,
    pub provenance: Provenance,
}

/// One pulse: a mark (carrier on) or space (carrier off) and its length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pulse {
    pub mark: bool,
    pub micros: u32,
}

impl Pulse {
    pub fn mark(micros: u32) -> Self {
        Self { mark: true, micros }
    }

    pub fn space(micros: u32) -> Self {
        Self {
            mark: false,
            micros,
        }
    }
}

/// Three block pointers of one press: once, held, and tail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrPointerGroup {
    pub once: Vec<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub held: Option<Vec<u16>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tail: Option<Vec<u16>>,
}

impl IrPointerGroup {
    pub fn new(once: Vec<u16>, held: Option<Vec<u16>>) -> Self {
        Self {
            once,
            held,
            tail: None,
        }
    }
}

/// A class-1 IR Record: carrier plus one or two pointer groups of blocks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrRecord {
    pub carrier_period_ns: u32,
    pub carrier_on_ns: u32,
    #[serde(default = "default_class")]
    pub class: u8,
    pub groups: Vec<IrPointerGroup>,
}

fn default_class() -> u8 {
    CLASS_STREAM
}

/// The bytes a record occupies and the address a pointer table must hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedIrRecord {
    /// Blocks followed by the header.
    pub bytes: Vec<u8>,
    /// Offset of the header inside `bytes`.
    pub header_offset: usize,
    /// Flash address of `bytes[0]`.
    pub base_addr: u32,
    /// Flash address of the class byte, where a record pointer lands.
    pub pointer: u32,
}

impl IrRecord {
    pub fn pointer_group_count(&self) -> usize {
        self.groups.len()
    }

    /// Serialize the record so its blocks sit in front of its header and a
    /// pointer at `header_addr + 7` lands on the class byte.
    pub fn encode_at(&self, header_addr: u32) -> Result<EncodedIrRecord, IrError> {
        if self.class != CLASS_STREAM {
            return Err(IrError::Record {
                detail: format!("only class 1 records are encoded, not class {}", self.class),
            });
        }
        if self.groups.is_empty() || self.groups.len() > MAX_POINTER_GROUPS {
            return Err(IrError::Record {
                detail: format!(
                    "a record carries one or two pointer groups, not {}",
                    self.groups.len()
                ),
            });
        }
        if self.carrier_period_ns > 0x00FF_FFFF || self.carrier_on_ns > 0x00FF_FFFF {
            return Err(IrError::Record {
                detail: "carrier timings exceed the header's 24-bit fields".into(),
            });
        }

        let mut blocks: Vec<Vec<u8>> = Vec::new();
        let mut group_blocks: Vec<[Option<usize>; 3]> = Vec::new();
        for group in &self.groups {
            let mut slots = [None, None, None];
            for (slot, block) in [Some(&group.once), group.held.as_ref(), group.tail.as_ref()]
                .into_iter()
                .enumerate()
            {
                if let Some(words) = block {
                    if words.is_empty() {
                        return Err(IrError::Record {
                            detail: "a record block carries no durations".into(),
                        });
                    }
                    if words.contains(&BLOCK_TERMINATOR) {
                        return Err(IrError::Record {
                            detail: "a duration word is zero, which would close the block".into(),
                        });
                    }
                    slots[slot] = Some(blocks.len());
                    let mut bytes = Vec::with_capacity(words.len() * 2 + 2);
                    for word in words {
                        bytes.extend_from_slice(&word.to_le_bytes());
                    }
                    bytes.extend_from_slice(&BLOCK_TERMINATOR.to_le_bytes());
                    blocks.push(bytes);
                }
            }
            group_blocks.push(slots);
        }

        let block_length: usize = blocks.iter().map(Vec::len).sum();
        let block_length_u32 = u32::try_from(block_length).map_err(|_| IrError::Record {
            detail: "record blocks are too large".into(),
        })?;
        if header_addr < block_length_u32 {
            return Err(IrError::Record {
                detail: format!("record address 0x{header_addr:06X} leaves no room for its blocks"),
            });
        }
        let base_addr = header_addr - block_length_u32;
        let pointer = header_addr + RECORD_POINTER_BIAS as u32;
        if pointer > 0x00FF_FFFF {
            return Err(IrError::Record {
                detail: "record pointer exceeds the 24-bit address space".into(),
            });
        }

        let header_offset = block_length;
        let mut bytes = Vec::with_capacity(block_length + 12 + 9 * self.groups.len());
        for block in &blocks {
            bytes.extend_from_slice(block);
        }
        bytes.push(0);
        bytes.extend_from_slice(&u24(self.carrier_period_ns));
        bytes.extend_from_slice(&u24(self.carrier_on_ns));
        bytes.push(self.class);
        bytes.extend_from_slice(&u24(header_addr));
        bytes.push(self.groups.len() as u8);
        let mut next_block = 0usize;
        let mut addresses = Vec::new();
        for block in &blocks {
            addresses.push(
                base_addr
                    + u32::try_from(next_block).map_err(|_| IrError::Record {
                        detail: "record blocks are too large".into(),
                    })?,
            );
            next_block += block.len();
        }
        for slots in &group_blocks {
            for slot in slots {
                let address = slot.map_or(0, |index| addresses[index]);
                bytes.extend_from_slice(&u24(address));
            }
        }

        Ok(EncodedIrRecord {
            bytes,
            header_offset,
            base_addr,
            pointer,
        })
    }

    /// Read a class-1 record whose header is at `base_addr + header_offset`.
    pub fn decode(bytes: &[u8], base_addr: u32, header_offset: usize) -> Result<Self, IrError> {
        if header_offset > bytes.len() {
            return Err(IrError::Record {
                detail: "record header offset is outside the record area".into(),
            });
        }
        let header = &bytes[header_offset..];
        if header.len() < 12 {
            return Err(IrError::Record {
                detail: "record header is truncated".into(),
            });
        }
        let carrier_period_ns = read_u24(&header[1..4]);
        let carrier_on_ns = read_u24(&header[4..7]);
        let class = header[7];
        let own_start = read_u24(&header[8..11]);
        let expected_start =
            base_addr
                .checked_add(header_offset as u32)
                .ok_or_else(|| IrError::Record {
                    detail: "record address overflows".into(),
                })?;
        if own_start != expected_start {
            return Err(IrError::Record {
                detail: format!(
                    "record states its start as 0x{own_start:06X}, not 0x{expected_start:06X}"
                ),
            });
        }
        let count = header[11] as usize;
        if count == 0 || count > MAX_POINTER_GROUPS {
            return Err(IrError::Record {
                detail: format!("unsupported pointer-group count {count}"),
            });
        }
        if header.len() < 12 + 9 * count {
            return Err(IrError::Record {
                detail: "record pointer groups are truncated".into(),
            });
        }
        let mut groups = Vec::with_capacity(count);
        for group in 0..count {
            let start = 12 + 9 * group;
            let pointers = [
                read_u24(&header[start..start + 3]),
                read_u24(&header[start + 3..start + 6]),
                read_u24(&header[start + 6..start + 9]),
            ];
            let once =
                read_block(bytes, base_addr, pointers[0])?.ok_or_else(|| IrError::Record {
                    detail: "the once pointer is NULL".into(),
                })?;
            let held = read_block(bytes, base_addr, pointers[1])?;
            let tail = read_block(bytes, base_addr, pointers[2])?;
            groups.push(IrPointerGroup { once, held, tail });
        }
        Ok(Self {
            carrier_period_ns,
            carrier_on_ns,
            class,
            groups,
        })
    }
}

fn read_u24(bytes: &[u8]) -> u32 {
    u32::from(bytes[0]) | (u32::from(bytes[1]) << 8) | (u32::from(bytes[2]) << 16)
}

fn u24(value: u32) -> [u8; 3] {
    [
        (value & 0xFF) as u8,
        ((value >> 8) & 0xFF) as u8,
        ((value >> 16) & 0xFF) as u8,
    ]
}

fn read_block(bytes: &[u8], base_addr: u32, pointer: u32) -> Result<Option<Vec<u16>>, IrError> {
    if pointer == 0 {
        return Ok(None);
    }
    let offset = pointer
        .checked_sub(base_addr)
        .map(|value| value as usize)
        .ok_or_else(|| IrError::Record {
            detail: format!("block pointer 0x{pointer:06X} precedes the record area"),
        })?;
    let mut words = Vec::new();
    let mut at = offset;
    loop {
        if at + 2 > bytes.len() {
            return Err(IrError::Record {
                detail: "record block runs past the record area".into(),
            });
        }
        let word = u16::from_le_bytes([bytes[at], bytes[at + 1]]);
        at += 2;
        if word == BLOCK_TERMINATOR {
            break;
        }
        words.push(word);
        if words.len() > 8192 {
            return Err(IrError::Record {
                detail: "record block does not close".into(),
            });
        }
    }
    Ok(Some(words))
}

/// Pack pulses into duration words: bit 15 marks a mark, bits 14..0 are
/// microseconds, and a long interval is split so no word asks for more than
/// 32767 microseconds.
pub fn pack_block(pulses: &[Pulse]) -> Vec<u16> {
    let mut merged: Vec<Pulse> = Vec::new();
    for pulse in pulses.iter().copied().filter(|pulse| pulse.micros > 0) {
        if let Some(last) = merged.last_mut()
            && last.mark == pulse.mark
        {
            last.micros = last.micros.saturating_add(pulse.micros);
            continue;
        }
        merged.push(pulse);
    }

    let mut words = Vec::new();
    for pulse in merged {
        for chunk in split_duration(pulse.micros) {
            words.push(chunk | if pulse.mark { MARK_BIT } else { 0 });
        }
    }
    words
}

fn split_duration(micros: u32) -> Vec<u16> {
    let mut words = Vec::new();
    let mut remaining = micros;
    while remaining > MAX_DURATION_US {
        if remaining - MAX_DURATION_US < MAX_DURATION_US / 2 {
            let first = remaining / 2;
            let second = remaining - first;
            words.push(first as u16);
            words.push(second as u16);
            return words;
        }
        words.push(MAX_DURATION_US as u16);
        remaining -= MAX_DURATION_US;
    }
    if remaining > 0 {
        words.push(remaining as u16);
    }
    words
}

#[derive(Debug, Clone)]
struct Context {
    source: String,
    selector: String,
}

impl Context {
    fn error(&self, detail: impl Into<String>) -> IrError {
        IrError::Source {
            file: self.source.clone(),
            selector: self.selector.clone(),
            detail: detail.into(),
        }
    }

    fn malformed(&self, detail: impl Into<String>) -> IrError {
        self.error(format!("malformed IR source: {}", detail.into()))
    }
}

struct Loaded {
    text: String,
    bytes: Vec<u8>,
    path: Option<String>,
}

/// Import one command from an IR Source.
///
/// `model_dir` resolves relative file paths. `sends_override` replaces the
/// protocol's default press count for protocol-bearing sources; a raw capture
/// already contains its own press and reports one send.
pub fn import(
    source: &IrSource,
    model_dir: &Path,
    sends_override: Option<u8>,
) -> Result<ImportedCommand, IrError> {
    if sends_override == Some(0) {
        return Err(IrError::Source {
            file: source.file.clone().unwrap_or_else(|| "inline".into()),
            selector: source.selector(),
            detail: "sends must be at least 1".into(),
        });
    }
    let loaded = load(source, model_dir)?;
    let context = Context {
        source: loaded
            .path
            .clone()
            .unwrap_or_else(|| "inline Pronto hex".into()),
        selector: source.selector(),
    };

    let parsed = match source.format {
        IrSourceKind::Pronto => parse_pronto(&loaded.text, &context, sends_override)?,
        IrSourceKind::Lirc => parse_lirc(&loaded.text, source, &context, sends_override)?,
        IrSourceKind::Flipper => parse_flipper(&loaded.text, source, &context, sends_override)?,
    };

    let sends = parsed.sends;

    Ok(ImportedCommand {
        record: parsed.record,
        provenance: Provenance {
            format: source.format,
            path: loaded.path,
            selector: source.selector(),
            sha256: container::sha256_hex(&loaded.bytes),
            protocol: parsed.protocol,
            sends,
        },
    })
}

fn load(source: &IrSource, model_dir: &Path) -> Result<Loaded, IrError> {
    let selector = source.selector();
    let inline_source = || {
        source
            .file
            .clone()
            .unwrap_or_else(|| format!("inline {}", source.format.as_str()))
    };
    match source.format {
        IrSourceKind::Pronto => {
            if let Some(hex) = &source.hex {
                return Ok(Loaded {
                    text: hex.clone(),
                    bytes: hex.as_bytes().to_vec(),
                    path: None,
                });
            }
            let file = source.file.as_deref().ok_or_else(|| IrError::Source {
                file: inline_source(),
                selector: selector.clone(),
                detail: "a pronto source needs inline `hex` or a `file`".into(),
            })?;
            let path = resolve_path(file, model_dir);
            let bytes = fs::read(&path).map_err(|error| IrError::Source {
                file: path.display().to_string(),
                selector: selector.clone(),
                detail: format!("cannot read IR source: {error}"),
            })?;
            let text = String::from_utf8(bytes.clone()).map_err(|_| IrError::Source {
                file: path.display().to_string(),
                selector: selector.clone(),
                detail: "IR source is not UTF-8".into(),
            })?;
            Ok(Loaded {
                text,
                bytes,
                path: Some(path.display().to_string()),
            })
        }
        IrSourceKind::Lirc | IrSourceKind::Flipper => {
            let file = source.file.as_deref().ok_or_else(|| IrError::Source {
                file: inline_source(),
                selector: selector.clone(),
                detail: format!("a {} source needs a `file`", source.format.as_str()),
            })?;
            let path = resolve_path(file, model_dir);
            let bytes = fs::read(&path).map_err(|error| IrError::Source {
                file: path.display().to_string(),
                selector: selector.clone(),
                detail: format!("cannot read IR source: {error}"),
            })?;
            let text = String::from_utf8(bytes.clone()).map_err(|_| IrError::Source {
                file: path.display().to_string(),
                selector: selector.clone(),
                detail: "IR source is not UTF-8".into(),
            })?;
            Ok(Loaded {
                text,
                bytes,
                path: Some(path.display().to_string()),
            })
        }
    }
}

fn resolve_path(file: &str, model_dir: &Path) -> PathBuf {
    let path = Path::new(file);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        model_dir.join(path)
    }
}

struct ParsedIr {
    record: IrRecord,
    protocol: String,
    sends: u8,
}

struct ProtocolFrame {
    frequency_hz: u32,
    pulses: Vec<Pulse>,
}

// ---------------------------------------------------------------------------
// Pronto
// ---------------------------------------------------------------------------

fn parse_pronto(text: &str, context: &Context, sends: Option<u8>) -> Result<ParsedIr, IrError> {
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

fn parse_lirc(
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

fn alternating_pulses(timings: &[u32]) -> Vec<Pulse> {
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

fn parse_flipper(
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

// ---------------------------------------------------------------------------
// Record construction and protocol encoders
// ---------------------------------------------------------------------------

fn carrier_period(frequency_hz: u32) -> Option<u32> {
    if frequency_hz == 0 {
        return None;
    }
    let period = 1_000_000_000u64 / u64::from(frequency_hz);
    u32::try_from(period).ok().filter(|period| *period > 0)
}

fn raw_record(carrier_period_ns: u32, once: &[Pulse], held: &[Pulse]) -> Result<IrRecord, IrError> {
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

fn nec_import(address: u8, command: u8, sends: u8) -> ParsedIr {
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

fn nec_import_extended(address: u16, command: u16, sends: u8) -> ParsedIr {
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

fn samsung_import(address: u8, command: u8, sends: u8) -> ParsedIr {
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

fn jvc_import(address: u8, command: u8, sends: u8) -> ParsedIr {
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

fn kaseikyo_import(payload: [u8; 6], sends: u8) -> ParsedIr {
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

fn sirc_import(bits: u8, address: u16, command: u8, sends: u8) -> Result<ParsedIr, IrError> {
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

fn rc5_import(address: u8, command: u8, protocol: &str, sends: u8) -> ParsedIr {
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
