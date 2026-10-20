//! Entities (ODA spec §20.4), decoded by their prescriptions into the DXF
//! groups the reader of `cad-dxf` takes (ADR-0036 §2). What this crate
//! does not decode is handed over by its kind alone, so the reader counts
//! and reports it.
//!
//! A field equal to its default — a scale of 1, a bulge of 0, the
//! extrusion (0, 0, 1) — is left out of the groups, as a writer leaves it
//! out of a file; the comparisons are with the stored value, exactly.
#![allow(clippy::float_cmp)]

use cad_core::Point2;

use crate::bits::BitReader;
use crate::groups::Groups;
use crate::objects::{EntityCommon, RawObject, Streams, types};
use crate::tables::{color_groups, lineweight_code};
use crate::{DwgError, File, Version};

/// A linetype as an entity names it (§20.4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinetypeRef {
    ByLayer,
    ByBlock,
    Continuous,
    Named(u64),
}

/// An entity decoded: its DXF kind and groups, and what the record names
/// by handle, resolved to names once every row is known.
#[derive(Clone, Debug, PartialEq)]
pub struct Decoded {
    pub handle: u64,
    pub kind: String,
    pub groups: Groups,
    /// 0: owned by a block or an entity; 1: paper space; 2: model space.
    pub entmode: u8,
    pub owner: Option<u64>,
    /// R2000: the next entity of the owner's chain.
    pub next: Option<u64>,
    pub layer: u64,
    pub linetype: LinetypeRef,
    pub text_style: Option<u64>,
    pub block: Option<u64>,
    /// A VERTEX, an ATTRIB, a SEQEND, a BLOCK, an ENDBLK: read with the
    /// entity that owns it, never on its own.
    pub subordinate: bool,
    /// R2004 and later: the vertices or attributes this entity owns, in
    /// order; R2000: the first and last of its chain.
    pub owned: Vec<u64>,
    pub first: Option<u64>,
    pub last: Option<u64>,
    /// Whether a SEQEND closes the owned entities.
    pub has_seqend: bool,
}

impl Decoded {
    fn new(handle: u64, kind: &str) -> Self {
        Self {
            handle,
            kind: kind.to_owned(),
            groups: Groups::default(),
            entmode: 2,
            owner: None,
            next: None,
            layer: 0,
            linetype: LinetypeRef::ByLayer,
            text_style: None,
            block: None,
            subordinate: false,
            owned: Vec::new(),
            first: None,
            last: None,
            has_seqend: false,
        }
    }
}

/// The DXF kind of a fixed entity type (§20.3), or `None` for one this
/// crate does not decode, whose name the reader reports.
fn kind_of(type_code: u16) -> &'static str {
    match type_code {
        types::TEXT => "TEXT",
        types::ATTRIB => "ATTRIB",
        types::ATTDEF => "ATTDEF",
        types::BLOCK => "BLOCK",
        types::ENDBLK => "ENDBLK",
        types::SEQEND => "SEQEND",
        types::INSERT | types::MINSERT => "INSERT",
        types::VERTEX_2D | types::VERTEX_3D => "VERTEX",
        // A mesh's and a polyface's vertices are 3D vertices; a polyface's
        // face is a vertex of indices, not read.
        0x0c | 0x0d => "VERTEX",
        0x0e => "VERTEX_PFACE_FACE",
        types::POLYLINE_2D | types::POLYLINE_3D => "POLYLINE",
        0x1d => "POLYLINE",
        0x1e => "POLYLINE",
        types::ARC => "ARC",
        types::CIRCLE => "CIRCLE",
        types::LINE => "LINE",
        types::DIMENSION_ORDINATE
        | types::DIMENSION_LINEAR
        | types::DIMENSION_ALIGNED
        | types::DIMENSION_ANGULAR_3PT
        | types::DIMENSION_ANGULAR_2LN
        | types::DIMENSION_RADIUS
        | types::DIMENSION_DIAMETER => "DIMENSION",
        types::POINT => "POINT",
        types::FACE_3D => "3DFACE",
        types::SOLID => "SOLID",
        0x20 => "TRACE",
        0x21 => "SHAPE",
        types::VIEWPORT => "VIEWPORT",
        types::ELLIPSE => "ELLIPSE",
        types::SPLINE => "SPLINE",
        0x25 => "REGION",
        0x26 => "3DSOLID",
        0x27 => "BODY",
        types::RAY => "RAY",
        types::XLINE => "XLINE",
        0x2b => "OLEFRAME",
        types::MTEXT => "MTEXT",
        types::LEADER => "LEADER",
        types::TOLERANCE => "TOLERANCE",
        types::MLINE => "MLINE",
        0x4a => "OLE2FRAME",
        types::LWPOLYLINE => "LWPOLYLINE",
        types::HATCH => "HATCH",
        types::PROXY_ENTITY => "ACAD_PROXY_ENTITY",
        _ => "(unknown entity type)",
    }
}

