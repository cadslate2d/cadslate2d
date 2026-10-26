//! Fixture-driven tests (DESIGN §41). See `test-data/dxf/README.md`.

use std::fs;
use std::path::{Path, PathBuf};

use cad_core::{Aabb, Point2, Vec2};
use cad_document::{
    BlockReferenceEntity, Chain, Color, DimStyleId, DimensionEntity, Drawing, Entity, Group,
    Justification, Layer, LayerId, Layout, LayoutId, LeaderEntity, LineEntity, Linetype,
    LinetypeDef, LinetypeId, Lineweight, MLeaderContent, MLeaderEntity, MLeaderStyle,
    MLeaderStyleId, MTextEntity, PointStyle, Properties, PurgeScope, TextEntity, TextStyleId,
    ViewportEntity,
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
        // The ENTITIES section is the model's: a layout's records are
        // written with the blocks, before the first step (#191).
        let model = drawing.entities_in(LayoutId::MODEL).count();
        assert!(steps > 0 || model <= 3, "{name}: {steps} steps");
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
        "src/wipeout.rs",
        "tests/fixtures.rs",
        // The drawing order's tiers, which the reader and the writer share.
        "../cad-document/src/draw_order.rs",
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

/// #182: a SPLINE by fit points alone is interpolated through them; one
/// by control points, knots, and weights is taken as it is; one of a
/// single point is not a spline and is reported.
#[test]
fn splines_by_fit_points_and_by_control_points() {
    let import = load("valid", "splines.dxf").unwrap();
    let splines: Vec<cad_document::SplineEntity> = in_file_order(&import.drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Spline(spline) => Some(spline),
            _ => None,
        })
        .collect();
    assert_eq!(import.report.splines, 2);
    assert_eq!(
        import.report.unsupported.get("(invalid geometry)"),
        Some(&1)
    );
    let arch = &splines[0];
    assert_eq!(arch.fit.len(), 4);
    assert_eq!(arch.degree, 3);
    assert!(arch.spline().distance_to_point(Point2::new(30.0, 60.0)) < 0.1);
    let circle = &splines[1];
    assert_eq!(
        (circle.degree, circle.control.len(), circle.weights.len()),
        (2, 9, 9)
    );
    assert!(circle.fit.is_empty());
    for i in 0..=20 {
        let point = circle.spline().point_at(i as f64 / 20.0);
        assert!((point.distance_to(Point2::new(50.0, 50.0)) - 10.0).abs() < 1e-9);
    }
}

/// #184, #191: two layouts with their paper-space blocks — a viewport and a
/// title line in one, a line in the other. The lines are entities of
/// their layouts, read like the model's; the viewport is kept as the file
/// held it; all of it is written back, the handles made this file's, and
/// read again the same. Nothing of it is a loss.
#[test]
fn layouts_are_kept_through_a_round_trip() {
    let import = load("valid", "layouts.dxf").unwrap();
    let drawing = &import.drawing;
    let layouts: Vec<(LayoutId, &Layout)> = drawing.layouts().iter().collect();
    let names: Vec<&str> = layouts
        .iter()
        .map(|(_, layout)| layout.name.as_str())
        .collect();
    assert_eq!(names, ["Model", "Layout1", "Layout2"]);
    let (one, layout1) = layouts[1];
    let (two, layout2) = layouts[2];
    assert_eq!(layout1.block_name, "*Paper_Space");
    assert_eq!(layout2.block_name, "*Paper_Space0");
    assert!(
        layout1.kept_entities.is_empty(),
        "the viewport is an entity (#192)"
    );
    assert!(layout2.kept_entities.is_empty());
    assert!(!layout1.kept.is_empty(), "the LAYOUT object, kept");
    // The lines and the viewport are entities of their layouts.
    assert_eq!(drawing.len(), 4);
    assert_eq!(drawing.shown().count(), 1, "the model holds its one line");
    assert_eq!(
        drawing.shown_in(one).count(),
        2,
        "the viewport and the title line"
    );
    assert_eq!(drawing.shown_in(two).count(), 1);
    let viewport = drawing
        .shown_in(one)
        .find_map(|(_, entity)| match entity {
            Entity::Viewport(viewport) => Some(*viewport),
            _ => None,
        })
        .expect("the viewport");
    assert_eq!(viewport.center, Point2::new(148.5, 105.0));
    assert_eq!((viewport.width, viewport.height), (277.0, 190.0));
    assert_eq!(viewport.view_center, Point2::new(50.0, 25.0));
    assert!(
        (viewport.scale - 190.0 / 200.0).abs() < 1e-12,
        "190 mm of paper for 200 units"
    );
    assert!(viewport.on);
    let (title, _) = drawing
        .shown_in(one)
        .find(|(_, entity)| matches!(entity, Entity::Line(_)))
        .unwrap();
    assert_eq!(drawing.layer_of(title).unwrap().name, "TITLE");
    assert_eq!(import.report.layouts, vec!["Layout1", "Layout2"]);
    assert_eq!(
        import.report.lines, 3,
        "a layout's entities count as imported"
    );
    assert_eq!(import.report.viewports, 1);
    assert!(
        import
            .report
            .summary()
            .iter()
            .any(|line| line == "Imported 3 lines, 1 viewport."),
        "{:?}",
        import.report.summary()
    );
    assert!(
        import.report.losses().is_empty(),
        "{:?}",
        import.report.losses()
    );
    assert!(
        import
            .report
            .summary()
            .iter()
            .any(|line| line == "2 layouts: Layout1, Layout2."),
        "{:?}",
        import.report.summary()
    );

    let text = export(drawing);
    // The groups the file held, other than handles, are written as they were.
    assert!(text.contains("\n  2\n*Paper_Space0\n"));
    assert!(text.contains("\nVIEWPORT\n"));
    assert!(
        text.contains("\n 45\n200.0\n"),
        "the viewport's view height"
    );
    assert!(text.contains("\n  8\nTITLE\n"), "the title line's layer");
    assert!(text.contains("\n  1\nLayout2\n"));
    assert!(
        !text.contains("\n  3\nLayout1\n  3\nLayout1"),
        "written once"
    );
    // A layout's line is in its paper block, flagged as paper, and the
    // ENTITIES section holds the model's line alone.
    let entities = text.split("\n  2\nENTITIES\n").nth(1).unwrap();
    let entities = &entities[..entities.find("\nENDSEC\n").unwrap()];
    assert_eq!(entities.matches("\nLINE\n").count(), 1);
    assert!(!entities.contains("\n 67\n1\n"));
    let blocks = text.split("\n  2\nBLOCKS\n").nth(1).unwrap();
    let paper = blocks.split("\n  2\n*Paper_Space\n").nth(1).unwrap();
    let paper = &paper[..paper.find("\nENDBLK\n").unwrap()];
    assert!(paper.contains("\n 67\n1\n  8\nTITLE\n"));
    // The viewport the layout names as active is the one written, under its new handle.
    let viewport_handle = text
        .split("\nVIEWPORT\n  5\n")
        .nth(1)
        .and_then(|rest| rest.lines().next())
        .expect("the viewport's handle");
    assert!(text.contains(&format!("\n331\n{viewport_handle}\n")));

    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.layouts, vec!["Layout1", "Layout2"]);
    assert!(back.report.losses().is_empty());
    assert_eq!(back.drawing.shown_in(one).count(), 2);
    assert_eq!(export(&back.drawing), text, "stable on the second pass");
}

