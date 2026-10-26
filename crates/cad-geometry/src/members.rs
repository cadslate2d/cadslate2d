//! Members with a width (#329): the centrelines of a frame — a truss, a
//! portal, a grid of beams — made into the outlines of members as wide as
//! they are, meeting as a fabricator would join them.
//!
//! Each member is a closed outline half its width either side of its
//! centreline. Where members meet:
//!
//! - **Through.** Collinear centrelines of one width that meet end to end
//!   are one member, and a member is continuous through any joint along
//!   it: a chord runs past the webs that meet it.
//! - **Butted.** A member that ends on another's side is cut by that
//!   member's face: a web stops at the chord. Where two such members end
//!   at the same point, they are also cut where they meet each other, so
//!   they do not overlap.
//! - **Mitred.** Members that end at one point and none through it are cut
//!   along the lines where each one's face meets its neighbour's around
//!   the point, and every outline there runs through the point itself, so
//!   together they fill the joint with no gap and no overlap. A mitre that
//!   would run out further than [`MITRE_LIMIT`] widths from the point is cut
//!   square there instead.
//! - **Free.** A member's end that meets nothing is square.
//!
//! Members that cross without a joint, or overlap along their length, are
//! not resolved: each keeps its own outline.

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::Segment;

/// How far a mitre may run from its joint, in the wider member's widths.
pub const MITRE_LIMIT: f64 = 4.0;

/// A member as drawn: its centreline and how wide it is.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Member {
    pub centre: Segment,
    pub width: f64,
}

/// A member's outline: a closed ring, counter-clockwise, and the members
/// given that it was made from — more than one where collinear ones were
/// one member.
#[derive(Clone, Debug, PartialEq)]
pub struct Outline {
    pub ring: Vec<Point2>,
    pub sources: Vec<usize>,
}

/// Why there are no outlines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MembersError {
    /// A member with no length, by its index.
    NoLength(usize),
    /// A width that is not a size, by the member's index.
    BadWidth(usize),
}

/// One member after the collinear ones are joined: its ends, width, and
/// where it came from.
#[derive(Clone, Debug)]
struct Joined {
    start: Point2,
    end: Point2,
    width: f64,
    sources: Vec<usize>,
}

impl Joined {
    fn direction(&self) -> Vec2 {
        let d = self.end - self.start;
        d * (1.0 / d.length())
    }
    fn length(&self) -> f64 {
        self.start.distance_to(self.end)
    }
}

/// The left normal of a direction: a quarter turn counter-clockwise.
fn left(v: Vec2) -> Vec2 {
    Vec2::new(-v.y, v.x)
}

/// Where the line through `p` along `u` meets the line through `q` along
/// `v`, as the distance along the first; `None` when they are parallel.
fn meet(p: Point2, u: Vec2, q: Point2, v: Vec2) -> Option<f64> {
    let cross = u.x * v.y - u.y * v.x;
    if cross.abs() < 1e-12 {
        return None;
    }
    let w = q - p;
    Some((w.x * v.y - w.y * v.x) / cross)
}

/// Joins collinear members of one width that meet end to end.
fn join(members: &[Member], tolerance: LinearTolerance) -> Vec<Joined> {
    let mut joined: Vec<Joined> = members
        .iter()
        .enumerate()
        .map(|(i, m)| Joined {
            start: m.centre.start,
            end: m.centre.end,
            width: m.width,
            sources: vec![i],
        })
        .collect();
    let same = |a: Point2, b: Point2| tolerance.coincident(a, b);
    loop {
        let mut merged = false;
        'search: for i in 0..joined.len() {
            for j in (i + 1)..joined.len() {
                let (a, b) = (&joined[i], &joined[j]);
                if (a.width - b.width).abs() > tolerance.value() {
                    continue;
                }
                let (da, db) = (a.direction(), b.direction());
                let parallel = (da.x * db.y - da.y * db.x).abs() < 1e-9;
                if !parallel {
                    continue;
                }
                // The far ends, when they share an end; and the shared end
                // between them on the line.
                let ends = if same(a.end, b.start) {
                    Some((a.start, b.end))
                } else if same(a.end, b.end) {
                    Some((a.start, b.start))
                } else if same(a.start, b.start) {
                    Some((a.end, b.end))
                } else if same(a.start, b.end) {
                    Some((a.end, b.start))
                } else {
                    None
                };
                let Some((from, to)) = ends else { continue };
                // Collinear and end to end, not folded back on itself.
                if from.distance_to(to) + tolerance.value() < a.length() + b.length() {
                    continue;
                }
                let mut sources = a.sources.clone();
                sources.extend(&b.sources);
                sources.sort_unstable();
                joined[i] = Joined {
                    start: from,
                    end: to,
                    width: a.width,
                    sources,
                };
                joined.remove(j);
                merged = true;
                break 'search;
            }
        }
        if !merged {
            return joined;
        }
    }
}

