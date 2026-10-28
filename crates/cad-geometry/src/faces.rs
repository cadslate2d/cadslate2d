//! The faces the drawn edges make (#358): every segment split where it
//! meets another, ends that lead nowhere pruned, and each bounded face
//! traced around — so a point picked between a block's outline and the
//! lines that divide it is in that parcel, not the whole block. What
//! HATCH and BOUNDARY find from a point, as AutoCAD's pick does.

use std::collections::HashMap;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::intersect::segment_segment;
use crate::{Segment, ring_contains, signed_area};

/// More edges than this are not arranged: a region is looked for among
/// closed outlines alone, as before.
pub const MAX_EDGES: usize = 20_000;

/// The bounded faces some edges make, each a ring turning anticlockwise:
/// worked out once, then looked up for each point picked or hovered.
#[derive(Clone, Debug, Default)]
pub struct Faces(Vec<(f64, Vec<Point2>)>);

impl Faces {
    /// The faces of `edges`; none when there are more than [`MAX_EDGES`].
    pub fn of(edges: &[Segment]) -> Self {
        let edges: Vec<Segment> = edges
            .iter()
            .copied()
            .filter(|e| {
                !LinearTolerance::DEFAULT
                    .around(&[e.start, e.end])
                    .coincident(e.start, e.end)
            })
            .collect();
        if edges.is_empty() || edges.len() > MAX_EDGES {
            return Self::default();
        }
        let all: Vec<Point2> = edges.iter().flat_map(|e| [e.start, e.end]).collect();
        let tolerance = LinearTolerance::DEFAULT.around(&all);
        let mut graph = Graph::new(tolerance.value().max(1e-9) * 16.0);
        for (a, b) in split(&edges, tolerance) {
            graph.edge(a, b);
        }
        graph.prune();
        Self(
            graph
                .faces()
                .into_iter()
                .map(|ring| (signed_area(&ring), ring))
                .filter(|(area, _)| *area > 0.0)
                .collect(),
        )
    }

    /// The smallest face around `point`, and its area.
    pub fn around(&self, point: Point2) -> Option<(f64, &[Point2])> {
        self.0
            .iter()
            .filter(|(_, ring)| ring_contains(ring, point))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(area, ring)| (*area, ring.as_slice()))
    }
}

/// The smallest bounded face around `point` that `edges` make, as a ring
/// turning anticlockwise; `None` when no face holds it, or there are more
/// than [`MAX_EDGES`].
pub fn face_around(edges: &[Segment], point: Point2) -> Option<Vec<Point2>> {
    Faces::of(edges)
        .around(point)
        .map(|(_, ring)| ring.to_vec())
}

/// Each edge cut where another meets it — crossing, touching, or running
/// along it — into the pieces between.
fn split(edges: &[Segment], tolerance: LinearTolerance) -> Vec<(Point2, Point2)> {
    let boxes: Vec<(f64, f64, f64, f64)> = edges
        .iter()
        .map(|e| {
            (
                e.start.x.min(e.end.x),
                e.start.y.min(e.end.y),
                e.start.x.max(e.end.x),
                e.start.y.max(e.end.y),
            )
        })
        .collect();
    let pad = tolerance.value();
    let mut cuts: Vec<Vec<f64>> = vec![Vec::new(); edges.len()];
    // Swept along x: each edge met only by those whose span of x reaches
    // its own, not every other.
    let mut by_left: Vec<usize> = (0..edges.len()).collect();
    by_left.sort_by(|&a, &b| boxes[a].0.total_cmp(&boxes[b].0));
    for (k, &i) in by_left.iter().enumerate() {
        for &j in &by_left[k + 1..] {
            let (a, b) = (boxes[i], boxes[j]);
            if b.0 > a.2 + pad {
                break;
            }
            if a.1 > b.3 + pad || b.1 > a.3 + pad {
                continue;
            }
            if let Some(at) = segment_segment(&edges[i], &edges[j]) {
                cuts[i].push(along(&edges[i], at));
                cuts[j].push(along(&edges[j], at));
            }
            // An end of one on the other: a T, or edges that run along each
            // other, which do not cross.
            for (k, l) in [(i, j), (j, i)] {
                for end in [edges[l].start, edges[l].end] {
                    if edges[k].distance_to_point(end) <= tolerance.value() {
                        cuts[k].push(along(&edges[k], end));
                    }
                }
            }
        }
    }
    let mut pieces = Vec::new();
    for (edge, mut at) in edges.iter().zip(cuts) {
        at.push(0.0);
        at.push(1.0);
        at.retain(|t| (0.0..=1.0).contains(t));
        at.sort_by(f64::total_cmp);
        let point = |t: f64| edge.start + (edge.end - edge.start) * t;
        for pair in at.windows(2) {
            let (a, b) = (point(pair[0]), point(pair[1]));
            if !tolerance.coincident(a, b) {
                pieces.push((a, b));
            }
        }
    }
    pieces
}

