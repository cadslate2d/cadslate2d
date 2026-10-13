//! The shapes a selection is made with (#185): a window or crossing box, a
//! window or crossing polygon, and a fence. Each decides, from an entity's
//! edges, whether the entity is taken; the entity itself is not known here.

use cad_core::{Aabb, Point2};

use crate::{Primitive, Segment, polygon};

/// How a selection shape takes an entity.
#[derive(Clone, Debug, PartialEq)]
pub enum SelectionShape {
    /// Everything entirely inside the box.
    Window(Aabb),
    /// Everything the box touches.
    Crossing(Aabb),
    /// Everything entirely inside the polygon, which need not be convex.
    WindowPolygon(Vec<Point2>),
    /// Everything the polygon touches or holds.
    CrossingPolygon(Vec<Point2>),
    /// Everything the open polyline crosses.
    Fence(Vec<Point2>),
}

impl SelectionShape {
    /// The box the shape lies in: what to ask the spatial index for.
    pub fn bounds(&self) -> Aabb {
        match self {
            Self::Window(bounds) | Self::Crossing(bounds) => *bounds,
            Self::WindowPolygon(points) | Self::CrossingPolygon(points) | Self::Fence(points) => {
                bounds_of(points)
            }
        }
    }

    /// The polygon's or fence's edges; a box has none to speak of, since
    /// it is tested as a box.
    fn edges(&self) -> Vec<Segment> {
        match self {
            Self::Window(_) | Self::Crossing(_) => Vec::new(),
            Self::WindowPolygon(ring) | Self::CrossingPolygon(ring) => ring_edges(ring),
            Self::Fence(path) => path
                .windows(2)
                .map(|pair| Segment::new(pair[0], pair[1]))
                .collect(),
        }
    }

    /// Whether the shape takes an entity, given the entity's `edges` — the
    /// curves it is drawn or framed by — and `encloses`, which says whether a
    /// point lies in the entity's own area (a text's box, a hatch's fill), so
    /// that a shape lying wholly inside such an entity still touches it.
    ///
    /// A window takes an entity when nothing of it crosses the shape's edge
    /// and some point of it is inside; a crossing shape when something of it
    /// crosses the edge, or it is inside, or the shape is inside it; a fence
    /// when something of it crosses the fence.
    pub fn takes(&self, edges: &[Primitive], encloses: impl Fn(Point2) -> bool) -> bool {
        let Some(first) = edges.first() else {
            return false;
        };
        match self {
            Self::Window(bounds) => edges.iter().all(|edge| bounds.contains(&edge.bounds())),
            Self::Crossing(bounds) => {
                edges.iter().any(|edge| edge.intersects_aabb(bounds)) || encloses(bounds.min())
            }
            Self::WindowPolygon(ring) => {
                ring.len() >= 3
                    && !self.crossed_by(edges)
                    && polygon::contains(ring, first.a_point())
            }
            Self::CrossingPolygon(ring) => {
                ring.len() >= 3
                    && (self.crossed_by(edges)
                        || polygon::contains(ring, first.a_point())
                        || encloses(ring[0]))
            }
            Self::Fence(_) => self.crossed_by(edges),
        }
    }

    fn crossed_by(&self, edges: &[Primitive]) -> bool {
        let own = self.edges();
        own.iter().any(|segment| {
            let segment = Primitive::Segment(*segment);
            edges.iter().any(|edge| !segment.intersect(edge).is_empty())
        })
    }
}

fn bounds_of(points: &[Point2]) -> Aabb {
    let first = points.first().copied().unwrap_or(Point2::ORIGIN);
    points
        .iter()
        .fold(Aabb::from_points(first, first), |bounds, point| {
            bounds.union(&Aabb::from_points(*point, *point))
        })
}

