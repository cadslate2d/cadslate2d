//! Fillets where at least one side is curved (DESIGN §22): line–arc,
//! arc–arc, and the same with full circles.
//!
//! A fillet arc of radius `r` is tangent to both objects, so its centre is
//! `r` away from each: on a parallel of a line, or on a circle concentric
//! with a circle and `r` larger or smaller. Intersecting those gives up to
//! eight centres. The pick points choose among them: the fillet wanted is
//! the one that touches each object nearest to where it was picked.
//!
//! Lines and arcs are then cut back or carried on to the points of
//! tangency, keeping the side that was picked. Circles are left whole.

use std::f64::consts::{PI, TAU};

use cad_core::tolerance::LinearTolerance;
use cad_core::{Point2, Vec2};

use crate::intersect::{Hits, circle_circle, line_circle, line_line};
use crate::{Arc, Circle, CornerError, Primitive, Segment, normalize_angle};

/// Two objects joined by a tangent arc: what each becomes — a circle stays
/// as it is — and the arc, absent when the radius is zero.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CurveFillet {
    pub first: Primitive,
    pub second: Primitive,
    pub arc: Option<Arc>,
}

/// The line or circle an object lies on.
#[derive(Clone, Copy)]
enum Carrier {
    Line(Segment),
    Circle(Circle),
}

impl Carrier {
    fn of(primitive: &Primitive) -> Self {
        match primitive {
            Primitive::Segment(segment) => Self::Line(*segment),
            Primitive::Circle(circle) => Self::Circle(*circle),
            Primitive::Arc(arc) => Self::Circle(arc.circle()),
        }
    }

    /// Where the centre of a circle of `radius` tangent to this can be:
    /// two parallels of a line; two circles concentric with a circle.
    fn tangent_centres(&self, radius: f64, tolerance: LinearTolerance) -> Vec<Carrier> {
        match self {
            Self::Line(line) => {
                let along = line.end - line.start;
                let length = along.length();
                if tolerance.is_zero(length) {
                    return Vec::new();
                }
                let shift = Vec2::new(-along.y, along.x) * (radius / length);
                vec![
                    Self::Line(Segment::new(line.start + shift, line.end + shift)),
                    Self::Line(Segment::new(line.start + (-shift), line.end + (-shift))),
                ]
            }
            Self::Circle(circle) => [circle.radius + radius, (circle.radius - radius).abs()]
                .into_iter()
                .filter(|radius| !tolerance.is_zero(*radius))
                .map(|radius| Self::Circle(Circle::new(circle.center, radius)))
                .collect(),
        }
    }

    fn meet(&self, other: &Carrier) -> Hits {
        match (self, other) {
            (Self::Line(a), Self::Line(b)) => line_line(a, b).map_or(Hits::NONE, Hits::one),
            (Self::Line(line), Self::Circle(circle)) | (Self::Circle(circle), Self::Line(line)) => {
                line_circle(line, circle)
            }
            (Self::Circle(a), Self::Circle(b)) => circle_circle(a, b),
        }
    }

    /// Where a circle about `centre` with `radius`, tangent to this, touches it.
    fn tangent_point(&self, centre: Point2, radius: f64) -> Point2 {
        match self {
            Self::Line(line) => {
                let along = line.end - line.start;
                let t = (centre - line.start).dot(along) / along.length_squared();
                line.start + along * t
            }
            Self::Circle(circle) => {
                let towards = centre - circle.center;
                let unit = towards * (1.0 / towards.length());
                // On the line of centres: on the near side, or — when the
                // fillet circle swallows this one — on the far side.
                let near = circle.center + unit * circle.radius;
                let far = circle.center + unit * -circle.radius;
                let off = |point: Point2| (point.distance_to(centre) - radius).abs();
                if off(near) <= off(far) { near } else { far }
            }
        }
    }
}

