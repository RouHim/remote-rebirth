//! IR Sources and IR Records.
//!
//! This module is the public facade over the IR codec: the shared types (`ir_types`), record
//! encoding (`ir_record`), the three importers (`ir_pronto`, `ir_lirc`, `ir_flipper`) and the six
//! protocol encoders (`ir_encode`).
//!
//! An [`IrSource`] names a user-supplied file and selector that supplies one
//! command's timings or protocol fields. Timing-bearing sources become class-1
//! pulse records directly; sources that carry protocol fields are encoded with
//! one of the six protocol encoders. [`IrRecord`] is the stored form: carrier,
//! class, and pointer groups of mark/space duration words.

// Production reaches the IR codec through this facade: `compose` and `compose_build` name these.
pub use crate::ir_types::{ImportedCommand, IrRecord, IrSource, IrSourceKind, Provenance, import};

// Codec internals that only the codec tests name.
#[cfg(test)]
pub use crate::ir_record::pack_block;
#[cfg(test)]
pub use crate::ir_types::{CLASS_STREAM, DURATION_MASK, IrError, MARK_BIT, Pulse};
