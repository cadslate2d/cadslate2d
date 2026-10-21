//! The objects that are records of the drawing (ODA spec §20.4): the
//! reverse of `records.rs`, each from the DXF groups the exporter of
//! `cad-dxf` writes.

use cad_core::Point2;

use crate::objects::types;
use crate::write::fields::Fields;
use crate::write::objects::{ObjectBytes, ObjectWriter};
use crate::write::{Names, WriteError};

/// Encodes an object record of the OBJECTS section.
pub fn object(
    kind: &str,
    groups: &[(i32, String)],
    names: &Names,
) -> Result<ObjectBytes, WriteError> {
    let f = Fields::new(groups);
    let handle = f
        .handle(5)
        .ok_or_else(|| WriteError::new(kind, "an object without a handle"))?;
    let owner = f.handle(330).unwrap_or(0);
    let class = |name: &str| {
        names
            .classes
            .get(name)
            .copied()
            .ok_or_else(|| WriteError::new(name, "no class defined for it"))
    };
    Ok(match kind {
        "DICTIONARY" => dictionary(&f, types::DICTIONARY, handle, owner, false, names),
        "ACDBDICTIONARYWDFLT" => dictionary(&f, class(kind)?, handle, owner, true, names),
        "ACDBPLACEHOLDER" => {
            let mut w = ObjectWriter::new(types::PLACEHOLDER, handle, names.version);
            w.object_common(0);
            w.object_handles(owner, 0);
            w.finish()
        }
        "LAYOUT" => layout(&f, class(kind)?, handle, owner, names),
        "GROUP" => {
            let mut w = ObjectWriter::new(types::GROUP, handle, names.version);
            w.object_common(0);
            w.data.text(f.text(300).unwrap_or(""));
            w.data.bs(f.int(70).unwrap_or(0) as i16);
            w.data.bs(f.int(71).unwrap_or(1) as i16);
            let members = f.handles(340);
            w.data.bl(members.len() as i32);
            w.object_handles(owner, 0);
            for member in members {
                w.handles.handle(5, member);
            }
            w.finish()
        }
        "SORTENTSTABLE" => sortents_table(&f, class(kind)?, handle, owner, names),
        "XRECORD" => xrecord(&f, handle, owner, names)?,
        "MLEADERSTYLE" => mleader_style(&f, class(kind)?, handle, owner, names),
        "WIPEOUTVARIABLES" => {
            let mut w = ObjectWriter::new(class(kind)?, handle, names.version);
            w.object_common(0);
            w.data.bs(f.int(70).unwrap_or(1) as i16);
            w.object_handles(owner, 0);
            w.finish()
        }
        other => {
            return Err(WriteError::new(
                other,
                "an object kind this crate does not write",
            ));
        }
    })
}

/// A DICTIONARY (§20.4.44), or one with a default (§20.4.45): each
/// entry's name, then its handle as a soft owner (350) or a hard owner
/// (360).
fn dictionary(
    f: &Fields<'_>,
    type_code: u16,
    handle: u64,
    owner: u64,
    with_default: bool,
    names: &Names,
) -> ObjectBytes {
    let mut entries: Vec<(&str, u64, u8)> = Vec::new();
    let mut name: Option<&str> = None;
    for (code, value) in f.groups() {
        match code {
            3 => name = Some(value.as_str()),
            350 | 360 => {
                if let (Some(n), Ok(h)) = (name.take(), u64::from_str_radix(value.trim(), 16)) {
                    entries.push((n, h, if *code == 360 { 3 } else { 2 }));
                }
            }
            _ => {}
        }
    }
    let mut w = ObjectWriter::new(type_code, handle, names.version);
    w.object_common(0);
    w.data.bl(entries.len() as i32);
    w.data.bs(f.int(281).unwrap_or(1) as i16);
    w.data.rc(f.int(280).unwrap_or(0) as u8);
    for (n, _, _) in &entries {
        w.data.text(n);
    }
    w.object_handles(owner, 0);
    for (_, h, code) in &entries {
        w.handles.handle(*code, *h);
    }
    if with_default {
        w.handles.handle(5, f.handle(340).unwrap_or(0));
    }
    w.finish()
}

