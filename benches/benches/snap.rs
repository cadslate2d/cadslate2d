//! Object-snap cost (DESIGN §18.1, §42; issues #76 and #78).
//!
//! Two questions:
//!
//! * Does a snap query depend on how large the drawing is? It must not: the
//!   candidates come from the spatial index, never from a scan.
//! * What does a crowded spot cost, where many entities sit under the
//!   aperture and intersections are searched pair by pair?

use std::f64::consts::PI;
use std::hint::black_box;

use cad_benchmarks::{Rng, SIZES, lines};
use cad_commands::snap::{SnapModes, find_snap};
use cad_core::{Point2, ScreenPoint, View};
use cad_document::{Drawing, Entity, LineEntity};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const QUERIES: usize = 1000;
const SEED: u64 = 0x5EED;

fn view() -> View {
    View::new(1920.0, 1080.0)
}

/// Cursor positions a few pixels away from the endpoints of random lines, so
/// every query has real candidates to rank.
fn cursors_near_endpoints(generated: &[LineEntity], view: &View) -> Vec<ScreenPoint> {
    let mut rng = Rng::new(SEED ^ 0xC0FFEE);
    (0..QUERIES)
        .map(|_| {
            let end = generated[rng.below(generated.len())].end;
            let near = Point2::new(end.x + rng.range(-4.0, 4.0), end.y + rng.range(-4.0, 4.0));
            view.world_to_screen(near)
        })
        .collect()
}

fn by_drawing_size(c: &mut Criterion) {
    let view = view();
    let mut group = c.benchmark_group("snap/find_1000");
    for size in SIZES {
        let generated = lines(size, SEED);
        let cursors = cursors_near_endpoints(&generated, &view);
        let drawing = Drawing::from_entities(generated.into_iter().map(Entity::from));
        group.bench_function(BenchmarkId::new("near_endpoints", size), |b| {
            b.iter(|| {
                cursors
                    .iter()
                    .filter_map(|cursor| {
                        find_snap(&drawing, &view, *cursor, SnapModes::default(), &[])
                    })
                    .count()
            })
        });
    }
    group.finish();
}

/// `count` lines through one point, like spokes: every one of them is a
/// candidate when the cursor is at the hub.
fn spokes(count: usize) -> Drawing {
    let spoke = |index: usize| {
        let angle = index as f64 * PI / count as f64;
        let reach = Point2::new(300.0 * angle.cos(), 300.0 * angle.sin());
        LineEntity::new(Point2::new(-reach.x, -reach.y), reach)
    };
    Drawing::from_entities((0..count).map(|index| Entity::from(spoke(index))))
}

fn by_local_density(c: &mut Criterion) {
    let view = view();
    let hub = view.world_to_screen(Point2::new(2.0, 1.0));
    let mut group = c.benchmark_group("snap/dense_spot");
    // 24 is the most candidates for which intersections are still searched.
    for count in [2, 8, 24, 25, 200, 2000] {
        let drawing = spokes(count);
        group.bench_function(BenchmarkId::new("spokes", count), |b| {
            b.iter(|| find_snap(&drawing, &view, black_box(hub), SnapModes::default(), &[]))
        });
    }
    group.finish();
}

criterion_group!(benches, by_drawing_size, by_local_density);
criterion_main!(benches);
