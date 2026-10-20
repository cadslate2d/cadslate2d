//! A DWG file into the drawing (ADR-0036 §2): every object decoded by its
//! prescription into the DXF groups the specification lists beside each
//! field, handed to `cad-dxf`'s reader as records in a file's order — the
//! header variables, the tables, the blocks, the entities, the objects —
//! so that what a record means is written once.

use std::collections::{HashMap, HashSet};

use cad_dxf::{Record, Section};

use crate::entities::{self, Decoded, LinetypeRef};
use crate::groups::Groups;
use crate::objects::{fixed_type_is_entity, types};
use crate::records::{self, ObjectRecord};
use crate::tables::{self, BlockHeader, Row};
use crate::{DwgError, File, ImportLimits};

/// A drawing read from a DWG file, with the report of what was not held.
pub type Import = cad_dxf::Import;

/// Report keys for what a DWG carries that no record can say.
const UNREADABLE: &str = "(unreadable-dwg-object)";
const MISSING: &str = "(dwg-object-missing-from-file)";

/// Imports a DWG file with the default limits.
pub fn import(bytes: &[u8]) -> Result<Import, DwgError> {
    import_with_limits(
        bytes,
        &ImportLimits::default(),
        &cad_dxf::ImportLimits::default(),
    )
}

/// Imports a DWG file: the containers under `limits`, the drawing under
/// `dxf_limits`, which bound what the records may make.
pub fn import_with_limits(
    bytes: &[u8],
    limits: &ImportLimits,
    dxf_limits: &cad_dxf::ImportLimits,
) -> Result<Import, DwgError> {
    let file = File::open_with_limits(bytes, limits)?;
    let records = records(&file)?;
    cad_dxf::import_records(records, dxf_limits).map_err(DwgError::Drawing)
}