/// Decodes an entity: its common data and handles, then its own fields by
/// its kind. A kind this crate does not decode comes back with its kind
/// and no groups.
pub fn decode(
    file: &File<'_>,
    object: &RawObject<'_>,
    streams: &mut Streams<'_>,
    class_name: Option<&str>,
) -> Result<Decoded, DwgError> {
    let version = file.version();
    let common = file.entity_common(streams, object)?;
    let handles = file.entity_handles(streams, &common)?;
    let kind = match class_name {
        Some(name) => name,
        None => kind_of(object.type_code),
    };
    let mut out = Decoded::new(object.handle, kind);
    out.entmode = common.entmode;
    out.owner = handles.owner.map(|h| h.resolve(object.handle));
    if version < Version::R2004 {
        out.next = Some(match handles.next {
            Some(next) => next.resolve(object.handle),
            None => object.handle.wrapping_add(1),
        });
    }
    out.layer = handles.layer.resolve(object.handle);
    out.linetype = match common.ltype_flags {
        1 => LinetypeRef::ByBlock,
        2 => LinetypeRef::Continuous,
        3 => LinetypeRef::Named(handles.linetype.map_or(0, |h| h.resolve(object.handle))),
        _ => LinetypeRef::ByLayer,
    };
    common_groups(&mut out.groups, object.handle, &common);
    let limits = file.limits();
    let d = &mut streams.data;
    match kind {
        "LINE" => {
            let zs_zero = d.bit()? == 1;
            let x1 = d.rd()?;
            let x2 = d.dd(x1)?;
            let y1 = d.rd()?;
            let y2 = d.dd(y1)?;
            let (z1, z2) = if zs_zero {
                (0.0, 0.0)
            } else {
                let z1 = d.rd()?;
                (z1, d.dd(z1)?)
            };
            out.groups.point3(10, Point2::new(x1, y1), z1);
            out.groups.point3(11, Point2::new(x2, y2), z2);
            thickness_and_extrusion(&mut out.groups, d)?;
        }
        "CIRCLE" => {
            let (center, z) = d.bd3()?;
            let radius = d.bd()?;
            out.groups.point3(10, center, z);
            out.groups.real(40, radius);
            thickness_and_extrusion(&mut out.groups, d)?;
        }
        "ARC" => {
            let (center, z) = d.bd3()?;
            let radius = d.bd()?;
            out.groups.point3(10, center, z);
            out.groups.real(40, radius);
            thickness_and_extrusion(&mut out.groups, d)?;
            let start = d.bd()?;
            let end = d.bd()?;
            out.groups.degrees(50, start);
            out.groups.degrees(51, end);
        }
        "POINT" => {
            let (point, z) = d.bd3()?;
            out.groups.point3(10, point, z);
            thickness_and_extrusion(&mut out.groups, d)?;
            let angle = d.bd()?;
            out.groups.degrees(50, angle);
        }
        "ELLIPSE" => {
            let (center, z) = d.bd3()?;
            let (major, major_z) = d.bd3()?;
            let (extrusion, extrusion_z) = d.bd3()?;
            let ratio = d.bd()?;
            let start = d.bd()?;
            let end = d.bd()?;
            out.groups.point3(10, center, z);
            out.groups.point3(11, major, major_z);
            out.groups.point3(210, extrusion, extrusion_z);
            out.groups.real(40, ratio);
            out.groups.real(41, start);
            out.groups.real(42, end);
        }
        "XLINE" | "RAY" => {
            let (point, z) = d.bd3()?;
            let (vector, vz) = d.bd3()?;
            out.groups.point3(10, point, z);
            out.groups.point3(11, vector, vz);
        }
        "SOLID" | "TRACE" => {
            let thickness = d.bt()?;
            let elevation = d.bd()?;
            let corners = [d.rd2()?, d.rd2()?, d.rd2()?, d.rd2()?];
            for (i, corner) in corners.into_iter().enumerate() {
                out.groups.point3(10 + i as i32, corner, elevation);
            }
            let extrusion = d.be()?;
            out.groups.real(39, thickness);
            extrusion_groups(&mut out.groups, extrusion);
        }
        "3DFACE" => {
            let has_no_flags = d.bit()? == 1;
            let z_is_zero = d.bit()? == 1;
            let x = d.rd()?;
            let y = d.rd()?;
            let z = if z_is_zero { 0.0 } else { d.rd()? };
            let mut previous = [x, y, z];
            out.groups.point3(10, Point2::new(x, y), z);
            for code in 11..=13 {
                let x = d.dd(previous[0])?;
                let y = d.dd(previous[1])?;
                let z = d.dd(previous[2])?;
                previous = [x, y, z];
                out.groups.point3(code, Point2::new(x, y), z);
            }
            let flags = if has_no_flags { 0 } else { d.bs()? };
            out.groups.int(70, flags);
        }
        "LWPOLYLINE" => lwpolyline(&mut out.groups, d, limits)?,
        "TEXT" | "ATTRIB" | "ATTDEF" => {
            text(
                &mut out.groups,
                d,
                streams.strings.as_mut(),
                version,
                limits,
            )?;
            if kind != "TEXT" {
                let tag = text_field(d, streams.strings.as_mut(), limits)?;
                let field_length = d.bs()?;
                let flags = d.rc()?;
                if version >= Version::R2007 {
                    d.bit()?; // lock position
                }
                out.groups.text(2, &tag);
                out.groups.int(73, field_length);
                out.groups.int(70, i64::from(flags));
                if kind == "ATTDEF" {
                    let prompt = text_field(d, streams.strings.as_mut(), limits)?;
                    out.groups.text(3, &prompt);
                }
                out.subordinate = kind == "ATTRIB";
            }
            out.text_style = Some(streams.handles.handle()?.resolve(object.handle));
        }
        "MTEXT" => {
            mtext(
                &mut out.groups,
                d,
                streams.strings.as_mut(),
                version,
                limits,
            )?;
            out.text_style = Some(streams.handles.handle()?.resolve(object.handle));
        }
        "INSERT" => {
            let minsert = object.type_code == types::MINSERT;
            insert(&mut out, d, version, minsert)?;
            let h = &mut streams.handles;
            out.block = Some(h.handle()?.resolve(object.handle));
            owned_handles(&mut out, h, version)?;
        }
        "POLYLINE" => {
            polyline(&mut out, d, object.type_code, version)?;
            owned_handles(&mut out, &mut streams.handles, version)?;
        }
        "VERTEX" => {
            vertex(&mut out.groups, d, object.type_code, version)?;
            out.subordinate = true;
        }
        "SEQEND" | "BLOCK" | "ENDBLK" | "VERTEX_PFACE_FACE" => {
            out.subordinate = true;
        }
        "SPLINE" => spline(&mut out.groups, d, limits)?,
        "HATCH" => hatch(
            &mut out.groups,
            d,
            streams.strings.as_mut(),
            version,
            limits,
        )?,
        "WIPEOUT" => wipeout(&mut out.groups, d, limits)?,
        _ => {
            // Not decoded: the kind alone, for the report. A kind the DXF
            // reader would try to read from groups is marked, so that it
            // is counted and not refused.
            out.groups = Groups::default();
            if matches!(kind, "DIMENSION" | "VIEWPORT" | "LEADER" | "MULTILEADER") {
                out.kind = format!("DWG:{kind}");
            }
        }
    }
    Ok(out)
}

