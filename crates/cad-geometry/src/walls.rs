//! Walls with a thickness (#358): centrelines and how thick each is, made
//! into the faces a plan draws — one wall where walls meet, the openings in
//! them cut — and the doors and windows in those openings.
//!
//! A wall is one object, its faces derived from its centreline and its
//! thickness: a revision that thickens a partition replaces it, and never
//! leaves the old line between two new faces, as the house's plan did.
//!
//! - **Joints.** Each wall is first a member as wide as it is thick
//!   ([`crate::members`]): mitred where walls end together, butted where
//!   one ends on another's side, through where collinear walls of one
//!   thickness meet. The faces are then the outline of all of them
//!   together: where a partition meets a wall there is no line across the
//!   joint, as a plan shows walls of one construction.
//! - **Openings.** An opening is a stretch of a wall's centreline: the
//!   faces are cut across it, its jambs closing the wall at each end. A
//!   window's glass runs along it; a door's leaf stands open into the side
//!   it swings to, hinged at the end asked, its swing drawn to the other
//!   jamb ([`crate::opening`]).
//!
//! What is drawn are segments and arcs, in the walls' own units.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::Segment;
use crate::members::{self, Member, MembersError};
use crate::opening::Opening as Cut;

/// A wall: its centreline and how thick it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Wall {
    pub from: Point2,
    pub to: Point2,
    pub thickness: f64,
}

/// What fills an opening.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Door,
    Window,
    /// A gap with nothing in it: a passage, an arch.
    Opening,
}

/// An opening in a wall: `at` along its centreline from its `from` to the
/// opening's middle, `width` wide. A door swings to the `left` of the wall
/// looking from `from` to `to`, or to its right, hinged at the opening's
/// end nearer the wall's `from` when `hinge_at_start`, else at the other.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Opening {
    pub wall: usize,
    pub at: f64,
    pub width: f64,
    pub kind: Kind,
    pub left: bool,
    pub hinge_at_start: bool,
}

/// An arc: its centre, radius, and the angles it runs between,
/// counter-clockwise, radians.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Swing {
    pub center: Point2,
    pub radius: f64,
    pub start: f64,
    pub end: f64,
}

/// What walls are drawn as.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Walled {
    /// The faces and the jambs: where wall meets no wall.
    pub faces: Vec<Segment>,
    /// The doors' leaves, each a thin rectangle's four sides.
    pub leaves: Vec<Segment>,
    /// The doors' swings.
    pub swings: Vec<Swing>,
    /// The windows' glass.
    pub glass: Vec<Segment>,
}

/// Why walls cannot be drawn, each by the index of what is wrong.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallsError {
    /// A wall with no length.
    NoLength(usize),
    /// A wall whose thickness is not a size.
    BadThickness(usize),
    /// An opening that names no wall.
    NoWall(usize),
    /// An opening not wholly on its wall, or of no width.
    OffTheWall(usize),
    /// An opening overlapping another on its wall.
    Overlap(usize),
}

/// How far either side of an edge is looked at to tell whether it is a
/// face: this many tolerances off it — far enough to be on one side, near
/// enough that no other edge lies between.
const PROBE: f64 = 1000.0;

