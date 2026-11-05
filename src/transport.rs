use crate::error::Error;
use crate::protocol::DATA_PACKET_PAYLOAD;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteDescriptor {
    pub bus: u8,
    pub address: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub bcd_device: u16,
    pub product: Option<String>,
    pub serial: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RemoteSelector {
    pub bus: u8,
    pub address: u8,
}

#[derive(Debug, Clone, Copy)]
pub struct OpenRead {
    pub handle: u8,
    pub size: u32,
}

pub trait RemoteBackend {
    fn list(&mut self) -> Result<Vec<RemoteDescriptor>, Error>;
    fn open(&mut self, selector: &RemoteSelector) -> Result<Box<dyn RemoteSession>, Error>;
}

pub trait RemoteSession {
    fn reset_filesystem(&mut self) -> Result<(), Error>;
    fn open_read(&mut self, path: &str) -> Result<OpenRead, Error>;
    fn read(&mut self, handle: u8, packets: u16) -> Result<Vec<u8>, Error>;
    fn open_write(&mut self, path: &str, size: u32) -> Result<u8, Error>;
    fn write_chunk(&mut self, handle: u8, data: &[u8], last: bool) -> Result<(), Error>;
    fn check(&mut self, handle: u8, expected: u16, covered_len: u32) -> Result<bool, Error>;
    fn flush(&mut self, handle: u8) -> Result<(), Error>;
    fn close(&mut self, handle: u8) -> Result<(), Error>;
    fn reboot(&mut self) -> Result<(), Error>;
}

pub fn read_file(session: &mut dyn RemoteSession, path: &str) -> Result<Vec<u8>, Error> {
    let OpenRead { handle, size } = session.open_read(path)?;
    let expected = size as usize;
    let mut data = Vec::with_capacity(expected);
    let read_result = (|| {
        while data.len() < expected {
            let remaining = expected - data.len();
            let packets = remaining.div_ceil(DATA_PACKET_PAYLOAD).min(32) as u16;
            let chunk = session.read(handle, packets)?;
            if chunk.is_empty() {
                break;
            }
            data.extend_from_slice(&chunk);
        }
        if data.len() < expected {
            return Err(Error::Protocol(format!(
                "short read of {path}: expected {expected} bytes, received {}",
                data.len()
            )));
        }
        data.truncate(expected);
        Ok(data)
    })();
    let close_result = session.close(handle);
    match (read_result, close_result) {
        (Ok(data), Ok(())) => Ok(data),
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
    }
}
