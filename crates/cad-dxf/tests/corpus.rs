//! The round-trip corpus (DESIGN §41): every `.dxf` under
//! `test-data/dxf/corpus/` with its `.expect` sidecar. A case is two files;
//! a failure is reported by file and line; every failing line is listed.
//!
//! Always checked, with no line needed: the file imports; our export of it
//! imports again with nothing unsupported; and export → import → export is
//! byte-identical. A sidecar line is one of (a `#` starts a comment):
//!
//! ```text
//! imported lines 3                 lines|circles|arcs|polylines|texts|block_references|dimensions|hatches
//! tables layers 2                  layers|linetypes|text_styles|dim_styles|blocks, besides the permanent rows
//! created layers 1                 created_layers|created_linetypes|created_text_styles|created_dim_styles
//! unsupported HATCH 1              a report key, quoted when it has spaces
//! dropped 2                        entities whose properties were dropped
//! formatting-dropped 1             paragraphs whose formatting was dropped
//! unit mm                          none|in|ft|mm|cm|m
//! entities 7                       in the drawing
//! line 0 0 0 100 0                 the i-th LINE in file order: x0 y0 x1 y1
//! circle 0 50 50 25                the i-th CIRCLE: cx cy r
//! arc 0 0 0 30 0 90                the i-th ARC: cx cy r start° end°
//! polyline 1 closed 4              the i-th polyline: open|closed, vertices
//! bulge 2 0 1                      polyline i, vertex j, bulge
//! text 0 "Left" 2.5                the i-th TEXT: words, height
//! mtext 0 "a\nb" 60                the i-th MTEXT: words (\n a break), width
//! reference 2 CHAIR 0 50 1 180 mirrored   the i-th INSERT: block x y scale rotation° mirrored|plain
//! dimension 0 linear 100           the i-th DIMENSION: linear|aligned|angular|radius|diameter, its value
//! hatch 0 ANSI31 2                 the i-th HATCH: its pattern (SOLID for a solid fill), its loops
//! block CHAIR 2                    entities inside the definition
//! layer WALLS colour 1 on locked linetype DASHED lineweight 50
//! entity 1 layer HIDDEN colour 5 linetype DASHED lineweight 35
//! current layer PIPES / current colour 2 / current linetype DOT / current lineweight 25
//! ltscale 5
//! ```
//!
//! Colours are `bylayer`, `byblock`, or an index; lineweights `bylayer`,
//! `byblock`, `default`, or hundredths; linetypes `bylayer`, `byblock`, or
//! a name. Numbers are compared to 1e-9.

use std::fs;
use std::path::{Path, PathBuf};

use cad_core::LengthUnit;
use cad_document::{Color, Drawing, Entity, Linetype, Lineweight};
use cad_dxf::{Import, export, import};

fn corpus() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/dxf/corpus")
}

struct Failures(Vec<String>);

impl Failures {
    fn note(&mut self, file: &str, line: usize, message: impl Into<String>) {
        self.0.push(format!("{file}:{line}: {}", message.into()));
    }
}

#[test]
fn every_corpus_file_imports_as_its_sidecar_says_and_round_trips() {
    let mut files: Vec<PathBuf> = fs::read_dir(corpus())
        .expect("the corpus directory")
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| e == "dxf" || e == "expect")
        })
        .collect();
    files.sort();
    let mut failures = Failures(Vec::new());
    let mut cases = 0;
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let partner = if path.extension().unwrap() == "dxf" {
            path.with_extension("expect")
        } else {
            path.with_extension("dxf")
        };
        if !partner.is_file() {
            failures.note(&name, 0, "has no partner: a case is a .dxf and a .expect");
            continue;
        }
        if path.extension().unwrap() != "dxf" {
            continue;
        }
        cases += 1;
        check(path, &partner, &mut failures);
    }
    assert!(cases >= 40, "{cases} corpus cases");
    assert!(
        failures.0.is_empty(),
        "{} corpus failures:\n{}",
        failures.0.len(),
        failures.0.join("\n")
    );
}

