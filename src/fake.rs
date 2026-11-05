use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::error::Error;
use crate::filewriter::CHUNK_BYTES;
use crate::transport::{OpenRead, RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession};

pub struct FakeDevice {
    pub descriptor: RemoteDescriptor,
    pub sysinfo: Vec<u8>,
    pub usercfg: Vec<u8>,
}

/// One recorded request or packet exchange, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    Open {
        bus: u8,
        address: u8,
    },
    Reset,
    OpenRead {
        path: String,
    },
    Read {
        handle: u8,
        packets: u16,
    },
    OpenWrite {
        path: String,
        size: u32,
    },
    WriteChunk {
        handle: u8,
        data: Vec<u8>,
        last: bool,
    },
    Check {
        handle: u8,
        expected: u16,
        covered_len: u32,
    },
    Flush {
        handle: u8,
    },
    Close {
        handle: u8,
    },
    Reboot,
}

/// Faults injected into the next session.
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// The checksum check replies `u` instead of `m`.
    pub check_refused: bool,
    /// A Read returns fewer bytes than its open reply announced.
    pub short_read: bool,
    /// A write packet is reported as malformed.
    pub malformed_packet: bool,
    /// The Remote disappears during the write.
    pub disconnect: bool,
    /// The Read-Back returns different bytes than were written.
    pub read_back_mismatch: bool,
}

/// The live state behind one fake Remote.
#[derive(Debug, Default)]
pub struct DeviceStore {
    pub sysinfo: Vec<u8>,
    pub usercfg: Vec<u8>,
    pub faults: Faults,
}

pub struct FakeBackend {
    pub devices: Vec<FakeDevice>,
    stores: Vec<Rc<RefCell<DeviceStore>>>,
    calls: Rc<RefCell<Vec<Call>>>,
}

impl FakeBackend {
    pub fn new(devices: Vec<FakeDevice>) -> Self {
        let stores = devices
            .iter()
            .map(|device| {
                Rc::new(RefCell::new(DeviceStore {
                    sysinfo: device.sysinfo.clone(),
                    usercfg: device.usercfg.clone(),
                    faults: Faults::default(),
                }))
            })
            .collect();
        Self {
            devices,
            stores,
            calls: Rc::new(RefCell::new(Vec::new())),
        }
    }

    pub fn store(&self, index: usize) -> Rc<RefCell<DeviceStore>> {
        Rc::clone(&self.stores[index])
    }

    pub fn calls(&self) -> Rc<RefCell<Vec<Call>>> {
        Rc::clone(&self.calls)
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
        let index = self
            .devices
            .iter()
            .position(|device| {
                device.descriptor.bus == selector.bus
                    && device.descriptor.address == selector.address
            })
            .ok_or_else(|| {
                Error::Device(format!(
                    "device {}:{} not found",
                    selector.bus, selector.address
                ))
            })?;
        self.calls.borrow_mut().push(Call::Open {
            bus: selector.bus,
            address: selector.address,
        });
        Ok(Box::new(FakeSession {
            store: Rc::clone(&self.stores[index]),
            calls: Rc::clone(&self.calls),
            files: HashMap::new(),
            next_handle: 1,
            write: None,
            write_completed: false,
        }))
    }
}

struct FakeFile {
    data: Vec<u8>,
    offset: usize,
}

struct WriteState {
    handle: u8,
    draft: Vec<u8>,
}

struct FakeSession {
    store: Rc<RefCell<DeviceStore>>,
    calls: Rc<RefCell<Vec<Call>>>,
    files: HashMap<u8, FakeFile>,
    next_handle: u8,
    write: Option<WriteState>,
    write_completed: bool,
}

impl FakeSession {
    fn take_handle(&mut self) -> u8 {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.wrapping_add(1);
        handle
    }
}

impl RemoteSession for FakeSession {
    fn reset_filesystem(&mut self) -> Result<(), Error> {
        self.calls.borrow_mut().push(Call::Reset);
        self.files.clear();
        Ok(())
    }

    fn open_read(&mut self, path: &str) -> Result<OpenRead, Error> {
        self.calls.borrow_mut().push(Call::OpenRead {
            path: path.to_string(),
        });
        let (mut data, faults) = {
            let store = self.store.borrow();
            let data = match path {
                "/sys/sysinfo" => store.sysinfo.clone(),
                "/cfg/usercfg" => store.usercfg.clone(),
                other => return Err(Error::Protocol(format!("no such file: {other}"))),
            };
            (data, store.faults.clone())
        };
        let size = data.len() as u32;
        if path == "/cfg/usercfg" && self.write_completed {
            if faults.short_read {
                data.truncate(data.len().saturating_sub(1));
            }
            if faults.read_back_mismatch
                && let Some(first) = data.first_mut()
            {
                *first ^= 0xFF;
            }
        }
        let handle = self.take_handle();
        self.files.insert(handle, FakeFile { data, offset: 0 });
        Ok(OpenRead { handle, size })
    }

