//! A drawing into a DWG file (ADR-0036 §2, #315): the records the DXF
//! exporter writes (`cad_dxf::export_records`) — the one walk over the
//! drawing — encoded object by object by the prescriptions of the ODA
//! specification, into the R2000 container. What the DWG reader turns
//! into records, this turns back into bits: the two are mirrors, and
//! what a record means stays written once, in `cad-dxf`.
//!
//! The handles are the records' own, so the output is the same for the
//! same drawing. What the DXF carries by name — a layer, a linetype, a
//! style, a block — the DWG carries by handle, resolved through the
//! tables' rows; the chains an R2000 file links its entities in (previous
//! and next, a block's first and last) are made here from the records'
//! order.

use std::collections::HashMap;
use std::fmt;

use cad_core::Point2;
use cad_document::Drawing;
use cad_dxf::{Record, Section};

use crate::Version;
use crate::bits::{Color, Handle};
use crate::header::Value;
use crate::write::classes::ClassDef;
use crate::write::fields::{Fields, lineweight_index};
use crate::write::objects::ObjectBytes;

pub mod bits;
mod classes;
mod crc64;
mod entities;
mod fields;
mod header;
mod lz77;
mod objects;
mod r2000;
mod r2004;
mod r2007;
mod records;
mod rs;
mod tables;

/// A record the writer cannot encode: its kind and why. The records of
/// this project's own exporter never raise one; another front end's
/// might.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WriteError {
    pub kind: String,
    pub what: &'static str,
}

impl WriteError {
    fn new(kind: &str, what: &'static str) -> Self {
        Self {
            kind: kind.to_owned(),
            what,
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cannot write a {} record: {}", self.kind, self.what)
    }
}

impl std::error::Error for WriteError {}

/// What the records name, by upper-case name, to the handle it has; and
/// the version being written, which every encoder branches on.
#[derive(Debug)]
pub struct Names {
    pub version: Version,
    pub layers: HashMap<String, u64>,
    pub linetypes: HashMap<String, u64>,
    pub text_styles: HashMap<String, u64>,
    pub dim_styles: HashMap<String, u64>,
    pub block_records: HashMap<String, u64>,
    /// Each class's object type, by DXF name.
    pub classes: HashMap<String, u16>,
    pub layer_zero: u64,
    pub standard_style: u64,
    pub standard_dim_style: u64,
    pub model_space: u64,
    pub paper_space: u64,
    pub ltype_byblock: u64,
    pub ltype_bylayer: u64,
    pub ltype_continuous: u64,
}

/// The attributes an INSERT owns, in order, and the SEQEND: R2000 names
/// the first and last, R2004 lists them all.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Owned {
    pub handles: Vec<u64>,
    pub seqend: u64,
}

/// Where an entity sits (§20.4.1–20.4.2): its space or owner, its
/// neighbours in the owner's chain, and what it owns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    pub entmode: u8,
    pub owner: u64,
    pub prev: u64,
    pub next: u64,
    pub owned: Option<Owned>,
}

/// What a block header needs of its definition (§20.4.52): the chain of
/// entities between its BLOCK and ENDBLK, which R2000 names by its ends
/// and R2004 lists.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BlockPlacement {
    pub flags: i64,
    pub base: Point2,
    pub base_z: f64,
    pub has_attributes: bool,
    pub begin: u64,
    pub end: u64,
    pub members: Vec<u64>,
}

/// Writes a drawing as a DWG file of `version`: AC1015 (AutoCAD 2000),
/// AC1018 (2004), or AC1021 (2007).
pub fn export(drawing: &Drawing, version: Version) -> Result<Vec<u8>, WriteError> {
    write_records(&cad_dxf::export_records(drawing), version)
}

/// What a DWG of `version` will not include of `drawing` (#319), line by
/// line, for the host to warn with before the export; empty when the file
/// holds the drawing whole. A spline is by its control points or by its
/// fit points (§20.4.40): one given by control data of its own and fit
/// points too keeps the control data, which is its geometry.
pub fn export_losses(drawing: &Drawing, _version: Version) -> Vec<String> {
    let splines = drawing
        .entities()
        .filter(|(_, entity)| match entity {
            cad_document::Entity::Spline(spline) => {
                let controls: Vec<(Point2, f64)> =
                    spline.control.iter().map(|p| (*p, 0.0)).collect();
                let fits: Vec<(Point2, f64)> = spline.fit.iter().map(|p| (*p, 0.0)).collect();
                !spline.fit.is_empty()
                    && !entities::fit_points_define(
                        &fits,
                        spline.degree as i32,
                        &controls,
                        &spline.knots,
                        &spline.weights,
                    )
            }
            _ => false,
        })
        .count();
    let mut losses = Vec::new();
    if splines > 0 {
        losses.push(format!(
            "{splines} spline{} with control points of their own also carry fit points, which a DWG cannot hold beside them (the control points are kept)",
            if splines == 1 { "" } else { "s" }
        ));
    }
    losses
}

