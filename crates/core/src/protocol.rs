use crate::container;
use crate::error::Error;
use crate::transport::{OpenRead, RemoteSession};

pub const REPORT_BYTES: usize = 64;
pub const DATA_PACKET_PAYLOAD: usize = 62;
pub const SEQUENCE_MODULUS: u8 = 64;
pub const DONE_PACKET: u8 = 0x7E;
pub const CHUNK_REPLY_FLAG: u8 = 0x80;

/// The request sequence counter shared by every file-protocol exchange.
///
/// It runs `0..64` and wraps; the done packet consumes no sequence number.
#[derive(Debug, Default)]
pub struct Sequence {
    next: u8,
}

impl Sequence {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn take(&mut self) -> u8 {
        let value = self.next % SEQUENCE_MODULUS;
        self.next = (value + 1) % SEQUENCE_MODULUS;
        value
    }
}

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

pub fn validate_reply(report: &[u8; REPORT_BYTES], command: u8, sequence: u8) -> Result<(), Error> {
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
    report[0] == 0xFF && report[1] == 0x03 && report[2] == expected
}

/// The byte-level link a [`FileProtocol`] speaks over: one 64-byte report each way.
pub trait ReportLink {
    fn send_report(&mut self, report: &[u8; REPORT_BYTES]) -> Result<(), Error>;
    fn receive_report(&mut self) -> Result<[u8; REPORT_BYTES], Error>;
}

/// The file protocol above a [`ReportLink`]: sequence numbers, commands, reads, chunks and checks.
pub struct FileProtocol<L: ReportLink> {
    link: L,
    sequence: Sequence,
}

impl<L: ReportLink> FileProtocol<L> {
    pub fn new(link: L) -> Self {
        Self {
            link,
            sequence: Sequence::new(),
        }
    }

    /// The reports this session sent, for tests that assert the wire bytes.
    #[cfg(test)]
    pub(crate) fn sent_reports(&self) -> &[[u8; REPORT_BYTES]]
    where
        L: AsRef<[[u8; REPORT_BYTES]]>,
    {
        self.link.as_ref()
    }

