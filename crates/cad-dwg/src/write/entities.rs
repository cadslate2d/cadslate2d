//! Entities encoded by their prescriptions (ODA spec §20.4): each record's
//! DXF groups, as the exporter of `cad-dxf` writes them and the reader of
//! `entities.rs` decodes them, into the fields of the object. The reverse
//! of that decoder, kind for kind.

// A value is written in its short form only when it is exactly the
// default (§2.9, §20.4.9): the comparison is the bit pattern's, and a
// value a hair off the default is written in full and read back as it is.
#![allow(clippy::float_cmp)]

use cad_core::Point2;

use crate::objects::types;
use crate::write::fields::{Fields, lineweight_index};
use crate::write::objects::{EntityCommon, EntityLinks, ObjectBytes, ObjectWriter};
use crate::write::{Names, Placement, WriteError};

/// The object type of an entity record's kind: a fixed type, a class
/// number for a class entity, or none for a kind this crate does not
/// write.
fn type_of(kind: &str, fields: &Fields<'_>, names: &Names) -> Option<u16> {
    Some(match kind {
        "LINE" => types::LINE,
        "CIRCLE" => types::CIRCLE,
        "ARC" => types::ARC,
        "POINT" => types::POINT,
        "ELLIPSE" => types::ELLIPSE,
        "XLINE" => types::XLINE,
        "RAY" => types::RAY,
        "LWPOLYLINE" => types::LWPOLYLINE,
        "TEXT" => types::TEXT,
        "ATTRIB" => types::ATTRIB,
        "ATTDEF" => types::ATTDEF,
        "MTEXT" => types::MTEXT,
        "INSERT" => types::INSERT,
        "SEQEND" => types::SEQEND,
        "BLOCK" => types::BLOCK,
        "ENDBLK" => types::ENDBLK,
        "SPLINE" => types::SPLINE,
        "HATCH" => types::HATCH,
        "LEADER" => types::LEADER,
        "VIEWPORT" => types::VIEWPORT,
        "DIMENSION" => match fields.int(70).unwrap_or(0) & 15 {
            0 => types::DIMENSION_LINEAR,
            1 => types::DIMENSION_ALIGNED,
            2 => types::DIMENSION_ANGULAR_2LN,
            3 => types::DIMENSION_DIAMETER,
            4 => types::DIMENSION_RADIUS,
            5 => types::DIMENSION_ANGULAR_3PT,
            6 => types::DIMENSION_ORDINATE,
            _ => return None,
        },
        other => *names.classes.get(other)?,
    })
}

/// Encodes an entity record.
pub fn entity(
    kind: &str,
    groups: &[(i32, String)],
    place: &Placement,
    names: &Names,
) -> Result<ObjectBytes, WriteError> {
    let f = Fields::new(groups);
    let type_code =
        type_of(kind, &f, names).ok_or_else(|| WriteError::new(kind, "no DWG object type"))?;
    let handle = f
        .handle(5)
        .ok_or_else(|| WriteError::new(kind, "no handle"))?;
    let (ltype_flags, linetype) = match f.text(6).map(str::trim) {
        None => (0, 0),
        Some(name) if name.eq_ignore_ascii_case("BYLAYER") => (0, 0),
        Some(name) if name.eq_ignore_ascii_case("BYBLOCK") => (1, 0),
        Some(name) if name.eq_ignore_ascii_case("CONTINUOUS") => (2, 0),
        Some(name) => match names.linetypes.get(&name.to_uppercase()) {
            Some(handle) => (3, *handle),
            None => (0, 0),
        },
    };
    let common = EntityCommon {
        entmode: place.entmode,
        color: f.int(62).unwrap_or(256).clamp(0, 256) as i16,
        ltype_scale: f.real(48).unwrap_or(1.0),
        ltype_flags,
        invisible: f.int(60).unwrap_or(0) != 0,
        lineweight: lineweight_index(f.int(370).unwrap_or(-1)),
    };
    let layer = f
        .text(8)
        .and_then(|name| names.layers.get(&name.trim().to_uppercase()))
        .copied()
        .unwrap_or(names.layer_zero);
    let links = EntityLinks {
        owner: place.owner,
        prev: place.prev,
        next: place.next,
        layer,
        linetype,
    };
    let mut w = ObjectWriter::new(type_code, handle, names.version);
    w.entity_common(&common);
    w.entity_handles(&common, &links);
    match kind {
        "LINE" => line(&mut w, &f),
        "CIRCLE" => {
            let center = f.point(10).unwrap_or(Point2::ORIGIN);
            w.data.bd3(center, f.z(10));
            w.data.bd(f.real(40).unwrap_or(0.0));
            thickness_and_extrusion(&mut w, &f);
        }
        "ARC" => {
            let center = f.point(10).unwrap_or(Point2::ORIGIN);
            w.data.bd3(center, f.z(10));
            w.data.bd(f.real(40).unwrap_or(0.0));
            thickness_and_extrusion(&mut w, &f);
            w.data.bd(f.real(50).unwrap_or(0.0).to_radians());
            w.data.bd(f.real(51).unwrap_or(0.0).to_radians());
        }
        "POINT" => {
            let point = f.point(10).unwrap_or(Point2::ORIGIN);
            w.data.bd3(point, f.z(10));
            thickness_and_extrusion(&mut w, &f);
            w.data.bd(f.real(50).unwrap_or(0.0).to_radians());
        }
        "ELLIPSE" => {
            w.data.bd3(f.point(10).unwrap_or(Point2::ORIGIN), f.z(10));
            w.data.bd3(f.point(11).unwrap_or(Point2::ORIGIN), f.z(11));
            let (extrusion, ez) = extrusion_of(&f);
            w.data.bd3(extrusion, ez);
            w.data.bd(f.real(40).unwrap_or(1.0));
            w.data.bd(f.real(41).unwrap_or(0.0));
            w.data.bd(f.real(42).unwrap_or(std::f64::consts::TAU));
        }
        "XLINE" | "RAY" => {
            w.data.bd3(f.point(10).unwrap_or(Point2::ORIGIN), f.z(10));
            w.data.bd3(f.point(11).unwrap_or(Point2::ORIGIN), f.z(11));
        }
        "LWPOLYLINE" => lwpolyline(&mut w, &f),
        "TEXT" | "ATTRIB" | "ATTDEF" => {
            text(&mut w, &f);
            if kind != "TEXT" {
                w.text(f.text(2).unwrap_or(""));
                w.data.bs(0); // field length
                w.data.rc(f.int(70).unwrap_or(0) as u8);
                if w.r2007() {
                    w.data.bit(0); // the position is not locked
                }
                if kind == "ATTDEF" {
                    w.text(f.text(3).unwrap_or(""));
                }
            }
            w.handles.handle(5, text_style(&f, names));
        }
        "MTEXT" => {
            mtext(&mut w, &f);
            w.handles.handle(5, text_style(&f, names));
        }
        "INSERT" => insert(&mut w, &f, place, names),
        "SEQEND" | "ENDBLK" => {}
        "BLOCK" => w.text(f.text(2).unwrap_or("")),
        "SPLINE" => spline(&mut w, &f),
        "HATCH" => hatch(&mut w, &f)?,
        "WIPEOUT" => wipeout(&mut w, &f),
        "DIMENSION" => dimension(&mut w, &f, type_code, names)?,
        "LEADER" => leader(&mut w, &f, names),
        "VIEWPORT" => viewport(&mut w, &f),
        "MULTILEADER" => mleader(&mut w, &f, names)?,
        other => {
            return Err(WriteError::new(
                other,
                "an entity kind this crate does not write",
            ));
        }
    }
    Ok(w.finish())
}

