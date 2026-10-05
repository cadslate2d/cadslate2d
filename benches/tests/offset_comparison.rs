//! The polyline-offset experiment (ADR-0011): both candidates, run on the
//! closed cases of the corpus. Run with `--nocapture` to print the table.

use cad_benchmarks::winding_offset::offset_by_winding;
use cad_core::Point2;
use cad_geometry::{Path, PathSpan, Primitive, offset_path, path_spans};

fn corpus() -> Vec<(String, Path, f64)> {
    let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../test-data/geometry/polyline-offsets.txt");
    let text = std::fs::read_to_string(file).unwrap();
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let (inputs, _) = line.split_once("=>").unwrap();
            let fields: Vec<&str> = inputs.split(':').map(str::trim).collect();
            let mut path = Path {
                vertices: Vec::new(),
                bulges: Vec::new(),
                closed: fields[1] == "closed",
            };
            for vertex in fields[2].split_whitespace() {
                let values: Vec<f64> = vertex.split(',').map(|v| v.parse().unwrap()).collect();
                path.vertices.push(Point2::new(values[0], values[1]));
                path.bulges.push(values.get(2).copied().unwrap_or(0.0));
            }
            (fields[0].to_owned(), path, fields[3].parse().unwrap())
        })
        .collect()
}

fn total_length(paths: &[Path]) -> f64 {
    paths
        .iter()
        .flat_map(path_spans)
        .map(|span| match span.primitive() {
            Primitive::Segment(segment) => segment.length(),
            Primitive::Arc(arc) => arc.radius * arc.sweep,
            Primitive::Circle(circle) => std::f64::consts::TAU * circle.radius,
        })
        .sum()
}

/// The worst distance error of any sampled point of `offsets`.
fn worst_deviation(source: &Path, offsets: &[Path], distance: f64) -> f64 {
    let shape: Vec<Primitive> = path_spans(source).iter().map(PathSpan::primitive).collect();
    offsets
        .iter()
        .flat_map(path_spans)
        .flat_map(|span| (0..=10).map(move |step| span.point_at(f64::from(step) / 10.0)))
        .map(|point| {
            let nearest = shape
                .iter()
                .map(|primitive| primitive.distance_to_point(point))
                .fold(f64::INFINITY, f64::min);
            (nearest - distance).abs()
        })
        .fold(0.0, f64::max)
}

#[test]
fn both_candidates_on_the_closed_cases_of_the_corpus() {
    let mut agreed = 0;
    let mut closed_cases = 0;
    println!("| Case | Slice and filter | Winding number | Agree |");
    println!("| --- | --- | --- | --- |");
    for (name, path, signed) in corpus() {
        let sliced = offset_path(&path, signed);
        let sliced_summary = format!(
            "{} path(s), length {:.4}",
            sliced.len(),
            total_length(&sliced)
        );
        // The shipped method must be right on every case, open or closed.
        assert!(
            worst_deviation(&path, &sliced, signed.abs()) < 1e-6,
            "{name}"
        );
        if !path.closed {
            println!("| {name} | {sliced_summary} | not applicable (open) | — |");
            continue;
        }
        closed_cases += 1;
        let wound = offset_by_winding(&path, signed);
        let deviation = worst_deviation(&path, &wound, signed.abs());
        let same = wound.len() == sliced.len()
            && (total_length(&wound) - total_length(&sliced)).abs()
                < 1e-6 * (1.0 + total_length(&sliced));
        agreed += usize::from(same);
        println!(
            "| {name} | {sliced_summary} | {} path(s), length {:.4}, worst error {:.1e} | {} |",
            wound.len(),
            total_length(&wound),
            deviation,
            if same { "yes" } else { "**no**" }
        );
    }
    println!("{agreed} of {closed_cases} closed cases agree");
    // The experiment's finding, pinned: the two methods agree on most closed
    // cases. Where they differ, the distance property above says which is right.
    assert!(closed_cases >= 10);
    assert!(
        agreed * 2 >= closed_cases,
        "the candidates disagree on most cases"
    );
}
