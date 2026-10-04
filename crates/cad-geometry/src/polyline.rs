//! Polylines: chains of straight segments through a list of vertices.

use cad_core::{Aabb, Point2};

use crate::Segment;

/// The segments of a polyline through `vertices`, including the closing
/// segment when `closed`. Fewer than two vertices yield no segments.
pub fn polyline_segments(vertices: &[Point2], closed: bool) -> impl Iterator<Item = Segment> + '_ {
    let closing = match (closed, vertices.first(), vertices.last()) {
        (true, Some(first), Some(last)) if vertices.len() > 2 => Some(Segment::new(*last, *first)),
        _ => None,
    };
    vertices
        .windows(2)
        .map(|pair| Segment::new(pair[0], pair[1]))
        .chain(closing)
}

/// The bounding box of a set of points, or `None` when there are none.
pub fn bounds_of(points: &[Point2]) -> Option<Aabb> {
    points
        .iter()
        .map(|point| Aabb::from_points(*point, *point))
        .reduce(|a, b| a.union(&b))
}

/// Distance from `point` to the nearest segment of the polyline.
pub fn polyline_distance_to_point(vertices: &[Point2], closed: bool, point: Point2) -> f64 {
    polyline_segments(vertices, closed)
        .map(|segment| segment.distance_to_point(point))
        .fold(f64::INFINITY, f64::min)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn square() -> Vec<Point2> {
        vec![p(0.0, 0.0), p(10.0, 0.0), p(10.0, 10.0), p(0.0, 10.0)]
    }

    #[test]
    fn an_open_polyline_has_one_segment_fewer_than_vertices() {
        assert_eq!(polyline_segments(&square(), false).count(), 3);
    }

    #[test]
    fn a_closed_polyline_adds_the_segment_back_to_the_start() {
        let segments: Vec<Segment> = polyline_segments(&square(), true).collect();
        assert_eq!(segments.len(), 4);
        assert_eq!(segments[3], Segment::new(p(0.0, 10.0), p(0.0, 0.0)));
    }

    #[test]
    fn degenerate_polylines_have_no_spurious_segments() {
        assert_eq!(polyline_segments(&[], true).count(), 0);
        assert_eq!(polyline_segments(&[p(1.0, 1.0)], true).count(), 0);
        // Two vertices closed is still one segment, not a doubled one.
        assert_eq!(
            polyline_segments(&[p(0.0, 0.0), p(1.0, 0.0)], true).count(),
            1
        );
    }

    #[test]
    fn bounds_cover_every_vertex() {
        let vertices = [p(3.0, -1.0), p(-2.0, 4.0), p(0.0, 0.0)];
        assert_eq!(
            bounds_of(&vertices),
            Some(Aabb::from_points(p(-2.0, -1.0), p(3.0, 4.0)))
        );
        assert_eq!(bounds_of(&[]), None);
    }

    #[test]
    fn distance_is_to_the_nearest_segment_and_respects_closure() {
        // Just outside the left edge, which exists only when closed.
        let point = p(-2.0, 5.0);
        assert_eq!(polyline_distance_to_point(&square(), true, point), 2.0);
        // Open: the nearest geometry is the vertex (0, 10) or (0, 0), 5 above or below.
        let open = polyline_distance_to_point(&square(), false, point);
        assert!((open - 29f64.sqrt()).abs() < 1e-12, "{open}");
        // The interior is not the polyline.
        assert_eq!(
            polyline_distance_to_point(&square(), true, p(5.0, 5.0)),
            5.0
        );
    }
}
