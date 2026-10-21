//! Table rows, control objects, and block headers encoded by their
//! prescriptions (ODA spec §20.4.49–20.4.68, LibreDWG's `dwg.spec` where
//! the specification is silent): the reverse of `tables.rs`.

use cad_core::Point2;

use crate::objects::types;
use crate::write::fields::{Fields, lineweight_index};
use crate::write::objects::{ObjectBytes, ObjectWriter};
use crate::write::{BlockPlacement, Names, WriteError};

/// The head every row starts with (§20.4.54 and the rest): the name, the
/// 64 flag, no xref, not dependent.
fn row_head(w: &mut ObjectWriter, name: &str) {
    w.data.text(name);
    w.data.bit(0);
    w.data.bs(0);
    w.data.bit(0);
}

/// A control object (§20.4.49, …): the count of its entries, then the
/// entries as soft owners, and what the kind names after them.
pub fn control(
    name: &str,
    handle: u64,
    xdic: u64,
    entries: &[u64],
    names: &Names,
) -> Result<ObjectBytes, WriteError> {
    let (type_code, long_count) = match name {
        "BLOCK_RECORD" => (types::BLOCK_CONTROL, true),
        "LAYER" => (types::LAYER_CONTROL, true),
        "STYLE" => (types::STYLE_CONTROL, true),
        "LTYPE" => (types::LTYPE_CONTROL, false),
        "VIEW" => (types::VIEW_CONTROL, true),
        "UCS" => (types::UCS_CONTROL, false),
        "VPORT" => (types::VPORT_CONTROL, false),
        "APPID" => (types::APPID_CONTROL, false),
        "DIMSTYLE" => (types::DIMSTYLE_CONTROL, false),
        other => return Err(WriteError::new(other, "a table this crate does not write")),
    };
    // The block and linetype controls keep their built-in rows apart from
    // the entries.
    let listed: Vec<u64> = match name {
        "BLOCK_RECORD" => entries
            .iter()
            .copied()
            .filter(|h| *h != names.model_space && *h != names.paper_space)
            .collect(),
        "LTYPE" => entries
            .iter()
            .copied()
            .filter(|h| *h != names.ltype_byblock && *h != names.ltype_bylayer)
            .collect(),
        _ => entries.to_vec(),
    };
    let mut w = ObjectWriter::new(type_code, handle, names.version);
    w.object_common(xdic);
    if long_count {
        w.data.bl(listed.len() as i32);
    } else {
        w.data.bs(listed.len() as i16);
    }
    if name == "DIMSTYLE" {
        w.data.rc(0); // no more handles
    }
    w.object_handles(0, xdic);
    for entry in &listed {
        w.handles.handle(2, *entry);
    }
    match name {
        "BLOCK_RECORD" => {
            w.handles.handle(3, names.model_space);
            w.handles.handle(3, names.paper_space);
        }
        "LTYPE" => {
            w.handles.handle(3, names.ltype_byblock);
            w.handles.handle(3, names.ltype_bylayer);
        }
        _ => {}
    }
    Ok(w.finish())
}

/// The viewport entity header control (§20.4.70), empty: R2000 files
/// carry one.
pub fn viewport_entity_header_control(handle: u64) -> ObjectBytes {
    let mut w = ObjectWriter::new(
        types::VIEWPORT_ENTITY_HEADER_CONTROL,
        handle,
        crate::Version::R2000,
    );
    w.object_common(0);
    w.data.bs(0);
    w.object_handles(0, 0);
    w.finish()
}

