//! Fixture-driven tests (DESIGN §41). See `test-data/dxf/README.md`.

use std::fs;
use std::path::{Path, PathBuf};

use cad_core::Point2;
use cad_document::{
    BlockReferenceEntity, Color, DimStyleId, Drawing, Entity, Justification, Layer, LayerId,
    LineEntity, Linetype, LinetypeDef, LinetypeId, Lineweight, MTextEntity, Properties, TextEntity,
    TextStyleId,
};
use cad_dxf::{DxfError, Import, export, import};

fn fixtures(kind: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/dxf")
        .join(kind)
}

/// Every `.dxf` file in a fixture directory, sorted for stable output.
fn each(kind: &str) -> Vec<(String, Vec<u8>)> {
    let mut files: Vec<(String, Vec<u8>)> = fs::read_dir(fixtures(kind))
        .unwrap_or_else(|error| panic!("cannot list {kind} fixtures: {error}"))
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "dxf"))
        .map(|path| {
            (
                path.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&path).unwrap(),
            )
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no {kind} fixtures found");
    files
}

fn load(kind: &str, name: &str) -> Result<Import, DxfError> {
    import(&fs::read(fixtures(kind).join(name)).unwrap())
}

/// Lines in file order (entity IDs are allocated in the order entities are read).
fn lines(drawing: &Drawing) -> Vec<LineEntity> {
    in_file_order(drawing)
        .iter()
        .filter_map(|entity| entity.as_line().copied())
        .collect()
}

fn in_file_order(drawing: &Drawing) -> Vec<Entity> {
    let mut entities: Vec<_> = drawing
        .entities()
        .map(|(id, entity)| (id, entity.clone()))
        .collect();
    entities.sort_by_key(|(id, _)| *id);
    entities.into_iter().map(|(_, entity)| entity).collect()
}

fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> LineEntity {
    LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1))
}

// ── directory-wide contracts ──

#[test]
fn every_valid_fixture_imports() {
    for (name, bytes) in each("valid") {
        if let Err(error) = import(&bytes) {
            panic!("{name}: {error}");
        }
    }
}

#[test]
fn every_valid_fixture_round_trips_stably() {
    for (name, bytes) in each("valid") {
        let first = import(&bytes).unwrap();
        let exported = export(&first.drawing);
        let second = import(exported.as_bytes())
            .unwrap_or_else(|error| panic!("{name}: re-import failed: {error}"));

        assert_eq!(
            lines(&second.drawing),
            lines(&first.drawing),
            "{name}: lines changed"
        );
        assert_eq!(
            polylines(&second.drawing),
            polylines(&first.drawing),
            "{name}: polylines changed"
        );
        assert_eq!(
            second.drawing.len(),
            first.drawing.len(),
            "{name}: entity count changed"
        );
        assert_eq!(
            paragraphs(&second.drawing),
            paragraphs(&first.drawing),
            "{name}: paragraphs changed"
        );
        assert_eq!(
            export(&second.drawing),
            exported,
            "{name}: second export differs"
        );
        assert!(
            !second.report.has_unsupported_content(),
            "{name}: our own export must be clean"
        );
    }
}

/// ADR-0024: the export in slices, however small the slices, is the export.
#[test]
fn every_valid_fixture_exports_the_same_in_slices() {
    for (name, bytes) in each("valid") {
        let drawing = import(&bytes).unwrap().drawing;
        let mut job = cad_dxf::Export::begin(&drawing);
        let mut steps = 0;
        while !job.step(&drawing, 3).unwrap() {
            steps += 1;
        }
        assert_eq!(job.finish(&drawing).unwrap(), export(&drawing), "{name}");
        // Three records fit one step; the transition after them costs none.
        assert!(steps > 0 || drawing.len() <= 3, "{name}: {steps} steps");
    }
}

#[test]
fn a_change_ends_an_export_in_slices() {
    let mut drawing = import(&fs::read(fixtures("valid").join("minimal-line.dxf")).unwrap())
        .unwrap()
        .drawing;
    let mut job = cad_dxf::Export::begin(&drawing);
    let mut tx = drawing.transaction();
    tx.insert(line(0.0, 0.0, 1.0, 1.0)).unwrap();
    tx.commit();
    assert_eq!(job.step(&drawing, 1), Err(cad_dxf::DrawingChanged));
    assert_eq!(
        cad_dxf::Export::begin(&drawing).finish(&drawing).unwrap(),
        export(&drawing),
        "begun afresh, it is the drawing as it is now"
    );
}

#[test]
fn every_malformed_fixture_is_rejected_without_panicking() {
    for (name, bytes) in each("malformed") {
        assert!(import(&bytes).is_err(), "{name} should be rejected");
    }
}

