//! The objects that are records of the drawing rather than entities or
//! table rows (ODA spec §20.4): dictionaries, layouts, groups, sort
//! tables, xrecords, multileader styles, and the view table, decoded into
//! the DXF groups the reader of `cad-dxf` keeps raw and reads itself.

use crate::groups::Groups;
use crate::objects::{ObjectCommon, Streams};
use crate::{DwgError, File, Version};

/// An object decoded: its DXF kind and groups, with the handle it is
/// listed under and its owner.
#[derive(Clone, Debug, PartialEq)]
pub struct ObjectRecord {
    pub kind: &'static str,
    pub handle: u64,
    pub groups: Groups,
}

/// A DICTIONARY (§20.4.44), or one with a default (§20.4.45): each
/// entry's name and the handle it names.
pub fn dictionary(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
    with_default: bool,
) -> Result<ObjectRecord, DwgError> {
    let limits = file.limits();
    let count = usize::try_from(streams.data.bl()?).map_err(|_| DwgError::Corrupt {
        what: "a dictionary count",
        at: 0,
    })?;
    if count > limits.max_objects || count * 2 > streams.data.remaining_bits() {
        return Err(DwgError::Corrupt {
            what: "a dictionary count",
            at: 0,
        });
    }
    let cloning = streams.data.bs()?;
    let hard_owner = streams.data.rc()?;
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        names.push(streams.text(limits)?);
    }
    let handles = file.object_handles(streams, common)?;
    let mut groups = Groups::default();
    groups.handle(5, handle);
    groups.handle(330, handles.owner.resolve(handle));
    groups.int(280, i64::from(hard_owner));
    groups.int(281, cloning);
    let code = if hard_owner & 1 != 0 { 360 } else { 350 };
    for name in names {
        let item = streams.handles.handle()?.resolve(handle);
        groups.text(3, &name);
        groups.handle(code, item);
    }
    if with_default {
        let default = streams.handles.handle()?.resolve(handle);
        groups.handle(340, default);
    }
    Ok(ObjectRecord {
        kind: "DICTIONARY",
        handle,
        groups,
    })
}

