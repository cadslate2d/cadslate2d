//! The DWG releases this crate reads and writes.

use crate::DwgError;

/// A DWG format by the AutoCAD release that introduced it (ODA spec §3.2.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Version {
    /// `AC1015`: AutoCAD 2000, 2000i, and 2002.
    R2000,
    /// `AC1018`: AutoCAD 2004, 2005, and 2006.
    R2004,
    /// `AC1021`: AutoCAD 2007, 2008, and 2009.
    R2007,
}

impl Version {
    /// The six bytes a file of this version starts with.
    pub fn id(self) -> &'static str {
        match self {
            Self::R2000 => "AC1015",
            Self::R2004 => "AC1018",
            Self::R2007 => "AC1021",
        }
    }

    /// The version a file's first bytes announce.
    pub fn detect(bytes: &[u8]) -> Result<Self, DwgError> {
        let Some(id) = bytes.get(..6) else {
            return Err(DwgError::NotDwg);
        };
        match id {
            b"AC1015" => Ok(Self::R2000),
            b"AC1018" => Ok(Self::R2004),
            b"AC1021" => Ok(Self::R2007),
            other if other.starts_with(b"AC10") && other.iter().all(u8::is_ascii_alphanumeric) => {
                Err(DwgError::UnsupportedVersion {
                    version: String::from_utf8_lossy(other).into_owned(),
                })
            }
            _ => Err(DwgError::NotDwg),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_three_versions_are_told_apart_and_the_rest_refused() {
        assert_eq!(Version::detect(b"AC1015\0\0"), Ok(Version::R2000));
        assert_eq!(Version::detect(b"AC1018\0\0"), Ok(Version::R2004));
        assert_eq!(Version::detect(b"AC1021\0\0"), Ok(Version::R2007));
        assert_eq!(
            Version::detect(b"AC1024\0"),
            Err(DwgError::UnsupportedVersion {
                version: "AC1024".into()
            })
        );
        assert_eq!(
            Version::detect(b"AC1009"),
            Err(DwgError::UnsupportedVersion {
                version: "AC1009".into()
            })
        );
        assert_eq!(Version::detect(b"  0\nSECTION"), Err(DwgError::NotDwg));
        assert_eq!(Version::detect(b"AC10"), Err(DwgError::NotDwg));
        assert_eq!(Version::detect(b"AC10\xff\xff"), Err(DwgError::NotDwg));
        assert_eq!(Version::R2007.id(), "AC1021");
    }
}