/// `docs/dxf-matrix.md` names, in backticks, the tests (`a_name()`) and
/// fixtures (`a-file.dxf`) that back each row. A row cannot outlive what
/// backs it: every name must exist.
#[test]
fn the_matrix_names_only_tests_and_fixtures_that_exist() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let matrix = fs::read_to_string(root.join("docs/dxf-matrix.md")).unwrap();
    let sources: String = [
        "src/import.rs",
        "src/export.rs",
        "src/report.rs",
        "tests/fixtures.rs",
        "tests/corpus.rs",
    ]
    .iter()
    .map(|file| fs::read_to_string(root.join("crates/cad-dxf").join(file)).unwrap())
    .collect();
    let mut checked = 0;
    for name in matrix.split('`').skip(1).step_by(2) {
        if let Some(test) = name.strip_suffix("()").filter(|test| !test.is_empty()) {
            assert!(
                sources.contains(&format!("fn {test}(")),
                "the matrix names a test that does not exist: {test}"
            );
            checked += 1;
        } else if name.ends_with(".dxf") {
            let exists = ["valid", "malformed"]
                .iter()
                .any(|kind| fixtures(kind).join(name).is_file());
            assert!(
                exists,
                "the matrix names a fixture that does not exist: {name}"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 60,
        "the matrix names {checked} tests and fixtures"
    );
}

// ── specific fixtures ──

#[test]
fn minimal_line() {
    let import = load("valid", "minimal-line.dxf").unwrap();
    assert_eq!(lines(&import.drawing), vec![line(0.0, 0.0, 100.0, 50.0)]);
    assert!(!import.report.has_unsupported_content());
}

/// A fuzzer finding (#177): every group value was trimmed, so a text
/// ending in a space lost it, and an MTEXT written in pieces fused the
/// words on either side of a piece boundary.
#[test]
fn text_keeps_the_spaces_at_its_ends() {
    let import = load("valid", "text-with-spaces.dxf").unwrap();
    let contents: Vec<String> = in_file_order(&import.drawing)
        .iter()
        .map(|entity| match entity {
            Entity::Text(text) => text.content.clone(),
            Entity::MText(text) => text.content.clone(),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(contents, vec!["  two  spaces  ", "Bold and italic"]);
}

/// A fuzzer finding (#177): a pattern line placed at y = 1e20, so far from
/// the region against its spacing that the pattern's lines could not be
/// told apart, and the walk over them never ended. The hatch is reported
/// as too dense and kept solid, like any pattern too fine to draw.
#[test]
fn a_hatch_pattern_placed_too_far_to_draw_is_reported() {
    let import = load("valid", "hatch-pattern-origin-far.dxf").unwrap();
    let hatches: Vec<cad_document::HatchEntity> = in_file_order(&import.drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .collect();
    assert_eq!(hatches.len(), 1);
    assert_eq!(hatches[0].style, cad_document::HatchStyle::Solid);
    assert_eq!(
        import.report.unsupported.get("(HATCH pattern too dense)"),
        Some(&1)
    );
}

/// #181: an ELLIPSE's centre, major axis, ratio, and parameters; one with
/// no axis and one with a ratio past 1 are not ellipses and are reported.
#[test]
fn ellipses_whole_and_as_arcs() {
    let import = load("valid", "ellipses.dxf").unwrap();
    let ellipses: Vec<cad_document::EllipseEntity> = in_file_order(&import.drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Ellipse(ellipse) => Some(ellipse),
            _ => None,
        })
        .collect();
    assert_eq!(import.report.ellipses, 2);
    assert_eq!(
        import.report.unsupported.get("(invalid geometry)"),
        Some(&2)
    );
    let whole = ellipses[0];
    assert_eq!(whole.center, Point2::new(50.0, 50.0));
    assert_eq!(whole.major, cad_core::Vec2::new(20.0, 0.0));
    assert_eq!(whole.ratio, 0.5);
    assert!(whole.is_full());
    let arc = ellipses[1].ellipse();
    assert!(!arc.is_full());
    assert!(arc.start_point().distance_to(Point2::new(0.0, 30.0)) < 1e-9);
    assert!(arc.end_point().distance_to(Point2::new(-7.5, 0.0)) < 1e-9);
}

#[test]
fn crlf_and_exponent_notation() {
    let import = load("valid", "crlf-exponent-notation.dxf").unwrap();
    assert_eq!(lines(&import.drawing), vec![line(10.0, -2.5, 3.0, 0.5)]);
    assert!(
        !import.report.has_unsupported_content(),
        "zero elevation loses nothing"
    );
}

#[test]
fn a_complete_r12_file() {
    let import = load("valid", "ezdxf-r12-three-lines.dxf").unwrap();
    assert_eq!(
        lines(&import.drawing),
        vec![
            line(0.0, 0.0, 100.0, 0.0),
            line(100.0, 0.0, 100.0, 50.0),
            line(100.0, 50.0, 0.0, 0.0)
        ]
    );
    assert!(!import.report.has_unsupported_content());
}

#[test]
fn a_modern_file_keeps_every_significant_digit() {
    let import = load("valid", "ezdxf-r2018-two-lines.dxf").unwrap();
    assert_eq!(
        lines(&import.drawing),
        vec![
            line(-12.5, 3.25, 1e6, -2.5e-3),
            line(0.1, 0.2, 0.30000000000000004, 707.1067811865476)
        ]
    );
    // Structural *Model_Space / *Paper_Space blocks are not user content.
    assert!(
        !import.report.has_unsupported_content(),
        "{:?}",
        import.report
    );
}

/// DESIGN §24.2: unsupported content is counted and reported; supported
/// geometry is still imported.
#[test]
fn unsupported_content_is_counted_and_reported() {
    let import = load("valid", "ezdxf-r2010-mixed-content.dxf").unwrap();
    assert_eq!(
        lines(&import.drawing),
        vec![
            line(0.0, 0.0, 10.0, 0.0),
            line(0.0, 0.0, 0.0, 10.0),
            line(5.0, 5.0, 6.0, 6.0)
        ]
    );
    let report = &import.report;
    assert_eq!((report.circles, report.arcs, report.polylines), (2, 1, 1));

    assert!(
        report.unsupported.is_empty(),
        "TEXT is imported since #145, the block and its INSERT since #149: {:?}",
        report.unsupported
    );
    assert_eq!((report.blocks, report.block_references), (1, 1));
    // Since #139 the layer WALLS and the colour are held, not dropped.
    assert_eq!(report.with_dropped_properties, 0);
    let walls = import.drawing.layers().find("WALLS").unwrap();
    assert_eq!(
        import.drawing.layers().get(walls).unwrap().color,
        Color::Index(1)
    );
    let on_walls = import
        .drawing
        .records()
        .filter(|(_, record)| record.properties.layer == walls)
        .count();
    let coloured = import
        .drawing
        .records()
        .filter(|(_, record)| record.properties.color == Color::Index(3))
        .count();
    assert_eq!((on_walls, coloured), (1, 1));
    assert!(
        !report.has_unsupported_content(),
        "everything in it is held now"
    );
    assert_eq!(
        report.summary(),
        vec![
            "Imported 3 lines, 2 circles, 1 arc, 1 polyline, 1 text, 1 block reference.",
            // ezdxf's Defpoints layer and WALLS; the DOOR block.
            "With 2 layers and 1 block.",
            // ezdxf wrote `$INSUNITS 6` into this file's header.
            "Drawing unit: metres.",
        ]
    );
}

/// Curves written by an independent DXF library: checks our reading of the
/// CIRCLE and ARC conventions (centre, radius, counter-clockwise degrees).
#[test]
fn circles_and_arcs_from_another_writer() {
    use cad_document::{ArcEntity, CircleEntity};
    let import = load("valid", "ezdxf-r2010-mixed-content.dxf").unwrap();
    let curves: Vec<Entity> = in_file_order(&import.drawing)
        .into_iter()
        .filter(|entity| matches!(entity, Entity::Circle(_) | Entity::Arc(_)))
        .collect();
    assert_eq!(
        curves,
        vec![
            Entity::Circle(CircleEntity::new(Point2::new(0.0, 0.0), 5.0)),
            Entity::Circle(CircleEntity::new(Point2::new(10.0, 10.0), 2.0)),
            Entity::Arc(ArcEntity::new(
                Point2::new(0.0, 0.0),
                3.0,
                0.0,
                std::f64::consts::FRAC_PI_2
            )),
        ]
    );
}

type PolylineSummary = (Vec<(f64, f64)>, bool);

fn polylines(drawing: &Drawing) -> Vec<PolylineSummary> {
    in_file_order(drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Polyline(polyline) => Some((
                polyline
                    .vertices
                    .iter()
                    .map(|vertex| (vertex.x, vertex.y))
                    .collect(),
                polyline.closed,
            )),
            _ => None,
        })
        .collect()
}

/// The bulges of every polyline, one per vertex, in file order.
fn bulges(drawing: &Drawing) -> Vec<Vec<f64>> {
    in_file_order(drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Polyline(polyline) => Some(polyline.bulges().collect()),
            _ => None,
        })
        .collect()
}

/// #115: polylines with arc segments, written by an independent library in
/// both encodings, import with the right arcs.
#[test]
fn bulged_polylines_in_both_encodings() {
    let quarter = (std::f64::consts::PI / 8.0).tan();
    for file in [
        "ezdxf-r12-bulged-polylines.dxf",
        "ezdxf-r2010-bulged-lwpolylines.dxf",
    ] {
        let import = load("valid", file).unwrap();
        assert_eq!(import.report.polylines, 3, "{file}");
        assert!(import.report.unsupported.is_empty(), "{file}");
        assert_eq!(
            polylines(&import.drawing),
            vec![
                (vec![(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (0.0, 4.0)], true),
                (
                    vec![(20.0, 0.0), (30.0, 0.0), (35.0, 5.0), (35.0, 15.0)],
                    false
                ),
                (vec![(50.0, 0.0), (60.0, 0.0)], false),
            ],
            "{file}"
        );
        let found = bulges(&import.drawing);
        assert_eq!(found[0], vec![0.0, 1.0, 0.0, 1.0], "{file}");
        assert_eq!(found[2], vec![-0.5, 0.0], "{file}");
        assert!(
            (found[1][1] - quarter).abs() < 1e-12,
            "{file}: {:?}",
            found[1]
        );

        // The geometry those bulges describe.
        let shapes = in_file_order(&import.drawing);
        // The slot's ends are semicircles of radius 2 reaching x = 12 and x = -2.
        assert!(
            shapes[0].distance_to(Point2::new(12.0, 2.0), cad_document::Tables::none()) < 1e-9,
            "{file}"
        );
        assert!(
            shapes[0].distance_to(Point2::new(-2.0, 2.0), cad_document::Tables::none()) < 1e-9,
            "{file}"
        );
        // The rounded corner is a quarter arc of radius 5 about (30, 5).
        let on_the_corner = Point2::new(30.0 + 5.0 * 0.5f64.sqrt(), 5.0 - 5.0 * 0.5f64.sqrt());
        assert!(
            shapes[1].distance_to(on_the_corner, cad_document::Tables::none()) < 1e-9,
            "{file}"
        );
        // The clockwise arc rises above its chord: sagitta 0.5 × 10 / 2.
        assert!(
            shapes[2].distance_to(Point2::new(55.0, 2.5), cad_document::Tables::none()) < 1e-9,
            "{file}"
        );
    }
}

/// Both polyline encodings, written by an independent library.
#[test]
fn polylines_in_both_encodings() {
    let modern = load("valid", "ezdxf-r2010-polylines.dxf").unwrap();
    assert_eq!(
        polylines(&modern.drawing),
        vec![
            (vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)], true),
            (vec![(20.0, 0.0), (25.0, 5.0), (30.0, 0.0)], false),
            // The one with an arc segment, which used to be reported.
            (vec![(0.0, 20.0), (10.0, 20.0)], false),
            (vec![(40.0, 0.0), (45.0, 5.0), (50.0, 0.0)], false),
        ]
    );
    assert_eq!(bulges(&modern.drawing)[2], vec![0.5, 0.0]);
    let skipped: Vec<(&str, usize)> = modern
        .report
        .unsupported
        .iter()
        .map(|(kind, count)| (kind.as_str(), *count))
        .collect();
    assert_eq!(skipped, vec![("POLYLINE", 1)], "only the 3D polyline");

    let classic = load("valid", "ezdxf-r12-polylines.dxf").unwrap();
    assert_eq!(
        polylines(&classic.drawing),
        vec![
            (vec![(0.0, 0.0), (10.0, 0.0), (10.0, 5.0), (0.0, 5.0)], true),
            (vec![(20.0, 0.0), (25.0, 5.0), (30.0, 0.0)], false),
        ]
    );
    assert!(!classic.report.has_unsupported_content());
}