/// `object` cut back or carried on so that it ends at `at`, a point on its
/// line or circle, keeping the side that `pick` is on.
fn end_at(object: &Primitive, at: Point2, pick: Point2, tolerance: LinearTolerance) -> Primitive {
    match object {
        Primitive::Circle(_) => *object,
        Primitive::Segment(segment) => {
            // Keep the end that lies the way the pick does, seen from `at`.
            let picked = segment.closest_point(pick);
            let towards = if tolerance.coincident(picked, at) {
                // The pick says nothing: keep the longer side.
                let (to_start, to_end) = (segment.start - at, segment.end - at);
                if to_end.length() >= to_start.length() {
                    to_end
                } else {
                    to_start
                }
            } else {
                picked - at
            };
            let keep_end = (segment.end - at).dot(towards) >= (segment.start - at).dot(towards);
            Primitive::Segment(if keep_end {
                Segment::new(at, segment.end)
            } else {
                Segment::new(segment.start, at)
            })
        }
        Primitive::Arc(arc) => {
            let angle = |point: Point2| {
                let offset = point - arc.center;
                normalize_angle(offset.y.atan2(offset.x) - arc.start)
            };
            let (cut, picked) = (angle(at), angle(arc.closest_point(pick)));
            let cut_point_angle = arc.start + cut;
            let trimmed = if cut < arc.sweep {
                // The point is on the arc: keep the part the pick is on.
                if picked <= cut {
                    Arc::new(arc.center, arc.radius, arc.start, cut)
                } else {
                    Arc::new(arc.center, arc.radius, cut_point_angle, arc.sweep - cut)
                }
            } else if cut - arc.sweep <= TAU - cut {
                // Beyond the end: carry the end on to it.
                Arc::new(arc.center, arc.radius, arc.start, cut)
            } else {
                // Before the start: carry the start back to it.
                Arc::new(
                    arc.center,
                    arc.radius,
                    cut_point_angle,
                    arc.sweep + (TAU - cut),
                )
            };
            Primitive::Arc(trimmed)
        }
    }
}

/// The arc of `radius` tangent to both objects that touches each nearest to
/// where it was picked, with both objects cut back or carried on to it. A
/// radius of zero joins them where their line or circle meets, with no arc.
///
/// `CornerError::NoSolution` when no such arc — or meeting point — exists.
pub fn fillet_curves(
    first: &Primitive,
    first_pick: Point2,
    second: &Primitive,
    second_pick: Point2,
    radius: f64,
    tolerance: LinearTolerance,
) -> Result<CurveFillet, CornerError> {
    let (a, b) = (Carrier::of(first), Carrier::of(second));
    let nearest = |candidates: Vec<(Point2, Point2, Point2)>| {
        candidates
            .into_iter()
            .filter(|(centre, touch_a, touch_b)| {
                centre.is_finite() && touch_a.is_finite() && touch_b.is_finite()
            })
            .min_by(|x, y| {
                let cost = |(_, touch_a, touch_b): &(Point2, Point2, Point2)| {
                    touch_a.distance_to(first_pick) + touch_b.distance_to(second_pick)
                };
                cost(x).total_cmp(&cost(y))
            })
            .ok_or(CornerError::NoSolution)
    };

    if tolerance.is_zero(radius) {
        // A sharp corner: where the two carriers meet, nearest the picks.
        let meetings = a
            .meet(&b)
            .as_slice()
            .iter()
            .map(|meeting| (*meeting, *meeting, *meeting))
            .collect();
        let (meeting, ..) = nearest(meetings)?;
        return Ok(CurveFillet {
            first: end_at(first, meeting, first_pick, tolerance),
            second: end_at(second, meeting, second_pick, tolerance),
            arc: None,
        });
    }

    let mut candidates = Vec::new();
    for centres_a in a.tangent_centres(radius, tolerance) {
        for centres_b in b.tangent_centres(radius, tolerance) {
            for centre in centres_a.meet(&centres_b).as_slice() {
                let touches = (
                    a.tangent_point(*centre, radius),
                    b.tangent_point(*centre, radius),
                );
                candidates.push((*centre, touches.0, touches.1));
            }
        }
    }
    let (centre, touch_a, touch_b) = nearest(candidates)?;
    if tolerance.coincident(touch_a, touch_b) {
        // The two objects already touch there: no room for an arc.
        return Err(CornerError::NoSolution);
    }
    let angle = |point: Point2| {
        let offset = point - centre;
        offset.y.atan2(offset.x)
    };
    // The fillet is the short way round between the two points of tangency.
    let (from, to) = (angle(touch_a), angle(touch_b));
    let arc = if normalize_angle(to - from) <= PI {
        Arc::from_angles(centre, radius, from, to)
    } else {
        Arc::from_angles(centre, radius, to, from)
    };
    Ok(CurveFillet {
        first: end_at(first, touch_a, first_pick, tolerance),
        second: end_at(second, touch_b, second_pick, tolerance),
        arc: Some(arc),
    })
}

