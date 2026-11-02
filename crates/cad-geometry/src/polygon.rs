//! Polygons (ADR-0021): rings of points, their area and what they contain,
//! a path flattened into one, and the triangles that fill a ring with
//! holes cut out of it — what a solid hatch is drawn as.

use cad_core::Point2;

use crate::path::Path;
use crate::polyline::{bulge_primitive, polyline_spans};
use crate::{Arc, Primitive};

/// The signed area of a ring: positive when it runs counter-clockwise.
pub fn signed_area(ring: &[Point2]) -> f64 {
    let n = ring.len();
    if n < 3 {
        return 0.0;
    }
    let twice: f64 = (0..n)
        .map(|i| {
            let (a, b) = (ring[i], ring[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum();
    twice / 2.0
}

/// Whether `point` lies in `ring` by the even-odd rule, with a ray cast to
/// the right. A point on an edge is taken as inside.
pub fn contains(ring: &[Point2], point: Point2) -> bool {
    let n = ring.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    for i in 0..n {
        let (a, b) = (ring[i], ring[(i + 1) % n]);
        if on_segment(a, b, point) {
            return true;
        }
        if (a.y > point.y) != (b.y > point.y) {
            let x = a.x + (point.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if point.x < x {
                inside = !inside;
            }
        }
    }
    inside
}

/// Whether `point` lies on the segment `a`–`b`, to a relative tolerance:
/// its distance to the nearest point of the segment, which for a segment
/// of no length is `a` itself.
fn on_segment(a: Point2, b: Point2, point: Point2) -> bool {
    let scale = 1.0 + a.x.abs() + a.y.abs() + b.x.abs() + b.y.abs();
    let along = b - a;
    let length_squared = along.length_squared();
    let t = if length_squared > 0.0 {
        ((point - a).dot(along) / length_squared).clamp(0.0, 1.0)
    } else {
        0.0
    };
    (a + along * t).distance_to(point) <= 1e-9 * scale
}

/// The points after the start of `arc`, up to and including its end, no
/// more than `max_step` radians apart; from the end back to the start when
/// `backwards`.
pub fn arc_chords(arc: &Arc, max_step: f64, backwards: bool) -> Vec<Point2> {
    let steps = (arc.sweep / max_step.max(1e-6)).ceil().max(1.0) as usize;
    (1..=steps)
        .map(|step| {
            let fraction = step as f64 / steps as f64;
            let fraction = if backwards { 1.0 - fraction } else { fraction };
            arc.point_at(arc.start + arc.sweep * fraction)
        })
        .collect()
}

/// A closed path as a ring: its vertices, with each arc segment as chords
/// no more than `max_step` radians apart. An open path is closed by its
/// last segment back to the start.
pub fn flatten_path(path: &Path, max_step: f64) -> Vec<Point2> {
    let vertices = &path.vertices;
    if vertices.is_empty() {
        return Vec::new();
    }
    let bulge = |index: usize| path.bulges.get(index).copied().unwrap_or(0.0);
    let mut ring = vec![vertices[0]];
    for (from, to) in polyline_spans(vertices.len(), true, bulge) {
        match bulge_primitive(vertices[from], vertices[to], bulge(from)) {
            Primitive::Segment(segment) => ring.push(segment.end),
            Primitive::Arc(arc) => ring.extend(arc_chords(&arc, max_step, bulge(from) < 0.0)),
            Primitive::Circle(circle) => {
                let full = Arc::new(circle.center, circle.radius, 0.0, std::f64::consts::TAU);
                ring.extend(arc_chords(&full, max_step, false));
            }
        }
    }
    // The last span closes the ring: its end is the start again, to
    // within what an arc's end computes to.
    if let (Some(first), Some(last)) = (ring.first(), ring.last())
        && ring.len() > 1
    {
        let scale = 1.0 + first.x.abs() + first.y.abs();
        if first.distance_to(*last) <= 1e-9 * scale {
            ring.pop();
        }
    }
    ring
}

/// Twice the signed area of the triangle `a`, `b`, `c`.
fn cross(a: Point2, b: Point2, c: Point2) -> f64 {
    (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)
}

/// Whether `p` lies inside the triangle `a`, `b`, `c`, in either
/// orientation, or on an edge of it without being a corner.
fn in_triangle(a: Point2, b: Point2, c: Point2, p: Point2) -> bool {
    if p == a || p == b || p == c {
        return false;
    }
    let (b, c) = if cross(a, b, c) < 0.0 { (c, b) } else { (b, c) };
    cross(a, b, p) >= 0.0 && cross(b, c, p) >= 0.0 && cross(c, a, p) >= 0.0
}

/// `ring` with consecutive duplicates removed, running counter-clockwise.
fn cleaned(ring: &[Point2], counter_clockwise: bool) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::with_capacity(ring.len());
    for point in ring {
        if out.last() != Some(point) {
            out.push(*point);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    if (signed_area(&out) > 0.0) != counter_clockwise {
        out.reverse();
    }
    out
}

/// Splices `hole` (clockwise) into `outer` (counter-clockwise) with a
/// bridge from the hole's leftmost vertex to a visible vertex of the outer
/// ring, so that one ring bounds the region between them.
fn bridge(outer: &mut Vec<Point2>, hole: &[Point2]) {
    let Some(start) = (0..hole.len()).min_by(|&i, &j| {
        hole[i]
            .x
            .total_cmp(&hole[j].x)
            .then(hole[i].y.total_cmp(&hole[j].y))
    }) else {
        return;
    };
    let from = hole[start];
    // The nearest edge of the outer ring crossed by a ray to the left of
    // the hole's leftmost vertex; its left-hand endpoint is a candidate:
    // the triangle between the hole vertex, the hit, and that endpoint
    // then lies wholly to the left of the hole, so nothing of the hole can
    // be in it, and the bridge cannot cross the hole (the right-hand
    // endpoint would reach back under it).
    let n = outer.len();
    let mut best: Option<(f64, usize)> = None;
    for i in 0..n {
        let (a, b) = (outer[i], outer[(i + 1) % n]);
        if (a.y > from.y) != (b.y > from.y) {
            let x = a.x + (from.y - a.y) / (b.y - a.y) * (b.x - a.x);
            if x <= from.x && best.is_none_or(|(bx, _)| x > bx) {
                let endpoint = if a.x < b.x { i } else { (i + 1) % n };
                best = Some((x, endpoint));
            }
        }
    }
    let Some((hit_x, mut candidate)) = best else {
        // No edge to the left: the hole is not inside; the outer vertex
        // nearest to it will do.
        let nearest = (0..n)
            .min_by(|&i, &j| {
                outer[i]
                    .distance_to(from)
                    .total_cmp(&outer[j].distance_to(from))
            })
            .unwrap_or(0);
        splice(outer, nearest, hole, start);
        return;
    };
    // A reflex vertex of the outer ring inside the triangle between the
    // hole vertex, the hit, and the candidate would block the bridge: take
    // the one of those making the smallest angle with the ray instead.
    let hit = Point2::new(hit_x, from.y);
    let candidate_point = outer[candidate];
    let mut best_angle = f64::INFINITY;
    for i in 0..n {
        let p = outer[i];
        if i == candidate || p.x > from.x {
            continue;
        }
        let (prev, next) = (outer[(i + n - 1) % n], outer[(i + 1) % n]);
        let reflex = cross(prev, p, next) < 0.0;
        if !reflex {
            continue;
        }
        let (lo, hi) = if candidate_point.y < from.y {
            (candidate_point, hit)
        } else {
            (hit, candidate_point)
        };
        if in_triangle(from, lo, hi, p) || p == hit {
            let angle = ((p.y - from.y).abs()).atan2(from.x - p.x);
            if angle < best_angle {
                best_angle = angle;
                candidate = i;
            }
        }
    }
    splice(outer, candidate, hole, start);
}

/// Inserts `hole`, starting at `start`, after `outer[at]`, with the bridge
/// edges there and back.
fn splice(outer: &mut Vec<Point2>, at: usize, hole: &[Point2], start: usize) {
    let mut inserted: Vec<Point2> = Vec::with_capacity(hole.len() + 2);
    for k in 0..=hole.len() {
        inserted.push(hole[(start + k) % hole.len()]);
    }
    inserted.push(outer[at]);
    let tail = outer.split_off(at + 1);
    outer.extend(inserted);
    outer.extend(tail);
}

/// The triangles filling `outer` with `holes` cut out of it: ear clipping,
/// the holes first bridged into the outer ring. Rings may run either way;
/// repeated points are tolerated. A ring of fewer than three distinct
/// points fills nothing.
pub fn tessellate(outer: &[Point2], holes: &[Vec<Point2>]) -> Vec<[Point2; 3]> {
    let mut ring = cleaned(outer, true);
    if ring.len() < 3 {
        return Vec::new();
    }
    let mut holes: Vec<Vec<Point2>> = holes
        .iter()
        .map(|hole| cleaned(hole, false))
        .filter(|hole| hole.len() >= 3)
        .collect();
    // Leftmost hole first, so each bridge goes to the outer ring as it is.
    holes.sort_by(|a, b| {
        let left = |ring: &[Point2]| ring.iter().map(|p| p.x).fold(f64::INFINITY, f64::min);
        left(a).total_cmp(&left(b))
    });
    for hole in &holes {
        bridge(&mut ring, hole);
    }
    ear_clip(ring)
}

/// Ear clipping of one counter-clockwise ring, possibly with bridges
/// (edges traversed both ways). When no ear can be found — a numerical
/// corner — the flattest corner is clipped so the clipping always ends.
fn ear_clip(mut ring: Vec<Point2>) -> Vec<[Point2; 3]> {
    let mut triangles = Vec::with_capacity(ring.len().saturating_sub(2));
    while ring.len() > 3 {
        let n = ring.len();
        let mut clipped = false;
        for i in 0..n {
            let (prev, here, next) = (ring[(i + n - 1) % n], ring[i], ring[(i + 1) % n]);
            if cross(prev, here, next) <= 0.0 {
                continue; // reflex or flat: not an ear
            }
            let blocked = ring.iter().any(|p| in_triangle(prev, here, next, *p));
            if blocked {
                continue;
            }
            triangles.push([prev, here, next]);
            ring.remove(i);
            clipped = true;
            break;
        }
        if !clipped {
            let flattest = (0..n)
                .min_by(|&i, &j| {
                    let area =
                        |k: usize| cross(ring[(k + n - 1) % n], ring[k], ring[(k + 1) % n]).abs();
                    area(i).total_cmp(&area(j))
                })
                .unwrap_or(0);
            let (prev, here, next) = (
                ring[(flattest + n - 1) % n],
                ring[flattest],
                ring[(flattest + 1) % n],
            );
            if cross(prev, here, next) > 0.0 {
                triangles.push([prev, here, next]);
            }
            ring.remove(flattest);
        }
    }
    if ring.len() == 3 && cross(ring[0], ring[1], ring[2]) > 0.0 {
        triangles.push([ring[0], ring[1], ring[2]]);
    }
    triangles
}

/// Whether any two edges of `rings` meet, but the two of one ring that
/// share a vertex (#414): rings that cross each other, or a ring that
/// crosses itself. Rings that do not are each inside or outside every
/// other whole, and [`tessellate`] fills them as outer rings with their
/// holes; ones that do take [`even_odd_fill`]. Edges that only touch are
/// taken as meeting.
pub fn rings_cross(rings: &[Vec<Point2>]) -> bool {
    // A sweep upwards: an edge is compared with those that reach its
    // height alone, so a ring of many vertices costs little more than its
    // sorting — this runs for every solid hatch drawn.
    let rings: Vec<Vec<Point2>> = rings.iter().map(|ring| without_repeats(ring)).collect();
    let mut edges = ring_edges(&rings);
    edges.sort_by(|p, q| p.0.y.min(p.1.y).total_cmp(&q.0.y.min(q.1.y)));
    let mut active: Vec<usize> = Vec::new();
    for (i, &(a, b, ring, index)) in edges.iter().enumerate() {
        let bottom = a.y.min(b.y);
        active.retain(|&j| edges[j].0.y.max(edges[j].1.y) >= bottom);
        for &j in &active {
            let (c, d, other, other_index) = edges[j];
            let adjacent = ring == other && {
                let count = rings[ring].len();
                let (low, high) = (index.min(other_index), index.max(other_index));
                high == low + 1 || (low == 0 && high + 1 == count)
            };
            if !adjacent && segments_meet(a, b, c, d) {
                return true;
            }
        }
        active.push(i);
    }
    false
}

/// The triangles filling the region `rings` bound by the even-odd rule —
/// a point is in it when it is inside an odd number of them, as
/// [`contains`] and a PDF's `f*` take it — however they cross (#414). The
/// region is cut into horizontal slabs at every vertex and every crossing,
/// so no two edges cross within one; across each, its edges are paired
/// left to right, and each pair bounds a trapezoid of the region, two
/// triangles counter-clockwise.
// Exact on purpose: an edge is level when its ends are at one height, and
// a slab at an edge's end has that end exactly, so slabs meet without gaps.
#[allow(clippy::float_cmp)]
pub fn even_odd_fill(rings: &[Vec<Point2>]) -> Vec<[Point2; 3]> {
    // Each edge running upwards, sorted by where it starts. Both passes
    // below sweep up through them, an edge meeting only the edges whose
    // heights overlap its own — loops that merely touch come this way
    // too, so their cost is the drawing's, not every pair of edges'.
    let mut edges: Vec<(Point2, Point2)> = ring_edges(rings)
        .into_iter()
        .filter(|(a, b, ..)| a.y != b.y)
        .map(|(a, b, ..)| if a.y < b.y { (a, b) } else { (b, a) })
        .collect();
    edges.sort_by(|p, q| p.0.y.total_cmp(&q.0.y));
    let mut levels: Vec<f64> = edges.iter().flat_map(|(a, b)| [a.y, b.y]).collect();
    let mut active: Vec<usize> = Vec::new();
    for (i, &(a, b)) in edges.iter().enumerate() {
        active.retain(|&j| edges[j].1.y >= a.y);
        for &j in &active {
            let (c, d) = edges[j];
            if let Some(y) = crossing_height(a, b, c, d) {
                levels.push(y);
            }
        }
        active.push(i);
    }
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let x_at = |(a, b): (Point2, Point2), y: f64| {
        if y == a.y {
            a.x
        } else if y == b.y {
            b.x
        } else {
            a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x)
        }
    };
    let mut triangles = Vec::new();
    let (mut spanning, mut next) = (Vec::<usize>::new(), 0);
    for slab in levels.windows(2) {
        let (low, high) = (slab[0], slab[1]);
        // The edges across the slab: begun by its bottom, not ended there.
        // Every edge ends at a level, so one past `low` reaches `high`.
        while next < edges.len() && edges[next].0.y <= low {
            spanning.push(next);
            next += 1;
        }
        spanning.retain(|&j| edges[j].1.y > low);
        let mut across: Vec<(f64, f64)> = spanning
            .iter()
            .map(|&j| (x_at(edges[j], low), x_at(edges[j], high)))
            .collect();
        across.sort_by(|p, q| (p.0 + p.1).total_cmp(&(q.0 + q.1)));
        for &[left, right] in across.as_chunks::<2>().0 {
            let corners = [
                Point2::new(left.0, low),
                Point2::new(right.0, low),
                Point2::new(right.1, high),
                Point2::new(left.1, high),
            ];
            for triangle in [
                [corners[0], corners[1], corners[2]],
                [corners[0], corners[2], corners[3]],
            ] {
                if cross(triangle[0], triangle[1], triangle[2]) > 0.0 {
                    triangles.push(triangle);
                }
            }
        }
    }
    triangles
}

/// `ring` without a point repeated after itself, the last before the
/// first included: a ring closed by its first point again has the edges a
/// ring has, each meeting only its neighbours.
fn without_repeats(ring: &[Point2]) -> Vec<Point2> {
    let mut out: Vec<Point2> = Vec::with_capacity(ring.len());
    for &point in ring {
        if out.last() != Some(&point) {
            out.push(point);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

/// Every edge of every ring of three points or more, with its ring and
/// its place in it; an edge of no length is none.
fn ring_edges(rings: &[Vec<Point2>]) -> Vec<(Point2, Point2, usize, usize)> {
    rings
        .iter()
        .enumerate()
        .filter(|(_, ring)| ring.len() >= 3)
        .flat_map(|(ring_index, ring)| {
            (0..ring.len()).filter_map(move |i| {
                let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                (a != b).then_some((a, b, ring_index, i))
            })
        })
        .collect()
}

/// Whether the segments `ab` and `cd` have a point in common.
fn segments_meet(a: Point2, b: Point2, c: Point2, d: Point2) -> bool {
    let (d1, d2) = (cross(c, d, a), cross(c, d, b));
    let (d3, d4) = (cross(a, b, c), cross(a, b, d));
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    let within = |p: Point2, q: Point2, r: Point2| {
        r.x >= p.x.min(q.x) && r.x <= p.x.max(q.x) && r.y >= p.y.min(q.y) && r.y <= p.y.max(q.y)
    };
    (d1 == 0.0 && within(c, d, a))
        || (d2 == 0.0 && within(c, d, b))
        || (d3 == 0.0 && within(a, b, c))
        || (d4 == 0.0 && within(a, b, d))
}

/// The height at which `ab` and `cd`, each running upwards, cross inside
/// both; none when they do not, or run alongside.
fn crossing_height(a: Point2, b: Point2, c: Point2, d: Point2) -> Option<f64> {
    let r = b - a;
    let s = d - c;
    let denominator = r.x * s.y - r.y * s.x;
    if denominator == 0.0 {
        return None;
    }
    let offset = c - a;
    let t = (offset.x * s.y - offset.y * s.x) / denominator;
    let u = (offset.x * r.y - offset.y * r.x) / denominator;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(a.y + t * r.y)
}

#[cfg(test)]
mod tests {
    use std::f64::consts::TAU;

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn square(x: f64, y: f64, size: f64) -> Vec<Point2> {
        vec![
            p(x, y),
            p(x + size, y),
            p(x + size, y + size),
            p(x, y + size),
        ]
    }

    fn area_of(triangles: &[[Point2; 3]]) -> f64 {
        triangles
            .iter()
            .map(|[a, b, c]| cross(*a, *b, *c) / 2.0)
            .sum()
    }

    fn centroid([a, b, c]: &[Point2; 3]) -> Point2 {
        p((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)
    }

    /// A diamond with a star hole whose leftmost vertex sits just left of
    /// the diamond's lower edge's upper end: a bridge to the edge's
    /// right-hand endpoint ran under the star and cut through it, and the
    /// fill came up short of the region (#172's workspace run found it).
    #[test]
    fn a_bridge_never_crosses_the_hole_it_is_for() {
        let outer = star(p(0.0, 0.0), 4, &[40.0]);
        let hole = star(p(3.0, -2.0), 8, &[8.035699601431164, 10.933490638889914]);
        let triangles = tessellate(&outer, std::slice::from_ref(&hole));
        let expected = signed_area(&outer).abs() - signed_area(&hole).abs();
        assert!((area_of(&triangles) - expected).abs() < 1e-6 * expected);
        for triangle in &triangles {
            let middle = centroid(triangle);
            assert!(
                contains(&outer, middle) && !contains(&hole, middle),
                "{triangle:?}"
            );
        }
    }

    #[test]
    fn area_and_containment_follow_the_even_odd_rule() {
        let ring = square(0.0, 0.0, 10.0);
        assert_eq!(signed_area(&ring), 100.0);
        let mut clockwise = ring.clone();
        clockwise.reverse();
        assert_eq!(signed_area(&clockwise), -100.0);
        assert!(contains(&ring, p(5.0, 5.0)));
        assert!(contains(&ring, p(10.0, 5.0)), "on an edge");
        assert!(contains(&ring, p(0.0, 0.0)), "on a corner");
        assert!(!contains(&ring, p(10.1, 5.0)));
        assert!(
            !contains(&ring, p(-1.0, 0.0)),
            "on the extension of an edge"
        );
        // A ring with a repeated point does not claim the whole line through it.
        let repeated = vec![
            p(-20.0, 0.0),
            p(0.0, 20.0),
            p(20.0, 0.0),
            p(0.0, -20.0),
            p(-20.0, 1e-15),
        ];
        assert!(!contains(&repeated, p(-80.0, 0.0)));
        assert!(contains(&repeated, p(0.0, 0.0)));
        assert!(!contains(&[p(0.0, 0.0), p(1.0, 1.0)], p(0.5, 0.5)));
    }

    #[test]
    fn a_square_is_two_triangles_and_a_square_with_a_hole_eight() {
        let outer = square(0.0, 0.0, 10.0);
        let triangles = tessellate(&outer, &[]);
        assert_eq!(triangles.len(), 2);
        assert!((area_of(&triangles) - 100.0).abs() < 1e-9);
        let hole = square(4.0, 4.0, 2.0);
        let triangles = tessellate(&outer, std::slice::from_ref(&hole));
        assert_eq!(triangles.len(), 8);
        assert!((area_of(&triangles) - 96.0).abs() < 1e-9);
        for triangle in &triangles {
            let middle = centroid(triangle);
            assert!(
                contains(&outer, middle) && !contains(&hole, middle),
                "{triangle:?}"
            );
        }
        // Orientation and repeated points make no difference.
        let mut clockwise = outer.clone();
        clockwise.reverse();
        clockwise.push(clockwise[0]);
        let mut hole_ccw = hole;
        hole_ccw.reverse();
        let again = tessellate(&clockwise, &[hole_ccw]);
        assert!((area_of(&again) - 96.0).abs() < 1e-9);
    }

    #[test]
    fn concave_rings_and_two_holes_are_filled_without_overlap() {
        // An L shape.
        let l = vec![
            p(0.0, 0.0),
            p(10.0, 0.0),
            p(10.0, 4.0),
            p(4.0, 4.0),
            p(4.0, 10.0),
            p(0.0, 10.0),
        ];
        let triangles = tessellate(&l, &[]);
        assert_eq!(triangles.len(), 4);
        assert!((area_of(&triangles) - 64.0).abs() < 1e-9);
        assert!(triangles.iter().all(|t| contains(&l, centroid(t))));
        // Two holes in a square: the area is what is left.
        let outer = square(0.0, 0.0, 20.0);
        let holes = vec![square(2.0, 2.0, 4.0), square(12.0, 10.0, 5.0)];
        let triangles = tessellate(&outer, &holes);
        assert!((area_of(&triangles) - (400.0 - 16.0 - 25.0)).abs() < 1e-9);
        for triangle in &triangles {
            let middle = centroid(triangle);
            assert!(contains(&outer, middle));
            assert!(
                holes.iter().all(|hole| !contains(hole, middle)),
                "{triangle:?}"
            );
        }
    }

    #[test]
    fn a_path_with_arcs_flattens_into_a_ring_that_closes() {
        // A slot: two semicircles (bulge 1) joining two straight sides.
        let slot = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 4.0), p(0.0, 4.0)],
            bulges: vec![0.0, 1.0, 0.0, 1.0],
            closed: true,
        };
        let ring = flatten_path(&slot, TAU / 32.0);
        assert!(ring.len() > 20);
        assert_ne!(ring.first(), ring.last());
        let expected = 40.0 + std::f64::consts::PI * 4.0;
        assert!(
            (signed_area(&ring) - expected).abs() < 0.2,
            "{}",
            signed_area(&ring)
        );
        // A circle as a path: two half turns.
        let circle = Path {
            vertices: vec![p(-5.0, 0.0), p(5.0, 0.0)],
            bulges: vec![1.0, 1.0],
            closed: true,
        };
        let ring = flatten_path(&circle, TAU / 64.0);
        assert!((signed_area(&ring) - std::f64::consts::PI * 25.0).abs() < 0.2);
        // A clockwise arc segment runs the chords the other way.
        let bowl = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)],
            bulges: vec![-0.5, 0.0, 0.0, 0.0],
            closed: true,
        };
        let ring = flatten_path(&bowl, TAU / 32.0);
        // Clockwise from (0,0) to (10,0) passes above the chord: the arc
        // eats into the square.
        // A bulge of a half rises a quarter of the chord: about two thirds of
        // 10 by 2.5 is cut away.
        assert!(
            signed_area(&ring) < 84.0 && signed_area(&ring) > 82.0,
            "{}",
            signed_area(&ring)
        );
        assert!(ring.iter().all(|point| point.y >= 0.0 && point.y <= 10.0));
    }

    /// The experiment behind ADR-0021, run by hand: `cargo test -p
    /// cad-geometry measure_fills -- --ignored --nocapture`. For regions of
    /// 10, 100, and 1,000 vertices with two holes: how many triangles the
    /// fill is, their bytes on the wire (six `f32`s each), and the time to
    /// cut them — against a fill of parallel lines one drawing unit apart,
    /// clipped to the region, which the renderer could draw as it draws
    /// everything else (four `f32`s and a distance each).
    #[test]
    #[ignore]
    fn measure_fills() {
        for &count in &[10usize, 100, 1000] {
            let outer = star(p(0.0, 0.0), count, &[100.0, 90.0, 95.0]);
            let holes = vec![
                star(p(30.0, 20.0), (count / 4).max(3), &[12.0, 9.0]),
                star(p(-35.0, -25.0), (count / 4).max(3), &[10.0, 14.0]),
            ];
            let started = std::time::Instant::now();
            let runs = 20;
            let mut triangles = Vec::new();
            for _ in 0..runs {
                triangles = tessellate(&outer, &holes);
            }
            let per_run = started.elapsed() / runs;
            let area = area_of(&triangles);
            // Lines a unit apart across the region, each clipped to it:
            // one segment per crossing of the region's rings.
            let started = std::time::Instant::now();
            let mut segments = 0usize;
            for _ in 0..runs {
                segments = 0;
                let mut y = -100.0;
                while y <= 100.0 {
                    let mut crossings: Vec<f64> = Vec::new();
                    for ring in std::iter::once(&outer).chain(&holes) {
                        for i in 0..ring.len() {
                            let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
                            if (a.y > y) != (b.y > y) {
                                crossings.push(a.x + (y - a.y) / (b.y - a.y) * (b.x - a.x));
                            }
                        }
                    }
                    crossings.sort_by(f64::total_cmp);
                    segments += crossings.len() / 2;
                    y += 1.0;
                }
            }
            let lines_per_run = started.elapsed() / runs;
            println!(
                "{count} vertices: {} triangles, {} bytes, {:?}; {area:.0} area; lines: {segments} segments, {} bytes, {:?}",
                triangles.len(),
                triangles.len() * 6 * 4,
                per_run,
                segments * 5 * 4,
                lines_per_run
            );
        }
    }

    /// A star-shaped ring: `count` points around `center` at radii between
    /// `low` and `high`, in angle order, so it never crosses itself.
    fn star(center: Point2, count: usize, radii: &[f64]) -> Vec<Point2> {
        (0..count)
            .map(|i| {
                let angle = i as f64 / count as f64 * TAU;
                let radius = radii[i % radii.len()];
                center + cad_core::Vec2::from_polar(radius, angle)
            })
            .collect()
    }

    /// #414: two squares crossing, neither inside the other, fill what is
    /// inside one of them alone, as `contains` and a PDF's `f*` take it:
    /// 150, not their 200 nor their union's 175.
    #[test]
    fn crossing_rings_fill_their_even_odd_region() {
        let rings = vec![square(0.0, 0.0, 10.0), square(5.0, 5.0, 10.0)];
        assert!(rings_cross(&rings));
        let triangles = even_odd_fill(&rings);
        assert!(
            (area_of(&triangles) - 150.0).abs() < 1e-9,
            "{}",
            area_of(&triangles)
        );
        let covered = |point: Point2| {
            triangles.iter().any(|&[a, b, c]| {
                cross(a, b, point) > 0.0 && cross(b, c, point) > 0.0 && cross(c, a, point) > 0.0
            })
        };
        assert!(!covered(p(7.5, 7.5)), "the overlap is empty");
        assert!(covered(p(2.0, 2.0)) && covered(p(12.0, 12.0)));
        // A ring crossing itself, a bow tie: its two halves.
        let bow = vec![vec![p(0.0, 0.0), p(10.0, 10.0), p(10.0, 0.0), p(0.0, 10.0)]];
        assert!(rings_cross(&bow));
        assert!((area_of(&even_odd_fill(&bow)) - 50.0).abs() < 1e-9);
        // A ring closed by its first point again, or with a point twice,
        // crosses nothing for that.
        let mut closed = square(0.0, 0.0, 10.0);
        closed.push(p(0.0, 0.0));
        closed.insert(2, closed[1]);
        assert!(!rings_cross(&[closed]));
        // An island touching its boundary at a vertex takes the even-odd
        // fill too, and comes to the ring less the island: a flattened
        // circle of 2,000 chords, swept rather than paired (#414).
        let ring: Vec<Point2> = (0..2000)
            .map(|i| {
                let angle = f64::from(i) / 2000.0 * TAU;
                p(100.0 * angle.cos(), 100.0 * angle.sin())
            })
            .collect();
        let island = vec![ring[0], p(50.0, 10.0), p(50.0, -10.0)];
        let both = vec![ring.clone(), island.clone()];
        assert!(rings_cross(&both), "touching is taken as meeting");
        let expected = signed_area(&ring).abs() - signed_area(&island).abs();
        let filled = area_of(&even_odd_fill(&both));
        assert!(
            (filled - expected).abs() < 1e-6 * expected,
            "{filled} vs {expected}"
        );
        // Nested, apart, or one ring: none cross.
        assert!(!rings_cross(&[
            square(0.0, 0.0, 10.0),
            square(2.0, 2.0, 2.0)
        ]));
        assert!(!rings_cross(&[
            square(0.0, 0.0, 10.0),
            square(20.0, 0.0, 5.0)
        ]));
        assert!(!rings_cross(&[square(0.0, 0.0, 10.0)]));
    }

    proptest! {
        // A fine grid per case: fewer cases than the default.
        #![proptest_config(ProptestConfig::with_cases(32))]

        /// The even-odd fill of rings crossing anyhow covers exactly the
        /// points an odd number of them contain, and agrees with the
        /// nested fill where they do not cross.
        #[test]
        fn the_even_odd_fill_covers_what_an_odd_number_of_rings_contain(
            centres in prop::collection::vec((-30.0f64..30.0, -30.0f64..30.0), 1..4),
            count in 3usize..9,
            radii in prop::collection::vec(5.0f64..40.0, 1..5),
        ) {
            let rings: Vec<Vec<Point2>> = centres
                .iter()
                .map(|&(x, y)| star(p(x, y), count, &radii))
                .collect();
            let triangles = even_odd_fill(&rings);
            // A sliver where edges cross a hair from a vertex has no middle
            // to place at this precision; the area below accounts for it.
            for triangle in triangles.iter().filter(|t| cross(t[0], t[1], t[2]) > 1e-6) {
                let middle = centroid(triangle);
                let around = rings.iter().filter(|ring| contains(ring, middle)).count();
                prop_assert!(around % 2 == 1, "{triangle:?} in {around} rings");
            }
            // The area agrees with the fraction of a grid an odd number of
            // rings hold, to the grid's coarseness.
            let (step, mut odd) = (0.5, 0usize);
            for i in 0..280 {
                for j in 0..280 {
                    let point = p(-70.0 + (f64::from(i) + 0.5) * step, -70.0 + (f64::from(j) + 0.5) * step);
                    if rings.iter().filter(|ring| contains(ring, point)).count() % 2 == 1 {
                        odd += 1;
                    }
                }
            }
            let sampled = odd as f64 * step * step;
            let filled = area_of(&triangles);
            prop_assert!((filled - sampled).abs() <= 0.03 * sampled.max(100.0), "{filled} vs {sampled}");
            if !rings_cross(&rings) && rings.len() == 1 {
                prop_assert!((filled - signed_area(&rings[0]).abs()).abs() < 1e-6 * filled.max(1.0));
            }
        }
    }

    proptest! {
        #[test]
        fn the_triangles_cover_exactly_the_ring_less_its_hole(
            count in 3usize..40,
            radii in prop::collection::vec(40.0f64..100.0, 1..8),
            hole_count in 3usize..12,
            hole_radii in prop::collection::vec(2.0f64..12.0, 1..4),
            with_hole in proptest::bool::ANY,
        ) {
            // The outer ring's inradius is at least 20 (a triangle of radius
            // 40 and up), so a hole of radius 12 about (3, −2) lies inside.
            let outer = star(p(0.0, 0.0), count, &radii);
            let holes: Vec<Vec<Point2>> = if with_hole {
                vec![star(p(3.0, -2.0), hole_count, &hole_radii)]
            } else {
                Vec::new()
            };
            let triangles = tessellate(&outer, &holes);
            let expected = signed_area(&outer).abs()
                - holes.iter().map(|hole| signed_area(hole).abs()).sum::<f64>();
            prop_assert!((area_of(&triangles) - expected).abs() < 1e-6 * expected, "{} vs {expected}", area_of(&triangles));
            let bytes = triangles.len() * 6 * 4;
            prop_assert_eq!(triangles.len(), count + holes.iter().map(|h| h.len() + 2).sum::<usize>() - 2);
            prop_assert!(bytes > 0);
            for triangle in &triangles {
                let middle = centroid(triangle);
                prop_assert!(contains(&outer, middle), "{triangle:?} outside");
                prop_assert!(holes.iter().all(|hole| !contains(hole, middle)), "{triangle:?} in the hole");
                prop_assert!(cross(triangle[0], triangle[1], triangle[2]) > 0.0, "counter-clockwise, with area");
            }
        }
    }
}
