//! The header variables section written (ODA spec §9): every variable of
//! the version in the order `header::LAYOUT` reads them, the drawing's values
//! where the records give them, a new drawing's otherwise; between the
//! sentinels, with the CRC.

use std::collections::HashMap;

use crate::crc::crc16;
use crate::header::{Kind, LAYOUT, SENTINEL_BEGIN};
use crate::write::bits::BitWriter;
use crate::write::objects::section_streams;
use crate::{Version, header::Value};

const SENTINEL_END: [u8; 16] = [
    0x30, 0x84, 0xE0, 0xDC, 0x02, 0x21, 0xC7, 0x56, 0xA0, 0x83, 0x97, 0x47, 0xB1, 0x92, 0xCC, 0xA0,
];

/// A new drawing's values for the variables that are not zero, false,
/// empty, or null: what AutoCAD 2000 writes into an empty drawing.
const DEFAULTS: &[(&str, Value)] = &[
    ("unknown_bd_1", Value::Real(412_148_564_080.0)),
    ("unknown_bd_2", Value::Real(1.0)),
    ("unknown_bd_3", Value::Real(1.0)),
    ("unknown_bd_4", Value::Real(1.0)),
    ("unknown_bl_1", Value::Int(24)),
    ("DIMASO", Value::Bool(true)),
    ("DIMSHO", Value::Bool(true)),
    ("REGENMODE", Value::Bool(true)),
    ("FILLMODE", Value::Bool(true)),
    ("PSLTSCALE", Value::Bool(true)),
    ("USRTIMER", Value::Bool(true)),
    ("WORLDVIEW", Value::Bool(true)),
    ("TILEMODE", Value::Bool(true)),
    ("VISRETAIN", Value::Bool(true)),
    ("DISPSILH", Value::Bool(true)),
    ("PROXYGRAPHICS", Value::Int(1)),
    ("TREEDEPTH", Value::Int(3020)),
    ("LUNITS", Value::Int(2)),
    ("LUPREC", Value::Int(4)),
    ("ATTMODE", Value::Int(1)),
    ("SPLINESEGS", Value::Int(8)),
    ("SURFU", Value::Int(6)),
    ("SURFV", Value::Int(6)),
    ("SURFTYPE", Value::Int(6)),
    ("SURFTAB1", Value::Int(6)),
    ("SURFTAB2", Value::Int(6)),
    ("SPLINETYPE", Value::Int(6)),
    ("SHADEDGE", Value::Int(3)),
    ("SHADEDIF", Value::Int(70)),
    ("MAXACTVP", Value::Int(64)),
    ("ISOLINES", Value::Int(4)),
    ("TEXTQLTY", Value::Int(50)),
    ("LTSCALE", Value::Real(1.0)),
    ("TEXTSIZE", Value::Real(2.5)),
    ("TRACEWID", Value::Real(0.05)),
    ("SKETCHINC", Value::Real(0.1)),
    ("FACETRES", Value::Real(0.5)),
    ("CMLSCALE", Value::Real(1.0)),
    ("CELTSCALE", Value::Real(1.0)),
    ("PEXTMIN", Value::Point([1e20, 1e20, 1e20])),
    ("PEXTMAX", Value::Point([-1e20, -1e20, -1e20])),
    ("PUCSXDIR", Value::Point([1.0, 0.0, 0.0])),
    ("PUCSYDIR", Value::Point([0.0, 1.0, 0.0])),
    ("EXTMIN", Value::Point([1e20, 1e20, 1e20])),
    ("EXTMAX", Value::Point([-1e20, -1e20, -1e20])),
    ("UCSXDIR", Value::Point([1.0, 0.0, 0.0])),
    ("UCSYDIR", Value::Point([0.0, 1.0, 0.0])),
    ("DIMSCALE", Value::Real(1.0)),
    ("DIMASZ", Value::Real(0.18)),
    ("DIMEXO", Value::Real(0.0625)),
    ("DIMDLI", Value::Real(0.38)),
    ("DIMEXE", Value::Real(0.18)),
    ("DIMTIH", Value::Bool(true)),
    ("DIMTOH", Value::Bool(true)),
    ("DIMTXT", Value::Real(0.18)),
    ("DIMCEN", Value::Real(0.09)),
    ("DIMALTF", Value::Real(25.4)),
    ("DIMLFAC", Value::Real(1.0)),
    ("DIMTFAC", Value::Real(1.0)),
    ("DIMGAP", Value::Real(0.09)),
    ("DIMALTD", Value::Int(2)),
    ("DIMDEC", Value::Int(4)),
    ("DIMTDEC", Value::Int(4)),
    ("DIMALTU", Value::Int(2)),
    ("DIMALTTD", Value::Int(4)),
    ("DIMLUNIT", Value::Int(2)),
    ("DIMDSEP", Value::Int(46)),
    ("DIMTOLJ", Value::Int(1)),
    ("DIMATFIT", Value::Int(3)),
    ("DIMLWD", Value::Int(-2)),
    ("DIMLWE", Value::Int(-2)),
    ("TSTACKALIGN", Value::Int(1)),
    ("TSTACKSIZE", Value::Int(70)),
    // R2004.
    ("SORTENTS", Value::Int(127)),
    ("HIDETEXT", Value::Int(1)),
    ("XCLIPFRAME", Value::Int(2)),
    ("DIMASSOC", Value::Int(2)),
    ("OBSCUREDCOLOR", Value::Int(257)),
    ("INTERSECTIONCOLOR", Value::Int(257)),
    // R2007: what AutoCAD 2007 writes into an empty drawing.
    ("DIMFXL", Value::Real(1.0)),
    ("DIMJOGANG", Value::Real(std::f64::consts::FRAC_PI_4)),
    ("STEPSPERSEC", Value::Real(2.0)),
    ("STEPSIZE", Value::Real(6.0)),
    ("3DDWFPREC", Value::Real(2.0)),
    ("LENSLENGTH", Value::Real(50.0)),
    ("SHOWHIST", Value::Int(1)),
    ("PSOLWIDTH", Value::Real(0.25)),
    ("PSOLHEIGHT", Value::Real(4.0)),
    ("LATITUDE", Value::Real(37.795)),
    ("LONGITUDE", Value::Real(-122.394)),
    ("TIMEZONE", Value::Int(-8000)),
    ("DWFFRAME", Value::Int(2)),
];