fn text_style(f: &Fields<'_>, names: &Names) -> u64 {
    f.text(7)
        .and_then(|name| names.text_styles.get(&name.trim().to_uppercase()))
        .copied()
        .unwrap_or(names.standard_style)
}

/// The extrusion under 210, (0, 0, 1) when absent.
fn extrusion_of(f: &Fields<'_>) -> (Point2, f64) {
    match f.point(210) {
        Some(xy) => (xy, f.real(230).unwrap_or(1.0)),
        None => (Point2::ORIGIN, 1.0),
    }
}

fn thickness_and_extrusion(w: &mut ObjectWriter, f: &Fields<'_>) {
    w.data.bt(f.real(39).unwrap_or(0.0));
    let (xy, z) = extrusion_of(f);
    w.data.be([xy.x, xy.y, z]);
}

/// A LINE (§20.4.21): the ends as defaults of each other.
fn line(w: &mut ObjectWriter, f: &Fields<'_>) {
    let a = f.point(10).unwrap_or(Point2::ORIGIN);
    let b = f.point(11).unwrap_or(Point2::ORIGIN);
    let (z1, z2) = (f.z(10), f.z(11));
    let zs_zero = z1 == 0.0 && z2 == 0.0;
    w.data.bit(u8::from(zs_zero));
    w.data.rd(a.x);
    w.data.dd(a.x, b.x);
    w.data.rd(a.y);
    w.data.dd(a.y, b.y);
    if !zs_zero {
        w.data.rd(z1);
        w.data.dd(z1, z2);
    }
    thickness_and_extrusion(w, f);
}

/// A TEXT's own fields (§20.4.3): the data flags say what is left out.
fn text(w: &mut ObjectWriter, f: &Fields<'_>) {
    let elevation = f.z(10);
    let insertion = f.point(10).unwrap_or(Point2::ORIGIN);
    let alignment = f.point(11);
    let oblique = f.real(51).unwrap_or(0.0).to_radians();
    let rotation = f.real(50).unwrap_or(0.0).to_radians();
    let height = f.real(40).unwrap_or(1.0);
    let width = f.real(41).unwrap_or(1.0);
    let generation = f.int(71).unwrap_or(0) as i16;
    let halign = f.int(72).unwrap_or(0) as i16;
    let valign = f.int(73).unwrap_or(0) as i16;
    let mut flags = 0u8;
    if elevation == 0.0 {
        flags |= 1;
    }
    if alignment.is_none() {
        flags |= 2;
    }
    if oblique == 0.0 {
        flags |= 4;
    }
    if rotation == 0.0 {
        flags |= 8;
    }
    if width == 1.0 {
        flags |= 0x10;
    }
    if generation == 0 {
        flags |= 0x20;
    }
    if halign == 0 {
        flags |= 0x40;
    }
    if valign == 0 {
        flags |= 0x80;
    }
    let d = &mut w.data;
    d.rc(flags);
    if flags & 1 == 0 {
        d.rd(elevation);
    }
    d.rd2(insertion);
    if let Some(alignment) = alignment {
        d.dd(insertion.x, alignment.x);
        d.dd(insertion.y, alignment.y);
    }
    let (xy, z) = extrusion_of(f);
    d.be([xy.x, xy.y, z]);
    d.bt(f.real(39).unwrap_or(0.0));
    if flags & 4 == 0 {
        d.rd(oblique);
    }
    if flags & 8 == 0 {
        d.rd(rotation);
    }
    d.rd(height);
    if flags & 0x10 == 0 {
        d.rd(width);
    }
    w.text(f.text(1).unwrap_or(""));
    let d = &mut w.data;
    if flags & 0x20 == 0 {
        d.bs(generation);
    }
    if flags & 0x40 == 0 {
        d.bs(halign);
    }
    if flags & 0x80 == 0 {
        d.bs(valign);
    }
}

/// An MTEXT's own fields (§20.4.46): the words joined from their pieces.
fn mtext(w: &mut ObjectWriter, f: &Fields<'_>) {
    let d = &mut w.data;
    d.bd3(f.point(10).unwrap_or(Point2::ORIGIN), f.z(10));
    let (xy, z) = extrusion_of(f);
    d.bd3(xy, z);
    let direction = match f.point(11) {
        Some(direction) => (direction, f.z(11)),
        None => {
            let rotation = f.real(50).unwrap_or(0.0).to_radians();
            (Point2::new(rotation.cos(), rotation.sin()), 0.0)
        }
    };
    d.bd3(direction.0, direction.1);
    d.bd(f.real(41).unwrap_or(0.0)); // the rectangle's width
    if w.r2007() {
        w.data.bd(0.0); // the rectangle's height
    }
    let d = &mut w.data;
    d.bd(f.real(40).unwrap_or(1.0)); // the height
    d.bs(f.int(71).unwrap_or(1) as i16);
    d.bs(f.int(72).unwrap_or(1) as i16);
    d.bd(0.0); // the extents' height, laid out by the reader
    d.bd(0.0); // the extents' width
    let mut words = String::new();
    for piece in f.texts(3) {
        words.push_str(piece);
    }
    words.push_str(f.text(1).unwrap_or(""));
    w.text(&words);
    let d = &mut w.data;
    d.bs(f.int(73).unwrap_or(1) as i16);
    d.bd(f.real(44).unwrap_or(1.0));
    d.bit(0);
    if w.r2004() {
        w.data.bl(0); // no background fill
    }
}

