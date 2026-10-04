//! Fixture-driven tests (DESIGN §41). See `test-data/dxf/README.md`.

use std::fs;
use std::path::{Path, PathBuf};

use cad_core::Point2;
use cad_document::{Drawing, Entity, LineEntity};
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
    let mut lines: Vec<(u64, LineEntity)> = drawing
        .entities()
        .map(|(id, Entity::Line(line))| (id.raw(), *line))
        .collect();
    lines.sort_by_key(|(id, _)| *id);
    lines.into_iter().map(|(_, line)| line).collect()
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
            "{name}: geometry changed"
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

#[test]
fn every_malformed_fixture_is_rejected_without_panicking() {
    for (name, bytes) in each("malformed") {
        assert!(import(&bytes).is_err(), "{name} should be rejected");
    }
}

// ── specific fixtures ──

#[test]
fn minimal_line() {
    let import = load("valid", "minimal-line.dxf").unwrap();
    assert_eq!(lines(&import.drawing), vec![line(0.0, 0.0, 100.0, 50.0)]);
    assert!(!import.report.has_unsupported_content());
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

    let counts: Vec<(&str, usize)> = import
        .report
        .unsupported
        .iter()
        .map(|(kind, count)| (kind.as_str(), *count))
        .collect();
    assert_eq!(
        counts,
        vec![
            ("ARC", 1),
            ("BLOCK", 1),
            ("CIRCLE", 2),
            ("INSERT", 1),
            ("LWPOLYLINE", 1),
            ("TEXT", 1)
        ]
    );
    assert_eq!(
        import.report.with_dropped_properties, 2,
        "one line on WALLS, one coloured"
    );
    assert!(import.report.has_unsupported_content());
    assert_eq!(
        import.report.summary(),
        vec![
            "Imported 3 lines.",
            "Not imported (unsupported): 1 ARC, 1 BLOCK, 2 CIRCLE, 1 INSERT, 1 LWPOLYLINE, 1 TEXT.",
            "Layer, colour, and linetype were dropped from 2 lines.",
        ]
    );
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
