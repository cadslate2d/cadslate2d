//! Lengthening a segment or an arc from one of its ends until it meets
//! other geometry (DESIGN §22).

use std::f64::consts::TAU;

use cad_core::Point2;
use cad_core::tolerance::LinearTolerance;

use crate::intersect::{line_circle, line_line};
use crate::{Arc, Primitive, Segment, normalize_angle};

/// Where the infinite line through `segment` meets `primitive` itself.
fn line_meets(segment: &Segment, primitive: &Primitive, tolerance: LinearTolerance) -> Vec<Point2> {
    let on_the_line: Vec<Point2> = match primitive {
        Primitive::Segment(other) => line_line(segment, other).into_iter().collect(),
        Primitive::Circle(circle) => line_circle(segment, circle).as_slice().to_vec(),
        Primitive::Arc(arc) => line_circle(segment, &arc.circle()).as_slice().to_vec(),
    };
    on_the_line
        .into_iter()
        .filter(|point| tolerance.is_zero(primitive.distance_to_point(*point)))
        .collect()
}

/// `segment` lengthened in a straight line from its end (or, with
/// `from_end` false, its start) to the nearest point where it meets
/// `boundary`. `None` when it would never meet it.
pub fn extend_segment(
    segment: &Segment,
    from_end: bool,
    boundary: &[Primitive],
    tolerance: LinearTolerance,
) -> Option<Segment> {
    let (tip, tail) = if from_end {
        (segment.end, segment.start)
    } else {
        (segment.start, segment.end)
    };
    let onwards = tip - tail;
    let meeting = boundary
        .iter()
        .flat_map(|primitive| line_meets(segment, primitive, tolerance))
        // Beyond the tip, not behind it; and already touching is not an extension.
        .filter(|point| (*point - tip).dot(onwards) > 0.0 && !tolerance.coincident(*point, tip))
        .min_by(|a, b| tip.distance_to(*a).total_cmp(&tip.distance_to(*b)))?;
    Some(if from_end {
        Segment::new(segment.start, meeting)
    } else {
        Segment::new(meeting, segment.end)
    })
}

/// `arc` lengthened along its circle from its end (counter-clockwise) or,
/// with `from_end` false, from its start (clockwise) to the nearest point
/// where it meets `boundary`. `None` when it would come back round to its
/// other end without meeting it.
pub fn extend_arc(
    arc: &Arc,
    from_end: bool,
    boundary: &[Primitive],
    tolerance: LinearTolerance,
) -> Option<Arc> {
    let circle = Primitive::Circle(arc.circle());
    let tip = if from_end {
        arc.end_point()
    } else {
        arc.start_point()
    };
    let gap = TAU - arc.sweep;
    // How far round from the tip, in the direction of growth, a point is.
    let turn = |point: Point2| {
        let offset = point - arc.center;
        let angle = offset.y.atan2(offset.x);
        if from_end {
            normalize_angle(angle - arc.end())
        } else {
            normalize_angle(arc.start - angle)
        }
    };
    let grow = boundary
        .iter()
        .flat_map(|primitive| circle.intersect(primitive).as_slice().to_vec())
        .filter(|point| !tolerance.coincident(*point, tip))
        .map(turn)
        .filter(|turn| *turn > 0.0 && *turn < gap)
        .min_by(f64::total_cmp)?;
    Some(if from_end {
        Arc::new(arc.center, arc.radius, arc.start, arc.sweep + grow)
    } else {
        Arc::new(arc.center, arc.radius, arc.start - grow, arc.sweep + grow)
    })
}

#[cfg(test)]
mod tests {
    use std::f64::consts::{FRAC_PI_2, PI};

    use crate::Circle;

    use super::*;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn wall(x: f64) -> Primitive {
        Primitive::Segment(Segment::new(p(x, -50.0), p(x, 50.0)))
    }

    fn segment() -> Segment {
        Segment::new(p(0.0, 0.0), p(10.0, 0.0))
    }

    #[test]
    fn a_segment_grows_from_the_chosen_end_to_the_boundary() {
        assert_eq!(
            extend_segment(&segment(), true, &[wall(30.0)], TOLERANCE),
            Some(Segment::new(p(0.0, 0.0), p(30.0, 0.0)))
        );
        assert_eq!(
            extend_segment(&segment(), false, &[wall(-20.0)], TOLERANCE),
            Some(Segment::new(p(-20.0, 0.0), p(10.0, 0.0)))
        );
    }