/// An INSERT (§20.4.9): the scale in its shortest form, the attributes
/// it owns by their first and last and the SEQEND.
fn insert(w: &mut ObjectWriter, f: &Fields<'_>, place: &Placement, names: &Names) {
    let d = &mut w.data;
    d.bd3(f.point(10).unwrap_or(Point2::ORIGIN), f.z(10));
    let sx = f.real(41).unwrap_or(1.0);
    let sy = f.real(42).unwrap_or(1.0);
    let sz = f.real(43).unwrap_or(1.0);
    if (sx, sy, sz) == (1.0, 1.0, 1.0) {
        d.bb(3);
    } else if sx == sy && sy == sz {
        d.bb(2);
        d.rd(sx);
    } else if sx == 1.0 {
        d.bb(1);
        d.dd(1.0, sy);
        d.dd(1.0, sz);
    } else {
        d.bb(0);
        d.rd(sx);
        d.dd(sx, sy);
        d.dd(sx, sz);
    }
    d.bd(f.real(50).unwrap_or(0.0).to_radians());
    let (xy, z) = extrusion_of(f);
    d.bd3(xy, z);
    let has_attributes = place.owned.is_some();
    d.bit(u8::from(has_attributes));
    if let (true, Some(owned)) = (w.r2004(), &place.owned) {
        w.data.bl(owned.handles.len() as i32);
    }
    let block = f
        .text(2)
        .and_then(|name| names.block_records.get(&name.trim().to_uppercase()))
        .copied()
        .unwrap_or(0);
    w.handles.handle(5, block);
    if let Some(owned) = &place.owned {
        if w.r2004() {
            for attribute in &owned.handles {
                w.handles.handle(3, *attribute);
            }
        } else {
            w.handles
                .handle(4, owned.handles.first().copied().unwrap_or(0));
            w.handles
                .handle(4, owned.handles.last().copied().unwrap_or(0));
        }
        w.handles.handle(3, owned.seqend);
    }
}

/// An LWPOLYLINE (§20.4.85): the flags say what follows; the points as
/// defaults of each other.
fn lwpolyline(w: &mut ObjectWriter, f: &Fields<'_>) {
    let dxf_flags = f.int(70).unwrap_or(0);
    let constant_width = f.real(43);
    let elevation = f.real(38);
    let thickness = f.real(39);
    let normal = f
        .point(210)
        .map(|xy| [xy.x, xy.y, f.real(230).unwrap_or(1.0)]);
    // Each vertex: its point, its widths, its bulge, in the order the
    // groups come.
    let mut points: Vec<Point2> = Vec::new();
    let mut bulges: Vec<f64> = Vec::new();
    let mut widths: Vec<(f64, f64)> = Vec::new();
    for (code, value) in f.groups() {
        let Ok(v) = value.trim().parse::<f64>() else {
            continue;
        };
        match code {
            10 => {
                points.push(Point2::new(v, 0.0));
                bulges.push(0.0);
                widths.push((0.0, 0.0));
            }
            20 => {
                if let Some(p) = points.last_mut() {
                    p.y = v;
                }
            }
            42 => {
                if let Some(b) = bulges.last_mut() {
                    *b = v;
                }
            }
            40 => {
                if let Some(wd) = widths.last_mut() {
                    wd.0 = v;
                }
            }
            41 => {
                if let Some(wd) = widths.last_mut() {
                    wd.1 = v;
                }
            }
            _ => {}
        }
    }
    let has_bulges = bulges.iter().any(|b| *b != 0.0);
    let has_widths = widths.iter().any(|wd| *wd != (0.0, 0.0));
    let mut flag: u16 = 0;
    if normal.is_some() {
        flag |= 1;
    }
    if thickness.is_some() {
        flag |= 2;
    }
    if constant_width.is_some() {
        flag |= 4;
    }
    if elevation.is_some() {
        flag |= 8;
    }
    if has_bulges {
        flag |= 16;
    }
    if has_widths {
        flag |= 32;
    }
    if dxf_flags & 128 != 0 {
        flag |= 256;
    }
    if dxf_flags & 1 != 0 {
        flag |= 512;
    }
    let d = &mut w.data;
    d.bs(flag as i16);
    if let Some(width) = constant_width {
        d.bd(width);
    }
    if let Some(elevation) = elevation {
        d.bd(elevation);
    }
    if let Some(thickness) = thickness {
        d.bd(thickness);
    }
    if let Some(normal) = normal {
        d.bd3(Point2::new(normal[0], normal[1]), normal[2]);
    }
    d.bl(points.len() as i32);
    if has_bulges {
        d.bl(bulges.len() as i32);
    }
    if has_widths {
        d.bl(widths.len() as i32);
    }
    let mut previous: Option<Point2> = None;
    for point in &points {
        match previous {
            None => {
                d.rd(point.x);
                d.rd(point.y);
            }
            Some(p) => {
                d.dd(p.x, point.x);
                d.dd(p.y, point.y);
            }
        }
        previous = Some(*point);
    }
    if has_bulges {
        for bulge in &bulges {
            d.bd(*bulge);
        }
    }
    if has_widths {
        for (start, end) in &widths {
            d.bd(*start);
            d.bd(*end);
        }
    }
}