/// The groups every entity carries (§20.4.1): its handle, colour,
/// lineweight, linetype scale, invisibility, and space.
fn common_groups(groups: &mut Groups, handle: u64, common: &EntityCommon) {
    groups.handle(5, handle);
    color_groups(groups, common.color, false);
    let lineweight = lineweight_code(common.lineweight);
    if lineweight != -1 {
        groups.int(370, lineweight);
    }
    if common.ltype_scale != 1.0 {
        groups.real(48, common.ltype_scale);
    }
    if common.invisible {
        groups.int(60, 1);
    }
    if common.entmode == 1 {
        groups.int(67, 1);
    }
}

fn thickness_and_extrusion(groups: &mut Groups, d: &mut BitReader<'_>) -> Result<(), DwgError> {
    let thickness = d.bt()?;
    let extrusion = d.be()?;
    if thickness != 0.0 {
        groups.real(39, thickness);
    }
    extrusion_groups(groups, extrusion);
    Ok(())
}

fn extrusion_groups(groups: &mut Groups, extrusion: [f64; 3]) {
    if extrusion != [0.0, 0.0, 1.0] {
        groups.point3(210, Point2::new(extrusion[0], extrusion[1]), extrusion[2]);
    }
}

/// A text field: from the string stream in R2007, else from the data.
fn text_field(
    d: &mut BitReader<'_>,
    strings: Option<&mut BitReader<'_>>,
    limits: &crate::ImportLimits,
) -> Result<String, DwgError> {
    match strings {
        Some(s) => s.text_unicode(limits),
        None => Ok(String::from_utf8_lossy(&d.text(limits)?).into_owned()),
    }
}

