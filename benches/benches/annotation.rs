//! Dimensions and hatches in bulk (DESIGN §19.6, §42; #170): ten thousand
//! dimensions in one style — building the scene, and what a style change
//! costs when every dimension is laid out afresh (ADR-0020) — and a
//! thousand hatches, solid and patterned, cut into triangles and lines
//! when the scene is built (ADR-0021).
//!
//! Run with `cargo bench -p cad-benchmarks --bench annotation`.

use std::time::Duration;

use cad_core::Point2;
use cad_document::{
    DimStyle, DimStyleId, DimensionEntity, Drawing, HatchEntity, HatchStyle, Measure, Path,
};
use cad_render::SceneCache;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const DIMENSIONS: usize = 10_000;
const HATCHES: usize = 1_000;

/// `count` aligned dimensions on a grid, in the drawing's first style.
fn dimensions(count: usize) -> Drawing {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let side = (count as f64).sqrt().ceil() as usize;
    for index in 0..count {
        let at = Point2::new((index % side) as f64 * 200.0, (index / side) as f64 * 100.0);
        let measure = Measure::Aligned {
            a: at,
            b: Point2::new(at.x + 100.0, at.y + 30.0),
        };
        tx.insert(DimensionEntity::new(
            measure,
            Point2::new(at.x, at.y + 40.0),
        ))
        .unwrap();
    }
    tx.commit();
    drawing
}

/// `count` hatches on a grid: squares with a round hole, every other one
/// patterned ANSI31.
fn hatches(count: usize) -> Drawing {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let side = (count as f64).sqrt().ceil() as usize;
    for index in 0..count {
        let (x, y) = ((index % side) as f64 * 150.0, (index / side) as f64 * 150.0);
        let square = Path {
            vertices: vec![
                Point2::new(x, y),
                Point2::new(x + 100.0, y),
                Point2::new(x + 100.0, y + 100.0),
                Point2::new(x, y + 100.0),
            ],
            bulges: Vec::new(),
            closed: true,
        };
        let hole = Path {
            vertices: vec![
                Point2::new(x + 30.0, y + 50.0),
                Point2::new(x + 70.0, y + 50.0),
            ],
            bulges: vec![1.0, 1.0],
            closed: true,
        };
        let hatch = HatchEntity::solid(vec![square, hole]);
        let hatch = if index % 2 == 0 {
            hatch
        } else {
            hatch.styled(HatchStyle::named("ANSI31", 0.0, 1.0))
        };
        tx.insert(hatch).unwrap();
    }
    tx.commit();
    drawing
}

fn annotation(c: &mut Criterion) {
    let mut drawing = dimensions(DIMENSIONS);
    let mut group = c.benchmark_group("dimensions/scene_build");
    group.bench_function(BenchmarkId::from_parameter(DIMENSIONS), |b| {
        b.iter(|| SceneCache::new().scene(&drawing).frame().segment_count())
    });
    group.finish();

    // One row changes; ten thousand dimensions are laid out afresh for the
    // index, and the scene is rebuilt.
    let mut group = c.benchmark_group("dimensions/restyle_and_rebuild");
    group.bench_function(BenchmarkId::from_parameter(DIMENSIONS), |b| {
        let mut taller = false;
        b.iter(|| {
            taller = !taller;
            let height = if taller { 3.5 } else { 2.5 };
            let mut tx = drawing.transaction();
            let mut style = DimStyle::named(DimStyle::STANDARD);
            style.text_height = height;
            tx.replace_dim_style(DimStyleId::FIRST, style).unwrap();
            tx.commit();
            SceneCache::new().scene(&drawing).frame().segment_count()
        })
    });
    group.finish();

    let drawing = hatches(HATCHES);
    let mut group = c.benchmark_group("hatches/scene_build");
    group.bench_function(BenchmarkId::from_parameter(HATCHES), |b| {
        b.iter(|| {
            let mut cache = SceneCache::new();
            let frame = cache.scene(&drawing).frame();
            (frame.triangle_count(), frame.segment_count())
        })
    });
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(5)).sample_size(10);
    targets = annotation
}
criterion_main!(benches);