/// Where `point` is along `edge`, 0 at its start and 1 at its end.
fn along(edge: &Segment, point: Point2) -> f64 {
    let direction = edge.end - edge.start;
    ((point - edge.start).dot(direction) / direction.dot(direction)).clamp(0.0, 1.0)
}

/// The pieces as a graph: points merged within the snap, each edge once.
struct Graph {
    snap: f64,
    points: Vec<Point2>,
    cells: HashMap<(i64, i64), Vec<usize>>,
    neighbours: Vec<Vec<usize>>,
}

impl Graph {
    fn new(snap: f64) -> Self {
        Self {
            snap,
            points: Vec::new(),
            cells: HashMap::new(),
            neighbours: Vec::new(),
        }
    }

    fn cell(&self, p: Point2) -> (i64, i64) {
        (
            (p.x / self.snap).floor() as i64,
            (p.y / self.snap).floor() as i64,
        )
    }

    /// The vertex at `p`: one already within the snap, or a new one.
    fn vertex(&mut self, p: Point2) -> usize {
        let (cx, cy) = self.cell(p);
        for dx in -1..=1 {
            for dy in -1..=1 {
                if let Some(found) = self.cells.get(&(cx + dx, cy + dy))
                    && let Some(&v) = found
                        .iter()
                        .find(|&&v| self.points[v].distance_to(p) <= self.snap)
                {
                    return v;
                }
            }
        }
        let v = self.points.len();
        self.points.push(p);
        self.neighbours.push(Vec::new());
        self.cells.entry((cx, cy)).or_default().push(v);
        v
    }

    fn edge(&mut self, a: Point2, b: Point2) {
        let (u, v) = (self.vertex(a), self.vertex(b));
        if u != v && !self.neighbours[u].contains(&v) {
            self.neighbours[u].push(v);
            self.neighbours[v].push(u);
        }
    }

    /// Ends that lead nowhere taken away, until none is left: they bound
    /// nothing.
    fn prune(&mut self) {
        let mut ends: Vec<usize> = (0..self.points.len())
            .filter(|&v| self.neighbours[v].len() == 1)
            .collect();
        while let Some(v) = ends.pop() {
            if self.neighbours[v].len() != 1 {
                continue;
            }
            let u = self.neighbours[v][0];
            self.neighbours[v].clear();
            self.neighbours[u].retain(|&w| w != v);
            if self.neighbours[u].len() == 1 {
                ends.push(u);
            }
        }
    }

