//! Wipeouts (#226): a polygon that masks what is beneath it in the
//! background colour.
//!
//! A wipeout is its boundary, a ring of straight edges, closed by the last
//! vertex back to the first. It is filled in whatever colour the canvas
//! is — the theme's on screen, the paper's on a plot — so what is under it
//! in the drawing order (ADR-0032) disappears there, and what is over it,
//! a note, stands on a clean ground. Its frame is drawn in its own
//! properties when the drawing shows wipeout frames (`WIPEOUT Frames`,
//! DXF `WIPEOUTVARIABLES`). It is picked and snapped by its edges, as a
//! closed polyline is.

use std::fmt;

use cad_core::tolerance::LinearTolerance;
use cad_core::{Aabb, Point2};
use cad_geometry::{Primitive, Segment, Transform, ring_contains, signed_area, tessellate};

#[derive(Clone, Debug, PartialEq)]
pub struct WipeoutEntity {
    /// The boundary's corners, in order; at least three, and the last is
    /// not the first again.
    pub vertices: Vec<Point2>,
}

/// Why a boundary cannot be a wipeout's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WipeoutError {
    /// Fewer than three corners, or none of them apart.
    TooFewVertices,
    /// The corners enclose nothing: they lie on one line.
    NoArea,
    /// A coordinate is not finite.
    Position,
}

impl fmt::Display for WipeoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooFewVertices => write!(f, "a wipeout needs at least three corners"),
            Self::NoArea => write!(f, "a wipeout must enclose an area"),
            Self::Position => write!(f, "a wipeout's corners must be finite"),
        }
    }
}

impl std::error::Error for WipeoutError {}

impl WipeoutEntity {
    /// A wipeout over the polygon with these corners. A corner repeating
    /// the one before it is dropped, as is a last corner that closes the
    /// ring onto the first.
    pub fn new(vertices: Vec<Point2>) -> Result<Self, WipeoutError> {
        if vertices
            .iter()
            .any(|point| !point.x.is_finite() || !point.y.is_finite())
        {
            return Err(WipeoutError::Position);
        }
        let tolerance = LinearTolerance::DEFAULT.around(&vertices);
        let mut ring: Vec<Point2> = Vec::with_capacity(vertices.len());
        for point in vertices {
            if ring
                .last()
                .is_none_or(|last| !tolerance.coincident(*last, point))
            {
                ring.push(point);
            }
        }
        while ring.len() > 1
            && tolerance.coincident(ring[0], *ring.last().expect("more than one corner"))
        {
            ring.pop();
        }
        if ring.len() < 3 {
            return Err(WipeoutError::TooFewVertices);
        }
        let size = extent_of(&ring).max() - extent_of(&ring).min();
        // Collinear corners enclose no more than rounding does.
        if signed_area(&ring).abs() <= tolerance.value() * size.x.max(size.y) {
            return Err(WipeoutError::NoArea);
        }
        Ok(Self { vertices: ring })
    }

    /// The boundary's edges, the closing one last.
    pub fn edges(&self) -> Vec<Segment> {
        let n = self.vertices.len();
        (0..n)
            .map(|i| Segment::new(self.vertices[i], self.vertices[(i + 1) % n]))
            .collect()
    }

    pub fn primitives(&self) -> Vec<Primitive> {
        self.edges().into_iter().map(Primitive::Segment).collect()
    }

    pub fn bounds(&self) -> Aabb {
        extent_of(&self.vertices)
    }

    /// The distance to its nearest edge: a wipeout is picked by its frame.
    pub fn distance_to(&self, point: Point2) -> f64 {
        self.edges()
            .iter()
            .map(|edge| edge.distance_to_point(point))
            .fold(f64::INFINITY, f64::min)
    }

    /// Whether `point` is in the masked area.
    pub fn contains(&self, point: Point2) -> bool {
        ring_contains(&self.vertices, point)
    }

    /// The masked area as triangles (ADR-0021).
    pub fn fill(&self) -> Vec<[Point2; 3]> {
        tessellate(&self.vertices, &[])
    }

    #[must_use]
    pub fn transformed(&self, transform: &Transform) -> Self {
        Self {
            vertices: self
                .vertices
                .iter()
                .map(|point| transform.apply(*point))
                .collect(),
        }
    }
}

/// The box around some corners.
fn extent_of(points: &[Point2]) -> Aabb {
    points
        .iter()
        .map(|point| Aabb::from_points(*point, *point))
        .reduce(|a, b| a.union(&b))
        .unwrap_or_else(|| Aabb::from_points(Point2::ORIGIN, Point2::ORIGIN))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> WipeoutEntity {
        WipeoutEntity::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(10.0, 10.0),
            Point2::new(0.0, 10.0),
        ])
        .unwrap()
    }

    #[test]
    fn a_wipeout_is_a_closed_ring_of_three_corners_or_more() {
        let closed_twice = WipeoutEntity::new(vec![
            Point2::new(0.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(10.0, 0.0),
            Point2::new(0.0, 10.0),
            Point2::new(0.0, 0.0),
        ])
        .unwrap();
        assert_eq!(closed_twice.vertices.len(), 3);
        assert_eq!(
            WipeoutEntity::new(vec![Point2::new(0.0, 0.0), Point2::new(1.0, 0.0)]),
            Err(WipeoutError::TooFewVertices)
        );
        assert_eq!(
            WipeoutEntity::new(vec![
                Point2::new(0.0, 0.0),
                Point2::new(1.0, 0.0),
                Point2::new(2.0, 0.0)
            ]),
            Err(WipeoutError::NoArea)
        );
        assert_eq!(
            WipeoutEntity::new(vec![
                Point2::new(0.0, 0.0),
                Point2::new(f64::NAN, 0.0),
                Point2::new(2.0, 1.0)
            ]),
            Err(WipeoutError::Position)
        );
    }

    #[test]
    fn a_wipeout_is_picked_by_its_frame_and_fills_its_area() {
        let wipeout = square();
        assert_eq!(wipeout.edges().len(), 4);
        assert!(wipeout.distance_to(Point2::new(5.0, 0.0)).abs() < 1e-12);
        assert!((wipeout.distance_to(Point2::new(5.0, 4.0)) - 4.0).abs() < 1e-12);
        assert!(wipeout.contains(Point2::new(5.0, 4.0)));
        assert!(!wipeout.contains(Point2::new(15.0, 4.0)));
        let area: f64 = wipeout.fill().iter().map(|t| signed_area(t).abs()).sum();
        assert!((area - 100.0).abs() < 1e-9);
    }
}