/// Whether `q` is inside the closed `ring`, by the crossings of a ray.
fn inside(ring: &[Point2], q: Point2) -> bool {
    let mut odd = false;
    for k in 0..ring.len() {
        let (a, b) = (ring[k], ring[(k + 1) % ring.len()]);
        if (a.y > q.y) != (b.y > q.y) {
            let x = a.x + (q.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x > q.x {
                odd = !odd;
            }
        }
    }
    odd
}

fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Where, along `s` from 0 to 1, the segments of `all` cut or touch it.
fn cuts(s: Segment, all: &[Segment], tolerance: f64) -> Vec<f64> {
    let d = s.end - s.start;
    let length = d.length();
    let mut at = vec![0.0, 1.0];
    for o in all {
        let e = o.end - o.start;
        let denominator = cross(d, e);
        if denominator.abs() > tolerance * length.max(e.length()) {
            // Crossing lines: where they meet, if on both.
            let w = o.start - s.start;
            let t = cross(w, e) / denominator;
            let u = cross(w, d) / denominator;
            let slack = tolerance / length;
            if (-slack..=1.0 + slack).contains(&t) && (-slack..=1.0 + slack).contains(&u) {
                at.push(t.clamp(0.0, 1.0));
            }
        } else {
            // Parallel: the other's ends where they lie on this one.
            for q in [o.start, o.end] {
                let w = q - s.start;
                if cross(d, w).abs() <= tolerance * length {
                    let t = w.dot(d) / (length * length);
                    if (0.0..=1.0).contains(&t) {
                        at.push(t);
                    }
                }
            }
        }
    }
    at.sort_by(f64::total_cmp);
    at.dedup_by(|a, b| (*a - *b).abs() * length <= tolerance);
    at
}

/// The pieces of `segments` where one side is in `solid` and the other
/// not, each once.
fn boundary(segments: &[Segment], solid: impl Fn(Point2) -> bool, tolerance: f64) -> Vec<Segment> {
    let probe = tolerance * PROBE;
    let mut out: Vec<Segment> = Vec::new();
    for s in segments {
        let at = cuts(*s, segments, tolerance);
        let d = s.end - s.start;
        let length = d.length();
        if length <= tolerance {
            continue;
        }
        let n = Vec2::new(-d.y / length, d.x / length);
        for pair in at.windows(2) {
            let a = s.start + d * pair[0];
            let b = s.start + d * pair[1];
            if (b - a).length() <= tolerance {
                continue;
            }
            let m = a + (b - a) * 0.5;
            if solid(m + n * probe) == solid(m - n * probe) {
                continue;
            }
            let same = |o: &Segment| {
                ((o.start - a).length() <= tolerance && (o.end - b).length() <= tolerance)
                    || ((o.start - b).length() <= tolerance && (o.end - a).length() <= tolerance)
            };
            if !out.iter().any(same) {
                out.push(Segment::new(a, b));
            }
        }
    }
    merged(out, tolerance)
}

/// `pieces` with those that continue each other in a line joined: one
/// segment for a face, not one per piece the other edges cut it into.
fn merged(mut pieces: Vec<Segment>, tolerance: f64) -> Vec<Segment> {
    let near = |a: Point2, b: Point2| (a - b).length() <= tolerance;
    loop {
        let mut joined = None;
        'search: for i in 0..pieces.len() {
            for j in i + 1..pieces.len() {
                let (p, q) = (pieces[i], pieces[j]);
                // The end they share, and the far end of each.
                let ends = [
                    (p.end, p.start, q.start, q.end),
                    (p.end, p.start, q.end, q.start),
                    (p.start, p.end, q.start, q.end),
                    (p.start, p.end, q.end, q.start),
                ];
                for (shared, a, other, b) in ends {
                    if !near(shared, other) {
                        continue;
                    }
                    let (u, v) = (shared - a, b - shared);
                    let lined = cross(u, v).abs() <= tolerance * u.length().max(v.length())
                        && u.dot(v) > 0.0;
                    // Joined only where nothing else meets that point.
                    let alone = pieces.iter().enumerate().all(|(k, o)| {
                        k == i || k == j || (!near(o.start, shared) && !near(o.end, shared))
                    });
                    if lined && alone {
                        joined = Some((i, j, Segment::new(a, b)));
                        break 'search;
                    }
                }
            }
        }
        let Some((i, j, s)) = joined else {
            return pieces;
        };
        pieces.remove(j);
        pieces[i] = s;
    }
}

