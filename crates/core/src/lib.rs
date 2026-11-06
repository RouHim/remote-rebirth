mod architecture;
mod backup;
#[cfg(feature = "cli")]
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
mod hid;
#[cfg(feature = "cli")]
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
pub mod model;
mod paths;
pub mod profile;
mod progress;
mod protocol;
mod restore;
#[cfg(feature = "cli")]
mod runtime;
mod sections;
mod sysinfo;
mod tables;
mod transport;
mod write;

#[cfg(test)]
mod architecture_tests;
#[cfg(test)]
mod compose_tests;
#[cfg(test)]
mod empty_tests;
#[cfg(test)]
mod gui_api_tests;
#[cfg(all(test, feature = "cli"))]
mod inspect_tests;
#[cfg(test)]
mod ir_tests;
#[cfg(test)]
mod tables_tests;
#[cfg(all(test, feature = "cli"))]
mod tests;

#[cfg(feature = "cli")]
pub use cli::run;
pub use clock::{Clock, SystemClock};
pub use empty::emit_empty;
pub use error::{Error, Exit};
pub use hid::HidBackend;
pub use paths::state_dir;
pub use progress::Progress;
#[cfg(feature = "cli")]
pub use runtime::Runtime;
pub use transport::{OpenRead, RemoteBackend, RemoteDescriptor, RemoteSelector, RemoteSession};

/// The core operations the GUI calls. Added by the GUI feature; the CLI keeps its own adapter.
pub mod gui_api;