/// A LAYOUT (§20.4.84): its plot settings under `AcDbPlotSettings`, then
/// its own fields under `AcDbLayout`, with the block record it names.
pub fn layout(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let version = file.version();
    let limits = file.limits();
    let mut groups = Groups::default();
    groups.handle(5, handle);
    groups.text(100, "AcDbPlotSettings");
    let page_setup = streams.text(limits)?;
    let printer = streams.text(limits)?;
    let d = &mut streams.data;
    let plot_flags = d.bs()?;
    let margins = [d.bd()?, d.bd()?, d.bd()?, d.bd()?];
    let paper_width = d.bd()?;
    let paper_height = d.bd()?;
    let paper_size = streams.text(limits)?;
    let d = &mut streams.data;
    let origin = d.bd2()?;
    let paper_units = d.bs()?;
    let rotation = d.bs()?;
    let plot_type = d.bs()?;
    let window_min = d.bd2()?;
    let window_max = d.bd2()?;
    let mut view_name = String::new();
    if version < Version::R2004 {
        view_name = streams.text(limits)?;
    }
    let d = &mut streams.data;
    let real_units = d.bd()?;
    let drawing_units = d.bd()?;
    let style_sheet = streams.text(limits)?;
    let d = &mut streams.data;
    let scale_type = d.bs()?;
    let scale_factor = d.bd()?;
    let image_origin = d.bd2()?;
    let mut shade = None;
    if version >= Version::R2004 {
        shade = Some((d.bs()?, d.bs()?, d.bs()?));
    }
    groups.text(1, &page_setup);
    groups.text(2, &printer);
    groups.text(4, &paper_size);
    groups.text(6, &view_name);
    groups.real(40, margins[0]);
    groups.real(41, margins[1]);
    groups.real(42, margins[2]);
    groups.real(43, margins[3]);
    groups.real(44, paper_width);
    groups.real(45, paper_height);
    groups.real(46, origin.x);
    groups.real(47, origin.y);
    groups.real(48, window_min.x);
    groups.real(49, window_min.y);
    groups.real(140, window_max.x);
    groups.real(141, window_max.y);
    groups.real(142, real_units);
    groups.real(143, drawing_units);
    groups.int(70, plot_flags);
    groups.int(72, paper_units);
    groups.int(73, rotation);
    groups.int(74, plot_type);
    groups.text(7, &style_sheet);
    groups.int(75, scale_type);
    groups.real(147, scale_factor);
    groups.real(148, image_origin.x);
    groups.real(149, image_origin.y);
    if let Some((mode, level, dpi)) = shade {
        groups.int(76, mode);
        groups.int(77, level);
        groups.int(78, dpi);
    }
    let name = streams.text(limits)?;
    let d = &mut streams.data;
    let tab_order = d.bl()?;
    let flags = d.bs()?;
    let (insertion, iz) = d.bd3()?;
    let limmin = d.rd2()?;
    let limmax = d.rd2()?;
    let (ucs_origin, uz) = d.bd3()?;
    let (ucs_x, uxz) = d.bd3()?;
    let (ucs_y, uyz) = d.bd3()?;
    let elevation = d.bd()?;
    let ortho = d.bs()?;
    let (extmin, emz) = d.bd3()?;
    let (extmax, exz) = d.bd3()?;
    let mut viewports = 0usize;
    if version >= Version::R2004 {
        // A BL, where the specification says RL (dwg-matrix errata).
        viewports = usize::try_from(d.bl()?).unwrap_or(usize::MAX);
        if viewports.saturating_mul(8) > streams.handles.remaining_bits() {
            return Err(DwgError::Corrupt {
                what: "a layout's viewport count",
                at: 0,
            });
        }
    }
    file.object_handles(streams, common)?;
    let h = &mut streams.handles;
    if version >= Version::R2004 {
        h.handle()?; // the plot view
    }
    if version >= Version::R2007 {
        h.handle()?; // the visual style
    }
    let block_record = h.handle()?.resolve(handle);
    let active_viewport = h.handle()?.resolve(handle);
    h.handle()?; // the base UCS
    h.handle()?; // the named UCS
    for _ in 0..viewports {
        h.handle()?;
    }
    groups.text(100, "AcDbLayout");
    groups.text(1, &name);
    groups.int(70, flags);
    groups.int(71, tab_order);
    groups.point(10, limmin);
    groups.point(11, limmax);
    groups.point3(12, insertion, iz);
    groups.point3(14, extmin, emz);
    groups.point3(15, extmax, exz);
    groups.real(146, elevation);
    groups.point3(13, ucs_origin, uz);
    groups.point3(16, ucs_x, uxz);
    groups.point3(17, ucs_y, uyz);
    groups.int(76, ortho);
    groups.handle(330, block_record);
    if active_viewport != 0 {
        groups.handle(331, active_viewport);
    }
    Ok(ObjectRecord {
        kind: "LAYOUT",
        handle,
        groups,
    })
}

/// A GROUP (§20.4.72): its description, flags, and members.
pub fn group(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let limits = file.limits();
    let description = streams.text(limits)?;
    let unnamed = streams.data.bs()?;
    let selectable = streams.data.bs()?;
    let count = usize::try_from(streams.data.bl()?).map_err(|_| DwgError::Corrupt {
        what: "a group count",
        at: 0,
    })?;
    if count * 8 > streams.handles.remaining_bits() {
        return Err(DwgError::Corrupt {
            what: "a group count",
            at: 0,
        });
    }
    let handles = file.object_handles(streams, common)?;
    let mut groups = Groups::default();
    groups.handle(5, handle);
    groups.handle(330, handles.owner.resolve(handle));
    groups.text(300, &description);
    groups.int(70, unnamed);
    groups.int(71, selectable);
    for _ in 0..count {
        let member = streams.handles.handle()?.resolve(handle);
        groups.handle(340, member);
    }
    Ok(ObjectRecord {
        kind: "GROUP",
        handle,
        groups,
    })
}

/// A SORTENTSTABLE (§20.4.93): the block record it orders, and each
/// entity's sort handle.
pub fn sortents_table(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let count = usize::try_from(streams.data.bl()?).map_err(|_| DwgError::Corrupt {
        what: "a sort table count",
        at: 0,
    })?;
    if count * 8 > streams.data.remaining_bits() || count * 8 > streams.handles.remaining_bits() {
        return Err(DwgError::Corrupt {
            what: "a sort table count",
            at: 0,
        });
    }
    let mut sort_handles = Vec::with_capacity(count);
    for _ in 0..count {
        sort_handles.push(streams.data.handle()?.resolve(handle));
    }
    file.object_handles(streams, common)?;
    let owner = streams.handles.handle()?.resolve(handle);
    let mut groups = Groups::default();
    groups.handle(5, handle);
    groups.text(100, "AcDbSortentsTable");
    groups.handle(330, owner);
    for sort in sort_handles {
        let entity = streams.handles.handle()?.resolve(handle);
        groups.handle(331, entity);
        groups.handle(5, sort);
    }
    Ok(ObjectRecord {
        kind: "SORTENTSTABLE",
        handle,
        groups,
    })
}

