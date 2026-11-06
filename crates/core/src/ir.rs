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

pub use crate::ir_record::pack_block;
pub use crate::ir_types::{
    BLOCK_TERMINATOR, CLASS_STREAM, DURATION_MASK, EncodedIrRecord, ImportedCommand, IrError,
    IrPointerGroup, IrRecord, IrSource, IrSourceKind, LEAD_IN_US, MARK_BIT, MAX_DURATION_US,
    MAX_POINTER_GROUPS, Provenance, Pulse, RECORD_POINTER_BIAS, import,
};