/// A SPLINE (§20.4.40): by its fit points when they are what defines it
/// (scenario 2) — none, or control data that is exactly what a spline
/// through them has — else by its control points (scenario 1), and the
/// fit points, if any, go (`export_losses`).
fn spline(w: &mut ObjectWriter, f: &Fields<'_>) {
    let flags = f.int(70).unwrap_or(8);
    let degree = f.int(71).unwrap_or(3) as i32;
    let knots = f.reals(40);
    let weights = f.reals(41);
    let controls = f.points(10);
    let fits = f.points(11);
    let d = &mut w.data;
    if controls.is_empty() || fit_points_define(&fits, degree, &controls, &knots, &weights) {
        d.bl(2);
        d.bl(degree);
        d.bd(f.real(44).unwrap_or(1e-10));
        let begin = f.point(12).unwrap_or(Point2::ORIGIN);
        let end = f.point(13).unwrap_or(Point2::ORIGIN);
        d.bd3(begin, f.z(12));
        d.bd3(end, f.z(13));
        d.bl(fits.len() as i32);
        for (point, z) in &fits {
            d.bd3(*point, *z);
        }
        return;
    }
    let weighted = weights.len() == controls.len() && !weights.is_empty();
    d.bl(1);
    d.bl(degree);
    // Rational when it has weights, whatever the flags say: a reader
    // takes the weights only of a rational spline.
    d.bit(u8::from(flags & 4 != 0 || weighted));
    d.bit(u8::from(flags & 1 != 0)); // closed
    d.bit(u8::from(flags & 2 != 0)); // periodic
    d.bd(f.real(42).unwrap_or(1e-7));
    d.bd(f.real(43).unwrap_or(1e-7));
    d.bl(knots.len() as i32);
    d.bl(controls.len() as i32);
    d.bit(u8::from(weighted));
    for knot in &knots {
        d.bd(*knot);
    }
    for (i, (point, z)) in controls.iter().enumerate() {
        d.bd3(*point, *z);
        if weighted {
            d.bd(weights[i]);
        }
    }
}

/// Whether `fits` define the spline whose control data this is: the
/// spline through them has the same degree, control points, and knots,
/// and no weights. Then the fit points alone carry it, and a reader that
/// interpolates them gets this control data back.
pub(crate) fn fit_points_define(
    fits: &[(Point2, f64)],
    degree: i32,
    controls: &[(Point2, f64)],
    knots: &[f64],
    weights: &[f64],
) -> bool {
    if fits.is_empty() || !weights.is_empty() || fits.iter().any(|(_, z)| *z != 0.0) {
        return false;
    }
    let points: Vec<Point2> = fits.iter().map(|(point, _)| *point).collect();
    let Some(through) = cad_document::SplineEntity::through(&points) else {
        return false;
    };
    through.degree as i32 == degree
        && through.knots == knots
        && through.control.len() == controls.len()
        && through
            .control
            .iter()
            .zip(controls)
            .all(|(a, (b, z))| a == b && *z == 0.0)
}

/// A HATCH (§20.4.75), from its groups in the order a file carries them.
fn hatch(w: &mut ObjectWriter, f: &Fields<'_>) -> Result<(), WriteError> {
    let groups = f.groups();
    if w.r2004() {
        // No gradient (§20.4.75, R2004): the flags, angle, shift, single
        // colour, tint, colours, and name all zero or empty.
        let d = &mut w.data;
        d.bl(f.int(450).unwrap_or(0) as i32);
        d.bl(0);
        d.bd(0.0);
        d.bd(0.0);
        d.bl(0);
        d.bd(0.0);
        d.bl(0);
        w.text("");
    }
    let d = &mut w.data;
    d.bd(f.z(10));
    let (xy, z) = extrusion_of(f);
    d.bd3(xy, z);
    w.text(f.text(2).unwrap_or("SOLID"));
    let d = &mut w.data;
    let solid = f.int(70).unwrap_or(1) != 0;
    d.bit(u8::from(solid));
    d.bit(u8::from(f.int(71).unwrap_or(0) != 0));
    // The paths start at the first 92; a path's groups run to the next 92
    // or to 75, which follows the last path.
    let starts: Vec<usize> = groups
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| *c == 92)
        .map(|(i, _)| i)
        .collect();
    let end = groups
        .iter()
        .position(|(c, _)| *c == 75)
        .unwrap_or(groups.len());
    d.bl(starts.len() as i32);
    let mut any_pixel = false;
    for (n, &start) in starts.iter().enumerate() {
        let stop = starts.get(n + 1).copied().unwrap_or(end);
        let path = Fields::new(&groups[start..stop]);
        let flags = path.int(92).unwrap_or(0) as i32;
        any_pixel |= flags & 4 != 0;
        d.bl(flags);
        if flags & 2 == 0 {
            edge_path(d, &path)?;
        } else {
            let bulges = path.int(72).unwrap_or(0) != 0;
            d.bit(u8::from(bulges));
            d.bit(u8::from(path.int(73).unwrap_or(0) != 0));
            let vertices = path.points(10);
            d.bl(vertices.len() as i32);
            let all_bulges = path.reals(42);
            for (i, (point, _)) in vertices.iter().enumerate() {
                d.rd2(*point);
                if bulges {
                    d.bd(all_bulges.get(i).copied().unwrap_or(0.0));
                }
            }
        }
        d.bl(0); // boundary sources
    }
    let tail = Fields::new(&groups[end..]);
    d.bs(tail.int(75).unwrap_or(0) as i16);
    d.bs(tail.int(76).unwrap_or(1) as i16);
    if !solid {
        d.bd(tail.real(52).unwrap_or(0.0).to_radians());
        d.bd(tail.real(41).unwrap_or(1.0));
        d.bit(u8::from(tail.int(77).unwrap_or(0) != 0));
        // Each line starts at 53; its dashes (49) follow its 79.
        let lines: Vec<usize> = tail
            .groups()
            .iter()
            .enumerate()
            .filter(|(_, (c, _))| *c == 53)
            .map(|(i, _)| i)
            .collect();
        d.bs(lines.len() as i16);
        for (n, &start) in lines.iter().enumerate() {
            let stop = lines.get(n + 1).copied().unwrap_or(tail.len());
            let line = tail.slice(start, stop);
            d.bd(line.real(53).unwrap_or(0.0).to_radians());
            d.bd2(Point2::new(
                line.real(43).unwrap_or(0.0),
                line.real(44).unwrap_or(0.0),
            ));
            d.bd2(Point2::new(
                line.real(45).unwrap_or(0.0),
                line.real(46).unwrap_or(0.0),
            ));
            let dashes = line.reals(49);
            d.bs(dashes.len() as i16);
            for dash in dashes {
                d.bd(dash);
            }
        }
    }
    if any_pixel {
        d.bd(tail.real(47).unwrap_or(0.0));
    }
    // The seed points: the 10/20 pairs after 98.
    let seeds = match tail.groups().iter().position(|(c, _)| *c == 98) {
        Some(at) => Fields::new(&tail.groups()[at..]).points(10),
        None => Vec::new(),
    };
    d.bl(seeds.len() as i32);
    for (point, _) in seeds {
        d.rd2(point);
    }
    Ok(())
}