/// #191: a layout made in the drawing is written with a paper block and a
/// sheet of its own, and reads back as a layout with its entities; its
/// name survives a rename of a layout a file gave its LAYOUT object.
#[test]
fn a_layout_made_here_and_a_renamed_one_are_written_and_read_back() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let sheet = tx.add_layout(Layout::named("Title sheet")).unwrap();
    tx.insert_in(
        sheet,
        LineEntity::new(Point2::new(0.0, 0.0), Point2::new(10.0, 0.0)),
    )
    .unwrap();
    tx.insert(LineEntity::new(
        Point2::new(0.0, 0.0),
        Point2::new(5.0, 0.0),
    ))
    .unwrap();
    tx.commit();
    let text = export(&drawing);
    assert!(text.contains("\n  2\n*Paper_Space\n"));
    assert!(
        text.contains("\n  3\nTitle sheet\n"),
        "the layouts dictionary"
    );
    assert!(text.contains("\n  1\nTitle sheet\n"), "the LAYOUT object");
    assert!(
        !text.contains("\n  1\nLayout1\n"),
        "no default sheet beside it"
    );
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    let names: Vec<String> = back
        .drawing
        .layouts()
        .iter()
        .map(|(_, l)| l.name.clone())
        .collect();
    assert_eq!(names, ["Model", "Title sheet"]);
    let (id, _) = back.drawing.layouts().iter().nth(1).unwrap();
    assert_eq!(back.drawing.shown_in(id).count(), 1);
    assert_eq!(back.drawing.shown().count(), 1);
    assert_eq!(export(&back.drawing), text, "stable on the second pass");

    // A file's layout renamed: the kept LAYOUT object carries the new name.
    let mut imported = load("valid", "layouts.dxf").unwrap().drawing;
    let (two, _) = imported.layouts().iter().nth(2).unwrap();
    let mut tx = imported.transaction();
    tx.rename_layout(two, "Details").unwrap();
    tx.commit();
    let text = export(&imported);
    assert!(text.contains("\n  1\nDetails\n"));
    assert!(!text.contains("\n  1\nLayout2\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.layouts, vec!["Layout1", "Details"]);
}

/// #208: a layout's plot settings — read from the kept LAYOUT groups as
/// the file gives them, changed here, written back into those groups, and
/// read back the same; a layout made here writes its own.
#[test]
fn a_layouts_plot_settings_are_read_changed_and_written_back() {
    use cad_document::{LayoutArea, LayoutPlot};
    let mut drawing = load("valid", "layouts.dxf").unwrap().drawing;
    let (one, row) = drawing.layouts().iter().nth(1).unwrap();
    assert_eq!(
        row.plot_settings(),
        LayoutPlot::default(),
        "a whole A3 sheet at 1:1"
    );
    let window = Aabb::from_points(Point2::new(10.0, 20.0), Point2::new(110.0, 70.0));
    let changed = LayoutPlot {
        area: LayoutArea::Window(window),
        scale: None,
        turns: 1,
    };
    let mut tx = drawing.transaction();
    tx.set_layout_plot(one, changed).unwrap();
    let made = tx.add_layout(Layout::named("Made here")).unwrap();
    tx.set_layout_plot(
        made,
        LayoutPlot {
            area: LayoutArea::Extents,
            scale: Some(0.25),
            turns: 3,
        },
    )
    .unwrap();
    tx.commit();
    let text = export(&drawing);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    let rows: Vec<LayoutPlot> = back
        .drawing
        .layouts()
        .iter()
        .skip(1)
        .map(|(_, row)| row.plot_settings())
        .collect();
    assert_eq!(rows[0], changed);
    assert_eq!(rows[1], LayoutPlot::default(), "Layout2 as it was");
    assert_eq!(
        rows[2],
        LayoutPlot {
            area: LayoutArea::Extents,
            scale: Some(0.25),
            turns: 3,
        }
    );
    assert_eq!(export(&back.drawing), text, "stable on the second pass");
}

/// #206: xlines and rays from another writer — on a layer of their own,
/// coloured — read as construction lines, left out of the extents, and
/// written back as XLINE and RAY with their unit directions.
#[test]
fn construction_lines_from_another_writer_round_trip_and_stay_out_of_the_extents() {
    let import = load("valid", "ezdxf-r2010-construction-lines.dxf").unwrap();
    assert_eq!(import.report.construction_lines, 4);
    assert_eq!(import.report.lines, 1);
    assert!(
        !import.report.has_unsupported_content(),
        "{:?}",
        import.report
    );
    let drawing = &import.drawing;
    assert_eq!(
        drawing.extents().unwrap().max(),
        Point2::new(100.0, 50.0),
        "the line alone counts"
    );
    let kinds: Vec<&str> = drawing
        .shown()
        .map(|(_, entity)| entity.kind_name())
        .collect();
    assert_eq!(
        kinds,
        [
            "line",
            "construction line",
            "construction line",
            "ray",
            "ray"
        ]
    );
    let (id, _) = drawing
        .shown()
        .find(|(_, entity)| matches!(entity, Entity::XLine(_)))
        .unwrap();
    assert_eq!(drawing.layer_of(id).unwrap().name, "CONSTRUCTION");
    let text = export(drawing);
    assert!(text.contains("\n100\nAcDbXline\n 10\n0.0\n 20\n25.0\n"));
    assert!(text.contains("\n100\nAcDbRay\n 10\n100.0\n 20\n50.0\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.construction_lines, 4);
    assert_eq!(export(&back.drawing), text, "stable on the second pass");
    // An xline with no direction is not one.
    let none = "  0\nSECTION\n  2\nENTITIES\n  0\nXLINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n0.0\n 21\n0.0\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(none.as_bytes()).unwrap();
    assert_eq!(import.report.construction_lines, 0);
    assert!(import.report.has_unsupported_content());
}

/// #205: leaders from another writer — one with an MTEXT note named by
/// handle, which becomes the leader's words and no entity of its own;
/// one with no note, ending leftwards; one with a spline path, read
/// straight — in their styles; a paragraph that is no note stays one;
/// written back with the note as an MTEXT under the leader's 340.
#[test]
fn leaders_from_another_writer_take_their_notes_and_round_trip() {
    let import = load("valid", "ezdxf-r2010-leaders.dxf").unwrap();
    assert_eq!(import.report.leaders, 3);
    assert_eq!(
        import.report.texts, 1,
        "the note is the leader's, the paragraph its own"
    );
    assert!(
        !import.report.has_unsupported_content(),
        "{:?}",
        import.report
    );
    let drawing = &import.drawing;
    let leaders: Vec<&LeaderEntity> = drawing
        .shown()
        .filter_map(|(_, entity)| match entity {
            Entity::Leader(leader) => Some(leader),
            _ => None,
        })
        .collect();
    assert_eq!(leaders.len(), 3);
    assert_eq!(leaders[0].text.as_deref(), Some("See detail"));
    assert_eq!(
        leaders[0].vertices,
        vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(20.0, 10.0)
        ]
    );
    let notes = drawing.dim_styles().find("NOTES").unwrap();
    assert_eq!(leaders[0].style, notes);
    assert_eq!(drawing.dim_styles().get(notes).unwrap().arrow_size, 3.0);
    assert_eq!(leaders[1].text, None);
    assert_eq!(leaders[1].style, DimStyleId::FIRST);
    assert_eq!(leaders[2].vertices.len(), 3, "a spline path, read straight");
    assert_eq!(drawing.shown().count(), 4);

    let text = export(drawing);
    assert!(text.contains("\n  0\nLEADER\n"));
    assert!(text.contains("\n 73\n0\n") && text.contains("\n 73\n3\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.leaders, 3);
    assert_eq!(back.report.texts, 1);
    let again: Vec<Option<String>> = back
        .drawing
        .shown()
        .filter_map(|(_, entity)| match entity {
            Entity::Leader(leader) => Some(leader.text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(again, vec![Some("See detail".to_owned()), None, None]);
    assert_eq!(export(&back.drawing), text, "stable on the second pass");
}

/// #204: a polyline's constant width from another writer — an
/// LWPOLYLINE's (43), a POLYLINE's defaults (40, 41), VERTEX widths that
/// all agree — is read as the width; one that varies along the polyline
/// is dropped and counted; the width is written back as 43.
#[test]
fn wide_polylines_from_another_writer_keep_their_width_and_a_varying_one_is_dropped() {
    for (file, widths) in [
        ("ezdxf-r2010-wide-polylines.dxf", [2.0, 1.5, 0.0]),
        ("ezdxf-r12-wide-polylines.dxf", [2.5, 1.0, 0.0]),
    ] {
        let import = load("valid", file).unwrap();
        assert_eq!(import.report.polylines, 3, "{file}");
        assert_eq!(
            import.report.with_dropped_properties, 1,
            "{file}: the varying one"
        );
        let polylines: Vec<f64> = import
            .drawing
            .shown()
            .filter_map(|(_, entity)| match entity {
                Entity::Polyline(polyline) => Some(polyline.width()),
                _ => None,
            })
            .collect();
        assert_eq!(polylines, widths, "{file}");
        let text = export(&import.drawing);
        assert!(
            text.contains("\n 43\n2.5\n") || text.contains("\n 43\n2.0\n"),
            "{file}"
        );
        let back = cad_dxf::import(text.as_bytes()).unwrap();
        let again: Vec<f64> = back
            .drawing
            .shown()
            .filter_map(|(_, entity)| match entity {
                Entity::Polyline(polyline) => Some(polyline.width()),
                _ => None,
            })
            .collect();
        assert_eq!(again, widths, "{file}: read back");
        assert_eq!(
            back.report.with_dropped_properties, 0,
            "{file}: nothing to drop in our own file"
        );
    }
}

/// #200: a file purged has no trace of the rows that went, and what a
/// second read reports is what the purge left.
#[test]
fn a_purged_file_is_written_without_the_rows_that_went() {
    let import = load("valid", "ezdxf-r12-layers-linetypes.dxf").unwrap();
    let mut drawing = import.drawing;
    let before = (drawing.layers().len(), drawing.linetypes().len());
    let mut tx = drawing.transaction();
    let purged = tx.purge(PurgeScope::ALL);
    tx.commit();
    // Every layer but ezdxf's Defpoints has something on it, and every
    // linetype is used by an entity or a layer.
    assert_eq!(purged.layers, vec!["Defpoints"]);
    assert!(purged.linetypes.is_empty());
    assert!(purged.blocks.is_empty() && purged.text_styles.is_empty());
    let text = export(&drawing);
    for name in purged.layers.iter().chain(&purged.linetypes) {
        assert!(
            !text.contains(&format!("\n  2\n{name}\n")),
            "{name} still written"
        );
    }
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(
        back.report.layers,
        before.0 - purged.layers.len() - 1,
        "less layer 0"
    );
    assert_eq!(
        back.report.linetypes,
        before.1 - purged.linetypes.len() - 1,
        "less Continuous"
    );
    assert_eq!(back.drawing.layers().len(), drawing.layers().len());
    assert_eq!(back.drawing.linetypes().len(), drawing.linetypes().len());
    // Nothing left to purge in what was written.
    let mut again = back.drawing;
    let mut tx = again.transaction();
    assert!(tx.purge(PurgeScope::ALL).is_empty());
    drop(tx);
}

/// #198: points from another writer — on layer 0, on a layer of their
/// own, coloured — with the header's point style; a style set here is
/// written as `$PDMODE` and `$PDSIZE` and read back; a size relative to
/// the screen is not held, and the report says so when the file has
/// points to show at it.
#[test]
fn points_from_another_writer_and_the_point_style_round_trip() {
    let import = load("valid", "ezdxf-r2010-points.dxf").unwrap();
    assert_eq!(import.report.points, 3);
    assert_eq!(import.report.lines, 1);
    assert!(!import.report.has_unsupported_content());
    let drawing = &import.drawing;
    assert_eq!(drawing.settings().point_style, PointStyle::new(35, 2.5));
    let points: Vec<(Point2, String)> = drawing
        .shown()
        .filter_map(|(id, entity)| match entity {
            Entity::Point(point) => Some((
                point.position,
                drawing
                    .layer_of(id)
                    .map(|layer| layer.name.clone())
                    .unwrap_or_default(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        points,
        vec![
            (Point2::new(10.0, 10.0), "0".to_owned()),
            (Point2::new(20.0, 5.0), "NODES".to_owned()),
            (Point2::new(30.5, -7.25), "0".to_owned()),
        ]
    );
    let text = export(drawing);
    assert!(
        text.contains("\n  9\n$PDMODE\n 70\n35\n  9\n$PDSIZE\n 40\n2.5\n"),
        "the style"
    );
    assert!(text.contains("\n100\nAcDbPoint\n 10\n20.0\n 20\n5.0\n"));

    // A relative size: the default stands, and the report says so.
    let relative = "  0\nSECTION\n  2\nHEADER\n  9\n$PDMODE\n 70\n3\n  9\n$PDSIZE\n 40\n-2.0\n  0\nENDSEC\n  0\nSECTION\n  2\nENTITIES\n  0\nPOINT\n  8\n0\n 10\n1.0\n 20\n2.0\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(relative.as_bytes()).unwrap();
    assert_eq!(
        import.drawing.settings().point_style,
        PointStyle::new(3, PointStyle::DEFAULT_SIZE)
    );
    assert!(import.report.point_size_relative);
    assert!(
        import
            .report
            .summary()
            .iter()
            .any(|line| line.starts_with("The points' size is relative to the screen")),
        "{:?}",
        import.report.summary()
    );
    assert!(
        !import.report.has_unsupported_content(),
        "not lossy: the size written back is the default"
    );
    let again = export(&import.drawing);
    assert!(again.contains("\n$PDMODE\n 70\n3\n") && !again.contains("$PDSIZE"));
    // A relative size with no points is not worth a word.
    let silent = "  0\nSECTION\n  2\nHEADER\n  9\n$PDSIZE\n 40\n0.0\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(silent.as_bytes()).unwrap();
    assert!(
        !import
            .report
            .summary()
            .iter()
            .any(|line| line.contains("relative"))
    );
    // An unknown mode is read as the default.
    let unknown = "  0\nSECTION\n  2\nHEADER\n  9\n$PDMODE\n 70\n7\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(unknown.as_bytes()).unwrap();
    assert_eq!(import.drawing.settings().point_style, PointStyle::default());
}

/// #194: a layout's sheet, set here, is the paper size its LAYOUT object
/// carries (44, 45) — in the sheet the writer makes for a layout made
/// here, and in the kept groups of a layout from a file — and is read
/// back as the sheet.
#[test]
fn a_layouts_sheet_set_here_is_written_as_its_paper_size_and_read_back() {
    let a4 = Vec2::new(297.0, 210.0);
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
    tx.set_layout_sheet(sheet, a4).unwrap();
    tx.commit();
    let text = export(&drawing);
    assert!(text.contains("\n 44\n297.0\n 45\n210.0\n"), "the sheet");
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    let (id, row) = back.drawing.layouts().iter().nth(1).unwrap();
    assert_eq!(row.sheet_size(), a4);
    assert_eq!(back.drawing.layouts().get(id).unwrap().paper_size, None);
    assert_eq!(export(&back.drawing), text, "stable on the second pass");

    let mut imported = load("valid", "layouts.dxf").unwrap().drawing;
    let (one, row) = imported.layouts().iter().nth(1).unwrap();
    assert_eq!(row.sheet_size(), Vec2::new(420.0, 297.0));
    let mut tx = imported.transaction();
    tx.set_layout_sheet(one, a4).unwrap();
    tx.commit();
    let text = export(&imported);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    let (_, row) = back.drawing.layouts().iter().nth(1).unwrap();
    assert_eq!(row.sheet_size(), a4, "the kept groups carry the sheet");
    let (_, other) = back.drawing.layouts().iter().nth(2).unwrap();
    assert_eq!(other.sheet_size(), Vec2::new(420.0, 297.0));
}

/// #191: an entity the ENTITIES section flags as paper (67) — as R12 and
/// some writers put the active layout's — is the active layout's, not the
/// model's; a paper block no LAYOUT object names is a layout of its own.
#[test]
fn a_paper_flag_in_the_entities_section_puts_the_entity_on_the_active_layout() {
    let text = "  0\nSECTION\n  2\nENTITIES\n  0\nLINE\n  8\n0\n 67\n1\n 10\n0.0\n 20\n0.0\n 11\n100.0\n 21\n0.0\n  0\nLINE\n  8\n0\n 10\n0.0\n 20\n0.0\n 11\n50.0\n 21\n0.0\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(text.as_bytes()).unwrap();
    let drawing = &import.drawing;
    let names: Vec<String> = drawing
        .layouts()
        .iter()
        .map(|(_, l)| l.name.clone())
        .collect();
    assert_eq!(names, ["Model", "Layout1"]);
    let (sheet, _) = drawing.layouts().iter().nth(1).unwrap();
    assert_eq!(drawing.shown().count(), 1);
    assert_eq!(drawing.shown_in(sheet).count(), 1);
    assert_eq!(import.report.layouts, vec!["Layout1"]);
    assert_eq!(import.report.lines, 2);
    let written = export(drawing);
    let back = cad_dxf::import(written.as_bytes()).unwrap();
    assert_eq!(back.drawing.shown_in(sheet).count(), 1);
    assert_eq!(export(&back.drawing), written);
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
fn attributes_from_another_writer() {
    let import = load("valid", "ezdxf-r2010-block-attributes.dxf").unwrap();
    let drawing = &import.drawing;
    let tag = drawing.blocks().find("TAG").expect("the tag");
    let records = &drawing.blocks().get(tag).unwrap().entities;
    assert_eq!(records.len(), 2, "the frame and the ATTDEF (#183)");
    assert!(matches!(&*records[0].entity, Entity::Polyline(polyline) if polyline.closed));
    let Entity::AttributeDefinition(definition) = &*records[1].entity else {
        panic!("{:?}", records[1].entity);
    };
    assert_eq!(definition.tag, "ROOM");
    let placed = references(drawing);
    assert_eq!(placed.len(), 2);
    assert_eq!(placed[0].attribute("ROOM"), Some("101"));
    assert_eq!(
        placed[1].attribute("ROOM"),
        None,
        "the second shows the default"
    );
    assert!((placed[1].rotation.to_degrees() - 45.0).abs() < 1e-9);
    // Placed, the first shows its value and the second the default, as texts.
    let shown = |reference: &cad_document::BlockReferenceEntity| -> Vec<String> {
        drawing
            .blocks()
            .flatten(reference, Properties::default())
            .into_iter()
            .filter_map(|(entity, _)| match entity {
                Entity::Text(text) => Some(text.content),
                _ => None,
            })
            .collect()
    };
    assert_eq!(shown(&placed[0]), vec!["101"]);
    assert_eq!(shown(&placed[1]), vec![""]);

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
    assert_eq!(counts, vec![]);
    // Written back, the reference carries its ATTRIB and reads the same.
    let text = export(drawing);
    assert!(text.contains("\nATTRIB\n"));
    assert!(text.contains("\nATTDEF\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(references(&back.drawing)[0].attribute("ROOM"), Some("101"));
    assert_eq!(export(&back.drawing), text);
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

/// Text styles and texts from another writer (#221): a style's fixed
/// height, width factor, and slant; a value no text can be drawn by,
/// reported; `$TEXTSTYLE`; each text's own width and slant, a slant of
/// 345° read as −15°; and the same back.
#[test]
fn text_styles_and_their_texts_from_another_writer_round_trip() {
    let import = load("valid", "ezdxf-r2010-text-styles.dxf").unwrap();
    assert_eq!(
        import.report.unsupported.get("(text style values)"),
        Some(&1)
    );
    let drawing = &import.drawing;
    let styles = drawing.text_styles();
    let narrow = styles.find("NARROW").unwrap();
    let style = styles.get(narrow).unwrap();
    assert_eq!((style.height, style.width_factor), (3.5, 0.7));
    assert!((style.oblique.to_degrees() - 15.0).abs() < 1e-9);
    assert_eq!(
        styles
            .get(styles.find("BAD").unwrap())
            .unwrap()
            .width_factor,
        1.0
    );
    assert_eq!(drawing.current_text_style(), narrow, "$TEXTSTYLE");
    let texts: Vec<(f64, f64)> = drawing
        .shown()
        .filter_map(|(_, entity)| match entity {
            Entity::Text(text) => Some((text.width_factor, text.oblique.to_degrees())),
            _ => None,
        })
        .collect();
    assert_eq!(texts.len(), 3);
    assert!((texts[0].0 - 0.7).abs() < 1e-12 && (texts[0].1 - 15.0).abs() < 1e-9);
    assert!((texts[1].0 - 1.5).abs() < 1e-12 && texts[1].1.abs() < 1e-12);
    assert!((texts[2].1 + 15.0).abs() < 1e-9, "345° leans back");

    let text = export(drawing);
    assert!(text.contains("\n  9\n$TEXTSTYLE\n  7\nNARROW\n"));
    // The slant goes through radians and back: within an ulp of 15°.
    assert!(text.contains("\n  2\nNARROW\n 70\n0\n 40\n3.5\n 41\n0.7\n 50\n1"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// Groups from another writer (#214): named, unnamed, and not selectable,
/// with their descriptions and their members among the drawing's
/// entities; one whose only object the drawing cannot hold is reported;
/// written back as GROUP objects in `ACAD_GROUP`, and the same again.
#[test]
fn groups_from_another_writer_keep_their_members_and_round_trip() {
    let import = load("valid", "ezdxf-r2010-groups.dxf").unwrap();
    let report = &import.report;
    assert_eq!(report.groups, 3);
    assert_eq!(report.unsupported.get("(group without objects)"), Some(&1));
    assert!(
        report
            .summary()
            .iter()
            .any(|line| line.contains("3 groups"))
    );
    let drawing = &import.drawing;
    let groups: Vec<&Group> = drawing.groups().iter().map(|(_, group)| group).collect();
    let kinds = |group: &Group| -> Vec<&'static str> {
        group
            .members
            .iter()
            .map(|id| drawing.entity(*id).unwrap().kind_name())
            .collect()
    };
    assert_eq!(groups[0].name, "DOOR");
    assert_eq!(groups[0].description, "The front door");
    assert_eq!(kinds(groups[0]), ["line", "arc"]);
    assert!(groups[1].is_unnamed());
    assert_eq!(kinds(groups[1]), ["polyline", "line"]);
    assert_eq!(groups[2].name, "NOTE");
    assert!(!groups[2].selectable);
    assert_eq!(kinds(groups[2]), ["text"]);

    let text = export(drawing);
    assert!(text.contains("\n  3\nDOOR\n350\n"));
    assert!(text.contains("\n100\nAcDbGroup\n300\nThe front door\n 70\n0\n 71\n1\n340\n"));
    assert!(text.contains("\n100\nAcDbGroup\n300\n\n 70\n0\n 71\n0\n340\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.groups, 3);
    assert_eq!(
        back.drawing
            .groups()
            .iter()
            .map(|(_, g)| g.clone())
            .collect::<Vec<_>>(),
        drawing
            .groups()
            .iter()
            .map(|(_, g)| g.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// Frozen and off layers from another writer (#213): frozen is frozen,
/// off is off, and a layer both stays both; none of their lines is shown;
/// each comes back as it went.
#[test]
fn frozen_and_off_layers_from_another_writer_round_trip_as_they_were() {
    let import = load("valid", "ezdxf-r2010-frozen-layers.dxf").unwrap();
    let drawing = &import.drawing;
    let layer = |name: &str| {
        let layers = drawing.layers();
        let layer = layers.get(layers.find(name).unwrap()).unwrap();
        (layer.on, layer.frozen)
    };
    assert_eq!(layer("FROZEN"), (true, true));
    assert_eq!(layer("OFF"), (false, false));
    assert_eq!(layer("BOTH"), (false, true));
    assert_eq!(layer("SHOWN"), (true, false));
    assert_eq!(drawing.shown().count(), 1, "only the line on SHOWN");

    let text = export(drawing);
    assert!(text.contains("\n  2\nFROZEN\n 70\n1\n 62\n1\n"));
    assert!(text.contains("\n  2\nOFF\n 70\n0\n 62\n-3\n"));
    assert!(text.contains("\n  2\nBOTH\n 70\n1\n 62\n-5\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// A baseline chain from another writer (#212): its style's `DIMDLI` is
/// the baseline spacing, and each dimension stands where chaining the one
/// before it by that spacing would put it; the spacing round-trips.
#[test]
fn a_baseline_chain_from_another_writer_is_spaced_by_its_style() {
    let import = load("valid", "ezdxf-r2010-dimension-chains.dxf").unwrap();
    let drawing = &import.drawing;
    let chain = drawing.dim_styles().find("CHAIN").unwrap();
    assert_eq!(
        drawing.dim_styles().get(chain).unwrap().baseline_spacing,
        7.0
    );
    assert_eq!(
        drawing
            .dim_styles()
            .get(DimStyleId::FIRST)
            .unwrap()
            .baseline_spacing,
        3.75,
        "ezdxf's Standard has the classic spacing"
    );
    let dimensions: Vec<&DimensionEntity> = drawing
        .shown()
        .filter_map(|(_, entity)| match entity {
            Entity::Dimension(dimension) => Some(dimension),
            _ => None,
        })
        .collect();
    assert_eq!(dimensions.len(), 3);
    for pair in dimensions.windows(2) {
        let [before, after] = pair else {
            unreachable!()
        };
        let to = after.measure.points()[1];
        let chained = before.chained(0, to, Chain::Baseline(7.0)).unwrap();
        assert_eq!(chained.measure, after.measure);
        assert!(
            (chained.line_point.y - after.line_point.y).abs() < 1e-9,
            "{chained:?} against {after:?}"
        );
    }

    let text = export(drawing);
    assert!(text.contains("\n  2\nCHAIN\n"));
    assert!(text.contains("\n 43\n7.0\n"));
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
        "AcDbDimStyleTableRecord\n  2\nARCH\n 70\n0\n  3\n<> mm\n 41\n2.0\n 42\n0.5\n 43\n3.75\n 44\n1.5\n140\n3.5\n147\n1.0\n271\n1\n"
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

/// DIMSCALE from another writer (#327): ezdxf's `CM` style scales its
/// sizes by 4, so a 1200-unit dimension's text is 10 high, not 2.5; the
/// value measured is not scaled; written back and read the same.
#[test]
fn a_dimension_style_scale_from_another_writer_scales_its_sizes() {
    let import = load("valid", "ezdxf-r2010-dimscale.dxf").unwrap();
    let drawing = &import.drawing;
    let styles = drawing.dim_styles();
    let cm = styles.get(styles.find("CM").unwrap()).unwrap();
    assert_eq!((cm.scale, cm.text_height, cm.arrow_size), (4.0, 2.5, 2.5));
    assert_eq!(styles.get(DimStyleId::FIRST).unwrap().scale, 1.0);
    let dimension = drawing
        .entities()
        .find_map(|(_, entity)| match entity {
            cad_document::Entity::Dimension(dimension) => Some(dimension),
            _ => None,
        })
        .expect("the dimension");
    let layout = dimension.layout(drawing.tables().dim_style(dimension.style));
    assert_eq!(layout.text.height, 10.0);
    assert!(
        (layout.value - 1200.0).abs() < 1e-9,
        "the value is not scaled"
    );
    assert_eq!(import.report.dim_styles, 1, "CM: Standard restyles ours");
    let text = export(drawing);
    assert!(
        text.contains("  2\nCM\n 70\n0\n 40\n4.0\n"),
        "DIMSCALE written as group 40"
    );
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    let again = back.drawing.dim_styles();
    assert_eq!(again.get(again.find("CM").unwrap()).unwrap().scale, 4.0);
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
    assert_eq!(dimensions.len(), 9);
    let kinds: Vec<&str> = dimensions
        .iter()
        .map(|dimension| dimension.measure.kind())
        .collect();
    assert_eq!(
        kinds,
        [
            "linear", "linear", "linear", "angular", "angular", "radius", "diameter", "linear",
            "ordinate"
        ]
    );
    let values: Vec<f64> = dimensions
        .iter()
        .map(|d| (d.value() * 1e6).round() / 1e6)
        .collect();
    assert_eq!(
        values,
        [100.0, 50.0, 40.0, 90.0, 90.0, 30.0, 60.0, 60.0, 400.0]
    );

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
    assert_eq!(report.dimensions, 9);
    assert_eq!(
        report.unsupported.get("(ordinate DIMENSION)"),
        None,
        "the ordinate is read since #227"
    );
    assert_eq!(report.summary()[0], "Imported 9 dimensions.");

    // Our file: a block record and an anonymous block per dimension, named
    // by each DIMENSION, holding what the style laid out.
    let text = export(drawing);
    for n in 1..=9 {
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
    assert_eq!(back.report.dimensions, 9);
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
    assert_eq!(hatches.len(), 7, "the spline-bounded one too, since #182");
    let report = &import.report;
    assert_eq!(report.hatches, 7);
    assert_eq!(
        report.unsupported.get("(HATCH boundary with an ellipse)"),
        None
    );
    for key in ["(gradient HATCH)", "(HATCH pattern not in the table)"] {
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
    // Bounded by a spline edge and a line (#182): the spline read as its chords.
    assert_eq!(hatches[4].style, HatchStyle::Solid);
    assert_eq!(hatches[4].loops.len(), 1);
    assert!(
        hatches[4].contains(Point2::new(850.0, 20.0)),
        "under the spline's arch"
    );
    // The gradient and the pattern the table lacks are filled solid.
    assert_eq!(hatches[5].style, HatchStyle::Solid);
    assert_eq!(hatches[6].style, HatchStyle::Solid);

    let text = export(&import.drawing);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.hatches, 7);
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

/// #192: a viewport made on a layout here is written and read back as it
/// was; one the ENTITIES section puts in model space has no place in the
/// drawing and is reported.
#[test]
fn a_viewport_round_trips_and_one_in_the_model_is_reported() {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let sheet = tx.add_layout(Layout::named("Sheet")).unwrap();
    let mut window = ViewportEntity::new(
        Point2::new(150.0, 100.0),
        200.0,
        120.0,
        Point2::new(1234.5, -67.25),
        0.02,
    );
    window.on = false;
    tx.insert_in(sheet, window).unwrap();
    tx.commit();
    let text = export(&drawing);
    assert!(text.contains("\nVIEWPORT\n"));
    assert!(
        text.contains("\n 45\n6000.0\n"),
        "120 mm at 1:50 is 6000 units"
    );
    assert!(text.contains("\n 68\n0\n"), "off");
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.viewports, 1);
    let (id, _) = back.drawing.layouts().iter().nth(1).unwrap();
    let read = back
        .drawing
        .shown_in(id)
        .find_map(|(_, entity)| match entity {
            Entity::Viewport(viewport) => Some(*viewport),
            _ => None,
        })
        .unwrap();
    assert_eq!(read, window);
    assert_eq!(export(&back.drawing), text, "stable on the second pass");

    let tiled = "  0\nSECTION\n  2\nENTITIES\n  0\nVIEWPORT\n  8\n0\n 10\n0.0\n 20\n0.0\n 40\n100.0\n 41\n50.0\n 45\n50.0\n 68\n1\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(tiled.as_bytes()).unwrap();
    assert_eq!(import.drawing.len(), 0);
    assert_eq!(import.report.unsupported.get("VIEWPORT"), Some(&1));
    let flat = "  0\nSECTION\n  2\nENTITIES\n  0\nVIEWPORT\n  8\n0\n 67\n1\n 10\n0.0\n 20\n0.0\n 40\n100.0\n 41\n0.0\n  0\nENDSEC\n  0\nEOF\n";
    let import = cad_dxf::import(flat.as_bytes()).unwrap();
    assert_eq!(
        import.report.unsupported.get("(invalid geometry)"),
        Some(&1)
    );
}

/// The drawing order and a wipeout from another writer (#226): a
/// SORTENTSTABLE sending a hatch to the back and a note to the front, a
/// WIPEOUT drawn after the lines it hides, a note after it, and wipeout
/// frames off. The tiers paint them as the file does; written back, the
/// records are in the order they are painted, which needs no table, and
/// the wipeout is the same polygon.
#[test]
fn the_drawing_order_and_a_wipeout_from_another_writer_round_trip() {
    let import = load("valid", "ezdxf-r2010-draw-order.dxf").unwrap();
    assert_eq!(import.report.wipeouts, 1);
    assert!(
        !import.report.has_unsupported_content(),
        "{:?}",
        import.report
    );
    assert!(
        import
            .report
            .summary()
            .iter()
            .any(|line| line.contains("1 wipeout"))
    );
    let drawing = &import.drawing;
    assert!(!drawing.settings().wipeout_frame, "frames off in the file");
    let records: Vec<(&'static str, i16)> = {
        let mut ids: Vec<_> = drawing.records().map(|(id, _)| id).collect();
        ids.sort_unstable();
        ids.into_iter()
            .map(|id| {
                let record = drawing.record(id).unwrap();
                (record.entity.kind_name(), record.order)
            })
            .collect()
    };
    assert_eq!(
        records,
        [
            ("line", 0),
            ("hatch", 0),
            ("text", 2),
            ("line", 0),
            ("wipeout", 1),
            ("text", 1),
        ],
        "the hatch beneath the lines, the wipeout over them, a note over it, NOTE over all"
    );
    let wipeout = drawing
        .entities()
        .find_map(|(_, entity)| match entity {
            Entity::Wipeout(wipeout) => Some(wipeout.clone()),
            _ => None,
        })
        .unwrap();
    assert_eq!(
        wipeout.vertices,
        [
            Point2::new(60.0, -5.0),
            Point2::new(90.0, -5.0),
            Point2::new(90.0, 5.0),
            Point2::new(60.0, 5.0)
        ]
    );

    let text = export(drawing);
    assert!(text.contains("\n  0\nCLASS\n  1\nWIPEOUT\n"));
    assert!(text.contains("\n100\nAcDbWipeoutVariables\n 70\n0\n"));
    assert!(!text.contains("SORTENTSTABLE"), "the order is the records'");
    let section = &text[text.find("ENTITIES").unwrap()..text.find("OBJECTS").unwrap()];
    let written: Vec<&str> = section
        .split("\n  0\n")
        .skip(1)
        .filter_map(|record| record.lines().next())
        .filter(|kind| !matches!(*kind, "ENDSEC" | "SECTION"))
        .collect();
    assert_eq!(
        written,
        ["HATCH", "LINE", "LINE", "WIPEOUT", "TEXT", "TEXT"],
        "in the order they are painted"
    );
    let back = import_back(&text);
    assert_eq!(painted(&back), painted(drawing), "painted as before");
    assert_eq!(export(&back), text, "export → import → export");

    // Nothing reordered, nothing written: no table, no class.
    let plain = load("valid", "ezdxf-r2010-mixed-content.dxf").unwrap();
    let text = export(&plain.drawing);
    assert!(!text.contains("SORTENTSTABLE"));
    assert!(!text.contains("WIPEOUT"));
}

/// The kinds of a drawing's entities, in the order they are painted.
fn painted(drawing: &Drawing) -> Vec<&'static str> {
    let mut keys: Vec<_> = drawing
        .records()
        .map(|(id, record)| {
            (
                cad_document::draw_order::paint_key(id, record.order, record.entity.is_fill()),
                record.entity.kind_name(),
            )
        })
        .collect();
    keys.sort_unstable_by_key(|(key, _)| *key);
    keys.into_iter().map(|(_, kind)| kind).collect()
}

fn import_back(text: &str) -> Drawing {
    import(text.as_bytes()).unwrap().drawing
}

/// Ordinate dimensions from another writer (#227): a plate dimensioned
/// from its corner, four X ordinates and three Y, one leader jogged. Each
/// is read with its datum (10), feature (13), leader end (14), and axis
/// (70, 64), measures what the file says, and comes back the same.
#[test]
fn ordinate_dimensions_from_another_writer_round_trip() {
    use cad_document::Measure;
    let import = load("valid", "ezdxf-r2010-ordinate-dimensions.dxf").unwrap();
    assert_eq!(import.report.dimensions, 7);
    assert_eq!(import.report.unsupported.get("(ordinate DIMENSION)"), None);
    let ordinates: Vec<(bool, f64, Point2)> = in_file_order(&import.drawing)
        .into_iter()
        .filter_map(|entity| match entity {
            Entity::Dimension(dimension) => match dimension.measure {
                Measure::Ordinate { origin, x, .. } => {
                    Some((x, (dimension.value() * 1e6).round() / 1e6, origin))
                }
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        ordinates
            .iter()
            .map(|(x, value, _)| (*x, *value))
            .collect::<Vec<_>>(),
        [
            (true, 0.0),
            (true, 30.0),
            (true, 75.0),
            (true, 100.0),
            (false, 0.0),
            (false, 30.0),
            (false, 40.0)
        ]
    );
    assert!(
        ordinates
            .iter()
            .all(|(_, _, origin)| *origin == Point2::new(50.0, 20.0)),
        "measured from the plate's corner"
    );

    let text = export(&import.drawing);
    assert!(text.contains("\n100\nAcDbOrdinateDimension\n 13\n"));
    assert!(text.contains("\n 70\n230\n"), "an X: 6, 32, 64, 128");
    assert!(text.contains("\n 70\n166\n"), "a Y: 6, 32, 128");
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert!(!back.report.has_unsupported_content());
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// Layer states from another writer (#228): `Plan` and `Ceiling` in the
/// LAYER table's `ACAD_LAYERSTATES` dictionary, their layers by handle —
/// and one the drawing lacks, by name — with switches, colours,
/// lineweights, and linetypes by handle. Restored, the layers are as the
/// state keeps them; written and read back, the states are the same.
#[test]
fn layer_states_from_another_writer_restore_and_round_trip() {
    let import = load("valid", "ezdxf-r2010-layer-states.dxf").unwrap();
    assert_eq!(import.report.layer_states, 2);
    assert!(
        import
            .report
            .summary()
            .iter()
            .any(|line| line.contains("2 layer states"))
    );
    let mut drawing = import.drawing;
    let states = drawing.layer_states();
    let plan = states.get(states.find("Plan").unwrap()).unwrap().clone();
    assert_eq!(plan.description, "Floor plan");
    let walls = plan.layer("walls").unwrap();
    assert_eq!(
        (
            walls.on,
            walls.color,
            walls.linetype.as_str(),
            walls.lineweight
        ),
        (true, Color::Index(1), "DASHED", Lineweight::Hundredths(50))
    );
    assert!(!plan.layer("CEILING").unwrap().on);
    let ceiling = states.get(states.find("Ceiling").unwrap()).unwrap().clone();
    let furniture = ceiling.layer("FURNITURE").unwrap();
    assert!(!furniture.on && furniture.frozen);
    assert!(ceiling.layer("WALLS").unwrap().locked);
    assert!(ceiling.layer("GONE").is_some(), "named by name, kept");

    let id = drawing.layer_states().find("Ceiling").unwrap();
    let mut tx = drawing.transaction();
    assert_eq!(tx.restore_layer_state(id), Ok(3));
    tx.commit();
    let layer = |name: &str| {
        let layers = drawing.layers();
        layers.get(layers.find(name).unwrap()).unwrap().clone()
    };
    assert!(layer("CEILING").on);
    assert!(layer("FURNITURE").frozen);
    assert_eq!(layer("WALLS").color, Color::Index(8));
    assert_eq!(layer("WALLS").linetype, Linetype::CONTINUOUS);
    assert!(layer("WALLS").locked);

    let text = export(&drawing);
    assert!(text.contains("\n  3\nACAD_LAYERSTATES\n360\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert!(!back.report.has_unsupported_content());
    let names = |drawing: &Drawing| -> Vec<cad_document::LayerState> {
        drawing
            .layer_states()
            .iter()
            .map(|(_, state)| state.clone())
            .collect()
    };
    assert_eq!(names(&back.drawing), names(&drawing));
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// Named views from another writer (#235): two of the model and one
/// flagged as paper, which goes to the first layout; written back the
/// same.
#[test]
fn named_views_from_another_writer_round_trip() {
    let import = load("valid", "ezdxf-r2010-named-views.dxf").unwrap();
    assert_eq!(import.report.views, 3);
    assert!(
        import
            .report
            .summary()
            .iter()
            .any(|line| line.contains("3 named views"))
    );
    let drawing = &import.drawing;
    let views = drawing.views();
    let detail = views.get(views.find("detail a").unwrap()).unwrap();
    assert_eq!(
        (detail.center, detail.width, detail.height, detail.space),
        (Point2::new(120.0, 80.0), 120.0, 90.0, LayoutId::MODEL)
    );
    let title = views.get(views.find("Title").unwrap()).unwrap();
    assert_ne!(
        title.space,
        LayoutId::MODEL,
        "a paper view, on the first layout"
    );

    let text = export(drawing);
    assert!(text.contains("AcDbViewTableRecord\n  2\nDetail A\n 70\n0\n 40\n90.0\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.views, 3);
    assert!(!back.report.has_unsupported_content());
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

#[test]
fn multileaders_from_another_writer_round_trip() {
    let import = load("valid", "ezdxf-r2010-multileaders.dxf").unwrap();
    let report = &import.report;
    assert_eq!((report.mleaders, report.mleader_styles), (3, 2));
    assert_eq!(
        report.unsupported.keys().collect::<Vec<_>>(),
        ["SOLID"],
        "only the arrowhead block's solid: every multileader is held"
    );
    let summary = report.summary();
    assert!(summary.iter().any(|line| line.contains("3 multileaders")));
    assert!(
        summary
            .iter()
            .any(|line| line.contains("2 multileader styles"))
    );
    let drawing = &import.drawing;
    let styles = drawing.mleader_styles();
    let callout = styles.find("Callout").unwrap();
    assert_eq!(
        styles.get(callout),
        Some(&MLeaderStyle {
            arrow_size: 3.0,
            landing_gap: 1.5,
            dogleg_length: 6.0,
            text_height: 3.5,
            ..MLeaderStyle::named("Callout")
        })
    );
    let standard = styles.get(MLeaderStyleId::FIRST).unwrap();
    assert_eq!(
        (standard.arrow_size, standard.dogleg_length),
        (4.0, 8.0),
        "the file's Standard replaces the drawing's"
    );
    assert_eq!(drawing.settings().current_mleader_style, callout);
    // A style whose new notes are a block, named through its record (343).
    let balloons = styles.get(styles.find("Balloon").unwrap()).unwrap();
    assert_eq!(
        (balloons.block.as_deref(), balloons.block_scale),
        (Some("BALLOON"), 1.5)
    );
    let mleaders: Vec<(MLeaderEntity, String)> = drawing
        .records()
        .filter_map(|(id, record)| match &*record.entity {
            Entity::MLeader(mleader) => {
                Some((mleader.clone(), drawing.layer_of(id).unwrap().name.clone()))
            }
            _ => None,
        })
        .collect();
    let (hole, layer) = &mleaders[0];
    assert_eq!(layer, "NOTES");
    assert_eq!(
        hole.leaders,
        vec![
            vec![Point2::new(0.0, 0.0)],
            vec![Point2::new(5.0, -10.0), Point2::new(10.0, 0.0)],
        ]
    );
    assert_eq!(hole.landing, Point2::new(10.0, 8.0));
    assert_eq!(hole.content, MLeaderContent::Text("Hole".to_owned()));
    assert_eq!(hole.style, MLeaderStyleId::FIRST);
    // The note sits where the other writer put it: past the dogleg and
    // the gap, its middle on the dogleg's line.
    let laid_out = hole.layout(standard, drawing.tables());
    assert_eq!(laid_out.note_at, Point2::new(20.0, 8.0));
    let (valve, _) = &mleaders[1];
    assert_eq!(valve.style, callout);
    assert_eq!(valve.content, MLeaderContent::Text("VALVE".to_owned()));
    assert!(valve.reads() < 0.0, "a leader from the right reads left");
    // The balloon is its block, placed where the other writer put it: its
    // left side's middle at the dogleg's end (#241).
    let (balloon, _) = &mleaders[2];
    let block = drawing.blocks().find("BALLOON").unwrap();
    assert_eq!(balloon.content, MLeaderContent::Block { block, scale: 1.0 });
    let (reference, _) = balloon.layout(standard, drawing.tables()).block.unwrap();
    assert_eq!(reference.insertion, Point2::new(20.0, 60.0));

    let text = export(drawing);
    assert!(text.contains("  0\nCLASS\n  1\nMULTILEADER\n"));
    assert!(text.contains("  3\nACAD_MLEADERSTYLE\n"));
    assert!(text.contains("  9\n$CMLEADERSTYLE\n340\n"));
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!((back.report.mleaders, back.report.mleader_styles), (3, 2));
    assert!(!back.report.has_unsupported_content());
    assert_eq!(back.drawing.settings().current_mleader_style, callout);
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// A fuzzer finding (2026-10-19): a saved layer with no linetype record, or
/// one naming a handle no row has, has no linetype — a restore leaves the
/// layer's own — and stays that way through a round trip. Defaulting it to
/// `Continuous` made the second export differ from the first.
#[test]
fn a_saved_layer_without_a_linetype_keeps_none_and_round_trips() {
    let import = load("valid", "layer-state-without-linetype.dxf").unwrap();
    let mut drawing = import.drawing;
    let states = drawing.layer_states();
    let plan = states.get(states.find("Plan").unwrap()).unwrap().clone();
    assert_eq!(plan.layer("WALLS").unwrap().linetype, "DASHED");
    assert_eq!(plan.layer("CEILING").unwrap().linetype, "", "no 331 at all");
    assert_eq!(
        plan.layer("FURNITURE").unwrap().linetype,
        "",
        "331 names no row"
    );

    let linetype_of = |drawing: &Drawing, name: &str| {
        let layers = drawing.layers();
        layers.get(layers.find(name).unwrap()).unwrap().linetype
    };
    let before = (
        linetype_of(&drawing, "CEILING"),
        linetype_of(&drawing, "FURNITURE"),
    );
    let id = drawing.layer_states().find("Plan").unwrap();
    let mut tx = drawing.transaction();
    tx.restore_layer_state(id).unwrap();
    tx.commit();
    assert_eq!(
        (
            linetype_of(&drawing, "CEILING"),
            linetype_of(&drawing, "FURNITURE")
        ),
        before,
        "a restore leaves the layer's own linetype"
    );

    let text = export(&drawing);
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(export(&back.drawing), text, "export → import → export");
}

/// #271: our constraints survive a save — every kind, every part, a
/// dimension's name, value, and place, a fix's place, a tangent's touch —
/// in XRECORDs of the `CADSLATE_CONSTRAINTS` dictionary, naming their
/// entities by handle; export → import → export is byte-identical.
#[test]
fn constraints_round_trip_through_their_xrecords() {
    use cad_document::{
        ArcEntity, Axis, CircleEntity, Constraint, Kind, LineEntity, Part, PointEntity,
        PolylineEntity, Reference, Touch,
    };
    let mut drawing = cad_document::Drawing::new();
    let mut tx = drawing.transaction();
    let a = tx
        .insert(LineEntity::new(
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
        ))
        .unwrap();
    let b = tx
        .insert(LineEntity::new(
            Point2::new(100.0, 0.0),
            Point2::new(100.0, 50.0),
        ))
        .unwrap();
    let c = tx
        .insert(CircleEntity::new(Point2::new(50.0, 25.0), 10.0))
        .unwrap();
    let d = tx
        .insert(ArcEntity::new(Point2::new(50.0, 25.0), 20.0, 0.0, 2.0))
        .unwrap();
    let e = tx
        .insert(PolylineEntity::new(
            vec![
                Point2::new(0.0, 80.0),
                Point2::new(40.0, 80.0),
                Point2::new(40.0, 120.0),
            ],
            false,
        ))
        .unwrap();
    let f = tx
        .insert(PointEntity::new(Point2::new(-20.0, -20.0)))
        .unwrap();
    tx.commit();
    let at = Reference::new;
    let constraints = [
        Constraint::geometric(Kind::Coincident, vec![at(a, Part::End), at(b, Part::Start)]),
        Constraint::geometric(Kind::Horizontal, vec![at(a, Part::Edge)]),
        Constraint::geometric(Kind::Vertical, vec![at(b, Part::Edge)]),
        Constraint::geometric(Kind::Concentric, vec![at(c, Part::Edge), at(d, Part::Edge)]),
        Constraint::geometric(Kind::Horizontal, vec![at(e, Part::Segment(0))]),
        Constraint::geometric(
            Kind::Vertical,
            vec![at(e, Part::Vertex(1)), at(e, Part::Vertex(2))],
        ),
        Constraint::geometric(
            Kind::Fix {
                at: Point2::new(-20.0, -20.0),
            },
            vec![at(f, Part::Point)],
        ),
        Constraint::geometric(
            Kind::Tangent { touch: Touch::Left },
            vec![at(a, Part::Edge), at(c, Part::Edge)],
        )
        .placed(Point2::new(1.0, 2.0)),
        Constraint::dimensional(
            "d1",
            Kind::Distance {
                axis: Axis::Horizontal,
                value: 100.0,
            },
            vec![at(a, Part::Start), at(a, Part::End)],
        )
        .placed(Point2::new(50.0, -15.25)),
        Constraint::dimensional(
            "rad1",
            Kind::Radius { value: 10.0 },
            vec![at(c, Part::Edge)],
        ),
        Constraint::dimensional(
            "ang1",
            Kind::Angle { value: 0.5 },
            vec![at(a, Part::Edge), at(e, Part::Segment(1))],
        ),
    ];
    for constraint in constraints {
        drawing.import_constraint(constraint).unwrap();
    }

    let text = export(&drawing);
    // Kept as a golden file, so the DWG writer and every fixture test
    // carry constraints too. Regenerate with `UPDATE_GOLDEN=1`.
    let golden = fixtures("valid").join("cadslate-constraints.dxf");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        fs::write(&golden, &text).unwrap();
    }
    assert_eq!(text, fs::read_to_string(&golden).unwrap(), "export changed");
    assert!(text.contains("\n  3\nCADSLATE_CONSTRAINTS\n350\n"));
    assert!(
        text.contains("\n100\nAcDbXrecord\n280\n1\n  1\nDistance\n  2\nd1\n 70\n0\n 40\n100.0\n")
    );
    let back = cad_dxf::import(text.as_bytes()).unwrap();
    assert_eq!(back.report.constraints, 11);
    assert!(
        back.report
            .summary()
            .iter()
            .any(|line| line.contains("11 constraints"))
    );
    let rows = |drawing: &cad_document::Drawing| -> Vec<Constraint> {
        drawing
            .constraints()
            .iter()
            .map(|(_, c)| c.clone())
            .collect()
    };
    // The entities keep their IDs in a fresh import: the rows come back whole.
    assert_eq!(rows(&back.drawing), rows(&drawing));
    assert_eq!(export(&back.drawing), text, "export → import → export");

    // A reader that does not know them keeps the geometry: removing the
    // dictionary entry leaves the drawing as it was, without constraints.
    let stripped = text.replacen("CADSLATE_CONSTRAINTS", "SOMEONE_ELSES", 1);
    let other = cad_dxf::import(stripped.as_bytes()).unwrap();
    assert!(other.drawing.constraints().is_empty());
    assert_eq!(other.drawing.len(), drawing.len());
}

/// #271: a constraint naming an entity the file does not hold is reported,
/// and the rest are read.
#[test]
fn a_constraint_naming_an_entity_that_is_not_there_is_reported() {
    use cad_document::{Constraint, Kind, LineEntity, Part, Reference};
    let mut drawing = cad_document::Drawing::new();
    let mut tx = drawing.transaction();
    let a = tx
        .insert(LineEntity::new(
            Point2::new(0.0, 0.0),
            Point2::new(100.0, 0.0),
        ))
        .unwrap();
    tx.commit();
    drawing
        .import_constraint(Constraint::geometric(
            Kind::Horizontal,
            vec![Reference::new(a, Part::Edge)],
        ))
        .unwrap();
    let text = export(&drawing);
    // The line's handle, in the constraint's record, made one no entity has.
    let record = text
        .find("\n  1\nHorizontal\n")
        .expect("the constraint's record");
    let at = record + text[record..].find("\n340\n").expect("a reference");
    let end = text[at + 5..].find('\n').unwrap() + at + 5;
    let broken = format!("{}\n340\nFFFFF{}", &text[..at], &text[end..]);
    let back = cad_dxf::import(broken.as_bytes()).unwrap();
    assert!(back.drawing.constraints().is_empty());
    assert_eq!(back.report.unsupported.get("(constraint)"), Some(&1));
    assert_eq!(back.drawing.len(), 1);
}

/// #271: AutoCAD's own constraints — an associative network — are
/// reported, not read; the geometry they hold is read as it is.
#[test]
fn autocad_constraints_are_reported_and_their_geometry_read() {
    let import = load("valid", "libdxf-autocad2010-constraints.dxf").unwrap();
    let report = &import.report;
    assert_eq!(
        report.unsupported.get("ACDBASSOC2DCONSTRAINTGROUP"),
        Some(&1)
    );
    assert_eq!(report.lines, 1);
    assert_eq!(report.constraints, 0);
    let drawing = &import.drawing;
    assert!(drawing.constraints().is_empty());
    let line = drawing
        .entities()
        .next()
        .unwrap()
        .1
        .as_line()
        .copied()
        .unwrap();
    // AutoCAD left it level, as its constraint holds it.
    assert_eq!(line.start.y, line.end.y);
    assert!((line.start.x - 12.839_004_921_216_47).abs() < 1e-12);
    assert!(
        report
            .summary()
            .iter()
            .any(|line| line.contains("1 ACDBASSOC2DCONSTRAINTGROUP")),
        "{:?}",
        report.summary()
    );
}
