mod architecture;
mod backup;
mod cli;
mod clock;
mod compose;
mod compose_build;
mod container;
mod detect;
mod empty;
mod error;
#[cfg(test)]
mod fake;
mod filewriter;
mod frame;
mod inspect;
mod inspect_container;
pub mod ir;
mod ir_encode;
mod ir_flipper;
mod ir_lirc;
mod ir_pronto;
mod ir_record;
mod ir_types;
mod journal;
mod model;
mod paths;
mod profile;
mod protocol;
mod restore;
mod runtime;
mod sections;
mod sysinfo;
mod tables;
mod transport;
mod usb;
mod write;

#[cfg(test)]
mod architecture_tests;
#[cfg(test)]
mod compose_tests;
#[cfg(test)]
mod empty_tests;
#[cfg(test)]
mod inspect_tests;
#[cfg(test)]
mod ir_tests;
#[cfg(test)]
mod tables_tests;
#[cfg(test)]
mod tests;

pub use cli::run;
pub use clock::{Clock, SystemClock};
pub use empty::emit_empty;
pub use error::{Error, Exit};
pub use paths::state_dir;
pub use runtime::Runtime;
pub use transport::{OpenRead, RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession};
pub use usb::NusbBackend;
