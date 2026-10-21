//! The header variables (ODA spec §9): one bit-coded stream of every
//! system variable, read whole into a table of named values. The layout
//! is a list of fields with the versions each is present in; a variable
//! the later issues need is reached by name.

use crate::bits::{BitReader, Color, Handle};
use crate::crc::crc16;
use crate::objects::string_stream;
use crate::sections::{File, SectionName};
use crate::{DwgError, Version};

pub(crate) const SENTINEL_BEGIN: [u8; 16] = [
    0xCF, 0x7B, 0x1F, 0x23, 0xFD, 0xDE, 0x38, 0xA9, 0x5F, 0x7C, 0x68, 0xB8, 0x4E, 0x6D, 0x33, 0x5F,
];

/// A header variable's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(bool),
    Int(i64),
    Real(f64),
    Point([f64; 3]),
    Text(String),
    Handle(Handle),
    Color(Color),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    B,
    Rc,
    Bs,
    Bl,
    Bd,
    Bd3,
    Rd2,
    Tv,
    /// A handle: in the handle stream for R2007, inline before.
    H,
    /// A handle always in the data stream: `HANDSEED` (§9).
    HData,
    /// `CPSNID`, present only when `CEPSNTYPE` is 3.
    HPlotStyle,
    Cmc,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum In {
    All,
    From(Version),
    Until(Version),
}

impl In {
    pub(crate) fn holds(self, v: Version) -> bool {
        match self {
            Self::All => true,
            Self::From(from) => v >= from,
            Self::Until(until) => v <= until,
        }
    }
}

use In::{All, From, Until};
use Kind::{B, Bd, Bd3, Bl, Bs, Cmc, H, HData, HPlotStyle, Rc, Rd2, Tv};
use Version::{R2004, R2007};

