//! The drawing a DWG fixture makes is the drawing its DXF source makes
//! (ADR-0036 §2, #313): entity by entity, property by property, table row
//! by table row, in all three versions.

use std::fs;
use std::path::{Path, PathBuf};

use cad_document::{Drawing, Entity, Linetype};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Every valid DWG fixture with the DXF it was written from.
fn pairs() -> Vec<(String, Vec<u8>, Vec<u8>)> {
    let dir = root().join("test-data/dwg/valid");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dwg"))
        .filter(|p| {
            !p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("crafted-"))
        })
        .collect();
    files.sort();
    assert!(!files.is_empty());
    files
        .into_iter()
        .map(|path| {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            let source = name.split('.').next().unwrap();
            let dxf = root()
                .join("test-data/dxf/valid")
                .join(format!("{source}.dxf"));
            (name, fs::read(&path).unwrap(), fs::read(&dxf).unwrap())
        })
        .collect()
}

/// Floats to six decimals, so that a degree that went through radians
/// and back compares equal.
fn rounded(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(|c: char| c.is_ascii_digit() || c == '-') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest
            .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e'))
            .unwrap_or(rest.len());
        let token = &rest[..end];
        match token.parse::<f64>() {
            Ok(value) if token.contains('.') || token.contains('e') => {
                let r = (value * 1e6).round() / 1e6;
                out.push_str(&format!("{r}"));
            }
            _ => out.push_str(token),
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// A drawing as lines of text: its settings, its tables by name, its
/// blocks, and its entities in order, every id resolved to a name.
fn describe(drawing: &Drawing) -> Vec<String> {
    let mut lines = Vec::new();
    let layer_name = |id| {
        drawing
            .layers()
            .get(id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    let linetype_name = |linetype: Linetype| match linetype {
        Linetype::ByLayer => "ByLayer".to_owned(),
        Linetype::ByBlock => "ByBlock".to_owned(),
        Linetype::Named(id) => drawing
            .linetypes()
            .get(id)
            .map(|l| l.name.clone())
            .unwrap_or_default(),
    };
    let describe_entity = |entity: &Entity| -> String {
        let mut text = format!("{entity:?}");
        // Ids inside an entity — a reference's block, a text's style —
        // named instead, since the two imports number rows differently.
        for (id, style) in drawing.text_styles().iter() {
            text = text.replace(
                &format!("style: {id:?},"),
                &format!("style: {},", style.name),
            );
        }
        for (id, block) in drawing.blocks().iter() {
            text = text.replace(
                &format!("block: {id:?},"),
                &format!("block: {},", block.name),
            );
        }
        rounded(&text)
    };
    // The current layer and linetype are left out: the independent writer
    // of the fixtures does not carry them into a DWG.
    lines.push(format!(
        "settings unit={:?} color={:?} lineweight={:?} ltscale={}",
        drawing.unit(),
        drawing.current_color(),
        drawing.current_lineweight(),
        drawing.ltscale()
    ));
    let mut layers: Vec<String> = drawing
        .layers()
        .iter()
        .map(|(_, l)| {
            format!(
                "layer {} color={:?} linetype={} lineweight={:?} on={} frozen={} locked={}",
                l.name,
                l.color,
                linetype_name(l.linetype),
                l.lineweight,
                l.on,
                l.frozen,
                l.locked
            )
        })
        .collect();
    layers.sort();
    lines.extend(layers);
    let mut linetypes: Vec<String> = drawing
        .linetypes()
        .iter()
        .map(|(_, l)| {
            rounded(&format!(
                "linetype {} {:?} {:?}",
                l.name, l.description, l.pattern
            ))
        })
        .collect();
    linetypes.sort();
    lines.extend(linetypes);
    let mut styles: Vec<String> = drawing
        .text_styles()
        .iter()
        .map(|(_, s)| rounded(&format!("text style {s:?}")))
        .collect();
    styles.sort();
    lines.extend(styles);
    let mut dims: Vec<String> = drawing
        .dim_styles()
        .iter()
        .map(|(_, s)| rounded(&format!("dim style {s:?}")))
        .collect();
    dims.sort();
    lines.extend(dims);
    let mut blocks: Vec<String> = drawing
        .blocks()
        .iter()
        .map(|(_, b)| {
            let entities: Vec<String> = b
                .entities
                .iter()
                .map(|r| {
                    format!(
                        "{} layer={} color={:?} linetype={} lineweight={:?}",
                        describe_entity(&r.entity),
                        layer_name(r.properties.layer),
                        r.properties.color,
                        linetype_name(r.properties.linetype),
                        r.properties.lineweight
                    )
                })
                .collect();
            rounded(&format!(
                "block {} base={:?} [{}]",
                b.name,
                b.base,
                entities.join("; ")
            ))
        })
        .collect();
    blocks.sort();
    lines.extend(blocks);
    for (_, record) in drawing.records() {
        lines.push(format!(
            "entity {} layer={} color={:?} linetype={} lineweight={:?} space={:?}",
            describe_entity(&record.entity),
            layer_name(record.properties.layer),
            record.properties.color,
            linetype_name(record.properties.linetype),
            record.properties.lineweight,
            record.space
        ));
    }
    lines
}

/// What the independent writer cannot carry into a version, by fixture
/// and a substring of the lines it changes, dropped from both sides: a
/// loss of the fixture's making, not of this crate's reading.
const WRITER_LOSSES: &[(&str, &str)] = &[
    // AC1015 holds no true colour: the writer picks its own nearest index.
    ("ezdxf-r2010-true-colour.AC1015", "Index(13"),
];

#[test]
fn every_fixture_makes_the_drawing_its_dxf_source_makes() {
    let mut failures = Vec::new();
    for (name, dwg, dxf) in pairs() {
        let from_dwg = cad_dwg::import(&dwg).unwrap_or_else(|e| panic!("{name}: {e}"));
        // The drawing the fixture was written from: the source as this
        // project reads it, written back and read again, which is what
        // the generator handed the independent writer (see
        // `examples/fixtures.rs`).
        let source = cad_dxf::import(&dxf).unwrap();
        let normalised = cad_dxf::export(&source.drawing);
        let from_dxf = cad_dxf::import(normalised.as_bytes()).unwrap();
        let losses: Vec<&str> = WRITER_LOSSES
            .iter()
            .filter(|(fixture, _)| name.starts_with(fixture))
            .map(|(_, pattern)| *pattern)
            .collect();
        let keep = |line: &String| !losses.iter().any(|pattern| line.contains(pattern));
        let a: Vec<String> = describe(&from_dwg.drawing)
            .into_iter()
            .filter(keep)
            .collect();
        let b: Vec<String> = describe(&from_dxf.drawing)
            .into_iter()
            .filter(keep)
            .collect();
        if a != b {
            let mut diff = String::new();
            for (i, (x, y)) in a.iter().zip(&b).enumerate() {
                if x != y {
                    diff.push_str(&format!("  line {i}:\n    dwg: {x}\n    dxf: {y}\n"));
                }
            }
            if a.len() != b.len() {
                diff.push_str(&format!("  dwg has {} lines, dxf {}\n", a.len(), b.len()));
                for x in a.iter().skip(b.len()) {
                    diff.push_str(&format!("    dwg only: {x}\n"));
                }
                for y in b.iter().skip(a.len()) {
                    diff.push_str(&format!("    dxf only: {y}\n"));
                }
            }
            failures.push(format!("{name}:\n{diff}"));
        }
        let report_a = from_dwg.report.summary();
        let report_b = from_dxf.report.summary();
        if report_a != report_b {
            failures.push(format!(
                "{name}: report differs\n    dwg: {report_a:?}\n    dxf: {report_b:?}\n"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// What this crate does not decode is reported by its kind, never guessed
/// at: a proxy entity, and an object of a class the drawing never defined
/// (two fixtures crafted from the draw-order one by rewriting an entity's
/// type and its CRC).
#[test]
fn a_proxy_entity_and_an_unknown_class_are_reported_not_guessed() {
    let dir = root().join("test-data/dwg/valid");
    let read = |name: &str| cad_dwg::import(&fs::read(dir.join(name)).unwrap()).unwrap();
    let proxy = read("crafted-proxy-entity.AC1015.dwg");
    let lines = proxy.report.summary().join("\n");
    assert!(lines.contains("1 ACAD_PROXY_ENTITY"), "{lines}");
    assert!(
        lines.contains("Imported 2 lines, 2 texts, 1 hatch."),
        "{lines}"
    );
    let unknown = read("crafted-unknown-class.AC1015.dwg");
    let lines = unknown.report.summary().join("\n");
    assert!(lines.contains("1 (UNKNOWN-CLASS-999)"), "{lines}");
    assert!(
        lines.contains("Imported 2 lines, 2 texts, 1 hatch."),
        "{lines}"
    );
    // The fixture they were crafted from holds the wipeout they lost.
    let original = read("ezdxf-r2010-draw-order.AC1015.dwg");
    let lines = original.report.summary().join("\n");
    assert!(lines.contains("1 wipeout"), "{lines}");
}