/// An XRECORD (§20.4.104): its data as the DXF groups it is a sequence of,
/// each value typed by its code as DXF does.
pub fn xrecord(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let version = file.version();
    let limits = file.limits();
    let bytes = usize::try_from(streams.data.bl()?).map_err(|_| DwgError::Corrupt {
        what: "an xrecord size",
        at: 0,
    })?;
    if bytes * 8 > streams.data.remaining_bits() {
        return Err(DwgError::Corrupt {
            what: "an xrecord size",
            at: 0,
        });
    }
    let start = streams.data.position();
    let end = start + bytes * 8;
    let mut data = streams.data.window(start, end)?;
    let mut groups = Groups::default();
    groups.handle(5, handle);
    let mut body = Groups::default();
    while data.remaining_bits() >= 16 {
        if body.0.len() >= limits.max_objects {
            return Err(DwgError::TooMany {
                what: "groups in an xrecord",
                limit: limits.max_objects,
            });
        }
        let code = i32::from(data.rs()?);
        match xrecord_kind(code) {
            XKind::Text => {
                // R2007+: a count of UTF-16 units, then the units; before:
                // a byte count, the code page, the bytes.
                let text = if version >= Version::R2007 {
                    let n = usize::from(data.rs()?);
                    if n > limits.max_string_bytes || n * 16 > data.remaining_bits() {
                        return Err(DwgError::Corrupt {
                            what: "an xrecord string",
                            at: data.byte_position(),
                        });
                    }
                    let mut units = Vec::with_capacity(n);
                    for _ in 0..n {
                        units.push(data.rs()?);
                    }
                    String::from_utf16_lossy(&units)
                } else {
                    let n = usize::from(data.rs()?);
                    data.rc()?; // the code page
                    if n > limits.max_string_bytes || n * 8 > data.remaining_bits() {
                        return Err(DwgError::Corrupt {
                            what: "an xrecord string",
                            at: data.byte_position(),
                        });
                    }
                    let mut raw = Vec::with_capacity(n);
                    for _ in 0..n {
                        raw.push(data.rc()?);
                    }
                    String::from_utf8_lossy(&raw).into_owned()
                };
                body.text(code, &text);
            }
            XKind::Point => {
                let x = data.rd()?;
                let y = data.rd()?;
                let z = data.rd()?;
                body.real(code, x);
                body.real(code + 10, y);
                body.real(code + 20, z);
            }
            XKind::Real => body.real(code, data.rd()?),
            XKind::Int8 => body.int(code, i64::from(data.rc()?)),
            XKind::Int16 => body.int(code, i64::from(data.rs()? as i16)),
            XKind::Int32 => body.int(code, i64::from(data.rl()? as i32)),
            XKind::Int64 => body.int(code, data.rll()? as i64),
            XKind::Handle => body.handle(code, data.rll()?),
            XKind::Binary => {
                let n = usize::from(data.rc()?);
                let mut hex = String::with_capacity(n * 2);
                for _ in 0..n {
                    hex.push_str(&format!("{:02X}", data.rc()?));
                }
                body.text(code, &hex);
            }
            XKind::Invalid => {
                return Err(DwgError::Corrupt {
                    what: "an xrecord group code",
                    at: data.byte_position(),
                });
            }
        }
    }
    streams.data.set_position(end)?;
    let cloning = streams.data.bs()?;
    let handles = file.object_handles(streams, common)?;
    groups.handle(330, handles.owner.resolve(handle));
    groups.text(100, "AcDbXrecord");
    groups.int(280, cloning);
    groups.0.extend(body.0);
    Ok(ObjectRecord {
        kind: "XRECORD",
        handle,
        groups,
    })
}

/// How a DXF group's value is stored, by its code: the DXF convention.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum XKind {
    Text,
    Point,
    Real,
    Int8,
    Int16,
    Int32,
    Int64,
    Handle,
    Binary,
    Invalid,
}