/// The sequence of §9 for R2000 through R2007. Fields the specification
/// leaves unnamed are named by their kind and position.
pub(crate) const LAYOUT: &[(&str, Kind, In)] = &[
    ("unknown_bd_1", Bd, All),
    ("unknown_bd_2", Bd, All),
    ("unknown_bd_3", Bd, All),
    ("unknown_bd_4", Bd, All),
    ("unknown_text_1", Tv, All),
    ("unknown_text_2", Tv, All),
    ("unknown_text_3", Tv, All),
    ("unknown_text_4", Tv, All),
    ("unknown_bl_1", Bl, All),
    ("unknown_bl_2", Bl, All),
    ("CURRENT_VIEWPORT_ENTITY_HEADER", H, Until(Version::R2000)),
    ("DIMASO", B, All),
    ("DIMSHO", B, All),
    ("PLINEGEN", B, All),
    ("ORTHOMODE", B, All),
    ("REGENMODE", B, All),
    ("FILLMODE", B, All),
    ("QTEXTMODE", B, All),
    ("PSLTSCALE", B, All),
    ("LIMCHECK", B, All),
    ("undocumented_b", B, From(R2004)),
    ("USRTIMER", B, All),
    ("SKPOLY", B, All),
    ("ANGDIR", B, All),
    ("SPLFRAME", B, All),
    ("MIRRTEXT", B, All),
    ("WORLDVIEW", B, All),
    ("TILEMODE", B, All),
    ("PLIMCHECK", B, All),
    ("VISRETAIN", B, All),
    ("DISPSILH", B, All),
    ("PELLIPSE", B, All),
    ("PROXYGRAPHICS", Bs, All),
    ("TREEDEPTH", Bs, All),
    ("LUNITS", Bs, All),
    ("LUPREC", Bs, All),
    ("AUNITS", Bs, All),
    ("AUPREC", Bs, All),
    ("ATTMODE", Bs, All),
    ("PDMODE", Bs, All),
    ("unknown_2004_bl_1", Bl, From(R2004)),
    ("unknown_2004_bl_2", Bl, From(R2004)),
    ("unknown_2004_bl_3", Bl, From(R2004)),
    ("USERI1", Bs, All),
    ("USERI2", Bs, All),
    ("USERI3", Bs, All),
    ("USERI4", Bs, All),
    ("USERI5", Bs, All),
    ("SPLINESEGS", Bs, All),
    ("SURFU", Bs, All),
    ("SURFV", Bs, All),
    ("SURFTYPE", Bs, All),
    ("SURFTAB1", Bs, All),
    ("SURFTAB2", Bs, All),
    ("SPLINETYPE", Bs, All),
    ("SHADEDGE", Bs, All),
    ("SHADEDIF", Bs, All),
    ("UNITMODE", Bs, All),
    ("MAXACTVP", Bs, All),
    ("ISOLINES", Bs, All),
    ("CMLJUST", Bs, All),
    ("TEXTQLTY", Bs, All),
    ("LTSCALE", Bd, All),
    ("TEXTSIZE", Bd, All),
    ("TRACEWID", Bd, All),
    ("SKETCHINC", Bd, All),
    ("FILLETRAD", Bd, All),
    ("THICKNESS", Bd, All),
    ("ANGBASE", Bd, All),
    ("PDSIZE", Bd, All),
    ("PLINEWID", Bd, All),
    ("USERR1", Bd, All),
    ("USERR2", Bd, All),
    ("USERR3", Bd, All),
    ("USERR4", Bd, All),
    ("USERR5", Bd, All),
    ("CHAMFERA", Bd, All),
    ("CHAMFERB", Bd, All),
    ("CHAMFERC", Bd, All),
    ("CHAMFERD", Bd, All),
    ("FACETRES", Bd, All),
    ("CMLSCALE", Bd, All),
    ("CELTSCALE", Bd, All),
    ("MENUNAME", Tv, Until(R2004)),
    ("TDCREATE_DAY", Bl, All),
    ("TDCREATE_MS", Bl, All),
    ("TDUPDATE_DAY", Bl, All),
    ("TDUPDATE_MS", Bl, All),
    ("unknown_2004_bl_4", Bl, From(R2004)),
    ("unknown_2004_bl_5", Bl, From(R2004)),
    ("unknown_2004_bl_6", Bl, From(R2004)),
    ("TDINDWG_DAY", Bl, All),
    ("TDINDWG_MS", Bl, All),
    ("TDUSRTIMER_DAY", Bl, All),
    ("TDUSRTIMER_MS", Bl, All),
    ("CECOLOR", Cmc, All),
    ("HANDSEED", HData, All),
    ("CLAYER", H, All),
    ("TEXTSTYLE", H, All),
    ("CELTYPE", H, All),
    ("CMATERIAL", H, From(R2007)),
    ("DIMSTYLE", H, All),
    ("CMLSTYLE", H, All),
    ("PSVPSCALE", Bd, All),
    ("PINSBASE", Bd3, All),
    ("PEXTMIN", Bd3, All),
    ("PEXTMAX", Bd3, All),
    ("PLIMMIN", Rd2, All),
    ("PLIMMAX", Rd2, All),
    ("PELEVATION", Bd, All),
    ("PUCSORG", Bd3, All),
    ("PUCSXDIR", Bd3, All),
    ("PUCSYDIR", Bd3, All),
    ("PUCSNAME", H, All),
    ("PUCSORTHOREF", H, All),
    ("PUCSORTHOVIEW", Bs, All),
    ("PUCSBASE", H, All),
    ("PUCSORGTOP", Bd3, All),
    ("PUCSORGBOTTOM", Bd3, All),
    ("PUCSORGLEFT", Bd3, All),
    ("PUCSORGRIGHT", Bd3, All),
    ("PUCSORGFRONT", Bd3, All),
    ("PUCSORGBACK", Bd3, All),
    ("INSBASE", Bd3, All),
    ("EXTMIN", Bd3, All),
    ("EXTMAX", Bd3, All),
    ("LIMMIN", Rd2, All),
    ("LIMMAX", Rd2, All),
    ("ELEVATION", Bd, All),
    ("UCSORG", Bd3, All),
    ("UCSXDIR", Bd3, All),
    ("UCSYDIR", Bd3, All),
    ("UCSNAME", H, All),
    ("UCSORTHOREF", H, All),
    ("UCSORTHOVIEW", Bs, All),
    ("UCSBASE", H, All),
    ("UCSORGTOP", Bd3, All),
    ("UCSORGBOTTOM", Bd3, All),
    ("UCSORGLEFT", Bd3, All),
    ("UCSORGRIGHT", Bd3, All),
    ("UCSORGFRONT", Bd3, All),
    ("UCSORGBACK", Bd3, All),
    ("DIMPOST", Tv, All),
    ("DIMAPOST", Tv, All),
    ("DIMSCALE", Bd, All),
    ("DIMASZ", Bd, All),
    ("DIMEXO", Bd, All),
    ("DIMDLI", Bd, All),
    ("DIMEXE", Bd, All),
    ("DIMRND", Bd, All),
    ("DIMDLE", Bd, All),
    ("DIMTP", Bd, All),
    ("DIMTM", Bd, All),
    ("DIMFXL", Bd, From(R2007)),
    ("DIMJOGANG", Bd, From(R2007)),
    ("DIMTFILL", Bs, From(R2007)),
    ("DIMTFILLCLR", Cmc, From(R2007)),
    ("DIMTOL", B, All),
    ("DIMLIM", B, All),
    ("DIMTIH", B, All),
    ("DIMTOH", B, All),
    ("DIMSE1", B, All),
    ("DIMSE2", B, All),
    ("DIMTAD", Bs, All),
    ("DIMZIN", Bs, All),
    ("DIMAZIN", Bs, All),
    ("DIMARCSYM", Bs, From(R2007)),
    ("DIMTXT", Bd, All),
    ("DIMCEN", Bd, All),
    ("DIMTSZ", Bd, All),
    ("DIMALTF", Bd, All),
    ("DIMLFAC", Bd, All),
    ("DIMTVP", Bd, All),
    ("DIMTFAC", Bd, All),
    ("DIMGAP", Bd, All),
    ("DIMALTRND", Bd, All),
    ("DIMALT", B, All),
    ("DIMALTD", Bs, All),
    ("DIMTOFL", B, All),
    ("DIMSAH", B, All),
    ("DIMTIX", B, All),
    ("DIMSOXD", B, All),
    ("DIMCLRD", Cmc, All),
    ("DIMCLRE", Cmc, All),
    ("DIMCLRT", Cmc, All),
    ("DIMADEC", Bs, All),
    ("DIMDEC", Bs, All),
    ("DIMTDEC", Bs, All),
    ("DIMALTU", Bs, All),
    ("DIMALTTD", Bs, All),
    ("DIMAUNIT", Bs, All),
    ("DIMFRAC", Bs, All),
    ("DIMLUNIT", Bs, All),
    ("DIMDSEP", Bs, All),
    ("DIMTMOVE", Bs, All),
    ("DIMJUST", Bs, All),
    ("DIMSD1", B, All),
    ("DIMSD2", B, All),
    ("DIMTOLJ", Bs, All),
    ("DIMTZIN", Bs, All),
    ("DIMALTZ", Bs, All),
    ("DIMALTTZ", Bs, All),
    ("DIMUPT", B, All),
    ("DIMATFIT", Bs, All),
    ("DIMFXLON", B, From(R2007)),
    ("DIMTXSTY", H, All),
    ("DIMLDRBLK", H, All),
    ("DIMBLK", H, All),
    ("DIMBLK1", H, All),
    ("DIMBLK2", H, All),
    ("DIMLTYPE", H, From(R2007)),
    ("DIMLTEX1", H, From(R2007)),
    ("DIMLTEX2", H, From(R2007)),
    ("DIMLWD", Bs, All),
    ("DIMLWE", Bs, All),
    ("BLOCK_CONTROL", H, All),
    ("LAYER_CONTROL", H, All),
    ("STYLE_CONTROL", H, All),
    ("LTYPE_CONTROL", H, All),
    ("VIEW_CONTROL", H, All),
    ("UCS_CONTROL", H, All),
    ("VPORT_CONTROL", H, All),
    ("APPID_CONTROL", H, All),
    ("DIMSTYLE_CONTROL", H, All),
    ("VIEWPORT_ENTITY_HEADER_CONTROL", H, Until(Version::R2000)),
    ("DICTIONARY_ACAD_GROUP", H, All),
    ("DICTIONARY_ACAD_MLINESTYLE", H, All),
    ("DICTIONARY_NAMED_OBJECTS", H, All),
    ("TSTACKALIGN", Bs, All),
    ("TSTACKSIZE", Bs, All),
    ("HYPERLINKBASE", Tv, All),
    ("STYLESHEET", Tv, All),
    ("DICTIONARY_LAYOUTS", H, All),
    ("DICTIONARY_PLOTSETTINGS", H, All),
    ("DICTIONARY_PLOTSTYLES", H, All),
    ("DICTIONARY_MATERIALS", H, From(R2004)),
    ("DICTIONARY_COLORS", H, From(R2004)),
    ("DICTIONARY_VISUALSTYLE", H, From(R2007)),
    ("FLAGS", Bl, All),
    ("INSUNITS", Bs, All),
    ("CEPSNTYPE", Bs, All),
    ("CPSNID", HPlotStyle, All),
    ("FINGERPRINTGUID", Tv, All),
    ("VERSIONGUID", Tv, All),
    ("SORTENTS", Rc, From(R2004)),
    ("INDEXCTL", Rc, From(R2004)),
    ("HIDETEXT", Rc, From(R2004)),
    ("XCLIPFRAME", Rc, From(R2004)),
    ("DIMASSOC", Rc, From(R2004)),
    ("HALOGAP", Rc, From(R2004)),
    ("OBSCUREDCOLOR", Bs, From(R2004)),
    ("INTERSECTIONCOLOR", Bs, From(R2004)),
    ("OBSCUREDLTYPE", Rc, From(R2004)),
    ("INTERSECTIONDISPLAY", Rc, From(R2004)),
    ("PROJECTNAME", Tv, From(R2004)),
    ("BLOCK_RECORD_PAPER_SPACE", H, All),
    ("BLOCK_RECORD_MODEL_SPACE", H, All),
    ("LTYPE_BYLAYER", H, All),
    ("LTYPE_BYBLOCK", H, All),
    ("LTYPE_CONTINUOUS", H, All),
    ("CAMERADISPLAY", B, From(R2007)),
    ("unknown_2007_bl_1", Bl, From(R2007)),
    ("unknown_2007_bl_2", Bl, From(R2007)),
    ("unknown_2007_bd_1", Bd, From(R2007)),
    ("STEPSPERSEC", Bd, From(R2007)),
    ("STEPSIZE", Bd, From(R2007)),
    ("3DDWFPREC", Bd, From(R2007)),
    ("LENSLENGTH", Bd, From(R2007)),
    ("CAMERAHEIGHT", Bd, From(R2007)),
    ("SOLIDHIST", Rc, From(R2007)),
    ("SHOWHIST", Rc, From(R2007)),
    ("PSOLWIDTH", Bd, From(R2007)),
    ("PSOLHEIGHT", Bd, From(R2007)),
    ("LOFTANG1", Bd, From(R2007)),
    ("LOFTANG2", Bd, From(R2007)),
    ("LOFTMAG1", Bd, From(R2007)),
    ("LOFTMAG2", Bd, From(R2007)),
    ("LOFTPARAM", Bs, From(R2007)),
    ("LOFTNORMALS", Rc, From(R2007)),
    ("LATITUDE", Bd, From(R2007)),
    ("LONGITUDE", Bd, From(R2007)),
    ("NORTHDIRECTION", Bd, From(R2007)),
    ("TIMEZONE", Bl, From(R2007)),
    ("LIGHTGLYPHDISPLAY", Rc, From(R2007)),
    ("TILEMODELIGHTSYNCH", Rc, From(R2007)),
    ("DWFFRAME", Rc, From(R2007)),
    ("DGNFRAME", Rc, From(R2007)),
    ("unknown_2007_b", B, From(R2007)),
    ("INTERFERECOLOR", Cmc, From(R2007)),
    ("INTERFEREOBJVS", H, From(R2007)),
    ("INTERFEREVPVS", H, From(R2007)),
    ("DRAGVS", H, From(R2007)),
    ("CSHADOW", Rc, From(R2007)),
    ("unknown_2007_bd_2", Bd, From(R2007)),
];