/// A TEXT's own fields (§20.4.3, R2000 and later).
fn text(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    strings: Option<&mut BitReader<'_>>,
    _version: Version,
    limits: &crate::ImportLimits,
) -> Result<(), DwgError> {
    let flags = d.rc()?;
    let elevation = if flags & 1 == 0 { d.rd()? } else { 0.0 };
    let insertion = d.rd2()?;
    let alignment = if flags & 2 == 0 {
        Some(Point2::new(d.dd(insertion.x)?, d.dd(insertion.y)?))
    } else {
        None
    };
    let extrusion = d.be()?;
    let thickness = d.bt()?;
    let oblique = if flags & 4 == 0 { d.rd()? } else { 0.0 };
    let rotation = if flags & 8 == 0 { d.rd()? } else { 0.0 };
    let height = d.rd()?;
    let width = if flags & 0x10 == 0 { d.rd()? } else { 1.0 };
    let value = text_field(d, strings, limits)?;
    let generation = if flags & 0x20 == 0 { d.bs()? } else { 0 };
    let halign = if flags & 0x40 == 0 { d.bs()? } else { 0 };
    let valign = if flags & 0x80 == 0 { d.bs()? } else { 0 };
    groups.point3(10, insertion, elevation);
    if let Some(alignment) = alignment {
        groups.point3(11, alignment, elevation);
    }
    groups.real(40, height);
    groups.text(1, &value);
    if rotation != 0.0 {
        groups.degrees(50, rotation);
    }
    if width != 1.0 {
        groups.real(41, width);
    }
    if oblique != 0.0 {
        groups.degrees(51, oblique);
    }
    if generation != 0 {
        groups.int(71, generation);
    }
    if halign != 0 {
        groups.int(72, halign);
    }
    if valign != 0 {
        groups.int(73, valign);
    }
    if thickness != 0.0 {
        groups.real(39, thickness);
    }
    extrusion_groups(groups, extrusion);
    Ok(())
}

/// An MTEXT's own fields (§20.4.46).
fn mtext(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    strings: Option<&mut BitReader<'_>>,
    version: Version,
    limits: &crate::ImportLimits,
) -> Result<(), DwgError> {
    let (insertion, z) = d.bd3()?;
    let (extrusion, ez) = d.bd3()?;
    let (direction, dz) = d.bd3()?;
    let width = d.bd()?;
    if version >= Version::R2007 {
        d.bd()?; // the rectangle's height
    }
    let height = d.bd()?;
    let attachment = d.bs()?;
    let drawing_direction = d.bs()?;
    d.bd()?; // the extents' height
    d.bd()?; // the extents' width
    let value = text_field(d, strings, limits)?;
    let spacing_style = d.bs()?;
    let spacing_factor = d.bd()?;
    d.bit()?;
    if version >= Version::R2004 {
        let background = d.bl()?;
        if background & 1 != 0 {
            d.bl()?;
            d.cmc(version, limits)?;
            d.bl()?;
        }
    }
    groups.point3(10, insertion, z);
    groups.real(40, height);
    groups.real(41, width);
    groups.int(71, attachment);
    groups.int(72, drawing_direction);
    // The words in pieces of 250 characters, the last under 1, as a file
    // carries them.
    let chars: Vec<char> = value.chars().collect();
    let mut at = 0;
    while chars.len() - at > 250 {
        let piece: String = chars[at..at + 250].iter().collect();
        groups.text(3, &piece);
        at += 250;
    }
    let last: String = chars[at..].iter().collect();
    groups.text(1, &last);
    groups.point3(11, direction, dz);
    groups.int(73, spacing_style);
    groups.real(44, spacing_factor);
    extrusion_groups(groups, [extrusion.x, extrusion.y, ez]);
    Ok(())
}