/// A table row of `table`, owned by its control.
pub fn row(
    table: &str,
    groups: &[(i32, String)],
    control: u64,
    names: &Names,
    block: Option<&BlockPlacement>,
) -> Result<ObjectBytes, WriteError> {
    let f = Fields::new(groups);
    let handle = f
        .handle(5)
        .or_else(|| f.handle(105))
        .ok_or_else(|| WriteError::new(table, "a row without a handle"))?;
    let name = f.text(2).unwrap_or("").trim();
    Ok(match table {
        "LAYER" => layer(&f, handle, name, control, names),
        "LTYPE" => linetype(&f, handle, name, control, names),
        "STYLE" => text_style(&f, handle, name, control, names),
        "DIMSTYLE" => dim_style(&f, handle, name, control, names),
        "VPORT" => vport(&f, handle, name, control, names),
        "APPID" => {
            let mut w = ObjectWriter::new(types::APPID, handle, names.version);
            w.object_common(0);
            row_head(&mut w, name);
            w.data.rc(f.int(71).unwrap_or(0) as u8);
            w.object_handles(control, 0);
            w.handles.handle(5, 0); // the xref block
            w.finish()
        }
        "VIEW" => view(&f, handle, name, control, names),
        "BLOCK_RECORD" => block_header(&f, handle, name, control, block, names),
        other => return Err(WriteError::new(other, "a table this crate does not write")),
    })
}

