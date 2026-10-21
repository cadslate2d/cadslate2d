//! The drawing a DWG fixture makes is the drawing its DXF source makes
//! (ADR-0036 §2, #313): entity by entity, property by property, table row
//! by table row, in all three versions.

use std::fs;

use cad_document::{Drawing, Entity};

mod common;
use common::{describe, pairs, root};

/// What the independent writer cannot carry into a version, by fixture
/// and a substring of the lines it changes, dropped from both sides: a
/// loss of the fixture's making, not of this crate's reading.
const WRITER_LOSSES: &[(&str, &str)] = &[
    // AC1015 holds no true colour: the writer picks its own nearest index.
    ("ezdxf-r2010-true-colour.AC1015", "Index(13"),
    // AC1015 predates multileaders: the writer defines no MLEADERSTYLE
    // class there, so every style is lost — a drawing's `Standard` comes
    // back with the defaults — and a multileader falls back to it.
    (".AC1015", "multileader style"),
    ("ezdxf-r2010-multileaders.AC1015", "MLeader("),
    ("ezdxf-r2010-multileaders.AC1015", "With 2 layers,"),
    // The writer leaves a style's block scale at 1 (LibreDWG reads the
    // same), so the Balloon style's 1.5 is the DXF's alone.
    ("ezdxf-r2010-multileaders", "name: \"Balloon\""),
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
            .filter(|(fixture, _)| name.contains(fixture))
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
        let report_a: Vec<String> = from_dwg.report.summary().into_iter().filter(keep).collect();
        let report_b: Vec<String> = from_dxf.report.summary().into_iter().filter(keep).collect();
        if report_a != report_b {
            failures.push(format!(
                "{name}: report differs\n    dwg: {report_a:?}\n    dxf: {report_b:?}\n"
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// A drawing with two layouts and a viewport (#314): every version opens
/// to the same spaces, each with what the source put on it, and the
/// viewport on the first layout's paper with its view.
#[test]
fn two_layouts_and_a_viewport_open_to_the_same_spaces_in_every_version() {
    let dir = root().join("test-data/dwg/valid");
    for version in ["AC1015", "AC1018", "AC1021"] {
        let name = format!("layouts.{version}.dwg");
        let import = cad_dwg::import(&fs::read(dir.join(&name)).unwrap()).unwrap();
        let drawing = &import.drawing;
        let layouts: Vec<(cad_document::LayoutId, &str)> = drawing
            .layouts()
            .iter()
            .map(|(id, layout)| (id, layout.name.as_str()))
            .collect();
        let names: Vec<&str> = layouts.iter().map(|(_, name)| *name).collect();
        assert_eq!(names, ["Model", "Layout1", "Layout2"], "{name}");
        let count = |space| drawing.entities_in(space).count();
        assert_eq!(count(layouts[0].0), 1, "{name}: the model's line");
        assert_eq!(
            count(layouts[1].0),
            2,
            "{name}: Layout1's line and viewport"
        );
        assert_eq!(count(layouts[2].0), 1, "{name}: Layout2's line");
        let viewport = drawing
            .entities_in(layouts[1].0)
            .find_map(|(_, entity)| match entity {
                Entity::Viewport(viewport) => Some(*viewport),
                _ => None,
            })
            .unwrap_or_else(|| panic!("{name}: a viewport on Layout1"));
        assert_eq!(
            viewport.center,
            cad_core::Point2::new(148.5, 105.0),
            "{name}"
        );
        assert_eq!((viewport.width, viewport.height), (277.0, 190.0), "{name}");
        assert_eq!(
            viewport.view_center,
            cad_core::Point2::new(50.0, 25.0),
            "{name}"
        );
        assert!((viewport.scale - 190.0 / 200.0).abs() < 1e-9, "{name}");
        assert!(viewport.on, "{name}");
        assert_eq!(
            import.report.summary()[0],
            "Imported 3 lines, 1 viewport.",
            "{name}"
        );
    }
}

/// The draw order a `SORTENTSTABLE` gives (#314): the fixture's records
/// come in the tiers its source puts them in — the normalised source, as
/// the fixtures are written from (`examples/fixtures.rs`), whose records
/// are in paint order — in every version.
#[test]
fn the_draw_order_of_a_sortentstable_fixture_matches_its_source() {
    let dir = root().join("test-data/dwg/valid");
    let raw = cad_dxf::import(
        &fs::read(root().join("test-data/dxf/valid/ezdxf-r2010-draw-order.dxf")).unwrap(),
    )
    .unwrap();
    let source = cad_dxf::import(cad_dxf::export(&raw.drawing).as_bytes()).unwrap();
    let tiers = |drawing: &Drawing| -> Vec<(String, i16)> {
        drawing
            .records()
            .map(|(_, record)| {
                let kind = match &*record.entity {
                    Entity::Line(_) => "line",
                    Entity::Text(_) => "text",
                    Entity::Hatch(_) => "hatch",
                    Entity::Wipeout(_) => "wipeout",
                    other => panic!("{other:?}"),
                };
                (kind.to_owned(), record.order)
            })
            .collect()
    };
    let expected = tiers(&source.drawing);
    let orders: std::collections::HashSet<i16> = expected.iter().map(|(_, o)| *o).collect();
    assert!(orders.len() > 1, "the source has tiers: {expected:?}");
    for version in ["AC1015", "AC1018", "AC1021"] {
        let name = format!("ezdxf-r2010-draw-order.{version}.dwg");
        let import = cad_dwg::import(&fs::read(dir.join(&name)).unwrap()).unwrap();
        assert_eq!(tiers(&import.drawing), expected, "{name}");
    }
}

/// What this crate does not decode is reported by its kind, never guessed
/// at: a proxy entity, an entity of a fixed type it does not read (an
/// MLINE), and an object of a class the drawing never defined (fixtures
/// crafted from the draw-order one by rewriting an entity's type and its
/// CRC).
#[test]
fn a_proxy_entity_and_an_unknown_class_are_reported_not_guessed() {
    let dir = root().join("test-data/dwg/valid");
    let read = |name: &str| cad_dwg::import(&fs::read(dir.join(name)).unwrap()).unwrap();
    let mline = read("crafted-mline-entity.AC1015.dwg");
    let lines = mline.report.summary().join("\n");
    assert!(lines.contains("1 MLINE"), "{lines}");
    assert!(
        lines.contains("Imported 2 lines, 2 texts, 1 hatch."),
        "{lines}"
    );
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
