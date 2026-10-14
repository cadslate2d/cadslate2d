//! The overlay rebuilt on every pointer move (#210): on the mixed drawing
//! at 10k, 50k, and 100k, with nothing in it but what it draws from the
//! drawing — the far curves of ADR-0009 — at the drawing's extents and
//! zoomed in a hundredfold on its centre.
//!
//! Run with `cargo bench -p cad-benchmarks --bench overlay`.

use cad_benchmarks::{SIZES, mixed};
use cad_core::{ScreenPoint, View};
use cad_render::OverlayFrame;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

const SEED: u64 = 2026;

fn overlay_rebuild(c: &mut Criterion) {
    for size in SIZES {
        let drawing = mixed(size, SEED);
        let extents = drawing.extents().expect("a drawing with something in it");
        let mut at_extents = View::new(1920.0, 1080.0);
        at_extents.fit(&extents);
        let mut zoomed = at_extents.clone();
        zoomed.zoom_about(ScreenPoint::new(960.0, 540.0), 100.0);
        for (name, view) in [("extents", &at_extents), ("zoomed", &zoomed)] {
            let mut overlay = OverlayFrame::new();
            let mut group = c.benchmark_group(format!("overlay/rebuild_{name}"));
            group.bench_function(BenchmarkId::from_parameter(size), |b| {
                b.iter(|| {
                    overlay.rebuild(&[], &drawing, view);
                    overlay.frame().segment_count()
                })
            });
            group.finish();
        }
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(20);
    targets = overlay_rebuild
}
criterion_main!(benches);