#[cfg(test)]
mod tests {
    use std::f64::consts::FRAC_PI_2;

    use proptest::prelude::*;

    use super::*;

    const TOLERANCE: LinearTolerance = LinearTolerance::DEFAULT;

    fn p(x: f64, y: f64) -> Point2 {
        Point2::new(x, y)
    }

    fn close(a: Point2, b: Point2) -> bool {
        a.distance_to(b) < 1e-9 * (1.0 + a.distance_to(Point2::ORIGIN))
    }

    fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Primitive {
        Primitive::Segment(Segment::new(p(x0, y0), p(x1, y1)))
    }

    fn as_segment(primitive: &Primitive) -> Segment {
        match primitive {
            Primitive::Segment(segment) => *segment,
            other => panic!("expected a segment, got {other:?}"),
        }
    }

    fn as_arc(primitive: &Primitive) -> Arc {
        match primitive {
            Primitive::Arc(arc) => *arc,
            other => panic!("expected an arc, got {other:?}"),
        }
    }

    #[test]
    fn a_line_and_a_circle_are_joined_on_the_side_that_was_picked() {
        // The X axis and a circle of radius 30 sitting 10 above it... its
        // lowest point is at (0, 10). Fillet radius 10.
        let floor = line(-200.0, 0.0, 200.0, 0.0);
        let circle = Primitive::Circle(Circle::new(p(0.0, 40.0), 30.0));
        // Picked right of the circle: the fillet nestles in the right-hand gap.
        let fillet = fillet_curves(
            &floor,
            p(60.0, 0.0),
            &circle,
            p(25.0, 25.0),
            10.0,
            TOLERANCE,
        )
        .unwrap();
        let arc = fillet.arc.unwrap();
        // Centre 10 above the floor and 40 from the circle's centre: x = √(1600 − 900).
        assert!(
            close(arc.center, p(700f64.sqrt(), 10.0)),
            "{:?}",
            arc.center
        );
        assert!((arc.radius - 10.0).abs() < 1e-12);
        // The circle is left whole; the line now ends under the fillet's centre.
        assert_eq!(fillet.second, circle);
        let trimmed = as_segment(&fillet.first);
        assert!(close(trimmed.start, p(700f64.sqrt(), 0.0)) && close(trimmed.end, p(200.0, 0.0)));

        // Picked on the left, the mirror image.
        let fillet = fillet_curves(
            &floor,
            p(-60.0, 0.0),
            &circle,
            p(-25.0, 25.0),
            10.0,
            TOLERANCE,
        )
        .unwrap();
        assert!(close(fillet.arc.unwrap().center, p(-(700f64.sqrt()), 10.0)));
        let trimmed = as_segment(&fillet.first);
        assert!(
            close(trimmed.start, p(-200.0, 0.0)) && close(trimmed.end, p(-(700f64.sqrt()), 0.0))
        );
    }

    #[test]
    fn a_line_and_an_arc_are_both_cut_back_to_the_fillet() {
        // A line along the X axis ending at the origin, and a quarter arc of
        // radius 50 about (0, 50) rising from the origin: tangent there...
        // so use an arc about (0, 60), which stops 10 short.
        let floor = line(-100.0, 0.0, 80.0, 0.0);
        let arc = Primitive::Arc(Arc::new(p(0.0, 60.0), 50.0, -FRAC_PI_2, FRAC_PI_2)); // (0,10) → (50,60)
        let fillet =
            fillet_curves(&floor, p(-50.0, 0.0), &arc, p(40.0, 30.0), 8.0, TOLERANCE).unwrap();
        let joint = fillet.arc.unwrap();
        assert!((joint.radius - 8.0).abs() < 1e-12);
        // Tangent to the floor: its centre is 8 above it.
        assert!((joint.center.y - 8.0).abs() < 1e-9);
        // Tangent to the arc's circle from outside: centres 58 apart.
        assert!((joint.center.distance_to(p(0.0, 60.0)) - 58.0).abs() < 1e-9);
        // The line keeps its picked (left) part and ends under the fillet's centre.
        let trimmed = as_segment(&fillet.first);
        assert!(close(trimmed.start, p(-100.0, 0.0)) && close(trimmed.end, p(joint.center.x, 0.0)));
        // The arc keeps its picked (upper) part and starts where the fillet touches it.
        let kept = as_arc(&fillet.second);
        assert!(close(kept.end_point(), p(50.0, 60.0)));
        let touches = [joint.start_point(), joint.end_point()];
        assert!(
            touches
                .iter()
                .any(|touch| close(*touch, kept.start_point()))
        );
    }