    fn command(&mut self, command: u8, params: &[Param<'_>]) -> Result<[u8; REPORT_BYTES], Error> {
        let sequence = self.sequence.take();
        let request = encode_request(command, sequence, params)?;
        self.link.send_report(&request)?;
        for _ in 0..64 {
            let reply = self.link.receive_report()?;
            if reply[0] != 0xFF || reply[1] != command {
                // A stale reply from an earlier session, or an unsolicited report.
                continue;
            }
            if reply[2] == 0xFF {
                return Err(Error::Protocol(format!(
                    "Remote refused command 0x{command:02X}"
                )));
            }
            if reply[2] & 0x7F != sequence & 0x7F {
                continue;
            }
            validate_reply(&reply, command, sequence)?;
            return Ok(reply);
        }
        Err(Error::Protocol(format!(
            "no reply matching command 0x{command:02X}"
        )))
    }

    fn send_only(&mut self, command: u8, params: &[Param<'_>]) -> Result<(), Error> {
        let sequence = self.sequence.take();
        let request = encode_request(command, sequence, params)?;
        self.link.send_report(&request)
    }
}
impl<L: ReportLink> RemoteSession for FileProtocol<L> {
    fn ping(&mut self) -> Result<(), Error> {
        // The client opens every session with a bare ping, but the reply is not needed: the
        // Harmony 300 can answer late or not at all, and the next command's matching loop skips
        // any ping reply that does arrive. Sending it still consumes the first-request slot.
        self.send_only(0x00, &[])
    }

    fn reset_filesystem(&mut self) -> Result<(), Error> {
        self.command(0xFF, &[Param::Byte(0x66)])?;
        Ok(())
    }

    fn open_read(&mut self, path: &str) -> Result<OpenRead, Error> {
        let reply = self.command(0x01, &[Param::String(path), Param::String("R")])?;
        parse_open(&reply)
    }

    fn read(&mut self, handle: u8, packets: u16) -> Result<Vec<u8>, Error> {
        let packets = u8::try_from(packets)
            .map_err(|_| Error::Protocol("too many read packets requested".into()))?;
        let sequence = self.sequence.take();
        let request = encode_request(0x04, sequence, &[Param::Byte(handle), Param::Byte(packets)])?;
        self.link.send_report(&request)?;

        let mut data = Vec::new();
        let mut reports = 0usize;
        while reports < usize::from(packets) {
            let reply = self.link.receive_report()?;
            reports += 1;
            if reply[0] == 0xFF {
                if reply[2] == 0xFF {
                    return Err(Error::Protocol("Remote refused read".into()));
                }
                // A stale reply from an earlier command, or the read's own echo.
                continue;
            }
            if reply[0] == 0xFE {
                break;
            }
            // The read size byte carries a flag in its high bits; the firmware's data packet
            // length is the low six bits, and a final packet pads to the transfer unit.
            let size = (reply[1] as usize).min(DATA_PACKET_PAYLOAD);
            data.extend_from_slice(&reply[2..2 + size]);
        }
        Ok(data)
    }

    fn open_write(&mut self, path: &str, size: u32) -> Result<u8, Error> {
        let reply = self.command(
            0x01,
            &[Param::String(path), Param::String("W"), Param::DWord(size)],
        )?;
        Ok(parse_open(&reply)?.handle)
    }

    fn write_chunk(&mut self, handle: u8, data: &[u8], last: bool) -> Result<(), Error> {
        if data.is_empty() {
            return Err(Error::Protocol("empty write chunk".into()));
        }
        let payloads: Vec<&[u8]> = data.chunks(DATA_PACKET_PAYLOAD).collect();
        let count = 1 + payloads.len() + usize::from(last);
        let count = u8::try_from(count)
            .map_err(|_| Error::Protocol("write chunk has too many packets".into()))?;
        let sequence = self.sequence.take();
        let request = encode_request(0x03, sequence, &[Param::Byte(handle), Param::Byte(count)])?;
        self.link.send_report(&request)?;

        let mut last_sequence = sequence;
        for payload in payloads {
            let sequence = self.sequence.take();
            last_sequence = sequence;
            let mut packet = [0u8; REPORT_BYTES];
            packet[0] = sequence;
            packet[1] = payload.len() as u8;
            packet[2..2 + payload.len()].copy_from_slice(payload);
            self.link.send_report(&packet)?;
        }
        if last {
            let mut done = [0u8; REPORT_BYTES];
            done[0] = DONE_PACKET;
            self.link.send_report(&done)?;
        }

        let reply = self.link.receive_report()?;
        if !chunk_reply_matches(&reply, last_sequence, last) {
            let expected = if last {
                DONE_PACKET | CHUNK_REPLY_FLAG
            } else {
                CHUNK_REPLY_FLAG | (last_sequence & 0x3F)
            };
            return Err(Error::Protocol(format!(
                "unexpected write chunk reply 0x{:02X} 0x{:02X} 0x{:02X}, expected 0x{expected:02X}",
                reply[0], reply[1], reply[2]
            )));
        }
        Ok(())
    }

    fn check(&mut self, handle: u8, expected: u16, covered_len: u32) -> Result<bool, Error> {
        let reply = self.command(
            0x06,
            &[
                Param::Byte(handle),
                Param::Byte(0x01),
                Param::String("XOR"),
                Param::Word(container::TRAILER_CHECKSUM_SEED),
                Param::DWord(0),
                Param::DWord(covered_len),
                Param::Word(expected),
            ],
        )?;
        Ok(reply[7] == b'm')
    }

    fn flush(&mut self, handle: u8) -> Result<(), Error> {
        self.command(0x05, &[Param::Byte(handle), Param::Byte(0)])?;
        Ok(())
    }

    fn close(&mut self, handle: u8) -> Result<(), Error> {
        self.command(0x07, &[Param::Byte(handle)])?;
        Ok(())
    }

    fn reboot(&mut self) -> Result<(), Error> {
        self.send_only(0xFF, &[Param::Byte(0x00)])
    }
}