    /// Every face, each traced keeping it on the left: at each vertex the
    /// next edge is the one just clockwise of the way back. A bounded face
    /// turns anticlockwise; the face outside each piece, clockwise.
    fn faces(&self) -> Vec<Vec<Point2>> {
        let angle = |from: usize, to: usize| {
            let d = self.points[to] - self.points[from];
            d.y.atan2(d.x)
        };
        let order: Vec<Vec<usize>> = (0..self.points.len())
            .map(|v| {
                let mut around = self.neighbours[v].clone();
                around.sort_by(|&a, &b| angle(v, a).total_cmp(&angle(v, b)));
                around
            })
            .collect();
        let mut seen: HashMap<(usize, usize), ()> = HashMap::new();
        let mut faces = Vec::new();
        for u in 0..self.points.len() {
            for &v in &order[u] {
                if seen.contains_key(&(u, v)) {
                    continue;
                }
                let mut ring = Vec::new();
                let (mut a, mut b) = (u, v);
                let limit = 2 * self.points.len() + 4;
                while !seen.contains_key(&(a, b)) && ring.len() <= limit {
                    seen.insert((a, b), ());
                    ring.push(self.points[a]);
                    let around = &order[b];
                    let back = around
                        .iter()
                        .position(|&w| w == a)
                        .expect("an edge both ways");
                    let next = around[(back + around.len() - 1) % around.len()];
                    (a, b) = (b, next);
                }
                if ring.len() >= 3 {
                    faces.push(ring);
                }
            }
        }
        faces
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn s(a: (f64, f64), b: (f64, f64)) -> Segment {
        Segment {
            start: p(a.0, a.1),
            end: p(b.0, b.1),
        }
    }

    fn rectangle(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<Segment> {
        vec![
            s((x0, y0), (x1, y0)),
            s((x1, y0), (x1, y1)),
            s((x1, y1), (x0, y1)),
            s((x0, y1), (x0, y0)),
        ]
    }

    /// A block divided by lines from its top to its foot: a point in a
    /// parcel is in that parcel alone, as an urban sheet's were (#358).
    #[test]
    fn a_block_divided_by_lines_gives_each_parcel_its_own_face() {
        let mut edges = rectangle(0.0, 0.0, 190.0, 140.0);
        for x in [38.0, 76.0, 114.0, 152.0] {
            edges.push(s((x, 0.0), (x, 140.0)));
        }
        for (i, x) in [19.0, 57.0, 95.0, 133.0, 171.0].into_iter().enumerate() {
            let face = face_around(&edges, p(x, 70.0)).unwrap();
            assert!(
                (signed_area(&face) - 38.0 * 140.0).abs() < 1e-6,
                "parcel {i}: {}",
                signed_area(&face)
            );
        }
        // Outside every face: none.
        assert_eq!(face_around(&edges, p(200.0, 70.0)), None);
    }

    /// Lines that cross, overshoot, and dangle: the face is what they
    /// enclose, the overshoots and the loose end left out.
    #[test]
    fn crossings_overshoots_and_loose_ends_bound_the_face_they_close() {
        let edges = vec![
            s((-10.0, 0.0), (110.0, 0.0)),
            s((100.0, -10.0), (100.0, 60.0)),
            s((110.0, 50.0), (-10.0, 50.0)),
            s((0.0, 60.0), (0.0, -10.0)),
            // A loose end inside.
            s((50.0, 0.0), (50.0, 20.0)),
        ];
        let face = face_around(&edges, p(25.0, 25.0)).unwrap();
        assert!((signed_area(&face) - 100.0 * 50.0).abs() < 1e-6, "{face:?}");
    }

    /// A grid of 60 lines each way, crossing 3,600 times, is arranged at
    /// once, and each cell is a face.
    #[test]
    fn a_large_grid_is_arranged_and_each_cell_is_a_face() {
        let mut edges = Vec::new();
        for i in 0..60 {
            let at = f64::from(i) * 10.0;
            edges.push(s((at, 0.0), (at, 590.0)));
            edges.push(s((0.0, at), (590.0, at)));
        }
        let started = std::time::Instant::now();
        let faces = Faces::of(&edges);
        assert!(started.elapsed().as_secs_f64() < 5.0);
        let (area, _) = faces.around(p(255.0, 255.0)).unwrap();
        assert!((area - 100.0).abs() < 1e-6);
    }

    /// A line along part of another's side, and a T: both split it.
    #[test]
    fn edges_touching_or_running_along_each_other_are_joined() {
        let mut edges = rectangle(0.0, 0.0, 100.0, 50.0);
        // A T from the top down to the foot, touching both sides.
        edges.push(s((40.0, 50.0), (40.0, 0.0)));
        // Along the foot, overlapping it.
        edges.push(s((20.0, 0.0), (60.0, 0.0)));
        let left = face_around(&edges, p(10.0, 10.0)).unwrap();
        assert!((signed_area(&left) - 40.0 * 50.0).abs() < 1e-6);
        let right = face_around(&edges, p(70.0, 10.0)).unwrap();
        assert!((signed_area(&right) - 60.0 * 50.0).abs() < 1e-6);
    }
}