    #[test]
    fn two_circles_are_joined_by_an_arc_on_the_side_of_the_picks() {
        let left = Primitive::Circle(Circle::new(p(-50.0, 0.0), 30.0));
        let right = Primitive::Circle(Circle::new(p(50.0, 0.0), 30.0));
        // Picked on top: an arc of radius 40 bridging the gap from above.
        let fillet = fillet_curves(
            &left,
            p(-30.0, 22.0),
            &right,
            p(30.0, 22.0),
            40.0,
            TOLERANCE,
        )
        .unwrap();
        // Externally tangent to both: 70 from each centre, on the axis of
        // symmetry — above it, because that is where the picks are.
        let arc = fillet.arc.unwrap();
        assert!(
            close(arc.center, p(0.0, 2400f64.sqrt())),
            "{:?}",
            arc.center
        );
        assert_eq!(
            (fillet.first, fillet.second),
            (left, right),
            "circles stay whole"
        );
        // Picked underneath, the mirror image.
        let below = fillet_curves(
            &left,
            p(-30.0, -22.0),
            &right,
            p(30.0, -22.0),
            40.0,
            TOLERANCE,
        )
        .unwrap();
        assert!(close(below.arc.unwrap().center, p(0.0, -(2400f64.sqrt()))));
    }

    #[test]
    fn a_zero_radius_brings_a_line_and_an_arc_to_the_point_where_they_meet() {
        // A line that would cross the circle of the arc at (30, 40) and (−30, 40).
        let level = line(-100.0, 40.0, 10.0, 40.0);
        let arc = Primitive::Arc(Arc::new(p(0.0, 0.0), 50.0, 0.0, 0.5)); // a short arc near (50, 0)
        let fillet =
            fillet_curves(&level, p(0.0, 40.0), &arc, p(48.0, 14.0), 0.0, TOLERANCE).unwrap();
        assert_eq!(fillet.arc, None);
        let extended = as_segment(&fillet.first);
        assert!(close(extended.end, p(30.0, 40.0)) && close(extended.start, p(-100.0, 40.0)));
        let grown = as_arc(&fillet.second);
        assert!(
            close(grown.start_point(), p(50.0, 0.0)),
            "its picked start stays"
        );
        assert!(
            close(grown.end_point(), p(30.0, 40.0)),
            "its end is carried round to the meeting"
        );
    }

    #[test]
    fn objects_too_far_apart_for_the_radius_have_no_fillet() {
        let floor = line(-200.0, 0.0, 200.0, 0.0);
        let far = Primitive::Circle(Circle::new(p(0.0, 500.0), 30.0));
        assert_eq!(
            fillet_curves(&floor, p(0.0, 0.0), &far, p(0.0, 470.0), 10.0, TOLERANCE),
            Err(CornerError::NoSolution)
        );
        // Radius zero: they never meet.
        assert_eq!(
            fillet_curves(&floor, p(0.0, 0.0), &far, p(0.0, 470.0), 0.0, TOLERANCE),
            Err(CornerError::NoSolution)
        );
        // Concentric circles are never tangent to a common small arc... unless it fits between them.
        let inner = Primitive::Circle(Circle::new(p(0.0, 0.0), 10.0));
        let outer = Primitive::Circle(Circle::new(p(0.0, 0.0), 100.0));
        assert_eq!(
            fillet_curves(&inner, p(10.0, 0.0), &outer, p(100.0, 0.0), 5.0, TOLERANCE),
            Err(CornerError::NoSolution)
        );
    }