/// The edges of a hatch path that is not a polyline: each starts at its
/// 72 and runs to the next.
fn edge_path(d: &mut crate::write::bits::BitWriter, path: &Fields<'_>) -> Result<(), WriteError> {
    let groups = path.groups();
    let starts: Vec<usize> = groups
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| *c == 72)
        .map(|(i, _)| i)
        .collect();
    let end = groups
        .iter()
        .position(|(c, _)| *c == 97)
        .unwrap_or(groups.len());
    d.bl(starts.len() as i32);
    for (n, &start) in starts.iter().enumerate() {
        let stop = starts.get(n + 1).copied().unwrap_or(end);
        let edge = Fields::new(&groups[start..stop]);
        let kind = edge.int(72).unwrap_or(1);
        d.rc(kind as u8);
        match kind {
            1 => {
                d.rd2(edge.point(10).unwrap_or(Point2::ORIGIN));
                d.rd2(edge.point(11).unwrap_or(Point2::ORIGIN));
            }
            2 => {
                d.rd2(edge.point(10).unwrap_or(Point2::ORIGIN));
                d.bd(edge.real(40).unwrap_or(0.0));
                d.bd(edge.real(50).unwrap_or(0.0).to_radians());
                d.bd(edge.real(51).unwrap_or(0.0).to_radians());
                d.bit(u8::from(edge.int(73).unwrap_or(1) != 0));
            }
            3 => {
                d.rd2(edge.point(10).unwrap_or(Point2::ORIGIN));
                d.rd2(edge.point(11).unwrap_or(Point2::ORIGIN));
                d.bd(edge.real(40).unwrap_or(1.0));
                d.bd(edge.real(50).unwrap_or(0.0).to_radians());
                d.bd(edge.real(51).unwrap_or(0.0).to_radians());
                d.bit(u8::from(edge.int(73).unwrap_or(1) != 0));
            }
            4 => {
                let rational = edge.int(73).unwrap_or(0) != 0;
                d.bl(edge.int(94).unwrap_or(3) as i32);
                d.bit(u8::from(rational));
                d.bit(u8::from(edge.int(74).unwrap_or(0) != 0));
                let knots = edge.reals(40);
                let controls = edge.points(10);
                let weights = edge.reals(42);
                d.bl(knots.len() as i32);
                d.bl(controls.len() as i32);
                for knot in knots {
                    d.bd(knot);
                }
                for (i, (point, _)) in controls.iter().enumerate() {
                    d.rd2(*point);
                    if rational {
                        d.bd(weights.get(i).copied().unwrap_or(1.0));
                    }
                }
            }
            _ => return Err(WriteError::new("HATCH", "an edge kind no file carries")),
        }
    }
    Ok(())
}

/// A WIPEOUT (§20.4.80): an image with no picture.
fn wipeout(w: &mut ObjectWriter, f: &Fields<'_>) {
    let d = &mut w.data;
    d.bl(f.int(90).unwrap_or(0) as i32);
    d.bd3(f.point(10).unwrap_or(Point2::ORIGIN), f.z(10));
    d.bd3(f.point(11).unwrap_or(Point2::new(1.0, 0.0)), f.z(11));
    d.bd3(f.point(12).unwrap_or(Point2::new(0.0, 1.0)), f.z(12));
    d.rd2(f.point(13).unwrap_or(Point2::new(1.0, 1.0)));
    d.bs(f.int(70).unwrap_or(7) as i16);
    d.bit(u8::from(f.int(280).unwrap_or(1) != 0));
    d.rc(f.int(281).unwrap_or(50) as u8);
    d.rc(f.int(282).unwrap_or(50) as u8);
    d.rc(f.int(283).unwrap_or(0) as u8);
    let boundary = f.int(71).unwrap_or(2) as i16;
    d.bs(boundary);
    let vertices = f.points(14);
    if boundary == 1 {
        for i in 0..2 {
            d.rd2(vertices.get(i).map_or(Point2::ORIGIN, |(p, _)| *p));
        }
    } else {
        d.bl(vertices.len() as i32);
        for (point, _) in vertices {
            d.rd2(point);
        }
    }
    w.handles.handle(5, 0); // the image definition
    w.handles.handle(3, 0); // its reactor
}