/// The walls drawn, their openings cut and filled.
pub fn walls(walls: &[Wall], openings: &[Opening]) -> Result<Walled, WallsError> {
    let points: Vec<Point2> = walls.iter().flat_map(|w| [w.from, w.to]).collect();
    let tolerance = LinearTolerance::DEFAULT.around(&points);
    let members: Vec<Member> = walls
        .iter()
        .map(|w| Member {
            centre: Segment::new(w.from, w.to),
            width: w.thickness,
        })
        .collect();
    let rings = members::outlines(&members, tolerance).map_err(|error| match error {
        MembersError::NoLength(i) => WallsError::NoLength(i),
        MembersError::BadWidth(i) => WallsError::BadThickness(i),
    })?;
    // Each opening: its wall's frame, its stretch, the box cut out of it.
    let mut cut_out: Vec<[Point2; 4]> = Vec::new();
    let mut taken: Vec<(usize, f64, f64)> = Vec::new();
    let mut out = Walled::default();
    for (k, opening) in openings.iter().enumerate() {
        let wall = walls.get(opening.wall).ok_or(WallsError::NoWall(k))?;
        let d = wall.to - wall.from;
        let length = d.length();
        let u = d * (1.0 / length);
        let v = Vec2::new(-u.y, u.x);
        let (s0, s1) = (
            opening.at - opening.width / 2.0,
            opening.at + opening.width / 2.0,
        );
        if !(opening.width > tolerance.value()
            && s0 >= -tolerance.value()
            && s1 <= length + tolerance.value())
        {
            return Err(WallsError::OffTheWall(k));
        }
        if taken.iter().any(|(w, a, b)| {
            *w == opening.wall && s0 < *b - tolerance.value() && *a < s1 - tolerance.value()
        }) {
            return Err(WallsError::Overlap(k));
        }
        taken.push((opening.wall, s0, s1));
        let at = |s: f64, across: f64| wall.from + u * s + v * across;
        let t = wall.thickness;
        cut_out.push([at(s0, -t), at(s1, -t), at(s1, t), at(s0, t)]);
        // What fills it, from the wall's two faces.
        let first = Segment::new(at(0.0, -t / 2.0), at(length, -t / 2.0));
        let second = Segment::new(at(0.0, t / 2.0), at(length, t / 2.0));
        let cut = Cut::new(first, second, at(opening.at, 0.0), opening.width, tolerance)
            .map_err(|_| WallsError::OffTheWall(k))?;
        match opening.kind {
            Kind::Window => {
                out.glass
                    .extend(cut.glass().iter().map(|(a, b)| Segment::new(*a, *b)));
            }
            Kind::Door => {
                let side = if opening.left { 1.0 } else { -1.0 };
                let end = if opening.hinge_at_start { s0 } else { s1 };
                let into = at(end, side * opening.width);
                let leaf = cut.leaf(into, t / 4.0);
                let ring = leaf.outline;
                for i in 0..4 {
                    out.leaves.push(Segment::new(ring[i], ring[(i + 1) % 4]));
                }
                out.swings.push(Swing {
                    center: leaf.hinge,
                    radius: leaf.radius,
                    start: leaf.swing.0,
                    end: leaf.swing.1,
                });
            }
            Kind::Opening => {}
        }
    }
    let mut edges: Vec<Segment> = Vec::new();
    for ring in &rings {
        let r = &ring.ring;
        for i in 0..r.len() {
            edges.push(Segment::new(r[i], r[(i + 1) % r.len()]));
        }
    }
    for b in &cut_out {
        for i in 0..4 {
            edges.push(Segment::new(b[i], b[(i + 1) % 4]));
        }
    }
    let solid = |q: Point2| {
        rings.iter().any(|o| inside(&o.ring, q)) && !cut_out.iter().any(|b| inside(b, q))
    };
    out.faces = boundary(&edges, solid, tolerance.value());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn wall(a: (f64, f64), b: (f64, f64), t: f64) -> Wall {
        Wall {
            from: p(a.0, a.1),
            to: p(b.0, b.1),
            thickness: t,
        }
    }

    fn on(s: &Segment, y: f64) -> bool {
        (s.start.y - y).abs() < 1e-6 && (s.end.y - y).abs() < 1e-6
    }

    fn total(faces: &[Segment]) -> f64 {
        faces.iter().map(Segment::length).sum()
    }

    /// A box of four 200 mm walls: its outside and inside faces, one
    /// segment each, nothing across the corners.
    #[test]
    fn a_closed_box_of_walls_is_two_rings_of_faces() {
        let box_ = [
            wall((0.0, 0.0), (7000.0, 0.0), 200.0),
            wall((7000.0, 0.0), (7000.0, 15000.0), 200.0),
            wall((7000.0, 15000.0), (0.0, 15000.0), 200.0),
            wall((0.0, 15000.0), (0.0, 0.0), 200.0),
        ];
        let w = walls(&box_, &[]).unwrap();
        assert_eq!(w.faces.len(), 8, "{:?}", w.faces);
        // Outside 7200 × 15200, inside 6800 × 14800.
        let perimeter = 2.0 * (7200.0 + 15200.0) + 2.0 * (6800.0 + 14800.0);
        assert!(
            (total(&w.faces) - perimeter).abs() < 1e-6,
            "{}",
            total(&w.faces)
        );
    }

    /// A 150 mm partition meeting a 200 mm wall (#358): its two faces run
    /// to the wall's inner face, which is broken there, and no line crosses
    /// the joint — never the three lines of a centreline left between faces.
    #[test]
    fn a_partition_meets_a_wall_with_no_line_across_the_joint() {
        let w = walls(
            &[
                wall((0.0, 0.0), (7000.0, 0.0), 200.0),
                wall((3000.0, 0.0), (3000.0, 4000.0), 150.0),
            ],
            &[],
        )
        .unwrap();
        // The wall's inner face, y = 100, is broken by the partition's 150.
        let inner: Vec<&Segment> = w.faces.iter().filter(|s| on(s, 100.0)).collect();
        assert_eq!(inner.len(), 2, "{inner:?}");
        assert!((inner.iter().map(|s| s.length()).sum::<f64>() - (7000.0 - 150.0)).abs() < 1e-6);
        // The partition's faces at x = 2925 and 3075, from the wall's face.
        let faces: Vec<&Segment> = w
            .faces
            .iter()
            .filter(|s| (s.start.x - s.end.x).abs() < 1e-9 && (2900.0..3100.0).contains(&s.start.x))
            .collect();
        assert_eq!(faces.len(), 2, "{faces:?}");
        for f in faces {
            assert!((f.start.y.min(f.end.y) - 100.0).abs() < 1e-6, "{f:?}");
        }
        // Nothing along the partition's centreline.
        assert!(
            !w.faces
                .iter()
                .any(|s| (s.start.x - 3000.0).abs() < 1e-6 && (s.end.x - 3000.0).abs() < 1e-6)
        );
    }

    /// A door in a wall: both faces cut, a jamb at each end, the leaf
    /// standing into the side asked and its swing; a window's glass.
    #[test]
    fn openings_are_cut_with_jambs_and_filled() {
        let w = walls(
            &[wall((0.0, 0.0), (5000.0, 0.0), 200.0)],
            &[
                Opening {
                    wall: 0,
                    at: 1000.0,
                    width: 900.0,
                    kind: Kind::Door,
                    left: true,
                    hinge_at_start: true,
                },
                Opening {
                    wall: 0,
                    at: 3500.0,
                    width: 1500.0,
                    kind: Kind::Window,
                    left: true,
                    hinge_at_start: true,
                },
            ],
        )
        .unwrap();
        // Each face in three stretches: before, between, after.
        for y in [-100.0, 100.0] {
            let face: f64 = w
                .faces
                .iter()
                .filter(|s| on(s, y))
                .map(Segment::length)
                .sum();
            assert!(
                (face - (5000.0 - 900.0 - 1500.0)).abs() < 1e-6,
                "{y}: {face}"
            );
        }
        // Four jambs across the wall, and the two ends.
        let across = w
            .faces
            .iter()
            .filter(|s| (s.start.x - s.end.x).abs() < 1e-9)
            .count();
        assert_eq!(across, 6);
        // The door's leaf stands up, into +y, from its hinge at x = 550.
        assert_eq!(w.leaves.len(), 4);
        assert_eq!(w.swings.len(), 1);
        assert!((w.swings[0].radius - 900.0).abs() < 1e-6);
        assert!((w.swings[0].center.x - 550.0).abs() < 1e-6 && w.swings[0].center.y > 0.0);
        assert!(w.leaves.iter().all(|s| s.start.y >= 100.0 - 1e-6));
        assert_eq!(w.glass.len(), 2);
    }

    #[test]
    fn what_cannot_be_walls_is_refused_by_its_index() {
        let one = [wall((0.0, 0.0), (1000.0, 0.0), 200.0)];
        let door = |wall, at, width| Opening {
            wall,
            at,
            width,
            kind: Kind::Door,
            left: true,
            hinge_at_start: true,
        };
        assert_eq!(
            walls(&[wall((0.0, 0.0), (0.0, 0.0), 200.0)], &[]),
            Err(WallsError::NoLength(0))
        );
        assert_eq!(
            walls(&[wall((0.0, 0.0), (1.0, 0.0), 0.0)], &[]),
            Err(WallsError::BadThickness(0))
        );
        assert_eq!(
            walls(&one, &[door(1, 500.0, 300.0)]),
            Err(WallsError::NoWall(0))
        );
        assert_eq!(
            walls(&one, &[door(0, 900.0, 300.0)]),
            Err(WallsError::OffTheWall(0))
        );
        assert_eq!(
            walls(&one, &[door(0, 300.0, 300.0), door(0, 500.0, 300.0)]),
            Err(WallsError::Overlap(1))
        );
    }
}
