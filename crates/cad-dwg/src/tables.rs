//! Table rows (ODA spec §20.4.51–20.4.68) and block headers, decoded into
//! the DXF groups the reader of `cad-dxf` takes.

use crate::bits::{Color, Handle};
use crate::groups::Groups;
use crate::objects::{ObjectCommon, Streams};
use crate::{DwgError, File, Version};

/// The lineweights of a DWG, by index (§20.4.1: the lineweight is a byte
/// indexing this table), as DXF group 370 states them: hundredths of a
/// millimetre, −1 by layer, −2 by block, −3 the default.
pub fn lineweight_code(index: u8) -> i64 {
    const TABLE: [i64; 24] = [
        0, 5, 9, 13, 15, 18, 20, 25, 30, 35, 40, 50, 53, 60, 70, 80, 90, 100, 106, 120, 140, 158,
        200, 211,
    ];
    match index {
        29 => -1,
        30 => -2,
        31 => -3,
        i => TABLE.get(usize::from(i)).copied().unwrap_or(-3),
    }
}

/// The common data of every table row (§20.4.54 and the rest): its name
/// and the xref flags, read after the object's common data. In R2007 the
/// two flag bits around the xref index are not stored (ADR-0036 errata):
/// the index alone says whether the row came from an xref.
fn row_head(streams: &mut Streams<'_>, file: &File<'_>) -> Result<(String, bool), DwgError> {
    let name = streams.text(file.limits())?;
    if file.version() >= Version::R2007 {
        let resolved = streams.data.bs()?;
        return Ok((name, resolved == 256));
    }
    let flag64 = streams.data.bit()? == 1;
    streams.data.bs()?; // xrefindex + 1
    let xdep = streams.data.bit()? == 1;
    Ok((name, flag64 || xdep))
}

/// The colour groups of a row or an entity: 62 for the index, 420 for a
/// true colour.
pub fn color_groups(groups: &mut Groups, color: Color, off: bool) {
    let index = i64::from(color.index);
    if let Some(rgb) = color.rgb {
        groups.int(62, if off { -index.max(1) } else { index.max(1) });
        groups.int(420, i64::from(rgb));
    } else if off {
        groups.int(62, -index);
    } else if index != 256 {
        groups.int(62, index);
    }
}

/// A row decoded: its DXF kind, its groups, and the handles the record
/// names, resolved later.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub kind: &'static str,
    pub handle: u64,
    pub name: String,
    pub groups: Groups,
    /// A layer's linetype, a dimension style's text style: by handle, to
    /// be named once every row is known.
    pub linetype: Option<Handle>,
    pub text_style: Option<Handle>,
}