/// Writes records — the DXF exporter's, or another front end's — as a
/// file of `version`.
pub fn write_records(records: &[Record], version: Version) -> Result<Vec<u8>, WriteError> {
    let plan = Plan::new(records, version)?;
    let mut objects: Vec<ObjectBytes> = Vec::new();
    let names = &plan.names;

    // The tables: each control object and its rows.
    for table in &plan.tables {
        let f = Fields::new(&table.head.groups);
        let handle = f
            .handle(5)
            .ok_or_else(|| WriteError::new("TABLE", "a table without a handle"))?;
        let xdic = f.handle(360).unwrap_or(0);
        let entries: Vec<u64> = table
            .rows
            .iter()
            .filter_map(|row| {
                let f = Fields::new(&row.groups);
                f.handle(5).or_else(|| f.handle(105))
            })
            .collect();
        objects.push(tables::control(&table.name, handle, xdic, &entries, names)?);
        for row in &table.rows {
            let block = Fields::new(&row.groups)
                .handle(5)
                .and_then(|h| plan.blocks.get(&h));
            objects.push(tables::row(&table.name, &row.groups, handle, names, block)?);
        }
    }
    if let Some(vx_control) = plan.vx_control {
        objects.push(tables::viewport_entity_header_control(vx_control));
    }

    for (record, place) in &plan.entities {
        objects.push(entities::entity(
            &record.kind,
            &record.groups,
            place,
            names,
        )?);
    }
    for record in &plan.objects {
        objects.push(records::object(&record.kind, &record.groups, names)?);
    }
    objects.sort_by_key(|object| object.handle);

    let header = header::section(&plan.header_values(), version);
    let classes = classes::section(&plan.classes, &plan.instances(), version);
    match version {
        Version::R2000 => Ok(r2000::file(&r2000::Parts {
            header,
            classes,
            objects,
            handseed: plan.handseed,
            measurement: plan.measurement(),
        })),
        Version::R2004 | Version::R2007 => {
            let parts = r2004::Parts {
                header,
                classes,
                objects,
                handseed: plan.handseed,
                measurement: plan.measurement(),
            };
            Ok(if version == Version::R2004 {
                r2004::file(&parts)
            } else {
                r2007::file(&parts)
            })
        }
    }
}

/// A table: its TABLE record and its rows.
struct Table<'a> {
    name: String,
    head: &'a Record,
    rows: Vec<&'a Record>,
}

/// The records sorted into what the file is made of.
struct Plan<'a> {
    header: HashMap<String, &'a [(i32, String)]>,
    classes: Vec<ClassDef>,
    tables: Vec<Table<'a>>,
    entities: Vec<(&'a Record, Placement)>,
    objects: Vec<&'a Record>,
    blocks: HashMap<u64, BlockPlacement>,
    names: Names,
    /// The viewport entity header control, an object only an R2000 DWG
    /// has: it takes the first free handle.
    vx_control: Option<u64>,
    handseed: u64,
    root_dictionary: u64,
    root_entries: HashMap<String, u64>,
}

