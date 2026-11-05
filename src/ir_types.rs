//! The shared IR types and the import entry point.
//!
//! An [`IrSource`] names a user-supplied file and selector that supplies one command's timings or
//! protocol fields. Timing-bearing sources become class-1 pulse records directly; sources that carry
//! protocol fields are encoded with one of the six protocol encoders. [`IrRecord`] is the stored
//! form: carrier, class, and pointer groups of mark/space duration words.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::container;
use crate::error::Error;
use crate::ir_flipper::parse_flipper;
use crate::ir_lirc::parse_lirc;
use crate::ir_pronto::parse_pronto;

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

#[derive(Debug, Clone)]
pub(crate) struct Context {
    pub(crate) source: String,
    pub(crate) selector: String,
}

impl Context {
    pub(crate) fn error(&self, detail: impl Into<String>) -> IrError {
        IrError::Source {
            file: self.source.clone(),
            selector: self.selector.clone(),
            detail: detail.into(),
        }
    }

    pub(crate) fn malformed(&self, detail: impl Into<String>) -> IrError {
        self.error(format!("malformed IR source: {}", detail.into()))
    }
}

pub(crate) struct Loaded {
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

pub(crate) struct ParsedIr {
    pub(crate) record: IrRecord,
    pub(crate) protocol: String,
    pub(crate) sends: u8,
}

pub(crate) struct ProtocolFrame {
    pub(crate) frequency_hz: u32,
    pub(crate) pulses: Vec<Pulse>,
}
