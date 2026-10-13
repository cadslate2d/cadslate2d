//! The DXF reader on bytes from strangers (#177): whatever comes in, it
//! must answer with a drawing or a structured error, never a panic; what
//! the writer then writes, the reader must read; and reading it again must
//! write the same file — to floating-point precision, since what the file
//! holds derived (degrees from radians, the points of a radius dimension
//! from its radius) comes back an ulp off. The curated fixtures hold the
//! stricter bar, byte for byte.
#![no_main]

use cad_dxf::{ImportLimits, export, import_with_limits};
use libfuzzer_sys::fuzz_target;

/// Smaller than the defaults, so a run spends its time on structure
/// rather than on allocating what a header claims.
fn limits() -> ImportLimits {
    ImportLimits {
        max_bytes: 1 << 20,
        max_entities: 10_000,
        max_table_rows: 1_000,
        max_record_groups: 200_000,
        max_value_bytes: 4 * 1024,
        ..ImportLimits::default()
    }
}

/// A piece of a value: a run of digits (with a point and a sign) read as
/// a number, or the text between such runs — a dimension's text is a
/// number after a `Ø` or before a unit.
#[derive(Debug, PartialEq)]
enum Piece<'a> {
    Number(f64),
    Text(&'a str),
}

fn pieces(value: &str) -> Vec<Piece<'_>> {
    let mut out = Vec::new();
    let bytes = value.as_bytes();
    let mut start = 0;
    let mut i = 0;
    let numeric = |i: usize| {
        let c = bytes[i];
        c.is_ascii_digit() || c == b'.' || (c == b'-' && bytes.get(i + 1).is_some_and(u8::is_ascii_digit))
    };
    while i < bytes.len() {
        if numeric(i) {
            if start < i {
                out.push(Piece::Text(&value[start..i]));
            }
            let from = i;
            i += 1;
            while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            match value[from..i].parse::<f64>() {
                Ok(number) => out.push(Piece::Number(number)),
                Err(_) => out.push(Piece::Text(&value[from..i])),
            }
            start = i;
        } else {
            i += 1;
        }
    }
    if start < bytes.len() {
        out.push(Piece::Text(&value[start..]));
    }
    out
}

/// Whether two group values are the same: equal as text, or the same
/// pieces with every number within a few ulps of the largest magnitude
/// in the file — a derived point drifts by ulps of the radius or distance
/// it was derived from, not of its own coordinates, and a dimension's
/// text is that number formatted.
fn same(a: &str, b: &str, scale: f64) -> bool {
    if a == b {
        return true;
    }
    let (pa, pb) = (pieces(a), pieces(b));
    pa.len() == pb.len()
        && pa.iter().zip(&pb).all(|(x, y)| match (x, y) {
            (Piece::Number(x), Piece::Number(y)) => {
                (x - y).abs() <= 1e-12 * x.abs().max(y.abs()).max(scale).max(1.0)
            }
            (x, y) => x == y,
        })
}

/// The largest magnitude among the file's numbers.
fn largest_magnitude(text: &str) -> f64 {
    text.lines()
        .filter_map(|line| line.trim().parse::<f64>().ok())
        .filter(|value| value.is_finite())
        .fold(0.0, |largest: f64, value| largest.max(value.abs()))
}

fuzz_target!(|data: &[u8]| {
    let limits = limits();
    let Ok(first) = import_with_limits(data, &limits) else {
        return;
    };
    let written = export(&first.drawing);
    let again = import_with_limits(written.as_bytes(), &limits)
        .expect("what the writer wrote, the reader reads");
    let rewritten = export(&again.drawing);
    let scale = largest_magnitude(&written);
    let (mut a, mut b) = (written.lines(), rewritten.lines());
    let mut line = 0;
    loop {
        match (a.next(), b.next()) {
            (None, None) => break,
            (Some(x), Some(y)) if same(x, y, scale) => {}
            (x, y) => panic!("export → import → export differs at line {line}: {x:?} / {y:?}"),
        }
        line += 1;
    }
});