#[test]
fn malformed_fixtures_fail_for_the_documented_reason() {
    let expect = |name: &str, matches: fn(&DxfError) -> bool| {
        let error = load("malformed", name).unwrap_err();
        assert!(matches(&error), "{name}: unexpected error {error:?}");
    };
    expect("empty.dxf", |e| matches!(e, DxfError::UnexpectedEnd { .. }));
    expect("not-dxf.dxf", |e| {
        matches!(e, DxfError::InvalidGroupCode { line: 1 })
    });
    expect("bad-group-code.dxf", |e| {
        matches!(e, DxfError::InvalidGroupCode { line: 5 })
    });
    expect("bad-coordinate.dxf", |e| {
        matches!(e, DxfError::InvalidNumber { group: 20, .. })
    });
    expect("line-missing-endpoint.dxf", |e| {
        matches!(e, DxfError::MissingCoordinate { group: 11, .. })
    });
    expect("truncated-mid-entity.dxf", |e| {
        matches!(e, DxfError::UnexpectedEnd { .. })
    });
    expect("missing-eof.dxf", |e| {
        matches!(e, DxfError::UnexpectedEnd { .. })
    });
    expect("binary-sentinel.dxf", |e| matches!(e, DxfError::BinaryDxf));
    expect(
        "cyclic-blocks.dxf",
        |e| matches!(e, DxfError::Tables(reason) if reason.contains("contain itself")),
    );
    expect("long-value.dxf", |e| {
        matches!(e, DxfError::ValueTooLong { limit: 65_536, .. })
    });
}

// ── hostile files (#156) ──

