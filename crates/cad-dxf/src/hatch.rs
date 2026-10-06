//! Reading a HATCH (#169; ADR-0021).
//!
//! A HATCH's groups mean different things where they stand — 10 and 20 are
//! a vertex in a polyline path, a line's start in an edge path, an arc's
//! centre, a seed point; 72, 73, 93, and 97 likewise — so its groups are
//! kept in order and read here by walking them, rather than absorbed into
//! fields like every other record's.

use std::f64::consts::TAU;

use cad_core::Point2;
use cad_document::{HatchStyle, Path, hatch_pattern};

/// What could not be held of a HATCH, by report key.
pub(crate) const SPLINE_OR_ELLIPSE: &str = "(HATCH boundary with a spline or ellipse)";
pub(crate) const UNKNOWN_PATTERN: &str = "(HATCH pattern not in the table)";
pub(crate) const GRADIENT: &str = "(gradient HATCH)";

/// A HATCH as read: its loops and fill, and what was not held.
#[derive(Debug, Default)]
pub(crate) struct ReadHatch {
    pub loops: Vec<Path>,
    pub style: Option<HatchStyle>,
    /// Report keys, one per thing not held.
    pub dropped: Vec<&'static str>,
}

/// The groups of one record, in order, with a cursor.
struct Groups<'a> {
    groups: &'a [(i32, String)],
    at: usize,
}

impl Groups<'_> {
    fn peek(&self) -> Option<i32> {
        self.groups.get(self.at).map(|(code, _)| *code)
    }

    /// The next group if it has `code`.
    fn take(&mut self, code: i32) -> Option<&str> {
        let (found, value) = self.groups.get(self.at)?;
        if *found != code {
            return None;
        }
        self.at += 1;
        Some(value.as_str())
    }

    fn real(&mut self, code: i32) -> Option<f64> {
        self.take(code)?
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
    }

    fn int(&mut self, code: i32) -> Option<i64> {
        self.take(code)?.trim().parse().ok()
    }

    fn point(&mut self, x: i32) -> Option<Point2> {
        let px = self.real(x)?;
        let py = self.real(x + 10)?;
        // A Z, where a release writes one.
        self.take(x + 20);
        Some(Point2::new(px, py))
    }

    /// Moves on to the next group with `code`, past anything else.
    fn seek(&mut self, code: i32) -> bool {
        while let Some(found) = self.peek() {
            if found == code {
                return true;
            }
            self.at += 1;
        }
        false
    }
}

/// A boundary edge as the start of a path segment: where it starts and
/// ends, and its bulge.
struct Span {
    start: Point2,
    end: Point2,
    bulge: f64,
}

/// Reads a HATCH from its groups, `None` when it is not one at all.
pub(crate) fn read(groups: &[(i32, String)]) -> Option<ReadHatch> {
    let mut g = Groups { groups, at: 0 };
    let mut out = ReadHatch::default();
    g.seek(2);
    let name = g.take(2)?.trim().to_owned();
    g.seek(70);
    let solid = g.int(70)? != 0;
    g.seek(91);
    let paths = g.int(91)?.max(0);
    // The paths end where the hatch style (75) begins; what follows is the
    // pattern. A path not held is skipped to the next, never past them.
    let end = groups[g.at..]
        .iter()
        .position(|(code, _)| *code == 75)
        .map_or(groups.len(), |offset| g.at + offset);
    let mut g = Groups {
        groups: &groups[..end],
        at: g.at,
    };
    for _ in 0..paths {
        if !g.seek(92) {
            break;
        }
        let flags = g.int(92).unwrap_or(0);
        let path = if flags & 2 != 0 {
            polyline_path(&mut g)
        } else {
            edge_path(&mut g)
        };
        match path {
            Ok(path) => out.loops.push(path),
            Err(key) => out.dropped.push(key),
        }
        skip_sources(&mut g);
    }
    // The pattern: after the paths, the style (75) and kind (76), then for
    // a pattern its angle (52), scale (41), and lines (78 …).
    let mut angle = 0.0;
    let mut scale = 1.0;
    let mut first_line: Option<(Point2, usize)> = None;
    let rest = &groups[end..];
    let mut tail = Groups {
        groups: rest,
        at: 0,
    };
    while let Some(code) = tail.peek() {
        match code {
            52 => angle = tail.real(52).unwrap_or(0.0).to_radians(),
            41 => scale = tail.real(41).unwrap_or(1.0),
            53 if first_line.is_none() => {
                tail.at += 1;
                if let Some(origin) = tail.point_pair(43) {
                    first_line = Some((origin, 0));
                }
            }
            450 => {
                if tail.int(450).unwrap_or(0) != 0 {
                    out.dropped.push(GRADIENT);
                }
            }
            _ => tail.at += 1,
        }
    }
    out.style = Some(if solid {
        HatchStyle::Solid
    } else {
        match hatch_pattern(&name) {
            Some(pattern) => {
                // Where the pattern starts: its first line's point, as the
                // file placed it, less where that line starts in the table.
                let origin = first_line.map_or(Point2::ORIGIN, |(placed, _)| {
                    pattern.families.first().map_or(placed, |family| {
                        let local = family.origin - Point2::ORIGIN;
                        let (sin, cos) = angle.sin_cos();
                        let turned = cad_core::Vec2::new(
                            local.x * cos - local.y * sin,
                            local.x * sin + local.y * cos,
                        );
                        placed - turned * scale
                    })
                });
                HatchStyle::Pattern {
                    name: pattern.name.clone(),
                    angle,
                    scale,
                    origin,
                }
            }
            None => {
                out.dropped.push(UNKNOWN_PATTERN);
                HatchStyle::Solid
            }
        }
    });
    Some(out)
}

