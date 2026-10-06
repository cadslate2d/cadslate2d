//! Block references in bulk (DESIGN §19.6, §42; #147): ten thousand
//! references to one block — building the scene, picking among them, and
//! what a redefinition costs when every reference follows.
//!
//! Run with `cargo bench -p cad-benchmarks --bench blocks`.

use std::time::Duration;

use cad_core::{Aabb, Point2};
use cad_document::{
    Block, BlockId, BlockReferenceEntity, Drawing, EntityRecord, LineEntity, Properties,
};
use cad_render::SceneCache;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const REFERENCES: usize = 10_000;

/// A chair of four lines, placed `count` times on a grid.
fn chairs(count: usize) -> (Drawing, BlockId) {
    let mut drawing = Drawing::new();
    let mut tx = drawing.transaction();
    let square = |x0: f64, y0: f64, x1: f64, y1: f64| {
        EntityRecord::new(
            LineEntity::new(Point2::new(x0, y0), Point2::new(x1, y1)),
            Properties::default(),
        )
    };
    let chair = tx
        .define_block(Block::new(
            "Chair",
            Point2::ORIGIN,
            vec![
                square(0.0, 0.0, 10.0, 0.0),
                square(10.0, 0.0, 10.0, 10.0),
                square(10.0, 10.0, 0.0, 10.0),
                square(0.0, 10.0, 0.0, 0.0),
            ],
        ))
        .unwrap();
    let side = (count as f64).sqrt().ceil() as usize;
    for index in 0..count {
        let at = Point2::new((index % side) as f64 * 20.0, (index / side) as f64 * 20.0);
        tx.insert(BlockReferenceEntity::new(chair, at, 1.0, 0.0))
            .unwrap();
    }
    tx.commit();
    (drawing, chair)
}

fn references(c: &mut Criterion) {
    let (mut drawing, chair) = chairs(REFERENCES);

    let mut group = c.benchmark_group("blocks/scene_build");
    group.bench_function(BenchmarkId::from_parameter(REFERENCES), |b| {
        b.iter(|| SceneCache::new().scene(&drawing).frame().segment_count())
    });
    group.finish();

    // One change, ten thousand index entries recomputed, one scene rebuild.
    let mut group = c.benchmark_group("blocks/redefine_and_rebuild");
    group.bench_function(BenchmarkId::from_parameter(REFERENCES), |b| {
        let mut cache = SceneCache::new();
        cache.scene(&drawing);
        b.iter(|| {
            let mut tx = drawing.transaction();
            tx.redefine_block(
                chair,
                Block::new(
                    "Chair",
                    Point2::ORIGIN,
                    vec![EntityRecord::new(
                        LineEntity::new(Point2::ORIGIN, Point2::new(10.0, 10.0)),
                        Properties::default(),
                    )],
                ),
            )
            .unwrap();
            tx.commit();
            let segments = cache.scene(&drawing).frame().segment_count();
            drawing.undo();
            cache.scene(&drawing);
            segments
        })
    });
    group.finish();

    // A pick: the index narrows to a few references, each flattened to test.
    let mut group = c.benchmark_group("blocks/pick");
    group.bench_function(BenchmarkId::from_parameter(REFERENCES), |b| {
        let probe = Point2::new(1005.0, 1000.0);
        let around = Aabb::from_points(
            Point2::new(probe.x - 2.0, probe.y - 2.0),
            Point2::new(probe.x + 2.0, probe.y + 2.0),
        );
        b.iter(|| {
            drawing
                .query_shown(&around)
                .into_iter()
                .filter(|id| {
                    drawing
                        .entity(*id)
                        .is_some_and(|e| e.distance_to(probe, drawing.blocks()) < 2.0)
                })
                .count()
        })
    });
    group.finish();
}

fn quick() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(2))
}

criterion_group! {
    name = benches;
    config = quick();
    targets = references
}
criterion_main!(benches);
