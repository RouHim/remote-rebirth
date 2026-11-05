use crate::error::Error;
use crate::transport::OpenRead;

pub const REPORT_BYTES: usize = 64;
pub const DATA_PACKET_PAYLOAD: usize = 62;
pub const SEQUENCE_MODULUS: u8 = 64;
pub const DONE_PACKET: u8 = 0x7E;
pub const CHUNK_REPLY_FLAG: u8 = 0x80;

pub enum Param<'a> {
    String(&'a str),
    Byte(u8),
    Word(u16),
    DWord(u32),
}

pub fn encode_request(
    command: u8,
    sequence: u8,
    params: &[Param<'_>],
) -> Result<[u8; REPORT_BYTES], Error> {
    let mut report = [0u8; REPORT_BYTES];
    report[0] = 0xFF;
    report[1] = command;
    report[2] = sequence;
    report[3] = params.len() as u8;

    let mut at = 4;
    for param in params {
        match param {
            Param::String(value) => {
                let bytes = value.as_bytes();
                ensure_capacity(at, 2 + bytes.len())?;
                report[at] = 0x80;
                at += 1;
                report[at..at + bytes.len()].copy_from_slice(bytes);
                at += bytes.len();
                report[at] = 0;
                at += 1;
            }
            Param::Byte(value) => {
                ensure_capacity(at, 2)?;
                report[at] = 0x01;
                report[at + 1] = *value;
                at += 2;
            }
            Param::Word(value) => {
                ensure_capacity(at, 3)?;
                report[at] = 0x02;
                report[at + 1] = (value >> 8) as u8;
                report[at + 2] = *value as u8;
                at += 3;
            }
            Param::DWord(value) => {
                ensure_capacity(at, 5)?;
                report[at] = 0x04;
                report[at + 1] = (value >> 24) as u8;
                report[at + 2] = (value >> 16) as u8;
                report[at + 3] = (value >> 8) as u8;
                report[at + 4] = *value as u8;
                at += 5;
            }
        }
    }
    Ok(report)
}

fn ensure_capacity(at: usize, needed: usize) -> Result<(), Error> {
    if at + needed > REPORT_BYTES {
        return Err(Error::Protocol(
            "request does not fit in one 64-byte report".into(),
        ));
    }
    Ok(())
}

pub fn validate_reply(
    report: &[u8; REPORT_BYTES],
    command: u8,
    sequence: u8,
) -> Result<(), Error> {
    if report[0] != 0xFF {
        return Err(Error::Protocol(format!(
            "reply does not start with 0xFF: 0x{:02X}",
            report[0]
        )));
    }
    if report[1] != command {
        return Err(Error::Protocol(format!(
            "reply command 0x{:02X} does not match request 0x{command:02X}",
            report[1]
        )));
    }
    if report[2] == 0xFF {
        return Err(Error::Protocol(format!(
            "Remote refused command 0x{command:02X}"
        )));
    }
    if report[2] & 0x7F != sequence & 0x7F {
        return Err(Error::Protocol(format!(
            "reply sequence 0x{:02X} does not match request 0x{:02X}",
            report[2], sequence
        )));
    }
    Ok(())
}

pub fn parse_open(report: &[u8; REPORT_BYTES]) -> Result<OpenRead, Error> {
    if report[4] != 0x01 || report[6] != 0x04 {
        return Err(Error::Protocol(
            "open reply does not carry a handle and a size".into(),
        ));
    }
    Ok(OpenRead {
        handle: report[5],
        size: u32::from_be_bytes([report[7], report[8], report[9], report[10]]),
    })
}

pub fn chunk_reply_matches(report: &[u8; REPORT_BYTES], last_sequence: u8, last: bool) -> bool {
    let expected = if last {
        DONE_PACKET | CHUNK_REPLY_FLAG
    } else {
        CHUNK_REPLY_FLAG | (last_sequence & 0x3F)
    };
    report[0] == expected
}