/// A LAYOUT (§20.4.84): the plot settings, then the layout, each from its
/// subclass's groups.
fn layout(f: &Fields<'_>, type_code: u16, handle: u64, owner: u64, names: &Names) -> ObjectBytes {
    let p = f.subclass("AcDbPlotSettings");
    let l = f.subclass("AcDbLayout");
    let mut w = ObjectWriter::new(type_code, handle, names.version);
    w.object_common(0);
    let r2004 = w.r2004();
    let d = &mut w.data;
    d.text(p.text(1).unwrap_or(""));
    d.text(p.text(2).unwrap_or(""));
    d.bs(p.int(70).unwrap_or(0) as i16);
    for code in 40..=43 {
        d.bd(p.real(code).unwrap_or(0.0));
    }
    d.bd(p.real(44).unwrap_or(420.0));
    d.bd(p.real(45).unwrap_or(297.0));
    d.text(p.text(4).unwrap_or(""));
    d.bd2(Point2::new(
        p.real(46).unwrap_or(0.0),
        p.real(47).unwrap_or(0.0),
    ));
    d.bs(p.int(72).unwrap_or(1) as i16);
    d.bs(p.int(73).unwrap_or(0) as i16);
    d.bs(p.int(74).unwrap_or(5) as i16);
    d.bd2(Point2::new(
        p.real(48).unwrap_or(0.0),
        p.real(49).unwrap_or(0.0),
    ));
    d.bd2(Point2::new(
        p.real(140).unwrap_or(0.0),
        p.real(141).unwrap_or(0.0),
    ));
    if !r2004 {
        d.text(p.text(6).unwrap_or("")); // the plot view, by name
    }
    d.bd(p.real(142).unwrap_or(1.0));
    d.bd(p.real(143).unwrap_or(1.0));
    d.text(p.text(7).unwrap_or(""));
    d.bs(p.int(75).unwrap_or(16) as i16);
    d.bd(p.real(147).unwrap_or(1.0));
    d.bd2(Point2::new(
        p.real(148).unwrap_or(0.0),
        p.real(149).unwrap_or(0.0),
    ));
    if r2004 {
        d.bs(p.int(76).unwrap_or(0) as i16);
        d.bs(p.int(77).unwrap_or(2) as i16);
        d.bs(p.int(78).unwrap_or(300) as i16);
    }
    d.text(l.text(1).unwrap_or(""));
    d.bl(l.int(71).unwrap_or(0) as i32);
    d.bs(l.int(70).unwrap_or(1) as i16);
    d.bd3(l.point(12).unwrap_or(Point2::ORIGIN), l.z(12));
    d.rd2(l.point(10).unwrap_or(Point2::ORIGIN));
    d.rd2(l.point(11).unwrap_or(Point2::ORIGIN));
    d.bd3(l.point(13).unwrap_or(Point2::ORIGIN), l.z(13));
    d.bd3(l.point(16).unwrap_or(Point2::new(1.0, 0.0)), l.z(16));
    d.bd3(l.point(17).unwrap_or(Point2::new(0.0, 1.0)), l.z(17));
    d.bd(l.real(146).unwrap_or(0.0));
    d.bs(l.int(76).unwrap_or(0) as i16);
    d.bd3(
        l.point(14).unwrap_or(Point2::new(1e20, 1e20)),
        l.real(34).unwrap_or(1e20),
    );
    d.bd3(
        l.point(15).unwrap_or(Point2::new(-1e20, -1e20)),
        l.real(35).unwrap_or(-1e20),
    );
    if r2004 {
        w.data.bl(0); // no viewports listed
    }
    w.object_handles(owner, 0);
    let h = &mut w.handles;
    if r2004 {
        h.handle(5, 0); // the plot view
    }
    h.handle(4, l.handle(330).unwrap_or(0)); // the block record
    h.handle(4, l.handle(331).unwrap_or(0)); // the active viewport
    h.handle(5, 0); // the base UCS
    h.handle(5, 0); // the named UCS
    w.finish()
}

