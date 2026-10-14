//! LENGTHEN (#201): a segment or an arc made longer or shorter at one end
//! by a length, along its own line or circle; and the regular polygons
//! POLYGON draws.

use std::f64::consts::{PI, TAU};

use cad_core::{Point2, Vec2};

use crate::{Arc, Segment};

/// The segment with its end (or its start) moved `delta` along the
/// segment's own direction: longer for a positive delta, shorter for a
/// negative one. `None` when the segment has no direction, or the result
/// would have no length or run back past its other end.
pub fn lengthen_segment(segment: &Segment, from_end: bool, delta: f64) -> Option<Segment> {
    let along = segment.end - segment.start;
    let length = along.length();
    if !(length > 0.0 && delta.is_finite()) || length + delta <= 0.0 {
        return None;
    }
    let step = along * (delta / length);
    Some(if from_end {
        Segment::new(segment.start, segment.end + step)
    } else {
        Segment::new(segment.start - step, segment.end)
    })
}

/// The arc with its end (counter-clockwise) or its start carried `delta`
/// further round its circle — back, for a negative delta. `None` when the
/// result would have no sweep, or would reach or pass its full circle.
pub fn lengthen_arc(arc: &Arc, from_end: bool, delta: f64) -> Option<Arc> {
    if !(arc.radius > 0.0 && delta.is_finite()) {
        return None;
    }
    let turn = delta / arc.radius;
    let sweep = arc.sweep + turn;
    if sweep <= 0.0 || sweep >= TAU {
        return None;
    }
    Some(if from_end {
        Arc::new(arc.center, arc.radius, arc.start, sweep)
    } else {
        Arc::new(arc.center, arc.radius, arc.start - turn, sweep)
    })
}

/// The corners of a regular polygon of `sides` about `center`, the first
/// at `start_angle` (radians): on a circle of `radius` when inscribed,
/// or with its sides tangent to that circle when circumscribed. Fewer
/// than three sides is no polygon.
pub fn regular_polygon(
    center: Point2,
    sides: usize,
    radius: f64,
    inscribed: bool,
    start_angle: f64,
) -> Option<Vec<Point2>> {
    if sides < 3 || !(radius > 0.0 && radius.is_finite()) {
        return None;
    }
    let step = TAU / sides as f64;
    let reach = if inscribed {
        radius
    } else {
        radius / (step / 2.0).cos()
    };
    Some(
        (0..sides)
            .map(|i| center + Vec2::from_polar(reach, start_angle + step * i as f64))
            .collect(),
    )
}

