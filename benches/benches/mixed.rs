//! A mixed drawing at the sizes of DESIGN §42 (#171): every kind of entity
//! in a drafter's proportions. Opening it (the DXF read, which builds the
//! drawing and its index), building its scene, and the scene re-pack after
//! one edit (ADR-0022) — the costs a person feels, measured natively; the
//! browser's are in `web/bench/mixed.bench.ts`.
//!
//! Run with `cargo bench -p cad-benchmarks --bench mixed`.

use std::collections::HashSet;
use std::time::Duration;

use cad_benchmarks::{SIZES, mixed};
use cad_core::{EntityId, Point2};
use cad_document::draw_order::{Placement, reordered};
use cad_document::{Entity, LineEntity};
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

        // A line in the middle of the drawing moved: the chunk holding it is
        // re-packed, and nothing else (ADR-0022). What an edit costs.
        let mut drawing = mixed(size, SEED);
        let mut cache = SceneCache::new();
        cache.scene(&drawing);
        let lines: Vec<EntityId> = drawing
            .records()
            .filter(|(_, record)| matches!(*record.entity, Entity::Line(_)))
            .map(|(id, _)| id)
            .collect();
        let moved = lines[lines.len() / 2];
        let mut group = c.benchmark_group("mixed/edit_and_repack");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut step = 0.0;
            b.iter(|| {
                step += 1.0;
                let mut tx = drawing.transaction();
                tx.replace(
                    moved,
                    LineEntity::new(Point2::new(step, 0.0), Point2::new(step, 1.0)),
                )
                .unwrap();
                tx.commit();
                cache.scene(&drawing).frame().segment_count()
            })
        });
        group.finish();

        // One line drawn: the last chunk, which holds the new line and
        // little else, is re-packed.
        let mut group = c.benchmark_group("mixed/insert_and_repack");
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

        // One line brought to the front and sent back again (#226,
        // ADR-0032): a change of tier is a change to its record, and the
        // chunk holding it is re-packed, as for any edit.
        let mut group = c.benchmark_group("mixed/reorder_and_repack");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut front = false;
            b.iter(|| {
                front = !front;
                let mut tx = drawing.transaction();
                tx.set_order(moved, i16::from(front)).unwrap();
                tx.commit();
                cache.scene(&drawing).frame().segment_count()
            })
        });
        group.finish();

        // Every hatch sent to the back, and brought forward again: one
        // DRAWORDER over a selection spread across the whole drawing, so
        // nearly every chunk is re-packed.
        let hatches: HashSet<EntityId> = drawing
            .records()
            .filter(|(_, record)| matches!(*record.entity, Entity::Hatch(_)))
            .map(|(id, _)| id)
            .collect();
        let mut group = c.benchmark_group("mixed/hatches_to_back_and_repack");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut back = false;
            b.iter(|| {
                back = !back;
                let all: Vec<(EntityId, i16)> = drawing
                    .records()
                    .map(|(id, record)| (id, record.order))
                    .collect();
                let placement = if back {
                    Placement::Back
                } else {
                    Placement::Front
                };
                let mut tx = drawing.transaction();
                for (id, order) in reordered(&all, &hatches, placement) {
                    tx.set_order(id, order).unwrap();
                }
                tx.commit();
                cache.scene(&drawing).frame().segment_count()
            })
        });
        group.finish();

        // The same edit with the scene rebuilt whole, for the comparison
        // ADR-0022 rests on — on a fresh drawing, as the inserts above grew
        // this one.
        let mut drawing = mixed(size, SEED);
        let mut group = c.benchmark_group("mixed/edit_and_rebuild_whole");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            let mut step = 0.0;
            b.iter(|| {
                step += 1.0;
                let mut tx = drawing.transaction();
                tx.replace(
                    moved,
                    LineEntity::new(Point2::new(step, 0.0), Point2::new(step, 1.0)),
                )
                .unwrap();
                tx.commit();
                SceneCache::new().scene(&drawing).frame().segment_count()
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