/// A SORTENTSTABLE (§20.4.93): the sort handles in the data, the block
/// record and the entities in the handle stream.
fn sortents_table(
    f: &Fields<'_>,
    type_code: u16,
    handle: u64,
    owner: u64,
    names: &Names,
) -> ObjectBytes {
    let s = f.subclass("AcDbSortentsTable");
    let mut pairs: Vec<(u64, u64)> = Vec::new();
    let mut entity: Option<u64> = None;
    for (code, value) in s.groups() {
        match code {
            331 => entity = u64::from_str_radix(value.trim(), 16).ok(),
            5 => {
                if let (Some(e), Ok(sort)) = (entity.take(), u64::from_str_radix(value.trim(), 16))
                {
                    pairs.push((e, sort));
                }
            }
            _ => {}
        }
    }
    let mut w = ObjectWriter::new(type_code, handle, names.version);
    w.object_common(0);
    w.data.bl(pairs.len() as i32);
    for (_, sort) in &pairs {
        w.data.handle(0, *sort);
    }
    w.object_handles(owner, 0);
    w.handles.handle(4, s.handle(330).unwrap_or(0));
    for (e, _) in &pairs {
        w.handles.handle(4, *e);
    }
    w.finish()
}

/// How a DXF group's value is stored in an xrecord, by its code: the
/// DXF convention (§20.4.104, as `records::xrecord_kind` reads it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Text,
    Point,
    Real,
    Int8,
    Int16,
    Int32,
    Int64,
    Handle(u8),
    Binary,
}

fn kind_of(code: i32) -> Option<Kind> {
    Some(match code {
        0..=9 | 100..=104 | 300..=309 | 410..=419 | 430..=439 | 470..=479 | 999 => Kind::Text,
        1000..=1003 | 1005..=1009 => Kind::Text,
        10..=37 | 110..=139 | 210..=269 | 1010..=1039 => Kind::Point,
        38..=59 | 140..=149 | 460..=469 | 1040..=1042 => Kind::Real,
        60..=79 | 170..=179 | 270..=279 | 370..=389 | 400..=409 | 1070 => Kind::Int16,
        80..=99 | 420..=429 | 440..=459 | 1071 => Kind::Int32,
        105 => Kind::Handle(5),
        160..=169 => Kind::Int64,
        280..=299 => Kind::Int8,
        310..=319 | 1004 => Kind::Binary,
        320..=329 | 340..=349 | 390..=399 => Kind::Handle(5),
        330..=339 => Kind::Handle(4),
        350..=359 => Kind::Handle(2),
        360..=369 => Kind::Handle(3),
        _ => return None,
    })
}

/// An XRECORD (§20.4.104): its groups after the subclass marker as typed
/// data, each handle also in the handle stream, then the cloning flag.
fn xrecord(
    f: &Fields<'_>,
    handle: u64,
    owner: u64,
    names: &Names,
) -> Result<ObjectBytes, WriteError> {
    let body = f.subclass("AcDbXrecord");
    let mut data = crate::write::bits::BitWriter::new();
    let mut handles: Vec<(u8, u64)> = Vec::new();
    let mut groups = body.groups().iter().peekable();
    while let Some((code, value)) = groups.next() {
        if *code == 280 {
            continue;
        }
        let Some(kind) = kind_of(*code) else {
            return Err(WriteError::new("XRECORD", "a group code no xrecord holds"));
        };
        data.rs(*code as u16);
        let v = value.trim();
        match kind {
            Kind::Text => {
                let bytes = value.as_bytes();
                data.rs(bytes.len().min(u16::MAX as usize) as u16);
                data.rc(30); // the code page
                for b in &bytes[..bytes.len().min(u16::MAX as usize)] {
                    data.rc(*b);
                }
            }
            Kind::Point => {
                data.rd(v.parse().unwrap_or(0.0));
                let mut y = 0.0;
                let mut z = 0.0;
                if groups.peek().is_some_and(|(c, _)| *c == code + 10) {
                    y = groups
                        .next()
                        .and_then(|(_, v)| v.trim().parse().ok())
                        .unwrap_or(0.0);
                }
                if groups.peek().is_some_and(|(c, _)| *c == code + 20) {
                    z = groups
                        .next()
                        .and_then(|(_, v)| v.trim().parse().ok())
                        .unwrap_or(0.0);
                }
                data.rd(y);
                data.rd(z);
            }
            Kind::Real => data.rd(v.parse().unwrap_or(0.0)),
            Kind::Int8 => data.rc(v.parse::<i64>().unwrap_or(0) as u8),
            Kind::Int16 => data.rs(v.parse::<i64>().unwrap_or(0) as u16),
            Kind::Int32 => data.rl(v.parse::<i64>().unwrap_or(0) as u32),
            Kind::Int64 => data.rll(v.parse::<i64>().unwrap_or(0) as u64),
            Kind::Handle(code) => {
                let h = u64::from_str_radix(v, 16).unwrap_or(0);
                data.rll(h);
                handles.push((code, h));
            }
            Kind::Binary => {
                let bytes: Vec<u8> = (0..v.len() / 2)
                    .filter_map(|i| u8::from_str_radix(&v[2 * i..2 * i + 2], 16).ok())
                    .collect();
                data.rc(bytes.len().min(255) as u8);
                for b in bytes.iter().take(255) {
                    data.rc(*b);
                }
            }
        }
    }
    let mut w = ObjectWriter::new(types::XRECORD, handle, names.version);
    w.object_common(0);
    w.data.bl(data.len() as i32);
    w.data.append(&data);
    w.data.bs(f.int(280).unwrap_or(1) as i16);
    w.object_handles(owner, 0);
    for (code, h) in handles {
        w.handles.handle(code, h);
    }
    Ok(w.finish())
}