    #[test]
    fn a_boundary_behind_the_chosen_end_or_off_to_the_side_is_never_met() {
        assert_eq!(
            extend_segment(&segment(), true, &[wall(-20.0)], TOLERANCE),
            None
        );
        let short = Primitive::Segment(Segment::new(p(30.0, 5.0), p(30.0, 50.0)));
        assert_eq!(
            extend_segment(&segment(), true, &[short], TOLERANCE),
            None,
            "passes below it"
        );
        let parallel = Primitive::Segment(Segment::new(p(0.0, 5.0), p(100.0, 5.0)));
        assert_eq!(
            extend_segment(&segment(), true, &[parallel], TOLERANCE),
            None
        );
    }

    #[test]
    fn the_nearest_meeting_wins() {
        let boundary = [wall(80.0), wall(30.0), wall(55.0)];
        assert_eq!(
            extend_segment(&segment(), true, &boundary, TOLERANCE),
            Some(Segment::new(p(0.0, 0.0), p(30.0, 0.0)))
        );
    }

    #[test]
    fn a_segment_extends_to_the_near_side_of_a_circle_and_to_an_arc() {
        let circle = Primitive::Circle(Circle::new(p(50.0, 0.0), 10.0));
        assert_eq!(
            extend_segment(&segment(), true, &[circle], TOLERANCE),
            Some(Segment::new(p(0.0, 0.0), p(40.0, 0.0)))
        );
        // Only the far half of that circle exists: the segment reaches it.
        let far_half = Primitive::Arc(Arc::new(p(50.0, 0.0), 10.0, -FRAC_PI_2, PI));
        let extended = extend_segment(&segment(), true, &[far_half], TOLERANCE).unwrap();
        assert!(extended.end.distance_to(p(60.0, 0.0)) < 1e-9);
    }

    #[test]
    fn an_end_already_on_the_boundary_extends_to_the_next_meeting_or_not_at_all() {
        let touching = Segment::new(p(0.0, 0.0), p(30.0, 0.0));
        assert_eq!(
            extend_segment(&touching, true, &[wall(30.0)], TOLERANCE),
            None
        );
        assert_eq!(
            extend_segment(&touching, true, &[wall(30.0), wall(45.0)], TOLERANCE),
            Some(Segment::new(p(0.0, 0.0), p(45.0, 0.0)))
        );
    }

    /// A quarter arc of radius 10 about the origin, from (10,0) up to (0,10).
    fn quarter() -> Arc {
        Arc::new(p(0.0, 0.0), 10.0, 0.0, FRAC_PI_2)
    }

    #[test]
    fn an_arc_grows_along_its_circle_from_either_end() {
        // The negative X axis: reached counter-clockwise from the end at 90°.
        let left = Primitive::Segment(Segment::new(p(-50.0, 0.0), p(-1.0, 0.0)));
        let grown = extend_arc(&quarter(), true, &[left], TOLERANCE).unwrap();
        assert_eq!(grown.start, 0.0);
        assert!((grown.sweep - PI).abs() < 1e-12);

        // The negative Y axis: reached clockwise from the start at 0°.
        let down = Primitive::Segment(Segment::new(p(0.0, -50.0), p(0.0, -1.0)));
        let grown = extend_arc(&quarter(), false, &[down], TOLERANCE).unwrap();
        assert!((normalize_angle(grown.start) - 3.0 * FRAC_PI_2).abs() < 1e-12);
        assert!((grown.sweep - PI).abs() < 1e-12);
        assert!(
            grown.end_point().distance_to(p(0.0, 10.0)) < 1e-12,
            "the other end stays"
        );
    }

    #[test]
    fn an_arc_takes_the_first_meeting_in_its_direction_of_growth() {
        // A line through the centre crosses the circle at 135° and at 315°.
        let diagonal = Primitive::Segment(Segment::new(p(-50.0, 50.0), p(50.0, -50.0)));
        let from_end = extend_arc(&quarter(), true, &[diagonal], TOLERANCE).unwrap();
        assert!((from_end.sweep - 3.0 * PI / 4.0).abs() < 1e-12, "to 135°");
        let from_start = extend_arc(&quarter(), false, &[diagonal], TOLERANCE).unwrap();
        assert!(
            (from_start.sweep - 3.0 * PI / 4.0).abs() < 1e-12,
            "back to 315°"
        );
    }

    #[test]
    fn an_arc_that_never_meets_the_boundary_is_not_extended() {
        let far = Primitive::Segment(Segment::new(p(100.0, 0.0), p(100.0, 50.0)));
        assert_eq!(extend_arc(&quarter(), true, &[far], TOLERANCE), None);
        // A boundary that only crosses the arc itself is no extension either.
        let through = Primitive::Segment(Segment::new(p(0.0, 0.0), p(20.0, 20.0)));
        assert_eq!(extend_arc(&quarter(), true, &[through], TOLERANCE), None);
    }
}