/// A DIMENSION (§20.4.22–20.4.29): the common data, then the kind's
/// points; the style and the anonymous block by handle.
fn dimension(
    w: &mut ObjectWriter,
    f: &Fields<'_>,
    type_code: u16,
    names: &Names,
) -> Result<(), WriteError> {
    let flags70 = f.int(70).unwrap_or(0);
    let p = |code: i32| (f.point(code).unwrap_or(Point2::ORIGIN), f.z(code));
    let d = &mut w.data;
    let (xy, z) = extrusion_of(f);
    d.bd3(xy, z);
    let (midpoint, elevation) = p(11);
    d.rd2(midpoint);
    d.bd(elevation);
    // Bit 0 set when the text was placed by the style, not by hand (128).
    d.rc(u8::from(flags70 & 128 == 0));
    w.text(f.text(1).unwrap_or(""));
    let d = &mut w.data;
    d.bd(f.real(53).unwrap_or(0.0).to_radians());
    d.bd(f.real(51).unwrap_or(0.0).to_radians());
    d.bd3(Point2::new(1.0, 1.0), 1.0); // the block's scale
    d.bd(0.0); // the block's rotation
    d.bs(f.int(71).unwrap_or(5) as i16);
    d.bs(f.int(72).unwrap_or(1) as i16);
    d.bd(f.real(41).unwrap_or(1.0));
    d.bd(f.real(42).unwrap_or(0.0));
    if w.r2007() {
        let d = &mut w.data;
        d.bit(0);
        d.bit(0); // the first arrowhead is not flipped
        d.bit(0); // nor the second
    }
    let d = &mut w.data;
    d.rd2(f.point(12).unwrap_or(Point2::ORIGIN));
    match type_code {
        types::DIMENSION_ORDINATE => {
            for code in [10, 13, 14] {
                let (xy, z) = p(code);
                d.bd3(xy, z);
            }
            d.rc(u8::from(flags70 & 64 != 0));
        }
        types::DIMENSION_LINEAR | types::DIMENSION_ALIGNED => {
            for code in [13, 14, 10] {
                let (xy, z) = p(code);
                d.bd3(xy, z);
            }
            d.bd(f.real(52).unwrap_or(0.0).to_radians());
            if type_code == types::DIMENSION_LINEAR {
                d.bd(f.real(50).unwrap_or(0.0).to_radians());
            }
        }
        types::DIMENSION_ANGULAR_3PT => {
            for code in [10, 13, 14, 15] {
                let (xy, z) = p(code);
                d.bd3(xy, z);
            }
        }
        types::DIMENSION_ANGULAR_2LN => {
            d.rd2(f.point(16).unwrap_or(Point2::ORIGIN));
            for code in [13, 14, 15, 10] {
                let (xy, z) = p(code);
                d.bd3(xy, z);
            }
        }
        types::DIMENSION_RADIUS => {
            for code in [10, 15] {
                let (xy, z) = p(code);
                d.bd3(xy, z);
            }
            d.bd(f.real(40).unwrap_or(0.0));
        }
        types::DIMENSION_DIAMETER => {
            for code in [15, 10] {
                let (xy, z) = p(code);
                d.bd3(xy, z);
            }
            d.bd(f.real(40).unwrap_or(0.0));
        }
        _ => return Err(WriteError::new("DIMENSION", "a kind no file carries")),
    }
    let style = f
        .text(3)
        .and_then(|name| names.dim_styles.get(&name.trim().to_uppercase()))
        .copied()
        .unwrap_or(names.standard_dim_style);
    let block = f
        .text(2)
        .and_then(|name| names.block_records.get(&name.trim().to_uppercase()))
        .copied()
        .unwrap_or(0);
    w.handles.handle(5, style);
    w.handles.handle(5, block);
    Ok(())
}

/// A LEADER (§20.4.47).
fn leader(w: &mut ObjectWriter, f: &Fields<'_>, names: &Names) {
    let d = &mut w.data;
    d.bit(0);
    d.bs(f.int(73).unwrap_or(3) as i16);
    d.bs(f.int(72).unwrap_or(0) as i16);
    let points = f.points(10);
    d.bl(points.len() as i32);
    for (point, z) in &points {
        d.bd3(*point, *z);
    }
    d.bd3(Point2::ORIGIN, 0.0); // the origin
    let (xy, z) = extrusion_of(f);
    d.bd3(xy, z);
    let dir = |code: i32, fallback: Point2| (f.point(code).unwrap_or(fallback), f.z(code));
    let (x, xz) = dir(211, Point2::new(1.0, 0.0));
    d.bd3(x, xz);
    let (offset, oz) = dir(212, Point2::ORIGIN);
    d.bd3(offset, oz);
    let (projection, pz) = dir(213, Point2::ORIGIN);
    d.bd3(projection, pz);
    d.bd(f.real(40).unwrap_or(0.0));
    d.bd(f.real(41).unwrap_or(0.0));
    d.bit(u8::from(f.int(74).unwrap_or(0) != 0));
    d.bit(u8::from(f.int(71).unwrap_or(1) != 0));
    d.bs(0);
    d.bit(0);
    d.bit(0);
    let style = f
        .text(3)
        .and_then(|name| names.dim_styles.get(&name.trim().to_uppercase()))
        .copied()
        .unwrap_or(names.standard_dim_style);
    w.handles.handle(5, f.handle(340).unwrap_or(0));
    w.handles.handle(5, style);
}

/// A VIEWPORT entity (§20.4.38) of R2000.
fn viewport(w: &mut ObjectWriter, f: &Fields<'_>) {
    let d = &mut w.data;
    d.bd3(f.point(10).unwrap_or(Point2::ORIGIN), f.z(10));
    d.bd(f.real(40).unwrap_or(1.0));
    d.bd(f.real(41).unwrap_or(1.0));
    d.bd3(f.point(17).unwrap_or(Point2::ORIGIN), f.z(17));
    d.bd3(
        f.point(16).unwrap_or(Point2::ORIGIN),
        f.real(36).unwrap_or(1.0),
    );
    d.bd(f.real(51).unwrap_or(0.0).to_radians());
    d.bd(f.real(45).unwrap_or(1.0));
    d.bd(f.real(42).unwrap_or(50.0));
    d.bd(f.real(43).unwrap_or(0.0));
    d.bd(f.real(44).unwrap_or(0.0));
    d.bd(f.real(50).unwrap_or(0.0).to_radians());
    d.rd2(f.point(12).unwrap_or(Point2::ORIGIN));
    d.rd2(f.point(13).unwrap_or(Point2::ORIGIN));
    d.rd2(f.point(14).unwrap_or(Point2::new(0.5, 0.5)));
    d.rd2(f.point(15).unwrap_or(Point2::new(0.5, 0.5)));
    d.bs(f.int(72).unwrap_or(1000) as i16);
    if w.r2007() {
        w.data.bs(f.int(61).unwrap_or(5) as i16); // grid major
    }
    let d = &mut w.data;
    d.bl(0); // frozen layers
    let mut status = f.int(90).unwrap_or(0) as i32;
    if f.int(68).unwrap_or(1) == 0 {
        status |= 0x20000;
    }
    d.bl(status);
    w.text(f.text(1).unwrap_or(""));
    let d = &mut w.data;
    d.rc(f.int(281).unwrap_or(0) as u8);
    d.bit(u8::from(f.int(74).unwrap_or(0) != 0));
    d.bit(u8::from(f.int(71).unwrap_or(1) != 0));
    d.bd3(f.point(110).unwrap_or(Point2::ORIGIN), f.z(110));
    d.bd3(f.point(111).unwrap_or(Point2::new(1.0, 0.0)), f.z(111));
    d.bd3(f.point(112).unwrap_or(Point2::new(0.0, 1.0)), f.z(112));
    d.bd(f.real(146).unwrap_or(0.0));
    d.bs(f.int(79).unwrap_or(0) as i16);
    let r2004 = w.r2004();
    let r2007 = w.r2007();
    if r2004 {
        w.data.bs(f.int(170).unwrap_or(0) as i16); // shade plot
    }
    if r2007 {
        let d = &mut w.data;
        d.bit(1); // the default lights
        d.rc(1); // one distant light
        d.bd(0.0); // brightness
        d.bd(0.0); // contrast
        w.cmc(250); // the ambient colour
    }
    let h = &mut w.handles;
    h.handle(5, 0); // the clip boundary
    if !r2004 {
        h.handle(5, 0); // the viewport entity header
    }
    h.handle(5, 0); // the named UCS
    h.handle(5, 0); // the base UCS
    if r2007 {
        h.handle(4, 0); // the background
        h.handle(5, 0); // the visual style
        h.handle(4, 0); // the shade plot
        h.handle(3, 0); // the sun
    }
}

