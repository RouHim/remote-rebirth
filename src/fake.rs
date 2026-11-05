use std::collections::HashMap;

use crate::error::Error;
use crate::transport::{OpenRead, RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession};

pub struct FakeDevice {
    pub descriptor: RemoteDescriptor,
    pub sysinfo: Vec<u8>,
    pub usercfg: Vec<u8>,
}

pub struct FakeBackend {
    pub devices: Vec<FakeDevice>,
}

impl FakeBackend {
    pub fn new(devices: Vec<FakeDevice>) -> Self {
        Self { devices }
    }
}

impl RemoteBackend for FakeBackend {
    fn list(&mut self) -> Result<Vec<RemoteDescriptor>, Error> {
        Ok(self
            .devices
            .iter()
            .map(|device| device.descriptor.clone())
            .collect())
    }

    fn open(&mut self, selector: &RemoteSelector) -> Result<Box<dyn RemoteSession>, Error> {
        let device = self
            .devices
            .iter()
            .find(|device| {
                device.descriptor.bus == selector.bus
                    && device.descriptor.address == selector.address
            })
            .ok_or_else(|| {
                Error::Device(format!(
                    "device {}:{} not found",
                    selector.bus, selector.address
                ))
            })?;
        Ok(Box::new(FakeSession {
            sysinfo: device.sysinfo.clone(),
            usercfg: device.usercfg.clone(),
            files: HashMap::new(),
            next_handle: 1,
        }))
    }
}

struct FakeFile {
    data: Vec<u8>,
    offset: usize,
}

struct FakeSession {
    sysinfo: Vec<u8>,
    usercfg: Vec<u8>,
    files: HashMap<u8, FakeFile>,
    next_handle: u8,
}

impl RemoteSession for FakeSession {
    fn reset_filesystem(&mut self) -> Result<(), Error> {
        Ok(())
    }

    fn open_read(&mut self, path: &str) -> Result<OpenRead, Error> {
        let data = match path {
            "/sys/sysinfo" => self.sysinfo.clone(),
            "/cfg/usercfg" => self.usercfg.clone(),
            other => return Err(Error::Protocol(format!("no such file: {other}"))),
        };
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        let size = data.len() as u32;
        self.files.insert(handle, FakeFile { data, offset: 0 });
        Ok(OpenRead { handle, size })
    }

    fn read(&mut self, handle: u8, packets: u16) -> Result<Vec<u8>, Error> {
        let file = self
            .files
            .get_mut(&handle)
            .ok_or_else(|| Error::Protocol(format!("invalid handle {handle}")))?;
        let maximum = (packets as usize).saturating_mul(62);
        let end = (file.offset + maximum).min(file.data.len());
        let chunk = file.data[file.offset..end].to_vec();
        file.offset = end;
        Ok(chunk)
    }

    fn open_write(&mut self, _path: &str, _size: u32) -> Result<u8, Error> {
        Err(Error::Protocol(
            "write is not implemented in the fake Remote".into(),
        ))
    }

    fn write_chunk(&mut self, _handle: u8, _data: &[u8], _last: bool) -> Result<(), Error> {
        Err(Error::Protocol(
            "write is not implemented in the fake Remote".into(),
        ))
    }

    fn check(&mut self, _handle: u8, _expected: u16, _covered_len: u32) -> Result<bool, Error> {
        Err(Error::Protocol(
            "check is not implemented in the fake Remote".into(),
        ))
    }

    fn flush(&mut self, _handle: u8) -> Result<(), Error> {
        Err(Error::Protocol(
            "flush is not implemented in the fake Remote".into(),
        ))
    }

    fn close(&mut self, handle: u8) -> Result<(), Error> {
        self.files.remove(&handle);
        Ok(())
    }

    fn reboot(&mut self) -> Result<(), Error> {
        Ok(())
    }
}