    fn read(&mut self, handle: u8, packets: u16) -> Result<Vec<u8>, Error> {
        self.calls.borrow_mut().push(Call::Read { handle, packets });
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

    fn open_write(&mut self, path: &str, size: u32) -> Result<u8, Error> {
        self.calls.borrow_mut().push(Call::OpenWrite {
            path: path.to_string(),
            size,
        });
        if path != "/cfg/usercfg" {
            return Err(Error::Protocol(format!(
                "the fake Remote refuses to open {path} for writing"
            )));
        }
        self.store.borrow_mut().usercfg = vec![0xFF; size as usize];
        let handle = self.take_handle();
        self.write = Some(WriteState {
            handle,
            draft: Vec::with_capacity(size as usize),
        });
        Ok(handle)
    }

    fn write_chunk(&mut self, handle: u8, data: &[u8], last: bool) -> Result<(), Error> {
        self.calls.borrow_mut().push(Call::WriteChunk {
            handle,
            data: data.to_vec(),
            last,
        });
        let faults = self.store.borrow().faults.clone();
        if faults.disconnect {
            return Err(Error::Protocol(
                "the fake Remote disconnected during the write".into(),
            ));
        }
        if faults.malformed_packet {
            return Err(Error::Protocol(
                "the fake Remote sent a malformed packet".into(),
            ));
        }
        if data.is_empty() {
            return Err(Error::Protocol("empty write chunk".into()));
        }
        if data.len() > CHUNK_BYTES {
            return Err(Error::Protocol(format!(
                "write chunk carries {} bytes; at most {CHUNK_BYTES} are allowed",
                data.len()
            )));
        }
        let write = self
            .write
            .as_mut()
            .ok_or_else(|| Error::Protocol(format!("no write is open for handle {handle}")))?;
        if write.handle != handle {
            return Err(Error::Protocol(format!(
                "handle {handle} does not match the open write {}",
                write.handle
            )));
        }
        write.draft.extend_from_slice(data);
        Ok(())
    }

    fn check(&mut self, handle: u8, expected: u16, covered_len: u32) -> Result<bool, Error> {
        self.calls.borrow_mut().push(Call::Check {
            handle,
            expected,
            covered_len,
        });
        if self.store.borrow().faults.check_refused {
            return Ok(false);
        }
        let write = self
            .write
            .as_ref()
            .ok_or_else(|| Error::Protocol(format!("no write is open for handle {handle}")))?;
        if write.handle != handle {
            return Err(Error::Protocol(format!(
                "handle {handle} does not match the open write {}",
                write.handle
            )));
        }
        let covered = covered_len as usize;
        if covered > write.draft.len() {
            return Err(Error::Protocol(format!(
                "checksum check covers {covered} bytes but only {} were written",
                write.draft.len()
            )));
        }
        let mut accumulator = 0x4321u16;
        for word in write.draft[..covered].as_chunks::<2>().0 {
            accumulator ^= u16::from_le_bytes(*word);
        }
        Ok(accumulator == expected)
    }

    fn flush(&mut self, handle: u8) -> Result<(), Error> {
        self.calls.borrow_mut().push(Call::Flush { handle });
        let draft = {
            let write = self
                .write
                .as_mut()
                .ok_or_else(|| Error::Protocol(format!("no write is open for handle {handle}")))?;
            if write.handle != handle {
                return Err(Error::Protocol(format!(
                    "handle {handle} does not match the open write {}",
                    write.handle
                )));
            }
            write.draft.clone()
        };
        self.store.borrow_mut().usercfg = draft;
        self.write_completed = true;
        Ok(())
    }

    fn close(&mut self, handle: u8) -> Result<(), Error> {
        self.calls.borrow_mut().push(Call::Close { handle });
        self.files.remove(&handle);
        if self
            .write
            .as_ref()
            .is_some_and(|write| write.handle == handle)
        {
            self.write = None;
        }
        Ok(())
    }

    fn reboot(&mut self) -> Result<(), Error> {
        self.calls.borrow_mut().push(Call::Reboot);
        Ok(())
    }
}