/// A polyline whose count claims four billion vertices has two: the count
/// is never trusted, and nothing is allocated for it.
#[test]
fn a_claimed_vertex_count_is_not_believed() {
    let loaded = load("valid", "huge-vertex-count.dxf").unwrap();
    assert_eq!(polylines(&loaded.drawing)[0].0.len(), 2);
    assert!(!loaded.report.has_unsupported_content());
}

/// Every header variable twice: the last one wins, and nothing grows.
#[test]
fn a_repeated_header_takes_the_last_of_each_variable() {
    let loaded = load("valid", "repeated-header.dxf").unwrap();
    let drawing = &loaded.drawing;
    let settings = drawing.settings();
    assert_eq!(loaded.report.unit, Some(cad_core::LengthUnit::Millimetres));
    assert_eq!(settings.current_layer, layer_named(drawing, "B").0);
    assert_eq!(settings.current_color, Color::Index(3));
    assert_eq!(settings.current_linetype, Linetype::ByLayer);
    assert_eq!(settings.current_lineweight, Lineweight::Hundredths(25));
    assert_eq!(settings.ltscale, 5.0);
}

// ── blocks (#149) ──

/// The references of the drawing, in file order.
fn references(drawing: &Drawing) -> Vec<BlockReferenceEntity> {
    in_file_order(drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::BlockRef(reference) => Some(reference),
            _ => None,
        })
        .collect()
}

fn close(a: Point2, b: Point2) -> bool {
    a.distance_to(b) < 1e-9
}

/// Nested blocks written by an independent DXF library: definitions with
/// properties inside them, a block placing another, and every way an
/// INSERT can be placed — scaled, turned, mirrored either way, scaled
/// unevenly, arrayed, and of a block the file never defines.
#[test]
fn nested_blocks_from_another_writer() {
    let loaded = load("valid", "ezdxf-r12-blocks.dxf").unwrap();
    let drawing = &loaded.drawing;
    let report = &loaded.report;

    // The two definitions, with their entities' properties as written.
    let chair = drawing.blocks().find("CHAIR").expect("the chair");
    let pair = drawing.blocks().find("PAIR").expect("the pair");
    let furniture = layer_named(drawing, "FURNITURE").0;
    let records = &drawing.blocks().get(chair).unwrap().entities;
    let kinds: Vec<&str> = records
        .iter()
        .map(|record| record.entity.kind_name())
        .collect();
    assert_eq!(kinds, ["line", "arc", "circle"]);
    assert_eq!(records[0].properties.color, Color::ByBlock);
    assert_eq!(records[1].properties.layer, furniture);
    assert_eq!(records[2].properties.color, Color::Index(5));
    let inner = &drawing.blocks().get(pair).unwrap().entities;
    assert_eq!(inner.len(), 2, "two chairs in the pair");
    assert!(matches!(&*inner[1].entity, Entity::BlockRef(reference) if reference.block == chair));

    // The drawing: a line and six references; the ghost is not among them.
    assert_eq!(lines(drawing), vec![line(0.0, 0.0, 10.0, 0.0)]);
    let placed = references(drawing);
    assert_eq!(placed.len(), 6);
    assert_eq!(
        placed[0],
        BlockReferenceEntity::new(pair, Point2::new(100.0, 0.0), 1.0, 0.0)
    );
    // Twice the size, a quarter turn, on FURNITURE in green.
    assert_eq!(placed[1].block, pair);
    assert_eq!(placed[1].scale, 2.0);
    assert!((placed[1].rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-12);
    assert!(!placed[1].mirrored);
    let turned = properties(drawing)[2];
    assert_eq!((turned.layer, turned.color), (furniture, Color::Index(3)));
    // A negative X scale is a mirror across the X axis and a half turn; a
    // negative Y scale is the mirror alone. Both land where ezdxf put them.
    assert!(placed[2].mirrored && placed[3].mirrored);
    assert!((placed[2].rotation - std::f64::consts::PI).abs() < 1e-12);
    assert_eq!(placed[3].rotation, 0.0);
    let bounds = |index: usize| drawing.bounds_of(&Entity::BlockRef(placed[index].clone()));
    assert!(
        close(bounds(2).min(), Point2::new(-10.0, 50.0))
            && close(bounds(2).max(), Point2::new(0.0, 62.0))
    );
    assert!(
        close(bounds(3).min(), Point2::new(0.0, 68.0))
            && close(bounds(3).max(), Point2::new(10.0, 80.0))
    );
    // The pair twice the size and turned: 60 up, 24 across to the left.
    assert!(
        close(bounds(1).min(), Point2::new(76.0, 100.0))
            && close(bounds(1).max(), Point2::new(100.0, 160.0))
    );
    // Uneven scales come in at the X magnitude; an array as its first copy.
    assert_eq!((placed[4].scale, placed[4].mirrored), (2.0, false));
    assert_eq!(
        placed[5],
        BlockReferenceEntity::new(chair, Point2::new(0.0, -50.0), 1.0, 0.0)
    );

    assert_eq!((report.blocks, report.block_references), (2, 6));
    assert!(report.has_unsupported_content());
    assert_eq!(
        report.summary(),
        vec![
            "Imported 1 line, 6 block references.",
            "Not imported (unsupported): 1 (INSERT array), 1 (INSERT of an undefined block), 1 (INSERT with a non-uniform scale).",
            // FURNITURE and ezdxf's Defpoints.
            "With 2 layers and 2 blocks.",
        ]
    );

    // Our export holds what the model holds, and reads back the same.
    let exported = export(drawing);
    let again = import(exported.as_bytes()).unwrap();
    assert_eq!(references(&again.drawing), placed);
    assert_eq!(again.drawing.blocks().len(), 3);
    assert!(!again.report.has_unsupported_content());
    assert_eq!(export(&again.drawing), exported, "byte-identical");
}

/// A block with attributes, from a modern file: the geometry and the
/// references come in, the attribute definitions and values are reported.
#[test]
fn block_attributes_are_reported() {
    let import = load("valid", "ezdxf-r2010-block-attributes.dxf").unwrap();
    let drawing = &import.drawing;
    let tag = drawing.blocks().find("TAG").expect("the tag");
    let records = &drawing.blocks().get(tag).unwrap().entities;
    assert_eq!(records.len(), 1, "the frame; the ATTDEF is not an entity");
    assert!(matches!(&*records[0].entity, Entity::Polyline(polyline) if polyline.closed));
    let placed = references(drawing);
    assert_eq!(placed.len(), 2);
    assert!((placed[1].rotation.to_degrees() - 45.0).abs() < 1e-9);

    let report = &import.report;
    assert_eq!(
        report.polylines, 0,
        "the frame is the block's, not the drawing's"
    );
    assert_eq!((report.blocks, report.block_references), (1, 2));
    let counts: Vec<(&str, usize)> = report
        .unsupported
        .iter()
        .map(|(kind, count)| (kind.as_str(), *count))
        .collect();
    assert_eq!(counts, vec![("(INSERT attributes)", 1), ("ATTDEF", 1)]);
}

// ── layers, colours, linetypes, lineweights (#139) ──

/// The properties of every entity, in file order.
fn properties(drawing: &Drawing) -> Vec<Properties> {
    let mut records: Vec<_> = drawing
        .records()
        .map(|(id, record)| (id, record.properties))
        .collect();
    records.sort_by_key(|(id, _)| *id);
    records.into_iter().map(|(_, p)| p).collect()
}

fn layer_named<'a>(drawing: &'a Drawing, name: &str) -> (LayerId, &'a Layer) {
    let id = drawing
        .layers()
        .find(name)
        .unwrap_or_else(|| panic!("no layer {name}"));
    (id, drawing.layers().get(id).unwrap())
}