fn check(dxf: &Path, expect: &Path, failures: &mut Failures) {
    let name = dxf.file_name().unwrap().to_string_lossy().into_owned();
    let bytes = fs::read(dxf).unwrap();
    let imported = match import(&bytes) {
        Ok(imported) => imported,
        Err(error) => return failures.note(&name, 0, format!("does not import: {error}")),
    };
    for (index, line) in fs::read_to_string(expect).unwrap().lines().enumerate() {
        let line = line.split('#').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        if let Err(message) = expectation(&imported, line) {
            failures.note(&name, index + 1, format!("{message}\n    {line}"));
        }
    }
    // Our own export holds everything the drawing does, and is stable.
    let first = export(&imported.drawing);
    match import(first.as_bytes()) {
        Err(error) => failures.note(&name, 0, format!("our export does not import: {error}")),
        Ok(again) => {
            if again.report.has_unsupported_content() {
                failures.note(
                    &name,
                    0,
                    format!("our export is not clean: {:?}", again.report.summary()),
                );
            }
            if export(&again.drawing) != first {
                failures.note(&name, 0, "export → import → export is not byte-identical");
            }
        }
    }
}

/// The words of a line: a quoted word may hold spaces and `\n`.
fn words(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = line.trim();
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix('"') {
            let end = after.find('"').unwrap_or(after.len());
            out.push(after[..end].replace("\\n", "\n"));
            rest = after[end..].strip_prefix('"').unwrap_or("").trim_start();
        } else {
            let end = rest.find(' ').unwrap_or(rest.len());
            out.push(rest[..end].to_owned());
            rest = rest[end..].trim_start();
        }
    }
    out
}

fn number(word: &str) -> Result<f64, String> {
    word.parse().map_err(|_| format!("not a number: {word}"))
}

fn index(word: &str) -> Result<usize, String> {
    word.parse().map_err(|_| format!("not an index: {word}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

/// Equal when written out the same way: a count, a word, a name.
fn same(what: &str, got: impl std::fmt::Debug, want: impl std::fmt::Debug) -> Result<(), String> {
    let (got, want) = (format!("{got:?}"), format!("{want:?}"));
    if got == want {
        Ok(())
    } else {
        Err(format!("{what}: got {got}, expected {want}"))
    }
}

fn in_file_order(drawing: &Drawing) -> Vec<(Entity, cad_document::Properties)> {
    let mut records: Vec<_> = drawing.records().collect();
    records.sort_by_key(|(id, _)| *id);
    records
        .into_iter()
        .map(|(_, record)| ((*record.entity).clone(), record.properties))
        .collect()
}

fn nth<'a, T>(items: &'a [T], word: &str, what: &str) -> Result<&'a T, String> {
    let i = index(word)?;
    items
        .get(i)
        .ok_or_else(|| format!("no {what} {i}: there are {}", items.len()))
}

fn colour_word(colour: Color) -> String {
    match colour {
        Color::ByLayer => "bylayer".into(),
        Color::ByBlock => "byblock".into(),
        Color::Index(index) => index.to_string(),
    }
}

fn lineweight_word(lineweight: Lineweight) -> String {
    match lineweight {
        Lineweight::ByLayer => "bylayer".into(),
        Lineweight::ByBlock => "byblock".into(),
        Lineweight::Default => "default".into(),
        Lineweight::Hundredths(n) => n.to_string(),
    }
}

/// Upper case, like the names it is compared with.
fn linetype_word(linetype: Linetype, drawing: &Drawing) -> String {
    match linetype {
        Linetype::ByLayer => "BYLAYER".into(),
        Linetype::ByBlock => "BYBLOCK".into(),
        Linetype::Named(id) => drawing
            .linetypes()
            .get(id)
            .map_or_else(|| "?".into(), |def| def.name.to_uppercase()),
    }
}

fn unit_word(unit: Option<LengthUnit>) -> &'static str {
    match unit {
        None => "none",
        Some(LengthUnit::Inches) => "in",
        Some(LengthUnit::Feet) => "ft",
        Some(LengthUnit::Millimetres) => "mm",
        Some(LengthUnit::Centimetres) => "cm",
        Some(LengthUnit::Metres) => "m",
    }
}