/// The header variables, by name, in the file's order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Header {
    values: Vec<(&'static str, Value)>,
    /// Whether the section's CRC matched (§9).
    pub crc_matches: bool,
}

impl Header {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.values.iter().find(|(n, _)| *n == name).map(|(_, v)| v)
    }

    pub fn handle(&self, name: &str) -> Option<Handle> {
        match self.get(name)? {
            Value::Handle(h) => Some(*h),
            _ => None,
        }
    }

    pub fn int(&self, name: &str) -> Option<i64> {
        match self.get(name)? {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(i64::from(*b)),
            _ => None,
        }
    }

    pub fn real(&self, name: &str) -> Option<f64> {
        match self.get(name)? {
            Value::Real(r) => Some(*r),
            _ => None,
        }
    }

    pub fn text(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    pub fn color(&self, name: &str) -> Option<Color> {
        match self.get(name)? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    /// Every variable, in order.
    pub fn values(&self) -> &[(&'static str, Value)] {
        &self.values
    }
}

impl File<'_> {
    /// Reads the header variables.
    pub fn header(&self) -> Result<Header, DwgError> {
        let section = self
            .section(SectionName::Header)
            .ok_or(DwgError::MissingSection {
                name: "AcDb:Header",
            })?;
        let at = section
            .windows(16)
            .take(64)
            .position(|w| w == SENTINEL_BEGIN)
            .ok_or(DwgError::Corrupt {
                what: "the header sentinel",
                at: 0,
            })?;
        let section = &section[at..];
        let version = self.version();
        let limits = self.limits();
        let full = BitReader::new(section);
        let mut data = full.window(16 * 8, full.end())?;
        let size = data.rl()? as usize;
        let end = (20 + size) * 8;
        if end > full.end() {
            return Err(DwgError::Truncated {
                what: "the header section",
                at: 20,
            });
        }
        // §9: the CRC from after the sentinel, seeded 0xC0C1.
        let crc_matches = section
            .get(20 + size..22 + size)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            == Some(crc16(0xC0C1, &section[16..20 + size]));
        let mut strings = None;
        let mut handles = None;
        if version >= Version::R2007 {
            let bitsize = data.rl()? as usize;
            strings = string_stream(&full, bitsize + 160)?;
            handles = Some(full.window(bitsize + 160, end)?);
        }
        let mut data = full.window(data.position(), end)?;
        let mut values: Vec<(&'static str, Value)> = Vec::with_capacity(LAYOUT.len());
        for &(name, kind, presence) in LAYOUT {
            if !presence.holds(version) {
                continue;
            }
            let value = match kind {
                B => Value::Bool(data.bit()? == 1),
                Rc => Value::Int(i64::from(data.rc()?)),
                Bs => Value::Int(i64::from(data.bs()?)),
                Bl => Value::Int(i64::from(data.bl()?)),
                Bd => Value::Real(data.bd()?),
                Bd3 => {
                    let (p, z) = data.bd3()?;
                    Value::Point([p.x, p.y, z])
                }
                Rd2 => {
                    let p = data.rd2()?;
                    Value::Point([p.x, p.y, 0.0])
                }
                Tv => Value::Text(match strings.as_mut() {
                    Some(s) => s.text_unicode(limits)?,
                    None => String::from_utf8_lossy(&data.text(limits)?).into_owned(),
                }),
                HData => Value::Handle(data.handle()?),
                H => Value::Handle(match handles.as_mut() {
                    Some(h) => h.handle()?,
                    None => data.handle()?,
                }),
                HPlotStyle => {
                    let kind = values
                        .iter()
                        .rev()
                        .find(|(n, _)| *n == "CEPSNTYPE")
                        .and_then(|(_, v)| match v {
                            Value::Int(i) => Some(*i),
                            _ => None,
                        });
                    if kind != Some(3) {
                        continue;
                    }
                    Value::Handle(match handles.as_mut() {
                        Some(h) => h.handle()?,
                        None => data.handle()?,
                    })
                }
                Cmc => Value::Color(data.cmc(version, limits)?),
            };
            values.push((name, value));
        }
        Ok(Header {
            values,
            crc_matches,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_layout_names_each_variable_once_and_holds_by_version() {
        let mut names: Vec<&str> = LAYOUT.iter().map(|f| f.0).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), LAYOUT.len(), "a name repeats");
        assert!(Until(Version::R2000).holds(Version::R2000));
        assert!(!Until(Version::R2000).holds(R2004));
        assert!(From(R2004).holds(R2007));
        assert!(!From(R2007).holds(R2004));
        assert!(All.holds(Version::R2000));
    }
}