fn linetype_named<'a>(drawing: &'a Drawing, name: &str) -> (LinetypeId, &'a LinetypeDef) {
    let id = drawing
        .linetypes()
        .find(name)
        .unwrap_or_else(|| panic!("no linetype {name}"));
    (id, drawing.linetypes().get(id).unwrap())
}

/// An R12 file from another writer, with layers, colours, and linetypes,
/// imports as written — switches, patterns, and header settings included.
#[test]
fn layers_colours_and_linetypes_from_another_writer() {
    let import = load("valid", "ezdxf-r12-layers-linetypes.dxf").unwrap();
    let drawing = &import.drawing;
    assert_eq!(import.report.imported_total(), 6);
    assert!(
        !import.report.has_unsupported_content(),
        "{:?}",
        import.report
    );

    let (_, dashed) = linetype_named(drawing, "dashed");
    assert_eq!(dashed.pattern, vec![0.5, 0.25]);
    assert_eq!(dashed.description, "Dashed __ __ __ __ __ __ __ __");
    let (center, def) = linetype_named(drawing, "center");
    assert_eq!(def.pattern, vec![1.25, 0.25, 0.25, 0.25]);
    let (dot, def) = linetype_named(drawing, "dot");
    assert_eq!(def.pattern, vec![0.0, 0.25]);
    assert_eq!(drawing.linetypes().len(), 4, "Continuous and three loaded");

    let (walls, def) = layer_named(drawing, "WALLS");
    assert_eq!(def.color, Color::Index(1));
    let (axes, def) = layer_named(drawing, "AXES");
    assert_eq!(
        (def.color, def.linetype),
        (Color::Index(5), Linetype::Named(center))
    );
    let (hidden, def) = layer_named(drawing, "HIDDEN");
    assert!(!def.on && !def.locked);
    assert_eq!(def.color, Color::Index(3), "off, and still colour 3");
    let (fixed, def) = layer_named(drawing, "FIXED");
    assert!(def.locked && def.on);
    assert_eq!(drawing.layers().len(), 6, "0, ezdxf's Defpoints, and four");

    let on = |layer: LayerId| Properties::on(layer);
    assert_eq!(
        properties(drawing),
        vec![
            on(walls),
            on(axes),
            Properties {
                color: Color::Index(4),
                ..on(hidden)
            },
            Properties {
                linetype: Linetype::Named(dot),
                ..on(fixed)
            },
            Properties {
                color: Color::ByBlock,
                linetype: Linetype::ByBlock,
                ..Properties::default()
            },
            Properties {
                color: Color::Index(6),
                linetype: Linetype::CONTINUOUS,
                ..on(walls)
            },
        ]
    );
    assert_eq!(drawing.current_layer(), walls);
    assert_eq!(drawing.current_color(), Color::Index(3));
    assert_eq!(drawing.current_linetype(), Linetype::Named(dot));
    assert_eq!(drawing.ltscale(), 10.0);
    assert_eq!(
        import.report.summary(),
        vec![
            "Imported 3 lines, 1 circle, 1 arc, 1 polyline.",
            "With 5 layers and 3 linetypes."
        ]
    );
}

