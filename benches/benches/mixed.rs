//! A mixed drawing at the sizes of DESIGN §42 (#171): every kind of entity
//! in a drafter's proportions. Opening it (the DXF read, which builds the
//! drawing and its index), building its scene, and the scene build after
//! one edit — the costs a person feels, measured natively; the browser's
//! are in `web/bench/mixed.bench.ts`.
//!
//! Run with `cargo bench -p cad-benchmarks --bench mixed`.

use std::time::Duration;

use cad_benchmarks::{SIZES, mixed};
use cad_core::Point2;
use cad_document::LineEntity;
use cad_render::SceneCache;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const SEED: u64 = 2026;

fn mixed_drawing(c: &mut Criterion) {
    for size in SIZES {
        let drawing = mixed(size, SEED);
        let text = cad_dxf::export(&drawing);

        let mut group = c.benchmark_group("mixed/open");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter(|| cad_dxf::import(text.as_bytes()).unwrap().drawing.len())
        });
        group.finish();

        let mut group = c.benchmark_group("mixed/scene_build");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter(|| {
                let mut cache = SceneCache::new();
                let frame = cache.scene(&drawing).frame();
                (
                    frame.segment_count(),
                    frame.arc_count(),
                    frame.triangle_count(),
                )
            })
        });
        group.finish();

        // One line drawn; the whole scene is rebuilt (ADR-0003, #172).
        let mut drawing = mixed(size, SEED);
        let mut cache = SceneCache::new();
        cache.scene(&drawing);
        let mut group = c.benchmark_group("mixed/edit_and_rebuild");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut step = 0.0;
            b.iter(|| {
                step += 1.0;
                let mut tx = drawing.transaction();
                tx.insert(LineEntity::new(
                    Point2::new(step, 0.0),
                    Point2::new(step, 1.0),
                ))
                .unwrap();
                tx.commit();
                cache.scene(&drawing).frame().segment_count()
            })
        });
        group.finish();
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(8)).sample_size(10);
    targets = mixed_drawing
}
criterion_main!(benches);
