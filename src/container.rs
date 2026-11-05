use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::error::Error;

#[allow(dead_code)]
pub const FLASH_BASE: u32 = 0x020000;
pub const MAX_CONTAINER_SIZE: usize = 256 * 1024;
pub const HEADER_MIN_LEN: usize = 0x4B;
pub const END_MARKER: &[u8; 4] = b"PTYY";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationStatus {
    Ok,
    ChecksumMismatch,
    Unreadable,
}

impl ValidationStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::ChecksumMismatch => "checksum_mismatch",
            Self::Unreadable => "unreadable",
        }
    }
}

pub fn has_header(bytes: &[u8]) -> bool {
    bytes.len() >= HEADER_MIN_LEN && &bytes[0..4] == b"GSPM" && &bytes[0x47..0x4B] == b"LWJL"
}

pub fn reconstruct_trailer(read: &[u8]) -> Vec<u8> {
    if has_header(read) && !read.ends_with(&END_MARKER[..]) {
        let mut out = Vec::with_capacity(read.len() + END_MARKER.len());
        out.extend_from_slice(read);
        out.extend_from_slice(END_MARKER);
        out
    } else {
        read.to_vec()
    }
}

pub fn trailer_checksum(bytes: &[u8]) -> Result<u16, Error> {
    if bytes.len() < 6 {
        return Err(Error::Validation(
            "Container is shorter than its trailer".into(),
        ));
    }
    let mut accumulator = 0x4321u16;
    for word in bytes[..bytes.len() - 6].as_chunks::<2>().0 {
        accumulator ^= u16::from_le_bytes(*word);
    }
    Ok(accumulator)
}

pub fn stored_checksum(bytes: &[u8]) -> Option<u16> {
    if bytes.len() < 6 {
        return None;
    }
    let start = bytes.len() - 6;
    Some(u16::from_le_bytes([bytes[start], bytes[start + 1]]))
}

pub fn validation_status(container: &[u8]) -> ValidationStatus {
    if !has_header(container) {
        return ValidationStatus::Unreadable;
    }
    match (stored_checksum(container), trailer_checksum(container)) {
        (Some(stored), Ok(computed)) if stored == computed => ValidationStatus::Ok,
        _ => ValidationStatus::ChecksumMismatch,
    }
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(digest.len() * 2);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
