//! Runs every brief of a corpus and reports what came of each: a plan,
//! checked by `verify`, or the reasons none was found; how long it took;
//! and the success rate — the briefs whose outcome is the one expected.
//!
//! cargo run --release -p cad-layout --example briefs -- [test-data/layout/briefs.jsonl] [ALTERNATIVES]
//!
//! With `SVG=DIR` it draws each brief's best plan as DIR/<name>.svg: the
//! footprint, the rooms on their centrelines, their floors, and the doors'
//! walls. With `SEEDS=N` it also runs one search per seed 0..N on every feasible
//! brief, and reports how often a single search finds a plan.

use std::time::Instant;

use cad_layout::{Spec, alternatives, verify};
use serde_json::Value;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args
        .next()
        .unwrap_or_else(|| "test-data/layout/briefs.jsonl".to_owned());
    let wanted: usize = args.next().and_then(|n| n.parse().ok()).unwrap_or(1);
    let text = std::fs::read_to_string(&path).expect("the corpus");
    let (mut right, mut total) = (0, 0);
    let mut times = Vec::new();
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let brief: Value = serde_json::from_str(line).expect("a brief");
        let name = brief["name"].as_str().unwrap_or("?");
        let expect = brief["expect"].as_str().unwrap_or("?");
        let spec: Spec = serde_json::from_value(brief["spec"].clone()).expect("a spec");
        let started = Instant::now();
        let outcome = alternatives(&spec, wanted);
        let elapsed = started.elapsed().as_secs_f64() * 1000.0;
        times.push(elapsed);
        total += 1;
        let (verdict, said) = match &outcome {
            Ok(plans) => {
                if let Ok(dir) = std::env::var("SVG") {
                    std::fs::write(format!("{dir}/{name}.svg"), svg(&spec, &plans[0]))
                        .expect("the SVG");
                }
                let broken: Vec<String> = plans.iter().flat_map(|p| verify(&spec, p)).collect();
                let ok = (expect == "feasible" || expect == "hard") && broken.is_empty();
                (
                    ok,
                    format!(
                        "{} plan(s), energy {:.2}{}",
                        plans.len(),
                        plans[0].energy,
                        if broken.is_empty() {
                            String::new()
                        } else {
                            format!(", BROKEN: {broken:?}")
                        }
                    ),
                )
            }
            Err(refusal) => {
                let reason = brief["reason"].as_str().unwrap_or("");
                let kind = match refusal {
                    cad_layout::Refusal::Infeasible { .. } => "infeasible",
                    cad_layout::Refusal::Exhausted { .. } => "exhausted",
                    cad_layout::Refusal::Invalid(_) => "invalid",
                };
                let ok = (expect == kind || (expect == "hard" && kind == "exhausted"))
                    && refusal.reasons().iter().any(|r| r.contains(reason));
                (ok, format!("{kind}: {refusal}"))
            }
        };
        if verdict {
            right += 1;
        }
        println!(
            "{} {name:16} {expect:10} {elapsed:8.1} ms  {said}",
            if verdict { "ok  " } else { "MISS" }
        );
    }
    if let Some(seeds) = std::env::var("SEEDS")
        .ok()
        .and_then(|n| n.parse::<u64>().ok())
    {
        let (mut found, mut runs) = (0, 0);
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let brief: Value = serde_json::from_str(line).expect("a brief");
            if brief["expect"] != "feasible" {
                continue;
            }
            let mut spec: Spec = serde_json::from_value(brief["spec"].clone()).expect("a spec");
            let mut here = 0;
            for seed in 0..seeds {
                spec.seed = seed;
                runs += 1;
                let one = cad_layout::alternatives_within(&spec, 1, 1);
                if one.is_ok_and(|plans| verify(&spec, &plans[0]).is_empty()) {
                    found += 1;
                    here += 1;
                }
            }
            if here < seeds {
                println!(
                    "  {}: {here}/{seeds} single searches found a plan",
                    brief["name"]
                );
            }
        }
        println!("single searches: {found}/{runs} found a plan");
    }
    times.sort_by(f64::total_cmp);
    let pick = |q: f64| times[((times.len() - 1) as f64 * q).round() as usize];
    println!(
        "\n{right}/{total} as expected; time per brief p50 {:.1} ms, p90 {:.1} ms, max {:.1} ms",
        pick(0.5),
        pick(0.9),
        times.last().copied().unwrap_or(0.0)
    );
}

/// A plan drawn as it will be drafted: the terrain, the walls' faces and
/// jambs, each door's leaf and swing, and each room's name and area. A
/// metre is 20 units.
fn svg(spec: &Spec, plan: &cad_layout::Layout) -> String {
    let k = 20.0;
    let (w, d) = (spec.terrain.width, spec.terrain.depth);
    let p = |q: cad_core::Point2| format!("{:.2},{:.2}", q.x * k, (d - q.y) * k);
    let line = |a: cad_core::Point2, b: cad_core::Point2, style: &str| {
        format!("<polyline points='{} {}' style='{style}'/>", p(a), p(b))
    };
    let mut out = format!(
        "<svg xmlns='http://www.w3.org/2000/svg' width='{:.0}' height='{:.0}' font-family='sans-serif' font-size='10'>",
        w * k,
        d * k
    );
    out.push_str(&format!(
        "<rect width='{:.0}' height='{:.0}' style='fill:#f4f7f0;stroke:#999'/>",
        w * k,
        d * k
    ));
    let walls = cad_layout::walls(spec, plan);
    let ring: Vec<String> = walls.outside.iter().map(|q| p(*q)).collect();
    out.push_str(&format!(
        "<{} points='{}' style='fill:none;stroke:#000;stroke-width:1.5'/>",
        if walls.outside_closed {
            "polygon"
        } else {
            "polyline"
        },
        ring.join(" ")
    ));
    for f in walls.exterior.iter().chain(&walls.interior) {
        out.push_str(&line(f.start, f.end, "stroke:#000;stroke-width:1"));
    }
    for (a, b) in walls.exterior_jambs.iter().chain(&walls.interior_jambs) {
        out.push_str(&line(*a, *b, "stroke:#000;stroke-width:1"));
    }
    for door in cad_layout::doors(spec, plan) {
        out.push_str(&line(door.hinge, door.leaf, "stroke:#26a;stroke-width:1"));
        let r = door.hinge.distance_to(door.leaf);
        let at = |angle: f64| {
            cad_core::Point2::new(
                door.hinge.x + r * angle.cos(),
                door.hinge.y + r * angle.sin(),
            )
        };
        out.push_str(&format!(
            "<path d='M {} A {:.2} {:.2} 0 0 0 {}' style='fill:none;stroke:#26a;stroke-width:0.6'/>",
            p(at(door.swing.0)),
            r * k,
            r * k,
            p(at(door.swing.1))
        ));
    }
    for label in cad_layout::labels(plan) {
        out.push_str(&format!(
            "<text x='{:.1}' y='{:.1}' text-anchor='middle'>{}</text><text x='{:.1}' y='{:.1}' text-anchor='middle'>{:.2} m²</text>",
            label.at.x * k,
            (d - label.at.y) * k - 2.0,
            label.name,
            label.at.x * k,
            (d - label.at.y) * k + 10.0,
            label.area
        ));
    }
    out.push_str("</svg>");
    out
}
