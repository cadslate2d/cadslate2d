//! A part's orthographic views (#339, ADR-0040), worked out in closed form
//! from its description — never a solid model.
//!
//! **A prism** is a profile in the front plane — `x` across, `z` up — of
//! straight edges, extruded `depth` back along `y`, with holes drilled
//! right through it along one axis each. **A turned part** is a stepped
//! profile of diameters along the `x` axis, with a bore right through.
//!
//! **The views**, each in its own plane's coordinates:
//! - the front, looking back along +y: `(x, z)`;
//! - the top, looking down: `(x, y)`, the front face at the bottom;
//! - the right side, looking along −x: `(y, z)`, the front face on the
//!   left.
//!
//! An extruded edge seen end on is a line across the view; it is visible
//! where nothing of the part lies between it and the eye, hidden
//! otherwise. A hole is a circle where it is seen end on, and two hidden
//! lines, its sides, where it is seen across, with its centre line.
//!
//! **A section** cuts the prism on the plane halfway back, or the turned
//! part on its axis: the cut material hatched, the holes it passes
//! through left open; it replaces the front view.

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::{Orientation, orientation};

/// An axis of the part: `x` across the front, `y` back from it, `z` up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    X,
    Y,
    Z,
}

/// A hole drilled right through a prism along `axis`, its centre `at` on
/// the plane square to it: `(x, z)` along y, `(x, y)` along z, `(y, z)`
/// along x.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hole {
    pub axis: Axis,
    pub at: [f64; 2],
    pub diameter: f64,
}

/// A part, described.
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Prism {
        /// The front face's outline, `(x, z)`, closed.
        profile: Vec<Point2>,
        depth: f64,
        holes: Vec<Hole>,
    },
    Turned {
        /// Each step's diameter and length, from x = 0.
        steps: Vec<(f64, f64)>,
        /// The bore's diameter; 0 for none.
        bore: f64,
    },
}

/// Why a part cannot be drawn.
#[derive(Clone, Debug, PartialEq)]
pub enum PartError {
    /// A profile of fewer than three corners, or with no area.
    Profile,
    /// A size that is not one: a depth, a diameter, a length.
    NotPositive(&'static str),
    /// A profile whose edges cross: not the outline of a part.
    ProfileCrosses,
    /// A hole whose centre is not inside the part.
    HoleOutside(usize),
    /// A hole that breaks out through a side: its edge passes outside the
    /// part somewhere along it.
    HoleBreaksOut(usize),
    /// A hole that enters or leaves the part through a face not square to
    /// it, or across a step: where it meets such a face its rim is an
    /// ellipse or a broken curve, not a circle (ADR-0040 §4).
    HoleOblique(usize),
    /// Two holes that meet: where they do, the edges are curves of
    /// intersection the views do not draw.
    HolesMeet(usize, usize),
    /// A bore as wide as a step, or wider.
    Bore,
}

/// A mark on a view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mark {
    Line(Point2, Point2),
    Circle(Point2, f64),
}

/// A view: what is seen, what is hidden, the centre lines, and, for a
/// section, the regions cut — each a set of loops filled by the even-odd
/// rule.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct View {
    pub visible: Vec<Mark>,
    pub hidden: Vec<Mark>,
    pub centre: Vec<(Point2, Point2)>,
    pub cut: Vec<Vec<Vec<Point2>>>,
    /// The view's extent: its lower left and upper right corners.
    pub min: Point2,
    pub max: Point2,
}

/// A part's views: the front — a section when asked — the top, unless the
/// part is turned, where it would repeat the front, and the right side.
#[derive(Clone, Debug, PartialEq)]
pub struct Views {
    pub front: View,
    pub top: Option<View>,
    pub right: View,
    /// For a sectioned prism, where the cutting plane runs in the top
    /// view: `y`.
    pub cutting_plane: Option<f64>,
}

fn p(a: f64, b: f64) -> Point2 {
    Point2::new(a, b)
}

/// How far a centre line runs past what it marks.
fn overrun(size: f64) -> f64 {
    (size * 0.1).max(1.0)
}

/// The intervals where a closed polygon lies on a line: crossings of its
/// edges, sorted and paired. `across` picks the coordinate the line holds
/// at `at`, `along` the one it runs in.
fn intervals(
    polygon: &[Point2],
    at: f64,
    across: impl Fn(Point2) -> f64,
    along: impl Fn(Point2) -> f64,
) -> Vec<(f64, f64)> {
    let n = polygon.len();
    let mut hits = Vec::new();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        let (ca, cb) = (across(a), across(b));
        // Half-open, so a vertex on the line counts once.
        if (ca <= at && cb > at) || (cb <= at && ca > at) {
            let t = (at - ca) / (cb - ca);
            hits.push(along(a) + (along(b) - along(a)) * t);
        }
    }
    hits.sort_by(f64::total_cmp);
    hits.as_chunks::<2>()
        .0
        .iter()
        .map(|&[a, b]| (a, b))
        .collect()
}