/// An INSERT or MINSERT's own fields (§20.4.9–20.4.10).
fn insert(
    out: &mut Decoded,
    d: &mut BitReader<'_>,
    version: Version,
    minsert: bool,
) -> Result<(), DwgError> {
    let (insertion, z) = d.bd3()?;
    let flags = d.bb()?;
    let (sx, sy, sz) = match flags {
        3 => (1.0, 1.0, 1.0),
        1 => {
            let y = d.dd(1.0)?;
            let z = d.dd(1.0)?;
            (1.0, y, z)
        }
        2 => {
            let x = d.rd()?;
            (x, x, x)
        }
        _ => {
            let x = d.rd()?;
            let y = d.dd(x)?;
            let z = d.dd(x)?;
            (x, y, z)
        }
    };
    let rotation = d.bd()?;
    let (extrusion, ez) = d.bd3()?;
    let has_attributes = d.bit()? == 1;
    // The owned count is stored only when there are attributes to own
    // (LibreDWG; §20.4.9 lists it unconditionally).
    if version >= Version::R2004 && has_attributes {
        let count = d.bl()?;
        out.owned = vec![0; usize::try_from(count).unwrap_or(0).min(d.remaining_bits())];
    }
    if minsert {
        let columns = d.bs()?;
        let rows = d.bs()?;
        let column_spacing = d.bd()?;
        let row_spacing = d.bd()?;
        out.groups.int(70, columns);
        out.groups.int(71, rows);
        out.groups.real(44, column_spacing);
        out.groups.real(45, row_spacing);
    }
    out.groups.point3(10, insertion, z);
    if (sx, sy, sz) != (1.0, 1.0, 1.0) {
        out.groups.real(41, sx);
        out.groups.real(42, sy);
        out.groups.real(43, sz);
    }
    if rotation != 0.0 {
        out.groups.degrees(50, rotation);
    }
    extrusion_groups(&mut out.groups, [extrusion.x, extrusion.y, ez]);
    out.has_seqend = has_attributes;
    if has_attributes {
        out.groups.int(66, 1);
    }
    Ok(())
}

/// The handles of what an INSERT or a POLYLINE owns (§20.4.9, §20.4.16):
/// the first and last of a chain in R2000, the list in R2004 and later,
/// then the SEQEND.
fn owned_handles(
    out: &mut Decoded,
    h: &mut BitReader<'_>,
    version: Version,
) -> Result<(), DwgError> {
    let own = out.handle;
    if version < Version::R2004 {
        if out.has_seqend {
            out.first = Some(h.handle()?.resolve(own));
            out.last = Some(h.handle()?.resolve(own));
        }
    } else {
        let count = out.owned.len();
        out.owned.clear();
        if count * 8 > h.remaining_bits() {
            return Err(DwgError::Corrupt {
                what: "an entity's owned objects",
                at: 0,
            });
        }
        for _ in 0..count {
            out.owned.push(h.handle()?.resolve(own));
        }
    }
    if out.has_seqend {
        h.handle()?; // SEQEND
    }
    Ok(())
}

/// A 2D or 3D POLYLINE's own fields (§20.4.16–20.4.17).
fn polyline(
    out: &mut Decoded,
    d: &mut BitReader<'_>,
    type_code: u16,
    version: Version,
) -> Result<(), DwgError> {
    let flags: i64;
    if type_code == types::POLYLINE_2D {
        flags = i64::from(d.bs()?);
        let curve_type = d.bs()?;
        let start_width = d.bd()?;
        let end_width = d.bd()?;
        let thickness = d.bt()?;
        let elevation = d.bd()?;
        let extrusion = d.be()?;
        out.groups.int(70, flags);
        if curve_type != 0 {
            out.groups.int(75, curve_type);
        }
        if start_width != 0.0 {
            out.groups.real(40, start_width);
        }
        if end_width != 0.0 {
            out.groups.real(41, end_width);
        }
        if thickness != 0.0 {
            out.groups.real(39, thickness);
        }
        out.groups.point3(10, Point2::ORIGIN, elevation);
        extrusion_groups(&mut out.groups, extrusion);
    } else if type_code == types::POLYLINE_3D {
        let splined = d.rc()?;
        let closed = d.rc()?;
        flags = 8 | i64::from(closed & 1) | if splined != 0 { 4 } else { 0 };
        out.groups.int(70, flags);
        out.groups.point3(10, Point2::ORIGIN, 0.0);
    } else {
        // A polyface or polygon mesh: not held; its kind says so.
        flags = if type_code == 0x1d { 64 } else { 16 };
        out.groups.int(70, flags);
    }
    if version >= Version::R2004 {
        // The owned count comes after the fields of the 2D and 3D kinds;
        // a mesh's own fields are not read, so its count is unknown.
        if matches!(type_code, types::POLYLINE_2D | types::POLYLINE_3D) {
            let count = d.bl()?;
            out.owned = vec![0; usize::try_from(count).unwrap_or(0).min(d.remaining_bits())];
        }
    }
    out.groups.int(66, 1);
    out.has_seqend = matches!(type_code, types::POLYLINE_2D | types::POLYLINE_3D);
    Ok(())
}