fn xrecord_kind(code: i32) -> XKind {
    match code {
        0..=9 => XKind::Text,
        10..=37 => XKind::Point,
        38..=59 => XKind::Real,
        60..=79 => XKind::Int16,
        80..=99 => XKind::Int32,
        100..=104 => XKind::Text,
        105 => XKind::Handle,
        110..=139 => XKind::Point,
        140..=149 => XKind::Real,
        160..=169 => XKind::Int64,
        170..=179 => XKind::Int16,
        210..=269 => XKind::Point,
        270..=279 => XKind::Int16,
        280..=289 => XKind::Int8,
        290..=299 => XKind::Int8,
        300..=309 => XKind::Text,
        310..=319 => XKind::Binary,
        320..=369 => XKind::Handle,
        370..=389 => XKind::Int16,
        390..=399 => XKind::Handle,
        400..=409 => XKind::Int16,
        410..=419 => XKind::Text,
        420..=429 => XKind::Int32,
        430..=439 => XKind::Text,
        440..=459 => XKind::Int32,
        460..=469 => XKind::Real,
        470..=479 => XKind::Text,
        999 => XKind::Text,
        1000..=1003 | 1005..=1009 => XKind::Text,
        1004 => XKind::Binary,
        1010..=1039 => XKind::Point,
        1040..=1042 => XKind::Real,
        1070 => XKind::Int16,
        1071 => XKind::Int32,
        _ => XKind::Invalid,
    }
}

/// A MLEADERSTYLE (§20.4.87): the values the drawing holds, under their
/// DXF codes, and the block it names.
pub fn mleader_style(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let version = file.version();
    let limits = file.limits();
    let d = &mut streams.data;
    let content_type = d.bs()?;
    let mleader_order = d.bs()?;
    let leader_order = d.bs()?;
    let max_points = d.bl()?;
    let first_angle = d.bd()?;
    let second_angle = d.bd()?;
    let leader_type = d.bs()?;
    let line_color = d.cmc(version, limits)?;
    // The linetype's handle is in the handle stream; the lineweight
    // follows the colour in the data.
    let lineweight = d.bl()?;
    let landing = d.bit()?;
    let landing_gap = d.bd()?;
    let dogleg = d.bit()?;
    let landing_distance = d.bd()?;
    let description = streams.text(limits)?;
    let d = &mut streams.data;
    let arrow_size = d.bd()?;
    let text_default = streams.text(limits)?;
    let d = &mut streams.data;
    let left = d.bs()?;
    let right = d.bs()?;
    let angle_type = d.bs()?;
    let alignment = d.bs()?;
    let text_color = d.cmc(version, limits)?;
    let text_height = d.bd()?;
    let frame = d.bit()?;
    let always_left = d.bit()?;
    let align_space = d.bd()?;
    let block_color = d.cmc(version, limits)?;
    let (scale, sz) = d.bd3()?;
    let use_scale = d.bit()?;
    let block_rotation = d.bd()?;
    let use_rotation = d.bit()?;
    let connection = d.bs()?;
    let scale_factor = d.bd()?;
    let changed = d.bit()?;
    let annotative = d.bit()?;
    let break_size = d.bd()?;
    file.object_handles(streams, common)?;
    let h = &mut streams.handles;
    let linetype = h.handle()?.resolve(handle);
    let arrow = h.handle()?.resolve(handle);
    let text_style = h.handle()?.resolve(handle);
    let block = h.handle()?.resolve(handle);
    let mut groups = Groups::default();
    groups.handle(5, handle);
    groups.int(170, content_type);
    groups.int(171, mleader_order);
    groups.int(172, leader_order);
    groups.int(90, max_points);
    groups.degrees(40, first_angle);
    groups.degrees(41, second_angle);
    groups.int(173, leader_type);
    groups.int(91, i64::from(line_color.index));
    if linetype != 0 {
        groups.handle(340, linetype);
    }
    groups.int(92, lineweight);
    groups.int(290, i64::from(landing));
    groups.real(42, landing_gap);
    groups.int(291, i64::from(dogleg));
    groups.real(43, landing_distance);
    groups.text(3, &description);
    if arrow != 0 {
        groups.handle(341, arrow);
    }
    groups.real(44, arrow_size);
    groups.text(300, &text_default);
    if text_style != 0 {
        groups.handle(342, text_style);
    }
    groups.int(174, left);
    groups.int(178, right);
    groups.int(175, angle_type);
    groups.int(176, alignment);
    groups.int(93, i64::from(text_color.index));
    groups.real(45, text_height);
    groups.int(292, i64::from(frame));
    groups.int(297, i64::from(always_left));
    groups.real(46, align_space);
    if block != 0 {
        groups.handle(343, block);
    }
    groups.int(94, i64::from(block_color.index));
    groups.real(47, scale.x);
    groups.real(49, scale.y);
    groups.real(140, sz);
    groups.int(293, i64::from(use_scale));
    groups.degrees(141, block_rotation);
    groups.int(294, i64::from(use_rotation));
    groups.int(177, connection);
    groups.real(142, scale_factor);
    groups.int(295, i64::from(changed));
    groups.int(296, i64::from(annotative));
    groups.real(143, break_size);
    Ok(ObjectRecord {
        kind: "MLEADERSTYLE",
        handle,
        groups,
    })
}