/// The records of a file, in a file's order.
pub fn records(file: &File<'_>) -> Result<Vec<Record>, DwgError> {
    let classes = file.classes()?;
    let header = file.header()?;
    let class_of = |type_code: u16| classes.iter().find(|c| c.number == type_code);

    let mut rows: Vec<Row> = Vec::new();
    let mut blocks: Vec<BlockHeader> = Vec::new();
    let mut entities: HashMap<u64, Decoded> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();
    let mut objects: Vec<ObjectRecord> = Vec::new();
    let mut unreadable: Vec<String> = Vec::new();

    for object in file.objects() {
        let object = match object {
            Ok(object) => object,
            Err(_) => {
                unreadable.push(UNREADABLE.to_owned());
                continue;
            }
        };
        let class = class_of(object.type_code);
        if class.is_none() && object.type_code >= types::FIRST_CLASS {
            // A class the drawing never defined: nothing can be said of
            // its instances but that they are there.
            unreadable.push(format!("(unknown-class-{})", object.type_code));
            continue;
        }
        let is_entity = fixed_type_is_entity(object.type_code)
            .unwrap_or_else(|| class.is_some_and(|c| c.is_entity()));
        let Ok((_, mut streams)) = file.open_object(&object) else {
            unreadable.push(UNREADABLE.to_owned());
            continue;
        };
        if is_entity {
            match entities::decode(
                file,
                &object,
                &mut streams,
                class.map(|c| c.dxf_name.as_str()),
            ) {
                Ok(decoded) => {
                    order.push(decoded.handle);
                    entities.insert(decoded.handle, decoded);
                }
                Err(_) => {
                    let kind = class.map_or_else(
                        || entity_kind_name(object.type_code),
                        |c| c.dxf_name.clone(),
                    );
                    unreadable.push(kind);
                }
            }
            continue;
        }
        let common = match file.object_common(&mut streams, &object) {
            Ok(common) => common,
            Err(_) => {
                unreadable.push(UNREADABLE.to_owned());
                continue;
            }
        };
        let handle = object.handle;
        let class_name = class.map(|c| c.dxf_name.as_str());
        let mut keep = |decoded: Result<ObjectRecord, DwgError>| {
            decoded.map(|record| {
                objects.push(record);
                None
            })
        };
        let decoded = match (object.type_code, class_name) {
            (types::DICTIONARY, _) | (_, Some("DICTIONARY")) => keep(records::dictionary(
                file,
                &mut streams,
                &common,
                handle,
                false,
            )),
            (_, Some("ACDBDICTIONARYWDFLT")) => keep(records::dictionary(
                file,
                &mut streams,
                &common,
                handle,
                true,
            )),
            (types::LAYOUT, _) | (_, Some("LAYOUT")) => {
                keep(records::layout(file, &mut streams, &common, handle))
            }
            (types::GROUP, _) | (_, Some("GROUP")) => {
                keep(records::group(file, &mut streams, &common, handle))
            }
            (_, Some("SORTENTSTABLE")) => {
                keep(records::sortents_table(file, &mut streams, &common, handle))
            }
            (types::XRECORD, _) | (_, Some("XRECORD")) => {
                keep(records::xrecord(file, &mut streams, &common, handle))
            }
            (_, Some("MLEADERSTYLE")) => {
                keep(records::mleader_style(file, &mut streams, &common, handle))
            }
            (_, Some("WIPEOUTVARIABLES")) => keep(records::wipeout_variables(
                file,
                &mut streams,
                &common,
                handle,
            )),
            (types::VIEW, _) => keep(records::view(file, &mut streams, &common, handle)),
            (types::LAYER, _) => tables::layer(file, &mut streams, &common, handle).map(Some),
            (types::LTYPE, _) => tables::linetype(file, &mut streams, &common, handle).map(Some),
            (types::STYLE, _) => tables::text_style(file, &mut streams, &common, handle).map(Some),
            (types::DIMSTYLE, _) => {
                tables::dim_style(file, &mut streams, &common, handle).map(Some)
            }
            (types::BLOCK_HEADER, _) => {
                match tables::block_header(file, &mut streams, &common, handle) {
                    Ok(block) => {
                        blocks.push(block);
                        Ok(None)
                    }
                    Err(e) => Err(e),
                }
            }
            _ => Ok(None),
        };
        match decoded {
            Ok(Some(row)) => rows.push(row),
            Ok(None) => {}
            Err(_) => unreadable.push(UNREADABLE.to_owned()),
        }
    }

    // A block's name is one of a kind. A second paper-space block under a
    // name taken — a writer that names every layout's block `*Paper_Space`
    // — is named as the next one is named: `*Paper_Space0`, `*Paper_Space1`.
    let mut taken: HashSet<String> = HashSet::new();
    for block in &mut blocks {
        let key = block.name.to_uppercase();
        if taken.insert(key) || !block.name.eq_ignore_ascii_case("*Paper_Space") {
            continue;
        }
        let next = (0..)
            .map(|n| format!("*Paper_Space{n}"))
            .find(|name| !taken.contains(&name.to_uppercase()))
            .expect("some number is free");
        taken.insert(next.to_uppercase());
        block.name = next;
    }

    // Names by handle, for what entities and rows name that way.
    let names: HashMap<u64, &str> = rows
        .iter()
        .map(|row| (row.handle, row.name.as_str()))
        .chain(blocks.iter().map(|b| (b.handle, b.name.as_str())))
        .collect();
    let name_of = |handle: u64| names.get(&handle).copied();

    let mut out: Vec<Record> = Vec::new();

    // The header variables the drawing has settings for.
    let mut var = |kind: &str, groups: Groups| {
        out.push(Record {
            section: Section::Header,
            kind: kind.to_owned(),
            groups: groups.0,
        });
    };
    if let Some(units) = header.int("INSUNITS") {
        let mut g = Groups::default();
        g.int(70, units);
        var("$INSUNITS", g);
    }
    for (variable, name, code) in [
        ("CLAYER", "$CLAYER", 8),
        ("CELTYPE", "$CELTYPE", 6),
        ("DIMSTYLE", "$DIMSTYLE", 2),
        ("TEXTSTYLE", "$TEXTSTYLE", 7),
    ] {
        if let Some(found) = header.handle(variable).and_then(|h| name_of(h.value)) {
            let mut g = Groups::default();
            g.text(code, found);
            var(name, g);
        }
    }
    if let Some(color) = header.color("CECOLOR") {
        let mut g = Groups::default();
        g.int(62, i64::from(color.index));
        var("$CECOLOR", g);
    }
    if let Some(flags) = header.int("FLAGS") {
        let mut g = Groups::default();
        g.int(370, tables::lineweight_code((flags & 0x1f) as u8));
        var("$CELWEIGHT", g);
    }
    for (variable, name, code) in [("LTSCALE", "$LTSCALE", 40), ("PDSIZE", "$PDSIZE", 40)] {
        if let Some(value) = header.real(variable) {
            let mut g = Groups::default();
            g.real(code, value);
            var(name, g);
        }
    }
    if let Some(mode) = header.int("PDMODE") {
        let mut g = Groups::default();
        g.int(70, mode);
        var("$PDMODE", g);
    }

    // The tables: each row, its handles named.
    for row in &rows {
        let mut groups = row.groups.clone();
        if let Some(linetype) = row.linetype {
            let name = name_of(linetype.value).unwrap_or("CONTINUOUS");
            groups.text(6, name);
        }
        if let Some(style) = row.text_style
            && let Some(name) = name_of(style.value)
        {
            groups.text(7, name);
        }
        out.push(Record {
            section: Section::Tables,
            kind: row.kind.to_owned(),
            groups: groups.0,
        });
    }
    for block in &blocks {
        let mut groups = Groups::default();
        groups.text(2, &block.name);
        groups.handle(5, block.handle);
        if let Some(layout) = block.layout.filter(|h| !h.is_null()) {
            groups.handle(340, layout.value);
        }
        out.push(Record {
            section: Section::Tables,
            kind: "BLOCK_RECORD".to_owned(),
            groups: groups.0,
        });
    }

    // The entities of each block, in the order the block owns them.
    let mut placed: HashSet<u64> = HashSet::new();
    let emit =
        |out: &mut Vec<Record>, section: Section, decoded: &Decoded, placed: &mut HashSet<u64>| {
            placed.insert(decoded.handle);
            if decoded.undecoded {
                // Nothing to say of it but its kind: reported, whatever
                // block held it.
                out.push(Record {
                    section: Section::Entities,
                    kind: decoded.kind.clone(),
                    groups: Vec::new(),
                });
                return;
            }
            // The properties before the entity's own groups, as a file
            // writes them: a multileader's own begin at its subclass marker
            // and are read on their own.
            let mut groups = Groups::default();
            groups.text(8, name_of(decoded.layer).unwrap_or("0"));
            match decoded.linetype {
                LinetypeRef::ByLayer => {}
                LinetypeRef::ByBlock => groups.text(6, "BYBLOCK"),
                LinetypeRef::Continuous => groups.text(6, "CONTINUOUS"),
                LinetypeRef::Named(handle) => {
                    if let Some(name) = name_of(handle) {
                        groups.text(6, name);
                    }
                }
            }
            if let Some(name) = decoded.text_style.and_then(name_of) {
                groups.text(7, name);
            }
            if let Some(name) = decoded.block.and_then(name_of) {
                groups.text(2, name);
            }
            if let Some(name) = decoded.dim_style.and_then(name_of) {
                groups.text(3, name);
            }
            if decoded.kind == "MULTILEADER" {
                groups.text(100, "AcDbMLeader");
            }
            groups.0.extend(decoded.groups.0.iter().cloned());
            out.push(Record {
                section,
                kind: decoded.kind.clone(),
                groups: groups.0,
            });
        };
    let chain = |first: Option<u64>,
                 last: Option<u64>,
                 owned: &[u64],
                 entities: &HashMap<u64, Decoded>|
     -> Vec<u64> {
        if !owned.is_empty() {
            return owned.to_vec();
        }
        let mut handles = Vec::new();
        let mut seen = HashSet::new();
        let mut at = first;
        while let Some(handle) = at {
            if !seen.insert(handle) || handles.len() > entities.len() {
                break;
            }
            let Some(entity) = entities.get(&handle) else {
                break;
            };
            handles.push(handle);
            if Some(handle) == last {
                break;
            }
            at = entity.next;
        }
        handles
    };
    // An entity with what it owns: a polyline's vertices and SEQEND, an
    // insert's attributes and SEQEND.
    let with_owned =
        |out: &mut Vec<Record>, section: Section, handle: u64, placed: &mut HashSet<u64>| {
            let Some(decoded) = entities.get(&handle) else {
                return;
            };
            if decoded.subordinate {
                return;
            }
            emit(out, section, decoded, placed);
            if decoded.kind == "POLYLINE" || decoded.has_seqend {
                let children = chain(decoded.first, decoded.last, &decoded.owned, &entities);
                for child in children {
                    if let Some(child) = entities.get(&child) {
                        emit(out, section, child, placed);
                    }
                }
                out.push(Record {
                    section,
                    kind: "SEQEND".to_owned(),
                    groups: Vec::new(),
                });
            }
        };
    let model_handle = header.handle("BLOCK_RECORD_MODEL_SPACE").map(|h| h.value);
    let mut model_entities: Vec<u64> = Vec::new();
    for block in &blocks {
        let members = chain(
            block.first.map(|h| h.resolve(block.handle)),
            block.last.map(|h| h.resolve(block.handle)),
            &block
                .owned
                .iter()
                .map(|h| h.resolve(block.handle))
                .collect::<Vec<_>>(),
            &entities,
        );
        if Some(block.handle) == model_handle {
            model_entities = members;
            continue;
        }
        let mut groups = Groups::default();
        groups.text(2, &block.name);
        groups.int(
            70,
            i64::from(block.anonymous) | if block.is_xref { 4 } else { 0 },
        );
        groups.point(10, block.base);
        groups.text(3, &block.name);
        groups.handle(5, block.handle);
        out.push(Record {
            section: Section::Blocks,
            kind: "BLOCK".to_owned(),
            groups: groups.0,
        });
        for handle in members {
            with_owned(&mut out, Section::Blocks, handle, &mut placed);
        }
        out.push(Record {
            section: Section::Blocks,
            kind: "ENDBLK".to_owned(),
            groups: Vec::new(),
        });
    }
    for handle in model_entities {
        with_owned(&mut out, Section::Entities, handle, &mut placed);
    }
    // Entities no block reached, in handle order: model space, or paper
    // space when they say so.
    for handle in &order {
        if placed.contains(handle) {
            continue;
        }
        if let Some(decoded) = entities.get(handle)
            && !decoded.subordinate
        {
            with_owned(&mut out, Section::Entities, *handle, &mut placed);
        }
    }
    // The objects, the views among the tables.
    for object in objects {
        let section = if object.kind == "VIEW" {
            Section::Tables
        } else {
            Section::Objects
        };
        out.push(Record {
            section,
            kind: object.kind.to_owned(),
            groups: object.groups.0,
        });
    }
    for kind in unreadable {
        out.push(Record {
            section: Section::Entities,
            kind,
            groups: Vec::new(),
        });
    }
    for _ in 0..file.unreachable_objects() {
        out.push(Record {
            section: Section::Entities,
            kind: MISSING.to_owned(),
            groups: Vec::new(),
        });
    }
    Ok(out)
}

/// The report name of an entity that could not be decoded.
fn entity_kind_name(type_code: u16) -> String {
    format!("(unreadable-dwg-entity-{type_code:#x})")
}
