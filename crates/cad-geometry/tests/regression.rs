//! Runs the intersection regression corpus in `test-data/geometry/`.
//!
//! The corpus is plain text so that recording a fixed bug is a one-line
//! change (DESIGN §21.3).

use std::path::Path;

use cad_core::Point2;
use cad_geometry::{Arc, Circle, Primitive, Segment};

/// How far a computed point may be from the expected one. Looser than the
/// geometric tolerance because expected values are typed to finite precision.
const AGREEMENT: f64 = 1e-6;

fn numbers(text: &str, line: usize) -> Vec<f64> {
    text.split_whitespace()
        .map(|token| {
            token
                .parse()
                .unwrap_or_else(|_| panic!("line {line}: `{token}` is not a number"))
        })
        .collect()
}

fn primitive(text: &str, line: usize) -> Primitive {
    let (kind, rest) = text
        .trim()
        .split_once(' ')
        .unwrap_or_else(|| panic!("line {line}: empty primitive"));
    let values = numbers(rest, line);
    match (kind, values.as_slice()) {
        ("segment", [x0, y0, x1, y1]) => {
            Primitive::Segment(Segment::new(Point2::new(*x0, *y0), Point2::new(*x1, *y1)))
        }
        ("circle", [x, y, radius]) => Primitive::Circle(Circle::new(Point2::new(*x, *y), *radius)),
        ("arc", [x, y, radius, start, sweep]) => Primitive::Arc(Arc::new(
            Point2::new(*x, *y),
            *radius,
            start.to_radians(),
            sweep.to_radians(),
        )),
        _ => panic!("line {line}: cannot read `{text}`"),
    }
}

fn expected(text: &str, line: usize) -> Vec<Point2> {
    if text.trim() == "none" {
        return Vec::new();
    }
    let values = numbers(text, line);
    assert!(
        values.len().is_multiple_of(2),
        "line {line}: expected points need an x and a y"
    );
    values
        .chunks(2)
        .map(|pair| Point2::new(pair[0], pair[1]))
        .collect()
}

#[test]
fn intersection_corpus() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-data/geometry/intersections.txt");
    let corpus = std::fs::read_to_string(&path).unwrap();
    let mut cases = 0;

    for (index, text) in corpus.lines().enumerate() {
        let line = index + 1;
        let text = text.trim();
        if text.is_empty() || text.starts_with('#') {
            continue;
        }
        let (inputs, outcome) = text
            .split_once("=>")
            .unwrap_or_else(|| panic!("line {line}: missing =>"));
        let (first, second) = inputs
            .split_once('|')
            .unwrap_or_else(|| panic!("line {line}: missing |"));
        let (a, b) = (primitive(first, line), primitive(second, line));
        let want = expected(outcome, line);

        // Both orders must agree with the corpus.
        for (label, hits) in [("a|b", a.intersect(&b)), ("b|a", b.intersect(&a))] {
            let got = hits.as_slice();
            assert_eq!(
                got.len(),
                want.len(),
                "line {line} ({label}): `{text}` gave {got:?}"
            );
            for point in &want {
                assert!(
                    got.iter()
                        .any(|found| found.distance_to(*point) < AGREEMENT),
                    "line {line} ({label}): `{text}` is missing {point:?}; got {got:?}"
                );
            }
        }
        cases += 1;
    }
    assert!(cases >= 20, "the corpus should not shrink: {cases} cases");
}
