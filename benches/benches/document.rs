//! Entity store and spatial index benchmarks (DESIGN §10.3, §20).
//!
//! Run with `cargo bench -p cad-benchmarks --bench document`.

use std::hint::black_box;
use std::sync::Arc;
use std::time::Duration;

use cad_benchmarks::{HashMapStore, Rng, SIZES, lines, pick_box};
use cad_core::{Aabb, EntityId};
use cad_document::spatial::{LinearIndex, RTreeIndex};
use cad_document::{Drawing, Entity, EntityStore, LineEntity, SharedEntity, SpatialIndex};
use criterion::{BatchSize, BenchmarkId, Criterion, criterion_group, criterion_main};

const SEED: u64 = 2026;
/// Operations per measured iteration for the "editing" benchmarks.
const BATCH: usize = 1000;

fn shared(lines: &[LineEntity]) -> Vec<(EntityId, SharedEntity)> {
    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            (
                EntityId::from_raw(index as u64 + 1),
                Arc::new(Entity::Line(*line)),
            )
        })
        .collect()
}

fn bounds(lines: &[LineEntity]) -> Vec<(EntityId, Aabb)> {
    shared(lines)
        .iter()
        .map(|(id, entity)| (*id, entity.bounds()))
        .collect()
}

fn dense_store(entities: &[(EntityId, SharedEntity)]) -> EntityStore {
    let mut store = EntityStore::new();
    for (id, entity) in entities {
        store.insert(*id, Arc::clone(entity)).unwrap();
    }
    store
}

fn hash_store(entities: &[(EntityId, SharedEntity)]) -> HashMapStore {
    let mut store = HashMapStore::default();
    for (id, entity) in entities {
        store.insert(*id, Arc::clone(entity));
    }
    store
}

fn store(c: &mut Criterion) {
    for size in SIZES {
        let entities = shared(&lines(size, SEED));
        let (dense, hash) = (dense_store(&entities), hash_store(&entities));

        // What scene preparation does: visit every entity once.
        let mut group = c.benchmark_group("store/iterate");
        group.bench_function(BenchmarkId::new("dense", size), |b| {
            b.iter(|| {
                dense
                    .iter()
                    .map(|(_, entity)| entity.bounds().min().x)
                    .sum::<f64>()
            })
        });
        group.bench_function(BenchmarkId::new("hashmap", size), |b| {
            b.iter(|| {
                hash.iter()
                    .map(|(_, entity)| entity.bounds().min().x)
                    .sum::<f64>()
            })
        });
        group.finish();

        let mut rng = Rng::new(SEED);
        let probes: Vec<EntityId> = (0..BATCH).map(|_| entities[rng.below(size)].0).collect();
        let mut group = c.benchmark_group("store/get_1000");
        group.bench_function(BenchmarkId::new("dense", size), |b| {
            b.iter(|| probes.iter().filter(|id| dense.get(**id).is_some()).count())
        });
        group.bench_function(BenchmarkId::new("hashmap", size), |b| {
            b.iter(|| probes.iter().filter(|id| hash.get(**id).is_some()).count())
        });
        group.finish();

        // Erase and restore: what an erase followed by undo does to storage.
        let mut group = c.benchmark_group("store/remove_and_restore_1000");
        group.bench_function(BenchmarkId::new("dense", size), |b| {
            b.iter_batched_ref(
                || dense_store(&entities),
                |store| {
                    for id in &probes {
                        if let Some(entity) = store.remove(*id) {
                            store.insert(*id, entity).unwrap();
                        }
                    }
                },
                BatchSize::LargeInput,
            )
        });
        group.bench_function(BenchmarkId::new("hashmap", size), |b| {
            b.iter_batched_ref(
                || hash_store(&entities),
                |store| {
                    for id in &probes {
                        if let Some(entity) = store.remove(*id) {
                            store.insert(*id, entity);
                        }
                    }
                },
                BatchSize::LargeInput,
            )
        });
        group.finish();
    }
}

fn loaded<I: SpatialIndex + Default>(entries: &[(EntityId, Aabb)]) -> I {
    let mut index = I::default();
    index.rebuild(&mut entries.iter().copied());
    index
}