fn ring_edges(ring: &[Point2]) -> Vec<Segment> {
    if ring.len() < 2 {
        return Vec::new();
    }
    (0..ring.len())
        .map(|index| Segment::new(ring[index], ring[(index + 1) % ring.len()]))
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;
    use crate::{Arc, Circle};

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn segment(x0: f64, y0: f64, x1: f64, y1: f64) -> Primitive {
        Primitive::Segment(Segment::new(p(x0, y0), p(x1, y1)))
    }

    fn never(_: Point2) -> bool {
        false
    }

    /// An L shape: the square 0..100 without its top-right quarter.
    fn l_shape() -> Vec<Point2> {
        vec![
            p(0.0, 0.0),
            p(100.0, 0.0),
            p(100.0, 50.0),
            p(50.0, 50.0),
            p(50.0, 100.0),
            p(0.0, 100.0),
        ]
    }

    #[test]
    fn a_window_polygon_takes_only_what_lies_wholly_inside_it() {
        let shape = SelectionShape::WindowPolygon(l_shape());
        assert!(shape.takes(&[segment(10.0, 10.0, 40.0, 40.0)], never));
        // Inside the shape's box, but across the notch: not inside.
        assert!(!shape.takes(&[segment(60.0, 60.0, 90.0, 90.0)], never));
        assert!(!shape.takes(&[segment(10.0, 10.0, 90.0, 90.0)], never));
        assert!(!shape.takes(&[segment(-10.0, 10.0, 10.0, 10.0)], never));
        assert!(!shape.takes(&[segment(200.0, 200.0, 300.0, 300.0)], never));
    }

    #[test]
    fn a_crossing_polygon_takes_what_it_touches_or_holds() {
        let shape = SelectionShape::CrossingPolygon(l_shape());
        assert!(shape.takes(&[segment(10.0, 10.0, 40.0, 40.0)], never));
        assert!(shape.takes(&[segment(10.0, 10.0, 90.0, 90.0)], never));
        assert!(!shape.takes(&[segment(60.0, 60.0, 90.0, 90.0)], never));
        assert!(!shape.takes(&[segment(200.0, 200.0, 300.0, 300.0)], never));
        // A hatch the whole polygon lies in is touched by it.
        let circle = Primitive::Circle(Circle::new(p(50.0, 50.0), 500.0));
        assert!(shape.takes(&[circle], |point| (point - p(50.0, 50.0)).length() < 500.0));
        assert!(!shape.takes(&[circle], never));
    }

    #[test]
    fn a_fence_takes_what_it_crosses_and_nothing_it_only_passes() {
        let shape = SelectionShape::Fence(vec![p(0.0, 50.0), p(100.0, 50.0), p(100.0, 150.0)]);
        assert!(shape.takes(&[segment(50.0, 0.0, 50.0, 100.0)], never));
        assert!(shape.takes(&[segment(0.0, 100.0, 200.0, 100.0)], never));
        assert!(!shape.takes(&[segment(0.0, 60.0, 90.0, 60.0)], never));
        // A fence takes nothing by lying inside it: it has no inside.
        let circle = Primitive::Circle(Circle::new(p(50.0, 50.0), 500.0));
        assert!(!shape.takes(&[circle], |_| true));
        assert!(shape.takes(
            &[Primitive::Arc(Arc::new(
                p(50.0, 0.0),
                60.0,
                0.0,
                std::f64::consts::PI
            ))],
            never
        ));
    }

    #[test]
    fn a_box_is_tested_as_a_box() {
        let bounds = Aabb::from_points(p(0.0, 0.0), p(100.0, 100.0));
        let inside = [segment(10.0, 10.0, 90.0, 90.0)];
        let across = [segment(-10.0, 50.0, 50.0, 50.0)];
        assert!(SelectionShape::Window(bounds).takes(&inside, never));
        assert!(!SelectionShape::Window(bounds).takes(&across, never));
        assert!(SelectionShape::Crossing(bounds).takes(&inside, never));
        assert!(SelectionShape::Crossing(bounds).takes(&across, never));
        assert!(!SelectionShape::Crossing(bounds).takes(&[segment(200.0, 0.0, 300.0, 0.0)], never));
        assert!(
            SelectionShape::Crossing(bounds).takes(&[segment(200.0, 0.0, 300.0, 0.0)], |_| true)
        );
    }

    #[test]
    fn nothing_is_taken_without_edges_or_by_a_degenerate_polygon() {
        let ring = vec![p(0.0, 0.0), p(100.0, 0.0)];
        assert!(
            !SelectionShape::WindowPolygon(ring.clone())
                .takes(&[segment(1.0, 1.0, 2.0, 2.0)], never)
        );
        assert!(
            !SelectionShape::CrossingPolygon(ring).takes(&[segment(1.0, 1.0, 2.0, 2.0)], never)
        );
        assert!(!SelectionShape::Fence(vec![p(0.0, 0.0), p(100.0, 0.0)]).takes(&[], |_| true));
        assert!(
            !SelectionShape::Fence(vec![p(50.0, 0.0)])
                .takes(&[segment(50.0, -1.0, 50.0, 1.0)], never)
        );
    }

    #[test]
    fn the_bounds_hold_every_point() {
        let shape = SelectionShape::Fence(vec![p(10.0, 5.0), p(-3.0, 40.0), p(7.0, 7.0)]);
        assert_eq!(
            shape.bounds(),
            Aabb::from_points(p(-3.0, 5.0), p(10.0, 40.0))
        );
    }

    proptest! {
        /// A segment both of whose ends lie strictly in a convex polygon is
        /// inside it, and a fence from a point outside to one inside crosses
        /// what the polygon holds no edge of — the shapes agree with the
        /// ray test they are built on.
        #[test]
        fn a_segment_inside_a_convex_ring_is_taken_by_a_window_and_a_crossing_polygon(
            (x0, y0, x1, y1) in (1.0..99.0, 1.0..99.0, 1.0..99.0, 1.0..99.0),
        ) {
            prop_assume!((p(x1, y1) - p(x0, y0)).length() > 1.0);
            let ring = vec![p(0.0, 0.0), p(100.0, 0.0), p(100.0, 100.0), p(0.0, 100.0)];
            let edges = [segment(x0, y0, x1, y1)];
            prop_assert!(SelectionShape::WindowPolygon(ring.clone()).takes(&edges, never));
            prop_assert!(SelectionShape::CrossingPolygon(ring).takes(&edges, never));
            // A fence across the segment's middle, at right angles, crosses it.
            let middle = p((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            let along = p(x1, y1) - p(x0, y0);
            let across = cad_core::Vec2::new(-along.y, along.x) * (10.0 / along.length());
            let fence = SelectionShape::Fence(vec![middle - across, middle + across]);
            prop_assert!(fence.takes(&edges, never));
        }
    }
}
