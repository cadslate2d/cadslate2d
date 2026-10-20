//! Why a DWG file could not be read.

use std::fmt;

/// A structured refusal. Positions are byte offsets: in the file for the
/// containers, in the decoded section for what lies inside one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DwgError {
    /// The first six bytes are not a DWG version id.
    NotDwg,
    /// A DWG of a release this crate does not read.
    UnsupportedVersion { version: String },
    /// The file exceeds the configured size limit.
    FileTooLarge { bytes: usize, limit: usize },
    /// The file ends before `what` is complete.
    Truncated { what: &'static str, at: usize },
    /// A stored checksum or CRC does not match the data it covers.
    Checksum { what: &'static str, at: usize },
    /// A structure is not what the specification says it is.
    Corrupt { what: &'static str, at: usize },
    /// A count claimed by the file is past the configured limit.
    TooMany { what: &'static str, limit: usize },
    /// A section the specification requires is missing.
    MissingSection { name: &'static str },
    /// The records could not be held as a drawing: the DXF reader's
    /// refusal, with its reason.
    Drawing(cad_dxf::DxfError),
}

impl fmt::Display for DwgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotDwg => write!(f, "the file is not a DWG: it does not start with AC10"),
            Self::UnsupportedVersion { version } => write!(
                f,
                "DWG version {version} is not supported; AutoCAD 2000, 2004, and 2007/2008 (AC1015, AC1018, AC1021) are"
            ),
            Self::FileTooLarge { bytes, limit } => {
                write!(f, "the file is {bytes} bytes, over the {limit}-byte limit")
            }
            Self::Truncated { what, at } => {
                write!(f, "the file ends inside {what} at byte {at}")
            }
            Self::Checksum { what, at } => {
                write!(f, "{what} at byte {at} fails its checksum")
            }
            Self::Corrupt { what, at } => write!(f, "{what} at byte {at} is corrupt"),
            Self::TooMany { what, limit } => {
                write!(f, "the file claims more than {limit} {what}")
            }
            Self::MissingSection { name } => write!(f, "the file has no {name} section"),
            Self::Drawing(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for DwgError {}
