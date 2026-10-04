//! Import errors.

use std::fmt;

/// Why a file could not be imported. Line numbers are 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DxfError {
    /// The file is a binary DXF, which is not supported.
    BinaryDxf,
    /// The file exceeds the configured size limit.
    FileTooLarge { bytes: usize, limit: usize },
    /// The file holds more supported entities than the configured limit.
    TooManyEntities { limit: usize },
    /// A group code line is not an integer.
    InvalidGroupCode { line: usize },
    /// The file ended in the middle of a group, an entity, or a section.
    UnexpectedEnd { line: usize },
    /// A coordinate is not a finite number.
    InvalidNumber { line: usize, group: i32 },
    /// An entity lacks a coordinate it must have.
    MissingCoordinate {
        line: usize,
        entity: &'static str,
        group: i32,
    },
}

impl fmt::Display for DxfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BinaryDxf => write!(f, "binary DXF files are not supported; save as ASCII DXF"),
            Self::FileTooLarge { bytes, limit } => {
                write!(f, "the file is {bytes} bytes, over the {limit}-byte limit")
            }
            Self::TooManyEntities { limit } => {
                write!(f, "the file has more than {limit} entities")
            }
            Self::InvalidGroupCode { line } => {
                write!(f, "line {line}: expected a DXF group code")
            }
            Self::UnexpectedEnd { line } => {
                write!(f, "line {line}: the file ends unexpectedly")
            }
            Self::InvalidNumber { line, group } => {
                write!(f, "line {line}: group {group} is not a valid number")
            }
            Self::MissingCoordinate {
                line,
                entity,
                group,
            } => {
                write!(f, "line {line}: {entity} is missing group {group}")
            }
        }
    }
}

impl std::error::Error for DxfError {}
