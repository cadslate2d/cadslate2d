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
    let mut mleader_styles: Vec<String> = drawing
        .mleader_styles()
        .iter()
        .map(|(_, s)| rounded(&format!("multileader style {s:?}")))
        .collect();
    mleader_styles.sort();
    lines.extend(mleader_styles);
    let layout_name = |id| {
        drawing
            .layouts()
            .get(id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    // A layout: its name, sheet, plot, and the file's block — not the
    // kept groups, which each reader keeps in its own form.
    for (_, layout) in drawing.layouts().iter() {
        lines.push(rounded(&format!(
            "layout {} paper={:?} plot={:?} block={} kept_entities={}",
            layout.name,
            layout.paper_size,
            layout.plot,
            layout.block_name,
            layout.kept_entities.len()
        )));
    }
    let mut views: Vec<String> = drawing
        .views()
        .iter()
        .map(|(_, v)| {
            rounded(&format!(
                "view {} space={} center={:?} width={} height={}",
                v.name,
                layout_name(v.space),
                v.center,
                v.width,
                v.height
            ))
        })
        .collect();
    views.sort();
    lines.extend(views);
    let mut states: Vec<String> = drawing
        .layer_states()
        .iter()
        .map(|(_, s)| rounded(&format!("layer state {s:?}")))
        .collect();
    states.sort();
    lines.extend(states);
    // A group's members by their place among the records.
    let places: Vec<_> = drawing.records().map(|(id, _)| id).collect();
    let mut groups: Vec<String> = drawing
        .groups()
        .iter()
        .map(|(_, g)| {
            let members: Vec<String> = g
                .members
                .iter()
                .map(|id| {
                    places
                        .iter()
                        .position(|p| p == id)
                        .map_or_else(|| "?".to_owned(), |at| at.to_string())
                })
                .collect();
            format!(
                "group {} description={:?} selectable={} members={}",
                g.name,
                g.description,
                g.selectable,
                members.join(",")
            )
        })
        .collect();
    groups.sort();
    lines.extend(groups);
    for (_, record) in drawing.records() {
        lines.push(format!(
            "entity {} layer={} color={:?} linetype={} lineweight={:?} space={} order={}",
            describe_entity(&record.entity),
            layer_name(record.properties.layer),
            record.properties.color,
            linetype_name(record.properties.linetype),
            record.properties.lineweight,
            layout_name(record.space),
            record.order
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
