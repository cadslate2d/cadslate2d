//! How reliable the whole procedure is: every brief of a corpus solved
//! from many starting seeds — each `solve` its full budget of searches —
//! with the share that made a verified plan, or the refusal expected, and
//! the time each took, per brief.
//!
//! cargo run --release -p cad-layout --example reliability -- [test-data/layout/briefs.jsonl] [STARTS]
//!
//! `CIRCULATION=share` gives every brief that circulation budget — 0 to
//! forbid it. The table says how many plans had a circulation space, and
//! the largest it was, as a share of the rooms' floor.

use std::time::Instant;

use cad_layout::{Refusal, Spec, solve, verify};
use serde_json::Value;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "test-data/layout/briefs.jsonl".to_owned());
    let starts: u64 = args.next().and_then(|n| n.parse().ok()).unwrap_or(100);
    let text = std::fs::read_to_string(&path).expect("the corpus");
    let budget: Option<f64> = std::env::var("CIRCULATION")
        .ok()
        .and_then(|v| v.parse().ok());
    println!(
        "| Brief | Rooms | Expected | As expected | p50 ms | p90 ms | max ms | With circulation | Largest |"
    );
    println!("| --- | --- | --- | --- | --- | --- | --- | --- | --- |");
    let (mut all_right, mut all_runs) = (0u64, 0u64);
    let mut all_times = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let brief: Value = serde_json::from_str(line).expect("a brief");
        let expect = brief["expect"].as_str().unwrap_or("?").to_owned();
        let mut spec: Spec = serde_json::from_value(brief["spec"].clone()).expect("a spec");
        if let Some(share) = budget {
            spec.max_circulation = share;
        }
        let floor: f64 = spec.rooms.iter().map(|r| r.area).sum();
        let (mut circulated, mut largest) = (0u64, 0.0f64);
        let mut times = Vec::new();
        let mut right = 0u64;
        for start in 0..starts {
            // Starts far apart, so no two share a seed within their budgets.
            spec.seed = start.wrapping_mul(1_000_003);
            let began = Instant::now();
            let outcome = solve(&spec);
            times.push(began.elapsed().as_secs_f64() * 1000.0);
            if let Ok(plan) = &outcome
                && let Some(c) = plan.circulation
            {
                circulated += 1;
                largest = largest.max(plan.rooms[c].area() / floor);
            }
            let ok = match (&outcome, expect.as_str()) {
                (Ok(plan), "feasible" | "hard") => verify(&spec, plan).is_empty(),
                (Err(Refusal::Infeasible { .. }), "infeasible") => true,
                (Err(Refusal::Exhausted { .. }), "exhausted") => true,
                _ => false,
            };
            right += u64::from(ok);
        }
        all_right += right;
        all_runs += starts;
        all_times.extend(times.iter().copied());
        times.sort_by(f64::total_cmp);
        let pick = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
        println!(
            "| {} | {} | {expect} | {right}/{starts} | {:.1} | {:.1} | {:.1} | {circulated} | {:.0}% |",
            brief["name"].as_str().unwrap_or("?"),
            spec.rooms.len(),
            pick(0.5),
            pick(0.9),
            times.last().copied().unwrap_or(0.0),
            largest * 100.0
        );
    }
    all_times.sort_by(f64::total_cmp);
    let pick = |q: f64| all_times[((all_times.len() - 1) as f64 * q).round() as usize];
    println!(
        "\n{all_right}/{all_runs} as expected; p50 {:.1} ms, p90 {:.1} ms, p99 {:.1} ms, max {:.1} ms",
        pick(0.5),
        pick(0.9),
        pick(0.99),
        all_times.last().copied().unwrap_or(0.0)
    );
}