impl<'a> Plan<'a> {
    fn new(records: &'a [Record], version: Version) -> Result<Self, WriteError> {
        let mut header: HashMap<String, &'a [(i32, String)]> = HashMap::new();
        let mut classes: Vec<ClassDef> = vec![
            ClassDef {
                dxf_name: "ACDBDICTIONARYWDFLT".to_owned(),
                cpp_name: "AcDbDictionaryWithDefault".to_owned(),
                app_name: "ObjectDBX Classes".to_owned(),
                proxy_flags: 0,
                was_zombie: false,
                is_entity: false,
            },
            ClassDef {
                dxf_name: "ACDBPLACEHOLDER".to_owned(),
                cpp_name: "AcDbPlaceHolder".to_owned(),
                app_name: "ObjectDBX Classes".to_owned(),
                proxy_flags: 0,
                was_zombie: false,
                is_entity: false,
            },
            ClassDef {
                dxf_name: "LAYOUT".to_owned(),
                cpp_name: "AcDbLayout".to_owned(),
                app_name: "ObjectDBX Classes".to_owned(),
                proxy_flags: 0,
                was_zombie: false,
                is_entity: false,
            },
        ];
        let mut tables: Vec<Table<'a>> = Vec::new();
        let mut block_defs: Vec<(&'a Record, Vec<&'a Record>, Option<&'a Record>)> = Vec::new();
        let mut entity_records: Vec<&'a Record> = Vec::new();
        let mut objects: Vec<&'a Record> = Vec::new();
        for record in records {
            match record.section {
                Section::Header => {
                    header.insert(record.kind.clone(), &record.groups);
                }
                Section::Classes => {
                    let f = Fields::new(&record.groups);
                    let dxf_name = f.text(1).unwrap_or("").trim().to_owned();
                    if !dxf_name.is_empty() && !classes.iter().any(|c| c.dxf_name == dxf_name) {
                        classes.push(ClassDef {
                            dxf_name,
                            cpp_name: f.text(2).unwrap_or("").trim().to_owned(),
                            app_name: f.text(3).unwrap_or("").trim().to_owned(),
                            proxy_flags: f.int(90).unwrap_or(0) as u16,
                            was_zombie: f.int(280).unwrap_or(0) != 0,
                            is_entity: f.int(281).unwrap_or(0) != 0,
                        });
                    }
                }
                Section::Tables => match record.kind.as_str() {
                    "TABLE" => tables.push(Table {
                        name: Fields::new(&record.groups)
                            .text(2)
                            .unwrap_or("")
                            .trim()
                            .to_owned(),
                        head: record,
                        rows: Vec::new(),
                    }),
                    "ENDTAB" => {}
                    _ => {
                        if let Some(table) = tables.last_mut() {
                            table.rows.push(record);
                        }
                    }
                },
                Section::Blocks => match record.kind.as_str() {
                    "BLOCK" => block_defs.push((record, Vec::new(), None)),
                    "ENDBLK" => {
                        if let Some(def) = block_defs.last_mut() {
                            def.2 = Some(record);
                        }
                    }
                    _ => {
                        if let Some(def) = block_defs.last_mut() {
                            def.1.push(record);
                        }
                    }
                },
                Section::Entities => entity_records.push(record),
                Section::Objects => objects.push(record),
            }
        }

        let mut names = Names {
            version,
            layers: HashMap::new(),
            linetypes: HashMap::new(),
            text_styles: HashMap::new(),
            dim_styles: HashMap::new(),
            block_records: HashMap::new(),
            classes: HashMap::new(),
            layer_zero: 0,
            standard_style: 0,
            standard_dim_style: 0,
            model_space: 0,
            paper_space: 0,
            ltype_byblock: 0,
            ltype_bylayer: 0,
            ltype_continuous: 0,
        };
        for (i, class) in classes.iter().enumerate() {
            names
                .classes
                .insert(class.dxf_name.clone(), (500 + i) as u16);
        }
        let mut max_handle = 0u64;
        let mut note = |handle: Option<u64>| {
            if let Some(h) = handle {
                max_handle = max_handle.max(h);
            }
        };
        for table in &tables {
            note(Fields::new(&table.head.groups).handle(5));
            let into: Option<&mut HashMap<String, u64>> = match table.name.as_str() {
                "LAYER" => Some(&mut names.layers),
                "LTYPE" => Some(&mut names.linetypes),
                "STYLE" => Some(&mut names.text_styles),
                "DIMSTYLE" => Some(&mut names.dim_styles),
                "BLOCK_RECORD" => Some(&mut names.block_records),
                _ => None,
            };
            let Some(into) = into else {
                for row in &table.rows {
                    note(Fields::new(&row.groups).handle(5));
                }
                continue;
            };
            for row in &table.rows {
                let f = Fields::new(&row.groups);
                let handle = f.handle(5).or_else(|| f.handle(105));
                note(handle);
                if let (Some(name), Some(handle)) = (f.text(2), handle) {
                    into.insert(name.trim().to_uppercase(), handle);
                }
            }
        }
        names.layer_zero = names.layers.get("0").copied().unwrap_or(0);
        names.standard_style = names.text_styles.get("STANDARD").copied().unwrap_or(0);
        names.standard_dim_style = names.dim_styles.get("STANDARD").copied().unwrap_or(0);
        names.model_space = names
            .block_records
            .get("*MODEL_SPACE")
            .copied()
            .unwrap_or(0);
        names.paper_space = names
            .block_records
            .get("*PAPER_SPACE")
            .copied()
            .unwrap_or(0);
        names.ltype_byblock = names.linetypes.get("BYBLOCK").copied().unwrap_or(0);
        names.ltype_bylayer = names.linetypes.get("BYLAYER").copied().unwrap_or(0);
        names.ltype_continuous = names.linetypes.get("CONTINUOUS").copied().unwrap_or(0);

        // The entities in each owner's order: the block definitions', then
        // the model's.
        let mut entities: Vec<(&'a Record, Placement)> = Vec::new();
        let mut blocks: HashMap<u64, BlockPlacement> = HashMap::new();
        let mut root_dictionary = 0u64;
        let mut root_entries: HashMap<String, u64> = HashMap::new();
        let entmode_of = |owner: u64| {
            if owner == names.model_space {
                2
            } else if owner == names.paper_space {
                1
            } else {
                0
            }
        };
        let place_chain =
            |members: &[&'a Record], entities: &mut Vec<(&'a Record, Placement)>| -> Vec<u64> {
                // The chain: every entity but the attributes an insert owns and
                // their SEQEND; those chain under the insert.
                let mut chain: Vec<(usize, u64)> = Vec::new();
                let mut i = 0;
                let mut placed: Vec<(usize, Placement)> = Vec::new();
                while i < members.len() {
                    let record = members[i];
                    let f = Fields::new(&record.groups);
                    let handle = f.handle(5).unwrap_or(0);
                    let owner = f.handle(330).unwrap_or(0);
                    let mut placement = Placement {
                        entmode: entmode_of(owner),
                        owner,
                        ..Placement::default()
                    };
                    chain.push((placed.len(), handle));
                    if record.kind == "INSERT" && f.int(66).unwrap_or(0) != 0 {
                        // The attributes up to the SEQEND.
                        let mut attribs: Vec<usize> = Vec::new();
                        let mut j = i + 1;
                        let mut seqend = 0u64;
                        while j < members.len() {
                            let kind = members[j].kind.as_str();
                            let h = Fields::new(&members[j].groups).handle(5).unwrap_or(0);
                            if kind == "SEQEND" {
                                seqend = h;
                                j += 1;
                                break;
                            }
                            if kind != "ATTRIB" {
                                break;
                            }
                            attribs.push(j);
                            j += 1;
                        }
                        let handles: Vec<u64> = attribs
                            .iter()
                            .map(|&k| Fields::new(&members[k].groups).handle(5).unwrap_or(0))
                            .collect();
                        placement.owned = Some(Owned {
                            handles: handles.clone(),
                            seqend,
                        });
                        placed.push((i, placement));
                        for (n, &k) in attribs.iter().enumerate() {
                            placed.push((
                                k,
                                Placement {
                                    entmode: 0,
                                    owner: handle,
                                    prev: if n == 0 { 0 } else { handles[n - 1] },
                                    next: handles.get(n + 1).copied().unwrap_or(0),
                                    owned: None,
                                },
                            ));
                        }
                        if seqend != 0 {
                            placed.push((
                                j - 1,
                                Placement {
                                    entmode: 0,
                                    owner: handle,
                                    ..Placement::default()
                                },
                            ));
                        }
                        i = j;
                        continue;
                    }
                    placed.push((i, placement));
                    i += 1;
                }
                for (n, &(at, _)) in chain.iter().enumerate() {
                    placed[at].1.prev = if n == 0 { 0 } else { chain[n - 1].1 };
                    placed[at].1.next = chain.get(n + 1).map_or(0, |(_, h)| *h);
                }
                for (k, placement) in placed {
                    entities.push((members[k], placement));
                }
                chain.into_iter().map(|(_, h)| h).collect()
            };
        for (begin, members, end) in &block_defs {
            let f = Fields::new(&begin.groups);
            let record = f.handle(330).unwrap_or(0);
            let begin_handle = f.handle(5).unwrap_or(0);
            let end_handle = end
                .and_then(|e| Fields::new(&e.groups).handle(5))
                .unwrap_or(0);
            note(Some(begin_handle));
            note(Some(end_handle));
            for member in members {
                note(Fields::new(&member.groups).handle(5));
            }
            let chain = place_chain(members, &mut entities);
            let block_place = Placement {
                entmode: entmode_of(record),
                owner: record,
                ..Placement::default()
            };
            entities.push((begin, block_place.clone()));
            if let Some(end) = end {
                entities.push((end, block_place));
            }
            blocks.insert(
                record,
                BlockPlacement {
                    flags: f.int(70).unwrap_or(0),
                    base: f.point(10).unwrap_or(Point2::ORIGIN),
                    base_z: f.z(10),
                    has_attributes: members.iter().any(|m| m.kind == "ATTDEF"),
                    begin: begin_handle,
                    end: end_handle,
                    members: chain,
                },
            );
        }
        for record in &entity_records {
            note(Fields::new(&record.groups).handle(5));
        }
        // The model's entities: one chain, whichever block they say they
        // are in (the exporter puts the model's alone here).
        let model_chain = place_chain(&entity_records, &mut entities);
        if let Some(model) = blocks.get_mut(&names.model_space) {
            model.members = model_chain;
        }
        for record in &objects {
            let f = Fields::new(&record.groups);
            note(f.handle(5));
            if record.kind == "DICTIONARY" && f.handle(330) == Some(0) && root_dictionary == 0 {
                root_dictionary = f.handle(5).unwrap_or(0);
                let mut name: Option<&str> = None;
                for (code, value) in &record.groups {
                    match code {
                        3 => name = Some(value.trim()),
                        350 | 360 => {
                            if let (Some(n), Ok(h)) =
                                (name.take(), u64::from_str_radix(value.trim(), 16))
                            {
                                root_entries.insert(n.to_uppercase(), h);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        let seed = header
            .get("$HANDSEED")
            .and_then(|g| Fields::new(g).handle(5))
            .unwrap_or(0)
            .max(max_handle + 1);
        let (vx_control, handseed) = if version == Version::R2000 {
            (Some(seed), seed + 1)
        } else {
            (None, seed)
        };
        Ok(Self {
            header,
            classes,
            tables,
            entities,
            objects,
            blocks,
            names,
            vx_control,
            handseed,
            root_dictionary,
            root_entries,
        })
    }

    /// How many objects each class has, in the classes' order (§10.2).
    fn instances(&self) -> Vec<u32> {
        let mut counts = vec![0u32; self.classes.len()];
        let kinds = self
            .entities
            .iter()
            .map(|(record, _)| record.kind.as_str())
            .chain(self.objects.iter().map(|record| record.kind.as_str()));
        for kind in kinds {
            if let Some(at) = self.classes.iter().position(|c| c.dxf_name == kind) {
                counts[at] += 1;
            }
        }
        counts
    }

    fn table_handle(&self, name: &str) -> u64 {
        self.tables
            .iter()
            .find(|t| t.name == name)
            .and_then(|t| Fields::new(&t.head.groups).handle(5))
            .unwrap_or(0)
    }

    /// The header variables the records give values for, over the
    /// defaults.
    fn header_values(&self) -> HashMap<&'static str, Value> {
        let names = &self.names;
        let var = |name: &str| self.header.get(name).map(|g| Fields::new(g));
        let mut out: HashMap<&'static str, Value> = HashMap::new();
        let handle = |code: u8, value: u64| Value::Handle(Handle { code, value });
        out.insert("HANDSEED", handle(0, self.handseed));
        let layer = var("$CLAYER")
            .and_then(|f| f.text(8))
            .and_then(|n| names.layers.get(&n.trim().to_uppercase()).copied())
            .unwrap_or(names.layer_zero);
        out.insert("CLAYER", handle(5, layer));
        let style = var("$TEXTSTYLE")
            .and_then(|f| f.text(7))
            .and_then(|n| names.text_styles.get(&n.trim().to_uppercase()).copied())
            .unwrap_or(names.standard_style);
        out.insert("TEXTSTYLE", handle(5, style));
        out.insert("DIMTXSTY", handle(5, names.standard_style));
        let linetype = var("$CELTYPE")
            .and_then(|f| f.text(6))
            .and_then(|n| names.linetypes.get(&n.trim().to_uppercase()).copied())
            .unwrap_or(names.ltype_bylayer);
        out.insert("CELTYPE", handle(5, linetype));
        let dim_style = var("$DIMSTYLE")
            .and_then(|f| f.text(2))
            .and_then(|n| names.dim_styles.get(&n.trim().to_uppercase()).copied())
            .unwrap_or(names.standard_dim_style);
        out.insert("DIMSTYLE", handle(5, dim_style));
        out.insert("CMLSTYLE", handle(5, 0));
        if let Some(index) = var("$CECOLOR").and_then(|f| f.int(62)) {
            out.insert(
                "CECOLOR",
                Value::Color(Color {
                    index: index.clamp(0, 256) as i16,
                    ..Color::default()
                }),
            );
        }
        let lineweight = var("$CELWEIGHT").and_then(|f| f.int(370)).unwrap_or(-1);
        out.insert(
            "FLAGS",
            Value::Int(0x2A00 | i64::from(lineweight_index(lineweight))),
        );
        if let Some(units) = var("$INSUNITS").and_then(|f| f.int(70)) {
            out.insert("INSUNITS", Value::Int(units));
        }
        if let Some(scale) = var("$LTSCALE").and_then(|f| f.real(40)) {
            out.insert("LTSCALE", Value::Real(scale));
        }
        if let Some(mode) = var("$PDMODE").and_then(|f| f.int(70)) {
            out.insert("PDMODE", Value::Int(mode));
        }
        if let Some(size) = var("$PDSIZE").and_then(|f| f.real(40)) {
            out.insert("PDSIZE", Value::Real(size));
        }
        for (variable, name) in [("$EXTMIN", "EXTMIN"), ("$EXTMAX", "EXTMAX")] {
            if let Some(f) = var(variable)
                && let Some(p) = f.point(10)
            {
                out.insert(name, Value::Point([p.x, p.y, f.z(10)]));
            }
        }
        for (table, name) in [
            ("BLOCK_RECORD", "BLOCK_CONTROL"),
            ("LAYER", "LAYER_CONTROL"),
            ("STYLE", "STYLE_CONTROL"),
            ("LTYPE", "LTYPE_CONTROL"),
            ("VIEW", "VIEW_CONTROL"),
            ("UCS", "UCS_CONTROL"),
            ("VPORT", "VPORT_CONTROL"),
            ("APPID", "APPID_CONTROL"),
            ("DIMSTYLE", "DIMSTYLE_CONTROL"),
        ] {
            out.insert(name, handle(3, self.table_handle(table)));
        }
        if let Some(vx_control) = self.vx_control {
            out.insert("VIEWPORT_ENTITY_HEADER_CONTROL", handle(3, vx_control));
            out.insert("CURRENT_VIEWPORT_ENTITY_HEADER", handle(5, 0));
        }
        out.insert("DICTIONARY_NAMED_OBJECTS", handle(3, self.root_dictionary));
        let entry = |name: &str| self.root_entries.get(name).copied().unwrap_or(0);
        out.insert("DICTIONARY_ACAD_GROUP", handle(5, entry("ACAD_GROUP")));
        out.insert(
            "DICTIONARY_ACAD_MLINESTYLE",
            handle(5, entry("ACAD_MLINESTYLE")),
        );
        out.insert("DICTIONARY_LAYOUTS", handle(5, entry("ACAD_LAYOUT")));
        out.insert(
            "DICTIONARY_PLOTSETTINGS",
            handle(5, entry("ACAD_PLOTSETTINGS")),
        );
        out.insert(
            "DICTIONARY_PLOTSTYLES",
            handle(5, entry("ACAD_PLOTSTYLENAME")),
        );
        out.insert("BLOCK_RECORD_PAPER_SPACE", handle(5, names.paper_space));
        out.insert("BLOCK_RECORD_MODEL_SPACE", handle(5, names.model_space));
        out.insert("LTYPE_BYLAYER", handle(5, names.ltype_bylayer));
        out.insert("LTYPE_BYBLOCK", handle(5, names.ltype_byblock));
        out.insert("LTYPE_CONTINUOUS", handle(5, names.ltype_continuous));
        out
    }

    /// MEASUREMENT (§22): metric when the drawing's unit is.
    fn measurement(&self) -> u16 {
        let units = self
            .header
            .get("$INSUNITS")
            .and_then(|g| Fields::new(g).int(70))
            .unwrap_or(0);
        u16::from(matches!(units, 4..=7 | 11..=20))
    }
}
