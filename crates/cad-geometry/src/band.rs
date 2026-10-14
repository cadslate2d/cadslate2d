//! A wide polyline's band (#204; ADR-0021): the filled region half the
//! width to either side of a path, as triangles — a quad per straight
//! segment, a strip of quads along an arc, and a disc at every vertex
//! and end, so joins and caps are round at any angle.

use std::f64::consts::TAU;

use cad_core::{Point2, Vec2};

use crate::polygon::arc_chords;
use crate::{Path, Primitive};

/// Chords per full turn along an arc's edges and round a vertex.
const DISC_SIDES: usize = 24;

impl Path {
    /// The band of this path at `width`: triangles that together cover
    /// every point within half the width of the path, and nothing
    /// further than that (give or take the chords an arc is drawn as).
    /// Empty for a width that is not positive.
    pub fn band(&self, width: f64) -> Vec<[Point2; 3]> {
        if !(width.is_finite() && width > 0.0) {
            return Vec::new();
        }
        let half = width / 2.0;
        let mut triangles = Vec::new();
        for primitive in self.primitives() {
            match primitive {
                Primitive::Segment(segment) => {
                    let along = segment.end - segment.start;
                    let length = along.length();
                    if length > 0.0 {
                        let normal = Vec2::new(-along.y, along.x) * (half / length);
                        let (a, b, c, d) = (
                            segment.start + normal,
                            segment.end + normal,
                            segment.end - normal,
                            segment.start - normal,
                        );
                        triangles.push([a, b, c]);
                        triangles.push([a, c, d]);
                    }
                }
                Primitive::Arc(arc) => {
                    let chords = arc_chords(&arc, TAU / DISC_SIDES as f64, false);
                    for pair in chords.windows(2) {
                        let radial = |point: Point2, reach: f64| {
                            let out = point - arc.center;
                            let length = out.length();
                            if length > 0.0 {
                                arc.center + out * (reach / length)
                            } else {
                                point
                            }
                        };
                        let (a, b) = (
                            radial(pair[0], arc.radius + half),
                            radial(pair[1], arc.radius + half),
                        );
                        let (c, d) = (
                            radial(pair[1], (arc.radius - half).max(0.0)),
                            radial(pair[0], (arc.radius - half).max(0.0)),
                        );
                        triangles.push([a, b, c]);
                        triangles.push([a, c, d]);
                    }
                }
                Primitive::Circle(circle) => {
                    let ring = Path {
                        vertices: vec![
                            circle.center - Vec2::new(circle.radius, 0.0),
                            circle.center + Vec2::new(circle.radius, 0.0),
                        ],
                        bulges: vec![1.0, 1.0],
                        closed: true,
                    };
                    triangles.extend(ring.band(width));
                }
            }
        }
        // A disc at every vertex: the round joins, and the round caps of
        // an open path.
        for vertex in &self.vertices {
            let rim: Vec<Point2> = (0..DISC_SIDES)
                .map(|i| *vertex + Vec2::from_polar(half, TAU * i as f64 / DISC_SIDES as f64))
                .collect();
            for i in 0..DISC_SIDES {
                triangles.push([*vertex, rim[i], rim[(i + 1) % DISC_SIDES]]);
            }
        }
        triangles
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn a_straight_band_is_a_quad_with_round_ends() {
        let path = Path {
            vertices: vec![p(0.0, 0.0), p(10.0, 0.0)],
            bulges: vec![],
            closed: false,
        };
        let band = path.band(2.0);
        assert_eq!(band.len(), 2 + 2 * DISC_SIDES);
        assert_eq!(band[0], [p(0.0, 1.0), p(10.0, 1.0), p(10.0, -1.0)]);
        assert!(path.band(0.0).is_empty());
        assert!(path.band(-1.0).is_empty());
    }

    proptest! {
        /// Every corner of every triangle of the band lies within half the
        /// width of the path — give or take an arc's chords — and every
        /// vertex of the path has band around it.
        #[test]
        fn the_band_stays_within_half_the_width_of_the_path(
            corners in prop::collection::vec((-100.0..100.0_f64, -100.0..100.0_f64, -0.9..0.9_f64), 2..6),
            closed in any::<bool>(),
            width in 0.1..20.0_f64,
        ) {
            let vertices: Vec<Point2> = corners.iter().map(|(x, y, _)| p(*x, *y)).collect();
            prop_assume!(vertices.windows(2).all(|w| w[0].distance_to(w[1]) > 1.0));
            let bulges: Vec<f64> = corners.iter().map(|(_, _, b)| *b).collect();
            let path = Path { vertices: vertices.clone(), bulges, closed };
            let primitives = path.primitives();
            let band = path.band(width);
            prop_assert!(!band.is_empty());
            // A chord of a 24-gon stands off its arc by r(1 − cos(π/24)).
            let slack = width / 2.0 * (1.0 - (std::f64::consts::PI / DISC_SIDES as f64).cos()) + 1e-9;
            for triangle in &band {
                for corner in triangle {
                    let distance = primitives
                        .iter()
                        .map(|primitive| primitive.distance_to_point(*corner))
                        .fold(f64::INFINITY, f64::min);
                    prop_assert!(distance <= width / 2.0 + slack, "{corner:?} is {distance} off, width {width}");
                }
            }
        }
    }
}