/// A WIPEOUTVARIABLES object: whether wipeouts show their frames.
pub fn wipeout_variables(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let frame = streams.data.bs()?;
    file.object_handles(streams, common)?;
    let mut groups = Groups::default();
    groups.handle(5, handle);
    groups.int(70, frame);
    Ok(ObjectRecord {
        kind: "WIPEOUTVARIABLES",
        handle,
        groups,
    })
}

/// A VIEW row (§20.4.60): the named view's size and centre, and whether
/// it is of paper.
pub fn view(
    file: &File<'_>,
    streams: &mut Streams<'_>,
    common: &ObjectCommon,
    handle: u64,
) -> Result<ObjectRecord, DwgError> {
    let version = file.version();
    let limits = file.limits();
    let name = streams.text(limits)?;
    let d = &mut streams.data;
    if version >= Version::R2007 {
        d.bs()?;
    } else {
        d.bit()?;
        d.bs()?;
        d.bit()?;
    }
    let height = d.bd()?;
    let width = d.bd()?;
    let center = d.rd2()?;
    let (target, tz) = d.bd3()?;
    let (direction, dz) = d.bd3()?;
    let twist = d.bd()?;
    let lens = d.bd()?;
    let front = d.bd()?;
    let back = d.bd()?;
    let mode = d.bits(4)?;
    let render_mode = d.rc()?;
    if version >= Version::R2007 {
        d.bit()?;
        d.rc()?;
        d.bd()?;
        d.bd()?;
        d.cmc(version, limits)?;
    }
    let paper = d.bit()?;
    let associated_ucs = d.bit()?;
    let mut ucs = None;
    if associated_ucs == 1 {
        let (origin, oz) = d.bd3()?;
        let (x, xz) = d.bd3()?;
        let (y, yz) = d.bd3()?;
        let elevation = d.bd()?;
        let ortho = d.bs()?;
        ucs = Some((origin, oz, x, xz, y, yz, elevation, ortho));
    }
    if version >= Version::R2007 {
        d.bit()?; // camera plottable
    }
    file.object_handles(streams, common)?;
    let mut groups = Groups::default();
    groups.text(2, &name);
    groups.int(70, i64::from(paper));
    groups.real(40, height);
    groups.point(10, center);
    groups.real(41, width);
    groups.point3(11, direction, dz);
    groups.point3(12, target, tz);
    groups.real(42, lens);
    groups.real(43, front);
    groups.real(44, back);
    groups.degrees(50, twist);
    let view_mode = (mode & 7) | if mode & 8 == 0 { 16 } else { 0 };
    groups.int(71, view_mode as i64);
    groups.int(281, i64::from(render_mode));
    groups.int(72, i64::from(associated_ucs));
    if let Some((origin, oz, x, xz, y, yz, elevation, ortho)) = ucs {
        groups.point3(110, origin, oz);
        groups.point3(111, x, xz);
        groups.point3(112, y, yz);
        groups.real(146, elevation);
        groups.int(79, ortho);
    }
    groups.handle(5, handle);
    Ok(ObjectRecord {
        kind: "VIEW",
        handle,
        groups,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_codes_are_typed_as_dxf_types_them() {
        assert_eq!(xrecord_kind(1), XKind::Text);
        assert_eq!(xrecord_kind(10), XKind::Point);
        assert_eq!(xrecord_kind(40), XKind::Real);
        assert_eq!(xrecord_kind(70), XKind::Int16);
        assert_eq!(xrecord_kind(90), XKind::Int32);
        assert_eq!(xrecord_kind(160), XKind::Int64);
        assert_eq!(xrecord_kind(280), XKind::Int8);
        assert_eq!(xrecord_kind(290), XKind::Int8);
        assert_eq!(xrecord_kind(301), XKind::Text);
        assert_eq!(xrecord_kind(310), XKind::Binary);
        assert_eq!(xrecord_kind(330), XKind::Handle);
        assert_eq!(xrecord_kind(370), XKind::Int16);
        assert_eq!(xrecord_kind(440), XKind::Int32);
        assert_eq!(xrecord_kind(1004), XKind::Binary);
        assert_eq!(xrecord_kind(500), XKind::Invalid);
    }
}
