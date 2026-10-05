//! Runs the intersection regression corpus in `test-data/geometry/`.
//!
//! The corpus is plain text so that recording a fixed bug is a one-line
//! change (DESIGN §21.3).

use std::path::Path;

use cad_core::Point2;
use cad_geometry::{
    Arc, Circle, Path as Polyline, PathSpan, Primitive, Segment, offset_path, path_spans,
};

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

// ── polyline offsets ──

fn corpus_file(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-data/geometry")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn polyline(kind: &str, vertices: &str, line: usize) -> Polyline {
    let mut path = Polyline {
        vertices: Vec::new(),
        bulges: Vec::new(),
        closed: match kind.trim() {
            "open" => false,
            "closed" => true,
            other => panic!("line {line}: `{other}` is neither open nor closed"),
        },
    };
    for vertex in vertices.split_whitespace() {
        let values: Vec<f64> = vertex
            .split(',')
            .map(|token| {
                token
                    .parse()
                    .unwrap_or_else(|_| panic!("line {line}: `{token}` is not a number"))
            })
            .collect();
        match values.as_slice() {
            [x, y] => {
                path.vertices.push(Point2::new(*x, *y));
                path.bulges.push(0.0);
            }
            [x, y, bulge] => {
                path.vertices.push(Point2::new(*x, *y));
                path.bulges.push(*bulge);
            }
            _ => panic!("line {line}: cannot read vertex `{vertex}`"),
        }
    }
    path
}

fn span_length(span: &PathSpan) -> f64 {
    match span.primitive() {
        Primitive::Segment(segment) => segment.length(),
        Primitive::Arc(arc) => arc.radius * arc.sweep,
        Primitive::Circle(circle) => std::f64::consts::TAU * circle.radius,
    }
}

/// The offset corpus: the count and total length recorded for each case,
/// and — for every case — the property the method is built on.
#[test]
fn polyline_offset_corpus() {
    let corpus = corpus_file("polyline-offsets.txt");
    let report = std::env::var_os("REPORT_OFFSETS").is_some();
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
        let fields: Vec<&str> = inputs.split(':').collect();
        let [name, kind, vertices, distance] = fields.as_slice() else {
            panic!("line {line}: expected name : open|closed : vertices : distance");
        };
        let name = name.trim();
        let source = polyline(kind, vertices, line);
        let signed: f64 = distance
            .trim()
            .parse()
            .unwrap_or_else(|_| panic!("line {line}: bad distance"));

        let offsets = offset_path(&source, signed);
        let total: f64 = offsets
            .iter()
            .flat_map(path_spans)
            .map(|span| span_length(&span))
            .sum();

        // Every point of the result is the offset distance from the source.
        let shape: Vec<Primitive> = path_spans(&source)
            .iter()
            .map(PathSpan::primitive)
            .collect();
        for span in offsets.iter().flat_map(path_spans) {
            for step in 0..=20 {
                let point = span.point_at(f64::from(step) / 20.0);
                let nearest = shape
                    .iter()
                    .map(|primitive| primitive.distance_to_point(point))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    (nearest - signed.abs()).abs() < AGREEMENT,
                    "line {line} ({name}): {point:?} is {nearest} from the source, not {}",
                    signed.abs()
                );
            }
        }

        if report {
            println!("{name}: {} {total}", offsets.len());
        }
        let wanted: Vec<&str> = outcome.split_whitespace().collect();
        if let [count, length] = wanted.as_slice() {
            let count: usize = count
                .parse()
                .unwrap_or_else(|_| panic!("line {line}: bad count"));
            let length: f64 = length
                .parse()
                .unwrap_or_else(|_| panic!("line {line}: bad length"));
            assert_eq!(offsets.len(), count, "line {line} ({name}): {offsets:?}");
            assert!(
                (total - length).abs() < AGREEMENT * (1.0 + length),
                "line {line} ({name}): total length {total}, expected {length}"
            );
        } else {
            panic!("line {line} ({name}): expected `=> <paths> <total length>`");
        }
        cases += 1;
    }
    assert!(cases >= 20, "the corpus should not shrink: {cases} cases");
}