/// The three streams a header is written into: before R2007 the texts and
/// the handles go into the data.
struct Streams {
    data: BitWriter,
    strings: BitWriter,
    handles: BitWriter,
    version: Version,
}

/// The header section: the sentinel, the size, the variables, the CRC,
/// the ending sentinel.
pub fn section(values: &HashMap<&'static str, Value>, version: Version) -> Vec<u8> {
    let mut streams = Streams {
        data: BitWriter::new(),
        strings: BitWriter::new(),
        handles: BitWriter::new(),
        version,
    };
    for &(name, kind, presence) in LAYOUT {
        if !presence.holds(version) {
            continue;
        }
        let value = values
            .get(name)
            .or_else(|| DEFAULTS.iter().find(|(n, _)| *n == name).map(|(_, v)| v));
        write_value(&mut streams, kind, value);
    }
    let data = &mut streams.data;
    // Four shorts "not required even for type 5" close the header (§9,
    // R14+), −1 as AutoCAD writes them; R2004 adds two longs and a bit
    // that acadrust reads and LibreDWG passes over.
    for _ in 0..4 {
        data.bs(-1);
    }
    if version >= Version::R2004 {
        data.bl(0);
        data.bl(0);
        data.bit(0);
    }
    let body = section_streams(streams.data, &streams.strings, &streams.handles, version);
    let mut out = Vec::with_capacity(body.len() + 38);
    out.extend_from_slice(&SENTINEL_BEGIN);
    let start = out.len();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    let crc = crc16(0xC0C1, &out[start..]);
    out.extend_from_slice(&crc.to_le_bytes());
    out.extend_from_slice(&SENTINEL_END);
    out
}

/// One variable by its kind, its value or a zero of that kind.
fn write_value(streams: &mut Streams, kind: Kind, value: Option<&Value>) {
    let version = streams.version;
    let r2007 = version >= Version::R2007;
    let d = &mut streams.data;
    let int = || match value {
        Some(Value::Int(i)) => *i,
        Some(Value::Bool(b)) => i64::from(*b),
        _ => 0,
    };
    let real = || match value {
        Some(Value::Real(r)) => *r,
        _ => 0.0,
    };
    match kind {
        Kind::B => d.bit(u8::from(int() != 0)),
        Kind::Rc => d.rc(int() as u8),
        Kind::Bs => d.bs(int() as i16),
        Kind::Bl => d.bl(int() as i32),
        Kind::Bd => d.bd(real()),
        Kind::Bd3 => {
            let p = match value {
                Some(Value::Point(p)) => *p,
                _ => [0.0; 3],
            };
            for v in p {
                d.bd(v);
            }
        }
        Kind::Rd2 => {
            let p = match value {
                Some(Value::Point(p)) => *p,
                _ => [0.0; 3],
            };
            d.rd(p[0]);
            d.rd(p[1]);
        }
        Kind::Tv => {
            let text = match value {
                Some(Value::Text(t)) => t,
                _ => "",
            };
            if r2007 {
                streams.strings.text_unicode(text);
            } else {
                d.text(text);
            }
        }
        Kind::H | Kind::HData => {
            let (code, handle) = match value {
                Some(Value::Handle(h)) => (h.code, h.value),
                _ => (if kind == Kind::H { 5 } else { 0 }, 0),
            };
            if r2007 && kind == Kind::H {
                streams.handles.handle(code, handle);
            } else {
                d.handle(code, handle);
            }
        }
        // CPSNID is stored only when CEPSNTYPE is 3, which is never
        // written here.
        Kind::HPlotStyle => {}
        Kind::Cmc => {
            let index = match value {
                Some(Value::Color(c)) => c.index,
                _ => 256,
            };
            if version >= Version::R2004 {
                // The R2004 form (§2.11): a zero index, the long with its
                // method, no names.
                d.bs(0);
                let long: u32 = match index {
                    256 => 0xC000_0000,
                    0 => 0xC100_0000,
                    i => 0xC300_0000 | u32::from(i as u16 & 0xff),
                };
                d.bl(long as i32);
                d.rc(0);
            } else {
                d.cmc(index);
            }
        }
    }
}

/// The presence table is the reader's, so a variable is written exactly
/// where it is read.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_r2000_variable_of_the_layout_is_present() {
        let count = LAYOUT
            .iter()
            .filter(|(_, _, p)| p.holds(Version::R2000))
            .count();
        assert!(count > 200);
    }
}