fn index(c: &mut Criterion) {
    for size in SIZES {
        let entries = bounds(&lines(size, SEED));
        let mut rng = Rng::new(SEED);
        let queries: Vec<Aabb> = (0..BATCH).map(|_| pick_box(&mut rng, size)).collect();
        let moved: Vec<(EntityId, Aabb)> = (0..BATCH)
            .map(|_| (entries[rng.below(size)].0, pick_box(&mut rng, size)))
            .collect();

        // Opening a file: load everything at once.
        let mut group = c.benchmark_group("index/rebuild");
        group.bench_function(BenchmarkId::new("rtree", size), |b| {
            b.iter(|| loaded::<RTreeIndex>(&entries))
        });
        group.bench_function(BenchmarkId::new("linear", size), |b| {
            b.iter(|| loaded::<LinearIndex>(&entries))
        });
        group.finish();

        // Drawing: one insert per committed entity.
        let mut group = c.benchmark_group("index/insert_1000_into");
        let extra: Vec<(EntityId, Aabb)> = (0..BATCH)
            .map(|offset| {
                (
                    EntityId::from_raw((size + offset) as u64 + 1),
                    pick_box(&mut rng, size),
                )
            })
            .collect();
        group.bench_function(BenchmarkId::new("rtree", size), |b| {
            b.iter_batched_ref(
                || loaded::<RTreeIndex>(&entries),
                |index| {
                    extra
                        .iter()
                        .for_each(|(id, bounds)| index.insert(*id, *bounds))
                },
                BatchSize::LargeInput,
            )
        });
        group.finish();

        // Picking, hover, and snapping: small boxes, constantly.
        let mut group = c.benchmark_group("index/pick_query_1000");
        let (rtree, linear) = (
            loaded::<RTreeIndex>(&entries),
            loaded::<LinearIndex>(&entries),
        );
        group.bench_function(BenchmarkId::new("rtree", size), |b| {
            b.iter(|| {
                queries
                    .iter()
                    .map(|query| rtree.query(query).len())
                    .sum::<usize>()
            })
        });
        group.bench_function(BenchmarkId::new("linear", size), |b| {
            b.iter(|| {
                queries
                    .iter()
                    .map(|query| linear.query(query).len())
                    .sum::<usize>()
            })
        });
        group.finish();

        // Moving entities: the bounds of existing entries change.
        let mut group = c.benchmark_group("index/update_1000");
        group.bench_function(BenchmarkId::new("rtree", size), |b| {
            b.iter_batched_ref(
                || loaded::<RTreeIndex>(&entries),
                |index| {
                    moved
                        .iter()
                        .for_each(|(id, bounds)| index.update(*id, *bounds))
                },
                BatchSize::LargeInput,
            )
        });
        group.bench_function(BenchmarkId::new("linear", size), |b| {
            b.iter_batched_ref(
                || loaded::<LinearIndex>(&entries),
                |index| {
                    moved
                        .iter()
                        .for_each(|(id, bounds)| index.update(*id, *bounds))
                },
                BatchSize::LargeInput,
            )
        });
        group.finish();
    }
}

fn drawing_of(size: usize) -> Drawing {
    Drawing::from_entities(lines(size, SEED).into_iter().map(Entity::Line))
}

/// The whole write path — store, index, history, revision — as commands use it.
fn drawing(c: &mut Criterion) {
    for size in SIZES {
        let extra = lines(BATCH, SEED + 1);

        let mut group = c.benchmark_group("drawing/commit_1000_lines_onto");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter_batched_ref(
                || drawing_of(size),
                |drawing| {
                    for line in &extra {
                        let mut transaction = drawing.transaction();
                        transaction.insert(*line);
                        transaction.commit();
                    }
                },
                BatchSize::LargeInput,
            )
        });
        group.finish();

        let mut group = c.benchmark_group("drawing/undo_then_redo_1000_on");
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter_batched_ref(
                || {
                    let mut drawing = drawing_of(size);
                    for line in &extra {
                        let mut transaction = drawing.transaction();
                        transaction.insert(*line);
                        transaction.commit();
                    }
                    drawing
                },
                |drawing| {
                    (0..BATCH).for_each(|_| drop(black_box(drawing.undo())));
                    (0..BATCH).for_each(|_| drop(black_box(drawing.redo())));
                },
                BatchSize::LargeInput,
            )
        });
        group.finish();

        let mut group = c.benchmark_group("drawing/open");
        let entities: Vec<Entity> = lines(size, SEED).into_iter().map(Entity::Line).collect();
        group.bench_function(BenchmarkId::from_parameter(size), |b| {
            b.iter(|| Drawing::from_entities(entities.iter().cloned()))
        });
        group.finish();
    }
}

fn quick() -> Criterion {
    Criterion::default()
        .sample_size(10)
        .warm_up_time(Duration::from_millis(300))
        .measurement_time(Duration::from_secs(1))
}

criterion_group! { name = benches; config = quick(); targets = store, index, drawing }
criterion_main!(benches);
