//! Writes a generated benchmark drawing as DXF.
//!
//! ```text
//! cargo run --release -p cad-benchmarks --bin generate-drawing -- 100000 benches/generated-drawings/lines-100k.dxf
//! ```

use std::process::ExitCode;

use cad_benchmarks::lines;
use cad_document::{Drawing, Entity};

const SEED: u64 = 2026;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(count), Some(path)) = (
        args.first().and_then(|text| text.parse::<usize>().ok()),
        args.get(1),
    ) else {
        eprintln!("usage: generate-drawing <line count> <output.dxf>");
        return ExitCode::FAILURE;
    };

    let drawing = Drawing::from_entities(lines(count, SEED).into_iter().map(Entity::Line));
    match std::fs::write(path, cad_dxf::export(&drawing)) {
        Ok(()) => {
            println!("wrote {count} lines to {path}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cannot write {path}: {error}");
            ExitCode::FAILURE
        }
    }
}
