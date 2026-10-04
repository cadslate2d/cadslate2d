//! Parser diagnostics.

use std::fmt;

/// Why a piece of input was rejected, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// A short message suitable for the command line.
    pub message: String,
    /// Byte offset into the input text where the problem starts.
    pub position: usize,
}

impl Diagnostic {
    pub(crate) fn new(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: message.into(),
            position,
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}
