//! Grip editing on a very large entity (issue #90).
//!
//! One pointer frame while a vertex of a 10,000-vertex polyline is being
//! dragged: the session handles the move, describes the overlay, and
//! `cad-render` packs it. That is everything Rust does per frame; it has to
//! fit comfortably inside 16 ms.

use cad_commands::{Modifiers, PointerButton, Session};
use cad_core::{Point2, ScreenPoint};
use cad_document::{Drawing, Entity, PolylineEntity};
use cad_render::OverlayFrame;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// A zig-zag polyline of `count` vertices across the view.
fn zigzag(count: usize) -> Entity {
    let vertices = (0..count).map(|index| {
        let x = index as f64 * (1800.0 / count as f64) - 900.0;
        Point2::new(x, if index % 2 == 0 { 0.0 } else { 40.0 })
    });
    PolylineEntity::new(vertices.collect(), false).into()
}

fn click(session: &mut Session, at: ScreenPoint) {
    session.pointer_down(at, PointerButton::Primary, Modifiers::NONE);
    session.pointer_up(at, PointerButton::Primary, Modifiers::NONE);
}

/// A session with the polyline selected and its first vertex's grip in hand.
fn dragging(count: usize) -> Session {
    let mut session = Session::with_drawing(Drawing::from_entities([zigzag(count)]));
    session.resize(1920.0, 1080.0);
    let first = session.view().world_to_screen(Point2::new(-900.0, 0.0));
    click(&mut session, first); // select
    click(&mut session, first); // take the grip
    assert!(session.active_grip().is_some(), "the grip was not taken");
    session
}

fn drag_frame(c: &mut Criterion) {
    let mut group = c.benchmark_group("grips/drag_frame");
    for count in [100, 1_000, 10_000] {
        let mut session = dragging(count);
        let mut overlay = OverlayFrame::new();
        let mut step = 0.0;
        group.bench_function(BenchmarkId::new("polyline_vertices", count), |b| {
            b.iter(|| {
                step = (step + 1.0) % 300.0;
                session.pointer_move(ScreenPoint::new(200.0 + step, 300.0), Modifiers::NONE);
                overlay.rebuild(&session.overlay(), session.drawing(), session.view());
                overlay.frame().segment_count()
            })
        });
    }
    group.finish();
}

fn commit(c: &mut Criterion) {
    let mut group = c.benchmark_group("grips/commit_and_undo");
    for count in [100, 1_000, 10_000] {
        let mut session = dragging(count);
        let first = session.view().world_to_screen(Point2::new(-900.0, 0.0));
        let drop = ScreenPoint::new(first.x + 40.0, first.y - 60.0);
        group.bench_function(BenchmarkId::new("polyline_vertices", count), |b| {
            b.iter(|| {
                click(&mut session, drop); // commit
                session.key(cad_commands::KeyAction::Undo);
                click(&mut session, first); // take the grip again
            })
        });
    }
    group.finish();
}

criterion_group!(benches, drag_frame, commit);
criterion_main!(benches);
