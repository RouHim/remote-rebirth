use std::io::Write;
use std::path::PathBuf;

use crate::clock::Clock;
use crate::transport::RemoteBackend;

pub struct Runtime<'a> {
    pub backend: &'a mut dyn RemoteBackend,
    pub clock: &'a dyn Clock,
    pub stdout: &'a mut dyn Write,
    pub stderr: &'a mut dyn Write,
    pub state_dir: PathBuf,
}
