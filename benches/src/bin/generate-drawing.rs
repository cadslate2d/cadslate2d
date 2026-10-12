//! Writes a generated benchmark drawing as DXF.
//!
//! ```text
//! cargo run --release -p cad-benchmarks --bin generate-drawing -- 100000 benches/generated-drawings/lines-100k.dxf
//! cargo run --release -p cad-benchmarks --bin generate-drawing -- --mixed 100000 benches/generated-drawings/mixed-100k.dxf
//! ```
//!
//! `--mixed` writes every kind of entity in the proportions of
//! `cad_benchmarks::MIXED_PER_HUNDRED` instead of lines alone.

use std::process::ExitCode;

use cad_benchmarks::{lines, mixed};
use cad_document::{Drawing, Entity};

const SEED: u64 = 2026;

fn main() -> ExitCode {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let is_mixed = args.first().is_some_and(|flag| flag == "--mixed");
    if is_mixed {
        args.remove(0);
    }
    let (Some(count), Some(path)) = (
        args.first().and_then(|text| text.parse::<usize>().ok()),
        args.get(1),
    ) else {
        eprintln!("usage: generate-drawing [--mixed] <entity count> <output.dxf>");
        return ExitCode::FAILURE;
    };

    let drawing = if is_mixed {
        mixed(count, SEED)
    } else {
        Drawing::from_entities(lines(count, SEED).into_iter().map(Entity::Line))
    };
    match std::fs::write(path, cad_dxf::export(&drawing)) {
        Ok(()) => {
            let what = if is_mixed { "mixed entities" } else { "lines" };
            println!("wrote {count} {what} to {path}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {path}: {error}");
            ExitCode::FAILURE
        }
    }
}
