//! Polyline offset cost (ADR-0011): both candidates on closed zig-zag
//! shapes of growing size. The shipped method compares every raw piece with
//! every source segment, so it is quadratic; this shows where that bites.

use cad_benchmarks::winding_offset::offset_by_winding;
use cad_core::Point2;
use cad_geometry::{Path, offset_path};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// A closed comb with `teeth` teeth: 4 vertices per tooth, counter-clockwise.
fn comb(teeth: usize) -> Path {
    let mut vertices = Vec::with_capacity(4 * teeth + 2);
    for tooth in 0..teeth {
        let x = tooth as f64 * 40.0;
        vertices.extend([
            Point2::new(x, 0.0),
            Point2::new(x + 20.0, 0.0),
            Point2::new(x + 20.0, -30.0),
            Point2::new(x + 40.0, -30.0),
        ]);
    }
    let width = teeth as f64 * 40.0;
    vertices.extend([Point2::new(width, 60.0), Point2::new(0.0, 60.0)]);
    vertices.reverse();
    let bulges = vec![0.0; vertices.len()];
    Path {
        vertices,
        bulges,
        closed: true,
    }
}

fn offset(c: &mut Criterion) {
    let mut group = c.benchmark_group("offset/closed_comb");
    group.sample_size(10);
    for teeth in [2, 25, 250] {
        let path = comb(teeth);
        let vertices = path.vertices.len();
        group.bench_function(BenchmarkId::new("slice_and_filter", vertices), |b| {
            b.iter(|| offset_path(&path, 5.0).len())
        });
        group.bench_function(BenchmarkId::new("winding_number", vertices), |b| {
            b.iter(|| offset_by_winding(&path, 5.0).len())
        });
    }
    group.finish();
}

criterion_group!(benches, offset);
criterion_main!(benches);
