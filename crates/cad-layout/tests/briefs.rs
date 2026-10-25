//! The corpus of briefs (#277, #278): each is laid out — every plan keeping
//! every hard constraint, checked by `verify` — or refused with the reason
//! it expects.

use cad_layout::{Spec, alternatives, verify};
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
        match (brief["expect"].as_str(), alternatives(&spec, 1)) {
            (Some("feasible"), Ok(plans)) => {
                for plan in plans {
                    let broken = verify(&spec, &plan);
                    if !broken.is_empty() {
                        misses.push(format!("{at}: {broken:?}"));
                    }
                }
            }
            (Some("infeasible"), Err(refused)) => {
                let reason = brief["reason"].as_str().unwrap_or_default();
                if !refused.reasons.iter().any(|r| r.contains(reason)) {
                    misses.push(format!("{at}: refused, but for {refused}"));
                }
            }
            (expect, Ok(_)) => misses.push(format!("{at}: expected {expect:?}, made a plan")),
            (expect, Err(refused)) => {
                misses.push(format!("{at}: expected {expect:?}, refused: {refused}"));
            }
        }
    }
    assert!(count >= 30, "{count} briefs");
    assert!(misses.is_empty(), "{}", misses.join("\n"));
}