/// A LAYER row (§20.4.54).
pub fn layer(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<Row, DwgError> {
    let version = file.version();
    let (name, _) = row_head(streams, file)?;
    let values = streams.data.bs()?;
    let color = streams.data.cmc(version, file.limits())?;
    let handles = file.object_handles(streams, common)?;
    let h = &mut streams.handles;
    h.handle()?; // the xref block
    h.handle()?; // the plot style
    if version >= Version::R2007 {
        h.handle()?; // the material
    }
    let linetype = h.handle()?;
    let _ = handles;
    // ADR-0036 errata: the 2 bit is "off", not "on" as §20.4.54 says.
    let frozen = values & 1 != 0;
    let off = values & 2 != 0;
    let locked = values & 8 != 0;
    let lineweight = ((values as u16 & 0x03e0) >> 5) as u8;
    let mut groups = Groups::default();
    groups.text(2, &name);
    groups.int(70, i64::from(frozen) | (i64::from(locked) << 2));
    color_groups(&mut groups, color, off);
    groups.int(370, lineweight_code(lineweight));
    groups.handle(5, handle);
    Ok(Row {
        kind: "LAYER",
        handle,
        name,
        groups,
        linetype: Some(linetype),
        text_style: None,
    })
}

/// An LTYPE row (§20.4.58).
pub fn linetype(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<Row, DwgError> {
    let version = file.version();
    let (name, _) = row_head(streams, file)?;
    let description = streams.text(file.limits())?;
    let length = streams.data.bd()?;
    let alignment = streams.data.rc()?;
    let count = streams.data.rc()?;
    let mut dashes = Vec::with_capacity(usize::from(count));
    let mut any_shape_text = false;
    for _ in 0..count {
        let dash = streams.data.bd()?;
        let shape = streams.data.bs()?;
        let x = streams.data.rd()?;
        let y = streams.data.rd()?;
        let scale = streams.data.bd()?;
        let rotation = streams.data.bd()?;
        let flags = streams.data.bs()?;
        any_shape_text |= flags & 2 != 0;
        dashes.push((dash, shape, x, y, scale, rotation, flags));
    }
    // The text area: 256 bytes always before R2007, 512 there when a dash
    // has text.
    let text_bytes = if version >= Version::R2007 {
        if any_shape_text { 512 } else { 0 }
    } else {
        256
    };
    for _ in 0..text_bytes {
        streams.data.rc()?;
    }
    file.object_handles(streams, common)?;
    let mut groups = Groups::default();
    groups.text(2, &name);
    groups.int(70, 0);
    groups.text(3, &description);
    groups.int(72, i64::from(alignment));
    groups.int(73, i64::from(count));
    groups.real(40, length);
    for (dash, shape, x, y, scale, rotation, flags) in dashes {
        groups.real(49, dash);
        groups.int(74, i64::from(flags));
        if flags != 0 {
            groups.int(75, i64::from(shape));
            groups.real(44, x);
            groups.real(45, y);
            groups.real(46, scale);
            groups.degrees(50, rotation);
        }
    }
    groups.handle(5, handle);
    Ok(Row {
        kind: "LTYPE",
        handle,
        name,
        groups,
        linetype: None,
        text_style: None,
    })
}

/// A STYLE row (§20.4.56, "shapefile").
pub fn text_style(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<Row, DwgError> {
    let (name, _) = row_head(streams, file)?;
    // ADR-0036 errata: the shape bit comes before the vertical bit, not
    // after as §20.4.56 lists them.
    let shape_file = streams.data.bit()? == 1;
    let vertical = streams.data.bit()? == 1;
    let height = streams.data.bd()?;
    let width = streams.data.bd()?;
    let oblique = streams.data.bd()?;
    let generation = streams.data.rc()?;
    let last_height = streams.data.bd()?;
    let font = streams.text(file.limits())?;
    let big_font = streams.text(file.limits())?;
    file.object_handles(streams, common)?;
    let mut groups = Groups::default();
    groups.text(2, &name);
    groups.int(70, i64::from(shape_file) | (i64::from(vertical) << 2));
    groups.real(40, height);
    groups.real(41, width);
    groups.degrees(50, oblique);
    groups.int(71, i64::from(generation));
    groups.real(42, last_height);
    groups.text(3, &font);
    groups.text(4, &big_font);
    groups.handle(5, handle);
    Ok(Row {
        kind: "STYLE",
        handle,
        name,
        groups,
        linetype: None,
        text_style: None,
    })
}

/// A DIMSTYLE row (§20.4.68), R2000 and later.
pub fn dim_style(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<Row, DwgError> {
    let version = file.version();
    let limits = file.limits();
    let (name, _) = row_head(streams, file)?;
    let mut groups = Groups::default();
    groups.text(2, &name);
    groups.int(70, 0);
    let post = streams.text(limits)?;
    let apost = streams.text(limits)?;
    groups.text(3, &post);
    groups.text(4, &apost);
    let d = &mut streams.data;
    for code in 40..=48 {
        let value = d.bd()?;
        groups.real(code, value);
    }
    if version >= Version::R2007 {
        let fxl = d.bd()?;
        let jogang = d.bd()?;
        let tfill = d.bs()?;
        let tfillclr = d.cmc(version, limits)?;
        groups.real(49, fxl);
        groups.degrees(50, jogang);
        groups.int(69, tfill);
        groups.int(70, i64::from(tfillclr.index));
    }
    for code in 71..=76 {
        let value = d.bit()?;
        groups.int(code, i64::from(value));
    }
    for code in 77..=79 {
        let value = d.bs()?;
        groups.int(code, value);
    }
    if version >= Version::R2007 {
        let arcsym = d.bs()?;
        groups.int(90, arcsym);
    }
    for code in 140..=148 {
        let value = d.bd()?;
        groups.real(code, value);
    }
    let alt = d.bit()?;
    groups.int(170, i64::from(alt));
    let altd = d.bs()?;
    groups.int(171, altd);
    for code in 172..=175 {
        let value = d.bit()?;
        groups.int(code, i64::from(value));
    }
    for code in 176..=178 {
        let color = d.cmc(version, limits)?;
        groups.int(code, i64::from(color.index));
    }
    let adec = d.bs()?;
    groups.int(179, adec);
    for code in 271..=280 {
        let value = d.bs()?;
        groups.int(code, value);
    }
    for code in 281..=282 {
        let value = d.bit()?;
        groups.int(code, i64::from(value));
    }
    for code in 283..=286 {
        let value = d.bs()?;
        groups.int(code, value);
    }
    let upt = d.bit()?;
    groups.int(288, i64::from(upt));
    let fit = d.bs()?;
    groups.int(287, fit);
    if version >= Version::R2007 {
        let fxlon = d.bit()?;
        groups.int(290, i64::from(fxlon));
    }
    let lwd = d.bs()?;
    let lwe = d.bs()?;
    groups.int(371, lwd);
    groups.int(372, lwe);
    d.bit()?; // the unknown bit
    file.object_handles(streams, common)?;
    let h = &mut streams.handles;
    h.handle()?; // the xref block
    let text_style = h.handle()?;
    groups.handle(5, handle);
    Ok(Row {
        kind: "DIMSTYLE",
        handle,
        name,
        groups,
        linetype: None,
        text_style: Some(text_style),
    })
}

/// A block header (§20.4.52): the block's name and base, and the handles
/// of the entities it owns, in order (R2004 and later), or the first and
/// last of its chain (R2000).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockHeader {
    pub handle: u64,
    pub name: String,
    pub anonymous: bool,
    pub is_xref: bool,
    pub base: cad_core::Point2,
    pub first: Option<Handle>,
    pub last: Option<Handle>,
    pub owned: Vec<Handle>,
    pub layout: Option<Handle>,
}

pub fn block_header(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<BlockHeader, DwgError> {
    let version = file.version();
    let limits = file.limits();
    let (name, _) = row_head(streams, file)?;
    let anonymous = streams.data.bit()? == 1;
    let _has_attributes = streams.data.bit()? == 1;
    let is_xref = streams.data.bit()? == 1;
    let overlaid = streams.data.bit()? == 1;
    let _loaded = streams.data.bit()?;
    let mut owned_count = 0usize;
    if version >= Version::R2004 {
        owned_count = usize::try_from(streams.data.bl()?).unwrap_or(0);
    }
    let (base, _) = streams.data.bd3()?;
    let _xref_path = streams.text(limits)?;
    let mut insert_count = 0usize;
    loop {
        let byte = streams.data.rc()?;
        if byte == 0 {
            break;
        }
        insert_count += 1;
        if insert_count > limits.max_objects {
            return Err(DwgError::TooMany {
                what: "block inserts",
                limit: limits.max_objects,
            });
        }
    }
    let _description = streams.text(limits)?;
    let preview = streams.data.bl()?;
    let preview = usize::try_from(preview).map_err(|_| DwgError::Corrupt {
        what: "a block preview",
        at: 0,
    })?;
    if preview * 8 > streams.data.remaining_bits() {
        return Err(DwgError::Corrupt {
            what: "a block preview",
            at: 0,
        });
    }
    streams
        .data
        .set_position(streams.data.position() + preview * 8)?;
    if version >= Version::R2007 {
        streams.data.bs()?; // insert units
        streams.data.bit()?; // explodable
        streams.data.rc()?; // scaling
    }
    file.object_handles(streams, common)?;
    let h = &mut streams.handles;
    h.handle()?; // NULL
    h.handle()?; // the BLOCK entity
    let mut header = BlockHeader {
        handle,
        name,
        anonymous,
        is_xref,
        base,
        ..BlockHeader::default()
    };
    if version < Version::R2004 {
        if !is_xref && !overlaid {
            header.first = Some(h.handle()?);
            header.last = Some(h.handle()?);
        }
    } else {
        if owned_count * 8 > h.remaining_bits() {
            return Err(DwgError::Corrupt {
                what: "a block's owned entities",
                at: 0,
            });
        }
        for _ in 0..owned_count {
            header.owned.push(h.handle()?);
        }
    }
    h.handle()?; // ENDBLK
    for _ in 0..insert_count {
        h.handle()?;
    }
    header.layout = h.handle().ok();
    Ok(header)
}