impl Groups<'_> {
    /// A point in groups `x` and `x + 1` (43 and 44): the pattern lines'
    /// own pairing, unlike every other point's.
    fn point_pair(&mut self, x: i32) -> Option<Point2> {
        let px = self.real(x)?;
        let py = self.real(x + 1)?;
        Some(Point2::new(px, py))
    }
}

/// A polyline path: whether it has bulges (72), closed (73), the count
/// (93), then each vertex (10, 20) and its bulge (42).
fn polyline_path(g: &mut Groups<'_>) -> Result<Path, &'static str> {
    let bulged = g.int(72).unwrap_or(0) != 0;
    g.int(73);
    let count = g.int(93).unwrap_or(0).max(0) as usize;
    let mut vertices = Vec::new();
    let mut bulges = Vec::new();
    for _ in 0..count {
        let Some(vertex) = g.point(10) else {
            break;
        };
        vertices.push(vertex);
        bulges.push(if bulged {
            g.real(42).unwrap_or(0.0)
        } else {
            0.0
        });
    }
    Ok(Path {
        vertices,
        bulges,
        closed: true,
    })
}

/// An edge path: the count (93), then each edge by its type (72) — a line
/// (1), an arc (2), an ellipse (3), a spline (4). A path with an ellipse
/// or a spline is not held.
fn edge_path(g: &mut Groups<'_>) -> Result<Path, &'static str> {
    let count = g.int(93).unwrap_or(0).max(0);
    let mut spans: Vec<Span> = Vec::new();
    for _ in 0..count {
        match g.int(72) {
            Some(1) => {
                let (Some(start), Some(end)) = (g.point(10), g.point(11)) else {
                    return Err(crate::import::INVALID);
                };
                spans.push(Span {
                    start,
                    end,
                    bulge: 0.0,
                });
            }
            Some(2) => {
                let center = g.point(10).ok_or(crate::import::INVALID)?;
                let radius = g.real(40).ok_or(crate::import::INVALID)?;
                let start = g.real(50).unwrap_or(0.0);
                let end = g.real(51).unwrap_or(360.0);
                let ccw = g.int(73).unwrap_or(1) != 0;
                spans.extend(arc_spans(center, radius, start, end, ccw));
            }
            Some(3 | 4) => return Err(SPLINE_OR_ELLIPSE),
            _ => return Err(crate::import::INVALID),
        }
    }
    chain(spans).ok_or(crate::import::INVALID)
}

/// An arc edge as spans. A clockwise arc's angles are stored negated: it
/// runs clockwise from −start to −end. A full turn is two half turns, as a
/// path cannot close on one vertex.
fn arc_spans(center: Point2, radius: f64, start: f64, end: f64, ccw: bool) -> Vec<Span> {
    let sweep_degrees = (end - start).rem_euclid(360.0);
    // The angles as the file wrote them: 0 to 360 is a full turn, 0 to 0
    // nothing — told apart exactly, not within a tolerance.
    #[allow(clippy::float_cmp)]
    let full_turn = sweep_degrees == 0.0 && end != start;
    let sweep = if full_turn {
        TAU
    } else {
        sweep_degrees.to_radians()
    };
    let (from, direction) = if ccw {
        (start.to_radians(), 1.0)
    } else {
        ((-start).to_radians(), -1.0)
    };
    let at = |angle: f64| center + cad_core::Vec2::from_polar(radius, angle);
    if sweep >= TAU - 1e-12 {
        let half = from + direction * std::f64::consts::PI;
        return vec![
            Span {
                start: at(from),
                end: at(half),
                bulge: direction,
            },
            Span {
                start: at(half),
                end: at(from),
                bulge: direction,
            },
        ];
    }
    vec![Span {
        start: at(from),
        end: at(from + direction * sweep),
        bulge: direction * (sweep / 4.0).tan(),
    }]
}

/// The spans joined into a closed path: each starting where the last ended,
/// turned round where it was written the other way.
fn chain(spans: Vec<Span>) -> Option<Path> {
    let mut vertices = Vec::new();
    let mut bulges = Vec::new();
    let mut last: Option<Point2> = None;
    for span in spans {
        let near = |a: Point2, b: Point2| {
            a.distance_to(b) <= 1e-9 * (1.0 + a.x.abs() + a.y.abs() + b.x.abs() + b.y.abs())
        };
        let (start, bulge) = match last {
            Some(previous) if !near(previous, span.start) && near(previous, span.end) => {
                (span.end, -span.bulge)
            }
            _ => (span.start, span.bulge),
        };
        let end = if start == span.start {
            span.end
        } else {
            span.start
        };
        vertices.push(start);
        bulges.push(bulge);
        last = Some(end);
    }
    (!vertices.is_empty()).then_some(Path {
        vertices,
        bulges,
        closed: true,
    })
}

/// The source objects a path names (97, then 330 each): not held.
fn skip_sources(g: &mut Groups<'_>) {
    if let Some(count) = g.int(97) {
        for _ in 0..count.max(0) {
            if g.take(330).is_none() {
                break;
            }
        }
    }
}