/// A MLEADERSTYLE (§20.4.87), every value under its DXF code in
/// LibreDWG's order.
fn mleader_style(
    f: &Fields<'_>,
    type_code: u16,
    handle: u64,
    owner: u64,
    names: &Names,
) -> ObjectBytes {
    let color = |code: i32| match f.int(code) {
        Some(v) if (0..=256).contains(&v) => v as i16,
        _ => 0,
    };
    let mut w = ObjectWriter::new(type_code, handle, names.version);
    w.object_common(0);
    let d = &mut w.data;
    d.bs(f.int(170).unwrap_or(2) as i16);
    d.bs(f.int(171).unwrap_or(1) as i16);
    d.bs(f.int(172).unwrap_or(0) as i16);
    d.bl(f.int(90).unwrap_or(2) as i32);
    d.bd(f.real(40).unwrap_or(0.0).to_radians());
    d.bd(f.real(41).unwrap_or(0.0).to_radians());
    d.bs(f.int(173).unwrap_or(1) as i16);
    w.cmc(color(91));
    let d = &mut w.data;
    d.bl(f.int(92).unwrap_or(-2) as i32);
    d.bit(u8::from(f.int(290).unwrap_or(1) != 0));
    d.bd(f.real(42).unwrap_or(0.0));
    d.bit(u8::from(f.int(291).unwrap_or(1) != 0));
    d.bd(f.real(43).unwrap_or(0.0));
    d.text(f.text(3).unwrap_or(""));
    d.bd(f.real(44).unwrap_or(0.0));
    d.text(f.text(300).unwrap_or(""));
    d.bs(f.int(174).unwrap_or(1) as i16);
    d.bs(f.int(178).unwrap_or(1) as i16);
    d.bs(f.int(175).unwrap_or(1) as i16);
    d.bs(f.int(176).unwrap_or(0) as i16);
    w.cmc(color(93));
    let d = &mut w.data;
    d.bd(f.real(45).unwrap_or(0.0));
    d.bit(u8::from(f.int(292).unwrap_or(0) != 0));
    d.bit(u8::from(f.int(297).unwrap_or(0) != 0));
    d.bd(f.real(46).unwrap_or(0.0));
    w.cmc(color(94));
    let d = &mut w.data;
    d.bd(f.real(47).unwrap_or(1.0));
    d.bd(f.real(49).unwrap_or(1.0));
    d.bd(f.real(140).unwrap_or(1.0));
    d.bit(u8::from(f.int(293).unwrap_or(0) != 0));
    d.bd(f.real(141).unwrap_or(0.0).to_radians());
    d.bit(u8::from(f.int(294).unwrap_or(0) != 0));
    d.bs(f.int(177).unwrap_or(0) as i16);
    d.bd(f.real(142).unwrap_or(1.0));
    d.bit(u8::from(f.int(295).unwrap_or(0) != 0));
    d.bit(u8::from(f.int(296).unwrap_or(0) != 0));
    d.bd(f.real(143).unwrap_or(0.0));
    w.object_handles(owner, 0);
    let h = &mut w.handles;
    h.handle(5, f.handle(340).unwrap_or(0)); // the linetype
    h.handle(5, f.handle(341).unwrap_or(0)); // the arrowhead
    h.handle(5, f.handle(342).unwrap_or(names.standard_style));
    h.handle(5, f.handle(343).unwrap_or(0)); // the block
    w.finish()
}
