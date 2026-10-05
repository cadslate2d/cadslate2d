//! The second candidate in the polyline-offset experiment (ADR-0011):
//! cleaning up a raw offset by winding number.
//!
//! This is experiment code. It exists so the comparison in the ADR can be
//! re-run; the application uses `cad_geometry::offset_path`.
//!
//! The idea: the raw offset of a closed shape is a closed curve that crosses
//! itself. Cut it at its self-crossings. A piece belongs to the true offset
//! exactly when the raw curve winds once round the region on its inner side
//! and not at all round the region on its outer side.
//!
//! It applies to closed, simple (not self-crossing) shapes only: an open
//! path has no inside to wind round.

use std::f64::consts::TAU;

use cad_core::{Point2, Vec2};
use cad_geometry::{Path, PathSpan, Primitive, path_spans, stitch_spans};

/// Chords an arc is divided into to add up the angle it subtends.
const CHORDS_PER_ARC: u32 = 48;

/// The raw offset as one continuous closed curve. Unlike the shipped raw
/// offset, an arc that collapses must not leave a gap, or winding numbers
/// mean nothing: it is replaced by a straight connector.
fn continuous_raw_offset(path: &Path, signed: f64) -> Vec<PathSpan> {
    let spans = path_spans(path);
    let mut raw: Vec<PathSpan> = Vec::new();
    let normal = |tangent: Vec2| {
        let length = tangent.length();
        Vec2::new(-tangent.y, tangent.x) * (signed / length)
    };
    for span in &spans {
        let piece = span.offset(signed).unwrap_or_else(|| {
            // Collapsed: connect where its ends would have been.
            let leaving = span.point_at(1e-6) - span.start;
            let arriving = span.end - span.point_at(1.0 - 1e-6);
            PathSpan {
                start: span.start + normal(leaving),
                end: span.end + normal(arriving),
                bulge: 0.0,
            }
        });
        if let Some(previous) = raw.last().copied() {
            join(&mut raw, previous.end, piece.start, span.start);
        }
        raw.push(piece);
    }
    if let (Some(first), Some(last)) = (raw.first().copied(), raw.last().copied()) {
        join(&mut raw, last.end, first.start, spans[0].start);
    }
    raw
}

/// Closes the gap at a vertex with an arc about it, the short way round.
fn join(raw: &mut Vec<PathSpan>, from: Point2, to: Point2, vertex: Point2) {
    if from.distance_to(to) < 1e-9 {
        return;
    }
    let (a, b) = (from - vertex, to - vertex);
    let turn = (a.x * b.y - a.y * b.x).atan2(a.dot(b));
    raw.push(PathSpan {
        start: from,
        end: to,
        bulge: (turn / 4.0).tan(),
    });
}

/// How many times the closed curve `raw` winds round `point`.
fn winding(raw: &[PathSpan], point: Point2) -> f64 {
    let subtended = |from: Point2, to: Point2| {
        let (a, b) = (from - point, to - point);
        (a.x * b.y - a.y * b.x).atan2(a.dot(b))
    };
    let mut total = 0.0;
    for span in raw {
        match span.primitive() {
            Primitive::Segment(_) => total += subtended(span.start, span.end),
            _ => {
                for step in 0..CHORDS_PER_ARC {
                    let from = span.point_at(f64::from(step) / f64::from(CHORDS_PER_ARC));
                    let to = span.point_at(f64::from(step + 1) / f64::from(CHORDS_PER_ARC));
                    total += subtended(from, to);
                }
            }
        }
    }
    (total / TAU).round()
}

/// Twice the signed area: positive for a counter-clockwise shape.
fn orientation(path: &Path) -> f64 {
    let vertices = &path.vertices;
    let straight: f64 = (0..vertices.len())
        .map(|index| {
            let (a, b) = (vertices[index], vertices[(index + 1) % vertices.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    // Arc segments add or remove their circular segment.
    let bulging: f64 = path_spans(path)
        .iter()
        .map(|span| match span.primitive() {
            Primitive::Arc(arc) => {
                let segment = arc.radius * arc.radius * (arc.sweep - arc.sweep.sin());
                if span.bulge > 0.0 { segment } else { -segment }
            }
            _ => 0.0,
        })
        .sum();
    straight + bulging
}

/// The offset of a closed, simple path by winding-number cleanup.
pub fn offset_by_winding(path: &Path, signed: f64) -> Vec<Path> {
    assert!(path.closed, "winding-number cleanup needs a closed shape");
    let raw = continuous_raw_offset(path, signed);
    let shapes: Vec<Primitive> = raw.iter().map(PathSpan::primitive).collect();
    let turns = orientation(path).signum();
    let probe = signed.abs() * 1e-4;

    let mut kept: Vec<PathSpan> = Vec::new();
    for (index, piece) in raw.iter().enumerate() {
        // Cut at every crossing with every other raw piece.
        let mut cuts: Vec<(f64, Point2)> = shapes
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .flat_map(|(_, other)| shapes[index].intersect(other).as_slice().to_vec())
            .filter(|cut| cut.distance_to(piece.start) > 1e-9 && cut.distance_to(piece.end) > 1e-9)
            .map(|cut| (piece.parameter_of(cut), cut))
            .collect();
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        cuts.dedup_by(|later, earlier| later.1.distance_to(earlier.1) < 1e-9);
        let stops: Vec<(f64, Point2)> = std::iter::once((0.0, piece.start))
            .chain(cuts)
            .chain([(1.0, piece.end)])
            .collect();

        for pair in stops.windows(2) {
            let (from, to) = (pair[0].0, pair[1].0);
            let middle = piece.point_at(from + (to - from) / 2.0);
            let ahead = piece.point_at(from + (to - from) * 0.5001) - middle;
            let length = ahead.length();
            if length == 0.0 {
                continue;
            }
            let left = Vec2::new(-ahead.y, ahead.x) * (probe / length);
            // For a counter-clockwise shape the offset's inside is on its left.
            let (inner, outer) = if turns > 0.0 {
                (middle + left, middle + (-left))
            } else {
                (middle + (-left), middle + left)
            };
            let inside = winding(&raw, inner) * turns;
            let outside = winding(&raw, outer) * turns;
            if inside >= 1.0 && outside <= 0.0 {
                let share = to - from;
                kept.push(PathSpan {
                    start: pair[0].1,
                    end: pair[1].1,
                    bulge: (piece.bulge.atan() * share).tan(),
                });
            }
        }
    }
    stitch_spans(kept)
}
