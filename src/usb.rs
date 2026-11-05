use std::io::{Read, Write};
use std::time::Duration;

use nusb::MaybeFuture;
use nusb::descriptors::TransferType;
use nusb::io::{EndpointRead, EndpointWrite};
use nusb::transfer::{Direction, In, Interrupt, Out};

use crate::error::Error;
use crate::protocol::{
    CHUNK_REPLY_FLAG, DONE_PACKET, Param, Sequence, chunk_reply_matches, encode_request,
    parse_open, validate_reply,
};
use crate::transport::{OpenRead, RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession};

const REPORT_BYTES: usize = 64;
const READ_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct NusbBackend;

impl NusbBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NusbBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteBackend for NusbBackend {
    fn list(&mut self) -> Result<Vec<RemoteDescriptor>, Error> {
        let devices = nusb::list_devices()
            .wait()
            .map_err(|error| Error::Device(format!("USB enumeration failed: {error}")))?;
        let mut descriptors = Vec::new();
        for device in devices {
            descriptors.push(RemoteDescriptor {
                bus: device.busnum(),
                address: device.device_address(),
                vendor_id: device.vendor_id(),
                product_id: device.product_id(),
                bcd_device: device.device_version(),
                product: device.product_string().map(str::to_owned),
                serial: device.serial_number().map(str::to_owned),
            });
        }
        Ok(descriptors)
    }

    fn open(&mut self, selector: &RemoteSelector) -> Result<Box<dyn RemoteSession>, Error> {
        let devices = nusb::list_devices()
            .wait()
            .map_err(|error| Error::Device(format!("USB enumeration failed: {error}")))?;
        let info = devices
            .into_iter()
            .find(|device| {
                device.busnum() == selector.bus && device.device_address() == selector.address
            })
            .ok_or_else(|| {
                Error::Device(format!(
                    "Remote at bus {} address {} is no longer connected",
                    selector.bus, selector.address
                ))
            })?;

        let device = info
            .open()
            .wait()
            .map_err(|error| Error::Device(format!("cannot open Remote: {error}")))?;
        let configuration = device
            .active_configuration()
            .map_err(|error| Error::Device(format!("cannot read USB configuration: {error}")))?;

        let interface_info = configuration
            .interfaces()
            .find(|interface| interface.first_alt_setting().class() == 3)
            .or_else(|| configuration.interfaces().next())
            .ok_or_else(|| Error::Device("Remote exposes no USB interface".into()))?;
        let interface_number = interface_info.interface_number();
        let descriptor = interface_info.first_alt_setting();

        let mut in_address = None;
        let mut out_address = None;
        for endpoint in descriptor.endpoints() {
            if endpoint.transfer_type() != TransferType::Interrupt {
                continue;
            }
            match endpoint.direction() {
                Direction::In => in_address = Some(endpoint.address()),
                Direction::Out => out_address = Some(endpoint.address()),
            }
        }
        let in_address = in_address
            .ok_or_else(|| Error::Device("Remote has no interrupt IN endpoint".into()))?;
        let out_address = out_address
            .ok_or_else(|| Error::Device("Remote has no interrupt OUT endpoint".into()))?;

        let interface = device
            .detach_and_claim_interface(interface_number)
            .wait()
            .map_err(|error| Error::Device(format!("cannot claim USB interface: {error}")))?;
        let reader = interface
            .endpoint::<Interrupt, In>(in_address)
            .map_err(|error| Error::Device(format!("cannot open interrupt IN endpoint: {error}")))?
            .reader(REPORT_BYTES)
            .with_read_timeout(READ_TIMEOUT);
        let writer = interface
            .endpoint::<Interrupt, Out>(out_address)
            .map_err(|error| Error::Device(format!("cannot open interrupt OUT endpoint: {error}")))?
            .writer(REPORT_BYTES)
            .with_write_timeout(WRITE_TIMEOUT);

        Ok(Box::new(NusbSession {
            reader,
            writer,
            sequence: Sequence::new(),
            _interface: interface,
        }))
    }
}