/// A VERTEX's own fields (§20.4.11–20.4.12).
fn vertex(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    type_code: u16,
    _version: Version,
) -> Result<(), DwgError> {
    let flags = d.rc()?;
    let (point, z) = d.bd3()?;
    if type_code == types::VERTEX_2D {
        let start_width = d.bd()?;
        let (start, end) = if start_width < 0.0 {
            (-start_width, -start_width)
        } else {
            (start_width, d.bd()?)
        };
        let bulge = d.bd()?;
        let tangent = d.bd()?;
        groups.point3(10, point, z);
        if start != 0.0 {
            groups.real(40, start);
        }
        if end != 0.0 {
            groups.real(41, end);
        }
        if bulge != 0.0 {
            groups.real(42, bulge);
        }
        if tangent != 0.0 {
            groups.degrees(50, tangent);
        }
    } else {
        groups.point3(10, point, z);
    }
    groups.int(70, i64::from(flags));
    Ok(())
}

/// An LWPOLYLINE (§20.4.85).
fn lwpolyline(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    limits: &crate::ImportLimits,
) -> Result<(), DwgError> {
    let flag = d.bs()? as u16;
    let constant_width = if flag & 4 != 0 { Some(d.bd()?) } else { None };
    let elevation = if flag & 8 != 0 { Some(d.bd()?) } else { None };
    let thickness = if flag & 2 != 0 { Some(d.bd()?) } else { None };
    let normal = if flag & 1 != 0 {
        let (n, z) = d.bd3()?;
        Some([n.x, n.y, z])
    } else {
        None
    };
    let count = count_within(d.bl()?, d, 2, limits)?;
    let bulges = if flag & 16 != 0 {
        count_within(d.bl()?, d, 2, limits)?
    } else {
        0
    };
    let widths = if flag & 32 != 0 {
        count_within(d.bl()?, d, 2, limits)?
    } else {
        0
    };
    let mut points = Vec::with_capacity(count);
    if count > 0 {
        let mut x = d.rd()?;
        let mut y = d.rd()?;
        points.push(Point2::new(x, y));
        for _ in 1..count {
            x = d.dd(x)?;
            y = d.dd(y)?;
            points.push(Point2::new(x, y));
        }
    }
    let mut bulge_values = Vec::with_capacity(bulges);
    for _ in 0..bulges {
        bulge_values.push(d.bd()?);
    }
    let mut width_values = Vec::with_capacity(widths);
    for _ in 0..widths {
        width_values.push((d.bd()?, d.bd()?));
    }
    // DXF 70: closed 1, linetype generation 128.
    let dxf_flags = i64::from(flag & 512 != 0) | if flag & 256 != 0 { 128 } else { 0 };
    groups.int(90, count as i64);
    groups.int(70, dxf_flags);
    if let Some(width) = constant_width {
        groups.real(43, width);
    }
    if let Some(elevation) = elevation {
        groups.real(38, elevation);
    }
    if let Some(thickness) = thickness {
        groups.real(39, thickness);
    }
    if let Some(normal) = normal {
        groups.point3(210, Point2::new(normal[0], normal[1]), normal[2]);
    }
    for (i, point) in points.into_iter().enumerate() {
        groups.point(10, point);
        if let Some((start, end)) = width_values.get(i) {
            groups.real(40, *start);
            groups.real(41, *end);
        }
        if let Some(bulge) = bulge_values.get(i)
            && *bulge != 0.0
        {
            groups.real(42, *bulge);
        }
    }
    Ok(())
}

/// A count the data could hold, each item at least `bits` bits: a claim
/// past that is corrupt, never an allocation.
fn count_within(
    count: i32,
    d: &BitReader<'_>,
    bits: usize,
    limits: &crate::ImportLimits,
) -> Result<usize, DwgError> {
    let count = usize::try_from(count).map_err(|_| DwgError::Corrupt {
        what: "a count",
        at: d.byte_position(),
    })?;
    if count.saturating_mul(bits) > d.remaining_bits() || count > limits.max_objects {
        return Err(DwgError::Corrupt {
            what: "a count past the object",
            at: d.byte_position(),
        });
    }
    Ok(count)
}