fn expectation(imported: &Import, line: &str) -> Result<(), String> {
    let words = words(line);
    let w = |i: usize| -> Result<&str, String> {
        words
            .get(i)
            .map(String::as_str)
            .ok_or_else(|| format!("missing word {i}"))
    };
    let drawing = &imported.drawing;
    let report = &imported.report;
    let entities = in_file_order(drawing);
    let of_kind = |kind: &str| -> Vec<Entity> {
        entities
            .iter()
            .filter(|(entity, _)| entity.kind_name() == kind)
            .map(|(entity, _)| entity.clone())
            .collect()
    };
    let numbers = |from: usize, want: &[f64]| -> Result<(), String> {
        for (k, expected) in want.iter().enumerate() {
            let got = number(w(from + k)?)?;
            if !close(got, *expected) {
                return Err(format!("value {} is {expected}, expected {got}", k + 1));
            }
        }
        Ok(())
    };
    match w(0)? {
        "imported" => {
            let got = match w(1)? {
                "lines" => report.lines,
                "circles" => report.circles,
                "arcs" => report.arcs,
                "ellipses" => report.ellipses,
                "splines" => report.splines,
                "polylines" => report.polylines,
                "texts" => report.texts,
                "block_references" => report.block_references,
                "dimensions" => report.dimensions,
                "hatches" => report.hatches,
                other => return Err(format!("unknown kind {other}")),
            };
            same("count", got, index(w(2)?)?)
        }
        "tables" => {
            let got = match w(1)? {
                "layers" => report.layers,
                "linetypes" => report.linetypes,
                "text_styles" => report.text_styles,
                "dim_styles" => report.dim_styles,
                "blocks" => report.blocks,
                other => return Err(format!("unknown table {other}")),
            };
            same("count", got, index(w(2)?)?)
        }
        "created" => {
            let got = match w(1)? {
                "layers" => report.created_layers,
                "linetypes" => report.created_linetypes,
                "text_styles" => report.created_text_styles,
                "dim_styles" => report.created_dim_styles,
                other => return Err(format!("unknown table {other}")),
            };
            same("count", got, index(w(2)?)?)
        }
        "unsupported" => same(
            "count",
            report.unsupported.get(w(1)?).copied().unwrap_or(0),
            index(w(2)?)?,
        ),
        "dropped" => same("count", report.with_dropped_properties, index(w(1)?)?),
        "formatting-dropped" => same("count", report.formatting_dropped, index(w(1)?)?),
        "unit" => same("unit", unit_word(report.unit), w(1)?),
        "entities" => same("count", drawing.len(), index(w(1)?)?),
        "dimension" => {
            let dimensions = of_kind("dimension");
            let Entity::Dimension(dimension) = nth(&dimensions, w(1)?, "dimension")? else {
                unreachable!()
            };
            same("kind", dimension.measure.kind(), w(2)?)?;
            numbers(3, &[dimension.value()])
        }
        "hatch" => {
            let hatches = of_kind("hatch");
            let Entity::Hatch(hatch) = nth(&hatches, w(1)?, "hatch")? else {
                unreachable!()
            };
            same("pattern", hatch.style.name(), w(2)?)?;
            same("loops", hatch.loops.len(), index(w(3)?)?)
        }
        "line" => {
            let lines = of_kind("line");
            let Entity::Line(line) = nth(&lines, w(1)?, "line")? else {
                unreachable!()
            };
            numbers(2, &[line.start.x, line.start.y, line.end.x, line.end.y])
        }
        "circle" => {
            let circles = of_kind("circle");
            let Entity::Circle(c) = nth(&circles, w(1)?, "circle")? else {
                unreachable!()
            };
            numbers(2, &[c.center.x, c.center.y, c.radius])
        }
        "arc" => {
            let arcs = of_kind("arc");
            let Entity::Arc(a) = nth(&arcs, w(1)?, "arc")? else {
                unreachable!()
            };
            numbers(
                2,
                &[
                    a.center.x,
                    a.center.y,
                    a.radius,
                    a.start_angle.to_degrees(),
                    a.end_angle.to_degrees(),
                ],
            )
        }
        "polyline" => {
            let polylines = of_kind("polyline");
            let Entity::Polyline(p) = nth(&polylines, w(1)?, "polyline")? else {
                unreachable!()
            };
            same("closed", if p.closed { "closed" } else { "open" }, w(2)?)?;
            same("vertices", p.vertices.len(), index(w(3)?)?)
        }
        "bulge" => {
            let polylines = of_kind("polyline");
            let Entity::Polyline(p) = nth(&polylines, w(1)?, "polyline")? else {
                unreachable!()
            };
            let vertex = index(w(2)?)?;
            if vertex >= p.vertices.len() {
                return Err(format!("no vertex {vertex}"));
            }
            numbers(3, &[p.bulge(vertex)])
        }
        "text" => {
            let texts = of_kind("text");
            let Entity::Text(t) = nth(&texts, w(1)?, "text")? else {
                unreachable!()
            };
            same("words", t.content.as_str(), w(2)?)?;
            numbers(3, &[t.height])
        }
        "mtext" => {
            let texts = of_kind("multiline text");
            let Entity::MText(t) = nth(&texts, w(1)?, "paragraph")? else {
                unreachable!()
            };
            same("words", t.content.as_str(), w(2)?)?;
            numbers(3, &[t.width])
        }
        // `attribute i TAG VALUE`: the i-th reference's value for TAG.
        "attribute" => {
            let refs = of_kind("block reference");
            let Entity::BlockRef(r) = nth(&refs, w(1)?, "reference")? else {
                unreachable!()
            };
            same("attribute", r.attribute(w(2)?).unwrap_or("?"), w(3)?)
        }
        "reference" => {
            let refs = of_kind("block reference");
            let Entity::BlockRef(r) = nth(&refs, w(1)?, "reference")? else {
                unreachable!()
            };
            let block = drawing.blocks().get(r.block).map(|b| b.name.to_uppercase());
            same(
                "block",
                block.as_deref().unwrap_or("?"),
                w(2)?.to_uppercase().as_str(),
            )?;
            numbers(
                3,
                &[
                    r.insertion.x,
                    r.insertion.y,
                    r.scale,
                    r.rotation.to_degrees(),
                ],
            )?;
            same(
                "mirrored",
                if r.mirrored { "mirrored" } else { "plain" },
                w(7)?,
            )
        }
        "block" => {
            let name = w(1)?;
            let id = drawing
                .blocks()
                .find(name)
                .ok_or_else(|| format!("no block {name}"))?;
            same(
                "entities",
                drawing.blocks().get(id).unwrap().entities.len(),
                index(w(2)?)?,
            )
        }
        "layer" => {
            let name = w(1)?;
            let id = drawing
                .layers()
                .find(name)
                .ok_or_else(|| format!("no layer {name}"))?;
            let layer = drawing.layers().get(id).unwrap();
            same("colour", colour_word(layer.color), w(3)?)?;
            same("on", if layer.on { "on" } else { "off" }, w(4)?)?;
            same(
                "locked",
                if layer.locked { "locked" } else { "unlocked" },
                w(5)?,
            )?;
            same(
                "linetype",
                linetype_word(layer.linetype, drawing).as_str(),
                w(7)?.to_uppercase().as_str(),
            )?;
            same(
                "lineweight",
                lineweight_word(layer.lineweight).as_str(),
                w(9)?,
            )
        }
        "entity" => {
            let (_, properties) = nth(&entities, w(1)?, "entity")?;
            let layer = drawing
                .layers()
                .get(properties.layer)
                .map_or("?".to_owned(), |l| l.name.to_uppercase());
            same("layer", layer.as_str(), w(3)?.to_uppercase().as_str())?;
            same("colour", colour_word(properties.color).as_str(), w(5)?)?;
            same(
                "linetype",
                linetype_word(properties.linetype, drawing).as_str(),
                w(7)?.to_uppercase().as_str(),
            )?;
            same(
                "lineweight",
                lineweight_word(properties.lineweight).as_str(),
                w(9)?,
            )
        }
        "current" => {
            let settings = drawing.settings();
            match w(1)? {
                "layer" => {
                    let name = drawing
                        .layers()
                        .get(settings.current_layer)
                        .map_or("?".to_owned(), |l| l.name.to_uppercase());
                    same(
                        "current layer",
                        name.as_str(),
                        w(2)?.to_uppercase().as_str(),
                    )
                }
                "colour" => same(
                    "current colour",
                    colour_word(settings.current_color).as_str(),
                    w(2)?,
                ),
                "linetype" => same(
                    "current linetype",
                    linetype_word(settings.current_linetype, drawing).as_str(),
                    w(2)?.to_uppercase().as_str(),
                ),
                "lineweight" => same(
                    "current lineweight",
                    lineweight_word(settings.current_lineweight).as_str(),
                    w(2)?,
                ),
                other => Err(format!("unknown setting {other}")),
            }
        }
        "ltscale" => numbers(1, &[drawing.settings().ltscale]),
        other => Err(format!("unknown expectation {other}")),
    }
}
