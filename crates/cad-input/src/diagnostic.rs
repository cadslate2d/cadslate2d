//! Parser diagnostics.

use std::fmt;

/// Why a piece of input was rejected, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// A short message suitable for the command line.
    pub message: String,
    /// Byte offset into the input text where the problem starts.
    pub position: usize,
    /// The text was the right kind of thing with one thing wrong, and the
    /// message says exactly what — an unknown unit, say. Such a message is
    /// shown as it is, where a general one gives way to a reminder of what
    /// the prompt accepts.
    pub(crate) specific: bool,
}

impl Diagnostic {
    pub(crate) fn new(message: impl Into<String>, position: usize) -> Self {
        Self {
            message: message.into(),
            position,
            specific: false,
        }
    }

    /// A diagnostic that says exactly what is wrong; see [`Self::specific`].
    pub(crate) fn specific(message: impl Into<String>, position: usize) -> Self {
        Self {
            specific: true,
            ..Self::new(message, position)
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Diagnostic {}
