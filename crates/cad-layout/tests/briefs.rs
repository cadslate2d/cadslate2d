//! The corpus of briefs (#277, #278): each is laid out — every plan keeping
//! every hard constraint, checked by `verify` — or refused as the brief
//! expects: infeasible, with a reason proved before any search, or
//! exhausted, its search finding nothing. A brief at the edge of
//! feasibility (`hard`) may come out either way, but never as anything
//! else.

use cad_layout::{Refusal, Spec, alternatives, verify};
use serde_json::Value;

#[test]
fn every_brief_comes_out_as_expected() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-data/layout/briefs.jsonl"
    );
    let text = std::fs::read_to_string(path).unwrap();
    let mut misses = Vec::new();
    let mut count = 0;
    for (index, line) in text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
    {
        count += 1;
        let brief: Value = serde_json::from_str(line).unwrap();
        let spec: Spec = serde_json::from_value(brief["spec"].clone()).unwrap();
        let at = format!("briefs.jsonl:{} ({})", index + 1, brief["name"]);
        let reason = brief["reason"].as_str().unwrap_or_default();
        let said = |refusal: &Refusal| refusal.reasons().iter().any(|r| r.contains(reason));
        match (brief["expect"].as_str(), alternatives(&spec, 1)) {
            (Some("feasible"), Ok(plans)) => {
                for plan in plans {
                    let broken = verify(&spec, &plan);
                    if !broken.is_empty() {
                        misses.push(format!("{at}: {broken:?}"));
                    }
                }
            }
            (Some("hard"), Ok(plans)) => {
                if plans.iter().any(|plan| !verify(&spec, plan).is_empty()) {
                    misses.push(format!("{at}: a plan that breaks its brief"));
                }
            }
            (Some("hard"), Err(Refusal::Exhausted { .. })) => {}
            (Some("infeasible"), Err(refusal @ Refusal::Infeasible { .. }))
            | (Some("exhausted"), Err(refusal @ Refusal::Exhausted { .. })) => {
                if !said(&refusal) {
                    misses.push(format!("{at}: refused, but for {refusal}"));
                }
            }
            (expect, Ok(_)) => misses.push(format!("{at}: expected {expect:?}, made a plan")),
            (expect, Err(refusal)) => {
                misses.push(format!("{at}: expected {expect:?}, refused: {refusal:?}"));
            }
        }
    }
    assert!(count >= 30, "{count} briefs");
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}