/// How one end of a member is cut: the two corners, on its right and its
/// left looking out of the member, and whether the joint's point is a
/// vertex between them.
struct End {
    right: Point2,
    left: Point2,
    through_point: bool,
}

/// The outlines of `members`, in the order of the joined members.
pub fn outlines(
    members: &[Member],
    tolerance: LinearTolerance,
) -> Result<Vec<Outline>, MembersError> {
    for (i, m) in members.iter().enumerate() {
        if !(m.width.is_finite() && m.width > 0.0) {
            return Err(MembersError::BadWidth(i));
        }
        if m.centre.length() <= tolerance.value() {
            return Err(MembersError::NoLength(i));
        }
    }
    let joined = join(members, tolerance);
    let mut out = Vec::with_capacity(joined.len());
    for (index, member) in joined.iter().enumerate() {
        let u = member.direction();
        // Each end, looking out of the member.
        let start = cut(&joined, index, member.start, -u, tolerance);
        let end = cut(&joined, index, member.end, u, tolerance);
        let mut ring = vec![end.right];
        if end.through_point {
            ring.push(member.end);
        }
        ring.push(end.left);
        ring.push(start.right);
        if start.through_point {
            ring.push(member.start);
        }
        ring.push(start.left);
        // Counter-clockwise.
        if crate::signed_area(&ring) < 0.0 {
            ring.reverse();
        }
        out.push(Outline {
            ring,
            sources: member.sources.clone(),
        });
    }
    Ok(out)
}

