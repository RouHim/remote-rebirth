//! Progress events shared by the CLI adapter and the GUI worker.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// A named stage, such as `detect`, `reset`, `write`, `check` or `read-back`.
    Step(&'static str),
    /// Bytes acknowledged by the Remote within the current write.
    Written { bytes: usize, total: usize },
}

/// A sink for callers that do not display progress.
#[cfg(feature = "cli")]
pub fn ignore(_: Progress) {}