/// A SPLINE (§20.4.40).
fn spline(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    limits: &crate::ImportLimits,
) -> Result<(), DwgError> {
    let scenario = d.bl()?;
    let degree = d.bl()?;
    let mut flags = 8i64; // planar
    let mut fit_count = 0;
    let mut knot_count = 0;
    let mut control_count = 0;
    let mut weighted = false;
    let mut tail = Groups::default();
    if scenario == 2 {
        let tolerance = d.bd()?;
        let (begin, bz) = d.bd3()?;
        let (end, ez) = d.bd3()?;
        fit_count = count_within(d.bl()?, d, 6, limits)?;
        tail.real(44, tolerance);
        tail.point3(12, begin, bz);
        tail.point3(13, end, ez);
    } else if scenario == 1 {
        let rational = d.bit()? == 1;
        let closed = d.bit()? == 1;
        let periodic = d.bit()? == 1;
        let knot_tolerance = d.bd()?;
        let control_tolerance = d.bd()?;
        knot_count = count_within(d.bl()?, d, 2, limits)?;
        control_count = count_within(d.bl()?, d, 6, limits)?;
        weighted = d.bit()? == 1;
        flags |= i64::from(closed) | (i64::from(periodic) << 1) | (i64::from(rational) << 2);
        tail.real(42, knot_tolerance);
        tail.real(43, control_tolerance);
    } else {
        return Err(DwgError::Corrupt {
            what: "a spline scenario",
            at: d.byte_position(),
        });
    }
    groups.int(70, flags);
    groups.int(71, degree);
    groups.int(72, knot_count as i64);
    groups.int(73, control_count as i64);
    groups.int(74, fit_count as i64);
    groups.0.extend(tail.0);
    for _ in 0..knot_count {
        let knot = d.bd()?;
        groups.real(40, knot);
    }
    for _ in 0..control_count {
        let (point, z) = d.bd3()?;
        groups.point3(10, point, z);
        if weighted {
            let weight = d.bd()?;
            groups.real(41, weight);
        }
    }
    for _ in 0..fit_count {
        let (point, z) = d.bd3()?;
        groups.point3(11, point, z);
    }
    Ok(())
}