/// A LAYER row (§20.4.54).
fn layer(f: &Fields<'_>, handle: u64, name: &str, control: u64, names: &Names) -> ObjectBytes {
    let mut w = ObjectWriter::new(types::LAYER, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    let flags = f.int(70).unwrap_or(0);
    let color = f.int(62).unwrap_or(7);
    let lineweight = lineweight_index(f.int(370).unwrap_or(-3));
    let mut values: u16 = 0;
    if flags & 1 != 0 {
        values |= 1; // frozen
    }
    if color < 0 {
        values |= 2; // off
    }
    if flags & 2 != 0 {
        values |= 4; // frozen in new viewports
    }
    if flags & 4 != 0 {
        values |= 8; // locked
    }
    values |= 16; // plots
    values |= (u16::from(lineweight) & 0x1f) << 5;
    w.data.bs(values as i16);
    w.cmc(color.unsigned_abs().min(256) as i16);
    w.object_handles(control, 0);
    w.handles.handle(5, 0); // the xref block
    w.handles.handle(5, f.handle(390).unwrap_or(0)); // the plot style
    let linetype = f
        .text(6)
        .and_then(|n| names.linetypes.get(&n.trim().to_uppercase()))
        .copied()
        .unwrap_or(names.ltype_continuous);
    w.handles.handle(5, linetype);
    w.finish()
}

/// An LTYPE row (§20.4.58): each dash with its shape fields, the text
/// area empty.
fn linetype(f: &Fields<'_>, handle: u64, name: &str, control: u64, names: &Names) -> ObjectBytes {
    let mut w = ObjectWriter::new(types::LTYPE, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    w.data.text(f.text(3).unwrap_or(""));
    w.data.bd(f.real(40).unwrap_or(0.0));
    w.data.rc(f.int(72).unwrap_or(65) as u8);
    // Each dash starts at its 49; its shape groups follow.
    let groups = f.groups();
    let starts: Vec<usize> = groups
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| *c == 49)
        .map(|(i, _)| i)
        .collect();
    w.data.rc(starts.len() as u8);
    for (n, &start) in starts.iter().enumerate() {
        let stop = starts.get(n + 1).copied().unwrap_or(groups.len());
        let dash = Fields::new(&groups[start..stop]);
        let d = &mut w.data;
        d.bd(dash.real(49).unwrap_or(0.0));
        d.bs(dash.int(75).unwrap_or(0) as i16);
        d.rd(dash.real(44).unwrap_or(0.0));
        d.rd(dash.real(45).unwrap_or(0.0));
        d.bd(dash.real(46).unwrap_or(0.0));
        d.bd(dash.real(50).unwrap_or(0.0).to_radians());
        d.bs(dash.int(74).unwrap_or(0) as i16);
    }
    for _ in 0..256 {
        w.data.rc(0);
    }
    w.object_handles(control, 0);
    w.handles.handle(5, 0); // the xref block
    for _ in &starts {
        w.handles.handle(5, 0); // the dash's shape style
    }
    w.finish()
}

/// A STYLE row (§20.4.56): the shape bit before the vertical bit.
fn text_style(f: &Fields<'_>, handle: u64, name: &str, control: u64, names: &Names) -> ObjectBytes {
    let mut w = ObjectWriter::new(types::STYLE, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    let flags = f.int(70).unwrap_or(0);
    let d = &mut w.data;
    d.bit(u8::from(flags & 1 != 0));
    d.bit(u8::from(flags & 4 != 0));
    d.bd(f.real(40).unwrap_or(0.0));
    d.bd(f.real(41).unwrap_or(1.0));
    d.bd(f.real(50).unwrap_or(0.0).to_radians());
    d.rc(f.int(71).unwrap_or(0) as u8);
    d.bd(f.real(42).unwrap_or(2.5));
    d.text(f.text(3).unwrap_or("txt"));
    d.text(f.text(4).unwrap_or(""));
    w.object_handles(control, 0);
    w.handles.handle(5, 0); // the xref block
    w.finish()
}

/// The values a DIMSTYLE row holds that a DXF row may leave out: the
/// defaults of a new drawing, by DXF code.
const DIMSTYLE_REALS: &[(i32, f64)] = &[
    (40, 1.0),
    (41, 0.18),
    (42, 0.0625),
    (43, 0.38),
    (44, 0.18),
    (45, 0.0),
    (46, 0.0),
    (47, 0.0),
    (48, 0.0),
    (140, 0.18),
    (141, 0.09),
    (142, 0.0),
    (143, 25.4),
    (144, 1.0),
    (145, 0.0),
    (146, 1.0),
    (147, 0.09),
    (148, 0.0),
];

const DIMSTYLE_INTS: &[(i32, i64)] = &[
    (71, 0),
    (72, 0),
    (73, 1),
    (74, 1),
    (75, 0),
    (76, 0),
    (77, 0),
    (78, 0),
    (79, 0),
    (170, 0),
    (171, 2),
    (172, 0),
    (173, 0),
    (174, 0),
    (175, 0),
    (176, 0),
    (177, 0),
    (178, 0),
    (179, 0),
    (271, 4),
    (272, 4),
    (273, 2),
    (274, 2),
    (275, 0),
    (276, 0),
    (277, 2),
    (278, 46),
    (279, 0),
    (280, 0),
    (281, 0),
    (282, 0),
    (283, 1),
    (284, 0),
    (285, 0),
    (286, 0),
    (287, 3),
    (288, 0),
    (371, -2),
    (372, -2),
];

/// A DIMSTYLE row (§20.4.68), R2000: every variable in order.
fn dim_style(f: &Fields<'_>, handle: u64, name: &str, control: u64, names: &Names) -> ObjectBytes {
    let real = |code: i32| {
        f.real(code).unwrap_or_else(|| {
            DIMSTYLE_REALS
                .iter()
                .find(|(c, _)| *c == code)
                .map_or(0.0, |(_, v)| *v)
        })
    };
    let int = |code: i32| {
        f.int(code).unwrap_or_else(|| {
            DIMSTYLE_INTS
                .iter()
                .find(|(c, _)| *c == code)
                .map_or(0, |(_, v)| *v)
        })
    };
    let mut w = ObjectWriter::new(types::DIMSTYLE, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    w.data.text(f.text(3).unwrap_or(""));
    w.data.text(f.text(4).unwrap_or(""));
    let d = &mut w.data;
    for code in 40..=48 {
        d.bd(real(code));
    }
    for code in 71..=76 {
        d.bit(u8::from(int(code) != 0));
    }
    for code in 77..=79 {
        d.bs(int(code) as i16);
    }
    for code in 140..=148 {
        d.bd(real(code));
    }
    d.bit(u8::from(int(170) != 0));
    d.bs(int(171) as i16);
    for code in 172..=175 {
        d.bit(u8::from(int(code) != 0));
    }
    for code in 176..=178 {
        w.cmc(int(code).clamp(0, 256) as i16);
    }
    let d = &mut w.data;
    d.bs(int(179) as i16);
    for code in 271..=280 {
        d.bs(int(code) as i16);
    }
    for code in 281..=282 {
        d.bit(u8::from(int(code) != 0));
    }
    for code in 283..=286 {
        d.bs(int(code) as i16);
    }
    d.bit(u8::from(int(288) != 0));
    d.bs(int(287) as i16);
    d.bs(int(371) as i16);
    d.bs(int(372) as i16);
    d.bit(0);
    w.object_handles(control, 0);
    let h = &mut w.handles;
    h.handle(5, 0); // the xref block
    let text_style = f
        .text(340)
        .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
        .or_else(|| {
            f.text(7)
                .and_then(|n| names.text_styles.get(&n.trim().to_uppercase()).copied())
        })
        .unwrap_or(names.standard_style);
    h.handle(5, text_style);
    for _ in 0..4 {
        h.handle(5, 0); // DIMLDRBLK, DIMBLK, DIMBLK1, DIMBLK2
    }
    w.finish()
}

/// A VPORT row (§20.4.64).
fn vport(f: &Fields<'_>, handle: u64, name: &str, control: u64, names: &Names) -> ObjectBytes {
    let mut w = ObjectWriter::new(types::VPORT, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    let height = f.real(40).unwrap_or(1.0);
    let d = &mut w.data;
    d.bd(height);
    d.bd(f.real(41).unwrap_or(1.0) * height);
    d.rd2(f.point(12).unwrap_or(Point2::ORIGIN));
    d.bd3(f.point(17).unwrap_or(Point2::ORIGIN), f.z(17));
    d.bd3(
        f.point(16).unwrap_or(Point2::ORIGIN),
        f.real(36).unwrap_or(1.0),
    );
    d.bd(f.real(51).unwrap_or(0.0).to_radians());
    d.bd(f.real(42).unwrap_or(50.0));
    d.bd(f.real(43).unwrap_or(0.0));
    d.bd(f.real(44).unwrap_or(0.0));
    let mode = f.int(71).unwrap_or(0);
    d.bits(4, ((mode & 7) | if mode & 16 == 0 { 8 } else { 0 }) as u64);
    d.rc(f.int(281).unwrap_or(0) as u8);
    d.rd2(f.point(10).unwrap_or(Point2::ORIGIN));
    d.rd2(f.point(11).unwrap_or(Point2::new(1.0, 1.0)));
    d.bit(u8::from(mode & 8 != 0)); // UCSFOLLOW
    d.bs(f.int(72).unwrap_or(1000) as i16);
    d.bit(u8::from(f.int(73).unwrap_or(1) != 0));
    d.bits(2, (f.int(74).unwrap_or(3) & 3) as u64);
    d.bit(u8::from(f.int(76).unwrap_or(0) != 0));
    d.rd2(f.point(15).unwrap_or(Point2::new(0.5, 0.5)));
    d.bit(u8::from(f.int(75).unwrap_or(0) != 0));
    d.bit(u8::from(f.int(77).unwrap_or(0) != 0));
    d.bs(f.int(78).unwrap_or(0) as i16);
    d.bd(f.real(50).unwrap_or(0.0).to_radians());
    d.rd2(f.point(13).unwrap_or(Point2::ORIGIN));
    d.rd2(f.point(14).unwrap_or(Point2::new(0.5, 0.5)));
    d.bit(0);
    d.bit(u8::from(f.int(65).unwrap_or(0) != 0));
    d.bd3(f.point(110).unwrap_or(Point2::ORIGIN), f.z(110));
    d.bd3(f.point(111).unwrap_or(Point2::new(1.0, 0.0)), f.z(111));
    d.bd3(f.point(112).unwrap_or(Point2::new(0.0, 1.0)), f.z(112));
    d.bd(f.real(146).unwrap_or(0.0));
    d.bs(f.int(79).unwrap_or(0) as i16);
    w.object_handles(control, 0);
    let h = &mut w.handles;
    h.handle(5, 0); // the xref block
    h.handle(5, 0); // the named UCS
    h.handle(5, 0); // the base UCS
    w.finish()
}

/// A VIEW row (§20.4.60).
fn view(f: &Fields<'_>, handle: u64, name: &str, control: u64, names: &Names) -> ObjectBytes {
    let mut w = ObjectWriter::new(types::VIEW, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    let d = &mut w.data;
    d.bd(f.real(40).unwrap_or(1.0));
    d.bd(f.real(41).unwrap_or(1.0));
    d.rd2(f.point(10).unwrap_or(Point2::ORIGIN));
    d.bd3(f.point(12).unwrap_or(Point2::ORIGIN), f.z(12));
    d.bd3(
        f.point(11).unwrap_or(Point2::ORIGIN),
        f.real(31).unwrap_or(1.0),
    );
    d.bd(f.real(50).unwrap_or(0.0).to_radians());
    d.bd(f.real(42).unwrap_or(50.0));
    d.bd(f.real(43).unwrap_or(0.0));
    d.bd(f.real(44).unwrap_or(0.0));
    let mode = f.int(71).unwrap_or(0);
    d.bits(4, ((mode & 7) | if mode & 16 == 0 { 8 } else { 0 }) as u64);
    d.rc(f.int(281).unwrap_or(0) as u8);
    d.bit(u8::from(f.int(70).unwrap_or(0) & 1 != 0));
    let ucs = f.int(72).unwrap_or(0) != 0;
    d.bit(u8::from(ucs));
    if ucs {
        d.bd3(f.point(110).unwrap_or(Point2::ORIGIN), f.z(110));
        d.bd3(f.point(111).unwrap_or(Point2::new(1.0, 0.0)), f.z(111));
        d.bd3(f.point(112).unwrap_or(Point2::new(0.0, 1.0)), f.z(112));
        d.bd(f.real(146).unwrap_or(0.0));
        d.bs(f.int(79).unwrap_or(0) as i16);
    }
    w.object_handles(control, 0);
    let h = &mut w.handles;
    h.handle(5, 0); // the xref block
    h.handle(5, 0); // the base UCS
    h.handle(5, 0); // the named UCS
    w.finish()
}

/// A block header (§20.4.52): the block's flags and base, the BLOCK and
/// ENDBLK entities and the chain between them, its layout.
fn block_header(
    f: &Fields<'_>,
    handle: u64,
    name: &str,
    control: u64,
    block: Option<&BlockPlacement>,
    names: &Names,
) -> ObjectBytes {
    let mut w = ObjectWriter::new(types::BLOCK_HEADER, handle, names.version);
    w.object_common(0);
    row_head(&mut w, name);
    let flags = block.map_or(0, |b| b.flags);
    let d = &mut w.data;
    d.bit(u8::from(flags & 1 != 0)); // anonymous
    d.bit(u8::from(block.is_some_and(|b| b.has_attributes)));
    d.bit(0); // an xref
    d.bit(0); // overlaid
    d.bit(0); // loaded
    if w.r2004() {
        w.data.bl(block.map_or(0, |b| b.members.len()) as i32);
    }
    let d = &mut w.data;
    let (base, z) = block.map_or((Point2::ORIGIN, 0.0), |b| (b.base, b.base_z));
    d.bd3(base, z);
    d.text(""); // the xref path
    d.rc(0); // no inserts listed
    d.text(""); // the description
    d.bl(0); // no preview
    w.object_handles(control, 0);
    let r2004 = w.r2004();
    let h = &mut w.handles;
    h.handle(5, 0);
    h.handle(3, block.map_or(0, |b| b.begin));
    if r2004 {
        for member in block.map_or(&[][..], |b| b.members.as_slice()) {
            h.handle(3, *member);
        }
    } else {
        h.handle(
            4,
            block.and_then(|b| b.members.first().copied()).unwrap_or(0),
        );
        h.handle(
            4,
            block.and_then(|b| b.members.last().copied()).unwrap_or(0),
        );
    }
    h.handle(3, block.map_or(0, |b| b.end));
    h.handle(5, f.handle(340).unwrap_or(0));
    w.finish()
}