/// The polygon's z-intervals on the vertical line x.
fn at_x(polygon: &[Point2], x: f64) -> Vec<(f64, f64)> {
    intervals(polygon, x, |q| q.x, |q| q.y)
}

/// The polygon's x-intervals on the horizontal line z.
fn at_z(polygon: &[Point2], z: f64) -> Vec<(f64, f64)> {
    intervals(polygon, z, |q| q.y, |q| q.x)
}

fn bounds(points: impl IntoIterator<Item = Point2>) -> (Point2, Point2) {
    points.into_iter().fold(
        (
            p(f64::INFINITY, f64::INFINITY),
            p(f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(lo, hi), q| {
            (
                p(lo.x.min(q.x), lo.y.min(q.y)),
                p(hi.x.max(q.x), hi.y.max(q.y)),
            )
        },
    )
}

fn area(polygon: &[Point2]) -> f64 {
    let n = polygon.len();
    (0..n)
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// A polygon cut by the half-plane `keep`, by Sutherland and Hodgman: one
/// ring, which may run along the cut where a concave polygon leaves it
/// and comes back — an edge with nothing on either side, harmless to a
/// hatch.
fn clip(polygon: &[Point2], keep: impl Fn(Point2) -> f64) -> Vec<Point2> {
    let n = polygon.len();
    let mut out = Vec::new();
    for i in 0..n {
        let (a, b) = (polygon[i], polygon[(i + 1) % n]);
        let (da, db) = (keep(a), keep(b));
        if da >= 0.0 {
            out.push(a);
        }
        if (da >= 0.0) != (db >= 0.0) {
            let t = da / (da - db);
            out.push(p(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    out
}

/// The views of `part`; with `section`, the front cut through.
pub fn views(part: &Part, section: bool) -> Result<Views, PartError> {
    match part {
        Part::Prism {
            profile,
            depth,
            holes,
        } => prism(profile, *depth, holes, section),
        Part::Turned { steps, bore } => turned(steps, *bore, section),
    }
}

/// What a prism's views can be drawn for, in closed form (ADR-0040 §4):
/// a simple profile; each hole wholly inside the part, entering and
/// leaving through faces square to it — so every rim is a circle — and
/// meeting no other hole. Anything else is refused, never drawn wrong.
fn check_prism(
    profile: &[Point2],
    depth: f64,
    holes: &[Hole],
    lo: Point2,
    hi: Point2,
    tol: f64,
) -> Result<(), PartError> {
    let n = profile.len();
    let edge = |i: usize| (profile[i], profile[(i + 1) % n]);
    // No two edges that do not share a corner cross or touch.
    for i in 0..n {
        for j in i + 1..n {
            if j == i + 1 || (i == 0 && j == n - 1) {
                continue;
            }
            let ((a, b), (c, d)) = (edge(i), edge(j));
            let side = |p: Point2, q: Point2, r: Point2| orientation(p, q, r);
            if side(a, b, c) != side(a, b, d) && side(c, d, a) != side(c, d, b) {
                return Err(PartError::ProfileCrosses);
            }
            let collinear = Orientation::Collinear;
            if [side(a, b, c), side(a, b, d), side(c, d, a), side(c, d, b)].contains(&collinear) {
                let on = |p: Point2, q: Point2, r: Point2| {
                    side(p, q, r) == collinear
                        && r.x >= p.x.min(q.x)
                        && r.x <= p.x.max(q.x)
                        && r.y >= p.y.min(q.y)
                        && r.y <= p.y.max(q.y)
                };
                if on(a, b, c) || on(a, b, d) || on(c, d, a) || on(c, d, b) {
                    return Err(PartError::ProfileCrosses);
                }
            }
        }
    }
    // The distance from a point to an edge.
    let distance = |p: Point2, (a, b): (Point2, Point2)| {
        let ab = b - a;
        let t = ((p - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0);
        p.distance_to(a + ab * t)
    };
    // A band of the profile — x or z between two values — crossed only by
    // edges square to the hole: horizontal ones for a hole down z,
    // vertical ones for a hole along x; and no corner inside it, so no
    // step.
    let square_band = |from: f64, to: f64, across_x: bool| {
        let coord = |q: Point2| if across_x { q.x } else { q.y };
        let other = |q: Point2| if across_x { q.y } else { q.x };
        profile
            .iter()
            .all(|q| coord(*q) < from - tol || coord(*q) > to + tol)
            && (0..n).all(|i| {
                let (a, b) = edge(i);
                let crosses = coord(a).min(coord(b)) < to && coord(a).max(coord(b)) > from;
                !crosses || (other(a) - other(b)).abs() <= tol
            })
    };
    for (i, hole) in holes.iter().enumerate() {
        if !(hole.diameter.is_finite() && hole.diameter > 0.0) {
            return Err(PartError::NotPositive("hole diameter"));
        }
        let r = hole.diameter / 2.0;
        let [a, b] = hole.at;
        let inside = match hole.axis {
            Axis::Y => at_x(profile, a).iter().any(|&(z0, z1)| b > z0 && b < z1),
            Axis::Z => a > lo.x && a < hi.x && b > 0.0 && b < depth && !at_x(profile, a).is_empty(),
            Axis::X => a > 0.0 && a < depth && b > lo.y && b < hi.y && !at_z(profile, b).is_empty(),
        };
        if !inside {
            return Err(PartError::HoleOutside(i));
        }
        let within = match hole.axis {
            Axis::Y => (0..n).all(|k| distance(p(a, b), edge(k)) > r + tol),
            Axis::Z => b - r > tol && b + r < depth - tol,
            Axis::X => a - r > tol && a + r < depth - tol,
        };
        if !within {
            return Err(PartError::HoleBreaksOut(i));
        }
        let square = match hole.axis {
            Axis::Y => true,
            Axis::Z => square_band(a - r, a + r, true),
            Axis::X => square_band(b - r, b + r, false),
        };
        if !square {
            return Err(PartError::HoleOblique(i));
        }
    }
    // Two holes meet where their axes come closer than their radii
    // together.
    for (i, h) in holes.iter().enumerate() {
        for (j, k) in holes.iter().enumerate().skip(i + 1) {
            let reach = (h.diameter + k.diameter) / 2.0;
            // Each hole's place as (x, y, z), its own axis's coordinate free.
            let place = |hole: &Hole| match hole.axis {
                Axis::Y => [Some(hole.at[0]), None, Some(hole.at[1])],
                Axis::Z => [Some(hole.at[0]), Some(hole.at[1]), None],
                Axis::X => [None, Some(hole.at[0]), Some(hole.at[1])],
            };
            let (u, v) = (place(h), place(k));
            let apart: f64 = (0..3)
                .filter_map(|c| Some((u[c]? - v[c]?).powi(2)))
                .sum::<f64>()
                .sqrt();
            if apart < reach + tol {
                return Err(PartError::HolesMeet(i, j));
            }
        }
    }
    Ok(())
}

fn prism(
    profile: &[Point2],
    depth: f64,
    holes: &[Hole],
    section: bool,
) -> Result<Views, PartError> {
    if profile.len() < 3 || area(profile).abs() <= f64::EPSILON {
        return Err(PartError::Profile);
    }
    if !(depth.is_finite() && depth > 0.0) {
        return Err(PartError::NotPositive("depth"));
    }
    let (lo, hi) = bounds(profile.iter().copied());
    let size = (hi.x - lo.x).max(hi.y - lo.y).max(depth);
    let tol = LinearTolerance::DEFAULT.at_magnitude(size).value();
    check_prism(profile, depth, holes, lo, hi, tol)?;
    let o = overrun(size);
    let mut front = View {
        min: lo,
        max: hi,
        ..View::default()
    };
    let mut top = View {
        min: p(lo.x, 0.0),
        max: p(hi.x, depth),
        ..View::default()
    };
    let mut right = View {
        min: p(0.0, lo.y),
        max: p(depth, hi.y),
        ..View::default()
    };

    // The front: the profile, seen square on.
    let n = profile.len();
    let edges: Vec<(Point2, Point2)> = (0..n).map(|i| (profile[i], profile[(i + 1) % n])).collect();
    // The top and the side: their outlines, and each corner of the profile
    // as a line across, visible where nothing lies above it — beside it,
    // for the side view.
    let rect = |a: Point2, b: Point2| -> Vec<Mark> {
        vec![
            Mark::Line(p(a.x, a.y), p(b.x, a.y)),
            Mark::Line(p(b.x, a.y), p(b.x, b.y)),
            Mark::Line(p(b.x, b.y), p(a.x, b.y)),
            Mark::Line(p(a.x, b.y), p(a.x, a.y)),
        ]
    };
    top.visible.extend(rect(top.min, top.max));
    right.visible.extend(rect(right.min, right.max));
    let highest = |x: f64| {
        [x - tol * 10.0, x + tol * 10.0]
            .iter()
            .flat_map(|&x| at_x(profile, x))
            .map(|(_, b)| b)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let furthest = |z: f64| {
        [z - tol * 10.0, z + tol * 10.0]
            .iter()
            .flat_map(|&z| at_z(profile, z))
            .map(|(_, b)| b)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut xs: Vec<(f64, bool)> = Vec::new();
    let mut zs: Vec<(f64, bool)> = Vec::new();
    for q in profile {
        let seen_from_above = q.y >= highest(q.x) - tol * 100.0;
        let seen_from_right = q.x >= furthest(q.y) - tol * 100.0;
        for (list, value, seen, edge_lo, edge_hi) in [
            (&mut xs, q.x, seen_from_above, lo.x, hi.x),
            (&mut zs, q.y, seen_from_right, lo.y, hi.y),
        ] {
            if (value - edge_lo).abs() <= tol || (value - edge_hi).abs() <= tol {
                continue;
            }
            match list.iter_mut().find(|(v, _)| (v - value).abs() <= tol) {
                Some(entry) => entry.1 |= seen,
                None => list.push((value, seen)),
            }
        }
    }
    for (x, seen) in xs {
        let line = Mark::Line(p(x, 0.0), p(x, depth));
        if seen {
            top.visible.push(line)
        } else {
            top.hidden.push(line)
        }
    }
    for (z, seen) in zs {
        let line = Mark::Line(p(0.0, z), p(depth, z));
        if seen {
            right.visible.push(line)
        } else {
            right.hidden.push(line)
        }
    }

    // The holes.
    for hole in holes {
        let r = hole.diameter / 2.0;
        let [a, b] = hole.at;
        match hole.axis {
            Axis::Y => {
                front.visible.push(Mark::Circle(p(a, b), r));
                front.centre.push((p(a - r - o, b), p(a + r + o, b)));
                front.centre.push((p(a, b - r - o), p(a, b + r + o)));
                for x in [a - r, a + r] {
                    top.hidden.push(Mark::Line(p(x, 0.0), p(x, depth)));
                }
                top.centre.push((p(a, -o), p(a, depth + o)));
                for z in [b - r, b + r] {
                    right.hidden.push(Mark::Line(p(0.0, z), p(depth, z)));
                }
                right.centre.push((p(-o, b), p(depth + o, b)));
            }
            Axis::Z => {
                top.visible.push(Mark::Circle(p(a, b), r));
                top.centre.push((p(a - r - o, b), p(a + r + o, b)));
                top.centre.push((p(a, b - r - o), p(a, b + r + o)));
                // Through the material at its sides.
                for x in [a - r, a + r] {
                    for (z0, z1) in at_x(profile, x) {
                        front.hidden.push(Mark::Line(p(x, z0), p(x, z1)));
                    }
                }
                let (z0, z1) = at_x(profile, a)
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &(s, e)| {
                        (l.min(s), h.max(e))
                    });
                front.centre.push((p(a, z0 - o), p(a, z1 + o)));
                for (z0, z1) in at_x(profile, a) {
                    for y in [b - r, b + r] {
                        right.hidden.push(Mark::Line(p(y, z0), p(y, z1)));
                    }
                }
                right.centre.push((p(b, z0 - o), p(b, z1 + o)));
            }
            Axis::X => {
                right.visible.push(Mark::Circle(p(a, b), r));
                right.centre.push((p(a - r - o, b), p(a + r + o, b)));
                right.centre.push((p(a, b - r - o), p(a, b + r + o)));
                for z in [b - r, b + r] {
                    for (x0, x1) in at_z(profile, z) {
                        front.hidden.push(Mark::Line(p(x0, z), p(x1, z)));
                    }
                }
                let (x0, x1) = at_z(profile, b)
                    .iter()
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &(s, e)| {
                        (l.min(s), h.max(e))
                    });
                front.centre.push((p(x0 - o, b), p(x1 + o, b)));
                for (x0, x1) in at_z(profile, b) {
                    for y in [a - r, a + r] {
                        top.hidden.push(Mark::Line(p(x0, y), p(x1, y)));
                    }
                }
                top.centre.push((p(x0 - o, a), p(x1 + o, a)));
            }
        }
    }

    let cutting_plane = section.then_some(depth / 2.0);
    if let Some(plane) = cutting_plane {
        // The material on the plane: the profile less the bands the holes
        // across it take, the holes along y left open as circles.
        let mut pieces = vec![profile.to_vec()];
        let mut sides: Vec<Mark> = Vec::new();
        for hole in holes {
            let r = hole.diameter / 2.0;
            let [a, b] = hole.at;
            let (centre, offset) = match hole.axis {
                Axis::Z => (a, b - plane),
                Axis::X => (b, a - plane),
                Axis::Y => continue,
            };
            if offset.abs() >= r {
                continue;
            }
            let half = (r * r - offset * offset).sqrt();
            let (low, high) = (centre - half, centre + half);
            let across_z = hole.axis == Axis::X;
            let coord = move |q: Point2| if across_z { q.y } else { q.x };
            pieces = pieces
                .into_iter()
                .flat_map(|piece| {
                    [
                        clip(&piece, |q| low - coord(q)),
                        clip(&piece, |q| coord(q) - high),
                    ]
                })
                .filter(|piece| piece.len() >= 3 && area(piece).abs() > tol)
                .collect();
            // The hole's walls, where there is material.
            for v in [low, high] {
                if across_z {
                    for (x0, x1) in at_z(profile, v) {
                        sides.push(Mark::Line(p(x0, v), p(x1, v)));
                    }
                } else {
                    for (z0, z1) in at_x(profile, v) {
                        sides.push(Mark::Line(p(v, z0), p(v, z1)));
                    }
                }
            }
        }
        let circles: Vec<(Point2, f64)> = holes
            .iter()
            .filter(|h| h.axis == Axis::Y)
            .map(|h| (p(h.at[0], h.at[1]), h.diameter / 2.0))
            .collect();
        front.cut = pieces
            .into_iter()
            .map(|piece| {
                let mut loops = vec![piece.clone()];
                for &(c, r) in &circles {
                    if at_x(&piece, c.x)
                        .iter()
                        .any(|&(z0, z1)| c.y > z0 && c.y < z1)
                    {
                        loops.push(
                            (0..48)
                                .map(|k| {
                                    let t = std::f64::consts::TAU * f64::from(k) / 48.0;
                                    p(c.x + r * t.cos(), c.y + r * t.sin())
                                })
                                .collect(),
                        );
                    }
                }
                loops
            })
            .collect();
        // A section shows what is cut and what lies beyond, no hidden lines.
        front.hidden.clear();
        front.visible.extend(sides);
    }
    for (a, b) in edges {
        front.visible.push(Mark::Line(a, b));
    }
    Ok(Views {
        front,
        top: Some(top),
        right,
        cutting_plane,
    })
}

fn turned(steps: &[(f64, f64)], bore: f64, section: bool) -> Result<Views, PartError> {
    if steps.is_empty() {
        return Err(PartError::Profile);
    }
    for &(d, l) in steps {
        if !(d.is_finite() && d > 0.0) {
            return Err(PartError::NotPositive("step diameter"));
        }
        if !(l.is_finite() && l > 0.0) {
            return Err(PartError::NotPositive("step length"));
        }
    }
    if !(bore.is_finite() && bore >= 0.0) || steps.iter().any(|&(d, _)| bore >= d) {
        return Err(PartError::Bore);
    }
    let rb = bore / 2.0;
    let length: f64 = steps.iter().map(|s| s.1).sum();
    let big = steps.iter().map(|s| s.0 / 2.0).fold(0.0, f64::max);
    let o = overrun(length.max(2.0 * big));
    // The stepped outline's upper half, left to right.
    let mut upper = Vec::new();
    let mut x = 0.0;
    for &(d, l) in steps {
        upper.push(p(x, d / 2.0));
        x += l;
        upper.push(p(x, d / 2.0));
    }
    let mut front = View {
        min: p(0.0, -big),
        max: p(length, big),
        ..View::default()
    };
    // The outline: the upper half, then the lower mirrored back.
    let mut ring = upper.clone();
    ring.extend(upper.iter().rev().map(|q| p(q.x, -q.y)));
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        if a != b {
            front.visible.push(Mark::Line(a, b));
        }
    }
    // Each shoulder between steps is a line across at the larger
    // diameter: the outline runs from the smaller to the larger already,
    // so the line across the smaller completes it.
    let mut x = 0.0;
    for pair in steps.windows(2) {
        x += pair[0].1;
        let r_small = pair[0].0.min(pair[1].0) / 2.0;
        if section && rb > 0.0 {
            // Cut on the axis, the bore is open between the halves.
            front.visible.push(Mark::Line(p(x, rb), p(x, r_small)));
            front.visible.push(Mark::Line(p(x, -r_small), p(x, -rb)));
        } else {
            front
                .visible
                .push(Mark::Line(p(x, -r_small), p(x, r_small)));
        }
    }
    front.centre.push((p(-o, 0.0), p(length + o, 0.0)));
    if rb > 0.0 {
        for z in [-rb, rb] {
            front.hidden.push(Mark::Line(p(0.0, z), p(length, z)));
        }
    }
    // The right side, looking back along the axis: each step's circle,
    // seen when no step nearer the eye is as large.
    let mut right = View {
        min: p(-big, -big),
        max: p(big, big),
        ..View::default()
    };
    let mut drawn: Vec<f64> = Vec::new();
    for (i, &(d, _)) in steps.iter().enumerate() {
        let r = d / 2.0;
        if drawn.iter().any(|&q| (q - r).abs() < 1e-9) {
            continue;
        }
        drawn.push(r);
        let nearer = steps[i + 1..].iter().map(|s| s.0 / 2.0).fold(0.0, f64::max);
        let circle = Mark::Circle(p(0.0, 0.0), r);
        if r > nearer {
            right.visible.push(circle);
        } else {
            right.hidden.push(circle);
        }
    }
    if rb > 0.0 {
        right.visible.push(Mark::Circle(p(0.0, 0.0), rb));
    }
    right.centre.push((p(-big - o, 0.0), p(big + o, 0.0)));
    right.centre.push((p(0.0, -big - o), p(0.0, big + o)));
    if section {
        // Cut on the axis: the material above and below the bore.
        let mut half = upper.clone();
        half.push(p(length, rb));
        half.push(p(0.0, rb));
        let lower: Vec<Point2> = half.iter().map(|q| p(q.x, -q.y)).collect();
        front.cut = vec![vec![half], vec![lower]];
        front.hidden.clear();
        if rb > 0.0 {
            for z in [-rb, rb] {
                front.visible.push(Mark::Line(p(0.0, z), p(length, z)));
            }
        }
    }
    Ok(Views {
        front,
        top: None,
        right,
        cutting_plane: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    fn lines(marks: &[Mark]) -> Vec<(Point2, Point2)> {
        marks
            .iter()
            .filter_map(|m| match m {
                Mark::Line(a, b) => Some((*a, *b)),
                Mark::Circle(..) => None,
            })
            .collect()
    }

    fn circles(marks: &[Mark]) -> Vec<(Point2, f64)> {
        marks
            .iter()
            .filter_map(|m| match m {
                Mark::Circle(c, r) => Some((*c, *r)),
                Mark::Line(..) => None,
            })
            .collect()
    }

    fn block(holes: Vec<Hole>) -> Part {
        Part::Prism {
            profile: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 50.0), p(0.0, 50.0)],
            depth: 40.0,
            holes,
        }
    }

    /// An L bracket: a base 100 long and 20 thick, an upright 30 wide and
    /// 60 high at its left end.
    fn bracket(holes: Vec<Hole>) -> Part {
        Part::Prism {
            profile: vec![
                p(0.0, 0.0),
                p(100.0, 0.0),
                p(100.0, 20.0),
                p(30.0, 20.0),
                p(30.0, 60.0),
                p(0.0, 60.0),
            ],
            depth: 40.0,
            holes,
        }
    }

    #[test]
    fn a_hole_is_at_the_same_place_in_every_view() {
        let hole = Hole {
            axis: Axis::Y,
            at: [60.0, 25.0],
            diameter: 10.0,
        };
        let v = views(&block(vec![hole]), false).unwrap();
        // A circle in front, at its centre.
        assert_eq!(circles(&v.front.visible), [(p(60.0, 25.0), 5.0)]);
        // Its sides hidden in the top view at the same x, through the depth.
        let top = v.top.unwrap();
        let sides: Vec<f64> = lines(&top.hidden).iter().map(|(a, _)| a.x).collect();
        assert_eq!(sides, [55.0, 65.0]);
        assert!(
            top.centre
                .iter()
                .any(|(a, b)| close(a.x, 60.0) && close(b.x, 60.0))
        );
        // And in the side view at the same height.
        let heights: Vec<f64> = lines(&v.right.hidden).iter().map(|(a, _)| a.y).collect();
        assert_eq!(heights, [20.0, 30.0]);
        assert!(
            v.right
                .centre
                .iter()
                .any(|(a, b)| close(a.y, 25.0) && close(b.y, 25.0))
        );
    }

    #[test]
    fn a_step_is_seen_from_above_and_from_the_side_where_nothing_covers_it() {
        let v = views(&bracket(vec![]), false).unwrap();
        let top = v.top.unwrap();
        // The upright's edge at x 30 is seen from above: the base steps
        // down past it.
        assert!(
            lines(&top.visible)
                .iter()
                .any(|(a, b)| close(a.x, 30.0) && close(b.x, 30.0))
        );
        assert!(top.hidden.is_empty());
        // From the right, the base's top at z 20 is seen: the base reaches
        // furthest there.
        assert!(
            lines(&v.right.visible)
                .iter()
                .any(|(a, b)| close(a.y, 20.0) && close(b.y, 20.0))
        );
        assert!(v.right.hidden.is_empty());
    }

    #[test]
    fn a_notch_from_below_is_hidden_from_above_and_holes_down_are_hidden_in_front() {
        // A block with a slot 20 wide and 15 deep cut up from its bottom.
        let slotted = Part::Prism {
            profile: vec![
                p(0.0, 0.0),
                p(40.0, 0.0),
                p(40.0, 15.0),
                p(60.0, 15.0),
                p(60.0, 0.0),
                p(100.0, 0.0),
                p(100.0, 50.0),
                p(0.0, 50.0),
            ],
            depth: 40.0,
            holes: vec![Hole {
                axis: Axis::Z,
                at: [80.0, 20.0],
                diameter: 8.0,
            }],
        };
        let v = views(&slotted, false).unwrap();
        let top = v.top.unwrap();
        let hidden: Vec<f64> = lines(&top.hidden).iter().map(|(a, _)| a.x).collect();
        assert!(
            hidden.contains(&40.0) && hidden.contains(&60.0),
            "{hidden:?}"
        );
        // The vertical hole: a circle from above, its sides hidden in
        // front from the bottom to the top at x 76 and 84.
        assert_eq!(circles(&top.visible), [(p(80.0, 20.0), 4.0)]);
        let sides = lines(&v.front.hidden);
        assert!(
            sides
                .iter()
                .any(|(a, b)| close(a.x, 76.0) && close(a.y, 0.0) && close(b.y, 50.0))
        );
        assert!(sides.iter().any(|(a, _)| close(a.x, 84.0)));
        // The side view sees it across, at y 16 and 24.
        let across: Vec<f64> = lines(&v.right.hidden).iter().map(|(a, _)| a.x).collect();
        assert!(
            across.contains(&16.0) && across.contains(&24.0),
            "{across:?}"
        );
    }

    #[test]
    fn a_section_hatches_only_the_material_the_plane_cuts() {
        let holes = vec![
            Hole {
                axis: Axis::Y,
                at: [70.0, 10.0],
                diameter: 8.0,
            },
            Hole {
                axis: Axis::Z,
                at: [15.0, 20.0],
                diameter: 10.0,
            },
        ];
        let v = views(&bracket(holes), true).unwrap();
        assert_eq!(v.cutting_plane, Some(20.0));
        // Inside a cut region by the even-odd rule.
        let filled = |q: Point2| {
            v.front.cut.iter().any(|loops| {
                loops
                    .iter()
                    .filter(|ring| at_x(ring, q.x).iter().any(|&(a, b)| q.y > a && q.y < b))
                    .count()
                    % 2
                    == 1
            })
        };
        assert!(filled(p(50.0, 5.0)), "the base");
        assert!(filled(p(5.0, 40.0)), "the upright, left of the hole");
        assert!(!filled(p(15.0, 40.0)), "the vertical hole, on the plane");
        assert!(!filled(p(70.0, 10.0)), "the hole along y");
        assert!(!filled(p(50.0, 40.0)), "air above the base");
        // No hidden lines in a section; the vertical hole's walls drawn.
        assert!(v.front.hidden.is_empty());
        assert!(
            lines(&v.front.visible)
                .iter()
                .any(|(a, b)| close(a.x, 10.0) && close(b.x, 10.0))
        );
    }

    #[test]
    fn a_turned_part_shows_its_steps_its_shoulders_and_its_bore() {
        let shaft = Part::Turned {
            steps: vec![(40.0, 30.0), (60.0, 20.0), (30.0, 50.0)],
            bore: 10.0,
        };
        let v = views(&shaft, false).unwrap();
        assert!(v.top.is_none());
        assert!(close(v.front.max.x, 100.0) && close(v.front.max.y, 30.0));
        // The shoulders at x 30 and 50.
        let across: Vec<f64> = lines(&v.front.visible)
            .iter()
            .filter(|(a, b)| close(a.x, b.x))
            .map(|(a, _)| a.x)
            .collect();
        assert!(
            across.contains(&30.0) && across.contains(&50.0),
            "{across:?}"
        );
        // The bore hidden in front, open at the end.
        assert_eq!(lines(&v.front.hidden).len(), 2);
        // From the right: the end (r 15) and the collar (r 30) seen; the
        // first step (r 20) behind the collar, hidden; the bore seen.
        let seen: Vec<f64> = circles(&v.right.visible).iter().map(|c| c.1).collect();
        let hidden: Vec<f64> = circles(&v.right.hidden).iter().map(|c| c.1).collect();
        assert!(
            seen.contains(&30.0) && seen.contains(&15.0) && seen.contains(&5.0),
            "{seen:?}"
        );
        assert_eq!(hidden, [20.0]);
        // In section: the material above and below the bore.
        let cut = views(&shaft, true).unwrap();
        assert_eq!(cut.front.cut.len(), 2);
        assert!(cut.front.hidden.is_empty());
        // Nothing crosses the open bore.
        assert!(
            lines(&cut.front.visible)
                .iter()
                .all(|(a, b)| !(a.y.min(b.y) < 5.0 - 1e-9
                    && a.y.max(b.y) > -5.0 + 1e-9
                    && close(a.x, b.x)
                    && a.x > 0.0
                    && a.x < 100.0))
        );
    }

    #[test]
    fn what_cannot_be_a_part_is_refused() {
        assert_eq!(
            views(
                &Part::Prism {
                    profile: vec![p(0.0, 0.0), p(1.0, 0.0)],
                    depth: 1.0,
                    holes: vec![]
                },
                false
            ),
            Err(PartError::Profile)
        );
        assert_eq!(
            views(&block(vec![]).with_depth(0.0), false),
            Err(PartError::NotPositive("depth"))
        );
        let outside = Hole {
            axis: Axis::Y,
            at: [150.0, 10.0],
            diameter: 5.0,
        };
        assert_eq!(
            views(&block(vec![outside]), false),
            Err(PartError::HoleOutside(0))
        );
        assert_eq!(
            views(
                &Part::Turned {
                    steps: vec![(10.0, 5.0)],
                    bore: 10.0
                },
                false
            ),
            Err(PartError::Bore)
        );
    }

    /// ADR-0040 §4: what the closed forms do not cover is refused, each
    /// case at its edge.
    #[test]
    fn what_the_views_cannot_draw_rightly_is_refused() {
        let hole = |axis, at: [f64; 2], diameter| Hole { axis, at, diameter };
        // A bow-tie profile.
        let crossed = Part::Prism {
            profile: vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 0.0), p(0.0, 4.0)],
            depth: 5.0,
            holes: vec![],
        };
        assert_eq!(views(&crossed, false), Err(PartError::ProfileCrosses));
        // A hole along y whose edge reaches past the block's top (z 50):
        // at z 45.1 with radius 5 it does; at 44 it is clear.
        assert_eq!(
            views(&block(vec![hole(Axis::Y, [50.0, 45.1], 10.0)]), false),
            Err(PartError::HoleBreaksOut(0))
        );
        assert!(views(&block(vec![hole(Axis::Y, [50.0, 44.0], 10.0)]), false).is_ok());
        // Down z, 6 from the back face of a block 40 deep: radius 6 breaks
        // out, 5.9 does not.
        assert_eq!(
            views(&block(vec![hole(Axis::Z, [50.0, 34.0], 12.0)]), false),
            Err(PartError::HoleBreaksOut(0))
        );
        assert!(views(&block(vec![hole(Axis::Z, [50.0, 34.0], 11.8)]), false).is_ok());
        // Down z across the bracket's step at x 30: it leaves through two
        // levels.
        assert_eq!(
            views(&bracket(vec![hole(Axis::Z, [32.0, 20.0], 8.0)]), false),
            Err(PartError::HoleOblique(0))
        );
        // Down z through a sloped top: its rim would be an ellipse.
        let ramp = Part::Prism {
            profile: vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 20.0), p(0.0, 50.0)],
            depth: 40.0,
            holes: vec![hole(Axis::Z, [50.0, 20.0], 10.0)],
        };
        assert_eq!(views(&ramp, false), Err(PartError::HoleOblique(0)));
        // Two holes that meet: along y at z 40 and along x at z 45, radii
        // 4 and 6; and two side by side along y, overlapping.
        assert_eq!(
            views(
                &bracket(vec![
                    hole(Axis::Y, [15.0, 40.0], 8.0),
                    hole(Axis::X, [20.0, 45.0], 12.0)
                ]),
                false
            ),
            Err(PartError::HolesMeet(0, 1))
        );
        assert_eq!(
            views(
                &block(vec![
                    hole(Axis::Y, [30.0, 25.0], 10.0),
                    hole(Axis::Y, [38.0, 25.0], 8.0)
                ]),
                false
            ),
            Err(PartError::HolesMeet(0, 1))
        );
        // Far enough apart, they are drawn.
        assert!(
            views(
                &block(vec![
                    hole(Axis::Y, [30.0, 25.0], 10.0),
                    hole(Axis::Y, [45.0, 25.0], 8.0)
                ]),
                false
            )
            .is_ok()
        );
    }

    impl Part {
        fn with_depth(self, d: f64) -> Self {
            match self {
                Part::Prism { profile, holes, .. } => Part::Prism {
                    profile,
                    depth: d,
                    holes,
                },
                other => other,
            }
        }
    }
}
