//! The constraint solver (ADR-0038) on chains of constrained plates of
//! about 10, 100, and 1,000 constraints, one component each. Every scenario
//! asserts its outcome before it is timed, so a fast time is never a fast
//! failure taken for a solve:
//!
//! - long-range propagation: the first plate's width changes, and every
//!   plate after it moves (solved);
//! - local deformation: a grip drags a corner of the last plate whose
//!   height is free, and only that plate changes (solved);
//! - an over-constrained drag: the same with the height kept, which cannot
//!   be met (reported, nothing moved);
//! - the same count of constraints as separate parts, one changed;
//! - assessing a whole new constraint, and counting degrees of freedom.

use cad_benchmarks::constraints::{Plates, dragged, plates, widened};
use cad_geometry::constraint::{Equation, Tolerances, Verdict, assess, freedom, solve};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// Plates for about 10, 100, and 1,000 constraints.
const PLATES: [usize; 3] = [1, 9, 91];

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-6
}

/// The long-range edit settled as it should: the last plate 30 to the right.
fn check_widened(chain: &Plates) {
    let solution = solve(&widened(chain), Tolerances::default());
    assert!(solution.is_solved());
    let last = chain.lines.last().expect("a plate");
    let before = chain.system.values[last[0].a.x];
    let expected = if chain.lines.len() == 1 {
        before
    } else {
        before + 30.0
    };
    assert!(near(solution.values[last[0].a.x], expected));
    assert!(near(
        solution.values[chain.lines[0][0].b.x] - solution.values[chain.lines[0][0].a.x],
        150.0
    ));
}

/// The local edit: the last plate's top at 85, the other plates where they
/// were.
fn check_dragged(chain: &Plates) {
    let solution = solve(&dragged(chain, false), Tolerances::default());
    assert!(solution.is_solved());
    let last = chain.lines.last().expect("a plate");
    assert!(near(solution.values[last[2].b.y], 85.0));
    assert!(near(solution.values[last[0].a.y], 0.0));
    for plate in &chain.lines[..chain.lines.len() - 1] {
        for line in plate {
            for param in [line.a.x, line.a.y, line.b.x, line.b.y] {
                assert!(near(solution.values[param], chain.system.values[param]));
            }
        }
    }
}

fn constraints(c: &mut Criterion) {
    let mut group = c.benchmark_group("constraints");
    group.sample_size(10);
    let tolerances = Tolerances::default();
    for count in PLATES {
        let chain = plates(count, true);
        let n = chain.constraints;

        check_widened(&chain);
        let wider = widened(&chain);
        group.bench_function(BenchmarkId::new("long_range_solved", n), |b| {
            b.iter(|| solve(&wider, tolerances).iterations)
        });

        check_dragged(&chain);
        let drag = dragged(&chain, false);
        group.bench_function(BenchmarkId::new("local_drag_solved", n), |b| {
            b.iter(|| solve(&drag, tolerances).iterations)
        });

        let stuck = dragged(&chain, true);
        let refused = solve(&stuck, tolerances);
        assert!(!refused.is_solved());
        assert_eq!(refused.values, stuck.values);
        group.bench_function(BenchmarkId::new("overconstrained_drag_refused", n), |b| {
            b.iter(|| solve(&stuck, tolerances).unsatisfied.len())
        });

        let apart = plates(count, false);
        let mut first_wider = apart.system.clone();
        if let Some(Equation::Offset { value, .. }) = first_wider
            .equations
            .iter_mut()
            .find(|e| matches!(e, Equation::Offset { .. }))
        {
            *value = 150.0;
        }
        assert!(solve(&first_wider, tolerances).is_solved());
        group.bench_function(BenchmarkId::new("separate_parts_one_changed", n), |b| {
            b.iter(|| solve(&first_wider, tolerances).iterations)
        });

        let last = chain.lines.last().expect("a plate");
        let parallel = [Equation::Parallel {
            a: last[0],
            b: last[2],
        }];
        assert_eq!(
            assess(&chain.system, &parallel, tolerances).verdict,
            Verdict::Redundant
        );
        group.bench_function(BenchmarkId::new("assess", n), |b| {
            b.iter(|| assess(&chain.system, &parallel, tolerances))
        });
        assert_eq!(freedom(&chain.system, tolerances), 0);
        group.bench_function(BenchmarkId::new("freedom", n), |b| {
            b.iter(|| freedom(&chain.system, tolerances))
        });
    }
    group.finish();
}

criterion_group!(benches, constraints);
criterion_main!(benches);