struct NusbSession {
    reader: EndpointRead<Interrupt>,
    writer: EndpointWrite<Interrupt>,
    sequence: Sequence,
    _interface: nusb::Interface,
}

impl NusbSession {
    fn take_sequence(&mut self) -> u8 {
        self.sequence.take()
    }

    fn send_report(&mut self, report: &[u8; REPORT_BYTES]) -> Result<(), Error> {
        self.writer
            .write_all(report)
            .map_err(|error| Error::Protocol(format!("USB write failed: {error}")))?;
        self.writer
            .flush()
            .map_err(|error| Error::Protocol(format!("USB write failed: {error}")))?;
        Ok(())
    }

    fn receive_report(&mut self) -> Result<[u8; REPORT_BYTES], Error> {
        let mut report = [0u8; REPORT_BYTES];
        self.reader
            .read_exact(&mut report)
            .map_err(|error| Error::Protocol(format!("no reply from Remote: {error}")))?;
        Ok(report)
    }

    fn command(&mut self, command: u8, params: &[Param<'_>]) -> Result<[u8; REPORT_BYTES], Error> {
        let sequence = self.take_sequence();
        let request = encode_request(command, sequence, params)?;
        self.send_report(&request)?;
        let reply = self.receive_report()?;
        validate_reply(&reply, command, sequence)?;
        Ok(reply)
    }

    fn send_only(&mut self, command: u8, params: &[Param<'_>]) -> Result<(), Error> {
        let sequence = self.take_sequence();
        let request = encode_request(command, sequence, params)?;
        self.send_report(&request)
    }
}

impl RemoteSession for NusbSession {
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
        let sequence = self.take_sequence();
        let request = encode_request(0x04, sequence, &[Param::Byte(handle), Param::Byte(packets)])?;
        self.send_report(&request)?;

        let mut data = Vec::new();
        let mut reports = 0usize;
        loop {
            let reply = self.receive_report()?;
            reports += 1;
            if reports > 256 {
                return Err(Error::Protocol(
                    "read reply did not terminate within 256 reports".into(),
                ));
            }
            if reply[0] == 0xFF {
                if reply[1] == 0x04 {
                    continue;
                }
                if reply[2] == 0xFF {
                    return Err(Error::Protocol("Remote refused read".into()));
                }
                return Err(Error::Protocol(format!(
                    "unexpected read reply 0x{:02X} 0x{:02X}",
                    reply[0], reply[1]
                )));
            }
            if reply[0] == 0xFE {
                break;
            }
            let size = reply[1] as usize;
            if size > 62 {
                return Err(Error::Protocol(format!(
                    "read packet declares {size} bytes"
                )));
            }
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
        let payloads: Vec<&[u8]> = data.chunks(62).collect();
        let count = 1 + payloads.len() + usize::from(last);
        let count = u8::try_from(count)
            .map_err(|_| Error::Protocol("write chunk has too many packets".into()))?;
        let sequence = self.take_sequence();
        let request = encode_request(0x03, sequence, &[Param::Byte(handle), Param::Byte(count)])?;
        self.send_report(&request)?;

        let mut last_sequence = sequence;
        for payload in payloads {
            let sequence = self.take_sequence();
            last_sequence = sequence;
            let mut packet = [0u8; REPORT_BYTES];
            packet[0] = sequence;
            packet[1] = payload.len() as u8;
            packet[2..2 + payload.len()].copy_from_slice(payload);
            self.send_report(&packet)?;
        }
        if last {
            let mut done = [0u8; REPORT_BYTES];
            done[0] = DONE_PACKET;
            self.send_report(&done)?;
        }

        let reply = self.receive_report()?;
        if !chunk_reply_matches(&reply, last_sequence, last) {
            let expected = if last {
                DONE_PACKET | CHUNK_REPLY_FLAG
            } else {
                CHUNK_REPLY_FLAG | (last_sequence & 0x3F)
            };
            return Err(Error::Protocol(format!(
                "unexpected write chunk reply 0x{:02X}, expected 0x{expected:02X}",
                reply[0]
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
                Param::Word(0x4321),
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