/// A newer file: lineweights are held; a linetype scale of an entity's
/// own, and the text in a complex linetype, are reported; a layer and a
/// linetype the file names but never defines are made.
#[test]
fn lineweights_scales_and_undefined_names_from_another_writer() {
    let import = load("valid", "ezdxf-r2010-properties.dxf").unwrap();
    let drawing = &import.drawing;
    let (heavy, def) = layer_named(drawing, "heavy");
    assert_eq!(def.lineweight, Lineweight::Hundredths(100));
    let (_, hidden) = linetype_named(drawing, "hidden");
    assert_eq!(hidden.pattern, vec![0.25, 0.125]);
    let (pipes, _) = layer_named(drawing, "pipes");
    let (_, gas) = linetype_named(drawing, "gas");
    assert_eq!(
        gas.pattern,
        vec![0.5, 0.45],
        "the dashes of a complex linetype"
    );
    let (ghost, def) = layer_named(drawing, "ghost");
    assert_eq!(
        (def.color, def.on),
        (Layer::DEFAULT_COLOR, true),
        "a new layer's settings"
    );
    let (phantom, def) = linetype_named(drawing, "phantom");
    assert!(def.is_continuous(), "nothing better is known");

    let all = properties(drawing);
    assert_eq!(all[0], Properties::on(heavy));
    assert_eq!(all[1].lineweight, Lineweight::Hundredths(35));
    assert_eq!(all[2].layer, pipes);
    assert_eq!(all[3].layer, ghost);
    assert_eq!(all[3].linetype, Linetype::Named(phantom));
    assert_eq!(all[4].lineweight, Lineweight::Hundredths(50));
    assert_eq!(drawing.current_lineweight(), Lineweight::Hundredths(25));

    let report = &import.report;
    assert_eq!(
        report.with_dropped_properties, 1,
        "the entity with a scale of its own"
    );
    assert_eq!(report.unsupported.get("(linetype shapes)"), Some(&1));
    assert_eq!((report.created_layers, report.created_linetypes), (1, 1));
    assert!(report.has_unsupported_content());
    assert_eq!(
        report.summary(),
        vec![
            "Imported 4 lines, 1 polyline.",
            "Not imported (unsupported): 1 (linetype shapes).",
            // ezdxf's Defpoints layer, HEAVY, PIPES, and GHOST.
            "With 4 layers and 3 linetypes.",
            "Named but not defined in the file, so created: 1 layer and 1 linetype.",
            "Properties the drawing cannot hold were dropped from 1 entity.",
            "Drawing unit: metres.",
        ]
    );

    // Written back, the lineweights travel too (ADR-0018).
    let text = export(drawing);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(properties(&back.drawing), properties(drawing));
    assert_eq!(
        layer_named(&back.drawing, "HEAVY").1.lineweight,
        layer_named(drawing, "HEAVY").1.lineweight
    );
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// True colours from a newer file (#153): the drawing holds the index
/// palette, so each becomes the index beside it or the nearest one, and
/// the import says so.
#[test]
fn true_colours_from_another_writer() {
    let loaded = load("valid", "ezdxf-r2010-true-colour.dxf").unwrap();
    let drawing = &loaded.drawing;
    let colours: Vec<Color> = properties(drawing).iter().map(|p| p.color).collect();
    // Orange → 30; a near-blue with index 5 beside it → 5 all the same;
    // pure red → 1; the plain indexed line keeps its 3.
    assert_eq!(
        colours,
        [
            Color::Index(30),
            Color::Index(5),
            Color::Index(1),
            Color::Index(3)
        ]
    );
    // ezdxf wrote a default 7 beside the layer's true colour; the colour decides.
    let (_, teal) = layer_named(drawing, "TEAL");
    assert_eq!(
        teal.color,
        Color::Index(cad_document::nearest_index([0, 128, 128]))
    );
    assert_ne!(teal.color, Color::Index(7));
    let report = &loaded.report;
    assert_eq!(report.with_dropped_properties, 3);
    assert!(report.has_unsupported_content());
    assert_eq!(
        report.summary(),
        vec![
            "Imported 3 lines, 1 circle.",
            "Not imported (unsupported): 1 (layer true colour).",
            "With 2 layers.",
            "Properties the drawing cannot hold were dropped from 3 entities.",
            "Drawing unit: metres.",
        ]
    );
    // Written back, the indices are what there is; the file is clean.
    let text = export(drawing);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert!(!back.report.has_unsupported_content());
    assert_eq!(export(&back.drawing), text);
}

// ── text (#145) ──

fn texts(drawing: &Drawing) -> Vec<TextEntity> {
    in_file_order(drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

fn paragraphs(drawing: &Drawing) -> Vec<MTextEntity> {
    in_file_order(drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::MText(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// An R12 file from another writer with TEXT in every justification and
/// a STYLE table imports as written; what the drawing cannot hold — fit
/// text — is reported.
#[test]
fn text_and_styles_from_another_writer() {
    let import = load("valid", "ezdxf-r12-text.dxf").unwrap();
    let drawing = &import.drawing;
    let texts = texts(drawing);
    assert_eq!(import.report.texts, 7);
    assert_eq!(texts.len(), 7);

    let (roman, _) = {
        let id = drawing.text_styles().find("ROMAN").unwrap();
        (id, drawing.text_styles().get(id).unwrap())
    };
    assert_eq!(
        drawing.text_styles().len(),
        3,
        "Standard, ROMAN, and the ghost"
    );
    let ghost = drawing.text_styles().find("GHOST").unwrap();

    let left = &texts[0];
    assert_eq!((left.content.as_str(), left.height), ("Left", 2.5));
    assert_eq!(left.insertion, Point2::new(0.0, 0.0));
    assert_eq!(left.justification, Justification::Left);
    assert_eq!(left.style, TextStyleId::FIRST);

    let centre = &texts[1];
    assert_eq!(centre.justification, Justification::Center);
    assert_eq!(
        centre.insertion,
        Point2::new(50.0, 0.0),
        "the alignment point"
    );
    assert_eq!(centre.style, roman);

    let right = &texts[2];
    assert_eq!(right.justification, Justification::Right);
    assert_eq!(right.insertion, Point2::new(100.0, 0.0));
    assert!((right.rotation - 30f64.to_radians()).abs() < 1e-12);
    assert_eq!(right.height, 5.0);
    assert_eq!(properties(drawing)[2].color, Color::Index(1));

    // Middle: centred, with the baseline half a height below the point.
    let middle = &texts[3];
    assert_eq!(middle.justification, Justification::Center);
    assert!(
        (middle.insertion.y - 18.0).abs() < 1e-9,
        "{:?}",
        middle.insertion
    );
    // Top left: the baseline a height below the point.
    let top = &texts[4];
    assert_eq!(top.justification, Justification::Left);
    assert!((top.insertion.y - 36.0).abs() < 1e-9, "{:?}", top.insertion);
    // Fit: read as left text at its first point; the stretch is dropped.
    let fit = &texts[5];
    assert_eq!(fit.insertion, Point2::new(0.0, 60.0));
    assert_eq!(
        texts[6].style, ghost,
        "a style the file never defined is made"
    );

    let report = &import.report;
    assert_eq!(report.with_dropped_properties, 1, "the fit text");
    assert_eq!((report.text_styles, report.created_text_styles), (2, 1));
    assert_eq!(
        report.summary(),
        vec![
            "Imported 7 texts.",
            "With 1 layer and 2 text styles.",
            "Named but not defined in the file, so created: 1 text style.",
            "Properties the drawing cannot hold were dropped from 1 entity.",
        ]
    );
    // What came in goes out, and comes back the same.
    let text = export(drawing);
    assert!(text.contains("AcDbTextStyleTableRecord\n  2\nROMAN\n"));
    assert!(text.contains("  7\nROMAN\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// A newer file's DIMSTYLE table: the variables the drawing holds are read,
/// the suffix from DIMPOST, the current style from `$DIMSTYLE`; a row with
/// a value a dimension cannot be drawn by keeps the default and is reported.
#[test]
fn dimension_styles_from_another_writer() {
    let import = load("valid", "ezdxf-r2010-dimstyles.dxf").unwrap();
    let drawing = &import.drawing;
    let styles = drawing.dim_styles();
    assert_eq!(styles.len(), 4, "Standard, ARCH, CIVIL, FLAT");
    let standard = styles.get(DimStyleId::FIRST).unwrap();
    assert_eq!(standard.name, "Standard", "ezdxf's Standard restyles ours");
    assert_eq!(standard.text_height, 2.5);

    let arch = styles.find("arch").unwrap();
    assert_eq!(drawing.current_dim_style(), arch, "$DIMSTYLE");
    let arch = styles.get(arch).unwrap();
    assert_eq!(arch.name, "ARCH");
    assert_eq!(
        (
            arch.text_height,
            arch.arrow_size,
            arch.text_gap,
            arch.extension_offset,
            arch.extension_overshoot
        ),
        (3.5, 2.0, 1.0, 0.5, 1.5)
    );
    assert_eq!((arch.decimals, arch.suffix.as_str()), (1, " mm"));
    assert_eq!(arch.format(12.345), "12.3 mm");

    let civil = styles.get(styles.find("CIVIL").unwrap()).unwrap();
    assert_eq!((civil.text_height, civil.decimals), (5.0, 0));
    // A DIMPOST without `<>` is all suffix; its leading space goes as every
    // value's does, since the reader trims what it reads.
    assert_eq!(civil.suffix, "m");
    assert_eq!(
        civil.arrow_size, 2.5,
        "what the row left out is the default"
    );

    let flat = styles.get(styles.find("FLAT").unwrap()).unwrap();
    assert_eq!(flat.text_height, 2.5, "a zero height is not taken");
    assert!(flat.check().is_ok());

    let report = &import.report;
    assert_eq!(report.dim_styles, 3);
    assert_eq!(report.unsupported.get("(dimension style values)"), Some(&1));
    assert_eq!(
        report.summary()[..3],
        [
            "Imported 1 line.",
            "Not imported (unsupported): 1 (dimension style values).",
            // ezdxf's Defpoints layer.
            "With 1 layer and 3 dimension styles."
        ]
    );

    // What came in goes out — the rows with their variables, the current
    // style in the header — and comes back the same.
    let text = export(drawing);
    assert!(text.contains("  9\n$DIMSTYLE\n  2\nARCH\n"));
    assert!(text.contains(
        "AcDbDimStyleTableRecord\n  2\nARCH\n 70\n0\n  3\n<> mm\n 41\n2.0\n 42\n0.5\n 44\n1.5\n140\n3.5\n147\n1.0\n271\n1\n"
    ));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.drawing.dim_styles().len(), 4);
    assert_eq!(
        back.drawing
            .dim_styles()
            .get(back.drawing.current_dim_style())
            .unwrap(),
        arch
    );
    assert!(
        !back.report.has_unsupported_content(),
        "our own rows are whole"
    );
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// DIMENSION from another writer (ADR-0020, #166): every kind the drawing
/// has a measure for, read from its points; its style, made when the file
/// did not define it; the drafter's text and turned text; the anonymous
/// blocks passed over; an ordinate dimension reported. Our export writes
/// the dimensions with anonymous blocks of their own, which the corpus
/// runner round-trips byte for byte.
#[test]
fn dimensions_from_another_writer() {
    use cad_document::Measure;
    let import = load("corpus", "r2010-dimensions.dxf").unwrap();
    let drawing = &import.drawing;
    let dimensions: Vec<_> = in_file_order(drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Dimension(dimension) => Some(dimension),
            _ => None,
        })
        .collect();
    assert_eq!(dimensions.len(), 8);
    let kinds: Vec<&str> = dimensions
        .iter()
        .map(|dimension| dimension.measure.kind())
        .collect();
    assert_eq!(
        kinds,
        [
            "linear", "linear", "linear", "angular", "angular", "radius", "diameter", "linear"
        ]
    );
    let values: Vec<f64> = dimensions
        .iter()
        .map(|d| (d.value() * 1e6).round() / 1e6)
        .collect();
    assert_eq!(values, [100.0, 50.0, 40.0, 90.0, 90.0, 30.0, 60.0, 60.0]);

    // The first is in ARCH, which the file defines; the rest in Standard.
    let arch = drawing.dim_styles().find("ARCH").unwrap();
    assert_eq!(dimensions[0].style, arch);
    assert_eq!(drawing.dim_styles().get(arch).unwrap().text_height, 3.5);
    assert!(dimensions[1..].iter().all(|d| d.style == DimStyleId::FIRST));
    // The two lines' angle, measured where the arc was placed.
    let Measure::Angular { vertex, .. } = dimensions[3].measure else {
        panic!("angular");
    };
    assert!(vertex.distance_to(Point2::new(0.0, 100.0)) < 1e-9);
    assert_eq!(dimensions[3].line_point, Point2::new(15.0, 115.0));
    // The radius: centre and radius from the two points, placed where the
    // text was.
    assert_eq!(
        dimensions[5].measure,
        Measure::Radius {
            center: Point2::new(200.0, 0.0),
            radius: 30.0
        }
    );
    // The drafter's text, and its turn away from where it would read.
    assert_eq!(dimensions[7].text_override.as_deref(), Some("TYP"));
    let turn = dimensions[7].text_angle.expect("turned");
    assert!((turn - 30f64.to_radians()).abs() < 1e-9, "{turn}");

    // The *D blocks are not blocks of ours; ezdxf's arrowhead block is.
    let block_names: Vec<&str> = drawing
        .blocks()
        .iter()
        .map(|(_, block)| block.name.as_str())
        .collect();
    assert_eq!(block_names, ["*Model_Space", "_CLOSEDFILLED"]);
    let report = &import.report;
    assert_eq!(report.dimensions, 8);
    assert_eq!(report.unsupported.get("(ordinate DIMENSION)"), Some(&1));
    assert_eq!(report.summary()[0], "Imported 8 dimensions.");

    // Our file: a block record and an anonymous block per dimension, named
    // by each DIMENSION, holding what the style laid out.
    let text = export(drawing);
    for n in 1..=8 {
        assert!(
            text.contains(&format!("AcDbBlockTableRecord\n  2\n*D{n}\n")),
            "record *D{n}"
        );
        assert!(
            text.contains(&format!("AcDbBlockBegin\n  2\n*D{n}\n 70\n1\n")),
            "block *D{n}"
        );
        assert!(
            text.contains(&format!("AcDbDimension\n  2\n*D{n}\n")),
            "dimension {n}"
        );
    }
    let turn_written: f64 = text
        .split(" 53\n")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .and_then(|value| value.parse().ok())
        .expect("the text's turn is written");
    assert!((turn_written - 30.0).abs() < 1e-9, "{turn_written}");
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.dimensions, 8);
    assert!(!back.report.has_unsupported_content());
    assert_eq!(back.drawing.blocks().len(), 2);
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// HATCH from another writer (ADR-0021, #169): polyline and edge paths —
/// lines, arcs either way round, a full circle — solid and pattern fills
/// with their angle and scale; a spline boundary, a gradient, and a
/// pattern the table lacks reported. What came in goes out and comes back
/// the same.
#[test]
fn hatches_from_another_writer() {
    use cad_document::HatchStyle;
    let import = load("valid", "ezdxf-r2010-hatches.dxf").unwrap();
    let hatches: Vec<_> = in_file_order(&import.drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .collect();
    assert_eq!(hatches.len(), 6, "all but the spline-bounded one");
    let report = &import.report;
    assert_eq!(report.hatches, 6);
    for key in [
        "(HATCH boundary with a spline or ellipse)",
        "(gradient HATCH)",
        "(HATCH pattern not in the table)",
    ] {
        assert_eq!(report.unsupported.get(key), Some(&1), "{key}");
    }

    // A square with a round hole: the hole an edge path of one full arc.
    let holed = &hatches[0];
    assert_eq!(holed.style, HatchStyle::Solid);
    assert_eq!(holed.loops.len(), 2);
    let expected = 10_000.0 - std::f64::consts::PI * 400.0;
    assert!((holed.area() - expected).abs() < 5.0, "{}", holed.area());
    assert!(!holed.contains(Point2::new(50.0, 50.0)));

    // ANSI31 at 30° and scale 2 on a path with a half-circle bulge.
    let HatchStyle::Pattern {
        name, angle, scale, ..
    } = &hatches[1].style
    else {
        panic!("a pattern");
    };
    assert_eq!(name, "ANSI31");
    assert!((angle.to_degrees() - 30.0).abs() < 1e-9);
    assert_eq!(*scale, 2.0);
    let half_circle = 10_000.0 + std::f64::consts::PI * 2500.0 / 2.0;
    assert!((hatches[1].area() - half_circle).abs() < 10.0);

    // Lines and an arc counter-clockwise, and the same shape wound the
    // other way with a clockwise arc: the same region.
    for hatch in &hatches[2..4] {
        assert!(
            (hatch.area() - half_circle).abs() < 10.0,
            "{}",
            hatch.area()
        );
        assert_eq!(hatch.style, HatchStyle::Solid);
    }
    assert!(
        hatches[2].contains(Point2::new(450.0, 140.0)),
        "under the arc"
    );
    assert!(
        hatches[3].contains(Point2::new(650.0, 140.0)),
        "under the clockwise arc"
    );
    // The gradient and the pattern the table lacks are filled solid.
    assert_eq!(hatches[4].style, HatchStyle::Solid);
    assert_eq!(hatches[5].style, HatchStyle::Solid);

    let text = export(&import.drawing);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.hatches, 6);
    assert!(
        !back.report.has_unsupported_content(),
        "our own hatches are whole"
    );
    let again: Vec<_> = in_file_order(&back.drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Hatch(hatch) => Some(hatch),
            _ => None,
        })
        .collect();
    assert_eq!(
        again[1].style, hatches[1].style,
        "the pattern's origin survives"
    );
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// A newer file's MTEXT: words and line breaks kept, formatting dropped
/// and reported, attachment points placed, long text in pieces joined.
#[test]
fn paragraphs_from_another_writer() {
    let import = load("valid", "ezdxf-r2010-mtext.dxf").unwrap();
    let drawing = &import.drawing;
    let notes = paragraphs(drawing);
    assert_eq!(notes.len(), 5);
    assert_eq!(import.report.texts, 5);

    assert_eq!(notes[0].content, "First line\nSecond line");
    assert_eq!((notes[0].width, notes[0].height), (60.0, 2.5));
    assert_eq!(notes[0].insertion, Point2::new(0.0, 0.0));

    // The source has a space before the non-breaking space: two spaces.
    assert_eq!(notes[1].content, "Bold and tall\nred a/b stacked  tied");
    assert!((notes[1].rotation - 30f64.to_radians()).abs() < 1e-12);

    // Attached at its middle centre (5): the top left is up and to the left.
    let centred = &notes[2];
    assert_eq!(centred.content, "Centred middle");
    assert!(
        centred.insertion.x < 100.0 && centred.insertion.y > 0.0,
        "{:?}",
        centred.insertion
    );
    let frame = centred.frame();
    let middle = centred.insertion + frame.along * (frame.width / 2.0)
        - frame.up * ((centred.insertion - frame.start).dot(frame.up) / 2.0);
    assert!(middle.distance_to(Point2::new(100.0, 0.0)) < 1e-9);
    // Bottom right (9): the box ends at the point.
    let corner = &notes[3];
    let frame = corner.frame();
    assert!(
        frame.start.x + frame.width - 200.0 < 1e-9
            && (frame.start.x + frame.width - 200.0).abs() < 1e-9
    );
    assert!((frame.start.y - 0.0).abs() < 1e-9, "{:?}", frame.start);

    assert_eq!(notes[4].content.len(), 304, "pieces of 250 joined");
    assert!(notes[4].content.ends_with("\nend"));

    let report = &import.report;
    assert_eq!(report.formatting_dropped, 1);
    assert!(report.has_unsupported_content());
    assert_eq!(
        report.summary(),
        vec![
            "Imported 5 texts.",
            "With 1 layer.",
            "Formatting — fonts, heights, colours, stacking — was dropped from 1 paragraph; the words and line breaks were kept.",
            "Drawing unit: metres.",
        ]
    );
    // Written back, a paragraph is an MTEXT with its break as a code (ADR-0018).
    let text = export(drawing);
    assert!(text.contains("  1\nFirst line\\PSecond line\n"));
    assert_eq!(
        paragraphs(&cad_dxf::import(text.as_bytes()).unwrap().drawing),
        notes
    );
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// Golden file: the exact bytes written for the M0 user-story line.
/// Regenerate deliberately with `UPDATE_GOLDEN=1 cargo test -p cad-dxf`.
#[test]
fn export_matches_the_golden_file() {
    let end = Point2::ORIGIN + cad_core::Vec2::from_polar(1000.0, 45f64.to_radians());
    let drawing = Drawing::from_entities([Entity::Line(LineEntity::new(Point2::ORIGIN, end))]);
    let exported = export(&drawing);

    let golden = fixtures("valid").join("cadslate-m0-line.dxf");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&golden, &exported).unwrap();
    }
    assert_eq!(
        exported,
        fs::read_to_string(&golden).unwrap(),
        "export changed; see the test's doc comment"
    );
}