/// A MULTILEADER (§20.4.48, §20.4.86, LibreDWG's `dwg2.spec`): the
/// context data — its roots, then its fields, its text or its block —
/// then the entity's own fields; the handles in the order they are met.
fn mleader(w: &mut ObjectWriter, f: &Fields<'_>, names: &Names) -> Result<(), WriteError> {
    let groups = f.groups();
    let open = f
        .position(300, "CONTEXT_DATA{")
        .ok_or_else(|| WriteError::new("MULTILEADER", "no context data"))?;
    let close = groups[open..]
        .iter()
        .position(|(c, v)| *c == 301 && v.trim() == "}")
        .map(|at| open + at)
        .ok_or_else(|| WriteError::new("MULTILEADER", "context data without an end"))?;
    let context = &groups[open + 1..close];
    let after = Fields::new(&groups[close + 1..]);
    // The roots: each from `302 LEADER{` to its `303 }`; the rest of the
    // context is the fields around them.
    let mut roots: Vec<&[(i32, String)]> = Vec::new();
    let mut rest: Vec<(i32, String)> = Vec::new();
    let mut i = 0;
    while i < context.len() {
        let (code, value) = &context[i];
        if *code == 302 && value.trim() == "LEADER{" {
            let end = context[i..]
                .iter()
                .position(|(c, v)| *c == 303 && v.trim() == "}")
                .map(|at| i + at)
                .ok_or_else(|| WriteError::new("MULTILEADER", "a leader without an end"))?;
            roots.push(&context[i + 1..end]);
            i = end + 1;
        } else {
            rest.push((*code, value.clone()));
            i += 1;
        }
    }
    let c = Fields::new(&rest);
    let d = &mut w.data;
    d.bl(roots.len() as i32);
    for root in roots {
        // The lines: each from `304 LEADER_LINE{` to its `305 }`.
        let mut lines: Vec<&[(i32, String)]> = Vec::new();
        let mut own: Vec<(i32, String)> = Vec::new();
        let mut i = 0;
        while i < root.len() {
            let (code, value) = &root[i];
            if *code == 304 && value.trim() == "LEADER_LINE{" {
                let end = root[i..]
                    .iter()
                    .position(|(c, v)| *c == 305 && v.trim() == "}")
                    .map(|at| i + at)
                    .ok_or_else(|| WriteError::new("MULTILEADER", "a line without an end"))?;
                lines.push(&root[i + 1..end]);
                i = end + 1;
            } else {
                own.push((*code, value.clone()));
                i += 1;
            }
        }
        let r = Fields::new(&own);
        let has_last = r.int(290).unwrap_or(1) != 0 && r.point(10).is_some();
        let has_dogleg = r.int(291).unwrap_or(1) != 0 && r.point(11).is_some();
        d.bit(u8::from(has_last));
        d.bit(u8::from(has_dogleg));
        if has_last {
            d.bd3(r.point(10).unwrap_or(Point2::ORIGIN), r.z(10));
        }
        if has_dogleg {
            d.bd3(r.point(11).unwrap_or(Point2::ORIGIN), r.z(11));
        }
        let breaks = r.points(12);
        let ends = r.points(13);
        d.bl(breaks.len() as i32);
        for (i, (a, az)) in breaks.iter().enumerate() {
            d.bd3(*a, *az);
            let (b, bz) = ends.get(i).copied().unwrap_or((Point2::ORIGIN, 0.0));
            d.bd3(b, bz);
        }
        d.bl(r.int(90).unwrap_or(0) as i32);
        d.bd(r.real(40).unwrap_or(0.0));
        d.bl(lines.len() as i32);
        for line in lines {
            let l = Fields::new(line);
            let points = l.points(10);
            d.bl(points.len() as i32);
            for (p, z) in points {
                d.bd3(p, z);
            }
            let breaks = l.points(11);
            let ends = l.points(12);
            d.bl(breaks.len() as i32);
            for (i, (a, az)) in breaks.iter().enumerate() {
                d.bd3(*a, *az);
                let (b, bz) = ends.get(i).copied().unwrap_or((Point2::ORIGIN, 0.0));
                d.bd3(b, bz);
            }
            d.bl(l.int(91).unwrap_or(0) as i32);
        }
    }
    d.bd(c.real(40).unwrap_or(1.0));
    d.bd3(c.point(10).unwrap_or(Point2::ORIGIN), c.z(10));
    d.bd(c.real(41).unwrap_or(1.0));
    d.bd(c.real(140).unwrap_or(1.0));
    d.bd(c.real(145).unwrap_or(1.0));
    d.bs(c.int(174).unwrap_or(1) as i16);
    d.bs(c.int(175).unwrap_or(1) as i16);
    d.bs(c.int(176).unwrap_or(0) as i16);
    d.bs(c.int(177).unwrap_or(0) as i16);
    let has_text = c.int(290).unwrap_or(0) != 0;
    d.bit(u8::from(has_text));
    if has_text {
        w.text(c.text(304).unwrap_or(""));
        let d = &mut w.data;
        d.bd3(
            c.point(11).unwrap_or(Point2::ORIGIN),
            c.real(31).unwrap_or(1.0),
        );
        w.handles
            .handle(5, c.handle(340).unwrap_or(names.standard_style));
        let d = &mut w.data;
        d.bd3(c.point(12).unwrap_or(Point2::ORIGIN), c.z(12));
        d.bd3(c.point(13).unwrap_or(Point2::new(1.0, 0.0)), c.z(13));
        d.bd(c.real(42).unwrap_or(0.0).to_radians());
        d.bd(c.real(43).unwrap_or(0.0));
        d.bd(c.real(44).unwrap_or(0.0));
        d.bd(c.real(45).unwrap_or(1.0));
        d.bs(c.int(170).unwrap_or(1) as i16);
        w.cmc(color_index(c.int(90)));
        let d = &mut w.data;
        d.bs(c.int(171).unwrap_or(1) as i16);
        d.bs(c.int(172).unwrap_or(1) as i16);
        w.cmc(color_index(c.int(91)));
        let d = &mut w.data;
        d.bd(c.real(141).unwrap_or(0.0));
        d.bl(c.int(92).unwrap_or(0) as i32);
        d.bit(u8::from(c.int(291).unwrap_or(0) != 0));
        d.bit(u8::from(c.int(292).unwrap_or(0) != 0));
        d.bs(c.int(173).unwrap_or(0) as i16);
        d.bit(u8::from(c.int(293).unwrap_or(0) != 0));
        d.bd(c.real(142).unwrap_or(0.0));
        d.bd(c.real(143).unwrap_or(0.0));
        d.bit(u8::from(c.int(294).unwrap_or(0) != 0));
        let sizes = c.reals(144);
        d.bl(sizes.len() as i32);
        for size in sizes {
            d.bd(size);
        }
        d.bit(u8::from(c.int(295).unwrap_or(0) != 0));
        d.bit(0);
    } else {
        let has_block = c.int(296).unwrap_or(0) != 0;
        d.bit(u8::from(has_block));
        if has_block {
            w.handles.handle(5, c.handle(341).unwrap_or(0));
            let d = &mut w.data;
            d.bd3(
                c.point(14).unwrap_or(Point2::ORIGIN),
                c.real(34).unwrap_or(1.0),
            );
            d.bd3(c.point(15).unwrap_or(Point2::ORIGIN), c.z(15));
            d.bd3(
                c.point(16).unwrap_or(Point2::new(1.0, 1.0)),
                c.real(36).unwrap_or(1.0),
            );
            d.bd(c.real(46).unwrap_or(0.0).to_radians());
            w.cmc(color_index(c.int(93)));
            let d = &mut w.data;
            let matrix = c.reals(47);
            for i in 0..16 {
                let identity = if i % 5 == 0 { 1.0 } else { 0.0 };
                d.bd(matrix.get(i).copied().unwrap_or(identity));
            }
        }
    }
    let d = &mut w.data;
    d.bd3(c.point(110).unwrap_or(Point2::ORIGIN), c.z(110));
    d.bd3(c.point(111).unwrap_or(Point2::new(1.0, 0.0)), c.z(111));
    d.bd3(c.point(112).unwrap_or(Point2::new(0.0, 1.0)), c.z(112));
    d.bit(u8::from(c.int(297).unwrap_or(0) != 0));
    // The entity's own fields after the context.
    let a = after;
    w.handles.handle(5, a.handle(340).unwrap_or(0));
    let d = &mut w.data;
    d.bl(a.int(90).unwrap_or(0) as i32);
    d.bs(a.int(170).unwrap_or(1) as i16);
    w.cmc(color_index(a.int(91)));
    w.handles.handle(5, a.handle(341).unwrap_or(0));
    let d = &mut w.data;
    d.bl(a.int(171).unwrap_or(-2) as i32);
    d.bit(u8::from(a.int(290).unwrap_or(1) != 0));
    d.bit(u8::from(a.int(291).unwrap_or(1) != 0));
    d.bd(a.real(41).unwrap_or(0.0));
    w.handles.handle(5, a.handle(342).unwrap_or(0));
    let d = &mut w.data;
    d.bd(a.real(42).unwrap_or(0.0));
    d.bs(a.int(172).unwrap_or(2) as i16);
    w.handles
        .handle(5, a.handle(343).unwrap_or(names.standard_style));
    let d = &mut w.data;
    d.bs(a.int(173).unwrap_or(1) as i16);
    d.bs(a.int(95).unwrap_or(1) as i16);
    d.bs(a.int(174).unwrap_or(0) as i16);
    d.bs(a.int(175).unwrap_or(0) as i16);
    w.cmc(color_index(a.int(92)));
    let d = &mut w.data;
    d.bit(u8::from(a.int(292).unwrap_or(0) != 0));
    w.handles.handle(5, a.handle(344).unwrap_or(0));
    w.cmc(color_index(a.int(93)));
    let d = &mut w.data;
    d.bd3(
        a.point(10).unwrap_or(Point2::new(1.0, 1.0)),
        a.real(30).unwrap_or(1.0),
    );
    d.bd(a.real(43).unwrap_or(0.0).to_radians());
    d.bs(a.int(176).unwrap_or(0) as i16);
    d.bit(u8::from(a.int(293).unwrap_or(0) != 0));
    // Arrowheads: a 94 each, with a 345 handle; labels: a 330 each with
    // 302, 177, 44.
    let arrowheads = a.texts(94);
    let arrow_handles = a.handles(345);
    d.bl(arrowheads.len() as i32);
    for (i, flag) in arrowheads.iter().enumerate() {
        w.data.bit(u8::from(flag.trim() != "0"));
        w.handles
            .handle(5, arrow_handles.get(i).copied().unwrap_or(0));
    }
    let labels = a.handles(330);
    let label_texts = a.texts(302);
    let label_indices = a.texts(177);
    let label_widths = a.reals(44);
    w.data.bl(labels.len() as i32);
    for (i, attdef) in labels.iter().enumerate() {
        w.handles.handle(5, *attdef);
        w.text(label_texts.get(i).copied().unwrap_or(""));
        let d = &mut w.data;
        d.bs(label_indices
            .get(i)
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(0));
        d.bd(label_widths.get(i).copied().unwrap_or(0.0));
    }
    let d = &mut w.data;
    d.bit(u8::from(a.int(294).unwrap_or(0) != 0));
    d.bs(a.int(178).unwrap_or(0) as i16);
    d.bs(a.int(179).unwrap_or(0) as i16);
    d.bd(a.real(45).unwrap_or(1.0));
    Ok(())
}

/// A colour index from a multileader's colour group: a negative value is
/// "by block" as those groups say it, written as index 0.
fn color_index(value: Option<i64>) -> i16 {
    match value {
        Some(v) if (0..=256).contains(&v) => v as i16,
        _ => 0,
    }
}