/// The corners of a regular polygon of `sides` whose first side runs from
/// `a` to `b`, the rest turning counter-clockwise from it.
pub fn polygon_from_edge(a: Point2, b: Point2, sides: usize) -> Option<Vec<Point2>> {
    let edge = b - a;
    let length = edge.length();
    if sides < 3 || !(length > 0.0 && length.is_finite()) {
        return None;
    }
    let step = TAU / sides as f64;
    // The centre sits above the edge's middle, at the apothem.
    let apothem = length / (2.0 * (PI / sides as f64).tan());
    let middle = a + edge * 0.5;
    let left = Vec2::new(-edge.y, edge.x) * (1.0 / length);
    let center = middle + left * apothem;
    let start_angle = (a - center).y.atan2((a - center).x);
    let reach = (a - center).length();
    Some(
        (0..sides)
            .map(|i| center + Vec2::from_polar(reach, start_angle + step * i as f64))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use proptest::prelude::*;

    use super::*;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    #[test]
    fn a_segment_grows_or_shrinks_at_the_chosen_end_and_never_vanishes() {
        let segment = Segment::new(p(0.0, 0.0), p(30.0, 40.0));
        let longer = lengthen_segment(&segment, true, 10.0).unwrap();
        assert_eq!(longer.start, segment.start);
        assert!(longer.end.distance_to(p(36.0, 48.0)) < 1e-9);
        let shorter = lengthen_segment(&segment, false, -25.0).unwrap();
        assert_eq!(shorter.end, segment.end);
        assert!(shorter.start.distance_to(p(15.0, 20.0)) < 1e-9);
        assert_eq!(lengthen_segment(&segment, true, -50.0), None);
        assert_eq!(lengthen_segment(&segment, true, -60.0), None);
        assert_eq!(
            lengthen_segment(&Segment::new(p(1.0, 1.0), p(1.0, 1.0)), true, 5.0),
            None
        );
    }

    #[test]
    fn an_arc_grows_round_its_circle_and_cannot_pass_its_full_circle() {
        let arc = Arc::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2);
        let longer = lengthen_arc(&arc, true, 10.0 * FRAC_PI_2).unwrap();
        assert!((longer.sweep - PI).abs() < 1e-12);
        assert_eq!(longer.start, 0.0);
        let back = lengthen_arc(&arc, false, 10.0 * FRAC_PI_2).unwrap();
        assert!((back.sweep - PI).abs() < 1e-12);
        assert!((back.start - (TAU - FRAC_PI_2)).abs() < 1e-12);
        assert_eq!(
            lengthen_arc(&arc, true, 10.0 * (TAU - FRAC_PI_2)),
            None,
            "a full circle"
        );
        assert_eq!(lengthen_arc(&arc, true, 10.0 * TAU), None, "past it");
        assert_eq!(
            lengthen_arc(&arc, true, -10.0 * FRAC_PI_2),
            None,
            "nothing left"
        );
    }

    #[test]
    fn a_polygon_is_regular_and_from_an_edge_turns_counter_clockwise() {
        let square = regular_polygon(p(0.0, 0.0), 4, 10.0, true, 0.0).unwrap();
        assert_eq!(square.len(), 4);
        assert!(square[0].distance_to(p(10.0, 0.0)) < 1e-9);
        assert!(square[1].distance_to(p(0.0, 10.0)) < 1e-9);
        let about = regular_polygon(p(0.0, 0.0), 4, 10.0, false, 0.0).unwrap();
        assert!((about[0].distance_to(p(0.0, 0.0)) - 10.0 * 2.0_f64.sqrt()).abs() < 1e-9);
        assert_eq!(regular_polygon(p(0.0, 0.0), 2, 10.0, true, 0.0), None);
        let from_edge = polygon_from_edge(p(0.0, 0.0), p(10.0, 0.0), 4).unwrap();
        assert!(from_edge[2].distance_to(p(10.0, 10.0)) < 1e-9);
        assert!(from_edge[3].distance_to(p(0.0, 10.0)) < 1e-9);
        let triangle = polygon_from_edge(p(0.0, 0.0), p(10.0, 0.0), 3).unwrap();
        assert!(triangle[2].distance_to(p(5.0, 75.0_f64.sqrt())) < 1e-9);
    }

    proptest! {
        /// Lengthening by a delta and back by the same is the identity, and
        /// the length changes by exactly the delta meanwhile.
        #[test]
        fn lengthening_there_and_back_is_the_identity(
            x in -100.0..100.0_f64, y in -100.0..100.0_f64, dx in -50.0..50.0_f64, dy in -50.0..50.0_f64,
            delta in -40.0..40.0_f64, from_end in any::<bool>(),
            radius in 1.0..50.0_f64, start in 0.0..TAU, sweep in 0.1..6.0_f64,
        ) {
            let segment = Segment::new(p(x, y), p(x + dx, y + dy));
            if let Some(longer) = lengthen_segment(&segment, from_end, delta) {
                prop_assert!((longer.length() - (segment.length() + delta)).abs() < 1e-9);
                let back = lengthen_segment(&longer, from_end, -delta).unwrap();
                prop_assert!(back.start.distance_to(segment.start) < 1e-9 && back.end.distance_to(segment.end) < 1e-9);
            } else {
                prop_assert!(segment.length() + delta <= 0.0 || segment.length() == 0.0);
            }
            let arc = Arc::new(p(x, y), radius, start, sweep);
            if let Some(longer) = lengthen_arc(&arc, from_end, delta) {
                prop_assert!((longer.radius * longer.sweep - (arc.radius * arc.sweep + delta)).abs() < 1e-9);
                let back = lengthen_arc(&longer, from_end, -delta).unwrap();
                prop_assert!(back.start_point().distance_to(arc.start_point()) < 1e-7);
                prop_assert!(back.end_point().distance_to(arc.end_point()) < 1e-7);
            } else {
                let sweep = arc.sweep + delta / radius;
                prop_assert!(sweep <= 0.0 || sweep >= TAU);
            }
        }

        /// Every corner of a regular polygon is on its circle — the
        /// inscribed one's at the radius, the circumscribed one's at the
        /// radius over the cosine of half a turn between corners — and every
        /// side is as long as every other.
        #[test]
        fn a_regular_polygons_corners_are_on_its_circle_with_equal_sides(
            cx in -100.0..100.0_f64, cy in -100.0..100.0_f64, sides in 3..13usize, radius in 0.5..100.0_f64,
            inscribed in any::<bool>(), start in 0.0..TAU,
        ) {
            let center = p(cx, cy);
            let corners = regular_polygon(center, sides, radius, inscribed, start).unwrap();
            let reach = if inscribed { radius } else { radius / (PI / sides as f64).cos() };
            for corner in &corners {
                prop_assert!((corner.distance_to(center) - reach).abs() < 1e-9);
            }
            let side = corners[0].distance_to(corners[1]);
            for i in 0..sides {
                let next = corners[(i + 1) % sides];
                prop_assert!((corners[i].distance_to(next) - side).abs() < 1e-9);
                if !inscribed {
                    // The side's middle is on the circle.
                    let middle = corners[i] + (next - corners[i]) * 0.5;
                    prop_assert!((middle.distance_to(center) - radius).abs() < 1e-9);
                }
            }
        }
    }
}
