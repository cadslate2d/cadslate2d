//! The writer (ADR-0036 §4, #315): every DXF fixture, exported as a DWG
//! of this crate's, reads back as the same drawing, writes the same bytes
//! again, and is read by the independent reader with nothing lost.

use std::fs;
use std::io::Cursor;
use std::path::PathBuf;

use cad_document::Drawing;
use cad_dwg::Version;

mod common;

/// The versions the writer writes.
const VERSIONS: [Version; 2] = [Version::R2000, Version::R2004];

/// Every source in every version, named by both.
fn cases(sources: &[(String, Drawing)]) -> Vec<(Version, String, &Drawing)> {
    VERSIONS
        .iter()
        .flat_map(|version| {
            sources
                .iter()
                .map(move |(name, drawing)| (*version, format!("{name}.{}", version.id()), drawing))
        })
        .collect()
}
use common::{describe, root};

/// Every DXF fixture this project's reader takes, with its name.
fn sources() -> Vec<(String, Drawing)> {
    let dir = root().join("test-data/dxf/valid");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "dxf"))
        .collect();
    files.sort();
    files
        .into_iter()
        .filter_map(|path| {
            let name = path.file_stem().unwrap().to_string_lossy().into_owned();
            let bytes = fs::read(&path).unwrap();
            let import = cad_dxf::import(&bytes).ok()?;
            Some((name, import.drawing))
        })
        .collect()
}

/// The drawing as the exporter's walk leaves it: what a DWG is written
/// from, read back through the DXF of the same walk.
fn normalised(drawing: &Drawing) -> Drawing {
    cad_dxf::import(cad_dxf::export(drawing).as_bytes())
        .unwrap()
        .drawing
}

fn diff(a: &[String], b: &[String]) -> String {
    let mut out = String::new();
    for (i, (x, y)) in a.iter().zip(b).enumerate() {
        if x != y {
            out.push_str(&format!("  line {i}:\n    ours: {x}\n    theirs: {y}\n"));
        }
    }
    if a.len() != b.len() {
        out.push_str(&format!(
            "  ours has {} lines, theirs {}\n",
            a.len(),
            b.len()
        ));
        for x in a.iter().skip(b.len()) {
            out.push_str(&format!("    ours only: {x}\n"));
        }
        for y in b.iter().skip(a.len()) {
            out.push_str(&format!("    theirs only: {y}\n"));
        }
    }
    out
}

/// Export → import → export is byte-identical, and the drawing read back
/// is the drawing written, for every fixture.
#[test]
fn every_fixture_round_trips_through_our_dwg_byte_for_byte() {
    let mut failures = Vec::new();
    let sources = sources();
    for (version, name, drawing) in cases(&sources) {
        let first = cad_dwg::export(drawing, version).unwrap_or_else(|e| panic!("{name}: {e}"));
        if Version::detect(&first) != Ok(version) {
            failures.push(format!("{name}: not a file of its version"));
        }
        let back = match cad_dwg::import(&first) {
            Ok(back) => back,
            Err(e) => {
                failures.push(format!("{name}: not read back: {e}"));
                continue;
            }
        };
        let expected: Vec<String> = describe(&normalised(drawing))
            .into_iter()
            .map(without_fit_points)
            .collect();
        let got: Vec<String> = describe(&back.drawing)
            .into_iter()
            .map(without_fit_points)
            .collect();
        if got != expected {
            failures.push(format!(
                "{name}: the drawing read back differs\n{}",
                diff(&got, &expected)
            ));
        }
        let second = cad_dwg::export(&back.drawing, version).unwrap();
        if second != first {
            failures.push(format!(
                "{name}: the second export differs ({} vs {} bytes)",
                second.len(),
                first.len()
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

/// What an AC1015 file cannot hold: a spline is by its control points or
/// by its fit points (§20.4.40), so one with both keeps its control
/// points, which are its geometry, and its fit points are read back as
/// none. The lines are compared without them.
fn without_fit_points(line: String) -> String {
    match (line.find("fit: ["), line.find("] })")) {
        (Some(from), Some(to)) if from < to => format!("{}fit: [{}", &line[..from], &line[to..]),
        _ => line,
    }
}

/// What the independent reader cannot carry back into a DXF of its own,
/// by fixture and a substring of the lines it changes, dropped from both
/// sides: a difference of its DXF, not of this crate's DWG, each checked
/// against LibreDWG's reading of the same file.
const READER_LOSSES: &[(&str, &str)] = &[
    // acadrust's DXF gives every vertex a width of zero (40, 41), which
    // overrides the constant width (43) it also writes; LibreDWG reads
    // the constant width from our file.
    ("wide-polylines", "Polyline("),
    ("multileaders", "_ARCHTICK"),
    ("ordinate-dimensions", "_ARCHTICK"),
    // acadrust's DXF says a viewport is off (68) whatever the status
    // flags (90) of our file say; LibreDWG's DXF says it is on.
    ("layouts", "Viewport("),
];

/// Keeps the lines no loss listed for `name` touches.
fn keeper(name: &str, losses: &[(&str, &str)]) -> impl Fn(&String) -> bool {
    let patterns: Vec<&str> = losses
        .iter()
        .filter(|(fixture, _)| name.contains(fixture))
        .map(|(_, pattern)| *pattern)
        .collect();
    move |line: &String| !patterns.iter().any(|pattern| line.contains(pattern))
}

/// The independent reader reads every fixture's DWG, and the DXF it
/// writes of it makes the same drawing: nothing lost.
#[test]
fn acadrust_reads_our_output_with_nothing_lost() {
    let mut failures = Vec::new();
    let sources = sources();
    for (version, name, drawing) in cases(&sources) {
        let dwg = cad_dwg::export(drawing, version).unwrap_or_else(|e| panic!("{name}: {e}"));
        let document = match acadrust::DwgReader::from_stream(Cursor::new(dwg)).read() {
            Ok(document) => document,
            Err(e) => {
                failures.push(format!("{name}: acadrust refuses it: {e}"));
                continue;
            }
        };
        let dxf = match acadrust::DxfWriter::new(&document).write_to_vec() {
            Ok(dxf) => dxf,
            Err(e) => {
                failures.push(format!("{name}: acadrust cannot write it back: {e}"));
                continue;
            }
        };
        let theirs = match cad_dxf::import(&dxf) {
            Ok(import) => import.drawing,
            Err(e) => {
                failures.push(format!("{name}: acadrust's DXF is refused: {e}"));
                continue;
            }
        };
        let keep = keeper(&name, READER_LOSSES);
        let expected: Vec<String> = describe(&normalised(drawing))
            .into_iter()
            .map(without_fit_points)
            .filter(&keep)
            .collect();
        let got: Vec<String> = describe(&theirs)
            .into_iter()
            .map(without_fit_points)
            .filter(&keep)
            .collect();
        if got != expected {
            failures.push(format!(
                "{name}: through acadrust differs\n{}",
                diff(&got, &expected)
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}
