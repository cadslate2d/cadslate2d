//! Render preparation benchmarks (DESIGN §12.6).
//!
//! Run with `cargo bench -p cad-benchmarks --bench render`.

use std::time::Duration;

use cad_benchmarks::{SIZES, lines};
use cad_core::{Point2, Vec2};
use cad_document::{Drawing, Entity, LineEntity};
use cad_render::{PackedFrame, SceneCache, StyleRole};
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

const SEED: u64 = 2026;

fn scene(c: &mut Criterion) {
    for size in [1_000].into_iter().chain(SIZES) {
        let mut drawing = Drawing::from_entities(lines(size, SEED).into_iter().map(Entity::Line));

        // First build: allocates the buffers.
        let mut group = c.benchmark_group("scene/build_cold");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter(|| SceneCache::new().scene(&drawing).frame().segment_count())
        });
        group.finish();

        // Rebuild after an edit: reuses the previous buffers.
        let mut group = c.benchmark_group("scene/rebuild_after_edit");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut cache = SceneCache::new();
            cache.scene(&drawing);
            b.iter(|| {
                let mut transaction = drawing.transaction();
                transaction.insert(LineEntity::new(Point2::ORIGIN, Point2::new(1.0, 1.0)));
                transaction.commit();
                let segments = cache.scene(&drawing).frame().segment_count();
                // Undo so the drawing stays the same size on every iteration.
                drawing.undo();
                segments
            })
        });
        group.finish();

        // What every pointer frame costs on the committed side: nothing is rebuilt.
        let mut group = c.benchmark_group("scene/unchanged_lookup");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut cache = SceneCache::new();
            cache.scene(&drawing);
            b.iter(|| cache.scene(&drawing).revision())
        });
        group.finish();
    }
}

/// Packing cost per segment, independent of the document.
fn packing(c: &mut Criterion) {
    let mut group = c.benchmark_group("frame/push_segments");
    for size in [10usize, 1_000, 100_000] {
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter_batched_ref(
                PackedFrame::default,
                |frame| {
                    for index in 0..size {
                        let start = Point2::new(index as f64, 0.0);
                        frame.push_segment(StyleRole::Entity, start, start + Vec2::new(1.0, 1.0));
                    }
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

fn quick() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(1))
}

criterion_group! { name = benches; config = quick(); targets = scene, packing }
criterion_main!(benches);