    fn coordinate() -> impl Strategy<Value = f64> {
        -200.0..200f64
    }

    proptest! {
        /// #124: the fillet arc is tangent to both objects — the radius to
        /// each point of tangency is perpendicular to a line, and in line
        /// with the centre of a circle — and both objects end on it.
        #[test]
        fn a_fillet_arc_is_tangent_to_a_line_and_a_circle(
            cx in coordinate(), cy in coordinate(), circle_radius in 10.0..120f64,
            ax in coordinate(), ay in coordinate(), bx in coordinate(), by in coordinate(),
            px in coordinate(), py in coordinate(), radius in 1.0..60f64,
        ) {
            let segment = Segment::new(p(ax, ay), p(bx, by));
            prop_assume!(segment.length() > 5.0);
            let circle = Circle::new(p(cx, cy), circle_radius);
            let (first, second) = (Primitive::Segment(segment), Primitive::Circle(circle));
            let pick = p(px, py);
            let Ok(fillet) = fillet_curves(&first, pick, &second, pick, radius, TOLERANCE) else {
                return Ok(()); // no arc of that radius fits
            };
            let arc = fillet.arc.unwrap();
            prop_assert!((arc.radius - radius).abs() < 1e-9);
            prop_assert!(arc.sweep <= PI + 1e-9);

            // Tangent to the line: the centre is `radius` from it.
            let along = segment.end - segment.start;
            let offset = arc.center - segment.start;
            let from_line = (along.x * offset.y - along.y * offset.x).abs() / along.length();
            prop_assert!((from_line - radius).abs() < 1e-6, "{} from the line", from_line);
            // Tangent to the circle: centres apart by the sum or the difference of the radii.
            let apart = arc.center.distance_to(circle.center);
            let external = (apart - (circle_radius + radius)).abs();
            let internal = (apart - (circle_radius - radius).abs()).abs();
            prop_assert!(external.min(internal) < 1e-6, "centres {} apart", apart);

            // The line now ends at one end of the fillet arc; the other is on the circle.
            let trimmed = as_segment(&fillet.first);
            let ends = [arc.start_point(), arc.end_point()];
            let meets = |point: Point2| ends.iter().any(|end| end.distance_to(point) < 1e-6);
            prop_assert!(meets(trimmed.start) || meets(trimmed.end));
            prop_assert!(ends.iter().any(|end| (end.distance_to(circle.center) - circle_radius).abs() < 1e-6));
        }

        /// The same for two arcs: tangent to both circles, and both arcs end on it.
        #[test]
        fn a_fillet_arc_is_tangent_to_two_arcs(
            ax in coordinate(), ay in coordinate(), ar in 10.0..120f64, a_start in 0.0..TAU, a_sweep in 0.2..5.5f64,
            bx in coordinate(), by in coordinate(), br in 10.0..120f64, b_start in 0.0..TAU, b_sweep in 0.2..5.5f64,
            radius in 1.0..60f64,
        ) {
            let (a, b) = (Arc::new(p(ax, ay), ar, a_start, a_sweep), Arc::new(p(bx, by), br, b_start, b_sweep));
            prop_assume!(a.center.distance_to(b.center) > 1.0);
            let (first, second) = (Primitive::Arc(a), Primitive::Arc(b));
            let Ok(fillet) = fillet_curves(&first, a.mid_point(), &second, b.mid_point(), radius, TOLERANCE) else {
                return Ok(());
            };
            let arc = fillet.arc.unwrap();
            for circle in [a.circle(), b.circle()] {
                let apart = arc.center.distance_to(circle.center);
                let external = (apart - (circle.radius + radius)).abs();
                let internal = (apart - (circle.radius - radius).abs()).abs();
                prop_assert!(external.min(internal) < 1e-6, "centres {} apart", apart);
            }
            let ends = [arc.start_point(), arc.end_point()];
            for kept in [as_arc(&fillet.first), as_arc(&fillet.second)] {
                let meets = |point: Point2| ends.iter().any(|end| end.distance_to(point) < 1e-6);
                prop_assert!(meets(kept.start_point()) || meets(kept.end_point()), "{:?} does not end on the fillet", kept);
                prop_assert!(kept.sweep > 0.0 && kept.sweep <= TAU);
            }
        }
    }
}