/// A HATCH (§20.4.75), in the group order a file carries.
fn hatch(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    strings: Option<&mut BitReader<'_>>,
    version: Version,
    limits: &crate::ImportLimits,
) -> Result<(), DwgError> {
    let mut gradient = Groups::default();
    let mut strings = strings;
    if version >= Version::R2004 {
        let is_gradient = d.bl()?;
        let reserved = d.bl()?;
        let angle = d.bd()?;
        let shift = d.bd()?;
        let single = d.bl()?;
        let tint = d.bd()?;
        let colors = count_within(d.bl()?, d, 8, limits)?;
        let mut color_groups = Groups::default();
        for _ in 0..colors {
            let value = d.bd()?;
            d.bs()?;
            let rgb = d.bl()?;
            d.rc()?;
            color_groups.real(463, value);
            color_groups.int(421, rgb);
        }
        let name = text_field(d, strings.as_deref_mut(), limits)?;
        gradient.int(450, is_gradient);
        gradient.int(451, reserved);
        gradient.degrees(460, angle);
        gradient.real(461, shift);
        gradient.int(452, single);
        gradient.real(462, tint);
        gradient.int(453, colors as i64);
        gradient.0.extend(color_groups.0);
        gradient.text(470, &name);
    }
    let elevation = d.bd()?;
    let (extrusion, ez) = d.bd3()?;
    let name = text_field(d, strings, limits)?;
    let solid = d.bit()?;
    let associative = d.bit()?;
    let paths = count_within(d.bl()?, d, 2, limits)?;
    groups.point3(10, Point2::ORIGIN, elevation);
    groups.point3(210, extrusion, ez);
    groups.text(2, &name);
    groups.int(70, i64::from(solid));
    groups.int(71, i64::from(associative));
    groups.int(91, paths as i64);
    let mut any_pixel = false;
    let mut boundary_handles = 0usize;
    for _ in 0..paths {
        let flags = d.bl()?;
        any_pixel |= flags & 4 != 0;
        groups.int(92, flags);
        if flags & 2 == 0 {
            let segments = count_within(d.bl()?, d, 8, limits)?;
            groups.int(93, segments as i64);
            for _ in 0..segments {
                let kind = d.rc()?;
                groups.int(72, i64::from(kind));
                match kind {
                    1 => {
                        groups.point(10, d.rd2()?);
                        groups.point(11, d.rd2()?);
                    }
                    2 => {
                        groups.point(10, d.rd2()?);
                        groups.real(40, d.bd()?);
                        groups.degrees(50, d.bd()?);
                        groups.degrees(51, d.bd()?);
                        groups.int(73, i64::from(d.bit()?));
                    }
                    3 => {
                        groups.point(10, d.rd2()?);
                        groups.point(11, d.rd2()?);
                        groups.real(40, d.bd()?);
                        groups.degrees(50, d.bd()?);
                        groups.degrees(51, d.bd()?);
                        groups.int(73, i64::from(d.bit()?));
                    }
                    4 => {
                        let degree = d.bl()?;
                        let rational = d.bit()?;
                        let periodic = d.bit()?;
                        let knots = count_within(d.bl()?, d, 2, limits)?;
                        let controls = count_within(d.bl()?, d, 4, limits)?;
                        groups.int(94, degree);
                        groups.int(73, i64::from(rational));
                        groups.int(74, i64::from(periodic));
                        groups.int(95, knots as i64);
                        groups.int(96, controls as i64);
                        for _ in 0..knots {
                            groups.real(40, d.bd()?);
                        }
                        for _ in 0..controls {
                            groups.point(10, d.rd2()?);
                            if rational == 1 {
                                groups.real(42, d.bd()?);
                            }
                        }
                    }
                    _ => {
                        return Err(DwgError::Corrupt {
                            what: "a hatch edge type",
                            at: d.byte_position(),
                        });
                    }
                }
            }
        } else {
            let bulges = d.bit()? == 1;
            let closed = d.bit()?;
            let segments = count_within(d.bl()?, d, 4, limits)?;
            groups.int(72, i64::from(bulges));
            groups.int(73, i64::from(closed));
            groups.int(93, segments as i64);
            for _ in 0..segments {
                groups.point(10, d.rd2()?);
                if bulges {
                    groups.real(42, d.bd()?);
                }
            }
        }
        let sources = count_within(d.bl()?, d, 0, limits)?;
        boundary_handles += sources;
        groups.int(97, sources as i64);
    }
    let style = d.bs()?;
    let pattern_type = d.bs()?;
    groups.int(75, style);
    groups.int(76, pattern_type);
    if solid == 0 {
        let angle = d.bd()?;
        let scale = d.bd()?;
        let double = d.bit()?;
        let lines = d.bs()?;
        groups.degrees(52, angle);
        groups.real(41, scale);
        groups.int(77, i64::from(double));
        groups.int(78, lines);
        for _ in 0..lines.max(0) {
            let angle = d.bd()?;
            let base = d.bd2()?;
            let offset = d.bd2()?;
            let dashes = d.bs()?;
            groups.degrees(53, angle);
            groups.real(43, base.x);
            groups.real(44, base.y);
            groups.real(45, offset.x);
            groups.real(46, offset.y);
            groups.int(79, dashes);
            for _ in 0..dashes.max(0) {
                groups.real(49, d.bd()?);
            }
        }
    }
    if any_pixel {
        groups.real(47, d.bd()?);
    }
    let seeds = count_within(d.bl()?, d, 4, limits)?;
    groups.int(98, seeds as i64);
    for _ in 0..seeds {
        groups.point(10, d.rd2()?);
    }
    groups.0.extend(gradient.0);
    let _ = boundary_handles;
    Ok(())
}

/// A WIPEOUT, which is an IMAGE with no picture (§20.4.80).
fn wipeout(
    groups: &mut Groups,
    d: &mut BitReader<'_>,
    limits: &crate::ImportLimits,
) -> Result<(), DwgError> {
    let class_version = d.bl()?;
    let (insertion, z) = d.bd3()?;
    let (u, uz) = d.bd3()?;
    let (v, vz) = d.bd3()?;
    let size = d.rd2()?;
    let display = d.bs()?;
    let clipping = d.bit()?;
    let brightness = d.rc()?;
    let contrast = d.rc()?;
    let fade = d.rc()?;
    let boundary = d.bs()?;
    groups.int(90, class_version);
    groups.point3(10, insertion, z);
    groups.point3(11, u, uz);
    groups.point3(12, v, vz);
    groups.point(13, size);
    groups.int(70, display);
    groups.int(280, i64::from(clipping));
    groups.int(281, i64::from(brightness));
    groups.int(282, i64::from(contrast));
    groups.int(283, i64::from(fade));
    groups.int(71, boundary);
    if boundary == 1 {
        groups.int(91, 2);
        groups.point(14, d.rd2()?);
        groups.point(14, d.rd2()?);
    } else {
        let count = count_within(d.bl()?, d, 16, limits)?;
        groups.int(91, count as i64);
        for _ in 0..count {
            groups.point(14, d.rd2()?);
        }
    }
    Ok(())
}