/// How member `index` is cut at its end `point`, looking out along `out`.
fn cut(
    joined: &[Joined],
    index: usize,
    point: Point2,
    out: Vec2,
    tolerance: LinearTolerance,
) -> End {
    let member = &joined[index];
    let half = member.width / 2.0;
    let n = left(out);
    // The edges, as lines through these points along `out`.
    let (left_edge, right_edge) = (point + n * half, point - n * half);
    let square = End {
        right: right_edge,
        left: left_edge,
        through_point: false,
    };
    // How far along `out` each edge must stop: the most a cut asks for,
    // measured from `point` (negative is back into the member).
    let mut stop_left: Option<f64> = None;
    let mut stop_right: Option<f64> = None;
    let cut_at = |stop: &mut Option<f64>, t: f64| {
        *stop = Some(stop.map_or(t, |s: f64| s.min(t)));
    };

    // Members the point is on, between their ends: the member stops at
    // their face.
    let mut through = false;
    for (j, other) in joined.iter().enumerate() {
        if j == index {
            continue;
        }
        let v = other.direction();
        let along = (point - other.start).dot(v);
        let off = (point - other.start).dot(left(v));
        if off.abs() > tolerance.value()
            || along <= tolerance.value()
            || along >= other.length() - tolerance.value()
        {
            continue;
        }
        // Not along it, but against it.
        if (out.x * v.y - out.y * v.x).abs() < 1e-9 {
            continue;
        }
        through = true;
        // Its face on this member's side: back along `-out` from the point.
        let side = if (-out).dot(left(v)) >= 0.0 {
            1.0
        } else {
            -1.0
        };
        let face = other.start + left(v) * (side * other.width / 2.0);
        if let Some(t) = meet(left_edge, out, face, v) {
            cut_at(&mut stop_left, t);
        }
        if let Some(t) = meet(right_edge, out, face, v) {
            cut_at(&mut stop_right, t);
        }
    }

    // Members that end at the point too, by the angle they leave it at.
    let mut ending: Vec<(f64, usize, Vec2)> = joined
        .iter()
        .enumerate()
        .filter(|(j, _)| *j != index)
        .filter_map(|(j, other)| {
            let leaving = if tolerance.coincident(other.start, point) {
                other.direction()
            } else if tolerance.coincident(other.end, point) {
                -other.direction()
            } else {
                return None;
            };
            Some((leaving.y.atan2(leaving.x), j, leaving))
        })
        .collect();
    if ending.is_empty() && !through {
        return square;
    }
    // This member leaves the point back along `-out`.
    let own = (-out).y.atan2(-out.x);
    let turn = |angle: f64| (angle - own).rem_euclid(std::f64::consts::TAU);
    ending.sort_by(|a, b| turn(a.0).total_cmp(&turn(b.0)));
    let limit = MITRE_LIMIT
        * ending
            .iter()
            .map(|(_, j, _)| joined[*j].width)
            .fold(member.width, f64::max);
    if let (Some(first), Some(last)) = (ending.first(), ending.last()) {
        // The neighbour counter-clockwise from this member, as both leave
        // the point, is on this member's right looking out, and this member
        // on its right; the one clockwise is on the left, and this member
        // on its left.
        for ((_, j, leaving), on_left) in [(first, false), (last, true)] {
            let other = &joined[*j];
            let m = left(*leaving);
            // The neighbour's face towards this member.
            let towards = if on_left { 1.0 } else { -1.0 };
            let face = point + m * (towards * other.width / 2.0);
            let edge = if on_left { left_edge } else { right_edge };
            match meet(edge, out, face, *leaving) {
                Some(t) if t.abs() <= limit => {
                    if on_left {
                        cut_at(&mut stop_left, t);
                    } else {
                        cut_at(&mut stop_right, t);
                    }
                }
                // Parallel, or a mitre too long: square at the point.
                _ => {
                    if on_left {
                        cut_at(&mut stop_left, 0.0);
                    } else {
                        cut_at(&mut stop_right, 0.0);
                    }
                }
            }
        }
    }
    let left_stop = stop_left.unwrap_or(0.0);
    let right_stop = stop_right.unwrap_or(0.0);
    End {
        left: left_edge + out * left_stop,
        right: right_edge + out * right_stop,
        // Members mitred round a point all run through it.
        through_point: !through && !ending.is_empty(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn m(a: Point2, b: Point2, width: f64) -> Member {
        Member {
            centre: Segment::new(a, b),
            width,
        }
    }

    const T: LinearTolerance = LinearTolerance::DEFAULT;

    fn area(o: &Outline) -> f64 {
        crate::signed_area(&o.ring)
    }

    /// Whether two outlines overlap: a vertex of one strictly inside the
    /// other, or their edges crossing properly.
    fn overlap(a: &Outline, b: &Outline) -> bool {
        let inside = |ring: &[Point2], q: Point2| {
            // Strictly: nudged towards the ring's centre, still inside, and
            // not on an edge of it.
            let on_edge = ring
                .iter()
                .zip(ring.iter().cycle().skip(1))
                .any(|(s, e)| Segment::new(*s, *e).closest_point(q).distance_to(q) < 1e-7);
            !on_edge && crate::ring_contains(ring, q)
        };
        if a.ring.iter().any(|q| inside(&b.ring, *q)) || b.ring.iter().any(|q| inside(&a.ring, *q))
        {
            return true;
        }
        let edges = |r: &[Point2]| -> Vec<Segment> {
            r.iter()
                .zip(r.iter().cycle().skip(1))
                .map(|(s, e)| Segment::new(*s, *e))
                .collect()
        };
        for ea in edges(&a.ring) {
            for eb in edges(&b.ring) {
                if let Some(x) = crate::intersect::segment_segment(&ea, &eb) {
                    let at_end = [ea.start, ea.end, eb.start, eb.end]
                        .iter()
                        .any(|q| q.distance_to(x) < 1e-7);
                    if !at_end {
                        return true;
                    }
                }
            }
        }
        false
    }

    #[test]
    fn a_lone_member_is_a_rectangle_with_square_ends() {
        let out = outlines(&[m(p(0.0, 0.0), p(100.0, 0.0), 10.0)], T).unwrap();
        assert_eq!(out.len(), 1);
        assert!((area(&out[0]) - 1000.0).abs() < 1e-9);
        assert_eq!(out[0].ring.len(), 4);
        assert_eq!(
            outlines(&[m(p(0.0, 0.0), p(0.0, 0.0), 10.0)], T),
            Err(MembersError::NoLength(0))
        );
        assert_eq!(
            outlines(&[m(p(0.0, 0.0), p(1.0, 0.0), 0.0)], T),
            Err(MembersError::BadWidth(0))
        );
    }

    #[test]
    fn collinear_pieces_of_one_width_are_one_member() {
        let out = outlines(
            &[
                m(p(0.0, 0.0), p(50.0, 0.0), 10.0),
                m(p(100.0, 0.0), p(50.0, 0.0), 10.0),
                m(p(100.0, 0.0), p(150.0, 0.0), 10.0),
            ],
            T,
        )
        .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].sources, vec![0, 1, 2]);
        assert!((area(&out[0]) - 1500.0).abs() < 1e-9);
    }

    #[test]
    fn a_corner_is_mitred_and_fills_the_joint_exactly() {
        // An L of two members 100 long, 10 wide: together the L's area.
        let out = outlines(
            &[
                m(p(0.0, 0.0), p(100.0, 0.0), 10.0),
                m(p(0.0, 0.0), p(0.0, 100.0), 10.0),
            ],
            T,
        )
        .unwrap();
        assert_eq!(out.len(), 2);
        let total: f64 = out.iter().map(area).sum();
        assert!((total - 2000.0).abs() < 1e-9, "{total}");
        assert!(!overlap(&out[0], &out[1]));
        // The mitre's corners are the L's: (-5, -5) outside, (5, 5) inside.
        for o in &out {
            assert!(o.ring.iter().any(|q| q.distance_to(p(-5.0, -5.0)) < 1e-9));
            assert!(o.ring.iter().any(|q| q.distance_to(p(5.0, 5.0)) < 1e-9));
        }
    }

    #[test]
    fn a_member_ending_on_anothers_side_stops_at_its_face() {
        // A post 6 wide standing on a beam 10 wide, at the beam's middle.
        let out = outlines(
            &[
                m(p(0.0, 0.0), p(100.0, 0.0), 10.0),
                m(p(50.0, 0.0), p(50.0, 80.0), 6.0),
            ],
            T,
        )
        .unwrap();
        assert!((area(&out[0]) - 1000.0).abs() < 1e-9, "the beam is whole");
        assert!(
            (area(&out[1]) - 6.0 * 75.0).abs() < 1e-9,
            "the post from y = 5"
        );
        assert!(out[1].ring.iter().all(|q| q.y >= 5.0 - 1e-9));
        assert!(!overlap(&out[0], &out[1]));
    }

    #[test]
    fn a_sharp_mitre_is_cut_square_at_the_limit() {
        // Two members at a hair's angle: a mitre would run out far past the
        // joint; each stops square there instead.
        let out = outlines(
            &[
                m(p(0.0, 0.0), p(100.0, 0.0), 10.0),
                m(p(0.0, 0.0), p(100.0, 1.0), 10.0),
            ],
            T,
        )
        .unwrap();
        for o in &out {
            let reach = o.ring.iter().map(|q| q.x).fold(f64::MAX, f64::min);
            assert!(reach > -MITRE_LIMIT * 10.0 - 1e-9, "{reach}");
        }
    }

    /// The maintainer's truss: span 1200, rise 200, eight panels of 150;
    /// chords 10 wide, Warren diagonals 5. No two outlines overlap, the
    /// chords are continuous, and the whole is symmetric about x = 600.
    #[test]
    fn a_warren_truss_has_members_that_meet_and_never_overlap() {
        let rise = |x: f64| {
            if x <= 600.0 {
                x / 3.0
            } else {
                (1200.0 - x) / 3.0
            }
        };
        let mut members = vec![
            m(p(0.0, 0.0), p(1200.0, 0.0), 10.0),
            m(p(0.0, 0.0), p(600.0, 200.0), 10.0),
            m(p(600.0, 200.0), p(1200.0, 0.0), 10.0),
        ];
        // Warren: diagonals from each bottom panel point up to the top
        // chord at the next and back down, alternating.
        for k in 0..8 {
            let (x0, x1) = (150.0 * f64::from(k), 150.0 * f64::from(k + 1));
            let (a, b) = if k % 2 == 0 {
                (p(x0, 0.0), p(x1, rise(x1)))
            } else {
                (p(x0, rise(x0)), p(x1, 0.0))
            };
            // The end diagonals run along the chords at the supports; leave
            // them out, as a Warren truss's first panel is the chords'.
            if a.distance_to(b) > 1.0 && !(k == 0 || k == 7) {
                members.push(m(a, b, 5.0));
            }
        }
        let out = outlines(&members, T).unwrap();
        assert_eq!(out.len(), members.len(), "nothing collinear to join");
        for (i, a) in out.iter().enumerate() {
            assert!(area(a) > 0.0);
            for b in &out[i + 1..] {
                assert!(
                    !overlap(a, b),
                    "{:?} and {:?} overlap",
                    a.sources,
                    b.sources
                );
            }
        }
        // Symmetric: the mirror image of every vertex is a vertex.
        let all: Vec<Point2> = out.iter().flat_map(|o| o.ring.clone()).collect();
        for q in &all {
            let mirrored = p(1200.0 - q.x, q.y);
            assert!(all.iter().any(|r| r.distance_to(mirrored) < 1e-6), "{q:?}");
        }
    }
}
