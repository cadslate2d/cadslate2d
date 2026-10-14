//! Construction lines (#206; ADR-0031): what a hundred of them cost on the
//! 100k mixed drawing, where they are felt — the overlay rebuilt on every
//! pointer move, which draws them clipped to the view, and the pick and
//! snap query, which tests them one by one beside the index's tree — with
//! none for the comparison.
//!
//! Run with `cargo bench -p cad-benchmarks --bench construction`.

use cad_benchmarks::mixed;
use cad_core::{PixelTolerance, Point2, ScreenPoint, Vec2, View};
use cad_document::{Drawing, XLineEntity};
use cad_render::OverlayFrame;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const SEED: u64 = 2026;
const SIZE: usize = 100_000;

/// The mixed drawing with `count` xlines through its extents, at angles
/// spread round the half turn.
fn with_construction_lines(count: usize) -> (Drawing, View) {
    let mut drawing = mixed(SIZE, SEED);
    let extents = drawing.extents().expect("a drawing with something in it");
    let mut tx = drawing.transaction();
    for index in 0..count {
        let t = index as f64 / count.max(1) as f64;
        let base = extents.min() + (extents.max() - extents.min()) * t;
        let direction = Vec2::from_polar(1.0, std::f64::consts::PI * t);
        tx.insert(XLineEntity::new(base, direction).expect("a direction"))
            .expect("the current layer exists");
    }
    tx.commit();
    let mut view = View::new(1920.0, 1080.0);
    view.fit(&extents);
    (drawing, view)
}

fn construction_lines(c: &mut Criterion) {
    for count in [0, 100] {
        let (drawing, view) = with_construction_lines(count);

        // What a pointer move costs in the overlay: everything it draws
        // from the drawing — far curves, construction lines — and nothing
        // else, so the difference is theirs.
        let mut overlay = OverlayFrame::new();
        let mut group = c.benchmark_group("construction/overlay_rebuild");
        group.bench_function(BenchmarkId::from_parameter(count), |b| {
            b.iter(|| {
                overlay.rebuild(&[], &drawing, &view);
                overlay.frame().segment_count()
            })
        });
        group.finish();

        // A pick or snap query at a point of the view: the tree's
        // candidates and the side list's.
        let at = view.world_to_screen(view.center() + Vec2::new(3.0, 2.0));
        let bounds = view.pick_bounds(ScreenPoint::new(at.x, at.y), PixelTolerance(10.0));
        let mut group = c.benchmark_group("construction/pick_query");
        group.bench_function(BenchmarkId::from_parameter(count), |b| {
            b.iter(|| drawing.query_shown(&bounds).len())
        });
        group.finish();
        let _ = Point2::ORIGIN;
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(30);
    targets = construction_lines
}
criterion_main!(benches);
