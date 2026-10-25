//! The constraint solver (ADR-0038) on chains of constrained plates of
//! about 10, 100, and 1,000 constraints: one component, settled after a
//! dimension changes and after a grip drags a corner; the same count as
//! separate parts; whether a new constraint over-constrains; and the dense
//! count of degrees of freedom.

use cad_benchmarks::constraints::{dragged, plates, widened};
use cad_geometry::constraint::{Equation, Tolerances, freedom, redundant, solve};
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

/// Plates for about 10, 100, and 1,000 constraints.
const PLATES: [usize; 3] = [1, 9, 91];

fn constraints(c: &mut Criterion) {
    let mut group = c.benchmark_group("constraints");
    group.sample_size(10);
    for count in PLATES {
        let chain = plates(count, true);
        let n = chain.constraints;
        let wider = widened(&chain);
        assert!(solve(&wider, Tolerances::default()).is_solved());
        group.bench_function(BenchmarkId::new("dimension_changed", n), |b| {
            b.iter(|| solve(&wider, Tolerances::default()).iterations)
        });
        let drag = dragged(&chain);
        assert!(solve(&drag, Tolerances::default()).is_solved());
        group.bench_function(BenchmarkId::new("grip_dragged", n), |b| {
            b.iter(|| solve(&drag, Tolerances::default()).iterations)
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
        group.bench_function(BenchmarkId::new("separate_parts_one_changed", n), |b| {
            b.iter(|| solve(&first_wider, Tolerances::default()).iterations)
        });
        let last = chain.lines.last().expect("a plate");
        let parallel = Equation::Parallel {
            a: last[0],
            b: last[2],
        };
        assert!(redundant(&chain.system, &parallel));
        group.bench_function(BenchmarkId::new("redundant", n), |b| {
            b.iter(|| redundant(&chain.system, &parallel))
        });
        assert_eq!(freedom(&chain.system), 0);
        group.bench_function(BenchmarkId::new("freedom", n), |b| {
            b.iter(|| freedom(&chain.system))
        });
    }
    group.finish();
}

criterion_group!(benches, constraints);
criterion_main!(benches);
